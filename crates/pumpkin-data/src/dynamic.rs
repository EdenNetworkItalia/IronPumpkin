//! Blocks, items and entity types that are not in the generated tables.
//!
//! Content is registered before the first world loads. [`freeze`] then allocates ids after the
//! generated counts, in name order so that plugin load order does not change them, and installs
//! immutable tables. The generated lookups fall through to these tables for ids at or above the
//! generated counts and for names the generated maps do not know. After the freeze the tables
//! never change, so readers take no lock.
//!
//! A custom block has a single state until block state properties are supported. Each custom
//! entry copies the data of its display entry, the vanilla entry that vanilla clients see.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, btree_map::Entry};
use std::fmt;
use std::sync::{Mutex, OnceLock, PoisonError};

use pumpkin_util::identifier::{Identifier, VANILLA_NAMESPACE};

#[cfg(feature = "entity_type")]
use crate::entity_type::EntityType;
#[cfg(feature = "item")]
use crate::item::Item;
use crate::{Block, BlockId, BlockState, BlockStateId};

/// The highest id the registry allocates. `u16::MAX` is the "none" value of u16 ids.
const MAX_ID: u16 = u16::MAX - 1;

/// A custom block with a single state.
#[derive(Debug, Clone)]
pub struct BlockDefinition {
    /// Namespaced name, for example `mymod:copper_lamp`.
    pub name: String,
    /// Vanilla state that vanilla clients see. The block copies its physical data.
    pub display: BlockStateId,
}

/// A custom item.
#[cfg(feature = "item")]
#[derive(Debug, Clone)]
pub struct ItemDefinition {
    /// Namespaced name, for example `mymod:copper_lamp`.
    pub name: String,
    /// Vanilla item that vanilla clients see. The item copies its components.
    pub display: &'static Item,
    /// Namespaced name of the custom block this item places.
    pub block: Option<String>,
}

/// A custom entity type.
#[cfg(feature = "entity_type")]
#[derive(Debug, Clone)]
pub struct EntityTypeDefinition {
    /// Namespaced name, for example `mymod:golem`.
    pub name: String,
    /// Vanilla entity type that vanilla clients see. The type copies its data.
    pub display: &'static EntityType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentKind {
    Block,
    Item,
    EntityType,
}

impl fmt::Display for ContentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Block => "block",
            Self::Item => "item",
            Self::EntityType => "entity type",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    /// The registry is frozen and accepts no new content.
    RegistryFrozen,
    /// The name is not a valid `namespace:path` resource location.
    InvalidName { kind: ContentKind, name: String },
    /// The name has no namespace or uses the `minecraft` namespace.
    ReservedNamespace { kind: ContentKind, name: String },
    /// The name is already registered for this kind.
    Duplicate { kind: ContentKind, name: String },
    /// The display entry is not a generated (vanilla) entry.
    DisplayNotVanilla { kind: ContentKind, name: String },
    /// The item places a block that is not registered.
    UnknownBlock { item: String, block: String },
    /// Another item already places this block.
    BlockAlreadyHasItem { item: String, block: String },
    /// The content does not fit in the free id range.
    RangeExceeded {
        registry: &'static str,
        count: usize,
    },
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RegistryFrozen => f.write_str("the content registry is frozen"),
            Self::InvalidName { kind, name } => write!(
                f,
                "invalid {kind} name \"{name}\": expected namespace:path with a-z 0-9 _ - . and / in the path"
            ),
            Self::ReservedNamespace { kind, name } => {
                write!(f, "{kind} \"{name}\" must use its own namespace")
            }
            Self::Duplicate { kind, name } => write!(f, "{kind} \"{name}\" is already registered"),
            Self::DisplayNotVanilla { kind, name } => {
                write!(f, "{kind} \"{name}\" needs a vanilla display entry")
            }
            Self::UnknownBlock { item, block } => {
                write!(f, "item \"{item}\" places unknown block \"{block}\"")
            }
            Self::BlockAlreadyHasItem { item, block } => {
                write!(
                    f,
                    "item \"{item}\" places block \"{block}\", which already has an item"
                )
            }
            Self::RangeExceeded { registry, count } => {
                write!(
                    f,
                    "{count} custom {registry} entries do not fit in the id range"
                )
            }
        }
    }
}

