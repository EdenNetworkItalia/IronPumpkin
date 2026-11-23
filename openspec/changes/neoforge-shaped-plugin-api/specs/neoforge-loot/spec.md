# Spec Delta

## Purpose

Lets a ported mod change generated loot with the behaviour of NeoForge global loot modifiers, as the `loot` module of `ironpumpkin-mods`, declared in data pack JSON and applied by mod callbacks.

## ADDED Requirements

### Requirement: Modifier types
`ModInit::register_loot_modifier_type` SHALL register a modifier type id with the callback that decodes its JSON, and SHALL fail with `LootModifierError::ReservedNamespace` for an id without a namespace or in `minecraft`, with `LootModifierError::Duplicate` for an existing id and with `LootModifierError::RegistrationClosed` after the registration phase ends with the last `RegisterEvent`. Status: supported (task 2.16, #166, tests `registration_fails_for_a_duplicate_an_invalid_id_and_after_close`, `a_duplicate_type_fails`, `modifiers_load_from_world_data_packs_and_change_the_rolls`).

#### Scenario: Duplicate type
- **WHEN** a mod registers `examplemod:remove_items` twice
- **THEN** the second call fails with `LootModifierError::Duplicate`

### Requirement: Loading modifiers
On startup and on each `/reload` the host SHALL read every file `data/<namespace>/loot_modifiers/<path>.json` of the enabled data packs as the modifier `<namespace>:<path>`. A file of a higher pack SHALL replace a file with the same id, as `LootModifierManager` does. Status: supported (task 2.16, #166, tests `reads_nested_files_and_keeps_parse_errors`, `modifiers_load_from_world_data_packs_and_change_the_rolls`).

#### Scenario: Override by a higher pack
- **WHEN** two enabled packs have `data/examplemod/loot_modifiers/extra.json`
- **THEN** only the file of the higher pack loads as `examplemod:extra`

### Requirement: Modifier fields
The host SHALL read the `LootModifier` fields of a file: `type`, the optional `condition` (an inline loot condition or a predicate id) and `priority` (default 1000). It SHALL pass the whole JSON object to the decode callback of the type. Status: supported (task 2.16, #166, tests `a_condition_can_name_a_predicate`, `modifiers_run_by_priority_then_id`).

#### Scenario: Default priority
- **WHEN** a modifier file has no `priority`
- **THEN** the modifier has the priority 1000

### Requirement: Bad modifier files
The host SHALL log by modifier id and skip a file whose name is not a resource location, that does not parse, whose type is unknown, whose condition has an unknown type, or whose decode fails or panics. The other files SHALL load. Status: supported (task 2.16, #166, tests `bad_files_are_skipped_and_the_others_load`, `review_a_panicking_decoder_escapes_the_load`).

#### Scenario: Bad file
- **WHEN** the decode callback returns an error for `data/examplemod/loot_modifiers/broken.json`
- **THEN** the log names `examplemod:broken` and the other modifiers still load

### Requirement: Applying modifiers
After each roll of a loot table by its id (`LootTable.getRandomItems`), the host SHALL run each loaded modifier whose `condition` passes through `GlobalLootModifier::apply`: highest `priority` first, by id inside one priority, each on the list of the one before. The last list SHALL be the loot. A modifier that panics SHALL be logged by modifier id and table id, and the next modifier SHALL get the loot from before it. Status: supported (task 2.16, #166, tests `modifiers_run_by_priority_then_id`, `review_a_panicking_modifier_escapes_the_roll`, `modifiers_load_from_world_data_packs_and_change_the_rolls`).

#### Scenario: Remove an item
- **WHEN** a modifier returns the loot without `minecraft:diamond`
- **THEN** no diamond drops

### Requirement: Raw rolls
A table that another table nests, and a roll through `LootContext::roll_loot_table_raw` (`getRandomItemsRaw`), SHALL run no modifiers. A roll with no loaded modifier SHALL do no extra work. Status: supported (task 2.16, #166, tests `add_table_rolls_the_other_table_with_the_same_context`, `a_roll_without_a_world_runs_no_modifiers`).

#### Scenario: Added table
- **WHEN** a `neoforge:add_table` modifier of `minecraft:chests/simple_dungeon` adds `examplemod:extra`
- **THEN** the modifiers whose condition is the table id `examplemod:extra` do not run on the added loot

### Requirement: Built-in types and conditions
The host SHALL provide the modifier type `neoforge:add_table`, which rolls another table with the same context, and the condition `neoforge:loot_table_id`. The condition SHALL work in the `condition` of a modifier, also inside `minecraft:all_of`, `minecraft:any_of`, `minecraft:inverted` and a named predicate. Status: supported (task 2.16, #166, tests `the_loot_table_id_condition_passes_for_its_table_only`, `add_table_rolls_the_other_table_with_the_same_context`).

#### Scenario: Table id condition
- **WHEN** a modifier has the condition `neoforge:loot_table_id` for `minecraft:chests/simple_dungeon`
- **THEN** it runs for that table only

### Requirement: Unsupported data conditions are ignored
Until the M5 engine extension adds `neoforge:conditions` to the datapack loader, data files with `neoforge:conditions` SHALL load as if the conditions were absent, as the inventory states. Status: supported for loot modifier files (task 2.16, #166, test `neoforge_conditions_are_ignored`).

#### Scenario: Conditional modifier
- **WHEN** a loot modifier file has a `neoforge:conditions` block
- **THEN** the modifier loads regardless of the condition
