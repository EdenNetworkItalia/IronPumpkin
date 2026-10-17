use std::sync::Arc;

use crate::entity::player::Player;
use crate::plugin::api::events::results::{InteractionResult, TriState};
use pumpkin_data::{Block, BlockDirection, item_stack::ItemStack};
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::Hand;
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};

use super::PlayerEvent;

/// Event that is triggered when a player interacts with a block or air.
///
/// This event includes information about the player, the action performed,
/// the item in the player's hand, the block interacted with, and the position clicked (if any).
/// It can be cancelled to prevent the default interaction behavior.
#[cancellable]
#[derive(Event, Clone)]
pub struct PlayerInteractEvent {
    /// The player who performed the interaction.
    pub player: Arc<Player>,

    /// The type of action the player performed.
    pub action: InteractAction,

    /// The position of the block that was clicked, if any.
    pub clicked_pos: Option<BlockPos>,

    /// The block that was interacted with.
    pub block: &'static Block,

    // IronPumpkin: the fields of `NeoForge` `PlayerInteractEvent.LeftClickBlock`,
    // `RightClickBlock` and `RightClickItem`.
    /// The hand of the interaction. A left click always uses the main hand, `Hand::Right`.
    pub hand: Hand,

    /// The clicked face of `block`, for a click on a block.
    pub face: Option<BlockDirection>,

    /// The clicked point of a right click on a block, in world coordinates.
    pub hit_vector: Option<Vector3<f64>>,

    /// The dig action of a left click on a block.
    pub left_click_action: Option<LeftClickBlockAction>,

    /// Whether the block reacts. On a right click, `False` skips the block use and `True` runs
    /// it also for a sneaking player. At the start of a dig, `False` skips the block attack.
    pub use_block: TriState,

    /// Whether the item reacts to a right click on a block. `False` skips the item use and
    /// `True` runs it also for an empty hand. A left click ignores it, as in `NeoForge`.
    pub use_item: TriState,

    /// The result of a cancelled right click, in place of the block or item use.
    /// `Success` swings the arm for the other players, `SuccessServer` for every player.
    pub cancellation_result: InteractionResult,
}

impl PlayerInteractEvent {
    /// Creates a new instance of `PlayerInteractEvent`.
    ///
    /// # Arguments
    ///
    /// - `player`: A reference-counted pointer to the player who triggered the event.
    /// - `action`: The type of interaction performed.
    /// - `block`: The block that was interacted with.
    /// - `clicked_pos`: The optional position of the block that was clicked.
    ///
    /// # Returns
    ///
    /// A new `PlayerInteractEvent` instance with the specified data.
    pub fn new(
        player: &Arc<Player>,
        action: InteractAction,
        block: &'static Block,
        clicked_pos: Option<BlockPos>,
    ) -> Self {
        Self {
            player: Arc::clone(player),
            action,
            block,
            clicked_pos,
            hand: Hand::Right,
            face: None,
            hit_vector: None,
            left_click_action: None,
            use_block: TriState::Default,
            use_item: TriState::Default,
            cancellation_result: InteractionResult::Pass,
            cancelled: false,
        }
    }

    /// `NeoForge` `PlayerInteractEvent.LeftClickBlock`: a dig action on `block`.
    #[must_use]
    pub fn left_click_block(
        player: &Arc<Player>,
        block: &'static Block,
        pos: BlockPos,
        face: Option<BlockDirection>,
        action: LeftClickBlockAction,
    ) -> Self {
        let mut event = Self::new(player, InteractAction::LeftClickBlock, block, Some(pos));
        event.face = face;
        event.left_click_action = Some(action);
        event
    }

    /// `NeoForge` `PlayerInteractEvent.RightClickBlock`: a use of `hand` on `block`.
    #[must_use]
    pub fn right_click_block(
        player: &Arc<Player>,
        hand: Hand,
        block: &'static Block,
        pos: BlockPos,
        face: BlockDirection,
        hit_vector: Vector3<f64>,
    ) -> Self {
        let mut event = Self::new(player, InteractAction::RightClickBlock, block, Some(pos));
        event.hand = hand;
        event.face = Some(face);
        event.hit_vector = Some(hit_vector);
        event
    }

    /// `NeoForge` `PlayerInteractEvent.RightClickItem`: a use of the item in `hand`.
    #[must_use]
    pub fn right_click_item(player: &Arc<Player>, hand: Hand) -> Self {
        let mut event = Self::new(player, InteractAction::RightClickAir, &Block::AIR, None);
        event.hand = hand;
        event
    }

    /// The stack in `hand` now, as `NeoForge` `getItemStack`.
    #[must_use]
    pub fn item_stack(&self) -> ItemStack {
        self.player.inventory().get_stack_in_hand(self.hand)
    }
}

/// The dig action of a left click on a block, as `NeoForge` `LeftClickBlock.Action`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeftClickBlockAction {
    /// The dig starts.
    Start,
    /// The dig ends with a break.
    Stop,
    /// The dig ends without a break.
    Abort,
}

/// Enum representing possible player interaction actions.
#[derive(Clone, PartialEq, Eq)]
pub enum InteractAction {
    /// Left-clicking a block
    LeftClickBlock,

    /// Left-clicking the air
    LeftClickAir,

    /// Right-clicking the air
    RightClickAir,

    /// Right-clicking a block
    RightClickBlock,
}

impl InteractAction {
    /// Gets whether this action is a result of a left click.
    #[must_use]
    #[inline]
    pub fn is_left_click(&self) -> bool {
        Self::LeftClickAir.eq(self) || Self::LeftClickBlock.eq(self)
    }

    /// Gets whether this action is a result of a right click.
    #[must_use]
    #[inline]
    pub fn is_right_click(&self) -> bool {
        Self::RightClickAir.eq(self) || Self::RightClickBlock.eq(self)
    }
}

impl PlayerEvent for PlayerInteractEvent {
    fn get_player(&self) -> &Arc<Player> {
        &self.player
    }
}
