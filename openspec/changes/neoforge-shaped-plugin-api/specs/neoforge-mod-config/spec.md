# Spec Delta

## Purpose

Lets a ported mod declare its configuration with a `ModConfigSpec`-shaped record, store it as TOML, read and change values, and send synced configs to NeoForge clients.

## ADDED Requirements

### Requirement: Register a config
`register-config` SHALL register a spec for the calling mod with a type `local`, `synced` or `startup` and an optional file name, defaulting to `<modid>-<type>.toml`, and SHALL fail with `duplicate` for a second config with the same type and file name. Status: planned (task 2.8).

#### Scenario: Default file name
- **WHEN** mod `examplemod` registers a `synced` config without a file name
- **THEN** the config file is `examplemod-synced.toml` in the mod's config folder

### Requirement: Spec validation
The host SHALL reject a spec with `invalid-spec` when two values share a path or a path is empty. Status: planned (task 2.8).

#### Scenario: Same path twice
- **WHEN** a spec defines `general.damage` twice
- **THEN** `register-config` fails with `config-error::invalid-spec`

### Requirement: Load and correct
The host SHALL load the file when the config registers, write missing values with their defaults and comments, replace values that fail their kind, range, allowed list or validator callback with the default, and post `ModConfigEvent.Loading`. Status: planned (task 2.8).

#### Scenario: Out of range
- **WHEN** the file holds 500 for a value defined in range 0 to 100 with default 10
- **THEN** `get` returns 10 and the file is rewritten with 10

### Requirement: Read and write values
`get` SHALL return the current value of a path, `set` SHALL validate and change it in memory, and `save` SHALL write the file. An unknown path SHALL fail with `unknown-path`; a value that fails validation SHALL fail with `invalid-value`. Status: planned (task 2.8).

#### Scenario: Set and save
- **WHEN** a mod sets `general.enabled` to false and calls `save`
- **THEN** the file holds `enabled = false` under `[general]`

### Requirement: Reload on file change
When the file of a loaded config changes on disk, the host SHALL reload it and post `ModConfigEvent.Reloading`. Status: planned (task 2.8).

#### Scenario: Edit by an operator
- **WHEN** an operator edits a config file while the server runs
- **THEN** `get` returns the new value and listeners of `mod-config-event-reloading` run

### Requirement: Synced configs reach clients
A `synced` config SHALL be sent to each NeoForge client during configuration with the `neoforge:config_file` payload of the phase 3 `SyncConfig` task. Status: planned (task 2.8).

#### Scenario: NeoForge client joins
- **WHEN** a NeoForge client configures with a server that has a synced config
- **THEN** it receives that config file before play starts

### Requirement: Startup configs load before registration
A `startup` config SHALL be loaded before the host posts `RegisterEvent`, so registration code can read it. Status: planned (task 2.8).

#### Scenario: Toggle content
- **WHEN** a mod reads a startup flag in a `RegisterEvent` listener
- **THEN** it gets the value from the file, not the default
