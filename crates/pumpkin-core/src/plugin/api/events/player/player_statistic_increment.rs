use pumpkin_data::statistic::StatisticCategory;
use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// An event that occurs when a player's statistic is incremented.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerStatisticIncrementEvent {
    /// The player whose statistic changed.
    pub player: Arc<Player>,

    /// The statistic identifier name, `{category:?}:{stat}` of the statistic at fire time.
    pub statistic_id: String,

    /// The amount incremented.
    pub amount: i32,

    /// The category of the statistic. The server adds `amount` to the statistic that `category`
    /// and `stat` name, as the last handler left them.
    pub category: StatisticCategory,

    /// The id of the statistic inside its category.
    pub stat: i32,
}

impl PlayerStatisticIncrementEvent {
    #[must_use]
    pub fn new(player: Arc<Player>, category: StatisticCategory, stat: i32, amount: i32) -> Self {
        Self {
            player,
            statistic_id: format!("{category:?}:{stat}"),
            amount,
            category,
            stat,
            cancelled: false,
        }
    }
}

impl PlayerEvent for PlayerStatisticIncrementEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
