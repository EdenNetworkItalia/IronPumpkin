use pumpkin_data::statistic::StatisticCategory;
use pumpkin_macros::{Event, cancellable};
use std::sync::Arc;

use super::PlayerEvent;
use crate::entity::player::Player;

/// An event that occurs when a player's statistic is incremented or set.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerStatisticIncrementEvent {
    /// The player whose statistic changed.
    pub player: Arc<Player>,

    /// The statistic identifier name, `{category:?}:{stat}` of the statistic at fire time.
    pub statistic_id: String,

    /// The change of the statistic: the new total minus the total before the event.
    pub amount: i32,

    /// The category of the statistic. The server stores `value` in the statistic that
    /// `category` and `stat` name, as the last handler left them.
    pub category: StatisticCategory,

    /// The id of the statistic inside its category.
    pub stat: i32,

    /// The new total of the statistic, which the server stores. A reset sets it to 0.
    pub value: i32,
}

impl PlayerStatisticIncrementEvent {
    #[must_use]
    pub fn new(
        player: Arc<Player>,
        category: StatisticCategory,
        stat: i32,
        amount: i32,
        value: i32,
    ) -> Self {
        Self {
            player,
            statistic_id: format!("{category:?}:{stat}"),
            amount,
            category,
            stat,
            value,
            cancelled: false,
        }
    }
}

impl PlayerEvent for PlayerStatisticIncrementEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
