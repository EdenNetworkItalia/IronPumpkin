# Spec Delta

## Purpose

Lets mods expose and look up item, fluid and energy handlers on blocks, entities and items through NeoForge capabilities, across plugins and over Pumpkin inventories.

## ADDED Requirements

### Requirement: Capability creation by name
`block-capability.create`, `entity-capability.create` and `item-capability.create` SHALL return the capability with that name, creating it on first use, and SHALL fail with `type-mismatch` when the name exists with another handler or context type. Status: planned (task 2.7).

#### Scenario: Built-in capability
- **WHEN** a mod creates the block capability `neoforge:item_handler` with `item-resource-handler` and `direction`
- **THEN** it gets the built-in item handler capability

#### Scenario: Conflicting types
- **WHEN** a mod creates `neoforge:item_handler` with `energy-handler`
- **THEN** the call fails with `capability-error::type-mismatch`

### Requirement: Providers from mods
A provider registered through `register-capabilities-event` SHALL answer lookups through the matching `callbacks.get-*-capability` function, and a returned handler id SHALL become a `capability-handle` whose calls the host forwards to that mod. Status: planned (task 2.7).

#### Scenario: Mod block exposes items
- **WHEN** another plugin calls `get-capability` on a block whose mod registered an item provider
- **THEN** it gets a handle and `insert` on it reaches the providing mod's `handler-insert`

### Requirement: Pumpkin inventories as providers
The built-in item capabilities SHALL return a handle over the Pumpkin block entity and entity inventories that vanilla exposes. Status: planned (task 2.7).

#### Scenario: Chest
- **WHEN** a mod looks up `neoforge:item_handler` on a chest
- **THEN** `size` returns 27

### Requirement: Transactions
`insert` and `extract` SHALL take an open `transaction`; changes SHALL take effect when the root transaction commits and SHALL roll back when it is dropped without commit, including changes of mod handlers, which get `transaction-closed`. Status: planned (task 2.7).

#### Scenario: Aborted extract
- **WHEN** a mod extracts 10 items in a transaction and drops it without commit
- **THEN** the inventory still holds the 10 items

### Requirement: Cache invalidation
A `block-capability-cache` SHALL return the current handle and SHALL call the invalidation listener when the block or its block entity changes. Status: planned (task 2.7).

#### Scenario: Block removed
- **WHEN** the block of a cache is broken
- **THEN** the host calls `capability-invalidated` for its listener

### Requirement: Registration closes
Calls on `register-capabilities-event` after its dispatch SHALL fail with `registration-closed`. Status: planned (task 2.7).

#### Scenario: Late provider
- **WHEN** a mod registers a provider after the event returned
- **THEN** the call fails with `capability-error::registration-closed`
