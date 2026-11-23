//! The `neoforge.mods.toml` of a native mod ([`NativeMod::mods_toml`](crate::NativeMod::mods_toml)).
//!
//! The keys and defaults follow `FancyModLoader` 12.0.8 `ModFileInfo`, `ModInfo` and
//! `ModInfo.ModVersion`: the `[[mods]]` entry gives the id, namespace, version, display name,
//! description and authors, and `[[dependencies.<modId>]]` gives the dependencies. `license` must
//! be set and not blank, as `ModFileInfo` requires ("Missing license"); its value is not kept.
//!
//! The file keys `properties` and `issueTrackerURL`, the `[[mods]]` keys `issueTrackerURL` and
//! `modUrl`, the `[modproperties.<modId>]` table and the dependency key `referralUrl` are kept on
//! the [`ModInfo`]. The file `issueTrackerURL`, `modUrl` and `referralUrl` are URLs: a value that
//! is blank or holds `myurl.me` or `example.invalid` is absent, as in FML `StringUtils.toURL`, and
//! a value without a scheme stops the load. The `[[mods]]` `issueTrackerURL` is a plain string,
//! as FML does not parse it.
//!
//! The host does not read the other keys: `modLoader`, `loaderVersion`, `showAsResourcePack`,
//! `showAsDataPack`, `services`, `[[mixins]]`, `[[accessTransformers]]`; in a `[[mods]]` entry
//! `logoFile`, `logoBlur`, `updateJSONURL`, `credits`, `displayURL`, `features`. design.md lists
//! why for each one.

use toml::{Table, Value};

use crate::version::{ArtifactVersion, VersionRange};

/// The description FML gives a `[[mods]]` entry without one.
const MISSING_DESCRIPTION: &str = "MISSING DESCRIPTION";
/// FML replaces this with the `Implementation-Version` of the jar; a native mod has no jar, so
/// it is replaced with [`NativeMod::version`](crate::NativeMod::version).
const JAR_VERSION: &str = "${file.jarVersion}";

/// A loaded mod, as `IModInfo` and `ModContainer` describe it in `NeoForge`.
#[derive(Debug, Clone)]
pub struct ModInfo {
    /// `getModId`.
    pub mod_id: String,
    /// `getNamespace`: the registry namespace, the mod id unless the `namespace` key sets one.
    pub namespace: String,
    /// `getVersion`.
    pub version: ArtifactVersion,
    /// `getDisplayName`.
    pub display_name: String,
    /// `getDescription`.
    pub description: String,
    /// The `authors` key, read by `NeoForge` `DefaultModDisplayInfo`.
    pub authors: Option<String>,
    /// `getDependencies`.
    pub dependencies: Vec<ModDependency>,
    /// `getModURL`: the `modUrl` key.
    pub mod_url: Option<String>,
    /// The `issueTrackerURL` key of the `[[mods]]` entry. [`ModInfo::issues_url`] falls back to
    /// the file key.
    pub issue_tracker_url: Option<String>,
    /// The `issueTrackerURL` key of the file (`IModFileInfo.getIssueURL`).
    pub file_issue_tracker_url: Option<String>,
    /// `getModProperties`: the `[modproperties.<modId>]` table of the file.
    pub mod_properties: Table,
    /// The `[properties]` table of the file (`IModFileInfo.getFileProperties`).
    pub file_properties: Table,
}

impl ModInfo {
    /// A mod without a descriptor: the namespace is the id and the description is the display
    /// name.
    pub(crate) fn plain(mod_id: &str, display_name: &str, version: &str) -> Self {
        Self {
            mod_id: mod_id.to_owned(),
            namespace: mod_id.to_owned(),
            version: ArtifactVersion::new(version),
            display_name: display_name.to_owned(),
            description: display_name.to_owned(),
            authors: None,
            dependencies: Vec::new(),
            mod_url: None,
            issue_tracker_url: None,
            file_issue_tracker_url: None,
            mod_properties: Table::new(),
            file_properties: Table::new(),
        }
    }

