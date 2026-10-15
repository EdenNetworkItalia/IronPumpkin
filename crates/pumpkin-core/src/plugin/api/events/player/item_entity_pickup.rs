use std::sync::Arc;

use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};

use super::PlayerEvent;
use crate::entity::{EntityBase, player::Player};
use crate::plugin::api::events::results::TriState;

/// `NeoForge` `ItemEntityPickupEvent.Pre`: a player touches an item entity.
///
/// Fires in `ItemEntity::on_player_collision` before any check. `can_pickup` is read back:
/// `False` stops the pickup, `True` ignores the pickup delay, `Default` lets vanilla decide.
#[derive(Event, Clone)]
pub struct ItemEntityPickupEventPre {
    pub player: Arc<Player>,
    pub item_entity: Arc<dyn EntityBase>,
    pub can_pickup: TriState,
}

impl ItemEntityPickupEventPre {
    #[must_use]
    pub fn new(player: Arc<Player>, item_entity: Arc<dyn EntityBase>) -> Self {
        Self {
            player,
            item_entity,
            can_pickup: TriState::Default,
        }
    }
}

impl PlayerEvent for ItemEntityPickupEventPre {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}

/// `NeoForge` `ItemEntityPickupEvent.Post`: a player picked up some or all of an item entity.
///
/// Fires in `ItemEntity::on_player_collision` after the items moved to the inventory.
#[derive(Event, Clone)]
pub struct ItemEntityPickupEventPost {
    pub player: Arc<Player>,
    pub item_entity: Arc<dyn EntityBase>,
    /// The stack of the item entity before the pickup.
    pub original_stack: ItemStack,
    /// The stack left in the item entity, empty when the player took all of it.
    pub current_stack: ItemStack,
}

impl ItemEntityPickupEventPost {
    #[must_use]
    pub const fn new(
        player: Arc<Player>,
        item_entity: Arc<dyn EntityBase>,
        original_stack: ItemStack,
        current_stack: ItemStack,
    ) -> Self {
        Self {
            player,
            item_entity,
            original_stack,
            current_stack,
        }
    }
}

impl PlayerEvent for ItemEntityPickupEventPost {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}

/// `NeoForge` `PlayerXpEvent.PickupXp`: a player picks up an experience orb.
///
/// Fires in `ExperienceOrbEntity::on_player_collision` when the pickup delay allows it, before
/// the orb is taken. A cancel leaves the orb.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerXpEventPickupXp {
    pub player: Arc<Player>,
    pub orb: Arc<dyn EntityBase>,
    /// The experience of the orb.
    pub amount: u32,
}

impl PlayerXpEventPickupXp {
    #[must_use]
    pub fn new(player: Arc<Player>, orb: Arc<dyn EntityBase>, amount: u32) -> Self {
        Self {
            player,
            orb,
            amount,
            cancelled: false,
        }
    }
}

impl PlayerEvent for PlayerXpEventPickupXp {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
