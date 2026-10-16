//! The `IronPumpkin` fire sites of the `NeoForge` interact events: `PlayerInteractEvent`
//! `.LeftClickBlock`, `.RightClickBlock` and `.RightClickItem` (all as [`PlayerInteractEvent`])
//! and [`UseItemOnBlockEvent`], as `CommonHooks` and the `ServerPlayerGameMode` patches of
//! `NeoForge` 26.3 fire them.
//!
//! Each function checks `has_handlers` before it builds an event, so a fire site with no
//! handler allocates nothing and returns the vanilla value.

use std::sync::Arc;

use pumpkin_data::{Block, BlockDirection, item_stack::ItemStack};
use pumpkin_util::{
    Hand,
    math::{position::BlockPos, vector3::Vector3},
};

use super::player_events::listening;
use crate::block::registry::BlockActionResult;
use crate::entity::player::Player;
use crate::plugin::api::events::{
    player::{
        player_interact_event::{LeftClickBlockAction, PlayerInteractEvent},
        use_item_on_block::{UseItemOnBlockEvent, UsePhase},
    },
    results::{InteractionResult, TriState},
};

/// The clicked point of a use on `pos`: the packet carries it relative to the block.
fn hit_vector(pos: BlockPos, cursor: Vector3<f32>) -> Vector3<f64> {
    Vector3::new(
        f64::from(pos.0.x) + f64::from(cursor.x),
        f64::from(pos.0.y) + f64::from(cursor.y),
        f64::from(pos.0.z) + f64::from(cursor.z),
    )
}

/// Fires `LeftClickBlock` for the dig `action` at `pos`. Returns `None` when a handler cancelled
/// it, otherwise its `use_block`.
pub fn left_click_block(
    player: &Arc<Player>,
    pos: BlockPos,
    face: u8,
    action: LeftClickBlockAction,
) -> Option<TriState> {
    let Some(server) = listening::<PlayerInteractEvent>(player) else {
        return Some(TriState::Default);
    };
    let face = BlockDirection::try_from(i32::from(face)).ok();
    let block = player.world().get_block(&pos);
    let mut event = PlayerInteractEvent::left_click_block(player, block, pos, face, action);
    server.plugin_manager.fire_blocking(&server, &mut event);
    (!event.cancelled).then_some(event.use_block)
}

/// Fires `RightClickBlock` for a use of `hand` on `block`, and returns it with the changes of
/// the handlers. `None` when no handler listens.
pub fn right_click_block(
    player: &Arc<Player>,
    hand: Hand,
    block: &'static Block,
    pos: BlockPos,
    face: BlockDirection,
    cursor: Vector3<f32>,
) -> Option<PlayerInteractEvent> {
    let server = listening::<PlayerInteractEvent>(player)?;
    let hit = hit_vector(pos, cursor);
    let mut event = PlayerInteractEvent::right_click_block(player, hand, block, pos, face, hit);
    server.plugin_manager.fire_blocking(&server, &mut event);
    Some(event)
}

/// Fires `RightClickItem` for a use of `hand`, as `CommonHooks.onItemRightClick`. Returns the
/// cancellation result when a handler cancelled it.
pub fn right_click_item(player: &Arc<Player>, hand: Hand) -> Option<InteractionResult> {
    let server = listening::<PlayerInteractEvent>(player)?;
    let mut event = PlayerInteractEvent::right_click_item(player, hand);
    server.plugin_manager.fire_blocking(&server, &mut event);
    event.cancelled.then_some(event.cancellation_result)
}

/// Fires [`UseItemOnBlockEvent`] for `phase`. Returns the cancellation result when a handler
/// cancelled it: the phase does not run.
pub fn use_item_on_block(
    player: &Arc<Player>,
    hand: Hand,
    item: &ItemStack,
    pos: BlockPos,
    face: BlockDirection,
    cursor: Vector3<f32>,
    phase: UsePhase,
) -> Option<InteractionResult> {
    let server = listening::<UseItemOnBlockEvent>(player)?;
    let hit = hit_vector(pos, cursor);
    let mut event =
        UseItemOnBlockEvent::new(player.clone(), hand, item.clone(), pos, face, hit, phase);
    server.plugin_manager.fire_blocking(&server, &mut event);
    event.cancelled.then_some(event.cancellation_result)
}

/// Swings the arm of `player` for the result of a use, as `handleUseItemOn` and
/// `handleUseItem` do in vanilla 26.3: `Success` for the other players, `SuccessServer` for
/// every player.
pub fn swing_for(player: &Player, hand: Hand, result: InteractionResult) {
    match result {
        InteractionResult::Success => player.swing_hand(hand, false),
        InteractionResult::SuccessServer => player.swing_hand(hand, true),
        _ => {}
    }
}

impl From<InteractionResult> for BlockActionResult {
    fn from(result: InteractionResult) -> Self {
        match result {
            InteractionResult::Success => Self::Success,
            InteractionResult::SuccessServer => Self::SuccessServer,
            InteractionResult::Consume => Self::Consume,
            InteractionResult::Fail => Self::Fail,
            InteractionResult::Pass => Self::Pass,
            InteractionResult::TryWithEmptyHand => Self::PassToDefaultBlockAction,
        }
    }
}
