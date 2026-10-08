# Spec Delta

## Purpose

Lets a ported mod change generated loot through NeoForge global loot modifiers, declared in data pack JSON and applied by mod callbacks.

## ADDED Requirements

### Requirement: Modifier types
`register-loot-modifier-type` SHALL register a modifier type id with the callback that decodes its JSON, and SHALL fail with `duplicate` for an existing id and with `registration-closed` after registration closes. Status: planned (task 2.16).

#### Scenario: Duplicate type
- **WHEN** a mod registers `examplemod:remove_items` twice
- **THEN** the second call fails with `loot-error::duplicate`

### Requirement: Loading modifiers
On each data load the host SHALL read `data/<namespace>/loot_modifiers/global_loot_modifiers.json`, decode each listed entry with the callback of its type, and log and skip an entry whose type is unknown or whose decode fails. Status: planned (task 2.16).

#### Scenario: Bad entry
- **WHEN** `decode-loot-modifier` returns an error for one entry
- **THEN** the log names the entry and the other modifiers still load

### Requirement: Applying modifiers
After a loot table rolls, the host SHALL run every loaded modifier whose `conditions` pass, in priority order, through `apply-loot-modifier`, and SHALL use the returned list as the loot. Status: planned (task 2.16).

#### Scenario: Remove an item
- **WHEN** a modifier returns the loot without `minecraft:diamond`
- **THEN** no diamond drops

### Requirement: Built-in types and conditions
The host SHALL provide the modifier type `neoforge:add_table` and the condition `neoforge:loot_table_id`, and `roll-loot-table` SHALL roll another table with the same context. Status: planned (task 2.16).

#### Scenario: Table id condition
- **WHEN** a modifier has the condition `neoforge:loot_table_id` for `minecraft:chests/simple_dungeon`
- **THEN** it runs for that table only

### Requirement: Unsupported data conditions are ignored
Data files with `neoforge:conditions` SHALL load as if the conditions were absent, as the inventory states. Status: planned (task 2.16).

#### Scenario: Conditional modifier
- **WHEN** a loot modifier file has a `neoforge:conditions` block
- **THEN** the modifier loads regardless of the condition
