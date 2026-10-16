//! `GameRuleChangedEvent` of `NeoForge`. Pumpkin has no event on a game rule change.

use pumpkin_data::game_rules::{GameRule, GameRuleValue};
use pumpkin_macros::Event;

/// A game rule changed.
///
/// `NeoForge`: `net.neoforged.neoforge.event.level.GameRuleChangedEvent`, fired by
/// `EventHooks.onGameRuleChanged` in `MinecraftServer.onGameRuleChanged` after the change. It
/// fires for the `/gamerule` command, which changes the rules of the server, and for
/// `World::set_game_rule`, which changes the rules of one world.
#[derive(Event)]
pub struct GameRuleChangedEvent {
    pub rule: GameRule,
    /// The new value (`getNewValue`).
    pub value: GameRuleValue<i64, bool>,
}

impl GameRuleChangedEvent {
    #[must_use]
    pub const fn new(rule: GameRule, value: GameRuleValue<i64, bool>) -> Self {
        Self { rule, value }
    }
}
