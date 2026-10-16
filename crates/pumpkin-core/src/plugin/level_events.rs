//! The `IronPumpkin` fire sites of the `NeoForge` level, block and tick events (`event.level`,
//! `event.level.block`, `event.tick`).
//!
//! Each function checks `has_handlers` before it builds an event, so a fire site with no handler
//! allocates nothing. The upstream code calls these functions with one line.

use std::sync::Arc;

use pumpkin_data::block_properties::{
    BedPart, DoubleBlockHalf, OakDoorLikeProperties, SmallDripleafLikeProperties,
    TallSeagrassLikeProperties, WhiteBedLikeProperties,
};
use pumpkin_data::game_rules::{GameRule, GameRuleValue};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::{Block, BlockDirection, BlockState, BlockStateId};
use pumpkin_util::math::position::BlockPos;

use crate::block::BlockMetadata;
use crate::block::blocks::plant::tall_plant::TallPlantBlock;
use crate::entity::{EntityBase, player::Player};
use crate::plugin::api::events::{
    block::{
        block_break::BlockBreakEvent, block_multi_place::BlockMultiPlaceEvent,
        block_place::BlockPlaceEvent,
    },
    level::{
        BlockEventEntityPlaceEvent, CropGrowEventPost, EntityTickEventPost, EntityTickEventPre,
        GameRuleChangedEvent, NoteBlockEventChange, PistonEventPost, PistonMoveType,
        PlayerTickEventPost, PlayerTickEventPre,
    },
};
use crate::plugin::player_events;
use crate::server::Server;
use crate::world::World;

/// Fires `BlockBreakEvent` before the break of `state` at `pos`. Returns `None` when a handler
/// cancelled it, else whether the block drops items and the experience to drop. The experience
/// is `None` without a handler: `drop_loot` rolls it then.
pub fn block_break(
    world: &Arc<World>,
    cause: Option<&Arc<Player>>,
    block: &'static Block,
    state: &'static BlockState,
    pos: &BlockPos,
    drop: bool,
) -> Option<(bool, Option<u32>)> {
    let Some(server) = world.server.upgrade() else {
        return Some((drop, None));
    };
    let Some(event) = server.plugin_manager.fire_blocking_with(&server, || {
        let tool = cause
            .map(|player| player.inventory().held_item())
            .filter(|item| !item.is_empty());
        let exp = if drop {
            crate::block::experience_to_drop(block, tool.as_ref())
        } else {
            0
        };
        BlockBreakEvent::new(cause.cloned(), block, state, *pos, exp, drop)
    }) else {
        return Some((drop, None));
    };
    (!event.cancelled).then_some((event.drop, Some(event.exp)))
}

/// Fires `BlockPlaceEvent` for a block that `player` places at `pos`, and
/// `BlockMultiPlaceEvent` when the placement also sets a second block. Returns `false` when a
/// handler cancelled one of them.
pub fn player_place(
    server: &Arc<Server>,
    world: &Arc<World>,
    player: &Arc<Player>,
    placed_block: &'static Block,
    placed_against: &'static Block,
    pos: BlockPos,
    state: BlockStateId,
) -> bool {
    let manager = &server.plugin_manager;
    let replaced = || world.get_block_state(&pos);
    if manager
        .fire_blocking_with(server, || {
            BlockPlaceEvent::new(
                player.clone(),
                placed_block,
                placed_against,
                pos,
                true,
                BlockState::from_id(state),
                replaced(),
            )
        })
        .is_some_and(|event| event.cancelled)
    {
        return false;
    }
    if !manager.has_handlers::<BlockMultiPlaceEvent>() {
        return true;
    }
    let Some((part_pos, part_state)) = second_placed_block(world, placed_block, state, &pos) else {
        return true;
    };
    !manager
        .fire_blocking_with(server, || {
            BlockMultiPlaceEvent::new(
                player.clone(),
                world.clone(),
                vec![(pos, state), (part_pos, part_state)],
                vec![
                    (pos, replaced()),
                    (part_pos, world.get_block_state(&part_pos)),
                ],
            )
        })
        .is_some_and(|event| event.cancelled)
}

