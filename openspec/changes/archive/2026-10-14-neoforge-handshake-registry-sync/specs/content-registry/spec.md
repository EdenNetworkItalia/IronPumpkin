# Spec Delta

## ADDED Requirements

### Requirement: Frozen tables accessor
`dynamic::tables()` SHALL return the frozen content tables after a successful freeze and `None` before the freeze or after a failed freeze. Every call SHALL return the same `&'static` tables that `freeze` installed. Status: implemented (#88).

#### Scenario: Before the freeze
- **WHEN** `tables()` runs before `freeze`
- **THEN** it returns `None`

#### Scenario: After the freeze
- **WHEN** `freeze` succeeds and `tables()` runs twice
- **THEN** both calls return the tables that `freeze` returned

### Requirement: Registry snapshots
The frozen tables SHALL give, for the block, item and entity type registries, the list of every id with its namespaced name: the generated entries from id 0 to the generated count minus one with `minecraft:` names, then the custom entries in id order. Placeholder entries SHALL be left out, so the list MAY have gaps at placeholder ids. Status: implemented (#88).

#### Scenario: Custom block snapshot
- **WHEN** `test:alpha` and `test:beta` are registered as blocks and the registry freezes
- **THEN** the block snapshot has `BlockId::COUNT + 2` entries, entry 0 is `minecraft:air`, entry `BlockId::COUNT` is `test:alpha` and the last entry is `test:beta`

#### Scenario: Placeholder left out of the snapshot
- **WHEN** a placeholder item `gone:widget` is registered and the registry freezes
- **THEN** the item snapshot has no entry for the placeholder's id, and the ids before and after it are present

#### Scenario: No custom content
- **WHEN** the registry freezes with no custom content
- **THEN** the entity type snapshot has `EntityType::COUNT` entries and every name has the `minecraft` namespace
