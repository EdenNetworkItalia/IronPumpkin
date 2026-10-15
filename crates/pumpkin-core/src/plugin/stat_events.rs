//! The `IronPumpkin` fire site of `StatAwardEvent` (`PlayerStatisticIncrementEvent`).
//!
//! `NeoForge` fires the event in `StatsCounter.setValue` with the new total of the statistic, so
//! an increment and a reset to 0 both fire it. The server stores `value` as the last handler left
//! it, in the statistic that `category` and `stat` name then.

use std::sync::Arc;

use pumpkin_data::statistic::StatisticCategory;

use crate::entity::player::Player;
use crate::plugin::player::player_statistic_increment::PlayerStatisticIncrementEvent;
use crate::server::Server;

/// The server and the player of a fire site, only when a handler listens.
fn listening(player: &Player) -> Option<(Arc<Server>, Arc<Player>)> {
    let world = player.world();
    let server = world.server.upgrade()?;
    if !server
        .plugin_manager
        .has_handlers::<PlayerStatisticIncrementEvent>()
    {
        return None;
    }
    let player = world.get_player_by_uuid(player.gameprofile.id)?;
    Some((server, player))
}

/// Adds `amount` to a statistic of `player` (`Player::increment_stat`).
pub fn increment_stat(player: &Player, category: StatisticCategory, stat: i32, amount: i32) {
    if let Some(listener) = listening(player) {
        award(player, listener, category, stat, |total| {
            (amount, total.saturating_add(amount))
        });
    } else if let Ok(mut stats) = player.stats.try_lock() {
        stats.increment(category, stat, amount);
    }
}

/// Sets a statistic of `player` to `value` (`Player::set_stat`).
pub fn set_stat(player: &Player, category: StatisticCategory, stat: i32, value: i32) {
    if let Some(listener) = listening(player) {
        award(player, listener, category, stat, |total| {
            (value.saturating_sub(total), value)
        });
    } else if let Ok(mut stats) = player.stats.try_lock() {
        stats.set(category, stat, value);
    }
}

/// Fires the event with the change and the new total that `change` returns for the current
/// total. Like the upstream fire site, a statistic whose lock is taken drops the change.
fn award(
    player: &Player,
    (server, player_arc): (Arc<Server>, Arc<Player>),
    category: StatisticCategory,
    stat: i32,
    change: impl FnOnce(i32) -> (i32, i32),
) {
    let Ok(total) = player
        .stats
        .try_lock()
        .map(|stats| stats.get(category, stat))
    else {
        return;
    };
    let (amount, value) = change(total);
    let mut event = PlayerStatisticIncrementEvent::new(player_arc, category, stat, amount, value);
    server.plugin_manager.fire_blocking(&server, &mut event);
    if !event.cancelled
        && let Ok(mut stats) = player.stats.try_lock()
    {
        stats.set(event.category, event.stat, event.value);
    }
}
