# Spec Delta

## Purpose

Lets a ported mod register blocks, items, entity types and tags through the `ModInit` builders of `ironpumpkin-mods`. They cover what NeoForge registers with `DeferredRegister`. Entries apply at init and bind at the single freeze of the content registry.

## ADDED Requirements

### Requirement: Registration through ModInit
A mod SHALL register content from its `NativeMod::init` with `ModInit::register_block`, `register_item`, `register_entity_type` and `register_tag`. Each function SHALL take a builder (`content::BlockBuilder`, `ItemBuilder`, `EntityTypeBuilder`, `TagBuilder`) and return a `Result` with a `RegistryError`. The mapping table SHALL map `DeferredRegister.Blocks.register` to `ModInit::register_block`, and the other typed registers in the same way. Status: supported (task 2.5, #109).

#### Scenario: Register a block
- **WHEN** a mod calls `register_block` with a `BlockBuilder` named `examplemod:mithril_block` in `init`
- **THEN** the call returns `Ok`, and the block has no numeric id yet

### Requirement: Entries apply at init and bind at the freeze
The registry SHALL apply each content registration at once in `init`, and SHALL bind the entries and allocate their numeric ids at the single freeze, after the last mod's `init` and before the first world loads. Lookups after the freeze SHALL go through the frozen tables. A registration after the freeze SHALL fail with `RegistryError::RegistryFrozen`. Status: supported (task 2.5, #109).

#### Scenario: Entries bind
- **WHEN** the freeze has run
- **THEN** a lookup of `examplemod:mithril_block` in the frozen tables returns its numeric id

#### Scenario: Late registration
- **WHEN** a mod registers a block after the freeze
- **THEN** the call fails with `RegistryError::RegistryFrozen`

### Requirement: Builders validate definitions
The registry SHALL reject a definition with a `RegistryError` when a field is out of range or names an unknown entry. The vanilla `display` entry of block, item and entity type properties SHALL be the one IronPumpkin-only extension of the NeoForge properties, and the docs SHALL name it as such. Status: supported (task 2.5, #109).

#### Scenario: Unknown block item target
- **WHEN** an item builder names a block that is not registered
- **THEN** the freeze fails with `RegistryError::UnknownBlock`

#### Scenario: Display entry
- **WHEN** a mod registers a block with a `display` state that is not a vanilla state
- **THEN** the registration fails with `RegistryError::DisplayNotVanilla`

### Requirement: Behaviour by type
A custom block SHALL run the `BlockBehaviour` implementation that the mod sets on its builder or else the Pumpkin behaviour of the `minecraft:block_type` id in its definition, and a custom item the `ItemBehaviour` implementation that the mod sets or else the Pumpkin item behaviour it names, backed by the M2 (Content registry) content registry. Status: supported (task 2.25, #122, test binary `registration_events`).

#### Scenario: Stair block
- **WHEN** a mod registers a block with the block type `minecraft:oak_stairs` (26.3 has no `minecraft:block_type` registry, so a vanilla block names the type)
- **THEN** the block places and connects like a vanilla stair

#### Scenario: Own behaviour
- **WHEN** a native mod registers a block with its own `BlockBehaviour` implementation
- **THEN** the server calls that implementation for the block, as for a vanilla block with a Pumpkin behaviour

### Requirement: Registries without backing are explicit
`ModInit` SHALL offer a register function only for a registry that a phase backs. A registry without backing SHALL have the status `planned` with the reason prefix `M5 engine extension:` in the inventory, or `not supported` when it is Java-only or display-only. A registration of `minecraft:creative_mode_tab` through `ModInit::register_creative_mode_tab` SHALL be accepted, logged in one line and ignored. Status: supported (task 2.5, #109).

#### Scenario: Fluid before its phase
- **WHEN** a mod author looks for a way to register a fluid and no phase backs `minecraft:fluid`
- **THEN** `ModInit` has no function for it, and the inventory row of the `fluid` registry says `planned` with the reason `M5 engine extension:`

#### Scenario: Creative tab
- **WHEN** a ported mod registers a creative mode tab
- **THEN** the call is accepted and the server sends nothing for it

### Requirement: Duplicate ids
A second entry with the same id in one registry SHALL fail with `RegistryError::Duplicate`. Status: supported (task 2.5, #109).

#### Scenario: Same name twice
- **WHEN** a mod registers `examplemod:ruby` twice as an item
- **THEN** the second call fails with `RegistryError::Duplicate`

### Requirement: Registration events
The NeoForge events `EntityAttributeCreationEvent`, `EntityAttributeModificationEvent`, `RegisterSpawnPlacementsEvent`, `BlockEntityTypeAddBlocksEvent` and `ExtendPoiTypesEvent` SHALL be `NativeMod` methods that receive a registration handle, and the host SHALL apply what the mod registered through the handle when the method returns. Status: supported (task 2.25, #122, test binary `registration_events`).

#### Scenario: Attributes of a mod entity
- **WHEN** the attribute creation method of a mod calls `put` for a mod entity type with a max health of 40
- **THEN** a spawned entity of that type has 40 max health

### Requirement: RegisterEvent
`RegisterEvent` SHALL be a Pumpkin event that fires once per backed registry, `minecraft:block`, `minecraft:entity_type` and `minecraft:item` in that order, after the last `init` and before the first `common_setup`. A handler SHALL register content through `content::RegisterEventExt`, which registers only on the fire of its own registry and binds at the same freeze. A failed registration SHALL stop the server after the last fire. Status: supported (task 2.5, #109).

#### Scenario: Register from the event
- **WHEN** a handler of `RegisterEvent` calls `register_block` with a `BlockBuilder` named `examplemod:mithril_block`
- **THEN** the block registers on the fire of `minecraft:block` only, and the frozen tables hold it

### Requirement: Registry lookups
`registry::contains`, `id_of`, `key_of`, `keys`, `is_in_tag` and `tag_entries` of `ironpumpkin-mods` SHALL answer for vanilla and mod entries alike, from the frozen tables after the freeze, including the `c:` and `neoforge:` tags of NeoForge `Tags` that pumpkin-data generates. An unknown entry, id or tag SHALL return `None` or `false`. Status: supported (task 2.5, #109).

#### Scenario: Conventional tag
- **WHEN** a mod asks `registry::is_in_tag(ContentKind::Item, "minecraft:iron_ingot", "c:ingots")`
- **THEN** the answer is true

### Requirement: Data components, sound events, mob effects and attributes
Registrations into `minecraft:data_component_type`, `minecraft:sound_event`, `minecraft:mob_effect` and `minecraft:attribute` SHALL succeed and bind like the other backed registries, and the host SHALL NOT send mod components, effects or attributes to vanilla clients. Status: planned (task 2.6).

#### Scenario: A played mod sound
- **WHEN** a mod registers the sound event `examplemod:zap` and plays it with the v0.2 `play-custom-sound`
- **THEN** clients near the position receive the sound by id

### Requirement: Access to mod components, effects and attributes
The `data-components` module of `ironpumpkin-mods` SHALL read, set and remove component values on item stacks in the JSON form of their codec, the `mob-effects` module SHALL add, read and remove effects and run the tick callbacks of mod effects, and the `entity-attributes` module SHALL read and change the attributes an entity has, for vanilla and mod entries alike. Status: planned (task 2.6).

#### Scenario: A mod component on an item
- **WHEN** a mod registers the persistent component type `examplemod:charge`, sets it to `5` on a stack with `set-component`, and the stack is saved and loaded
- **THEN** `get-component` returns `5`

#### Scenario: A mod effect ticks
- **WHEN** a mod registers a mob effect with a tick callback and applies it with `mob-effects.add-effect`
- **THEN** the host calls `mob-effect-apply-tick` while the effect lasts, and `has-effect` returns true until it ends