impl std::error::Error for RegistryError {}

/// The frozen custom content. Ids are the generated count plus the index in each table.
#[derive(Debug)]
pub struct ContentTables {
    blocks: Vec<Block>,
    block_names: HashMap<&'static str, usize>,
    states: &'static [BlockState],
    state_blocks: Vec<BlockId>,
    state_displays: Vec<BlockStateId>,
    #[cfg(feature = "item")]
    items: Vec<Item>,
    #[cfg(feature = "item")]
    item_blocks: Vec<Option<BlockId>>,
    #[cfg(feature = "item")]
    item_names: HashMap<&'static str, usize>,
    #[cfg(feature = "entity_type")]
    entity_types: Vec<EntityType>,
    #[cfg(feature = "entity_type")]
    entity_type_names: HashMap<&'static str, usize>,
}

impl ContentTables {
    /// Custom blocks in id order.
    #[must_use]
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// Custom items in id order.
    #[cfg(feature = "item")]
    #[must_use]
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// Custom entity types in id order.
    #[cfg(feature = "entity_type")]
    #[must_use]
    pub fn entity_types(&self) -> &[EntityType] {
        &self.entity_types
    }
}

#[derive(Debug)]
struct ContentBuilder {
    blocks: BTreeMap<String, BlockStateId>,
    #[cfg(feature = "item")]
    items: BTreeMap<String, (&'static Item, Option<String>)>,
    #[cfg(feature = "entity_type")]
    entity_types: BTreeMap<String, &'static EntityType>,
}

impl ContentBuilder {
    const fn new() -> Self {
        Self {
            blocks: BTreeMap::new(),
            #[cfg(feature = "item")]
            items: BTreeMap::new(),
            #[cfg(feature = "entity_type")]
            entity_types: BTreeMap::new(),
        }
    }

    fn register_block(&mut self, definition: BlockDefinition) -> Result<(), RegistryError> {
        let kind = ContentKind::Block;
        validate_name(kind, &definition.name)?;
        // `to_be_network_id` relies on this: display states are vanilla, and registration only
        // runs before the freeze, when no custom state id can exist yet.
        if definition.display.as_u16() >= BlockStateId::STATE_COUNT {
            return Err(RegistryError::DisplayNotVanilla {
                kind,
                name: definition.name,
            });
        }
        insert_new(&mut self.blocks, kind, definition.name, definition.display)
    }

    #[cfg(feature = "item")]
    fn register_item(&mut self, definition: ItemDefinition) -> Result<(), RegistryError> {
        let kind = ContentKind::Item;
        validate_name(kind, &definition.name)?;
        // Copy from the generated table: the passed value has pub fields and can be forged.
        let display = definition.display.id;
        let Some(display) = Item::from_id(display).filter(|_| display < Item::COUNT) else {
            return Err(RegistryError::DisplayNotVanilla {
                kind,
                name: definition.name,
            });
        };
        insert_new(
            &mut self.items,
            kind,
            definition.name,
            (display, definition.block),
        )
    }

    #[cfg(feature = "entity_type")]
    fn register_entity_type(
        &mut self,
        definition: EntityTypeDefinition,
    ) -> Result<(), RegistryError> {
        let kind = ContentKind::EntityType;
        validate_name(kind, &definition.name)?;
        // Copy from the generated table: the passed value has pub fields and can be forged.
        let display = definition.display.id;
        let Some(display) = EntityType::from_raw(display).filter(|_| display < EntityType::COUNT)
        else {
            return Err(RegistryError::DisplayNotVanilla {
                kind,
                name: definition.name,
            });
        };
        insert_new(&mut self.entity_types, kind, definition.name, display)
    }

