//! Mod config: a TOML config spec with the semantics of `NeoForge`'s `ModConfigSpec` and
//! `FancyModLoader`'s `ConfigTracker`.
//!
//! A mod builds a [`ConfigSpec`] with a [`ConfigSpecBuilder`] and registers it in its
//! [`NativeMod::init`](crate::NativeMod::init) with [`ModInit::register_config`]. The file is
//! `config/<modid>-<type>.toml`, next to the server, like the `NeoForge` file, so the client-side
//! and server-side files stay parallel.
//!
//! ```no_run
//! use ironpumpkin_mods::{ModInit, config::{ConfigSpec, ConfigType}};
//!
//! fn init(cx: &mut ModInit) {
//!     let mut builder = ConfigSpec::builder();
//!     builder.comment("General settings").push("general");
//!     builder.comment("Whether the mod is on").define("enabled", true);
//!     builder.define_in_range("damage", 10i64, 0, 100);
//!     builder.pop();
//!     let Ok(spec) = builder.build() else { return };
//!     if let Ok(config) = cx.register_config(ConfigType::Synced, spec) {
//!         // A synced config loads after the last `init`: keep `config` and read it later.
//!         let _ = config;
//!     }
//! }
//! ```
//!
//! # Load, correction and reload
//!
//! The host reads the file and checks every value against its spec, as `ConfigTracker.loadConfig`
//! and `ModConfigSpec.correct` do: a missing value gets its default, a number out of its range is
//! clamped to the nearest bound (`ModConfigSpec.Range.correct`), and any other value that fails
//! its kind, allowed values or validator gets its default. A list keeps the elements that pass and
//! falls back to its default when none pass. Keys that the spec does not define are removed. A
//! file that is not valid TOML is recreated with the defaults. When the corrected file differs
//! from the file on disk, the host backs the old file up as `<name>-1.toml.bak` (five backups are
//! kept, like `ConfigTracker.backUpConfig`) and rewrites it with the comments of the spec.
//!
//! While the server runs, the host checks the files every `mod_config_reload_ticks` ticks
//! (`pumpkin.toml`, 100 by default, 0 turns it off) and reloads a file that changed. This replaces
//! the file watcher of `FancyModLoader` (`ConfigWatcher`). A reload calls
//! [`NativeMod::config_reloading`](crate::NativeMod::config_reloading).
//!
//! # When each type loads
//!
//! - [`ConfigType::Startup`] loads in [`ModInit::register_config`], like `ConfigTracker.registerConfig`,
//!   so `init` reads it before it registers content.
//! - [`ConfigType::Local`] and [`ConfigType::Synced`] load at the end of `init_mods`, after the
//!   last `init` and the lifecycle steps, and before the server starts and the first world loads.
//!   A `NeoForge` 26.3 dedicated server has the same order (trace of the parity harness,
//!   `tools/parity-probe/captures/neoforge-26.3.0.64-beta/probe-trace.jsonl`): construct,
//!   `RegisterEvent`, `FMLCommonSetupEvent`, `FMLLoadCompleteEvent`,
//!   `RegisterPayloadHandlersEvent`, then `ModConfigEvent.Loading` of the synced config, then
//!   `ServerAboutToStartEvent`. `CommonModLoader` loads `LOCAL` in its "Config loading" step and
//!   `ServerLifecycleHooks.handleServerAboutToStart` loads `SYNCED`.
//!
//! [`NativeMod::config_loading`](crate::NativeMod::config_loading) runs for every config after it
//! loads; for a startup config, after `init` returns.

use std::{
    collections::BTreeSet,
    fmt::{self, Write as _},
    fs, io,
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard,
        atomic::{AtomicU32, Ordering},
    },
    time::SystemTime,
};

use pumpkin_core::plugin::{
    BoxFuture, EventHandler,
    api::events::{EventPriority, server::server_tick_end::ServerTickEndEvent},
    startup::{self, SyncedConfigSource},
};
use toml::{Table, Value};
use tracing::{error, info, warn};

use crate::{ModInit, NativeMod, server::Server};

/// The type of a mod config: where it lives, when it loads and whether clients get it.
///
/// `FancyModLoader` also has `CLIENT`, which a dedicated server never loads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ConfigType {
    /// Loaded on each side and never synced, like `ModConfig.Type.LOCAL`.
    Local,
    /// Sent to every `NeoForge` client during configuration as `neoforge:config_file`, like
    /// `ModConfig.Type.SYNCED`.
    Synced,
    /// Loaded at registration, before the mod registers content, like `ModConfig.Type.STARTUP`.
    /// Never synced: a value that changes content must agree with the client by other means.
    Startup,
}

impl ConfigType {
    /// The suffix of the default file name, like `ModConfig.Type.extension`.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Synced => "synced",
            Self::Startup => "startup",
        }
    }
}

impl fmt::Display for ConfigType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.extension())
    }
}

/// Why a config operation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// No value of the spec has this path.
    UnknownPath(String),
    /// The value fails its kind, range, allowed values or validator, or the caller asked for a
    /// different type.
    InvalidValue { path: String, reason: String },
    /// The spec is malformed: a path is empty or defined twice, a comment or flag dangles, a
    /// default fails its own validator, or a flag does not fit the config type.
    InvalidSpec(String),
    /// Another config already uses this file.
    Duplicate(String),
    /// The config has not loaded yet: a local or synced config loads after the last `init`.
    NotLoaded(String),
    /// The file cannot be read or written.
    Io { path: PathBuf, reason: String },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownPath(path) => write!(f, "the config has no value \"{path}\""),
            Self::InvalidValue { path, reason } => {
                write!(f, "invalid value for \"{path}\": {reason}")
            }
            Self::InvalidSpec(reason) => write!(f, "invalid config spec: {reason}"),
            Self::Duplicate(file) => write!(f, "the config file \"{file}\" is registered twice"),
            Self::NotLoaded(file) => write!(f, "the config file \"{file}\" has not loaded yet"),
            Self::Io { path, reason } => write!(f, "{}: {reason}", path.display()),
        }
    }
}

impl std::error::Error for ConfigError {}

/// A Rust type that a config value holds: `bool`, `i32`, `i64`, `f64`, `String`, a [`ConfigEnum`]
/// or a `Vec` of those.
///
/// An `f64` value also accepts an integer in the file. An integer value does not accept a float:
/// the float is replaced with the default.
pub trait TomlValue: Sized {
    #[doc(hidden)]
    fn from_toml(value: &Value) -> Option<Self>;
    #[doc(hidden)]
    fn into_toml(self) -> Value;
}

