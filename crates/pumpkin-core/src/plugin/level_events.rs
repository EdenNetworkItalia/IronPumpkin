//! The `IronPumpkin` fire sites of the `NeoForge` level, block and tick events (`event.level`,
//! `event.level.block`, `event.tick`).
//!
//! Each function checks `has_handlers` before it builds an event, so a fire site with no handler
//! allocates nothing. The upstream code calls these functions with one line.

use std::sync::Arc;

use pumpkin_data::biome::Spawner;
use pumpkin_data::block_properties::{
    BedPart, DoubleBlockHalf, OakDoorLikeProperties, SmallDripleafLikeProperties,
    TallSeagrassLikeProperties, WhiteBedLikeProperties,
};
use pumpkin_data::entity::MobCategory;
use pumpkin_data::game_rules::{GameRule, GameRuleValue};
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::{Block, BlockDirection, BlockState, BlockStateId};
use pumpkin_util::math::{position::BlockPos, vector2::Vector2, vector3::Vector3};
use pumpkin_world::level::LoadedChunkChange;

use crate::block::BlockMetadata;
use crate::block::blocks::plant::tall_plant::TallPlantBlock;
use crate::entity::{EntityBase, player::Player};
use crate::plugin::api::events::{
    block::{
        block_break::BlockBreakEvent, block_multi_place::BlockMultiPlaceEvent,
        block_place::BlockPlaceEvent,
    },
    level::{
        BlockEventBlockToolModificationEvent, BlockEventEntityPlaceEvent, ChunkWatchEventUnWatch,
        CreateFluidSourceEvent, CropGrowEventPost, EntityTickEventPost, EntityTickEventPre,
        ExplosionEventDetonate, ExplosionKnockbackEvent, GameRuleChangedEvent,
        LevelEventPotentialSpawns, NoteBlockEventChange, PistonEventPost, PistonMoveType,
        PlayerTickEventPost, PlayerTickEventPre,
    },
    world::chunk_load::ChunkLoad,
};
use crate::plugin::player_events;
use crate::server::Server;
use crate::world::World;
use crate::world::explosion::Blocks;

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
    if !player_place_one(
        server,
        world,
        player,
        placed_block,
        placed_against,
        pos,
        state,
    ) {
        return false;
    }
    let manager = &server.plugin_manager;
    let replaced = || world.get_block_state(&pos);
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

