//! The block type and the block entity type of custom blocks.
//!
//! A custom block can take a block type: the vanilla block whose server behaviour, state
//! properties and per-state physical data it uses, as a `NeoForge` mod block takes them from the
//! vanilla class it extends (`StairBlock`, `DoorBlock` and the others). Minecraft 26.3 has no
//! `minecraft:block_type` registry, so the type is named by a vanilla block of that kind, for
//! example `minecraft:oak_stairs` for a stair.
//!
//! The generated property structs accept a typed custom block wherever they accept its type, and
//! `Taggable::has_tag` answers for a custom entry through its block type or display entry. Both
//! read [`block_type`] and [`tag_source`] only for ids at or above the generated counts.
//!
//! A custom block can also join the valid blocks of a vanilla block entity type: its states then
//! carry that block entity type, and placing the block creates the block entity.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock, PoisonError};

use crate::dynamic::{self, BlockDefinition, BlockPropertyDefinition, ContentKind, RegistryError};
use crate::tag::RegistryKey;
use crate::{Block, BlockId};

static BLOCK_TYPES: Mutex<BTreeMap<String, BlockId>> = Mutex::new(BTreeMap::new());
static BLOCK_TYPE_TABLE: OnceLock<Vec<Option<BlockId>>> = OnceLock::new();
static BLOCK_ENTITY_TYPES: Mutex<BTreeMap<String, u16>> = Mutex::new(BTreeMap::new());

/// Registers a custom block that takes the block type `block_type`, a vanilla block name such as
/// `minecraft:oak_stairs`.
///
/// A definition without properties gets the properties of the type. A definition with
/// properties must list the same properties and values as the type, so the states of both share
/// one layout. Its defaults may differ.
///
/// # Errors
///
/// [`RegistryError::InvalidBlockType`] when `block_type` is not a vanilla block or the properties
/// differ, else the error of [`dynamic::register_block`].
pub fn register_block(
    mut definition: BlockDefinition,
    block_type: &str,
) -> Result<(), RegistryError> {
    let invalid = |reason| RegistryError::InvalidBlockType {
        block: definition.name.clone(),
        block_type: block_type.to_owned(),
        reason,
    };
    let Some(base) =
        Block::from_name(block_type).filter(|block| block.id.as_u16() < BlockId::COUNT)
    else {
        return Err(invalid("it is not a vanilla block"));
    };
    let properties = property_definitions(base);
    if definition.properties.is_empty() {
        definition.properties.clone_from(&properties);
    } else if layout(definition.properties.clone()) != layout(properties) {
        return Err(invalid("the block lists other properties than its type"));
    }
    let name = definition.name.clone();
    // Held across both steps, so the freeze sees the type of every accepted block.
    let mut types = BLOCK_TYPES.lock().unwrap_or_else(PoisonError::into_inner);
    dynamic::register_block(definition)?;
    types.insert(name, base.id);
    Ok(())
}

/// Makes the states of the custom block `block` carry the block entity type with index
/// `block_entity_type` in [`BLOCK_ENTITY_TYPES`](crate::block_properties::BLOCK_ENTITY_TYPES).
/// Returns `false`, and changes nothing, when the block already has another block entity type: a
/// state has one block entity type.
///
/// # Errors
///
/// [`RegistryError::RegistryFrozen`] after the freeze.
pub fn add_block_entity_block(block: &str, block_entity_type: u16) -> Result<bool, RegistryError> {
    let mut types = BLOCK_ENTITY_TYPES
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if dynamic::tables().is_some() {
        return Err(RegistryError::RegistryFrozen);
    }
    Ok(*types.entry(block.to_owned()).or_insert(block_entity_type) == block_entity_type)
}

/// The block type of a custom block, or `None` for a generated block and an untyped custom block.
#[inline]
#[must_use]
pub fn block_type(block: BlockId) -> Option<BlockId> {
    if block.as_u16() < BlockId::COUNT {
        return None;
    }
    custom_block_type(block)
}

/// The generated entry whose tags `Taggable::has_tag` reads for the entry `id` of the registry
/// `key`: the block type or the display entry of a custom entry. `None` for a generated entry.
#[inline]
#[must_use]
pub fn tag_source(key: RegistryKey, id: u16) -> Option<u16> {
    match key {
        RegistryKey::Block if id >= BlockId::COUNT => block_tag_source(id),
        #[cfg(feature = "item")]
        RegistryKey::Item if id >= crate::item::Item::COUNT => crate::item::Item::from_id(id)
            .map(|item| item.to_java_network_id(dynamic::ContentIds::Display)),
        #[cfg(feature = "entity_type")]
        RegistryKey::EntityType if id >= crate::entity::EntityType::COUNT => {
            crate::entity::EntityType::from_raw(id).map(|ty| ty.display_type().id)
        }
        _ => None,
    }
}

#[cold]
#[inline(never)]
fn block_tag_source(id: u16) -> Option<u16> {
    let block = BlockId::new(id)?;
    Some(block_type(block).map_or_else(
        || Block::from_id(block).to_java_network_id(dynamic::ContentIds::Display),
        BlockId::as_u16,
    ))
}

#[cold]
#[inline(never)]
fn custom_block_type(block: BlockId) -> Option<BlockId> {
    // Before the freeze no custom id exists, and the table must not be built yet.
    let tables = dynamic::tables()?;
    BLOCK_TYPE_TABLE
        .get_or_init(|| {
            let types = BLOCK_TYPES.lock().unwrap_or_else(PoisonError::into_inner);
            tables
                .blocks()
                .iter()
                .map(|block| types.get(block.name).copied())
                .collect()
        })
        .get(usize::from(block.as_u16() - BlockId::COUNT))
        .copied()
        .flatten()
}

/// The block type of the registered custom block `name`, for the freeze.
pub(crate) fn pending_block_type(name: &str) -> Option<BlockId> {
    BLOCK_TYPES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(name)
        .copied()
}

/// The block entity type of the registered custom block `name`, for the freeze.
pub(crate) fn pending_block_entity_type(name: &str) -> Option<u16> {
    BLOCK_ENTITY_TYPES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(name)
        .copied()
}

/// The properties of a generated block in state layout order: the values of each property in the
/// order of its index, and the values of the default state as defaults.
#[must_use]
pub fn property_definitions(block: &Block) -> Vec<BlockPropertyDefinition> {
    let Some(default) = block.properties(block.default_state.id) else {
        return Vec::new();
    };
    let mut properties: Vec<BlockPropertyDefinition> = default
        .to_props()
        .into_iter()
        .map(|(name, value)| BlockPropertyDefinition {
            name: name.to_owned(),
            values: Vec::new(),
            default: value.to_owned(),
        })
        .collect();
    // The first state of each value index comes before the first state of the next one.
    for state in block.states {
        let Some(props) = block.properties(state.id) else {
            continue;
        };
        for (property, (_, value)) in properties.iter_mut().zip(props.to_props()) {
            if !property.values.iter().any(|known| known == value) {
                property.values.push(value.to_owned());
            }
        }
    }
    properties
}

/// The names and values of the properties in name order, the order of the state layout. The
/// defaults do not change the layout.
fn layout(properties: Vec<BlockPropertyDefinition>) -> Vec<(String, Vec<String>)> {
    let mut layout: Vec<_> = properties
        .into_iter()
        .map(|property| (property.name, property.values))
        .collect();
    layout.sort();
    layout
}

/// Whether two generated blocks have the same properties, so their states share one layout.
pub(crate) fn same_layout(a: &Block, b: &Block) -> bool {
    a.states.len() == b.states.len()
        && layout(property_definitions(a)) == layout(property_definitions(b))
}