impl TomlValue for bool {
    fn from_toml(value: &Value) -> Option<Self> {
        value.as_bool()
    }

    fn into_toml(self) -> Value {
        Value::Boolean(self)
    }
}

impl TomlValue for i64 {
    fn from_toml(value: &Value) -> Option<Self> {
        value.as_integer()
    }

    fn into_toml(self) -> Value {
        Value::Integer(self)
    }
}

impl TomlValue for i32 {
    fn from_toml(value: &Value) -> Option<Self> {
        value
            .as_integer()
            .and_then(|value| Self::try_from(value).ok())
    }

    fn into_toml(self) -> Value {
        Value::Integer(self.into())
    }
}

impl TomlValue for f64 {
    fn from_toml(value: &Value) -> Option<Self> {
        match value {
            Value::Float(value) => Some(*value),
            Value::Integer(value) => Some(*value as Self),
            _ => None,
        }
    }

    fn into_toml(self) -> Value {
        Value::Float(self)
    }
}

impl TomlValue for String {
    fn from_toml(value: &Value) -> Option<Self> {
        value.as_str().map(ToOwned::to_owned)
    }

    fn into_toml(self) -> Value {
        Value::String(self)
    }
}

impl<T: TomlValue> TomlValue for Vec<T> {
    fn from_toml(value: &Value) -> Option<Self> {
        value.as_array()?.iter().map(T::from_toml).collect()
    }

    fn into_toml(self) -> Value {
        Value::Array(self.into_iter().map(TomlValue::into_toml).collect())
    }
}

/// A Rust enum that a config value holds by variant name, like an `EnumValue` of `NeoForge`.
/// The file accepts a name in any case (`EnumGetMethod.NAME_IGNORECASE`).
pub trait ConfigEnum: Copy + PartialEq + Send + Sync + 'static {
    /// Every variant, in the order of the "Allowed Values" comment.
    const VARIANTS: &'static [Self];
    /// The name of the variant in the file.
    fn name(self) -> &'static str;
}

impl<E: ConfigEnum> TomlValue for E {
    fn from_toml(value: &Value) -> Option<Self> {
        let name = value.as_str()?;
        E::VARIANTS
            .iter()
            .copied()
            .find(|variant| variant.name().eq_ignore_ascii_case(name))
    }

    fn into_toml(self) -> Value {
        Value::String(self.name().to_owned())
    }
}

/// A number type with a range: `i32`, `i64` or `f64`.
pub trait RangeBound: TomlValue + Copy + PartialOrd + Send + Sync + 'static {
    /// The `Range:` comment line, like `ModConfigSpec.Range.toString`.
    #[doc(hidden)]
    fn range_comment(min: Self, max: Self) -> String {
        format!("{} ~ {}", min.into_toml(), max.into_toml())
    }
}

impl RangeBound for i64 {}
impl RangeBound for f64 {}

impl RangeBound for i32 {
    fn range_comment(min: Self, max: Self) -> String {
        if max == Self::MAX {
            format!("> {min}")
        } else if min == Self::MIN {
            format!("< {max}")
        } else {
            format!("{min} ~ {max}")
        }
    }
}

/// When a changed value takes effect, like `ModConfigSpec.RestartType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Restart {
    None,
    World,
    Game,
}

type Test = Box<dyn Fn(&Value) -> bool + Send + Sync>;
type Correct = Box<dyn Fn(&Value) -> Value + Send + Sync>;

/// One value of a spec, like `ModConfigSpec.ValueSpec`.
struct ValueSpec {
    path: Vec<String>,
    key: String,
    comment: Vec<String>,
    translation: Option<String>,
    restart: Restart,
    default: Value,
    test: Test,
    /// Called for a present value that fails `test`.
    correct: Correct,
}

/// A section of a spec, or the root.
#[derive(Default)]
struct Section {
    comment: Vec<String>,
    translation: Option<String>,
    entries: Vec<(String, Entry)>,
}

enum Entry {
    Value(usize),
    Section(Section),
}

impl Section {
    fn entry(&self, key: &str) -> Option<&Entry> {
        self.entries
            .iter()
            .find_map(|(name, entry)| (name == key).then_some(entry))
    }

    fn entry_mut(&mut self, key: &str) -> Option<&mut Entry> {
        self.entries
            .iter_mut()
            .find_map(|(name, entry)| (name == key).then_some(entry))
    }

    /// The section at `path`, created if missing, or `None` when a value holds a segment.
    fn section_mut(&mut self, path: &[String]) -> Option<&mut Self> {
        let Some((first, rest)) = path.split_first() else {
            return Some(self);
        };
        if self.entry(first).is_none() {
            self.entries
                .push((first.clone(), Entry::Section(Self::default())));
        }
        match self.entry_mut(first) {
            Some(Entry::Section(section)) => section.section_mut(rest),
            _ => None,
        }
    }
}

/// A built config spec, like `ModConfigSpec`. Register it with [`ModInit::register_config`].
pub struct ConfigSpec {
    values: Vec<ValueSpec>,
    root: Section,
}

impl ConfigSpec {
    #[must_use]
    pub fn builder() -> ConfigSpecBuilder {
        ConfigSpecBuilder::default()
    }

    fn index(&self, path: &str) -> Result<usize, ConfigError> {
        self.values
            .iter()
            .position(|value| value.key == path)
            .ok_or_else(|| ConfigError::UnknownPath(path.to_owned()))
    }

    fn defaults(&self) -> Vec<Value> {
        self.values
            .iter()
            .map(|value| value.default.clone())
            .collect()
    }

    /// The values of `table`, corrected like `ModConfigSpec.correct`.
    fn correct(&self, table: &Table) -> Vec<Value> {
        self.values
            .iter()
            .map(|spec| match lookup(table, &spec.path) {
                Some(value) if (spec.test)(value) => value.clone(),
                Some(value) => {
                    let corrected = (spec.correct)(value);
                    warn!(
                        "[ironpumpkin] the config value {} was corrected from {value} to {corrected}",
                        spec.key
                    );
                    corrected
                }
                None => spec.default.clone(),
            })
            .collect()
    }

