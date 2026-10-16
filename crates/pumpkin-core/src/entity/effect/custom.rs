//! The behaviour of the mob effects that mods register.
//!
//! The effect itself lives in [`pumpkin_data::dynamic_registries`]. Its behaviour lives here,
//! keyed by its name, and becomes a lock-free table indexed by the custom id on the first lookup
//! after the freeze, like the behaviours of custom blocks and items.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use pumpkin_data::dynamic::RegistryError;
use pumpkin_data::dynamic_registries::{self, MobEffectDefinition};
use pumpkin_data::effect::StatusEffect;

use super::MobEffect;

static PENDING: Mutex<BTreeMap<String, Arc<dyn MobEffect>>> = Mutex::new(BTreeMap::new());
static TABLE: OnceLock<Vec<Option<Arc<dyn MobEffect>>>> = OnceLock::new();

/// Registers a mob effect and the behaviour that ticks it, like a `MobEffect` subclass of a mod.
///
/// # Errors
///
/// Returns the error of [`dynamic_registries::register_mob_effect`]. The behaviour is dropped
/// then.
pub fn register_mob_effect(
    definition: MobEffectDefinition,
    behaviour: Option<Arc<dyn MobEffect>>,
) -> Result<(), RegistryError> {
    let name = definition.name.clone();
    // The lock is held across both steps, so the table built after the freeze sees every
    // behaviour whose effect was accepted.
    let mut pending = PENDING.lock().unwrap_or_else(PoisonError::into_inner);
    dynamic_registries::register_mob_effect(definition)?;
    if let Some(behaviour) = behaviour {
        pending.insert(name, behaviour);
    }
    Ok(())
}

/// The behaviour of a mod effect, or `None` for a generated effect or a mod effect without one.
#[must_use]
pub fn mob_effect_behaviour(effect: &'static StatusEffect) -> Option<&'static dyn MobEffect> {
    let index = usize::from(effect.id.checked_sub(StatusEffect::COUNT)?);
    // A mod effect exists only after the freeze, and registration fails after it, so the pending
    // map is complete when the table is built.
    TABLE
        .get_or_init(|| {
            let mut pending = PENDING.lock().unwrap_or_else(PoisonError::into_inner);
            dynamic_registries::mob_effects()
                .iter()
                .map(|effect| pending.remove(effect.minecraft_name))
                .collect()
        })
        .get(index)?
        .as_deref()
}
