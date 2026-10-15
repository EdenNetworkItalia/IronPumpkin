# Spec Delta

## Purpose

Lets a ported mod declare its configuration with a Rust TOML config spec in `ironpumpkin-mods`, store it as TOML in the `config/` folder, read and change values, reload the file by polling, and send synced configs to NeoForge clients. The spec builder takes comments, sections and typed values with ranges, enums and lists, and adds no new dependency.

## ADDED Requirements

### Requirement: Register a config
`register-config` SHALL register a spec for the calling mod with a type `local`, `synced` or `startup` and an optional file name, defaulting to `<modid>-<type>.toml`, and SHALL fail with `duplicate` for a second config with the same type and file name. The file SHALL live in the `config/` folder, like the NeoForge file, so client-side and server-side files stay parallel. Status: planned (task 2.8).

#### Scenario: Default file name
- **WHEN** mod `examplemod` registers a `synced` config without a file name
- **THEN** the config file is `config/examplemod-synced.toml`

### Requirement: Spec validation
The host SHALL reject a spec with `invalid-spec` when two values share a path or a path is empty. Status: planned (task 2.8).

#### Scenario: Same path twice
- **WHEN** a spec defines `general.damage` twice
- **THEN** `register-config` fails with `config-error::invalid-spec`

### Requirement: Load and correct
The host SHALL load the file when the config registers, write missing values with their defaults and comments, replace values that fail their kind, range, allowed list or validator callback with the default, and call the loading hook of the mod (a `NativeMod` method or a Pumpkin event). Status: planned (task 2.8).

#### Scenario: Out of range
- **WHEN** the file holds 500 for a value defined in range 0 to 100 with default 10
- **THEN** `get` returns 10 and the file is rewritten with 10

### Requirement: Read and write values
`get` SHALL return the current value of a path, `set` SHALL validate and change it in memory, and `save` SHALL write the file with its comments. An unknown path SHALL fail with `unknown-path`; a value that fails validation SHALL fail with `invalid-value`. Status: planned (task 2.8).

#### Scenario: Set and save
- **WHEN** a mod sets `general.enabled` to false and calls `save`
- **THEN** the file holds `enabled = false` under `[general]`

### Requirement: Reload on file change
The host SHALL poll the file of a loaded config every N ticks, and SHALL reload it and call the reloading hook of the mod (a `NativeMod` method or a Pumpkin event) when the file changed on disk. Status: planned (task 2.8).

#### Scenario: Edit by an operator
- **WHEN** an operator edits a config file while the server runs
- **THEN** `get` returns the new value within N ticks and the reloading hook of the mod runs

### Requirement: Synced configs reach clients
A `synced` config SHALL feed the `neoforge:config_file` payload of the M3 (NeoForge handshake and registry sync) `SyncConfig` task through a live source that the task reads for each configuration. A file that the host reloaded SHALL reach every client that configures afterwards. Status: planned (task 2.8).

#### Scenario: NeoForge client joins
- **WHEN** a NeoForge client configures with a server that has a synced config
- **THEN** it receives that config file before play starts

#### Scenario: Reloaded synced file
- **WHEN** an operator edits a synced config file, the host reloads it, and a NeoForge client then joins
- **THEN** the client receives the edited contents

### Requirement: Startup configs load before registration
A `startup` config SHALL be loaded before the host calls the first content registration of a mod, so registration code can read it. Status: planned (task 2.8).

#### Scenario: Toggle content
- **WHEN** a mod reads a startup flag in its `init` before it registers content
- **THEN** it gets the value from the file, not the default