    /// `DefaultModDisplayInfo.issuesUrl`: the `issueTrackerURL` of the mod, else the one of the
    /// file.
    #[must_use]
    pub fn issues_url(&self) -> Option<&str> {
        self.issue_tracker_url
            .as_deref()
            .or(self.file_issue_tracker_url.as_deref())
    }
}

/// One `[[dependencies.<modId>]]` entry (`IModInfo.ModVersion`).
#[derive(Debug, Clone)]
pub struct ModDependency {
    pub mod_id: String,
    pub dependency_type: DependencyType,
    pub version_range: VersionRange,
    pub ordering: Ordering,
    pub side: DependencySide,
    /// Shown when the dependency is incompatible or discouraged.
    pub reason: Option<String>,
    /// `getReferralURL`: the `referralUrl` key.
    pub referral_url: Option<String>,
}

/// `IModInfo.DependencyType`. The `type` key is case-insensitive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyType {
    /// The server does not start without the dependency in its version range.
    Required,
    /// The dependency may be absent; when it is present, it must be in its version range.
    Optional,
    /// The server does not start when the dependency is present in its version range.
    Incompatible,
    /// The server logs a warning when the dependency is present in its version range.
    Discouraged,
}

/// `IModInfo.Ordering`: whether the mod loads before or after the dependency. The `ordering` key
/// is case-sensitive, as in FML.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ordering {
    Before,
    After,
    None,
}

/// `IModInfo.DependencySide`. A dedicated server ignores the `CLIENT` dependencies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencySide {
    Client,
    Server,
    Both,
}

/// Whether `id` matches the mod id pattern of FML 12.0.8 `ModInfo.VALID_MODID`.
///
/// The pattern is `^(?=.{2,64}$)[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$`: 2 to 64 characters,
/// dot-separated sections that start with a lowercase letter and hold lowercase letters, digits
/// and `_`.
#[must_use]
pub fn is_valid_mod_id(id: &str) -> bool {
    (2..=64).contains(&id.len())
        && id.split('.').all(|section| {
            let mut bytes = section.bytes();
            bytes.next().is_some_and(|b| b.is_ascii_lowercase())
                && bytes.all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_'))
        })
}

/// Whether `namespace` matches FML 12.0.8 `ModInfo.VALID_NAMESPACE`, `^[a-z][a-z0-9_.-]{1,63}$`.
#[must_use]
pub fn is_valid_namespace(namespace: &str) -> bool {
    let mut bytes = namespace.bytes();
    (2..=64).contains(&namespace.len())
        && bytes.next().is_some_and(|b| b.is_ascii_lowercase())
        && bytes.all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'.' | b'-'))
}