    /// Allocates ids in name order and builds the tables. The maps iterate in name order.
    fn build(self) -> Result<ContentTables, RegistryError> {
        check_range("block", BlockId::COUNT, self.blocks.len())?;
        check_range("block state", BlockStateId::STATE_COUNT, self.blocks.len())?;
        #[cfg(feature = "item")]
        check_range("item", Item::COUNT, self.items.len())?;
        #[cfg(feature = "entity_type")]
        check_range("entity type", EntityType::COUNT, self.entity_types.len())?;

        #[cfg(feature = "item")]
        let (block_items, item_blocks) = self.link_items()?;
        // Item id 0 is air, which generated blocks without an item use too.
        #[cfg(not(feature = "item"))]
        let block_items = vec![0; self.blocks.len()];

        let blocks: Vec<(&'static str, BlockStateId)> = self
            .blocks
            .into_iter()
            .map(|(name, display)| (leak(name), display))
            .collect();
        let block_names: HashMap<&'static str, usize> = blocks
            .iter()
            .enumerate()
            .map(|(index, (name, _))| (*name, index))
            .collect();

        #[cfg(feature = "item")]
        let items: Vec<Item> = self
            .items
            .into_iter()
            .enumerate()
            .map(|(index, (name, (display, _)))| Item {
                id: Item::COUNT + index as u16,
                registry_key: leak(name),
                ..display.clone()
            })
            .collect();

        let states: Vec<BlockState> = blocks
            .iter()
            .enumerate()
            .map(|(index, (_, display))| {
                let display = display.to_state();
                BlockState {
                    id: BlockStateId::new_unchecked(BlockStateId::STATE_COUNT + index as u16),
                    state_flags: display.state_flags,
                    side_flags: display.side_flags,
                    instrument: display.instrument,
                    luminance: display.luminance,
                    piston_behavior: display.piston_behavior.clone(),
                    hardness: display.hardness,
                    collision_shapes: display.collision_shapes,
                    outline_shapes: display.outline_shapes,
                    opacity: display.opacity,
                    // Block entities are behaviour, which custom blocks do not have yet.
                    block_entity_type: u16::MAX,
                }
            })
            .collect();
        let states: &'static [BlockState] = Box::leak(states.into_boxed_slice());

        let block_ids: Vec<BlockId> = (0..blocks.len())
            .map(|index| BlockId::new_unchecked(BlockId::COUNT + index as u16))
            .collect();
        let custom_blocks = blocks
            .iter()
            .zip(states)
            .zip(&block_ids)
            .zip(&block_items)
            .map(|((((name, display), state), &id), &item_id)| Block {
                id,
                name,
                item_id,
                default_state: state,
                states: std::slice::from_ref(state),
                ..display.to_block().clone()
            })
            .collect();

        #[cfg(feature = "entity_type")]
        let entity_types: Vec<EntityType> = self
            .entity_types
            .into_iter()
            .enumerate()
            .map(|(index, (name, display))| EntityType {
                id: EntityType::COUNT + index as u16,
                resource_name: leak(name),
                ..display.clone()
            })
            .collect();

        Ok(ContentTables {
            blocks: custom_blocks,
            block_names,
            states,
            state_displays: blocks.iter().map(|(_, display)| *display).collect(),
            state_blocks: block_ids,
            #[cfg(feature = "item")]
            item_names: name_index(items.iter().map(|item| item.registry_key)),
            #[cfg(feature = "item")]
            items,
            #[cfg(feature = "item")]
            item_blocks,
            #[cfg(feature = "entity_type")]
            entity_type_names: name_index(entity_types.iter().map(|ty| ty.resource_name)),
            #[cfg(feature = "entity_type")]
            entity_types,
        })
    }

    /// Resolves the item -> block links before anything is leaked, so a failed build leaks
    /// nothing. Returns the item id of each custom block and the block of each custom item. The
    /// BTreeMaps iterate in name order, which is the allocation order.
    #[cfg(feature = "item")]
    fn link_items(&self) -> Result<(Vec<u16>, Vec<Option<BlockId>>), RegistryError> {
        let block_index: HashMap<&str, usize> = self
            .blocks
            .keys()
            .enumerate()
            .map(|(index, name)| (name.as_str(), index))
            .collect();
        let mut block_items = vec![Item::AIR.id; self.blocks.len()];
        let mut item_blocks = Vec::with_capacity(self.items.len());
        for (index, (name, (_, block))) in self.items.iter().enumerate() {
            let placed = match block {
                None => None,
                Some(block) => {
                    let Some(&block_index) = block_index.get(block.as_str()) else {
                        return Err(RegistryError::UnknownBlock {
                            item: name.clone(),
                            block: block.clone(),
                        });
                    };
                    if block_items[block_index] != Item::AIR.id {
                        return Err(RegistryError::BlockAlreadyHasItem {
                            item: name.clone(),
                            block: block.clone(),
                        });
                    }
                    block_items[block_index] = Item::COUNT + index as u16;
                    Some(BlockId::new_unchecked(BlockId::COUNT + block_index as u16))
                }
            };
            item_blocks.push(placed);
        }
        Ok((block_items, item_blocks))
    }
}

fn leak(name: String) -> &'static str {
    Box::leak(name.into_boxed_str())
}

fn name_index(names: impl Iterator<Item = &'static str>) -> HashMap<&'static str, usize> {
    names
        .enumerate()
        .map(|(index, name)| (name, index))
        .collect()
}

fn insert_new<V>(
    map: &mut BTreeMap<String, V>,
    kind: ContentKind,
    name: String,
    value: V,
) -> Result<(), RegistryError> {
    match map.entry(name) {
        Entry::Occupied(entry) => Err(RegistryError::Duplicate {
            kind,
            name: entry.key().clone(),
        }),
        Entry::Vacant(entry) => {
            entry.insert(value);
            Ok(())
        }
    }
}

/// Accepts `namespace:path` with the characters vanilla allows in a resource location.
fn validate_name(kind: ContentKind, name: &str) -> Result<(), RegistryError> {
    let reserved = || RegistryError::ReservedNamespace {
        kind,
        name: name.to_string(),
    };
    // A name without a namespace, or with an empty one, resolves to `minecraft`.
    let Some((namespace, path)) = name.split_once(':') else {
        return Err(reserved());
    };
    if namespace.is_empty() || namespace == VANILLA_NAMESPACE {
        return Err(reserved());
    }
    if Identifier::is_valid_namespace(namespace)
        && !path.is_empty()
        && Identifier::is_valid_path(path)
    {
        Ok(())
    } else {
        Err(RegistryError::InvalidName {
            kind,
            name: name.to_string(),
        })
    }
}

fn check_range(registry: &'static str, generated: u16, count: usize) -> Result<(), RegistryError> {
    if count > usize::from(MAX_ID - generated) + 1 {
        return Err(RegistryError::RangeExceeded { registry, count });
    }
    Ok(())
}

static PENDING: Mutex<Option<ContentBuilder>> = Mutex::new(Some(ContentBuilder::new()));
static TABLES: OnceLock<ContentTables> = OnceLock::new();

fn with_builder(
    register: impl FnOnce(&mut ContentBuilder) -> Result<(), RegistryError>,
) -> Result<(), RegistryError> {
    let mut pending = PENDING.lock().unwrap_or_else(PoisonError::into_inner);
    pending
        .as_mut()
        .map_or(Err(RegistryError::RegistryFrozen), register)
}

/// Registers a custom block. Its id is allocated by [`freeze`].
pub fn register_block(definition: BlockDefinition) -> Result<(), RegistryError> {
    with_builder(|builder| builder.register_block(definition))
}

/// Registers a custom item. Its id is allocated by [`freeze`].
#[cfg(feature = "item")]
pub fn register_item(definition: ItemDefinition) -> Result<(), RegistryError> {
    with_builder(|builder| builder.register_item(definition))
}

/// Registers a custom entity type. Its id is allocated by [`freeze`].
#[cfg(feature = "entity_type")]
pub fn register_entity_type(definition: EntityTypeDefinition) -> Result<(), RegistryError> {
    with_builder(|builder| builder.register_entity_type(definition))
}

/// Allocates the ids of all registered content and installs the tables. Runs once: later calls,
/// and registrations after it, return [`RegistryError::RegistryFrozen`].
pub fn freeze() -> Result<&'static ContentTables, RegistryError> {
    let builder = PENDING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
        .ok_or(RegistryError::RegistryFrozen)?;
    let tables = builder.build()?;
    Ok(TABLES.get_or_init(|| tables))
}

/// Returns the namespaced form of a block, item or entity type name. Generated content keeps the
/// bare vanilla name, custom content carries its own namespace.
#[must_use]
pub fn namespaced_name(name: &str) -> Cow<'_, str> {
    if name.contains(':') {
        Cow::Borrowed(name)
    } else {
        Cow::Owned(format!("{VANILLA_NAMESPACE}:{name}"))
    }
}

