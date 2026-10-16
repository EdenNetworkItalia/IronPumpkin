//! The block events of `NeoForge` (`event.level.BlockEvent`, `event.level.block`) at fire sites
//! where Pumpkin has no event.

use std::sync::Arc;

use pumpkin_data::{BlockDirection, BlockState, item_stack::ItemStack};
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::position::BlockPos;

use crate::entity::{EntityBase, player::Player};
use crate::world::World;

/// An entity that is not a player places a block.
///
/// `NeoForge`: `BlockEvent.EntityPlaceEvent` with a non-player entity, fired by
/// `EventHooks.onBlockPlace` in `EndermanLeaveBlockGoal.tick` before the block changes. A
/// placement by a player fires [`BlockPlaceEvent`](crate::plugin::block::block_place::BlockPlaceEvent).
/// A cancel keeps the block. `NeoForge` 26.3 builds the snapshot at the block below the target, so
/// its `getPos` and `getState` name that block; here `pos` is the target.
#[cancellable]
#[derive(Event, Clone)]
pub struct BlockEventEntityPlaceEvent {
    /// The entity that places the block (`getEntity`).
    pub entity: Arc<dyn EntityBase>,
    pub world: Arc<World>,
    pub pos: BlockPos,
    /// The state that the placement sets (`getPlacedBlock`).
    pub placed_state: &'static BlockState,
    /// The state that the placement replaces (`getBlockSnapshot().getState()`).
    pub replaced_state: &'static BlockState,
    /// The state of the block below `pos` (`getPlacedAgainst`).
    pub placed_against: &'static BlockState,
}

impl BlockEventEntityPlaceEvent {
    #[must_use]
    pub const fn new(
        entity: Arc<dyn EntityBase>,
        world: Arc<World>,
        pos: BlockPos,
        placed_state: &'static BlockState,
        replaced_state: &'static BlockState,
        placed_against: &'static BlockState,
    ) -> Self {
        Self {
            entity,
            world,
            pos,
            placed_state,
            replaced_state,
            placed_against,
            cancelled: false,
        }
    }
}

/// A crop grew one age in its random tick.
///
/// `NeoForge`: `net.neoforged.neoforge.event.level.block.CropGrowEvent.Post`, fired by
/// `CommonHooks.fireCropGrowPost` in `CropBlock.randomTick` after the block changed. It fires
/// where `CropGrowEvent.Pre` (`BlockGrowEvent`) fires: the crops of `CropBlockBase`.
#[derive(Event, Clone)]
pub struct CropGrowEventPost {
    pub world: Arc<World>,
    pub pos: BlockPos,
    /// The state before the growth (`getOriginalState`).
    pub original_state: &'static BlockState,
    /// The state after the growth (`getState`).
    pub state: &'static BlockState,
}

impl CropGrowEventPost {
    #[must_use]
    pub const fn new(
        world: Arc<World>,
        pos: BlockPos,
        original_state: &'static BlockState,
        state: &'static BlockState,
    ) -> Self {
        Self {
            world,
            pos,
            original_state,
            state,
        }
    }
}

/// A player tunes a note block.
///
/// `NeoForge`: `net.neoforged.neoforge.event.level.NoteBlockEvent.Change`, fired by
/// `CommonHooks.onNoteChange` in `NoteBlock.useWithoutItem` before the block changes. `note` is
/// the note id of `getVanillaNoteId`, from 0 to 24, and is settable: a value above 24 keeps the
/// vanilla note. A cancel keeps the note, plays nothing and fails the use.
#[cancellable]
#[derive(Event, Clone)]
pub struct NoteBlockEventChange {
    pub world: Arc<World>,
    pub pos: BlockPos,
    /// The state of the note block before the change.
    pub state: &'static BlockState,
    /// The note id before the change (`getOldNote` and `getOldOctave`).
    pub old_note: u8,
    pub note: u8,
}

impl NoteBlockEventChange {
    #[must_use]
    pub const fn new(
        world: Arc<World>,
        pos: BlockPos,
        state: &'static BlockState,
        old_note: u8,
        note: u8,
    ) -> Self {
        Self {
            world,
            pos,
            state,
            old_note,
            note,
            cancelled: false,
        }
    }
}

/// The move of a piston (`PistonEvent.PistonMoveType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PistonMoveType {
    Extend,
    Retract,
}

