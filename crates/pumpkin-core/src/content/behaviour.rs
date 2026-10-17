//! Behaviours of custom blocks and items.
//!
//! The vanilla [`BlockRegistry`](crate::block::registry::BlockRegistry) and
//! [`ItemRegistry`](crate::item::registry::ItemRegistry) are built before native mods run and
//! never change. The behaviour of a custom block or item lives here, keyed by its name, and
//! becomes a lock-free table indexed by the custom id on the first lookup after the freeze. The
//! two registries read it for ids at or above the generated counts.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use pumpkin_data::dynamic::{self, BlockDefinition, ItemDefinition, RegistryError};
use pumpkin_data::dynamic_types;
use pumpkin_data::item::Item;
use pumpkin_data::{Block, BlockId};

use crate::block::BlockBehaviour;
use crate::item::ItemBehaviour;

struct Behaviours<T: ?Sized> {
    pending: Mutex<BTreeMap<String, Arc<T>>>,
    table: OnceLock<Vec<Option<Arc<T>>>>,
}

impl<T: ?Sized> Behaviours<T> {
    const fn new() -> Self {
        Self {
            pending: Mutex::new(BTreeMap::new()),
            table: OnceLock::new(),
        }
    }

    /// Registers the content with `register` and keeps `behaviour` only if the registry accepts
    /// it. The lock is held across both steps, so the table built after the freeze sees every
    /// behaviour whose content was accepted.
    fn register(
        &self,
        name: String,
        behaviour: Option<Arc<T>>,
        register: impl FnOnce() -> Result<(), RegistryError>,
    ) -> Result<(), RegistryError> {
        let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
        register()?;
        if let Some(behaviour) = behaviour {
            pending.insert(name, behaviour);
        }
        Ok(())
    }

    /// `names` yields the name of each custom id in id order. Call it only after the freeze.
    fn get<'a>(&self, index: usize, names: impl FnOnce() -> Vec<&'a str>) -> Option<&Arc<T>> {
        self.table
            .get_or_init(|| {
                let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
                names()
                    .into_iter()
                    .map(|name| pending.remove(name))
                    .collect()
            })
            .get(index)?
            .as_ref()
    }
}

static BLOCKS: Behaviours<dyn BlockBehaviour> = Behaviours::new();
static ITEMS: Behaviours<dyn ItemBehaviour> = Behaviours::new();

/// Registers a custom block and the behaviour that the server runs for each of its states.
///
/// # Errors
///
/// Returns the error of [`dynamic::register_block`]. The behaviour is dropped then.
pub fn register_block(
    definition: BlockDefinition,
    behaviour: Option<Arc<dyn BlockBehaviour>>,
) -> Result<(), RegistryError> {
    let name = definition.name.clone();
    BLOCKS.register(name, behaviour, || dynamic::register_block(definition))
}

/// Registers a custom item and the behaviour that the server runs when a player uses it.
///
/// # Errors
///
/// Returns the error of [`dynamic::register_item`]. The behaviour is dropped then.
pub fn register_item(
    definition: ItemDefinition,
    behaviour: Option<Arc<dyn ItemBehaviour>>,
) -> Result<(), RegistryError> {
    let name = definition.name.clone();
    ITEMS.register(name, behaviour, || dynamic::register_item(definition))
}

/// The behaviour of a custom block, or `None` for generated blocks, placeholders and custom
/// blocks registered without one.
#[cold]
#[inline(never)]
#[must_use]
pub fn block_behaviour(block: BlockId) -> Option<&'static Arc<dyn BlockBehaviour>> {
    let index = usize::from(block.as_u16().checked_sub(BlockId::COUNT)?);
    // A custom `BlockId` exists only after the freeze, and registration fails after it, so the
    // pending map is complete when the table is built.
    BLOCKS.get(index, || {
        (BlockId::COUNT..=u16::MAX)
            .map_while(BlockId::new)
            .map(|id| Block::from_id(id).name)
            .collect()
    })
}

/// The behaviour of a custom item, or `None` for generated items, placeholders and custom items
/// registered without one.
#[cold]
#[inline(never)]
#[must_use]
pub fn item_behaviour(item: u16) -> Option<&'static Arc<dyn ItemBehaviour>> {
    let index = usize::from(item.checked_sub(Item::COUNT)?);
    // `Item` has pub fields, so an id above the generated count can exist before the freeze.
    // A custom id resolves only after it.
    Item::from_id(item)?;
    ITEMS.get(index, || {
        (Item::COUNT..=u16::MAX)
            .map_while(Item::from_id)
            .map(|item| item.registry_key)
            .collect()
    })
}

/// Registers a custom block that takes the block type `block_type`, a vanilla block such as
/// `minecraft:oak_stairs`.
///
/// See [`dynamic_types::register_block`]. Without `behaviour` the server runs the behaviour of
/// the block type for it.
///
/// # Errors
///
/// Returns the error of [`dynamic_types::register_block`]. The behaviour is dropped then.
pub fn register_typed_block(
    definition: BlockDefinition,
    behaviour: Option<Arc<dyn BlockBehaviour>>,
    block_type: &str,
) -> Result<(), RegistryError> {
    let name = definition.name.clone();
    BLOCKS.register(name, behaviour, || {
        dynamic_types::register_block(definition, block_type)
    })
}

static ITEM_TYPES: Mutex<BTreeMap<String, u16>> = Mutex::new(BTreeMap::new());
static ITEM_TYPE_TABLE: OnceLock<Vec<Option<u16>>> = OnceLock::new();

/// Registers a custom item that takes the item type `item_type`, a vanilla item such as
/// `minecraft:bucket`. Without `behaviour` the server runs the behaviour of that item for it.
///
/// # Errors
///
/// [`RegistryError::UnknownItemType`] when `item_type` is not a vanilla item, else the error of
/// [`dynamic::register_item`]. The behaviour is dropped then.
pub fn register_typed_item(
    definition: ItemDefinition,
    behaviour: Option<Arc<dyn ItemBehaviour>>,
    item_type: &str,
) -> Result<(), RegistryError> {
    let Some(base) = Item::from_registry_key(item_type).filter(|item| item.id < Item::COUNT) else {
        return Err(RegistryError::UnknownItemType {
            item: definition.name,
            item_type: item_type.to_owned(),
        });
    };
    let name = definition.name.clone();
    // Held across the registration, so the table built after the freeze sees every type.
    let mut types = ITEM_TYPES.lock().unwrap_or_else(PoisonError::into_inner);
    ITEMS.register(name.clone(), behaviour, || {
        dynamic::register_item(definition)
    })?;
    types.insert(name, base.id);
    Ok(())
}

/// The vanilla item whose behaviour a custom item takes, or `None` for generated items and
/// custom items without an item type.
#[cold]
#[inline(never)]
#[must_use]
pub fn item_type(item: u16) -> Option<u16> {
    let index = usize::from(item.checked_sub(Item::COUNT)?);
    Item::from_id(item)?;
    ITEM_TYPE_TABLE
        .get_or_init(|| {
            let types = ITEM_TYPES.lock().unwrap_or_else(PoisonError::into_inner);
            (Item::COUNT..=u16::MAX)
                .map_while(Item::from_id)
                .map(|item| types.get(item.registry_key).copied())
                .collect()
        })
        .get(index)
        .copied()
        .flatten()
}