fn custom_index(raw: u16, generated: u16) -> Option<usize> {
    raw.checked_sub(generated).map(usize::from)
}

#[cold]
#[inline(never)]
pub(crate) fn is_custom_state(raw: u16) -> bool {
    TABLES.get().is_some_and(|tables| {
        custom_index(raw, BlockStateId::STATE_COUNT)
            .is_some_and(|index| index < tables.states.len())
    })
}

#[cold]
#[inline(never)]
pub(crate) fn is_custom_block(raw: u16) -> bool {
    TABLES.get().is_some_and(|tables| {
        custom_index(raw, BlockId::COUNT).is_some_and(|index| index < tables.blocks.len())
    })
}

// A `BlockStateId` or `BlockId` above the generated count only exists after its constructor
// found it in the installed tables, and the tables never change after that.

#[cold]
#[inline(never)]
#[expect(
    clippy::unreachable,
    reason = "BlockStateId::new checks the installed tables"
)]
pub(crate) fn state(id: BlockStateId) -> &'static BlockState {
    TABLES
        .get()
        .and_then(|tables| {
            tables
                .states
                .get(custom_index(id.as_u16(), BlockStateId::STATE_COUNT)?)
        })
        .unwrap_or_else(|| unreachable!("{id:?} is not an installed custom state"))
}