    /// The file text of `values`, with the comments of the spec, in the layout of
    /// `NightConfig`'s `TomlWriter`: comment lines start with `#`, a section indents its body by
    /// one tab.
    fn write(&self, values: &[Value]) -> String {
        let mut out = String::new();
        write_section(&mut out, &self.root, &mut Vec::new(), &self.values, values);
        out
    }
}

fn lookup<'a>(table: &'a Table, path: &[String]) -> Option<&'a Value> {
    let (last, sections) = path.split_last()?;
    let mut table = table;
    for section in sections {
        table = table.get(section)?.as_table()?;
    }
    table.get(last)
}

fn write_comment(out: &mut String, indent: &str, comment: &[String]) {
    for line in comment {
        out.push_str(indent);
        out.push('#');
        out.push_str(line);
        out.push('\n');
    }
}

fn key(name: &str) -> String {
    if !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        name.to_owned()
    } else {
        Value::String(name.to_owned()).to_string()
    }
}

fn write_section(
    out: &mut String,
    section: &Section,
    path: &mut Vec<String>,
    specs: &[ValueSpec],
    values: &[Value],
) {
    let indent = "\t".repeat(path.len());
    for (name, entry) in &section.entries {
        if let Entry::Value(index) = entry {
            write_comment(out, &indent, &specs[*index].comment);
            let _ = writeln!(out, "{indent}{} = {}", key(name), values[*index]);
        }
    }
    for (name, entry) in &section.entries {
        if let Entry::Section(child) = entry {
            if !out.is_empty() {
                out.push('\n');
            }
            path.push(key(name));
            write_comment(out, &indent, &child.comment);
            let _ = writeln!(out, "{indent}[{}]", path.join("."));
            write_section(out, child, path, specs, values);
            path.pop();
        }
    }
}

/// The pending comment and flags of the next value or section, like `BuilderContext`.
#[derive(Default)]
struct Pending {
    comment: Vec<String>,
    translation: Option<String>,
    restart: Option<Restart>,
}

/// Builds a [`ConfigSpec`], like `ModConfigSpec.Builder`.
///
/// `comment`, `translation`, `world_restart` and `game_restart` apply to the next value, and a
/// comment or translation before `push` applies to the section. A path splits on `.`, and the
/// sections of the current `push` prefix it. The first error is kept and returned by `build`.
#[derive(Default)]
pub struct ConfigSpecBuilder {
    values: Vec<ValueSpec>,
    root: Section,
    current: Vec<String>,
    pending: Pending,
    error: Option<ConfigError>,
}

fn split(path: &str) -> Vec<String> {
    path.split('.').map(ToOwned::to_owned).collect()
}

impl ConfigSpecBuilder {
    /// Adds a comment line for the next value or section. A line is written as `#<line>`.
    pub fn comment(&mut self, line: impl Into<String>) -> &mut Self {
        self.pending.comment.push(line.into());
        self
    }

    /// Sets the translation key of the next value or section, for config screens.
    pub fn translation(&mut self, key: impl Into<String>) -> &mut Self {
        self.pending.translation = Some(key.into());
        self
    }

    /// The next value keeps the value it loaded with until the server restarts. A dedicated
    /// server closes its world only when it stops, so this acts like [`Self::game_restart`].
    pub const fn world_restart(&mut self) -> &mut Self {
        self.pending.restart = Some(Restart::World);
        self
    }

    /// The next value keeps the value it loaded with until the server restarts. Not allowed in
    /// a synced config.
    pub const fn game_restart(&mut self) -> &mut Self {
        self.pending.restart = Some(Restart::Game);
        self
    }

    /// Opens the section `path` (split on `.`) inside the current one.
    pub fn push(&mut self, path: &str) -> &mut Self {
        let path = split(path);
        if path.iter().any(String::is_empty) {
            self.fail(format!(
                "the section \"{}\" has an empty key",
                path.join(".")
            ));
            return self;
        }
        self.current.extend(path);
        let pending = std::mem::take(&mut self.pending);
        if pending.restart.is_some() {
            self.fail(format!(
                "a restart flag is set before the section \"{}\"",
                self.current.join(".")
            ));
        }
        let current = self.current.clone();
        match self.root.section_mut(&current) {
            Some(section) => {
                if !pending.comment.is_empty() {
                    section.comment = pending.comment;
                }
                if pending.translation.is_some() {
                    section.translation = pending.translation;
                }
            }
            None => self.fail(format!(
                "the section \"{}\" is also a value",
                current.join(".")
            )),
        }
        self
    }

    /// Closes the innermost section.
    pub fn pop(&mut self) -> &mut Self {
        if self.current.pop().is_none() {
            self.fail("pop without an open section".to_owned());
        }
        self
    }

