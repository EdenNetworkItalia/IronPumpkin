//! The hopper state that the item handler of `ironpumpkin-mods` journals (`cooldownTimeJournal`
//! of the `NeoForge` hopper patch).

use std::sync::atomic::Ordering;

use super::HopperBlockEntity;

impl HopperBlockEntity {
    /// `cooldownTime`.
    #[must_use]
    pub fn cooldown(&self) -> i32 {
        self.cooldown_time.load(Ordering::Relaxed)
    }

    /// `setCooldown`.
    pub fn set_cooldown(&self, cooldown: i32) {
        self.cooldown_time.store(cooldown, Ordering::Relaxed);
    }
}