#[cold]
#[inline(never)]
#[expect(
    clippy::unreachable,
    reason = "BlockStateId::new checks the installed tables"
)]
pub(crate) fn block_id_of_state(id: BlockStateId) -> BlockId {
    TABLES
        .get()
        .and_then(|tables| {
            tables
                .state_blocks
                .get(custom_index(id.as_u16(), BlockStateId::STATE_COUNT)?)
        })
        .copied()
        .unwrap_or_else(|| unreachable!("{id:?} is not an installed custom state"))
}

#[cold]
#[inline(never)]
#[expect(
    clippy::unreachable,
    reason = "BlockStateId::new checks the installed tables"
)]
pub(crate) fn display_state(id: BlockStateId) -> BlockStateId {
    TABLES
        .get()
        .and_then(|tables| {
            tables
                .state_displays
                .get(custom_index(id.as_u16(), BlockStateId::STATE_COUNT)?)
        })
        .copied()
        .unwrap_or_else(|| unreachable!("{id:?} is not an installed custom state"))
}

#[cold]
#[inline(never)]
#[expect(
    clippy::unreachable,
    reason = "BlockId::new checks the installed tables"
)]
pub(crate) fn block(id: BlockId) -> &'static Block {
    TABLES
        .get()
        .and_then(|tables| {
            tables
                .blocks
                .get(custom_index(id.as_u16(), BlockId::COUNT)?)
        })
        .unwrap_or_else(|| unreachable!("{id:?} is not an installed custom block"))
}

#[cold]
#[inline(never)]
pub(crate) fn block_by_name(name: &str) -> Option<&'static Block> {
    let tables = TABLES.get()?;
    tables.blocks.get(*tables.block_names.get(name)?)
}

/// The first custom item id. `Block::from_item_id` answers ids below it inline.
#[cfg(feature = "item")]
pub(crate) const ITEM_COUNT: u16 = Item::COUNT;
/// Without the item feature no custom item exists, so every id stays on the generated path.
#[cfg(not(feature = "item"))]
pub(crate) const ITEM_COUNT: u16 = u16::MAX;