/// Reads the `[[mods]]` entry of a native mod from its `neoforge.mods.toml`: the entry whose
/// `modId` is `id`, or the only entry. `${file.jarVersion}` in the version becomes
/// `jar_version`.
///
/// # Errors
///
/// Returns the reason when the text is not TOML, a key has the wrong type, the license or the
/// entry is missing, or the mod id, namespace, version, dependency type, ordering, side or
/// version range is invalid.
pub fn parse(text: &str, id: &str, jar_version: &str) -> Result<ModInfo, String> {
    let file: Table = toml::from_str(text).map_err(|err| err.message().to_owned())?;
    if string(&file, "license")?.is_none_or(|license| license.trim().is_empty()) {
        return Err("Missing license".to_owned());
    }
    let entries = tables(&file, "mods")?;
    let entry = match entries
        .iter()
        .find(|entry| string(entry, "modId").ok().flatten() == Some(id))
    {
        Some(entry) => *entry,
        None if entries.len() == 1 => entries[0],
        None => {
            return Err(format!(
                "{} [[mods]] entries and none has the modId \"{id}\"",
                entries.len()
            ));
        }
    };
    let mod_id = string(entry, "modId")?.ok_or("missing modId")?.to_owned();
    if !is_valid_mod_id(&mod_id) {
        return Err(format!(
            "invalid modId \"{mod_id}\": use 2 to 64 characters a-z 0-9 _, sections separated by \
             . and starting with a-z"
        ));
    }
    let namespace = string(entry, "namespace")?.unwrap_or(&mod_id).to_owned();
    if !is_valid_namespace(&namespace) {
        return Err(format!(
            "invalid namespace \"{namespace}\": use 2 to 64 characters a-z 0-9 _ . -, starting \
             with a-z"
        ));
    }
    let version = string(entry, "version")?
        .unwrap_or("1")
        .replace(JAR_VERSION, jar_version);
    if !version.starts_with(|c: char| c.is_ascii_digit()) {
        return Err(format!(
            "invalid version \"{version}\" of \"{mod_id}\": it must start with a digit"
        ));
    }
    let dependencies = match file
        .get("dependencies")
        .map(|dependencies| {
            dependencies
                .as_table()
                .ok_or("dependencies must be a table of [[dependencies.<modId>]] arrays")
        })
        .transpose()?
    {
        Some(dependencies) if dependencies.contains_key(&mod_id) => tables(dependencies, &mod_id)?
            .into_iter()
            .map(dependency)
            .collect::<Result<_, _>>()?,
        _ => Vec::new(),
    };
    Ok(ModInfo {
        mod_url: url(entry, "modUrl")?,
        issue_tracker_url: string(entry, "issueTrackerURL")?.map(str::to_owned),
        file_issue_tracker_url: url(&file, "issueTrackerURL")?,
        mod_properties: sub_table(&file, "modproperties", &mod_id)?,
        file_properties: match file.get("properties") {
            None => Table::new(),
            Some(value) => value
                .as_table()
                .ok_or("properties must be a table")?
                .clone(),
        },
        display_name: string(entry, "displayName")?.unwrap_or(&mod_id).to_owned(),
        description: string(entry, "description")?.map_or_else(
            || MISSING_DESCRIPTION.to_owned(),
            |d| d.replace("\r\n", "\n"),
        ),
        authors: string(entry, "authors")?.map(str::to_owned),
        version: ArtifactVersion::new(&version),
        mod_id,
        namespace,
        dependencies,
    })
}

fn dependency(entry: &Table) -> Result<ModDependency, String> {
    let mod_id = string(entry, "modId")?
        .ok_or("missing modId in a dependency")?
        .to_owned();
    let dependency_type = match string(entry, "type")?
        .map(str::to_ascii_uppercase)
        .as_deref()
    {
        None | Some("REQUIRED") => DependencyType::Required,
        Some("OPTIONAL") => DependencyType::Optional,
        Some("INCOMPATIBLE") => DependencyType::Incompatible,
        Some("DISCOURAGED") => DependencyType::Discouraged,
        Some(other) => return Err(format!("unknown dependency type \"{other}\" of {mod_id}")),
    };
    let ordering = match string(entry, "ordering")? {
        None | Some("NONE") => Ordering::None,
        Some("BEFORE") => Ordering::Before,
        Some("AFTER") => Ordering::After,
        Some(other) => {
            return Err(format!(
                "unknown ordering \"{other}\" of {mod_id}: use BEFORE, AFTER or NONE"
            ));
        }
    };
    let side = match string(entry, "side")? {
        None | Some("BOTH") => DependencySide::Both,
        Some("CLIENT") => DependencySide::Client,
        Some("SERVER") => DependencySide::Server,
        Some(other) => {
            return Err(format!(
                "unknown side \"{other}\" of {mod_id}: use CLIENT, SERVER or BOTH"
            ));
        }
    };
    let version_range = string(entry, "versionRange")?
        .map(VersionRange::parse)
        .transpose()
        .map_err(|err| format!("{mod_id}: {err}"))?
        .unwrap_or_else(VersionRange::unbounded);
    Ok(ModDependency {
        reason: string(entry, "reason")?.map(str::to_owned),
        referral_url: url(entry, "referralUrl")?,
        mod_id,
        dependency_type,
        version_range,
        ordering,
        side,
    })
}

