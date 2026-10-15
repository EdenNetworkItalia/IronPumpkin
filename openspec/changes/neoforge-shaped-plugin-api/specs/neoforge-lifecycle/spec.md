# Spec Delta

## Purpose

Defines how a ported mod is described, in which order its `init` and its lifecycle methods run, and how it finds itself and other mods. The lifecycle events of NeoForge are methods of the `NativeMod` trait with a default no-op. `neoforge.mods.toml` is optional metadata.

## ADDED Requirements

### Requirement: Metadata is optional
A mod MAY return a `neoforge.mods.toml` text from `NativeMod::mods_toml() -> Option<&'static str>`, whose default is `None`. The host SHALL read that text before it calls the mod's `init`, and SHALL use the `[[mods]]` entry whose `mod-id` matches the mod for its id, version, description, authors and dependencies. The trait methods `id`, `display_name` and `version` SHALL stay the fallback for a mod that returns `None`. Status: planned (task 2.2).

#### Scenario: Mod id
- **WHEN** a mod returns a `neoforge.mods.toml` with one entry with mod id `examplemod`
- **THEN** the mod is known as `examplemod` and `get-mod-container().get-mod-id()` returns it

#### Scenario: No mods.toml
- **WHEN** a mod returns `None` from `mods_toml`
- **THEN** the mod is known by its `id`, `display_name` and `version` methods

### Requirement: Mod id charset and namespace
A mod that has a `neoforge.mods.toml` SHALL use the NeoForge mod id charset `[a-z][a-z0-9_]{1,63}`, and the host SHALL NOT load a mod whose id does not match. The registry namespace of a mod MAY differ from its mod id, through the `namespace` key. Status: planned (task 2.2).

#### Scenario: Bad id
- **WHEN** a mod with a `neoforge.mods.toml` has the mod id `My-Mod`
- **THEN** the mod does not load and the log names the id

#### Scenario: Other namespace
- **WHEN** mod `examplemod` sets the namespace `example` in its `[[mods]]` entry
- **THEN** its content registers under `example:` and the mod list still names the mod `examplemod`

### Requirement: Dependencies
A `required` dependency that is missing or outside its version range SHALL stop the mod from loading with an error naming the dependency; an `incompatible` one that is present SHALL do the same; `ordering` SHALL order loading; client-side dependencies SHALL be ignored. Status: planned (task 2.2).

#### Scenario: Missing required mod
- **WHEN** a mod requires `geckolib` in `[4.0,5.0)` and no such mod is loaded
- **THEN** the mod does not load and the log names `geckolib`

### Requirement: Init order
The host SHALL call the `init` of the mods in dependency order, with the mod id as tie-break. `PluginManager::topological_sort` is the precedent for the sort. Status: planned (task 2.2).

#### Scenario: Dependency first
- **WHEN** mod `alpha` requires mod `zeta`
- **THEN** the `init` of `zeta` runs before the `init` of `alpha`

#### Scenario: Id as tie-break
- **WHEN** mods `alpha` and `beta` have no dependency on each other
- **THEN** the `init` of `alpha` runs before the `init` of `beta`

### Requirement: Lifecycle order
After the `init` of the last mod returns, `init_mods` SHALL call the lifecycle methods of `NativeMod` in the order of the NeoForge mod lifecycle (the lifecycle inventory in design.md), for every mod in dependency order, before the server starts its first tick. Status: planned (task 2.2).

#### Scenario: Setup after registration
- **WHEN** a mod registers a block in `init` and overrides the common setup method
- **THEN** the block is registered before the setup method runs

### Requirement: Dedicated server environment
`get-dist` SHALL return `dedicated-server`, and handlers or entry points meant for the client dist SHALL never run. Status: planned (task 2.2).

#### Scenario: Licence gate
- **WHEN** a mod checks `get-dist` in its licence gate
- **THEN** it gets `dedicated-server`

### Requirement: Mod list
The mod list API of `ironpumpkin-mods` (`is-loaded`, `get-mods`, `get-sorted-mods`, `size` and `get-mod-container-by-id`) SHALL describe every loaded native mod. Status: planned (task 2.2).

#### Scenario: Optional integration
- **WHEN** a mod calls `is-loaded("jei")` and JEI is not a loaded mod
- **THEN** it gets false

### Requirement: Inter-mod messages
The inter-mod message queue of `ironpumpkin-mods` SHALL hold the messages that a mod sends with `send-imc`, and the target SHALL receive them in the `imc-messages` of its inter-mod process method. Status: planned (task 2.2).

#### Scenario: Message delivered
- **WHEN** mod A sends an IMC message to mod B during its inter-mod enqueue method
- **THEN** the inter-mod process method of mod B lists that message

### Requirement: Built-in data packs
A pack that a mod adds in its pack finders method SHALL load from the mod's data folder with the given type, source and position. Status: planned (task 2.2).

#### Scenario: Vanilla overrides pack
- **WHEN** a mod adds an always-active server data pack at `data/vanilla_overrides`
- **THEN** its recipes replace the vanilla recipes after the next load
