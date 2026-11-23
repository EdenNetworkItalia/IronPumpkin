# Spec Delta

## Purpose

Lets a ported mod add world generation: configured and placed features as NeoForge JSON, mod feature types and placement modifier types, and NeoForge biome modifiers. Carvers, mod biomes and mod structures follow in a last design pass.

## ADDED Requirements

### Requirement: Vanilla placement modifiers
World generation SHALL support the vanilla placement modifiers `cuboid`, `random_chance` and `randomly_selected`. Status: M4 wave 10 (#189).

#### Scenario: A vanilla placement
- **WHEN** a placed feature uses one of these placement modifiers
- **THEN** the feature places as the decompiled vanilla 26.3 code does

### Requirement: Startup load
The server SHALL read the configured features, the placed features and the biome modifiers of the enabled data packs once, before the first world loads. A data pack reload SHALL skip the mod features and the biome modifiers. Status: planned (wave 4).

#### Scenario: A startup load
- **WHEN** a mod data pack has a placed feature and the server starts
- **THEN** the feature is known before the first world loads

#### Scenario: A reload
- **WHEN** an operator reloads data packs after a placed feature file changed
- **THEN** world generation keeps the features of the startup load

### Requirement: JSON is the registration contract
A mod SHALL register configured and placed features as the NeoForge JSON, in mod data packs or as JSON values in code. The public API of `ironpumpkin-mods` SHALL NOT expose the feature structs of `pumpkin-world`. Mod features SHALL have their full names in a table beside the vanilla names. A biome feature list SHALL accept vanilla and mod placed features. Status: planned (wave 4).

#### Scenario: A mod ore
- **WHEN** a mod registers an `ore` configured feature and a placed feature as JSON, and a biome modifier adds it to the overworld
- **THEN** chunk generation places the ore in new chunks

#### Scenario: A vanilla feature
- **WHEN** a chunk generates in a modpack with mod features
- **THEN** the vanilla features place as in a modpack without mod features

### Requirement: Runtime JSON reader
The server SHALL read configured and placed features from JSON at run time. The supported feature types are `ore`, `scattered_ore`, `tree`, `random_patch`, `simple_block`, `random_selector` and `weighted_random_selector`, with their placements. A file SHALL pass the conditions of `neoforge-data-conditions`. An invalid file SHALL be skipped with a log line that names its id. Status: planned (wave 4).

#### Scenario: A data pack ore
- **WHEN** a mod data pack has an `ore` configured feature and a placed feature in JSON
- **THEN** the feature exists after the startup load and generates like a vanilla one

#### Scenario: An invalid file
- **WHEN** a placed feature file names a placement modifier that does not exist
- **THEN** the server logs the id and the cause, and loads the other files

### Requirement: Differential test
A test SHALL parse every vanilla configured and placed feature of the supported types from `assets/datapack` at run time. The test SHALL compare each result with the generated value. A difference SHALL fail the test. Status: planned (wave 4).

#### Scenario: A reader that drifts
- **WHEN** the generated value of a supported feature differs from the runtime reader result
- **THEN** the differential test fails

### Requirement: Mod types with a trait object
A mod SHALL register a type in each of these registries: `feature_type`, `placement_modifier_type`, `block_state_provider_type`, `trunk_placer_type`, `foliage_placer_type`, `tree_decorator_type`, `rule_test`, `height_provider_type` and `int_provider_type`. The server SHALL call the behaviour of the mod where vanilla uses an entry of that registry. The behaviour SHALL see the world through an object-safe facade. Status: planned (wave 4).

#### Scenario: A mod feature type
- **WHEN** a mod registers the feature type `examplemod:crystal_cluster` and a biome modifier adds a placed feature of that type
- **THEN** chunk generation calls the behaviour of the mod at each position

#### Scenario: A mod placement modifier
- **WHEN** a placed feature uses a mod placement modifier that filters by depth
- **THEN** the feature places only at the positions that the modifier lets pass

#### Scenario: A mod height provider
- **WHEN** a placement names a mod height provider type
- **THEN** the runtime reader decodes it and the placement uses its heights

### Requirement: Biome modifiers
The server SHALL apply the NeoForge biome modifiers `add_features`, `remove_features`, `add_spawns` and `remove_spawns` as an overlay per biome. The modifiers come from data packs and from mods. The overlay SHALL freeze before the first chunk generates. The vanilla biome data SHALL stay the same. A modifier SHALL pass the conditions of `neoforge-data-conditions`. Status: planned (wave 4).

#### Scenario: Add a spawn
- **WHEN** a biome modifier adds a spawn for a mod entity type to `minecraft:plains`
- **THEN** the mob spawner can spawn that entity in plains

#### Scenario: Remove a feature
- **WHEN** a biome modifier removes a vanilla placed feature from a biome
- **THEN** chunks that generate afterwards do not place that feature in that biome

#### Scenario: A late modifier
- **WHEN** a modifier file loads after the first chunk generates
- **THEN** it does not apply, and the server logs it

### Requirement: Structure template palette keeps mod properties
A structure template SHALL keep the block properties of a mod block when it parses and when it saves. A template block of a mod SHALL place with its state. Status: planned (wave 1).

#### Scenario: A mod block in a template
- **WHEN** a template palette holds a mod block with a property that has no vanilla property kind
- **THEN** placing the template gives the block that state

#### Scenario: Re-save
- **WHEN** the template saves again
- **THEN** the property is still in the palette

### Requirement: The last design pass
One design pass SHALL cover `add_carvers`, `carver_type`, `chunk_generator`, `biome_source` and `density_function_type`. The same pass SHALL cover mod biomes and mod structures. It SHALL decide between the engine change of option S1 and the source patch route. It SHALL decide the biome id width, the structure keys and the structure save keys. Status: planned (wave 6).

#### Scenario: The decision
- **WHEN** the design pass ends
- **THEN** design.md records the route and the reason, and each row states `supported` with a test or `not supported` with the reason

#### Scenario: Unknown structure data
- **WHEN** a world holds a structure start of an absent mod
- **THEN** the world loads and the next save keeps the start

### Requirement: Server-only content and cost
World generation content SHALL be server-only. A `Display` connection and a `Real` connection SHALL receive no data of a mod feature. A modpack with no mod world generation content SHALL pay one check of the overlay tables in chunk generation. Status: planned (wave 4).

#### Scenario: A client joins
- **WHEN** a client joins a modpack with mod features
- **THEN** the join succeeds and no packet holds a mod feature name

#### Scenario: The performance guardrail
- **WHEN** `cargo bench -p pumpkin-world` runs with no mod world generation content
- **THEN** `noise_generation` and `full_chunk_generation` show no significant regression
