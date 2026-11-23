# Spec Delta

## Purpose

Lets a ported mod add entries to the static registries that Pumpkin generates as closed sets and that a mod of the reference modpack uses. It also covers the registry sync rows, item abilities, special plantables, the farmland water hook and mod-defined registries.

## ADDED Requirements

### Requirement: Registries with demand
The server SHALL accept mod entries in the id table registries `particle_type`, `point_of_interest_type`, `potion`, `custom_stat` and `map_decoration_type`. The recipe, menu and fluid registries have their own specs. Status: planned (wave 1).

#### Scenario: A listed registry
- **WHEN** a mod registers an entry in `potion` before the freeze
- **THEN** the registration succeeds

### Requirement: Decoder registries with demand
The server SHALL accept decoder entries in `trigger_type`, `loot_function_type`, `loot_pool_entry_type`, `loot_condition_type`, `data_component_predicate_type` and `entity_sub_predicate_type`. The world generation registries and the enchantment registries have their own requirements. Status: planned (wave 1).

#### Scenario: A listed decoder registry
- **WHEN** a mod registers a decoder in `loot_condition_type` before the freeze
- **THEN** the registration succeeds

### Requirement: Registries without demand
A mod uses a registry when it registers an entry into it, through the registry or through a helper such as `GameRules.register`. A registry that no mod of the reference modpack uses SHALL be `not supported` in the inventory. The reason SHALL be "no mod of the reference modpack uses it". Status: planned (wave 1).

#### Scenario: A registry without demand
- **WHEN** a reader looks up `consume_effect_type` in the inventory
- **THEN** the row says `not supported` with the reason "no mod of the reference modpack uses it"

#### Scenario: A helper registration
- **WHEN** a mod of the reference modpack registers a game rule through `GameRules.register`
- **THEN** the game rule row counts that mod as demand

### Requirement: Ids of id table entries
At the freeze, each mod entry of an id table registry SHALL get an id after the vanilla entries, in name order. A lookup by vanilla name or vanilla id SHALL return the vanilla entry. Each registry SHALL have a test of its vanilla count and names against the generated lookups. Status: planned (wave 1).

#### Scenario: A mod particle type
- **WHEN** a mod registers the particle type `examplemod:spark`
- **THEN** its id follows the last vanilla particle type, and every vanilla particle type keeps its id

#### Scenario: Registration after the freeze
- **WHEN** a mod registers a potion after the freeze
- **THEN** the call fails with the frozen-registry error and the registry does not change

#### Scenario: Duplicate name
- **WHEN** two mods register the custom stat `examplemod:crafted_gears`
- **THEN** the second call fails with the duplicate error

#### Scenario: Too many entries
- **WHEN** a registry gets more entries than its id range holds
- **THEN** the freeze fails with `RegistryError::RangeExceeded` that names the registry

### Requirement: Display entry at registration
A mod SHALL name the display entry of each particle type, potion and map decoration type at registration. A `Display` connection SHALL get that display entry where an encoder writes the id. Status: planned (wave 1).

#### Scenario: A named display entry
- **WHEN** a mod registers a potion with the display entry `minecraft:water`
- **THEN** a `Display` connection receives `minecraft:water` for that potion

### Requirement: Point of interest types
A mod SHALL register a point of interest type through the id table. The type SHALL work with `ExtendPoiTypesEvent` of M4. The POI storage SHALL keep the id of the type, and the POI save SHALL keep its name. The tags packet SHALL leave out mod point of interest ids on every connection. Status: planned (wave 1).

#### Scenario: A mod POI type
- **WHEN** a mod registers `examplemod:forge_station` and adds its block states in `ExtendPoiTypesEvent`
- **THEN** a placed block of that state registers a point of interest of that type, and a query by that type finds it

#### Scenario: A POI save
- **WHEN** the chunk saves and loads
- **THEN** the point of interest keeps its mod type

#### Scenario: No row
- **WHEN** a mod registers a point of interest type
- **THEN** the sync sends no `minecraft:point_of_interest_type` row

### Requirement: Sync rows for carried ids
The freeze SHALL build one sync row for each registry that NeoForge syncs and that a server packet carries mod ids of. The tags packet carries ids of `fluid` and `potion`. A registry without a mod entry SHALL get no row. A modpack without such entries SHALL send only the 7 base rows. Status: planned (wave 1).

#### Scenario: A row for a carried id
- **WHEN** a mod registers a particle type and a `Real` connection joins
- **THEN** the client receives a `minecraft:particle_type` row with the vanilla names first and `examplemod:spark` last

#### Scenario: A row for the tags packet
- **WHEN** a mod registers a potion
- **THEN** the sync sends a `minecraft:potion` row

#### Scenario: No packet carries the id
- **WHEN** a mod registers a decoder entry
- **THEN** the sync sends no row for its registry