#[cold]
#[inline(never)]
pub(crate) fn block_from_item_id(id: u16) -> Option<&'static Block> {
    #[cfg(feature = "item")]
    {
        let tables = TABLES.get()?;
        let block_id = (*tables.item_blocks.get(custom_index(id, Item::COUNT)?)?)?;
        tables
            .blocks
            .get(custom_index(block_id.as_u16(), BlockId::COUNT)?)
    }
    #[cfg(not(feature = "item"))]
    {
        let _ = id;
        None
    }
}

#[cold]
#[inline(never)]
#[cfg(feature = "item")]
pub(crate) fn item(id: u16) -> Option<&'static Item> {
    TABLES.get()?.items.get(custom_index(id, Item::COUNT)?)
}

#[cold]
#[inline(never)]
#[cfg(feature = "item")]
pub(crate) fn item_by_name(name: &str) -> Option<&'static Item> {
    let tables = TABLES.get()?;
    tables.items.get(*tables.item_names.get(name)?)
}

#[cold]
#[inline(never)]
#[cfg(feature = "entity_type")]
pub(crate) fn entity_type(id: u16) -> Option<&'static EntityType> {
    TABLES
        .get()?
        .entity_types
        .get(custom_index(id, EntityType::COUNT)?)
}

#[cold]
#[inline(never)]
#[cfg(feature = "entity_type")]
pub(crate) fn entity_type_by_name(name: &str) -> Option<&'static EntityType> {
    let tables = TABLES.get()?;
    tables
        .entity_types
        .get(*tables.entity_type_names.get(name)?)
}

#[cfg(all(test, feature = "item", feature = "entity_type"))]
mod tests {
    use super::*;
    use crate::tag::Taggable;

    fn block(name: &str, display: &Block) -> BlockDefinition {
        BlockDefinition {
            name: name.to_string(),
            display: display.default_state.id,
        }
    }

    fn item(name: &str, block: Option<&str>) -> ItemDefinition {
        ItemDefinition {
            name: name.to_string(),
            display: &Item::DIAMOND_SWORD,
            block: block.map(str::to_string),
        }
    }

    fn entity_type(name: &str) -> EntityTypeDefinition {
        EntityTypeDefinition {
            name: name.to_string(),
            display: &EntityType::ZOMBIE,
        }
    }

    fn register_all(builder: &mut ContentBuilder, names: &[&str]) {
        for name in names {
            builder.register_block(block(name, &Block::STONE)).unwrap();
            builder.register_item(item(name, Some(name))).unwrap();
            builder.register_entity_type(entity_type(name)).unwrap();
        }
    }

    fn allocation(tables: &ContentTables) -> Vec<(&'static str, u16, u16, u16, u16)> {
        tables
            .blocks()
            .iter()
            .zip(tables.items())
            .zip(tables.entity_types())
            .map(|((block, item), entity_type)| {
                (
                    block.name,
                    block.id.as_u16(),
                    block.default_state.id.as_u16(),
                    item.id,
                    entity_type.id,
                )
            })
            .collect()
    }

    #[test]
    fn allocation_does_not_depend_on_registration_order() {
        let mut forward = ContentBuilder::new();
        register_all(&mut forward, &["b:middle", "a:first", "c:last"]);
        let mut backward = ContentBuilder::new();
        register_all(&mut backward, &["c:last", "a:first", "b:middle"]);

        let forward = forward.build().unwrap();
        let backward = backward.build().unwrap();

        let block = BlockId::COUNT;
        let state = BlockStateId::STATE_COUNT;
        let item = Item::COUNT;
        let entity = EntityType::COUNT;
        assert_eq!(
            allocation(&forward),
            vec![
                ("a:first", block, state, item, entity),
                ("b:middle", block + 1, state + 1, item + 1, entity + 1),
                ("c:last", block + 2, state + 2, item + 2, entity + 2),
            ]
        );
        assert_eq!(allocation(&forward), allocation(&backward));
        for (block, item) in forward.blocks().iter().zip(forward.items()) {
            assert_eq!(block.item_id, item.id);
        }
    }

