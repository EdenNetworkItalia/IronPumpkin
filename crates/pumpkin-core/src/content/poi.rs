//! Blocks that native mods add to the job site POI types, the backing of `NeoForge`
//! `ExtendPoiTypesEvent`.
//!
//! The server knows the POI types of villager job sites through
//! [`profession_for_block`](crate::world::villager_poi::profession_for_block), which reads this
//! table for a block it does not know. Mods add blocks by name before the freeze. On the first
//! lookup after it the names become a table by block id.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use pumpkin_data::Block;
use pumpkin_data::dynamic::{self, RegistryError};
use pumpkin_data::villager::VillagerProfession;
use rustc_hash::FxHashMap;

static PENDING: Mutex<BTreeMap<String, VillagerProfession>> = Mutex::new(BTreeMap::new());
/// Whether a mod added any block, so a block change on a server without one skips the table.
static ANY: AtomicBool = AtomicBool::new(false);
static TABLE: OnceLock<FxHashMap<u16, VillagerProfession>> = OnceLock::new();

/// The villager profession of a job site POI type such as `minecraft:librarian`, or `None` for
/// any other name.
#[must_use]
pub fn job_site_profession(poi_type: &str) -> Option<VillagerProfession> {
    let path = match poi_type.split_once(':') {
        Some(("minecraft", path)) => path,
        Some(_) => return None,
        None => poi_type,
    };
    VillagerProfession::from_name(path).filter(|profession| {
        !matches!(
            profession,
            VillagerProfession::None | VillagerProfession::Nitwit
        )
    })
}

/// Adds every state of the block `name` to the job site POI type of `profession`. Returns the
/// profession the block already had, and changes nothing then.
///
/// # Errors
///
/// [`RegistryError::RegistryFrozen`] after the freeze.
pub fn add_block(
    name: &str,
    profession: VillagerProfession,
) -> Result<Option<VillagerProfession>, RegistryError> {
    if dynamic::tables().is_some() {
        return Err(RegistryError::RegistryFrozen);
    }
    let mut pending = PENDING.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(previous) = pending.get(name) {
        return Ok(Some(*previous));
    }
    pending.insert(name.to_owned(), profession);
    ANY.store(true, Ordering::Relaxed);
    Ok(None)
}

/// The profession of a block that a mod added to a job site POI type.
#[must_use]
pub fn profession_for_block(block: &Block) -> Option<VillagerProfession> {
    if !ANY.load(Ordering::Relaxed) {
        return None;
    }
    table()?.get(&block.id.as_u16()).copied()
}

#[cold]
#[inline(never)]
fn table() -> Option<&'static FxHashMap<u16, VillagerProfession>> {
    // A custom block gets its id at the freeze, so the table waits for it.
    dynamic::tables()?;
    Some(TABLE.get_or_init(|| {
        PENDING
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter_map(|(name, profession)| {
                Some((Block::from_name(name)?.id.as_u16(), *profession))
            })
            .collect()
    }))
}