fn string<'a>(table: &'a Table, key: &str) -> Result<Option<&'a str>, String> {
    table
        .get(key)
        .map(|value| value.as_str().ok_or(format!("{key} must be a string")))
        .transpose()
}

/// FML `StringUtils.toURL`: a blank or placeholder value is absent, and a value without a scheme
/// is an error.
fn url(table: &Table, key: &str) -> Result<Option<String>, String> {
    let Some(value) = string(table, key)? else {
        return Ok(None);
    };
    if value.trim().is_empty() || value.contains("myurl.me") || value.contains("example.invalid") {
        return Ok(None);
    }
    let scheme_end = value.find(':');
    let valid = scheme_end.is_some_and(|end| {
        value[..end].starts_with(|c: char| c.is_ascii_alphabetic())
            && value[..end]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    });
    if valid {
        Ok(Some(value.to_owned()))
    } else {
        Err(format!("{key} \"{value}\" is not an URL: it has no scheme"))
    }
}

/// The `[<key>.<name>]` table of the file, empty when absent.
fn sub_table(file: &Table, key: &str, name: &str) -> Result<Table, String> {
    let Some(parent) = file.get(key) else {
        return Ok(Table::new());
    };
    let parent = parent
        .as_table()
        .ok_or_else(|| format!("{key} must be a table of [{key}.<modId>] tables"))?;
    parent.get(name).map_or_else(
        || Ok(Table::new()),
        |value| {
            value
                .as_table()
                .cloned()
                .ok_or_else(|| format!("{key}.{name} must be a table"))
        },
    )
}

