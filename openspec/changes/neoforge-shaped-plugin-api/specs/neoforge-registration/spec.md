# Spec Delta

## Purpose

Lets a ported mod register blocks, items, entity types and the other content NeoForge registers through `DeferredRegister`, with holders that bind when the registry registers the entry.

## ADDED Requirements

### Requirement: Deferred registers
A mod SHALL create a `deferred-register` for a registry id and namespace, or the typed `deferred-register-blocks`, `deferred-register-items`, `deferred-register-entities` and `deferred-register-data-components`, and SHALL register entries on it from `on-load`. Status: planned (task 2.5).

#### Scenario: Register a block
- **WHEN** a mod calls `register-simple-block("mithril_block", properties)` on its blocks register in `on-load`
- **THEN** it gets a `deferred-block` for `examplemod:mithril_block` that is not bound yet

### Requirement: Entries apply at RegisterEvent
The host SHALL apply the entries of every register when it posts `RegisterEvent` for that registry, in registration order, and SHALL bind each holder then. Registration after that point SHALL fail with `registry-frozen`. Status: planned (task 2.5).

#### Scenario: Holder binds
- **WHEN** `RegisterEvent` for `minecraft:block` has been posted
- **THEN** `is-bound` of the block holder returns true and `raw-id` returns its numeric id

#### Scenario: Late registration
- **WHEN** a mod registers a block after `RegisterEvent` for `minecraft:block`
- **THEN** the call fails with `registration-error::registry-frozen`

### Requirement: Builders are records
`BlockBehaviour.Properties`, `Item.Properties`, `EntityType.Builder` and the other builders SHALL be records, and a definition SHALL fail with `invalid-definition` when a field is out of range or names an unknown entry. Status: planned (task 2.5).

#### Scenario: Unknown block item target
- **WHEN** an item definition names a block that is not registered
- **THEN** the registration fails with `invalid-definition`

### Requirement: Behaviour by type
A custom block SHALL run the Pumpkin behaviour of the `minecraft:block_type` id in its definition, and a custom item the Pumpkin item behaviour it names, backed by the phase 2 content registry. Status: planned (task 2.5).

#### Scenario: Stair block
- **WHEN** a mod registers a block with `block-type` `minecraft:stair`
- **THEN** the block places and connects like a vanilla stair

### Requirement: Registries without backing fail clearly
A registration into a registry that no phase backs yet SHALL fail with `unsupported-registry` naming the registry, except `minecraft:creative_mode_tab`, which the host SHALL accept and ignore. Status: planned (task 2.5).

#### Scenario: Fluid before its phase
- **WHEN** a mod registers a fluid and no phase backs `minecraft:fluid`
- **THEN** the call fails with `registration-error::unsupported-registry("minecraft:fluid")`

### Requirement: Duplicate ids
A second entry with the same id in one registry SHALL fail with `duplicate`. Status: planned (task 2.5).

#### Scenario: Same name twice
- **WHEN** a mod registers `examplemod:ruby` twice in `minecraft:item`
- **THEN** the second call fails with `registration-error::duplicate`

### Requirement: Registration events
The mod bus events `EntityAttributeCreationEvent`, `EntityAttributeModificationEvent`, `RegisterSpawnPlacementsEvent`, `BlockEntityTypeAddBlocksEvent`, `ExtendPoiTypesEvent` and `RegisterEvent` SHALL carry their resource, and the host SHALL apply what the listener registered when the dispatch ends. Status: planned (task 2.5).

#### Scenario: Attributes of a mod entity
- **WHEN** a listener calls `put` for a mod entity type with a max health of 40
- **THEN** a spawned entity of that type has 40 max health

### Requirement: Registry lookups
`registry-contains`, `registry-get-id`, `registry-get-key`, `registry-keys`, `is-in-tag` and `tag-entries` SHALL answer for vanilla and mod entries alike, including the `c:` and `neoforge:` tags of NeoForge `Tags`. Status: planned (task 2.5).

#### Scenario: Conventional tag
- **WHEN** a mod asks `is-in-tag("minecraft:iron_ingot", c:ingots)`
- **THEN** the answer is true

### Requirement: Data components, sound events, mob effects and attributes
Registrations into `minecraft:data_component_type`, `minecraft:sound_event`, `minecraft:mob_effect` and `minecraft:attribute` SHALL succeed and bind like the other backed registries, and the host SHALL NOT send mod components, effects or attributes to vanilla clients. Status: planned (task 2.6).

#### Scenario: A played mod sound
- **WHEN** a mod registers the sound event `examplemod:zap` and plays it with the v0.2 `play-custom-sound`
- **THEN** clients near the position receive the sound by id

### Requirement: Access to mod components, effects and attributes
`data-components` SHALL read, set and remove component values on item stacks in the JSON form of their codec, `mob-effects` SHALL add, read and remove effects and run the tick callbacks of mod effects, and `entity-attributes` SHALL read and change the attributes an entity has, for vanilla and mod entries alike. Status: planned (task 2.6).

#### Scenario: A mod component on an item
- **WHEN** a mod registers the persistent component type `examplemod:charge`, sets it to `5` on a stack with `set-component`, and the stack is saved and loaded
- **THEN** `get-component` returns `5`

#### Scenario: A mod effect ticks
- **WHEN** a mod registers a mob effect with a tick callback and applies it with `mob-effects.add-effect`
- **THEN** the host calls `mob-effect-apply-tick` while the effect lasts, and `has-effect` returns true until it ends