    /// Defines a value of type `T` with its default. A value of another type is replaced with
    /// the default.
    pub fn define<T: TomlValue + 'static>(&mut self, path: &str, default: T) -> &mut Self {
        self.define_validated(path, default, |_: &T| true)
    }

    /// Defines a value that must also pass `validator`.
    pub fn define_validated<T: TomlValue + 'static>(
        &mut self,
        path: &str,
        default: T,
        validator: impl Fn(&T) -> bool + Send + Sync + 'static,
    ) -> &mut Self {
        let default = default.into_toml();
        let correct_to = default.clone();
        self.add(
            path,
            default,
            Box::new(move |value| T::from_toml(value).is_some_and(|value| validator(&value))),
            Box::new(move |_| correct_to.clone()),
        )
    }

    /// Defines a number in `min..=max`, with the comment lines `Default:` and `Range:` after
    /// the comment of the value. A number out of range is clamped to the nearest bound.
    pub fn define_in_range<T: RangeBound>(
        &mut self,
        path: &str,
        default: T,
        min: T,
        max: T,
    ) -> &mut Self {
        if min > max {
            self.fail(format!("the range of \"{path}\" has min > max"));
            return self;
        }
        self.comment(format!(" Default: {}", default.into_toml()));
        self.comment(format!(" Range: {}", T::range_comment(min, max)));
        let fallback = default.into_toml();
        self.add(
            path,
            default.into_toml(),
            Box::new(move |value| T::from_toml(value).is_some_and(|v| min <= v && v <= max)),
            Box::new(move |value| match T::from_toml(value) {
                Some(v) if v < min => min.into_toml(),
                Some(v) if v > max => max.into_toml(),
                _ => fallback.clone(),
            }),
        )
    }

    /// Defines an enum value, with the comment line `Allowed Values:` after the comment of the
    /// value.
    pub fn define_enum<E: ConfigEnum>(&mut self, path: &str, default: E) -> &mut Self {
        let names: Vec<&str> = E::VARIANTS.iter().map(|variant| variant.name()).collect();
        self.comment(format!("Allowed Values: {}", names.join(", ")));
        self.define(path, default)
    }

    /// Defines a list whose elements must pass `element`. Elements that fail are removed, and a
    /// list with no element left, or a value that is not a list, gets the default. An empty list
    /// in the file is kept.
    pub fn define_list<T: TomlValue + 'static>(
        &mut self,
        path: &str,
        default: Vec<T>,
        element: impl Fn(&T) -> bool + Send + Sync + 'static,
    ) -> &mut Self {
        let element =
            Arc::new(move |value: &Value| T::from_toml(value).is_some_and(|v| element(&v)));
        let test_element = element.clone();
        let default = default.into_toml();
        let fallback = default.clone();
        self.add(
            path,
            default,
            Box::new(move |value| {
                value
                    .as_array()
                    .is_some_and(|items| items.iter().all(|item| test_element(item)))
            }),
            Box::new(move |value| {
                let kept: Vec<Value> = value
                    .as_array()
                    .map(|items| items.iter().filter(|item| element(item)).cloned().collect())
                    .unwrap_or_default();
                if kept.is_empty() {
                    fallback.clone()
                } else {
                    Value::Array(kept)
                }
            }),
        )
    }

    fn fail(&mut self, reason: String) {
        self.error.get_or_insert(ConfigError::InvalidSpec(reason));
    }

    fn add(&mut self, path: &str, default: Value, test: Test, correct: Correct) -> &mut Self {
        let pending = std::mem::take(&mut self.pending);
        let mut full = self.current.clone();
        full.extend(split(path));
        let key = full.join(".");
        if full.iter().any(String::is_empty) {
            self.fail(format!("the value \"{key}\" has an empty key"));
            return self;
        }
        if !test(&default) {
            self.fail(format!(
                "the validator of \"{key}\" does not accept its own default {default}"
            ));
            return self;
        }
        let index = self.values.len();
        let (name, sections) = full.split_last().map_or_else(
            || (String::new(), &[][..]),
            |(name, sections)| (name.clone(), sections),
        );
        match self.root.section_mut(sections) {
            Some(section) if section.entry(&name).is_none() => {
                section.entries.push((name, Entry::Value(index)));
            }
            _ => {
                self.fail(format!("the path \"{key}\" is defined twice"));
                return self;
            }
        }
        self.values.push(ValueSpec {
            path: full,
            key,
            comment: pending.comment,
            translation: pending.translation,
            restart: pending.restart.unwrap_or(Restart::None),
            default,
            test,
            correct,
        });
        self
    }

    /// Builds the spec.
    ///
    /// # Errors
    ///
    /// [`ConfigError::InvalidSpec`] for the first malformed definition, or a comment, translation
    /// or restart flag that no value or section took.
    pub fn build(&mut self) -> Result<ConfigSpec, ConfigError> {
        let pending = std::mem::take(&mut self.pending);
        if !pending.comment.is_empty() || pending.translation.is_some() || pending.restart.is_some()
        {
            self.fail(
                "a comment, translation or restart flag is set after the last value".to_owned(),
            );
        }
        if let Some(error) = self.error.take() {
            return Err(error);
        }
        Ok(ConfigSpec {
            values: std::mem::take(&mut self.values),
            root: std::mem::take(&mut self.root),
        })
    }
}

impl ConfigSpec {
    /// The translation key of the value or section at `path`, for config screens.
    #[must_use]
    pub fn translation(&self, path: &str) -> Option<&str> {
        if let Ok(index) = self.index(path) {
            return self.values[index].translation.as_deref();
        }
        let mut section = &self.root;
        for key in path.split('.') {
            match section.entry(key)? {
                Entry::Section(child) => section = child,
                Entry::Value(_) => return None,
            }
        }
        section.translation.as_deref()
    }
}

/// The size and modification time of a file, to see an edit without reading the file.
type Stamp = Option<(SystemTime, u64)>;

fn stamp(path: &Path) -> Stamp {
    let metadata = fs::metadata(path).ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}

#[derive(Default)]
struct State {
    /// `None` until the config loads.
    values: Option<Vec<Value>>,
    /// What `get` returns for a value with a restart flag: the value it loaded with.
    pinned: Vec<Option<Value>>,
    /// The file as the host last read or wrote it, which is also the synced body.
    text: String,
    stamp: Stamp,
}

/// A registered config of a mod with its loaded values, like `ModConfig` and its
/// `ModConfigSpec.ConfigValue`s.
pub struct ModConfig {
    mod_id: &'static str,
    config_type: ConfigType,
    file_name: String,
    path: PathBuf,
    spec: ConfigSpec,
    state: RwLock<State>,
    owner: OnceLock<&'static dyn NativeMod>,
}

