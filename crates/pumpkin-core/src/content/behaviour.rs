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
