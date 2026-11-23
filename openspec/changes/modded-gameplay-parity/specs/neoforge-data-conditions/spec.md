# Spec Delta

## Purpose

Makes the server evaluate the `neoforge:conditions` key of data pack files, with the NeoForge 26.3 condition types, once for every loader that NeoForge covers.

## ADDED Requirements

### Requirement: Evaluate conditions on load
Each data pack loader that NeoForge covers SHALL evaluate the `neoforge:conditions` list of a JSON file after the parse. A file whose conditions all pass SHALL load. A file with a failing condition SHALL NOT load and SHALL NOT produce an error. A value that is not a JSON object SHALL use the `neoforge:value` form. Status: planned (wave 2).

#### Scenario: A passing condition
- **WHEN** a recipe file has `"neoforge:conditions": [{"type": "neoforge:always"}]`
- **THEN** the recipe loads

#### Scenario: A failing condition
- **WHEN** a recipe file has `"neoforge:conditions": [{"type": "neoforge:never"}]`
- **THEN** the recipe does not load, the server logs no error, and the other recipes load

#### Scenario: The value form
- **WHEN** a list entry has the form `{"neoforge:conditions": [..], "neoforge:value": 5}` and the condition fails
- **THEN** the loader skips the entry

### Requirement: Condition types
The server SHALL support the condition types `neoforge:always`, `never`, `registered`, `mod_loaded`, `tag_empty`, `feature_flags_enabled`, `and`, `or` and `not`. `registered` and `tag_empty` SHALL default their registry to `minecraft:item`. `feature_flags_enabled` SHALL need a non-empty list. Status: planned (wave 2).

#### Scenario: Registered
- **WHEN** a condition `neoforge:registered` names an item that a mod registers
- **THEN** the condition passes, and it fails for an item that no mod registers

#### Scenario: Combination
- **WHEN** a file uses `neoforge:and` of `always` and `not` of `never`
- **THEN** the condition passes

### Requirement: Registered without a lookup
`neoforge:registered` SHALL be false for a registry that Pumpkin has no lookup for. The server SHALL log a warning. Status: planned (wave 2).

#### Scenario: An unknown registry
- **WHEN** a condition `neoforge:registered` names a registry without a Pumpkin lookup
- **THEN** the condition fails and the server logs a warning

### Requirement: Feature flags
`neoforge:feature_flags_enabled` SHALL read the enabled feature flags of the server in every loader. Status: planned (wave 2).

#### Scenario: An enabled flag
- **WHEN** a condition names a flag that the server enables
- **THEN** the condition passes in every loader

### Requirement: Old condition names
The server SHALL NOT accept the 21.1 names `neoforge:true`, `false` and `item_exists`. A file with such a type SHALL be skipped with a log line. Status: planned (wave 2).

#### Scenario: An old name
- **WHEN** a file uses the condition type `neoforge:true`
- **THEN** the server logs the file and the unknown type, and skips the file

### Requirement: Mod loaded
`neoforge:mod_loaded` SHALL pass when the named mod is in the native mod list. A Wasm plugin SHALL NOT count as a mod. Status: planned (wave 2).

#### Scenario: A mod
- **WHEN** a file has the condition `mod_loaded` for `examplemod` and the modpack has that mod
- **THEN** the file loads

#### Scenario: A Wasm plugin
- **WHEN** a Wasm plugin has the id `examplemod` and no mod has that id
- **THEN** the condition fails

### Requirement: Tags load first
The loaders SHALL run in two passes: tags first, then the other loaders. `neoforge:tag_empty` SHALL see the tags of every data pack of the same load. Status: planned (wave 2).

#### Scenario: A tag from another data pack
- **WHEN** one data pack defines the item tag `c:ingots/tin` with an entry, and a recipe in another data pack has the condition `not tag_empty` for it
- **THEN** the recipe loads

### Requirement: Covered loaders
The condition evaluation SHALL apply to the recipe, advancement, loot table, loot predicate, data map and loot modifier loaders. It SHALL apply to every other JSON loader that NeoForge patches. It SHALL NOT apply to tag files. One shared function SHALL do the evaluation. Status: planned (wave 2).

#### Scenario: A data map entry
- **WHEN** a data map file has an entry with a failing condition
- **THEN** the data map does not include the entry

#### Scenario: A global loot modifier
- **WHEN** a loot modifier file has a failing condition
- **THEN** the modifier does not apply

#### Scenario: A tag file
- **WHEN** a tag file has a `neoforge:conditions` key
- **THEN** the loader ignores the key and loads the tag

### Requirement: Mod condition types
A mod SHALL register a condition type in a decoder registry. The condition SHALL decode from the JSON of the file and SHALL answer with the load context. Status: planned (wave 2).

#### Scenario: A mod condition
- **WHEN** a mod registers the condition `examplemod:config_flag` that reads its config, and a recipe uses it with the flag off
- **THEN** the recipe does not load

### Requirement: Conditions are server-only
A `Display` connection and a `Real` connection SHALL receive no data of a condition. Status: planned (wave 2).

#### Scenario: A client joins
- **WHEN** a client joins a modpack whose data files use conditions
- **THEN** the join succeeds and no packet holds a condition
