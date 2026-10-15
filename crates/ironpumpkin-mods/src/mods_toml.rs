//! The `neoforge.mods.toml` of a native mod ([`NativeMod::mods_toml`](crate::NativeMod::mods_toml)).
//!
//! The keys and defaults follow `FancyModLoader` 12.0.8 `ModFileInfo`, `ModInfo` and
//! `ModInfo.ModVersion`: the `[[mods]]` entry gives the id, namespace, version, display name,
//! description and authors, and `[[dependencies.<modId>]]` gives the dependencies. `license` must
//! be set and not blank, as `ModFileInfo` requires ("Missing license"); its value is not kept.
//!
//! The host does not read the other keys: `modLoader`, `loaderVersion`, `showAsResourcePack`,
//! `showAsDataPack`, `services`, `properties`, `issueTrackerURL`, `[[mixins]]`,
//! `[[accessTransformers]]`; in a `[[mods]]` entry `logoFile`, `logoBlur`, `updateJSONURL`,
//! `modUrl`, `credits`, `displayURL`, `features`, `modproperties`; in a dependency `referralUrl`.
//! design.md lists why for each one.

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
