# Spec Delta

## Purpose

Lets mods declare NeoForge data maps, attach JSON values to registry entries through data packs, and read them back, with merging, removal and client sync.

## ADDED Requirements

### Requirement: Data map registration
`register-data-map-types-event.register` SHALL register a data map for one registry, and SHALL fail with `duplicate` for an existing id and with `unsupported-registry` for a registry that cannot carry data maps. Status: planned (task 2.17).

#### Scenario: Duplicate map
- **WHEN** two mods register `examplemod:ore_values`
- **THEN** the second call fails with `data-map-error::duplicate`

### Requirement: Values from data packs
The host SHALL load `data/<namespace>/data_maps/<registry>/<map>.json` from every enabled data pack on each reload, and `get-data` SHALL return the JSON value for an entry or none when no file sets it. Status: planned (task 2.17).

#### Scenario: Value from a file
- **WHEN** a data pack sets `{"value": 3}` for `minecraft:iron_ore` in a block data map
- **THEN** `get-data` for that map and entry returns that JSON value

### Requirement: Merging and removal
Values for one entry from several packs SHALL combine with the map's `value-merger`, and a removal entry SHALL apply the map's remover, through the `merge-data-map-value` and `remove-data-map-value` callbacks for custom ones. Status: planned (task 2.17).

#### Scenario: List merger
- **WHEN** two packs set `[1]` and `[2]` for an entry of a map with the `list` merger
- **THEN** `get-data` returns `[1, 2]`

### Requirement: Tag entries
An entry that targets a tag SHALL apply to every member of the tag, unless the map has `no-tags-reason`, in which case the loader SHALL reject it with that reason. Status: planned (task 2.17).

#### Scenario: Tag target
- **WHEN** a data file sets a value for `#c:ores`
- **THEN** every block in `c:ores` gets the value

### Requirement: Updated event
After each load the host SHALL post `DataMapsUpdatedEvent` with cause `server-reload` for every registry that has data maps. Status: planned (task 2.17).

#### Scenario: Reload
- **WHEN** an operator runs `/reload`
- **THEN** listeners of `data-maps-updated-event` run with cause `server-reload`

### Requirement: Backing phase
Until phase 6 adds the loader, registration SHALL succeed and lookups SHALL return none. Status: planned (task 2.17).

#### Scenario: Before phase 6
- **WHEN** a mod calls `get-data` on a host without the data map loader
- **THEN** the call returns none
