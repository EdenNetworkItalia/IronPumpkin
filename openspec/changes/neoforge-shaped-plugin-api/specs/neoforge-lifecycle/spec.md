# Spec Delta

## Purpose

Defines how a ported mod is described by its `neoforge.mods.toml`, which lifecycle events it receives in which order, and how it finds itself and other mods.

## ADDED Requirements

### Requirement: Metadata from neoforge.mods.toml
The host SHALL call `callbacks.get-mods-toml` before `on-load` and SHALL use the `[[mods]]` entry whose `mod-id` matches the plugin for its id, version, description, authors and dependencies. Status: planned (task 2.2).

#### Scenario: Mod id
- **WHEN** a mod's `get-mods-toml` returns one entry with mod id `examplemod`
- **THEN** the plugin is known as `examplemod` and `get-mod-container().get-mod-id()` returns it

### Requirement: Dependencies
A `required` dependency that is missing or outside its version range SHALL stop the mod from loading with an error naming the dependency; an `incompatible` one that is present SHALL do the same; `ordering` SHALL order loading; client-side dependencies SHALL be ignored. Status: planned (task 2.2).

#### Scenario: Missing required mod
- **WHEN** a mod requires `geckolib` in `[4.0,5.0)` and no such plugin is loaded
- **THEN** the mod does not load and the log names `geckolib`

### Requirement: Lifecycle order
After `on-load` returns, the host SHALL post the mod bus events in the order the `lifecycle` doc lists, for every mod in dependency order, before the server starts its first tick. Status: planned (task 2.2).

#### Scenario: Setup after registration
- **WHEN** a mod listens to `RegisterEvent` and `FMLCommonSetupEvent`
- **THEN** every `RegisterEvent` listener runs before the setup listener

### Requirement: Dedicated server environment
`get-dist` SHALL return `dedicated-server`, and listeners or entry points meant for the client dist SHALL never run. Status: planned (task 2.2).

#### Scenario: Licence gate
- **WHEN** a mod checks `get-dist` in its licence gate
- **THEN** it gets `dedicated-server`

### Requirement: Mod list
`is-loaded`, `get-mods`, `get-sorted-mods`, `size` and `get-mod-container-by-id` SHALL describe the loaded mods of the `neo-plugin` world. Status: planned (task 2.2).

#### Scenario: Optional integration
- **WHEN** a mod calls `is-loaded("jei")` and JEI is not a loaded plugin
- **THEN** it gets false

### Requirement: Inter-mod messages
`send-imc` SHALL queue a message for the target mod, and the target SHALL receive it in the `imc-messages` of its `InterModProcessEvent`. Status: planned (task 2.2).

#### Scenario: Message delivered
- **WHEN** mod A sends an IMC message to mod B during `InterModEnqueueEvent`
- **THEN** mod B's `InterModProcessEvent` lists that message

### Requirement: Built-in data packs
A pack added through `add-pack-finders-event` SHALL load from the plugin data folder with the given type, source and position. Status: planned (task 2.2).

#### Scenario: Vanilla overrides pack
- **WHEN** a mod adds an always-active server data pack at `data/vanilla_overrides`
- **THEN** its recipes replace the vanilla recipes after the next load