### Requirement: Rows list every mod
A row SHALL list the entries of every mod that registers in that registry. A NeoForge client needs the client half of every mod with synced content. Status: planned (wave 1).

#### Scenario: A mod without a required client half
- **WHEN** a mod whose `client_required()` returns false registers a particle type
- **THEN** the row lists that particle type

### Requirement: One id function
A sync row and the packet encoders SHALL use the same id function. A row SHALL list the vanilla names, or the NeoForge built-in names for a registry that NeoForge owns, then the mod names in name order. The built-in names SHALL come from the generated lookups for a vanilla registry and from a NeoForge 26.3 capture for a NeoForge registry. Status: planned (wave 1).

#### Scenario: Ids match the packets
- **WHEN** a test decodes a particle packet of a mod particle with the ids of the sync row
- **THEN** the id resolves to the same particle

#### Scenario: Built-in names
- **WHEN** a test compares a row with a capture from a NeoForge 26.3 dedicated server
- **THEN** the built-in names have the order of the capture

### Requirement: Policy of id table registries
For a `Display` connection, the encoders SHALL map a mod potion or map decoration type to its display entry. They SHALL map a mod particle type to an option-less vanilla particle. They SHALL leave a mod custom stat out of the statistics packet. They SHALL leave mod ids out of the tags packet. Status: planned (wave 1).

#### Scenario: A particle on a Display connection
- **WHEN** the server sends a mod particle to a `Display` connection
- **THEN** the packet holds an option-less vanilla particle

#### Scenario: A custom stat on a Display connection
- **WHEN** a player earns a mod custom stat on a `Display` connection
- **THEN** the statistics packet omits the stat

#### Scenario: A tag on a Display connection
- **WHEN** a tag holds a mod potion and the connection is `Display`
- **THEN** the tags packet omits the mod entry

### Requirement: Decoder registries
A mod SHALL register a decoder in a decoder registry before the freeze. A decoder maps a name to a function that decodes the JSON of a data pack file into a behaviour of the mod. The registry SHALL reject a duplicate name. The registry SHALL reject the `minecraft` namespace. It SHALL close at the freeze. Status: planned (wave 1).

#### Scenario: The minecraft namespace
- **WHEN** a mod registers the loot function type `minecraft:scale`
- **THEN** the call fails with a namespace error

#### Scenario: After the freeze
- **WHEN** a mod registers a trigger type after the freeze
- **THEN** the call fails with the frozen-registry error

#### Scenario: No id and no row
- **WHEN** a mod registers a decoder
- **THEN** the decoder gets no numeric id and the sync sends no row for its registry

### Requirement: Use sites call the decoded behaviour
A data pack file that names a mod decoder entry as its type SHALL use the behaviour of the mod where vanilla uses an entry of that registry. A decode error SHALL skip the file with a log line. A file that names an unregistered type SHALL be skipped with a log line. Status: planned (wave 2).

#### Scenario: A mod loot function
- **WHEN** a loot table names the mod loot function type `examplemod:scale_by_depth`
- **THEN** a drop from that table runs the function of the mod

#### Scenario: A mod advancement trigger
- **WHEN** a mod fires its trigger type for a player
- **THEN** the advancements of that player that name the trigger type evaluate their criteria

#### Scenario: An unregistered type
- **WHEN** a loot table names `othermod:unknown_function` and no mod registers it
- **THEN** the server logs the table id and the type, skips that table, and loads the other tables

### Requirement: Decoder registries are server-only
A `Display` connection and a `Real` connection SHALL receive no data of a decoder entry. Status: planned (wave 1).

#### Scenario: A client joins
- **WHEN** a client joins a server whose mods register decoder entries
- **THEN** the join succeeds and no packet holds a decoder name

### Requirement: Enchantment effect registries
A data pack enchantment SHALL run its effects. A mod SHALL register a decoder in `enchantment_effect_component_type` and `enchantment_entity_effect_type`. The decode path of a mod entry SHALL sit next to the vanilla decode path. Status: planned (wave 6).

#### Scenario: A data pack enchantment
- **WHEN** a data pack enchantment names a vanilla effect
- **THEN** the effect runs

#### Scenario: A mod enchantment effect
- **WHEN** a data pack enchantment names a mod effect component type
- **THEN** the enchantment loads and the effect runs

### Requirement: Holder set types
A mod SHALL register a custom holder set type in a decoder registry next to the tag reader. The server SHALL write a custom holder set as the direct list of its members. The server SHALL send no row for `neoforge:holder_set_type`. Status: planned (wave 2).

#### Scenario: A custom holder set on the wire
- **WHEN** the server sends a holder set of a mod holder set type
- **THEN** the packet holds the list of its members

### Requirement: Saves keep mod entries by name
A save SHALL record a mod entry of an id table registry by its name. A world that holds an entry of an absent mod SHALL load. The next save SHALL write the unknown data back unchanged. A POI record of an unknown type SHALL stay as a raw record that no query matches. The stats file SHALL keep unknown stat keys. Status: planned (wave 1).

