# content-registry Specification

## Purpose
Defines the content registry core in `crates/pumpkin-data/src/dynamic.rs`: how custom blocks, items and entity types are registered, how the freeze allocates their ids, and how the generated lookups answer for them. The startup content phase, the content manifest, placeholders and the block state space are separate capabilities. Until the block state space lands, a custom block has exactly one state.

## Requirements

### Requirement: Namespaced names
`register_block`, `register_item` and `register_entity_type` SHALL accept only a `namespace:path` name with the characters vanilla allows in a resource location. They SHALL reject a name without a namespace, with an empty namespace or in the `minecraft` namespace with `ReservedNamespace`, and any other malformed name with `InvalidName`. Status: implemented (#17).

#### Scenario: Reserved namespace
- **WHEN** a block is registered as `minecraft:lamp`, `lamp` or `:lamp`
- **THEN** the call returns `ReservedNamespace` and nothing is registered

#### Scenario: Malformed name
- **WHEN** an entity type is registered as `MyMod:lamp` or `mymod:`
- **THEN** the call returns `InvalidName`

### Requirement: Unique names per registry
A name SHALL be registered at most once per registry. A second registration of the same name in the same registry SHALL return `Duplicate`. The same name MAY exist once in each of the block, item and entity type registries. Status: implemented (#17).

#### Scenario: Duplicate block
- **WHEN** `mymod:lamp` is registered as a block twice
- **THEN** the second call returns `Duplicate` and the first registration is kept

#### Scenario: Block and item with one name
- **WHEN** `mymod:lamp` is registered as a block and as an item
- **THEN** both calls succeed

### Requirement: Vanilla display entries
Each definition SHALL name a generated (vanilla) display entry: a block state, an item or an entity type. A custom entry SHALL copy the data of its display entry, except its id and name. A custom block state SHALL have no block entity type. A definition with a custom display entry SHALL return `DisplayNotVanilla`. Status: implemented (#17).

#### Scenario: Copied block data
- **WHEN** a block registered with the stone default state as display is frozen
- **THEN** its state has the hardness, shapes and flags of stone, and `block_entity_type` is `u16::MAX`

### Requirement: Block items
An item definition MAY name a custom block it places. At the freeze, the block's `item_id` SHALL become that item's id, and a block without an item SHALL keep item id 0 (air), as generated blocks do. An item that names an unregistered block SHALL fail the freeze with `UnknownBlock`, and a second item for the same block SHALL fail it with `BlockAlreadyHasItem`. Status: implemented (#17).

#### Scenario: Linked block and item
- **WHEN** item `mymod:lamp` places block `mymod:lamp` and the registry freezes
- **THEN** `Block::from_item_id` of the item id returns the block, and the block's `item_id` is the item id

### Requirement: Name-ordered id allocation
The freeze SHALL allocate ids per registry in lexicographic order of the namespaced name, as one dense range that starts at the generated count: `BlockId::COUNT`, `BlockStateId::COUNT`, `Item::COUNT` and `EntityType::COUNT`. Registration order SHALL NOT change any id. Each block SHALL receive a contiguous state range that follows the previous block's last state. Status: implemented (#17).

#### Scenario: Order independence
- **WHEN** one registry receives `b:middle`, `a:first`, `c:last` and another receives `c:last`, `a:first`, `b:middle`
- **THEN** both freezes give `a:first` the first id of each range, `b:middle` the second and `c:last` the third

### Requirement: Id range limit
No id above 65,534 SHALL be allocated, because `u16::MAX` is the "none" value of u16 ids. Content that does not fit SHALL fail the freeze with `RangeExceeded`, which names the registry and the count. Status: implemented (#17).

#### Scenario: Range exceeded
- **WHEN** more custom blocks are registered than state ids remain below 65,535
- **THEN** the freeze returns `RangeExceeded` for `block state`

### Requirement: One freeze
`freeze` SHALL run once. It SHALL build every custom block, block state, item and entity type, leak them as `'static` data and install the tables in a `OnceLock`. After the freeze, every registration and every further `freeze` call SHALL return `RegistryFrozen`, and the installed tables SHALL NOT change. A failed freeze SHALL be terminal: it installs no tables, and later calls return `RegistryFrozen`. Status: implemented (#17).

#### Scenario: Failed freeze
- **WHEN** `freeze` fails with `UnknownBlock`
- **THEN** no custom lookup answers, and later registrations and `freeze` calls return `RegistryFrozen`

#### Scenario: Registration after the freeze
- **WHEN** a plugin registers `test:late` as a block, an item or an entity type after `freeze`
- **THEN** the call returns `RegistryFrozen` and `Block::from_name("test:late")` returns `None`

### Requirement: Custom-aware id constructors
`BlockStateId::new` and `BlockId::new` SHALL return `Some` for a generated id and for an installed custom id, and `None` above them; `new_or_air` SHALL return air above them. `new_vanilla` SHALL accept generated ids only and stay `const`; the generated constants and `get_potted_item` use it. Status: implemented (#17).

#### Scenario: Unknown custom id
- **WHEN** two custom blocks are installed
- **THEN** `BlockStateId::new(BlockStateId::COUNT + 2)` returns `None` and `BlockStateId::new_or_air` of the same value returns air

### Requirement: Lookup fallthrough
The block, block state, item and entity type lookups in `pumpkin-data` SHALL keep their signatures and try the generated tables first. For an id at or above the generated count, or a name the generated tables do not know, they SHALL answer from the installed tables through a `#[cold]` `#[inline(never)]` function that takes no lock. Status: implemented (#17).

#### Scenario: Custom block by id, state and name
- **WHEN** `test:alpha` is the first custom block and the registry is frozen
- **THEN** `Block::from_id(BlockId::new(BlockId::COUNT))`, `Block::from_state_id`, `Block::from_registry_key("test:alpha")` and `Block::from_name("test:alpha")` return it

#### Scenario: Custom item and entity type
- **WHEN** one custom item and one custom entity type are installed
- **THEN** `Item::from_id(Item::COUNT)` and `EntityType::from_raw(EntityType::COUNT)` return them, and `Item::from_id(Item::COUNT + 1)` returns `None`

#### Scenario: Vanilla lookups unchanged
- **WHEN** the registry is frozen with custom content
- **THEN** `Block::from_name("minecraft:stone")`, `Item::from_registry_key("minecraft:stone")` and `EntityType::from_name("minecraft:zombie")` return the generated entries

### Requirement: Single-state custom blocks
Until the block state space lands, `Block::properties` SHALL return `None` for a custom block, and `BlockState::to_be_network_id` SHALL return the Bedrock id of the display state. Status: implemented (#17).

#### Scenario: Bedrock id
- **WHEN** a custom block has the stone default state as display
- **THEN** `BlockState::to_be_network_id` of its state equals that of stone

### Requirement: Generated tag lists hold generated ids only
`BlockId::has_tag`, `Taggable::has_tag` and `Taggable::is_tagged_with` SHALL keep their generated code and SHALL return false for a custom id. These are the worldgen hot path: a custom-id branch in `BlockId::has_tag` measured +5% on the `noise_generation` bench. Status: implemented (#17).

#### Scenario: Custom block on the hot path
- **WHEN** a custom block has the stone default state as display
- **THEN** `has_tag` for `minecraft:base_stone_overworld` returns false for it

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

### Requirement: Tag queries off the hot path
`DynamicTaggable::has_tag_dynamic` and `dynamic::tag_ids` SHALL read both tables. Recipe ingredients, recipe book ingredient sets, command tag predicates, tool rules, repair items and equippable entity tags SHALL use them. Worldgen SHALL NOT use them. Status: implemented (#45).

#### Scenario: Generated entry
- **WHEN** `has_tag_dynamic` runs for stone and `minecraft:base_stone_overworld`
- **THEN** it returns true, as `is_tagged_with` does

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