    #[test]
    fn rejects_duplicates_and_reserved_names() {
        let mut builder = ContentBuilder::new();
        builder
            .register_block(block("mymod:lamp", &Block::STONE))
            .unwrap();
        assert_eq!(
            builder.register_block(block("mymod:lamp", &Block::DIRT)),
            Err(RegistryError::Duplicate {
                kind: ContentKind::Block,
                name: "mymod:lamp".to_string()
            })
        );
        // The same name in another registry is a different entry, as in vanilla.
        builder.register_item(item("mymod:lamp", None)).unwrap();

        for name in ["minecraft:lamp", "lamp", ":lamp"] {
            assert_eq!(
                builder.register_block(block(name, &Block::STONE)),
                Err(RegistryError::ReservedNamespace {
                    kind: ContentKind::Block,
                    name: name.to_string()
                })
            );
        }
        for name in ["MyMod:lamp", "mymod:", "mymod:lamp:2", "my mod:lamp"] {
            assert!(matches!(
                builder.register_entity_type(entity_type(name)),
                Err(RegistryError::InvalidName { .. })
            ));
        }
    }

    #[test]
    fn rejects_items_for_unknown_or_taken_blocks() {
        let mut builder = ContentBuilder::new();
        builder
            .register_item(item("mymod:a", Some("mymod:missing")))
            .unwrap();
        assert_eq!(
            builder.build().unwrap_err(),
            RegistryError::UnknownBlock {
                item: "mymod:a".to_string(),
                block: "mymod:missing".to_string()
            }
        );

        let mut builder = ContentBuilder::new();
        builder
            .register_block(block("mymod:lamp", &Block::STONE))
            .unwrap();
        builder
            .register_item(item("mymod:a", Some("mymod:lamp")))
            .unwrap();
        builder
            .register_item(item("mymod:b", Some("mymod:lamp")))
            .unwrap();
        assert_eq!(
            builder.build().unwrap_err(),
            RegistryError::BlockAlreadyHasItem {
                item: "mymod:b".to_string(),
                block: "mymod:lamp".to_string()
            }
        );
    }

    #[test]
    fn failed_item_link_fails_before_allocation() {
        let mut failing = ContentBuilder::new();
        register_all(&mut failing, &["mymod:a", "mymod:b"]);
        failing
            .register_item(item("mymod:c", Some("mymod:missing")))
            .unwrap();
        assert_eq!(
            failing.build().unwrap_err(),
            RegistryError::UnknownBlock {
                item: "mymod:c".to_string(),
                block: "mymod:missing".to_string()
            }
        );

        let mut fixed = ContentBuilder::new();
        register_all(&mut fixed, &["mymod:a", "mymod:b"]);
        fixed.register_item(item("mymod:c", None)).unwrap();
        let tables = fixed.build().unwrap();
        assert_eq!(tables.blocks().len(), 2);
        assert_eq!(tables.items().len(), 3);
        assert_eq!(tables.items()[2].id, Item::COUNT + 2);
        assert_eq!(tables.blocks()[1].item_id, Item::COUNT + 1);
    }

    #[test]
    fn rejects_more_blocks_than_free_state_ids() {
        let free = usize::from(MAX_ID - BlockStateId::STATE_COUNT) + 1;
        assert_eq!(free, 29_812);
        let mut builder = ContentBuilder::new();
        for index in 0..=free {
            builder
                .register_block(block(&format!("mymod:b{index}"), &Block::STONE))
                .unwrap();
        }
        assert_eq!(
            builder.build().unwrap_err(),
            RegistryError::RangeExceeded {
                registry: "block state",
                count: free + 1
            }
        );
    }

    #[test]
    fn rejects_content_beyond_the_id_range() {
        let free = usize::from(MAX_ID - BlockStateId::STATE_COUNT) + 1;
        assert!(check_range("block state", BlockStateId::STATE_COUNT, free).is_ok());
        assert_eq!(
            check_range("block state", BlockStateId::STATE_COUNT, free + 1),
            Err(RegistryError::RangeExceeded {
                registry: "block state",
                count: free + 1
            })
        );
    }

