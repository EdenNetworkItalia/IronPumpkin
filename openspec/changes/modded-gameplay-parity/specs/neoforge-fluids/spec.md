# Spec Delta

## Purpose

Lets a ported mod register fluids with the behaviour of `BaseFlowingFluid` and `FluidType`: the still and flowing pair, flow, buckets, cauldron content and the interaction with entities.

## ADDED Requirements

### Requirement: Register a fluid pair
A mod SHALL register a still fluid and a flowing fluid as a pair before the freeze. The pair SHALL take a `FluidType`, a fluid block, the slope find distance, the level decrease per block, the explosion resistance and the tick rate. The values SHALL default to the `BaseFlowingFluid` defaults of NeoForge. At the freeze, the pair SHALL get ids after the vanilla fluids, in name order. Status: planned (wave 3).

#### Scenario: Register a pair
- **WHEN** a mod registers `examplemod:brine` and `examplemod:flowing_brine` with a fluid block
- **THEN** both fluids exist after the freeze, and every vanilla fluid keeps its id

#### Scenario: Fluid tags
- **WHEN** a mod adds `examplemod:brine` to the tag `minecraft:water`
- **THEN** the code that tests the tag treats the fluid as water

### Requirement: Bucket and cauldron links
A fluid MAY declare a bucket link and a cauldron link. A fluid MAY have neither. The freeze SHALL fail when a declared link has a missing target or a wrong target. The error SHALL name the mod and the fluid. Status: planned (wave 3).

#### Scenario: A missing bucket
- **WHEN** a fluid pair declares a bucket item that no mod registers
- **THEN** the freeze fails with an error that names the mod, the fluid and the bucket

#### Scenario: No links
- **WHEN** a fluid declares no bucket and no cauldron link
- **THEN** the freeze succeeds

### Requirement: Shared flow code
The shared flow code SHALL take the registration id from the fluid, not from a fixed list. It SHALL NOT treat every fluid as water in the replace rule. Status: planned (wave 3).

#### Scenario: A fluid that is not water
- **WHEN** the replace rule runs for a flowing fluid that is not water
- **THEN** it applies the rule of that fluid

#### Scenario: Water and lava
- **WHEN** the vanilla water and lava flow tests run
- **THEN** they give the vanilla results

### Requirement: Flow
A mod fluid SHALL flow, spread and decay as vanilla fluid code does with the parameters of the mod. A fluid whose type allows source conversion SHALL create a source between two sources. A mod fluid SHALL be replaced only from above and not by the same fluid. Status: planned (wave 3).

#### Scenario: Spread on flat ground
- **WHEN** a player places a source of a fluid with slope find distance 4 and level decrease 1 on a flat floor
- **THEN** the fluid spreads 7 blocks with decreasing levels

#### Scenario: Source creation
- **WHEN** the type allows source conversion and two sources flank a flowing block on solid ground
- **THEN** the flowing block becomes a source

### Requirement: Fluid contact rules
Contact rules between a mod fluid and water or lava SHALL follow the NeoForge `FluidInteractionRegistry`, under the scope rule. Without demand, the inventory row SHALL be `not supported` with the reason. Status: planned (wave 3).

#### Scenario: The scope rule decides
- **WHEN** no mod of the reference modpack registers a contact rule
- **THEN** the inventory row says `not supported` with the reason "no mod of the reference modpack uses it"

### Requirement: Fluid type
A mod SHALL define a `FluidType` with the members that the engine reads: viscosity, the swim, drown and extinguish rules, the motion scale and the bucket item. The members SHALL have the NeoForge defaults. The server SHALL use these values where vanilla uses the properties of water and lava. Status: planned (wave 3).

#### Scenario: Defaults
- **WHEN** a mod defines a type without values
- **THEN** the type has viscosity 1000 and motion scale 0.014

#### Scenario: A bucket item
- **WHEN** a type names its bucket item
- **THEN** the empty bucket uses that item to pick up the fluid

### Requirement: Buckets
The vanilla empty bucket SHALL ask the fluid of the target block for its bucket through one helper call after its vanilla arms. A filled bucket of a mod fluid SHALL be an item behaviour. That behaviour SHALL place the fluid and play the sound of the type. A placement SHALL return an empty bucket. Status: planned (wave 3).

#### Scenario: Place a fluid
- **WHEN** a player uses a bucket of `examplemod:brine` on a block
- **THEN** a source of the fluid appears and the player holds an empty bucket

