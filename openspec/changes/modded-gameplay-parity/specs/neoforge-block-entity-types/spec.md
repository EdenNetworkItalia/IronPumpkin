# Spec Delta

## Purpose

Lets a ported mod define block entity types of its own, with the behaviour of `BlockEntityType` in NeoForge: a type name, a creation set, a valid set, a new block entity, a saved block entity and client sync.

## ADDED Requirements

### Requirement: Register a block entity type
A mod SHALL register a block entity type with a name, a creation set, a valid set, a factory and a loader. The factory SHALL make a new block entity from a position and a block state. The loader SHALL make a saved block entity from a position, a block state and its NBT. Registration SHALL happen before the freeze. Status: planned (wave 1).

#### Scenario: Register a type
- **WHEN** a mod registers `examplemod:grinder` with the creation set `examplemod:grinder_block`
- **THEN** a player who places the block gets a block entity of that type

#### Scenario: Registration after the freeze
- **WHEN** a mod registers a block entity type after the freeze
- **THEN** the call fails with the frozen-registry error

### Requirement: Type ids
At the freeze, each mod type SHALL get an id after the vanilla types, in name order. The freeze SHALL fail with `RegistryError::RangeExceeded` when the types exceed `u16::MAX - 1`. Status: planned (wave 1).

#### Scenario: Id order
- **WHEN** a modpack registers two mod types
- **THEN** both ids follow the vanilla ids, in name order

### Requirement: Creation set and valid set
The creation set SHALL decide which block entity a mod block creates. The valid set SHALL only answer the validity check, as on NeoForge. `BlockEntityTypeAddBlocksEvent` MAY set the creation type of a mod block that has none. A vanilla block in a valid set SHALL keep its vanilla block entity. When two types claim one mod block, the first claim SHALL win with a warning. Status: planned (wave 1).

#### Scenario: A vanilla block in the valid set
- **WHEN** a mod type lists `minecraft:furnace` in its valid set only
- **THEN** a placed furnace creates a vanilla furnace block entity

#### Scenario: Two types claim a block
- **WHEN** two types name the same mod block in their creation sets
- **THEN** the first type creates the block entity and the server logs a warning that names both types

### Requirement: Full name lookup
The lookup of a type by name SHALL match the full name. The generated list SHALL answer only for the `minecraft` namespace. A name that resolves to no type SHALL stay out of every packet. Status: planned (wave 1); the helper of the chunk packet: M4 wave 10 (#192).

#### Scenario: A name with a vanilla path
- **WHEN** a mod registers `examplemod:chest` and the server resolves that name
- **THEN** it resolves to the mod type and not to the vanilla chest

#### Scenario: An unknown name
- **WHEN** a chunk holds a block entity whose type name resolves to no type
- **THEN** the chunk packet omits that block entity

### Requirement: Save and load
The server SHALL save a mod block entity with its full type name and its data. The server SHALL load it through the loader of the mod with the block state of its position. A world that loses the mod SHALL load. The next save SHALL write the unknown block entity data back unchanged. Status: planned (wave 1).

#### Scenario: Save and reload
- **WHEN** a grinder holds 3 items, the chunk saves and unloads, and the chunk loads again
- **THEN** the block entity has the same type and the same 3 items

#### Scenario: A removed mod
- **WHEN** a world with an `examplemod:grinder` block entity loads in a modpack without `examplemod`
- **THEN** the world loads and the next save keeps the data of that block entity

### Requirement: Ticking
A mod block entity that overrides `tick` SHALL tick in the phase of the vanilla block entities. Status: planned (wave 1).

#### Scenario: A ticking block entity
- **WHEN** a mod block entity counts ticks in `tick`
- **THEN** the count grows by 1 each game tick while its chunk is loaded

### Requirement: Client sync on a Real connection
A `Real` connection SHALL receive a mod block entity in the chunk packet and in the block entity data packet. Both packets SHALL carry the type id of the synced block entity type row. The data packet SHALL carry the vanilla update tag. The freeze SHALL build the `minecraft:block_entity_type` row when a mod type exists. Status: planned (wave 1); the update tag: M4 wave 10 (#192).

#### Scenario: Chunk packet
- **WHEN** a `Real` connection loads a chunk with a grinder
- **THEN** the chunk packet holds the block entity with the type id of `examplemod:grinder`

#### Scenario: Update packet
- **WHEN** a grinder requests an update after its synced data changes
- **THEN** the client receives an update packet with the type id and the update tag

#### Scenario: The sync row
- **WHEN** a mod registers a type
- **THEN** the sync sends the `minecraft:block_entity_type` row with the vanilla names first

### Requirement: Policy for Display connections
For a `Display` connection, the chunk packet, the block entity data packet and the `block_entity_data` item component SHALL leave out a mod block entity type. Status: planned (wave 1).

#### Scenario: A chunk on a Display connection
- **WHEN** a `Display` connection loads a chunk with a grinder
- **THEN** the chunk packet holds no block entity for the grinder

#### Scenario: An item stack on a Display connection
- **WHEN** a `Display` connection receives a stack whose `block_entity_data` names a mod type
- **THEN** the component is left out

### Requirement: Add blocks to a type
`BlockEntityTypeAddBlocksEvent` SHALL accept the name of a mod type. The event SHALL add a block to the valid set of that type. For a mod block with no creation type, the event SHALL also set the creation type. Status: planned (wave 1).

#### Scenario: An addon block
- **WHEN** an addon mod adds its block to `basemod:grinder` in the event
- **THEN** the block is valid for that type

#### Scenario: A block with no creation type
- **WHEN** the block of an addon mod has no creation type and the event adds it to `basemod:grinder`
- **THEN** a placed block creates a block entity of that type

### Requirement: Capability registration for a mod type
A mod SHALL register capabilities for a mod block entity type with the capability registration of `ironpumpkin-mods`, as for a vanilla type. Status: planned (wave 1).

#### Scenario: An item handler capability
- **WHEN** a mod registers an item handler capability for `examplemod:grinder` and a hopper asks the block
- **THEN** the hopper uses the handler of the grinder

### Requirement: Cost of a type that does not tick
A mod type that does not override `tick` SHALL cost one virtual call in the tick loop. A server with no mod type SHALL pay at most one compare at each added call site. Status: planned (wave 1).

#### Scenario: The performance guardrail
- **WHEN** the benches of the performance guardrail run on a modpack with no mod block entity type
- **THEN** no bench reports a significant regression
