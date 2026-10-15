use std::sync::Arc;

use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::Hand;

use super::PlayerEvent;
use crate::entity::player::Player;
use crate::plugin::api::events::results::InteractionResult;
use crate::world::World;

/// `NeoForge` `ArrowNockEvent`: a player starts to draw a bow.
///
/// Fires in the bow `ItemBehaviour::normal_use` before the draw starts. A cancel, or a set
/// `action`, stops the draw. Pumpkin's item use returns no result, so the value of `action`
/// has no other effect.
#[cancellable]
#[derive(Event, Clone)]
pub struct ArrowNockEvent {
    pub player: Arc<Player>,
    pub bow: ItemStack,
    pub hand: Hand,
    pub world: Arc<World>,
    /// Whether the player has an arrow, or needs none.
    pub has_ammo: bool,
    pub action: Option<InteractionResult>,
}

impl ArrowNockEvent {
    #[must_use]
    pub const fn new(
        player: Arc<Player>,
        bow: ItemStack,
        hand: Hand,
        world: Arc<World>,
        has_ammo: bool,
    ) -> Self {
        Self {
            player,
            bow,
            hand,
            world,
            has_ammo,
            action: None,
            cancelled: false,
        }
    }
}

impl PlayerEvent for ArrowNockEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}

/// `NeoForge` `ArrowLooseEvent`: a player releases a drawn bow.
///
/// Fires in `BowItem::release_bow` before the arrow is shot. `charge` (the ticks the bow was
/// drawn) is read back. A cancel shoots nothing.
#[cancellable]
#[derive(Event, Clone)]
pub struct ArrowLooseEvent {
    pub player: Arc<Player>,
    pub bow: ItemStack,
    pub world: Arc<World>,
    pub has_ammo: bool,
    pub charge: i32,
}

impl ArrowLooseEvent {
    #[must_use]
    pub const fn new(
        player: Arc<Player>,
        bow: ItemStack,
        world: Arc<World>,
        charge: i32,
        has_ammo: bool,
    ) -> Self {
        Self {
            player,
            bow,
            world,
            has_ammo,
            charge,
            cancelled: false,
        }
    }
}

impl PlayerEvent for ArrowLooseEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