#### Scenario: Pick up
- **WHEN** a player uses an empty bucket on a source of a mod fluid
- **THEN** the source is gone and the player holds the bucket of that fluid

#### Scenario: Vanilla buckets
- **WHEN** a player uses an empty bucket on water or lava
- **THEN** the player gets the vanilla bucket

### Requirement: Entity interaction, water-like fluids
An entity in a water-like mod fluid SHALL swim, sink, drown, lose fire and move as in water. The server SHALL use the viscosity, swim, drown, extinguish and motion scale values of the type. Status: planned (wave 3).

#### Scenario: Swim
- **WHEN** a player enters a water-like fluid whose type allows swimming
- **THEN** the player swims

#### Scenario: Drown
- **WHEN** a type forbids drowning and a player stays under the fluid
- **THEN** the air supply of the player does not drop

#### Scenario: Extinguish
- **WHEN** a burning entity enters a fluid whose type can extinguish
- **THEN** the fire stops

### Requirement: Entity fluid scan
A per-fluid scan SHALL start only when a ported mod needs a fluid that is not water-like. The entity code SHALL keep a fast path for entities that touch only generated fluids. The entity tick bench SHALL show no significant regression. Status: planned (wave 3); the scan: planned (wave 6).

#### Scenario: The performance guardrail
- **WHEN** the entity tick bench runs in a modpack with a mod fluid and no entity in it
- **THEN** the bench reports no significant regression

### Requirement: Cauldron content and transfer
A mod SHALL link a cauldron block to a fluid, a volume and an optional level property. The registration SHALL reject a duplicate block, a duplicate fluid, an amount of 0 and a level property that does not start at 1. The cauldron code and the fluid handler of a cauldron block SHALL read one content table. A bucket SHALL fill and empty a linked cauldron. Status: planned (wave 3).

#### Scenario: Fill a cauldron
- **WHEN** a player uses a bucket of a mod fluid on a cauldron that a mod links to that fluid
- **THEN** the cauldron holds the fluid and the player holds an empty bucket

#### Scenario: A duplicate link
- **WHEN** a mod links a cauldron block that another link already uses
- **THEN** the registration fails

#### Scenario: A wrong cauldron target
- **WHEN** a fluid declares a cauldron link to a block that is not a cauldron
- **THEN** the freeze fails with an error that names the mod and the fluid

#### Scenario: A pipe
- **WHEN** a mod block asks the fluid handler of a linked cauldron and inserts a volume
- **THEN** the cauldron level changes by that volume

### Requirement: Registry sync
The freeze SHALL build the `fluid` row when a mod fluid exists, because the tags packet carries fluid ids. The row SHALL list the vanilla names first, then the mod names in name order. The server SHALL NOT send a `neoforge:fluid_type` row, because no packet carries its id. Status: planned (wave 3).

#### Scenario: A row with a mod fluid
- **WHEN** a modpack has a mod fluid
- **THEN** the sync sends the `fluid` row

#### Scenario: No fluid type row
- **WHEN** a modpack has a mod fluid
- **THEN** the sync sends no `neoforge:fluid_type` row

#### Scenario: No mod fluid
- **WHEN** a modpack has no mod fluid
- **THEN** the sync sends no `fluid` row

### Requirement: Policy for Display connections
For a `Display` connection, the encoders SHALL map a mod fluid to a vanilla display fluid. The tags packet SHALL leave out the mod fluid ids. A mod fluid block SHALL show the block state display of M2. Status: planned (wave 3).

#### Scenario: A Display connection
- **WHEN** a `Display` connection loads a chunk with a mod fluid
- **THEN** the client sees the display fluid and no packet holds a mod fluid id

### Requirement: Saves keep mod fluids
A chunk SHALL save a fluid tick with the full name of a mod fluid. A chunk that holds a tick of an unregistered fluid SHALL load. The load SHALL drop that tick, as vanilla does. Status: planned (wave 3).

#### Scenario: A pending tick
- **WHEN** a mod fluid has a scheduled tick and the chunk saves and reloads
- **THEN** the tick is still scheduled

#### Scenario: A removed mod
- **WHEN** a chunk holds a tick of a fluid whose mod the modpack no longer has
- **THEN** the chunk loads and the tick is dropped

### Requirement: Fluid spread event
The fluid spread event SHALL carry the positions and the block of the spread. Status: M4 wave 10 (#190).

#### Scenario: A flow event
- **WHEN** a mod fluid flows into a block
- **THEN** the event names that fluid, the source position, the target position and the target block