impl fmt::Debug for ModConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ModConfig")
            .field("mod_id", &self.mod_id)
            .field("config_type", &self.config_type)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl ModConfig {
    #[must_use]
    pub const fn mod_id(&self) -> &'static str {
        self.mod_id
    }

    #[must_use]
    pub const fn config_type(&self) -> ConfigType {
        self.config_type
    }

    /// The file name inside the `config/` folder, also the name a client gets for a synced
    /// config.
    #[must_use]
    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    /// The path of the file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub const fn spec(&self) -> &ConfigSpec {
        &self.spec
    }

    fn read_state(&self) -> RwLockReadGuard<'_, State> {
        self.state.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn write_state(&self) -> RwLockWriteGuard<'_, State> {
        self.state.write().unwrap_or_else(PoisonError::into_inner)
    }

    fn convert<T: TomlValue>(&self, index: usize, value: &Value) -> Result<T, ConfigError> {
        T::from_toml(value).ok_or_else(|| ConfigError::InvalidValue {
            path: self.spec.values[index].key.clone(),
            reason: format!("{value} is not of the requested type"),
        })
    }

    /// The current value at `path` (sections and key joined with `.`). A value with a restart
    /// flag keeps the value it loaded with until the server restarts, like the cached `get` of
    /// `NeoForge`.
    ///
    /// # Errors
    ///
    /// [`ConfigError::UnknownPath`], [`ConfigError::NotLoaded`], or
    /// [`ConfigError::InvalidValue`] when `T` is not the type of the value.
    pub fn get<T: TomlValue>(&self, path: &str) -> Result<T, ConfigError> {
        let index = self.spec.index(path)?;
        let state = self.read_state();
        let values = state
            .values
            .as_ref()
            .ok_or_else(|| ConfigError::NotLoaded(self.file_name.clone()))?;
        let value = state.pinned[index].as_ref().unwrap_or(&values[index]);
        self.convert(index, value)
    }

    /// The default value at `path`.
    ///
    /// # Errors
    ///
    /// [`ConfigError::UnknownPath`], or [`ConfigError::InvalidValue`] when `T` is not the type of
    /// the value.
    pub fn get_default<T: TomlValue>(&self, path: &str) -> Result<T, ConfigError> {
        let index = self.spec.index(path)?;
        self.convert(index, &self.spec.values[index].default)
    }

    /// Changes the value at `path` in memory. [`Self::save`] writes it to the file. A value with
    /// a restart flag changes in the file, and `get` returns it after the restart.
    ///
    /// # Errors
    ///
    /// [`ConfigError::UnknownPath`], [`ConfigError::NotLoaded`], or
    /// [`ConfigError::InvalidValue`] when the value fails its kind, range, allowed values or
    /// validator.
    pub fn set<T: TomlValue>(&self, path: &str, value: T) -> Result<(), ConfigError> {
        let index = self.spec.index(path)?;
        let spec = &self.spec.values[index];
        let value = value.into_toml();
        if !(spec.test)(&value) {
            return Err(ConfigError::InvalidValue {
                path: spec.key.clone(),
                reason: format!("{value} fails the spec of the value"),
            });
        }
        let mut state = self.write_state();
        let values = state
            .values
            .as_mut()
            .ok_or_else(|| ConfigError::NotLoaded(self.file_name.clone()))?;
        values[index] = value;
        Ok(())
    }

    /// Writes the values to the file with the comments of the spec, then calls the reloading hook
    /// of the mod, like `ModConfigSpec.save`.
    ///
    /// # Errors
    ///
    /// [`ConfigError::NotLoaded`] or [`ConfigError::Io`].
    pub fn save(&self) -> Result<(), ConfigError> {
        let mut state = self.write_state();
        let values = state
            .values
            .as_ref()
            .ok_or_else(|| ConfigError::NotLoaded(self.file_name.clone()))?;
        let text = self.spec.write(values);
        self.write_file(&text)?;
        state.stamp = stamp(&self.path);
        state.text = text;
        drop(state);
        self.fire(|owner, config| owner.config_reloading(config));
        Ok(())
    }

    fn io_error(&self, error: &io::Error) -> ConfigError {
        ConfigError::Io {
            path: self.path.clone(),
            reason: error.to_string(),
        }
    }

    /// Writes `text` through a temporary file, like `WritingMode.REPLACE_ATOMIC`.
    fn write_file(&self, text: &str) -> Result<(), ConfigError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| self.io_error(&error))?;
        }
        let mut temporary = self.path.clone().into_os_string();
        temporary.push(".tmp");
        fs::write(&temporary, text)
            .and_then(|()| fs::rename(&temporary, &self.path))
            .map_err(|error| self.io_error(&error))
    }

    /// Copies the file to `<stem>-1.<ext>.bak` and shifts older backups up to five, like
    /// `ConfigTracker.backUpConfig`.
    fn back_up(&self) {
        const BACKUPS: u32 = 5;
        let (Some(parent), Some(stem)) = (self.path.parent(), self.path.file_stem()) else {
            return;
        };
        let stem = stem.to_string_lossy();
        let extension = self.path.extension().map_or_else(String::new, |extension| {
            extension.to_string_lossy().into_owned()
        });
        let backup = |n: u32| parent.join(format!("{stem}-{n}.{extension}.bak"));
        let result = (1..=BACKUPS).rev().try_for_each(|n| {
            let old = backup(n);
            if !old.exists() {
                Ok(())
            } else if n == BACKUPS {
                fs::remove_file(old)
            } else {
                fs::rename(old, backup(n + 1))
            }
        });
        if let Err(error) = result.and_then(|()| fs::copy(&self.path, backup(1)).map(drop)) {
            warn!(
                "[ironpumpkin] cannot back up the config file {}: {error}",
                self.path.display()
            );
        }
    }

    /// Reads the file, corrects it and rewrites it when it differs, like
    /// `ConfigTracker.loadConfig`.
    fn load_into(&self, state: &mut State) -> Result<(), ConfigError> {
        // `None`: no file. `Some(None)`: a file that is not UTF-8.
        let on_disk: Option<Option<String>> = match fs::read(&self.path) {
            Ok(bytes) => Some(String::from_utf8(bytes).ok()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(self.io_error(&error)),
        };
        let parsed = on_disk
            .as_ref()
            .map(|text| text.as_deref().and_then(|text| text.parse::<Table>().ok()));
        let values = match &parsed {
            Some(Some(table)) => self.spec.correct(table),
            Some(None) => {
                warn!(
                    "[ironpumpkin] the config file {} is not valid TOML; recreating it",
                    self.path.display()
                );
                self.spec.defaults()
            }
            None => self.spec.defaults(),
        };
        let canonical = self.spec.write(&values);
        if let Some(text) = &on_disk {
            if text.as_deref() != Some(canonical.as_str()) {
                warn!(
                    "[ironpumpkin] the config file {} is not correct; correcting it",
                    self.path.display()
                );
                self.back_up();
                self.write_file(&canonical)?;
            }
        } else {
            self.write_file(&canonical)?;
        }
        if state.values.is_none() {
            state.pinned = self
                .spec
                .values
                .iter()
                .zip(&values)
                .map(|(spec, value)| (spec.restart != Restart::None).then(|| value.clone()))
                .collect();
        }
        state.values = Some(values);
        state.stamp = stamp(&self.path);
        state.text = canonical;
        Ok(())
    }

    fn load(&self) -> Result<(), ConfigError> {
        let mut state = self.write_state();
        self.load_into(&mut state)
    }

    /// Reloads the file if it changed on disk since the host last read or wrote it. Returns
    /// whether it reloaded.
    fn reload_if_changed(&self) -> Result<bool, ConfigError> {
        let now = stamp(&self.path);
        {
            let state = self.read_state();
            if state.values.is_none() || state.stamp == now {
                return Ok(false);
            }
        }
        let mut state = self.write_state();
        if fs::read(&self.path).is_ok_and(|bytes| bytes == state.text.as_bytes()) {
            state.stamp = now;
            return Ok(false);
        }
        info!(
            "[ironpumpkin] the config file {} changed; reloading it",
            self.path.display()
        );
        self.load_into(&mut state)?;
        Ok(true)
    }

    fn fire(&self, hook: impl FnOnce(&dyn NativeMod, &Self)) {
        if let Some(owner) = self.owner.get() {
            hook(*owner, self);
        }
    }
}

