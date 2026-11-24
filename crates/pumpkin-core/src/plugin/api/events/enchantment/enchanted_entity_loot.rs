use std::sync::Arc;

use pumpkin_data::Enchantment;
use pumpkin_macros::Event;

use crate::entity::EntityBase;
use crate::plugin::living::DamageSource;

/// Fires when the loot of a killed entity asks for an enchantment level.
///
/// `NeoForge`: `net.neoforged.neoforge.event.enchanting.EnchantedEntityLootEvent`, fired by
/// `EventHooks.getEntityLootEnchantmentLevel` in `EnchantedCountIncreaseFunction` and
/// `LootItemRandomChanceWithEnchantedBonusCondition` when the loot context has a living entity
/// and a damage source. `enchantment_level` is settable: the loot function or condition reads it
/// after the event. A negative level is ignored, as `setEnchantmentLevel` throws for it.
#[derive(Event, Clone)]
pub struct EnchantedEntityLootEvent {
    /// The entity that drops the loot, the killed one.
    pub entity: Arc<dyn EntityBase>,
    pub damage_source: DamageSource,
    pub enchantment: &'static Enchantment,
    /// The effective level of the attacker, with the result of `GetEnchantmentLevelEvent`.
    pub enchantment_level: i32,
}

impl EnchantedEntityLootEvent {
    #[must_use]
    pub const fn new(
        entity: Arc<dyn EntityBase>,
        damage_source: DamageSource,
        enchantment: &'static Enchantment,
        enchantment_level: i32,
    ) -> Self {
        Self {
            entity,
            damage_source,
            enchantment,
            enchantment_level,
        }
    }
}
