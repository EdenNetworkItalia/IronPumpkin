# Spec Delta

## Purpose

Lets a ported mod register recipe types, recipe serializers and custom ingredient types, load its recipes from data packs, query them by type, and send them to NeoForge clients with `neoforge:recipe_content`.

## ADDED Requirements

### Requirement: Tag ingredient strings
The server SHALL read an ingredient written as a tag string (`"#c:ingots"`) as a tag. Status: M4 wave 10 (#191).

#### Scenario: A tag string ingredient
- **WHEN** a data pack recipe has the ingredient `"#c:ingots"`
- **THEN** a stack from that tag matches the ingredient

### Requirement: Register a recipe type and a serializer
A mod SHALL register a recipe type and a recipe serializer by name before the freeze, as entries of the id table. A recipe file whose `type` names a mod serializer SHALL load through it. A decode error SHALL be logged with the recipe id. The other recipes SHALL load. Status: planned (wave 2).

#### Scenario: A mod recipe loads
- **WHEN** a data pack has a recipe of type `examplemod:grinding` and the mod registers its serializer
- **THEN** the recipe is in the recipe snapshot under its id

#### Scenario: A decode error
- **WHEN** a recipe file of a mod type has a field of the wrong type
- **THEN** the server logs the recipe id and the error, and skips that recipe

### Requirement: Serializer codecs
A serializer SHALL register a JSON decoder and a stream encoder. The decoder SHALL decode the JSON object of a recipe file into a recipe value of the mod. The stream encoder SHALL write the bytes that the stream codec of the client half reads. Status: planned (wave 2).

#### Scenario: A stream encoder
- **WHEN** the server sends a mod recipe in `neoforge:recipe_content`
- **THEN** the bytes equal the output of the stream codec of the NeoForge client half

### Requirement: One recipe snapshot
The server SHALL hold the data pack recipes and a per-type index of the mod recipes in one immutable snapshot. `set_recipes` SHALL swap the snapshot in one step. A query SHALL see the old snapshot or the new one. The public API of `RecipeManager` SHALL stay the same. Status: planned (wave 2).

#### Scenario: A reload
- **WHEN** a reload runs while a block entity queries
- **THEN** the query sees either the old recipes or the new recipes, not a mix

#### Scenario: A vanilla recipe
- **WHEN** a player crafts a vanilla recipe
- **THEN** the result is the vanilla result

### Requirement: Query recipes by type
A mod SHALL query the recipes of one mod type by input. The query SHALL return the hint recipe first, then the recipes of the type in priority order. A query SHALL NOT scan the recipes of other types. Status: planned (wave 2).

#### Scenario: A lookup
- **WHEN** a grinder block entity asks for the recipe of `examplemod:grinding` for an input stack
- **THEN** it receives the matching recipe or none

#### Scenario: Lookup cost
- **WHEN** the recipe lookup bench runs over the test mod recipes
- **THEN** a query of one type does not scale with the recipes of other types

### Requirement: Custom ingredient types
A mod SHALL register an ingredient serializer, with a stream encoder, as an entry of the id table. An ingredient JSON object with `neoforge:ingredient_type` SHALL decode through the registered type. The built-in types `compound`, `components`, `difference`, `intersection`, `block_tag` and `custom_display` SHALL work as on NeoForge 26.3. Status: planned (wave 2).

#### Scenario: A mod ingredient
- **WHEN** a recipe uses an ingredient of a mod type that matches stacks with an energy level
- **THEN** only a stack with that energy level matches

#### Scenario: A compound ingredient
- **WHEN** a recipe uses a `neoforge:compound` ingredient of two children
- **THEN** a stack that matches either child matches the ingredient

### Requirement: Ingredient wire form
The flattened item list of a custom ingredient SHALL be its display form for every connection. The server SHALL send the NeoForge custom ingredient form only inside `neoforge:recipe_content`. Status: planned (wave 2).

#### Scenario: A recipe book entry
- **WHEN** a recipe book entry holds a custom ingredient
- **THEN** the client receives the list of the items that the ingredient matches

#### Scenario: Recipe content
- **WHEN** `neoforge:recipe_content` holds a recipe with a custom ingredient for a `Real` connection
- **THEN** the bytes equal the output of the NeoForge stream codec for that ingredient

### Requirement: Recipe sync
The server SHALL NOT send a mod recipe type by default. A mod SHALL list the mod recipe types to send at registration. A vanilla recipe type SHALL NOT be listable. The server SHALL send the listed recipes to `Real` connections with `neoforge:recipe_content` on join and after a reload. Status: planned (wave 2).

#### Scenario: A listed type
- **WHEN** a mod lists `examplemod:grinding` at registration and a `Real` connection joins
- **THEN** the client receives `neoforge:recipe_content` with the recipes of that type

#### Scenario: An unlisted type
- **WHEN** a mod does not list its type
- **THEN** no `neoforge:recipe_content` holds its recipes

#### Scenario: A vanilla type
- **WHEN** a mod lists `minecraft:crafting_shaped`
- **THEN** the registration fails with an error

### Requirement: Recipe registry rows
The freeze SHALL decide the sync rows of `recipe_type`, `recipe_serializer` and `neoforge:ingredient_serializer`. It SHALL decide them from the listed types and the registered serializers. A reload SHALL NOT change them. Status: planned (wave 2).

#### Scenario: Registry rows
- **WHEN** a mod lists a recipe type and registers its serializer
- **THEN** the sync sends the `recipe_type` and `recipe_serializer` rows with the vanilla names first

#### Scenario: A reload with a new serializer name
- **WHEN** a reload loads a recipe whose serializer no mod registered
- **THEN** the server logs and skips the recipe, and the rows stay the same

### Requirement: Policy for Display connections
A `Display` connection SHALL receive no mod recipe and no mod recipe registry id. Status: planned (wave 2).

#### Scenario: A reload with a Display connection
- **WHEN** a reload runs while a `Display` connection is online
- **THEN** that connection receives no recipe of a mod type

### Requirement: Recipe conditions
A recipe file SHALL pass the conditions of `neoforge-data-conditions` before it decodes. Status: planned (wave 2).

#### Scenario: A conditional mod recipe
- **WHEN** a mod recipe has a failing condition
- **THEN** the serializer of the mod does not run
