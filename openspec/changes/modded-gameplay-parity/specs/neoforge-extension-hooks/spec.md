# Spec Delta

## Purpose

Applies the scope rule to the inventory, makes the engine call the hooks of the NeoForge extension interfaces where NeoForge calls them, and closes the remaining `M5 engine extension:` rows.

## ADDED Requirements

### Requirement: Scope follows the modpack demand
The inventory SHALL apply the scope rule to every `M5 engine extension:` row. A row SHALL stay `planned` when a mod of the reference modpack scan uses it on the server. A row SHALL stay `planned` when a mod of `examples/modpack` needs it. Every other row SHALL become `not supported` with the reason "no mod of the reference modpack uses it". Status: planned (wave 1).

#### Scenario: A row without demand
- **WHEN** the scope rule runs and no mod of the scan uses a row
- **THEN** the row says `not supported` with the reason "no mod of the reference modpack uses it"

#### Scenario: The status script
- **WHEN** `python3 -I tools/openspec-sync/mapping_status.py --check` runs after the rule
- **THEN** it passes

### Requirement: Closed rows
Each `M5 engine extension:` row that stays `planned` SHALL end `supported`, with a test, or `not supported`, with the reason. Status: planned (wave 6).

#### Scenario: A closed inventory
- **WHEN** the change is archived
- **THEN** no row has the status `planned` with the prefix `M5 engine extension:`

### Requirement: Register a hook implementation
A mod SHALL register a hook implementation per hook and per target with the builder of the target. The targets are blocks, items, entity types, block entity types, fluids and mob effects. The upstream behaviour traits SHALL NOT get a new method. Status: planned (wave 5).

#### Scenario: A mod block hook
- **WHEN** a mod block builder registers an implementation of a hook that NeoForge calls on neighbour changes
- **THEN** the engine calls the implementation when a neighbour of the block changes

#### Scenario: A mob effect hook
- **WHEN** a mod mob effect builder registers an implementation of a mob effect hook
- **THEN** the engine calls it where NeoForge calls that hook

### Requirement: Slot tables
At the freeze, each hook that at least one mod implements SHALL get a table indexed by the target id. A target without an implementation SHALL have an empty slot. A hook that no mod implements SHALL get no table. A filled slot SHALL call the mod implementation. An empty slot SHALL call the vanilla function. Status: planned (wave 5).

#### Scenario: A hook with no implementation
- **WHEN** no mod implements a hook
- **THEN** the call site calls the vanilla function and the server holds no table for the hook

#### Scenario: An empty slot
- **WHEN** a mod implements a hook for one block and the engine calls the hook for another block
- **THEN** the vanilla function answers

### Requirement: The vanilla answer in a function
The hard-coded vanilla answer of a hook SHALL live in a function. The function SHALL first resolve the vanilla type of an M2 typed mod block. A mod block of type `minecraft:tnt` SHALL get the TNT answer. Status: planned (wave 5).

#### Scenario: A typed mod block
- **WHEN** a mod block has the M2 type `minecraft:tnt` and fire reaches it
- **THEN** the function gives the TNT answer for `onCaughtFire`

### Requirement: Hook tests
Each hook SHALL have a test with a mod target. Each hook SHALL have a test with a typed mod block. Each hook SHALL have a test with a vanilla target. Status: planned (wave 5).

#### Scenario: Three targets
- **WHEN** the tests of a hook run
- **THEN** the mod target runs its implementation, the typed mod block gets the answer of its vanilla type, and the vanilla target gets its vanilla answer

### Requirement: Cost of a hook
A server with no mod implementation of a hook SHALL pay at most two compares at a hook call site. A server with no mod content SHALL pay at most one compare at each other added call site. Status: planned (wave 5).

#### Scenario: The performance guardrail
- **WHEN** the benches of the performance guardrail run after a hook task
- **THEN** no bench reports a significant regression

### Requirement: Remaining rows
These rows SHALL each become `supported` with a test or `not supported` with the reason: `ChunkDataEvent.Load` and `.Save`, `EntityEvent.Size`, `ModifyDefaultComponentsEvent` and `DefaultDataComponentsBoundEvent`. The same holds for the three `RegisterCauldronInteractionEvent` rows and `neoforge:auxiliary_light_data`. It also holds for `neoforge:entity_data_serializers`, `NeoForgeLootContextParams`, `common.advancements.critereon` and `common.command`. Status: planned (wave 6).

#### Scenario: A supported event
- **WHEN** a row becomes `supported`
- **THEN** a test exercises the fire site, including cancel and write-back where the NeoForge event has them

#### Scenario: An unsupported row
- **WHEN** a row becomes `not supported`
- **THEN** the inventory states the reason in the row

### Requirement: Command argument types
A mod SHALL register a command argument type in the id table before the freeze. A `Real` connection SHALL parse a command that uses it with the type of the mod. The command tree SHALL leave out the nodes of a mod argument type for a `Display` connection, as the NeoForge `CommandTreeCleaner` does. Status: planned (wave 2).

#### Scenario: A Real connection
- **WHEN** a mod registers an argument type and a command that uses it
- **THEN** the command tree of a `Real` connection holds the nodes with the type of the mod

#### Scenario: A Display connection
- **WHEN** a `Display` connection receives the command tree
- **THEN** the tree holds no node of a mod argument type

### Requirement: Command argument type row
The freeze SHALL build the `command_argument_type` sync row when a mod argument type exists, because the command tree carries argument ids. The row SHALL list the vanilla names first, then the mod names in name order. Status: planned (wave 2).

#### Scenario: A row for an argument type
- **WHEN** a mod registers an argument type
- **THEN** the sync sends the `command_argument_type` row with the vanilla names first

### Requirement: Registries with an engine change
The registries `villager_profession`, `memory_module_type` and `sensor_type` SHALL each be `supported`, with a test, or `not supported`, with the reason. Each `supported` registry SHALL state its policy for a `Display` connection. Status: planned (wave 6).

#### Scenario: A villager profession
- **WHEN** the task for `villager_profession` ends
- **THEN** the inventory says whether a mod profession works, and the policy for a `Display` connection