    /// The only test that installs the global tables: they are process-wide and install once.
    #[test]
    fn frozen_registry_falls_through_and_rejects_registration() {
        register_block(block("test:zeta", &Block::DIRT)).unwrap();
        register_block(block("test:alpha", &Block::STONE)).unwrap();
        register_item(item("test:alpha", Some("test:alpha"))).unwrap();
        register_entity_type(entity_type("test:golem")).unwrap();

        let tables = freeze().unwrap();
        assert_eq!(tables.blocks().len(), 2);

        // Blocks and states.
        let alpha = Block::from_registry_key("test:alpha").unwrap();
        assert_eq!(alpha.id.as_u16(), BlockId::COUNT);
        assert_eq!(Block::from_name("test:alpha"), Some(alpha));
        assert_eq!(
            Block::from_name("test:zeta").unwrap().id.as_u16(),
            BlockId::COUNT + 1
        );
        assert_eq!(Block::from_name("minecraft:stone"), Some(&Block::STONE));
        assert_eq!(Block::from_registry_key("test:missing"), None);

        let block_id = BlockId::new(BlockId::COUNT).unwrap();
        assert_eq!(Block::from_id(block_id), alpha);
        assert_eq!(block_id.to_block(), alpha);
        assert_eq!(BlockId::new(BlockId::COUNT + 2), None);
        assert_eq!(BlockId::new_or_air(BlockId::COUNT + 2), BlockId::AIR);

        let state_id = BlockStateId::new(BlockStateId::COUNT).unwrap();
        assert_eq!(state_id, alpha.default_state.id);
        assert_eq!(BlockStateId::new(BlockStateId::COUNT + 2), None);
        assert_eq!(
            BlockStateId::new_or_air(BlockStateId::COUNT + 2),
            BlockStateId::AIR
        );
        assert_eq!(BlockState::from_id(state_id).id, state_id);
        assert_eq!(
            state_id.to_state().hardness,
            Block::STONE.default_state.hardness
        );
        assert_eq!(Block::from_state_id(state_id), alpha);
        assert_eq!(BlockId::from_state_id(state_id), block_id);
        assert_eq!(BlockState::from_id_with_block(state_id).0, alpha);
        assert_eq!(
            BlockState::to_be_network_id(state_id),
            BlockState::to_be_network_id(Block::STONE.default_state.id)
        );
        assert!(alpha.properties(state_id).is_none());

        // Items and the block they place.
        let alpha_item = Item::from_registry_key("test:alpha").unwrap();
        assert_eq!(alpha_item.id, Item::COUNT);
        assert_eq!(Item::from_id(Item::COUNT), Some(alpha_item));
        assert_eq!(Item::from_id(Item::COUNT + 1), None);
        assert_eq!(
            Item::from_registry_key("minecraft:stone"),
            Some(&Item::STONE)
        );
        assert_eq!(alpha.item_id, alpha_item.id);
        assert_eq!(Block::from_item_id(alpha_item.id), Some(alpha));
        assert_eq!(Block::from_item_id(Item::DIAMOND.id), None);

        // Entity types.
        let golem = EntityType::from_name("test:golem").unwrap();
        assert_eq!(golem.id, EntityType::COUNT);
        assert_eq!(EntityType::from_raw(EntityType::COUNT), Some(golem));
        assert_eq!(EntityType::from_raw(EntityType::COUNT + 1), None);
        assert_eq!(
            EntityType::from_name("minecraft:zombie"),
            Some(&EntityType::ZOMBIE)
        );

        // Generated tag lists hold generated ids only.
        assert!(!alpha.has_tag(&crate::tag::Block::MINECRAFT_BASE_STONE_OVERWORLD));

        // Frozen.
        assert_eq!(
            register_block(block("test:late", &Block::STONE)),
            Err(RegistryError::RegistryFrozen)
        );
        assert_eq!(
            register_item(item("test:late", None)),
            Err(RegistryError::RegistryFrozen)
        );
        assert_eq!(
            register_entity_type(entity_type("test:late")),
            Err(RegistryError::RegistryFrozen)
        );
        assert_eq!(freeze().unwrap_err(), RegistryError::RegistryFrozen);
        assert_eq!(Block::from_name("test:late"), None);
    }
}