/// A piston moved.
///
/// `NeoForge`: `net.neoforged.neoforge.event.level.PistonEvent.Post`, fired by
/// `EventHooks.onPistonMovePost` at the end of `PistonBaseBlock.triggerEvent`, after the blocks
/// moved. `PistonEvent.Pre` is `BlockPistonExtendEvent` or `BlockPistonRetractEvent`.
#[derive(Event, Clone)]
pub struct PistonEventPost {
    pub world: Arc<World>,
    /// The position of the piston base.
    pub pos: BlockPos,
    /// The state of the piston base after the move.
    pub state: &'static BlockState,
    pub direction: BlockDirection,
    pub move_type: PistonMoveType,
}

impl PistonEventPost {
    #[must_use]
    pub const fn new(
        world: Arc<World>,
        pos: BlockPos,
        state: &'static BlockState,
        direction: BlockDirection,
        move_type: PistonMoveType,
    ) -> Self {
        Self {
            world,
            pos,
            state,
            direction,
            move_type,
        }
    }
}

/// A fluid source counts toward a new source next to it.
///
/// `NeoForge`: `net.neoforged.neoforge.event.level.block.CreateFluidSourceEvent`, fired by
/// `EventHooks.canCreateFluidSource` in `FlowingFluid.getNewLiquid` for each source next to a
/// flowing fluid; the flowing fluid becomes a source when two of them can convert. `can_convert`
/// is settable and starts at `vanilla_result`, the source conversion game rule of the fluid.
/// Pumpkin checks a new source also when the fluid spreads, so the event fires more often than in
/// vanilla.
#[derive(Event, Clone)]
pub struct CreateFluidSourceEvent {
    pub world: Arc<World>,
    /// The position of the source (`getPos`).
    pub pos: BlockPos,
    /// The state of the source (`getState`).
    pub state: &'static BlockState,
    /// `getVanillaResult`.
    pub vanilla_result: bool,
    /// `canConvert`.
    pub can_convert: bool,
}

impl CreateFluidSourceEvent {
    #[must_use]
    pub const fn new(
        world: Arc<World>,
        pos: BlockPos,
        state: &'static BlockState,
        vanilla_result: bool,
    ) -> Self {
        Self {
            world,
            pos,
            state,
            vanilla_result,
            can_convert: vanilla_result,
        }
    }
}

/// The `ItemAbility` of `NeoForge` that lights a block with a fire starter (`firestarter_light`).
pub const FIRESTARTER_LIGHT: &str = "firestarter_light";

/// The `ItemAbility` of `NeoForge` that stops the growth of a plant with shears (`shears_trim`).
pub const SHEARS_TRIM: &str = "shears_trim";

/// A tool changes the state of a block.
///
/// `NeoForge`: `BlockEvent.BlockToolModificationEvent`, fired by `EventHooks.onToolUse` in
/// `IBlockStateExtension.getToolModifiedState` for each use of a fire starter on a block
/// ([`FIRESTARTER_LIGHT`]: flint and steel, fire charge and a dispenser of flint and steel) and
/// of shears ([`SHEARS_TRIM`]). `NeoForge` 26.3 fires it for no other tool: axes, hoes and
/// shovels use the data-driven block transformers. `final_state` starts at `state` and is
/// settable; `None` means no change. When the last handler leaves `state`, the vanilla change
/// applies. A cancel means no change: a fire starter places fire next to the block instead.
#[cancellable]
#[derive(Event, Clone)]
pub struct BlockEventBlockToolModificationEvent {
    pub world: Arc<World>,
    pub pos: BlockPos,
    /// The player that uses the tool, `None` for a dispenser (`getPlayer`).
    pub player: Option<Arc<Player>>,
    /// The tool (`getHeldItemStack`).
    pub held_item: ItemStack,
    /// The ability that the tool performs (`getItemAbility`), such as [`FIRESTARTER_LIGHT`].
    pub item_ability: &'static str,
    /// Always `false`: the server never simulates a use (`isSimulated`).
    pub simulated: bool,
    /// The state before the use (`getState`).
    pub state: &'static BlockState,
    /// The state after the use (`getFinalState`).
    pub final_state: Option<&'static BlockState>,
}

impl BlockEventBlockToolModificationEvent {
    #[must_use]
    pub const fn new(
        world: Arc<World>,
        pos: BlockPos,
        player: Option<Arc<Player>>,
        held_item: ItemStack,
        item_ability: &'static str,
        state: &'static BlockState,
    ) -> Self {
        Self {
            world,
            pos,
            player,
            held_item,
            item_ability,
            simulated: false,
            state,
            final_state: Some(state),
            cancelled: false,
        }
    }
}
