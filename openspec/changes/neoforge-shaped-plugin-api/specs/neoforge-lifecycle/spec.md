# Spec Delta

## Purpose

Defines how a ported mod is described, in which order its `init` and its lifecycle methods run, and how it finds itself and other mods. The lifecycle events of NeoForge are methods of the `NativeMod` trait with a default no-op. The server lifecycle events (`ServerAboutToStartEvent` and the others) are game bus events, so they are Pumpkin events that a mod registers with `ModInit::register_event`. `neoforge.mods.toml` is optional metadata.

## ADDED Requirements

### Requirement: Metadata is optional
A mod MAY return a `neoforge.mods.toml` text from `NativeMod::mods_toml()`, whose default is `None`. The host SHALL read it before any `init`, and SHALL take the mod's metadata and dependencies from the `[[mods]]` entry whose `modId` is the mod's `id`, or from the only entry, with the defaults of FancyModLoader 12.0.8 `ModInfo`. The trait methods `id`, `display_name` and `version` SHALL stay the fallback for a mod that returns `None`. Status: implemented (#103).

#### Scenario: Mod id
- **WHEN** a mod returns a `neoforge.mods.toml` with one entry with mod id `examplemod`
- **THEN** the mod is known as `examplemod` and `ModList::get_mod_container_by_id("examplemod")` returns its `ModInfo`

#### Scenario: Jar version
- **WHEN** the `[[mods]]` entry has the version `${file.jarVersion}`
- **THEN** the mod version is the `version` of the mod

#### Scenario: No mods.toml
- **WHEN** a mod returns `None` from `mods_toml`
- **THEN** the mod is known by its `id`, `display_name` and `version` methods

### Requirement: Mod id charset and namespace
A mod that has a `neoforge.mods.toml` SHALL use the mod id pattern of FancyModLoader 12.0.8 `ModInfo.VALID_MODID`, `^(?=.{2,64}$)[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$`, and the host SHALL NOT load a mod whose id does not match. The registry namespace of a mod MAY differ from its mod id, through the `namespace` key, which follows `ModInfo.VALID_NAMESPACE`, `^[a-z][a-z0-9_.-]{1,63}$`. Status: implemented (#103).

#### Scenario: Bad id
- **WHEN** a mod with a `neoforge.mods.toml` has the mod id `My-Mod`
- **THEN** the mod does not load and the log names the id

#### Scenario: Other namespace
- **WHEN** mod `examplemod` sets the namespace `example` in its `[[mods]]` entry
- **THEN** `ModList::get_mod_container_by_id("examplemod")` returns a `ModInfo` whose `namespace` is `example`, and the mod list still names the mod `examplemod`; the namespace is metadata, and each block, item, entity type or tag keeps the full name that its builder gets

### Requirement: Dependencies
A `required` dependency that is missing or outside its Maven version range SHALL stop the server with an error naming the dependency; an `optional` one that is present outside its range and an `incompatible` one that is present in its range SHALL do the same; a `discouraged` one that is present in its range SHALL log a warning; `CLIENT` dependencies SHALL be ignored, as in FancyModLoader 12.0.8 `ModSorter.verifyDependencyVersions`. Status: implemented (#103).

#### Scenario: Missing required mod
- **WHEN** a mod requires `geckolib` in `[4.0,5.0)` and no such mod is loaded
- **THEN** the mod does not load and the log names `geckolib`

### Requirement: System mods
The mod list SHALL hold the system mods `minecraft`, with the Minecraft version of the server, and `neoforge`, with the NeoForge version that the server emulates, so a dependency on them resolves as on NeoForge. Status: implemented (#103).

#### Scenario: NeoForge dependency
- **WHEN** a mod requires `neoforge` in `[26.3.0.64-beta,)`
- **THEN** the mod loads and `is_loaded("neoforge")` is true

### Requirement: Init order
The host SHALL call the `init` of the mods in load order: a dependency with the ordering `BEFORE` loads the mod before the dependency, `AFTER` after it, as in FancyModLoader 12.0.8 `ModSorter.sort`. A dependency with the ordering `NONE` adds no constraint. The mod id SHALL break ties, where FML uses the discovery order. An ordering cycle SHALL stop the server with an error naming the mods of the cycle. Status: implemented (#103).

#### Scenario: Dependency first
- **WHEN** mod `alpha` declares a dependency on mod `zeta` with the ordering `AFTER`
- **THEN** the `init` of `zeta` runs before the `init` of `alpha`

#### Scenario: Id as tie-break
- **WHEN** mods `alpha` and `beta` have no ordering between them
- **THEN** the `init` of `alpha` runs before the `init` of `beta`

### Requirement: Lifecycle order
After the `init` of the last mod returns, and before the first tick, `init_mods` SHALL call the lifecycle methods of `NativeMod` in the order of NeoForge 26.3 `CommonModLoader.load`: `common_setup`, `dedicated_server_setup`, `inter_mod_enqueue`, `inter_mod_process`, `load_complete`. Each step SHALL call the method of every mod in load order before the next step starts. Status: implemented (#103).

#### Scenario: Setup after registration
- **WHEN** a mod registers a block in `init` and overrides the common setup method
- **THEN** the block is registered before the setup method runs

### Requirement: Dedicated server environment
`mod_list::dist()` SHALL return `Dist::DedicatedServer`, and handlers or entry points meant for the client dist SHALL never run. Status: implemented (#103).

#### Scenario: Licence gate
- **WHEN** a mod checks `mod_list::dist()` in its licence gate
- **THEN** it gets `Dist::DedicatedServer`

### Requirement: Mod list
The mod list API of `ironpumpkin-mods` (`ModList::get`, `is_loaded`, `get_mods`, `get_sorted_mods`, `size` and `get_mod_container_by_id`) SHALL describe every loaded native mod and the system mods, in load order, from before the first `init`. Status: implemented (#103).

#### Scenario: Optional integration
- **WHEN** a mod calls `is_loaded("jei")` and JEI is not a loaded mod
- **THEN** it gets false

### Requirement: Inter-mod messages
The inter-mod message queue of `ironpumpkin-mods` SHALL hold the messages that a mod sends with `InterModEnqueue::send` in its `inter_mod_enqueue` method, and the target SHALL receive them from `InterModProcess::get_messages` in its `inter_mod_process` method. A message to a mod that is not loaded SHALL be dropped and `send` SHALL return false, as `InterModComms.sendTo` does. Status: implemented (#103).

#### Scenario: Message delivered
- **WHEN** mod A sends an IMC message to mod B during its inter-mod enqueue method
- **THEN** the inter-mod process method of mod B lists that message

### Requirement: Built-in data packs
A pack that a mod adds in its pack finders method SHALL load from the mod's data folder with the given type, source and position. Status: planned (follow-up of task 2.2: `DatapackManager::load_all` in `crates/pumpkin-core/src/data/datapack/mod.rs` reads only `<world>/datapacks` and has no hook for a mod pack or a reload listener).

#### Scenario: Vanilla overrides pack
- **WHEN** a mod adds an always-active server data pack at `data/vanilla_overrides`
- **THEN** its recipes replace the vanilla recipes after the next load