#### Scenario: A removed mod
- **WHEN** a world saved with a stat of `examplemod` loads in a modpack without `examplemod`
- **THEN** the world loads and the next save holds the stat under its name

#### Scenario: An unknown POI type
- **WHEN** a chunk holds a POI record of a type of an absent mod
- **THEN** no POI query returns the record and the next save writes it back

### Requirement: Item abilities
The server SHALL answer `can_perform_action(stack, ability)` for each vanilla ability with its NeoForge default. A mod SHALL answer for its own item with a fixed answer or an answer that depends on the stack. Each vanilla site that tests an ability SHALL use this function. Sheep and snow golems SHALL keep their own shears check, as NeoForge 26.3 does. Status: planned (wave 1).

#### Scenario: A mod item shears
- **WHEN** a mod answers `shears_harvest` for its item and a player uses it on a beehive
- **THEN** the beehive gives honeycomb as it does for shears

#### Scenario: A stack-dependent answer
- **WHEN** a mod answers `shears_dig` with true only for a stack with durability left, and the stack breaks
- **THEN** the next use of the stack does not count as shears

#### Scenario: A vanilla item
- **WHEN** a player uses vanilla shears on a tripwire
- **THEN** the tripwire is disarmed

### Requirement: Special plantables
A mod SHALL register a special plantable for its item. The plantable SHALL answer whether a plant can be placed at a position. The plantable SHALL place it. The plantable SHALL say whether a villager may plant the item. The villager farming code and the villager pickup check SHALL call it, with the `neoforge:villager_farmlands` tag of M4. Status: planned (wave 1).

#### Scenario: A villager plants a mod crop
- **WHEN** a mod item has a special plantable that allows villagers and a farmer holds the item next to tilled farmland
- **THEN** the farmer plants the crop at a position that the plantable accepts

#### Scenario: A plantable refuses the position
- **WHEN** the plantable answers false for a position
- **THEN** the farmer places nothing there

#### Scenario: The pickup check
- **WHEN** a villager sees a mod item whose plantable allows villagers
- **THEN** the villager picks it up

### Requirement: Farmland water tickets
A mod SHALL add a water ticket for a box or a position with an owner. Farmland inside a ticket SHALL count as watered. A ticket SHALL leave when its chunk unloads. A world with no tickets SHALL pay one empty check per farmland random tick. Status: planned (wave 1).

#### Scenario: A ticket waters farmland
- **WHEN** a mod adds a water ticket over dry farmland that has no water nearby
- **THEN** the farmland does not dry out on its random tick

#### Scenario: A chunk unloads
- **WHEN** the chunk of a ticket unloads
- **THEN** the ticket is gone

### Requirement: Minecart collision handler
The inventory SHALL mark `IMinecartCollisionHandler` `not supported`. The reason SHALL be "no mod of the reference modpack uses it". Status: planned (wave 1).

#### Scenario: The inventory row
- **WHEN** a reader looks up `IMinecartCollisionHandler` in the inventory
- **THEN** the row says `not supported` with that reason

### Requirement: Mod-defined registries
A mod SHALL define its own registry, as `NewRegistryEvent`, `RegistryBuilder` and `DeferredRegister#makeRegistry` do in NeoForge. The registry SHALL accept entries from the owner mod and from other mods before the freeze. It SHALL freeze with the other mod registries. Entries SHALL get ids in name order. The registry SHALL exist on the server only. Status: planned (wave 2).

#### Scenario: An addon registers into a base mod registry
- **WHEN** a base mod defines `basemod:machine_types` and an addon mod registers `addon:press` into it
- **THEN** after the freeze a lookup by name returns the entry of the addon

#### Scenario: A late entry
- **WHEN** a mod registers into a mod-defined registry after the freeze
- **THEN** the call fails with the frozen-registry error

#### Scenario: No sync row
- **WHEN** a `Real` connection joins a server with a mod-defined registry
- **THEN** the server sends no sync row for it

### Requirement: Synced mod-defined registries are refused
A registration with `sync(true)` SHALL fail at the freeze with a "not supported" error. The error SHALL name the mod and the registry. Status: planned (wave 2).

#### Scenario: A synced registration
- **WHEN** a mod defines a registry with `sync(true)`
- **THEN** the freeze fails with the "not supported" error

### Requirement: Registry callbacks
`ModifyRegistriesEvent` and the registry callbacks SHALL attach to the mod tables. An add callback SHALL run once per entry at registration. A bake callback SHALL run once after the freeze. A clear callback SHALL NOT run. Status: planned (wave 2).

#### Scenario: Add and bake
- **WHEN** a mod registers a registry with an add callback and a bake callback and adds 3 entries
- **THEN** the add callback runs 3 times and the bake callback runs once after the freeze
