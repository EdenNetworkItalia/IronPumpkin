use std::sync::Arc;

use pumpkin_macros::Event;
use pumpkin_protocol::java::client::play::MerchantOffer;

use super::PlayerEvent;
use crate::entity::{EntityBase, player::Player};

/// `NeoForge` `TradeWithVillagerEvent`: a player completed a trade with a villager or a
/// wandering trader.
///
/// Fires for each trade of a slot click after the click released the screen-handler lock of the
/// trade screen. The merchant counted the offer use in `complete_trade`, and `offer` is a copy
/// made then.
#[derive(Event, Clone)]
pub struct TradeWithVillagerEvent {
    pub player: Arc<Player>,
    pub offer: MerchantOffer,
    pub villager: Arc<dyn EntityBase>,
}

impl TradeWithVillagerEvent {
    #[must_use]
    pub fn new(player: Arc<Player>, offer: MerchantOffer, villager: Arc<dyn EntityBase>) -> Self {
        Self {
            player,
            offer,
            villager,
        }
    }
}

impl PlayerEvent for TradeWithVillagerEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