fn tables<'a>(table: &'a Table, key: &str) -> Result<Vec<&'a Table>, String> {
    let error = || format!("{key} must be an array of tables ([[{key}]])");
    let array = table
        .get(key)
        .ok_or_else(|| format!("missing [[{key}]]"))?
        .as_array()
        .ok_or_else(error)?;
    if array.is_empty() {
        return Err(format!("missing [[{key}]]"));
    }
    array
        .iter()
        .map(|value| Value::as_table(value).ok_or_else(error))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{DependencySide, DependencyType, Ordering, is_valid_mod_id, parse};

    /// The descriptor of the example client mod, which a `NeoForge` client loads.
    const CLIENT_MOD: &str = include_str!(
        "../../../examples/modpack/client-mod/src/main/resources/META-INF/neoforge.mods.toml"
    );

    #[test]
    fn parses_the_client_mod_descriptor() {
        let info = parse(CLIENT_MOD, "hello_mod", "0.0.0").unwrap();
        assert_eq!(info.mod_id, "hello_mod");
        assert_eq!(info.namespace, "hello_mod");
        assert_eq!(info.version.to_string(), "1.0.0");
        assert_eq!(info.display_name, "Hello Mod (IronPumpkin client)");
        assert!(
            info.description
                .starts_with("Client side of the IronPumpkin")
        );
        assert_eq!(info.authors, None);
        let dependencies: Vec<_> = info
            .dependencies
            .iter()
            .map(|dep| {
                (
                    dep.mod_id.as_str(),
                    dep.dependency_type,
                    dep.version_range.to_string(),
                    dep.ordering,
                    dep.side,
                )
            })
            .collect();
        assert_eq!(
            dependencies,
            [
                (
                    "neoforge",
                    DependencyType::Required,
                    "[26.3.0.64-beta,)".to_owned(),
                    Ordering::None,
                    DependencySide::Both
                ),
                (
                    "minecraft",
                    DependencyType::Required,
                    "[26.3]".to_owned(),
                    Ordering::None,
                    DependencySide::Both
                ),
            ]
        );
    }

    #[test]
    fn applies_the_fml_defaults() {
        let info = parse(
            "license = \"MIT\"\n[[mods]]\nmodId = \"examplemod\"\nnamespace = \"example\"\nversion = \"${file.jarVersion}\"\n\
             [[dependencies.examplemod]]\nmodId = \"geckolib\"\ntype = \"optional\"\n",
            "examplemod",
            "2.1.0",
        )
        .unwrap();
        assert_eq!(info.mod_id, "examplemod");
        assert_eq!(info.namespace, "example");
        assert_eq!(info.version.to_string(), "2.1.0");
        assert_eq!(info.display_name, "examplemod");
        assert_eq!(info.description, "MISSING DESCRIPTION");
        let dep = &info.dependencies[0];
        assert_eq!(dep.dependency_type, DependencyType::Optional);
        assert_eq!(dep.ordering, Ordering::None);
        assert_eq!(dep.side, DependencySide::Both);
        assert_eq!(dep.version_range.to_string(), "any version");
    }

    #[test]
    fn picks_the_entry_of_the_mod() {
        let text = "license = \"MIT\"\n[[mods]]\nmodId = \"first\"\n[[mods]]\nmodId = \"second\"\n";
        assert_eq!(parse(text, "second", "1").unwrap().mod_id, "second");
        let none = parse(text, "third", "1").unwrap_err();
        assert!(none.contains("\"third\""), "{none}");
    }

    #[test]
    fn properties_are_kept_for_the_file() {
        let info = parse(
            "license = \"MIT\"\n[properties]\nstringy = \"x\"\nnumber = 3\n[[mods]]\nmodId = \"ok\"\n",
            "ok",
            "1",
        )
        .unwrap();
        assert_eq!(info.file_properties["stringy"].as_str(), Some("x"));
        assert_eq!(info.file_properties["number"].as_integer(), Some(3));
        assert!(info.mod_properties.is_empty());
        let none = parse("license = \"MIT\"\n[[mods]]\nmodId = \"ok\"\n", "ok", "1").unwrap();
        assert!(none.file_properties.is_empty());
        let bad = parse(
            "license = \"MIT\"\nproperties = 1\n[[mods]]\nmodId = \"ok\"\n",
            "ok",
            "1",
        );
        assert!(bad.is_err());
    }

    #[test]
    fn issue_tracker_urls_are_read_at_file_and_mod_level() {
        let file_only = parse(
            "license = \"MIT\"\nissueTrackerURL = \"https://file.test/issues\"\n[[mods]]\nmodId = \"ok\"\n",
            "ok",
            "1",
        )
        .unwrap();
        assert_eq!(file_only.issue_tracker_url, None);
        assert_eq!(file_only.issues_url(), Some("https://file.test/issues"));
        let both = parse(
            "license = \"MIT\"\nissueTrackerURL = \"https://file.test/issues\"\n[[mods]]\nmodId = \"ok\"\nissueTrackerURL = \"https://mod.test/issues\"\n",
            "ok",
            "1",
        )
        .unwrap();
        assert_eq!(
            both.issue_tracker_url.as_deref(),
            Some("https://mod.test/issues")
        );
        assert_eq!(
            both.file_issue_tracker_url.as_deref(),
            Some("https://file.test/issues")
        );
        assert_eq!(both.issues_url(), Some("https://mod.test/issues"));
        let none = parse("license = \"MIT\"\n[[mods]]\nmodId = \"ok\"\n", "ok", "1").unwrap();
        assert_eq!(none.issues_url(), None);
        // FML never parses the mod-level key as a URL.
        let plain = parse(
            "license = \"MIT\"\n[[mods]]\nmodId = \"ok\"\nissueTrackerURL = \"no scheme\"\n",
            "ok",
            "1",
        )
        .unwrap();
        assert_eq!(plain.issues_url(), Some("no scheme"));
    }

    #[test]
    fn mod_url_and_url_rules_follow_fml() {
        let with = |value: &str| {
            parse(
                &format!("license = \"MIT\"\n[[mods]]\nmodId = \"ok\"\nmodUrl = \"{value}\"\n"),
                "ok",
                "1",
            )
        };
        assert_eq!(
            with("https://mod.test").unwrap().mod_url.as_deref(),
            Some("https://mod.test")
        );
        for placeholder in ["", "  ", "https://myurl.me/x", "https://example.invalid"] {
            assert_eq!(with(placeholder).unwrap().mod_url, None, "{placeholder}");
        }
        assert!(with("no scheme").is_err());
    }

    #[test]
    fn modproperties_are_read_for_the_mod_id() {
        let info = parse(
            "license = \"MIT\"\n[modproperties.ok]\nkey = \"v\"\n[modproperties.other]\nkey = \"w\"\n[[mods]]\nmodId = \"ok\"\n",
            "ok",
            "1",
        )
        .unwrap();
        assert_eq!(info.mod_properties.len(), 1);
        assert_eq!(info.mod_properties["key"].as_str(), Some("v"));
    }

    #[test]
    fn referral_url_is_read_on_a_dependency() {
        let info = parse(
            "license = \"MIT\"\n[[mods]]\nmodId = \"ok\"\n[[dependencies.ok]]\nmodId = \"a\"\nreferralUrl = \"https://a.test\"\n[[dependencies.ok]]\nmodId = \"b\"\n",
            "ok",
            "1",
        )
        .unwrap();
        assert_eq!(
            info.dependencies[0].referral_url.as_deref(),
            Some("https://a.test")
        );
        assert_eq!(info.dependencies[1].referral_url, None);
    }

    const LICENSE: &str = "license = \"MIT\"\n";

    #[test]
    fn rejects_invalid_descriptors() {
        let parse = |text: &str| parse(&format!("{LICENSE}{text}"), "ok", "1");
        let bad_id = parse("[[mods]]\nmodId = \"My-Mod\"\n").unwrap_err();
        assert!(bad_id.contains("\"My-Mod\""), "{bad_id}");
        assert!(parse("[[mods]]\nmodId = \"ok\"\nversion = \"v1\"\n").is_err());
        assert!(parse("[[mods]]\nmodId = \"ok\"\nnamespace = \"-x\"\n").is_err());
        assert!(parse("").is_err());
        let ordering =
            "[[mods]]\nmodId = \"ok\"\n[[dependencies.ok]]\nmodId = \"b\"\nordering = \"after\"\n";
        assert!(parse(ordering).unwrap_err().contains("\"after\""));
        let range =
            "[[mods]]\nmodId = \"ok\"\n[[dependencies.ok]]\nmodId = \"b\"\nversionRange = \"[1\"\n";
        assert!(parse(range).unwrap_err().starts_with("b: "));
    }

    /// `ModFileInfo` reads a missing license as "" and rejects a blank one.
    #[test]
    fn a_missing_or_blank_license_is_rejected() {
        let entry = "[[mods]]\nmodId = \"ok\"\n";
        for license in ["", "license = \"\"\n", "license = \" \"\n"] {
            assert_eq!(
                parse(&format!("{license}{entry}"), "ok", "1").unwrap_err(),
                "Missing license"
            );
        }
        assert!(parse(&format!("{LICENSE}{entry}"), "ok", "1").is_ok());
    }

    #[test]
    fn mod_id_charset_follows_fml() {
        for valid in [
            "examplemod",
            "ab",
            "my_mod2",
            "com.example",
            &"a".repeat(64),
        ] {
            assert!(is_valid_mod_id(valid), "{valid}");
        }
        for invalid in [
            "a",
            "My-Mod",
            "hello-mod",
            "2mod",
            "_mod",
            "a..b",
            "a.2",
            &"a".repeat(65),
        ] {
            assert!(!is_valid_mod_id(invalid), "{invalid}");
        }
    }
}