/// Fires `BlockPlaceEvent` alone, for a placement of one block (`EventHooks.onBlockPlace`).
/// Returns `false` when a handler cancelled it.
pub fn player_place_one(
    server: &Arc<Server>,
    world: &Arc<World>,
    player: &Arc<Player>,
    placed_block: &'static Block,
    placed_against: &'static Block,
    pos: BlockPos,
    state: BlockStateId,
) -> bool {
    !server
        .plugin_manager
        .fire_blocking_with(server, || {
            BlockPlaceEvent::new(
                player.clone(),
                placed_block,
                placed_against,
                pos,
                true,
                BlockState::from_id(state),
                world.get_block_state(&pos),
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

/// Which tick events have a handler. The world tick reads it once, before its parallel loops,
/// so an entity or a player with no tick handler costs no lookup.
#[derive(Clone, Copy)]
pub struct TickEventHandlers {
    entity_pre: bool,
    entity_post: bool,
    player_pre: bool,
    player_post: bool,
}

impl TickEventHandlers {
    #[must_use]
    pub fn read(server: &Server) -> Self {
        let plugins = &server.plugin_manager;
        Self {
            entity_pre: plugins.has_handlers::<EntityTickEventPre>(),
            entity_post: plugins.has_handlers::<EntityTickEventPost>(),
            player_pre: plugins.has_handlers::<PlayerTickEventPre>(),
            player_post: plugins.has_handlers::<PlayerTickEventPost>(),
        }
    }
}

/// Fires `EntityTickEventPre` before `entity` ticks. Returns `false` when a handler cancelled it.
pub fn entity_tick_pre(
    server: &Arc<Server>,
    entity: &Arc<dyn EntityBase>,
    handlers: TickEventHandlers,
) -> bool {
    !handlers.entity_pre
        || !server
            .plugin_manager
            .fire_blocking_with(server, || EntityTickEventPre::new(entity.clone()))
            .is_some_and(|event| event.cancelled)
}

/// Fires `EntityTickEventPost` after `entity` ticked.
pub fn entity_tick_post(
    server: &Arc<Server>,
    entity: &Arc<dyn EntityBase>,
    handlers: TickEventHandlers,
) {
    if handlers.entity_post {
        server
            .plugin_manager
            .fire_blocking_with(server, || EntityTickEventPost::new(entity.clone()));
    }
}

/// Fires `PlayerTickEventPre` at the start of the tick of `player`.
pub fn player_tick_pre(player: &Player, handlers: TickEventHandlers) {
    if handlers.player_pre {
        player_events::fire(player, PlayerTickEventPre::new);
    }
}

/// Fires `PlayerTickEventPost` at the end of the tick of `player`.
pub fn player_tick_post(player: &Player, handlers: TickEventHandlers) {
    if handlers.player_post {
        player_events::fire(player, PlayerTickEventPost::new);
    }
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

/// Whether a handler listens to `ExplosionEventDetonate`: an explosion that keeps the blocks
/// finds them only for it.
pub fn listens_to_explosion_detonate(world: &World) -> bool {
    world.server.upgrade().is_some_and(|server| {
        server
            .plugin_manager
            .has_handlers::<ExplosionEventDetonate>()
    })
}

/// Fires `ExplosionEventDetonate` and keeps the blocks and the entities that the last handler
/// left in the lists.
pub fn explosion_detonate(
    world: &Arc<World>,
    center: Vector3<f64>,
    power: f32,
    blocks: &mut Blocks,
    entities: &mut Vec<Arc<dyn EntityBase>>,
) {
    let Some(server) = world.server.upgrade() else {
        return;
    };
    let Some(event) = server.plugin_manager.fire_blocking_with(&server, || {
        ExplosionEventDetonate::new(
            world.clone(),
            center,
            power,
            blocks.keys().copied().collect(),
            entities.clone(),
        )
    }) else {
        return;
    };
    *entities = event.affected_entities;
    *blocks = event
        .affected_blocks
        .into_iter()
        .map(|pos| {
            let entry = blocks
                .get(&pos)
                .copied()
                .unwrap_or_else(|| world.get_block_and_state(&pos));
            (pos, entry)
        })
        .collect();
}

/// Fires `ExplosionKnockbackEvent` for the push of `entity` and returns the knockback that the
/// last handler left.
pub fn explosion_knockback(
    world: &Arc<World>,
    center: Vector3<f64>,
    power: f32,
    entity: &Arc<dyn EntityBase>,
    knockback: Vector3<f64>,
    blocks: &Blocks,
) -> Vector3<f64> {
    let Some(server) = world.server.upgrade() else {
        return knockback;
    };
    server
        .plugin_manager
        .fire_blocking_with(&server, || {
            ExplosionKnockbackEvent::new(
                world.clone(),
                center,
                power,
                entity.clone(),
                knockback,
                blocks.keys().copied().collect(),
            )
        })
        .map_or(knockback, |event| event.knockback)
}

/// Fires `ChunkWatchEventUnWatch` for each of `chunks` that `player` no longer watches in
/// `world`.
pub fn chunk_unwatch(
    player: &Arc<Player>,
    world: &Arc<World>,
    chunks: impl IntoIterator<Item = Vector2<i32>>,
) {
    let Some(server) = world.server.upgrade() else {
        return;
    };
    if !server
        .plugin_manager
        .has_handlers::<ChunkWatchEventUnWatch>()
    {
        return;
    }
    for pos in chunks {
        let mut event = ChunkWatchEventUnWatch::new(player.clone(), world.clone(), pos);
        server.plugin_manager.fire_blocking(&server, &mut event);
    }
}

/// Fires `ChunkLoad` and `ChunkUnloadEvent` for the chunks that joined or left the loaded chunks
/// of `world` since the last world tick.
pub fn chunk_changes(world: &Arc<World>, changes: &[LoadedChunkChange]) {
    let Some(server) = world.server.upgrade() else {
        return;
    };
    let listens_to_load = server.plugin_manager.has_handlers::<ChunkLoad>();
    for change in changes {
        match *change {
            LoadedChunkChange::Loaded(pos) => {
                // A chunk that unloaded again in the same tick fires no load.
                let chunk = listens_to_load
                    .then(|| world.level.loaded_chunks.get(&pos).map(|c| c.clone()))
                    .flatten();
                if let Some(chunk) = chunk {
                    let mut event = ChunkLoad::new(world.clone(), chunk);
                    server.plugin_manager.fire_blocking(&server, &mut event);
                }
            }
            LoadedChunkChange::Unloaded(pos) => world.unload_chunk(pos),
        }
    }
}

/// Fires `CreateFluidSourceEvent` for the source at `pos` and returns whether it counts toward a
/// new source. Without a handler it returns `vanilla`.
pub fn create_fluid_source(
    world: &Arc<World>,
    pos: &BlockPos,
    state: BlockStateId,
    vanilla: bool,
) -> bool {
    let Some(server) = world.server.upgrade() else {
        return vanilla;
    };
    server
        .plugin_manager
        .fire_blocking_with(&server, || {
            CreateFluidSourceEvent::new(world.clone(), *pos, BlockState::from_id(state), vanilla)
        })
        .map_or(vanilla, |event| event.can_convert)
}

/// Fires `BlockEventBlockToolModificationEvent` for a use of `item` with `ability` on the block
/// at `pos`. Returns the state to set, or `None` for no change; `vanilla` gives the vanilla
/// change when the handlers leave the state.
pub fn tool_modification(
    world: &Arc<World>,
    pos: &BlockPos,
    player: Option<&Player>,
    item: &ItemStack,
    ability: &'static str,
    vanilla: impl FnOnce() -> Option<BlockStateId>,
) -> Option<BlockStateId> {
    let Some(server) = world.server.upgrade() else {
        return vanilla();
    };
    let state = world.get_block_state(pos);
    let Some(event) = server.plugin_manager.fire_blocking_with(&server, || {
        let player = player.and_then(|player| world.get_player_by_uuid(player.gameprofile.id));
        BlockEventBlockToolModificationEvent::new(
            world.clone(),
            *pos,
            player,
            item.clone(),
            ability,
            state,
        )
    }) else {
        return vanilla();
    };
    if event.cancelled {
        return None;
    }
    match event.final_state {
        Some(final_state) if final_state.id == state.id => vanilla(),
        final_state => final_state.map(|final_state| final_state.id),
    }
}

/// Fires `LevelEventPotentialSpawns` for the spawn list of `category` at `pos`. Returns `None`
/// without a handler, else the list that the last handler left, empty when it cancelled.
pub fn potential_spawns(
    world: &Arc<World>,
    category: &'static MobCategory,
    pos: &BlockPos,
    spawns: &'static [Spawner],
) -> Option<Vec<&'static Spawner>> {
    let server = world.server.upgrade()?;
    let event = server.plugin_manager.fire_blocking_with(&server, || {
        LevelEventPotentialSpawns::new(world.clone(), category, *pos, spawns.iter().collect())
    })?;
    Some(if event.cancelled {
        Vec::new()
    } else {
        event.spawns
    })
}
