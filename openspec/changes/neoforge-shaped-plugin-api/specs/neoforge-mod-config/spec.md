# Spec Delta

## Purpose

Lets a ported mod declare its configuration with a Rust TOML config spec in `ironpumpkin-mods` (`ironpumpkin_mods::config`), store it as TOML in the `config/` folder, read and change values, reload the file by polling, and send synced configs to NeoForge clients. The spec builder (`ConfigSpecBuilder`) takes comments, sections and typed values with ranges, enums and lists, and adds no new dependency.

## ADDED Requirements

### Requirement: Register a config
`ModInit::register_config` SHALL register a spec for the calling mod with a `ConfigType` and the file `config/<modid>-<type>.toml`, like NeoForge, so client-side and server-side files stay parallel. A second config with the same file SHALL fail with `ConfigError::Duplicate`. Status: planned (task 2.8).

#### Scenario: Same file twice
- **WHEN** a mod registers two `ConfigType::Local` configs without a file name
- **THEN** the second registration fails with `ConfigError::Duplicate`

#### Scenario: Default file name
- **WHEN** mod `examplemod` registers a `ConfigType::Synced` config without a file name
- **THEN** the config file is `config/examplemod-synced.toml`

### Requirement: Spec validation
`ConfigSpecBuilder::build` SHALL fail with `ConfigError::InvalidSpec` when two values share a path, a value and a section share a path, a path has an empty key, a default fails its own validator, or a comment, translation or restart flag is left without a value. Status: planned (task 2.8).

#### Scenario: Same path twice
- **WHEN** a spec defines `general.damage` twice
- **THEN** `ConfigSpecBuilder::build` fails with `ConfigError::InvalidSpec`

### Requirement: Load and correct
The host SHALL load the file when the config loads, write missing values with their defaults and comments, clamp a number out of its range to the nearest bound (`ModConfigSpec.Range.correct`), replace other values that fail their kind, allowed values or validator with the default, remove keys the spec does not define, back the old file up, rewrite it, and call `NativeMod::config_loading`. Status: planned (task 2.8).

#### Scenario: Out of range
- **WHEN** the file holds 500 for a value defined in range 0 to 100 with default 10
- **THEN** `ModConfig::get` returns 100 and the file is rewritten with 100 and the `Range: 0 ~ 100` comment

### Requirement: Read and write values
`ModConfig::get` SHALL return the current value of a path, `ModConfig::set` SHALL validate and change it in memory, and `ModConfig::save` SHALL write the file with its comments and call `NativeMod::config_reloading`. An unknown path SHALL fail with `ConfigError::UnknownPath`; a value that fails validation SHALL fail with `ConfigError::InvalidValue`. Status: planned (task 2.8).

#### Scenario: Set and save
- **WHEN** a mod sets `general.enabled` to false and calls `save`
- **THEN** the file holds `enabled = false` under `[general]`

### Requirement: Reload on file change
The host SHALL check the file of a loaded config every N server ticks (`mod_config_reload_ticks` in `pumpkin.toml`, 100 by default, 0 turns it off), read the file only when its size or modification time changed, and reload it and call `NativeMod::config_reloading` when its contents changed. Status: planned (task 2.8).

#### Scenario: Edit by an operator
- **WHEN** an operator edits a config file while the server runs
- **THEN** `ModConfig::get` returns the new value within N ticks and `NativeMod::config_reloading` runs

### Requirement: Synced configs reach clients
A `ConfigType::Synced` config SHALL feed the `neoforge:config_file` payload of the M3 (NeoForge handshake and registry sync) `SyncConfig` task through a live source (`pumpkin_core::plugin::startup::SyncedConfigSource`) that the task reads for each configuration. The body SHALL be the file as the host last read or wrote it, like `ConfigSync.syncAllConfigs`. A file that the host reloaded SHALL reach every client that configures afterwards. Status: planned (task 2.8).

#### Scenario: NeoForge client joins
- **WHEN** a NeoForge client configures with a server that has a synced config
- **THEN** it receives that config file before play starts

#### Scenario: Reloaded synced file
- **WHEN** an operator edits a synced config file, the host reloads it, and a NeoForge client then joins
- **THEN** the client receives the edited contents

### Requirement: Startup configs load before registration
A `ConfigType::Startup` config SHALL be loaded in `ModInit::register_config`, before the host calls the first content registration of a mod, so registration code can read it. `ConfigType::Local` and `ConfigType::Synced` configs SHALL load after the last `init` and the lifecycle steps (`FMLCommonSetupEvent`, `FMLLoadCompleteEvent`) and before the server starts, the order of a NeoForge 26.3 dedicated server. Status: planned (task 2.8).

#### Scenario: Toggle content
- **WHEN** a mod reads a startup flag in its `init` before it registers content
- **THEN** it gets the value from the file, not the default
