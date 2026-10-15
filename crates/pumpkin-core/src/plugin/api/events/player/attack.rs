use std::sync::Arc;

use pumpkin_macros::{Event, cancellable};

use super::PlayerEvent;
use crate::entity::{EntityBase, player::Player};

/// `NeoForge` `CriticalHitEvent`: decides whether a player attack is a critical hit.
///
/// Fires in `Player::attack` before the damage is dealt. `critical_hit`, `damage_multiplier`
/// and `disable_sweep` are read back: a critical hit multiplies the damage by
/// `damage_multiplier` (a negative value counts as 0), and with `disable_sweep` it stops the
/// sweep, as in vanilla. A forced critical hit keeps the sprint knockback and the mace smash.
/// `vanilla_critical` is `false` for a falling mace hit: Pumpkin's `AttackType::new` makes it a
/// mace smash, never a critical hit.
#[derive(Event, Clone)]
pub struct CriticalHitEvent {
    pub player: Arc<Player>,
    pub target: Arc<dyn EntityBase>,
    /// The vanilla multiplier: 1.5 for a vanilla critical hit, 1.0 otherwise.
    pub vanilla_multiplier: f32,
    /// Whether vanilla made the attack a critical hit.
    pub vanilla_critical: bool,
    pub damage_multiplier: f32,
    pub critical_hit: bool,
    /// Whether a critical hit stops the sweep. Starts as `true`.
    pub disable_sweep: bool,
}

impl CriticalHitEvent {
    #[must_use]
    pub fn new(
        player: Arc<Player>,
        target: Arc<dyn EntityBase>,
        damage_multiplier: f32,
        critical_hit: bool,
    ) -> Self {
        Self {
            player,
            target,
            vanilla_multiplier: damage_multiplier,
            vanilla_critical: critical_hit,
            damage_multiplier,
            critical_hit,
            disable_sweep: true,
        }
    }
}

impl PlayerEvent for CriticalHitEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}

/// `NeoForge` `SweepAttackEvent`: decides whether a player attack sweeps.
///
/// Fires in `Player::attack` after [`CriticalHitEvent`]. `sweeping` is read back, also when a
/// handler cancels the event.
#[cancellable]
#[derive(Event, Clone)]
pub struct SweepAttackEvent {
    pub player: Arc<Player>,
    pub target: Arc<dyn EntityBase>,
    /// Whether vanilla made the attack sweep.
    pub vanilla_sweep: bool,
    pub sweeping: bool,
}

impl SweepAttackEvent {
    #[must_use]
    pub fn new(player: Arc<Player>, target: Arc<dyn EntityBase>, vanilla_sweep: bool) -> Self {
        Self {
            player,
            target,
            vanilla_sweep,
            sweeping: vanilla_sweep,
            cancelled: false,
        }
    }
}

impl PlayerEvent for SweepAttackEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
