## MODIFIED Requirements

### Requirement: Custom tag table
At the freeze the registry SHALL build a second table with the custom ids of each generated block, item and entity type tag and of each mod tag. A custom entry SHALL join every generated tag of its display entry and every tag its definition lists. A listed `minecraft` tag that the registry does not have SHALL fail the registration with `UnknownTag`; a listed mod tag that no mod registers SHALL fail the freeze with `UnknownTag`. Status: implemented (#45, #69).

#### Scenario: Tag inherited from the display entry
- **WHEN** a custom block has the stone default state as display
- **THEN** `has_tag_dynamic` for `minecraft:base_stone_overworld` returns true for it

#### Scenario: Explicit tag
- **WHEN** a custom block with the dirt default state as display lists `minecraft:mineable/pickaxe` in its tags
- **THEN** `has_tag_dynamic` for `minecraft:mineable/pickaxe` and for `minecraft:dirt` returns true for it, and `dynamic::tag_ids` for `minecraft:mineable/pickaxe` lists its id among the custom ids

#### Scenario: Explicit mod tag
- **WHEN** a custom block lists `test:lamps` before a mod registers the block tag `test:lamps`
- **THEN** the freeze succeeds and `has_tag_dynamic` for `#test:lamps` returns true for the block

#### Scenario: Unknown tag
- **WHEN** a custom item lists `minecraft:no_such_tag`, a name that no generated item tag has
- **THEN** the registration fails with `RegistryError::UnknownTag` and the item is not registered

#### Scenario: Unknown mod tag
- **WHEN** a custom block lists `test:missing` and no mod registers that block tag
- **THEN** the freeze fails with `RegistryError::UnknownTag` and installs no tables

## ADDED Requirements

### Requirement: Mod tag names
`register_tag` SHALL register a block, item or entity type tag that a mod names. The name SHALL be a `namespace:path` outside the `minecraft` namespace, or the call returns `InvalidTagName`, and unique in its registry, where a generated tag of the registry counts as registered, or the call returns `DuplicateTag`. The same name MAY exist once in each registry. After the freeze the call SHALL return `RegistryFrozen`. Status: implemented (#69).

#### Scenario: Reserved tag name
- **WHEN** a mod registers the block tag `minecraft:ores` or `ores`
- **THEN** the call returns `InvalidTagName` and no tag is registered

#### Scenario: Duplicate tag name
- **WHEN** a mod registers the item tag `test:ores` twice and the block tag `test:ores` once
- **THEN** the second item tag call returns `DuplicateTag` and the block tag call succeeds

#### Scenario: Generated tag name
- **WHEN** a mod registers the block tag `c:ores`, which the generated block tags have
- **THEN** the call returns `DuplicateTag`

### Requirement: Mod tag members
A mod tag SHALL hold the entries its definition lists and the custom entries that list it, and no entry through its display entry. A listed generated entry SHALL exist at registration and a listed custom entry at the freeze, or the call fails with `UnknownTagMember`. `has_tag_dynamic` and `dynamic::tag_ids` SHALL answer for mod tags. Generated tag lists, the datapack tag loader and the tags sent to clients SHALL NOT change. Status: implemented (#69).

#### Scenario: Vanilla and custom members
- **WHEN** the block tag `test:lamps` lists `minecraft:redstone_lamp` and the custom block `test:alpha`
- **THEN** `dynamic::tag_ids` for `test:lamps` returns the redstone lamp id as generated member and the id of `test:alpha` as custom member

#### Scenario: Display entry in a mod tag
- **WHEN** a custom block has the redstone lamp as display and does not list `test:lamps`
- **THEN** `has_tag_dynamic` for `test:lamps` returns false for it

#### Scenario: Unknown member
- **WHEN** the block tag `test:ores` lists `minecraft:no_such_block`
- **THEN** the registration returns `UnknownTagMember`

#### Scenario: Generated tag lists unchanged
- **WHEN** the redstone lamp is a member of the mod tag `test:lamps`
- **THEN** `is_tagged_with("test:lamps")` returns `None` for it