impl SyncedConfigSource for ModConfig {
    fn file_name(&self) -> &str {
        &self.file_name
    }

    fn contents(&self) -> Box<[u8]> {
        self.read_state().text.as_bytes().into()
    }
}

/// The files of every registered config, so that two configs never share one.
static FILES: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());

fn check_file_name(file_name: &str) -> Result<(), ConfigError> {
    let relative = Path::new(file_name)
        .components()
        .all(|component| matches!(component, Component::Normal(_)));
    if file_name.is_empty() || !relative {
        return Err(ConfigError::InvalidSpec(format!(
            "the config file name \"{file_name}\" is not a relative path inside config/"
        )));
    }
    Ok(())
}

impl ModInit {
    /// Registers a config of this mod with the file `config/<modid>-<type>.toml`, like
    /// `ModContainer.registerConfig`. See the [module](crate::config) for when it loads.
    ///
    /// Keep the returned handle to read the values later.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Duplicate`] when another config uses the file, [`ConfigError::InvalidSpec`]
    /// when a value with `game_restart` is in a synced config, and for a startup config the
    /// [`ConfigError::Io`] of its load.
    pub fn register_config(
        &mut self,
        config_type: ConfigType,
        spec: ConfigSpec,
    ) -> Result<Arc<ModConfig>, ConfigError> {
        let file_name = format!("{}-{}.toml", self.id, config_type.extension());
        self.register_config_with_file_name(config_type, spec, file_name)
    }

    /// Registers a config with a file name of its own, which may name a folder inside
    /// `config/` (`examplemod/server.toml`). For a synced config, the client applies only a file
    /// name that its own copy of the mod registers.
    ///
    /// # Errors
    ///
    /// See [`Self::register_config`], and [`ConfigError::InvalidSpec`] when the file name is not a
    /// relative path.
    pub fn register_config_with_file_name(
        &mut self,
        config_type: ConfigType,
        spec: ConfigSpec,
        file_name: impl Into<String>,
    ) -> Result<Arc<ModConfig>, ConfigError> {
        let file_name = file_name.into();
        check_file_name(&file_name)?;
        if config_type == ConfigType::Synced
            && let Some(value) = spec
                .values
                .iter()
                .find(|value| value.restart == Restart::Game)
        {
            return Err(ConfigError::InvalidSpec(format!(
                "the value \"{}\" needs a game restart, which a synced config cannot have",
                value.key
            )));
        }
        let path = self.config_dir.join(&file_name);
        if !FILES
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(path.clone())
        {
            return Err(ConfigError::Duplicate(file_name));
        }
        let config = Arc::new(ModConfig {
            mod_id: self.id,
            config_type,
            file_name,
            path,
            spec,
            state: RwLock::new(State::default()),
            owner: OnceLock::new(),
        });
        if config_type == ConfigType::Startup {
            config.load()?;
        }
        self.configs.push(config.clone());
        Ok(config)
    }
}

/// Polls the config files every `interval` server ticks, in place of `ConfigWatcher`.
struct Poller {
    configs: Vec<Arc<ModConfig>>,
    interval: u32,
    ticks: AtomicU32,
}

impl Poller {
    fn tick(&self) {
        let ticks = self.ticks.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        if !ticks.is_multiple_of(self.interval) {
            return;
        }
        for config in &self.configs {
            match config.reload_if_changed() {
                Ok(true) => config.fire(|owner, config| owner.config_reloading(config)),
                Ok(false) => {}
                Err(error) => error!("[ironpumpkin] cannot reload a config: {error}"),
            }
        }
    }
}

impl EventHandler<ServerTickEndEvent> for Poller {
    fn handle<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        _event: &'a ServerTickEndEvent,
    ) -> BoxFuture<'a, ()> {
        self.tick();
        Box::pin(async {})
    }
}

