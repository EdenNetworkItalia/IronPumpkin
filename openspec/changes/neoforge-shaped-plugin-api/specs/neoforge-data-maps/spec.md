# Spec Delta

## Purpose

Lets mods declare data maps with the behaviour of NeoForge data maps, as the `data_maps` module of `ironpumpkin-mods`, attach values to registry entries through data packs, and read them back, with merging, removal and client sync.

## ADDED Requirements

### Requirement: Data map registration
`RegisterDataMapTypesEvent::register` SHALL register a `DataMapType` or an `AdvancedDataMapType` for one registry. It SHALL fail with `DataMapError::Duplicate` for an id that the registry has already, and with `DataMapError::UnsupportedRegistry` for a registry that cannot carry the map: a synced map on a world registry that is not synced or on a reloadable registry, as NeoForge refuses it, or a registry whose entries the server does not know. The host SHALL register the eleven types of `NeoForgeDataMaps` before the mods. Status: supported (task 2.17).

#### Scenario: Duplicate map
- **WHEN** two mods register `examplemod:ore_values` for `minecraft:block`
- **THEN** the second call fails with `DataMapError::Duplicate` and the server does not start

#### Scenario: Synced map on a reloadable registry
- **WHEN** a mod registers a synced map for `minecraft:loot_table`
- **THEN** the call fails with `DataMapError::UnsupportedRegistry`

### Requirement: Values from data packs
The host SHALL load `data/<namespace>/data_maps/<registry>/<map>.json` from every enabled data pack at startup and on `/reload`, in pack order, where `<registry>` is the registry path with its namespace as a prefix outside `minecraft`. A file with `replace` SHALL drop the values of lower packs. `DataMapType::get_data` SHALL return the value of an entry, or none when no file sets it. A registry with no data map file SHALL keep its values, as `DataMapLoader.apply` leaves it. Status: supported (task 2.17).

#### Scenario: Value from a file
- **WHEN** a data pack sets `{"value": 3}` for `minecraft:iron_ore` in a block data map with an integer value
- **THEN** `get_data` for that map and entry returns 3

### Requirement: Merging and removal
Values for one entry from several files SHALL combine with the merger of the map, unless the newer entry has `replace`, and an entry of `remove` SHALL drop the value, or, with a remover object of an advanced map, keep what the remover returns. A mod SHALL be able to give its own merger (`DataMapValueMerger`) and remover type (`DataMapValueRemover`). Status: supported (task 2.17).

#### Scenario: List merger
- **WHEN** two packs set `[1]` and `[2]` for an entry of a map with `ListMerger`
- **THEN** `get_data` returns `[1, 2]`

### Requirement: Tag entries
A value or a removal that targets a tag SHALL apply to every entry of the tag. When the map has a no-tags reason, the loader SHALL drop the tag targets of the values of a file and log that reason. Status: supported (task 2.17).

#### Scenario: Tag target
- **WHEN** a data file sets a value for `#c:ores`
- **THEN** every block in `c:ores` gets the value

### Requirement: Updated event
After each load the host SHALL fire the Pumpkin event `DataMapsUpdatedEvent` with cause `UpdateCause::ServerReload` for every registry that had a data map file. Status: supported (task 2.17).

#### Scenario: Reload
- **WHEN** an operator runs `/reload`
- **THEN** handlers of `DataMapsUpdatedEvent` run with cause `UpdateCause::ServerReload`

### Requirement: Client sync
The configuration phase SHALL send a NeoForge client the synced data maps of every registry with data maps (`neoforge:known_registry_data_maps`). A vanilla client SHALL get nothing, and SHALL be refused when a map is `mandatory_sync`, with the text of `RegistryDataMapNegotiation`. On join and after `/reload`, the host SHALL send each NeoForge client the values of the synced maps that it named in its reply, one `neoforge:registry_data_map_sync` per registry, in the NeoForge wire format. The NeoForge client compares the mandatory maps and refuses itself. Status: supported (task 2.17).

#### Scenario: Sync after reload
- **WHEN** an operator runs `/reload` while a NeoForge client that knows a synced map is in play
- **THEN** the client gets the new values of that map