/// The second block that the `placed` of a bed, a door, a tall plant or a small dripleaf sets
/// after `state` is placed at `pos`, as their `setPlacedBy` in vanilla.
fn second_placed_block(
    world: &World,
    block: &Block,
    state: BlockStateId,
    pos: &BlockPos,
) -> Option<(BlockPos, BlockStateId)> {
    if block.has_tag(&tag::Block::MINECRAFT_BEDS) {
        let foot = WhiteBedLikeProperties::from_state_id(state);
        if foot.part != BedPart::Foot {
            return None;
        }
        let mut head = WhiteBedLikeProperties::default(block);
        head.facing = foot.facing;
        head.part = BedPart::Head;
        return Some((pos.offset(foot.facing.to_offset()), head.to_state_id(block)));
    }
    let upper = pos.up();
    if block.has_tag(&tag::Block::MINECRAFT_DOORS) {
        let mut door = OakDoorLikeProperties::from_state_id(state);
        if door.half != DoubleBlockHalf::Lower {
            return None;
        }
        door.half = DoubleBlockHalf::Upper;
        return Some((upper, door.to_state_id(block)));
    }
    if TallPlantBlock::ids().contains(&block.id) {
        let mut plant = TallSeagrassLikeProperties::from_state_id(state);
        if plant.half != DoubleBlockHalf::Lower {
            return None;
        }
        plant.half = DoubleBlockHalf::Upper;
        return Some((upper, plant.to_state_id(block)));
    }
    if block == &Block::SMALL_DRIPLEAF {
        let lower = SmallDripleafLikeProperties::from_state_id(state);
        if lower.half != DoubleBlockHalf::Lower {
            return None;
        }
        let mut leaf = SmallDripleafLikeProperties::default(block);
        leaf.facing = lower.facing;
        leaf.waterlogged = world.get_block(&upper) == &Block::WATER;
        leaf.half = DoubleBlockHalf::Upper;
        return Some((upper, leaf.to_state_id(block)));
    }
    None
}

/// Fires `BlockEventEntityPlaceEvent` for a block that `entity`, not a player, places at `pos`.
/// Returns `false` when a handler cancelled it.
pub fn entity_place(
    world: &Arc<World>,
    entity: &Arc<dyn EntityBase>,
    pos: BlockPos,
    state: BlockStateId,
) -> bool {
    let Some(server) = world.server.upgrade() else {
        return true;
    };
    !server
        .plugin_manager
        .fire_blocking_with(&server, || {
            BlockEventEntityPlaceEvent::new(
                entity.clone(),
                world.clone(),
                pos,
                BlockState::from_id(state),
                world.get_block_state(&pos),
                world.get_block_state(&pos.down()),
            )
        })
        .is_some_and(|event| event.cancelled)
}

/// Fires `EntityTickEventPre` before `entity` ticks. Returns `false` when a handler cancelled it.
pub fn entity_tick_pre(server: &Arc<Server>, entity: &Arc<dyn EntityBase>) -> bool {
    !server
        .plugin_manager
        .fire_blocking_with(server, || EntityTickEventPre::new(entity.clone()))
        .is_some_and(|event| event.cancelled)
}

/// Fires `EntityTickEventPost` after `entity` ticked.
pub fn entity_tick_post(server: &Arc<Server>, entity: &Arc<dyn EntityBase>) {
    server
        .plugin_manager
        .fire_blocking_with(server, || EntityTickEventPost::new(entity.clone()));
}

/// Fires `PlayerTickEventPre` at the start of the tick of `player`.
pub fn player_tick_pre(player: &Player) {
    player_events::fire(player, PlayerTickEventPre::new);
}

/// Fires `PlayerTickEventPost` at the end of the tick of `player`.
pub fn player_tick_post(player: &Player) {
    player_events::fire(player, PlayerTickEventPost::new);
}

/// Fires `CropGrowEventPost` after the crop at `pos` grew from `original`.
pub fn crop_grow_post(world: &Arc<World>, pos: &BlockPos, original: BlockStateId) {
    let Some(server) = world.server.upgrade() else {
        return;
    };
    server.plugin_manager.fire_blocking_with(&server, || {
        CropGrowEventPost::new(
            world.clone(),
            *pos,
            BlockState::from_id(original),
            world.get_block_state(pos),
        )
    });
}

/// The highest note id of a note block (`NoteBlock.NOTE`).
const MAX_NOTE: u8 = 24;

/// Fires `NoteBlockEventChange` for a tune of the note block at `pos` from `old` to `note`.
/// Returns the note to set, or `None` when a handler cancelled the change.
pub fn note_change(
    world: &Arc<World>,
    pos: &BlockPos,
    state: &'static BlockState,
    old: u8,
    note: u8,
) -> Option<u8> {
    let Some(server) = world.server.upgrade() else {
        return Some(note);
    };
    let Some(event) = server.plugin_manager.fire_blocking_with(&server, || {
        NoteBlockEventChange::new(world.clone(), *pos, state, old, note)
    }) else {
        return Some(note);
    };
    (!event.cancelled).then_some(if event.note <= MAX_NOTE {
        event.note
    } else {
        note
    })
}

/// Fires `PistonEventPost` after the piston at `pos` moved.
pub fn piston_post(
    world: &Arc<World>,
    pos: &BlockPos,
    direction: BlockDirection,
    move_type: PistonMoveType,
) {
    let Some(server) = world.server.upgrade() else {
        return;
    };
    server.plugin_manager.fire_blocking_with(&server, || {
        PistonEventPost::new(
            world.clone(),
            *pos,
            world.get_block_state(pos),
            direction,
            move_type,
        )
    });
}

/// Fires `GameRuleChangedEvent` after `rule` changed to `value`.
pub fn game_rule_changed(
    server: &Arc<Server>,
    rule: &GameRule,
    value: impl FnOnce() -> GameRuleValue<i64, bool>,
) {
    server
        .plugin_manager
        .fire_blocking_with(server, || GameRuleChangedEvent::new(rule.clone(), value()));
}