/// Loads the local and synced configs after the last `init`, calls the loading hooks, publishes
/// the synced configs to the `SyncConfig` configuration task and starts the polling.
pub(crate) fn start(
    server: &Arc<Server>,
    configs: Vec<(Arc<ModConfig>, &'static dyn NativeMod)>,
) -> Result<(), String> {
    let configs = load_all(configs)?;
    startup::set_synced_configs(
        configs
            .iter()
            .filter(|config| config.config_type == ConfigType::Synced)
            .map(|config| config.clone() as Arc<dyn SyncedConfigSource>)
            .collect(),
    )?;
    let interval = server.basic_config.mod_config_reload_ticks;
    if interval > 0 && !configs.is_empty() {
        server.plugin_manager.register::<ServerTickEndEvent, _>(
            Arc::new(Poller {
                configs,
                interval,
                ticks: AtomicU32::new(0),
            }),
            EventPriority::Normal,
            false,
        );
    }
    Ok(())
}

fn load_all(
    configs: Vec<(Arc<ModConfig>, &'static dyn NativeMod)>,
) -> Result<Vec<Arc<ModConfig>>, String> {
    configs
        .into_iter()
        .map(|(config, owner)| {
            let _ = config.owner.set(owner);
            if config.config_type != ConfigType::Startup {
                config.load().map_err(|error| {
                    format!(
                        "native mod \"{}\" cannot load its config: {error}",
                        config.mod_id
                    )
                })?;
            }
            config.fire(|owner, config| owner.config_loading(config));
            Ok(config)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::Path,
        sync::{
            Arc,
            atomic::{AtomicU32, AtomicUsize, Ordering},
        },
    };

    use toml::{Table, Value};

    use super::{
        ConfigEnum, ConfigError, ConfigSpec, ConfigType, ModConfig, Poller, SyncedConfigSource,
        load_all,
    };
    use crate::{ModInit, NativeMod};

    fn init_in(dir: &Path, id: &'static str) -> ModInit {
        let mut init = ModInit::new(id);
        init.config_dir = dir.join("config");
        init
    }

    /// `[general]` with `enabled` (true) and `damage` (10 in 0..=100).
    fn general_spec() -> ConfigSpec {
        let mut builder = ConfigSpec::builder();
        builder.comment(" General settings").push("general");
        builder
            .comment(" Whether the mod is on")
            .define("enabled", true);
        builder.define_in_range("damage", 10i64, 0, 100);
        builder.pop();
        builder.build().unwrap()
    }

    fn table(config: &ModConfig) -> Table {
        fs::read_to_string(config.path())
            .unwrap()
            .parse::<Table>()
            .unwrap()
    }

    #[test]
    fn the_default_file_is_modid_type_toml_in_the_config_folder() {
        let dir = tempfile::tempdir().unwrap();
        let mut init = init_in(dir.path(), "examplemod");
        let synced = init
            .register_config(ConfigType::Synced, general_spec())
            .unwrap();
        assert_eq!(synced.file_name(), "examplemod-synced.toml");
        assert_eq!(
            synced.path(),
            dir.path().join("config").join("examplemod-synced.toml")
        );
        // A synced config loads after the last `init`, so the folder does not exist yet.
        assert!(!dir.path().join("config").exists());
        assert_eq!(
            synced.get::<bool>("general.enabled"),
            Err(ConfigError::NotLoaded("examplemod-synced.toml".to_owned()))
        );

        // A startup config loads at registration and creates the folder and the file.
        let startup = init
            .register_config(ConfigType::Startup, general_spec())
            .unwrap();
        assert_eq!(
            startup.path(),
            dir.path().join("config").join("examplemod-startup.toml")
        );
        let text = fs::read_to_string(startup.path()).unwrap();
        assert_eq!(
            text,
            "# General settings\n[general]\n\t# Whether the mod is on\n\tenabled = true\n\
             \t# Default: 10\n\t# Range: 0 ~ 100\n\tdamage = 10\n"
        );
        assert_eq!(init.configs.len(), 2);
    }

    #[test]
    fn a_second_config_with_the_same_file_is_a_duplicate() {
        let dir = tempfile::tempdir().unwrap();
        let mut init = init_in(dir.path(), "dupmod");
        init.register_config(ConfigType::Local, general_spec())
            .unwrap();
        let error = init
            .register_config(ConfigType::Local, general_spec())
            .unwrap_err();
        assert_eq!(
            error,
            ConfigError::Duplicate("dupmod-local.toml".to_owned())
        );
        let error = init
            .register_config_with_file_name(ConfigType::Synced, general_spec(), "../escape.toml")
            .unwrap_err();
        assert!(matches!(error, ConfigError::InvalidSpec(_)), "{error}");
    }

    #[test]
    fn an_out_of_range_value_is_clamped_and_the_file_rewritten() {
        let dir = tempfile::tempdir().unwrap();
        let mut init = init_in(dir.path(), "rangemod");
        let config_dir = dir.path().join("config");
        fs::create_dir_all(&config_dir).unwrap();
        let path = config_dir.join("rangemod-startup.toml");
        fs::write(
            &path,
            "[general]\ndamage = 500\nenabled = \"yes\"\nstale = 1\n",
        )
        .unwrap();

        let config = init
            .register_config(ConfigType::Startup, general_spec())
            .unwrap();
        // `ModConfigSpec.Range.correct` clamps a number to the nearest bound; a value of the
        // wrong type gets its default; an unknown key is removed.
        assert_eq!(config.get::<i64>("general.damage"), Ok(100));
        assert_eq!(config.get::<bool>("general.enabled"), Ok(true));
        let text = fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("\t# Range: 0 ~ 100\n\tdamage = 100\n"),
            "{text}"
        );
        assert!(!text.contains("stale"), "{text}");
        let backup = fs::read_to_string(config_dir.join("rangemod-startup-1.toml.bak")).unwrap();
        assert!(backup.contains("damage = 500"));
    }

    #[test]
    fn a_file_that_is_not_toml_is_recreated_with_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let mut init = init_in(dir.path(), "brokenmod");
        let config_dir = dir.path().join("config");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(config_dir.join("brokenmod-startup.toml"), "[general\n").unwrap();
        let config = init
            .register_config(ConfigType::Startup, general_spec())
            .unwrap();
        assert_eq!(config.get::<i64>("general.damage"), Ok(10));
        assert!(config_dir.join("brokenmod-startup-1.toml.bak").exists());
    }

    #[test]
    fn a_path_defined_twice_is_an_invalid_spec() {
        let mut builder = ConfigSpec::builder();
        builder.push("general").define("damage", 1i64).pop();
        builder.define("general.damage", 2i64);
        assert!(
            matches!(builder.build(), Err(ConfigError::InvalidSpec(reason)) if reason.contains("general.damage"))
        );

        let mut builder = ConfigSpec::builder();
        builder.define("general", true).push("general");
        assert!(matches!(builder.build(), Err(ConfigError::InvalidSpec(_))));

        let mut builder = ConfigSpec::builder();
        builder.define("general..damage", 1i64);
        assert!(matches!(builder.build(), Err(ConfigError::InvalidSpec(_))));

        let mut builder = ConfigSpec::builder();
        builder.define_validated("odd", 2i64, |value| value % 2 == 1);
        assert!(matches!(builder.build(), Err(ConfigError::InvalidSpec(_))));

        let mut builder = ConfigSpec::builder();
        builder.define("a", 1i64).comment("dangling");
        assert!(matches!(builder.build(), Err(ConfigError::InvalidSpec(_))));

        let mut builder = ConfigSpec::builder();
        builder.pop();
        assert!(matches!(builder.build(), Err(ConfigError::InvalidSpec(_))));
    }

    #[test]
    fn set_and_save_write_the_key_under_its_section() {
        let dir = tempfile::tempdir().unwrap();
        let mut init = init_in(dir.path(), "savemod");
        let config = init
            .register_config(ConfigType::Startup, general_spec())
            .unwrap();
        config.set("general.enabled", false).unwrap();
        config.save().unwrap();
        let table = table(&config);
        assert_eq!(table["general"]["enabled"], Value::Boolean(false));
        let text = fs::read_to_string(config.path()).unwrap();
        assert!(text.contains("[general]\n\t# Whether the mod is on\n\tenabled = false\n"));
        assert_eq!(config.get::<bool>("general.enabled"), Ok(false));

        assert_eq!(
            config.set("general.missing", 1i64),
            Err(ConfigError::UnknownPath("general.missing".to_owned()))
        );
        assert!(matches!(
            config.set("general.damage", 101i64),
            Err(ConfigError::InvalidValue { .. })
        ));
        assert!(matches!(
            config.get::<String>("general.damage"),
            Err(ConfigError::InvalidValue { .. })
        ));
        assert_eq!(config.get_default::<i64>("general.damage"), Ok(10));
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Mode {
        Fast,
        Safe,
    }

    impl ConfigEnum for Mode {
        const VARIANTS: &'static [Self] = &[Self::Fast, Self::Safe];

        fn name(self) -> &'static str {
            match self {
                Self::Fast => "FAST",
                Self::Safe => "SAFE",
            }
        }
    }

    #[test]
    fn enums_lists_and_restart_values_follow_the_spec() {
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().join("config");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(
            config_dir.join("kindmod-startup.toml"),
            "mode = \"safe\"\nnames = [\"a\", 3, \"\"]\nratio = 2\nseed = 7\n",
        )
        .unwrap();
        let mut builder = ConfigSpec::builder();
        builder.define_enum("mode", Mode::Fast);
        builder.define_list("names", vec!["x".to_owned()], |name: &String| {
            !name.is_empty()
        });
        builder.define_in_range("ratio", 0.5, 0.0, 4.0);
        builder.translation("kindmod.config.seed").world_restart();
        builder.define("seed", 1i64);
        let spec = builder.build().unwrap();
        assert_eq!(spec.translation("seed"), Some("kindmod.config.seed"));

        let mut init = init_in(dir.path(), "kindmod");
        let config = init.register_config(ConfigType::Startup, spec).unwrap();
        // `NAME_IGNORECASE`: any case is accepted and kept.
        assert_eq!(config.get::<Mode>("mode"), Ok(Mode::Safe));
        assert_eq!(config.get::<Vec<String>>("names"), Ok(vec!["a".to_owned()]));
        assert_eq!(config.get::<f64>("ratio"), Ok(2.0));
        let text = fs::read_to_string(config.path()).unwrap();
        assert!(
            text.contains("#Allowed Values: FAST, SAFE\nmode = \"safe\"\n"),
            "{text}"
        );
        assert!(text.contains("# Range: 0.0 ~ 4.0\nratio = 2\n"), "{text}");

        // A value with a restart flag keeps the value it loaded with.
        config.set("seed", 9i64).unwrap();
        assert_eq!(config.get::<i64>("seed"), Ok(7));
    }

    #[test]
    fn a_synced_config_cannot_need_a_game_restart() {
        let dir = tempfile::tempdir().unwrap();
        let mut builder = ConfigSpec::builder();
        builder.game_restart().define("seed", 1i64);
        let mut init = init_in(dir.path(), "restartmod");
        let error = init
            .register_config(ConfigType::Synced, builder.build().unwrap())
            .unwrap_err();
        assert!(matches!(error, ConfigError::InvalidSpec(_)), "{error}");
    }

    static LOADINGS: AtomicUsize = AtomicUsize::new(0);
    static RELOADINGS: AtomicUsize = AtomicUsize::new(0);

    struct HookMod;

    impl NativeMod for HookMod {
        fn id(&self) -> &'static str {
            "hookmod"
        }

        fn display_name(&self) -> &'static str {
            "Hook mod"
        }

        fn version(&self) -> &'static str {
            "1.0.0"
        }

        fn init(&self, _cx: &mut ModInit) {}

        fn config_loading(&self, config: &ModConfig) {
            assert_eq!(config.get::<i64>("general.damage"), Ok(10));
            LOADINGS.fetch_add(1, Ordering::Relaxed);
        }

        fn config_reloading(&self, config: &ModConfig) {
            assert_eq!(config.get::<i64>("general.damage"), Ok(7));
            RELOADINGS.fetch_add(1, Ordering::Relaxed);
        }
    }

    static HOOK_MOD: HookMod = HookMod;

    #[test]
    fn an_edit_reloads_within_the_interval_and_changes_the_synced_body() {
        let dir = tempfile::tempdir().unwrap();
        let mut init = init_in(dir.path(), "hookmod");
        let config = init
            .register_config(ConfigType::Synced, general_spec())
            .unwrap();
        let configs = load_all(vec![(config.clone(), &HOOK_MOD)]).unwrap();
        assert_eq!(LOADINGS.load(Ordering::Relaxed), 1);
        let file = fs::read(config.path()).unwrap();
        assert_eq!(*config.contents(), *file);

        let edited = String::from_utf8(file)
            .unwrap()
            .replace("damage = 10", "damage = 7");
        fs::write(config.path(), &edited).unwrap();
        let poller = Poller {
            configs,
            interval: 3,
            ticks: AtomicU32::new(0),
        };
        poller.tick();
        poller.tick();
        assert_eq!(config.get::<i64>("general.damage"), Ok(10));
        assert_eq!(RELOADINGS.load(Ordering::Relaxed), 0);
        poller.tick();
        assert_eq!(config.get::<i64>("general.damage"), Ok(7));
        assert_eq!(RELOADINGS.load(Ordering::Relaxed), 1);
        assert_eq!(*config.contents(), *edited.as_bytes());
        assert_eq!(
            SyncedConfigSource::file_name(&*config),
            "hookmod-synced.toml"
        );

        // No change on disk: no reload.
        for _ in 0..3 {
            poller.tick();
        }
        assert_eq!(RELOADINGS.load(Ordering::Relaxed), 1);
        assert!(Arc::strong_count(&config) >= 2);
    }

    static STARTUP_FLAG: AtomicUsize = AtomicUsize::new(0);

    struct StartupMod;

    impl NativeMod for StartupMod {
        fn id(&self) -> &'static str {
            "startupmod"
        }

        fn display_name(&self) -> &'static str {
            "Startup mod"
        }

        fn version(&self) -> &'static str {
            "1.0.0"
        }

        fn init(&self, cx: &mut ModInit) {
            let mut builder = ConfigSpec::builder();
            builder.define("extra_block", false);
            let config = cx
                .register_config(ConfigType::Startup, builder.build().unwrap())
                .unwrap();
            if config.get::<bool>("extra_block").unwrap() {
                STARTUP_FLAG.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    #[test]
    fn init_reads_a_startup_config_from_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().join("config");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(
            config_dir.join("startupmod-startup.toml"),
            "extra_block = true\n",
        )
        .unwrap();
        let mut init = init_in(dir.path(), "startupmod");
        StartupMod.init(&mut init);
        assert_eq!(STARTUP_FLAG.load(Ordering::Relaxed), 1);
    }
}
