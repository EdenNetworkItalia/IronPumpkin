use crate::block::blocks::fire::FireBlockBase;
use crate::block::blocks::fire::fire::FireBlock;
use crate::entity::player::Player;
use crate::plugin::level::FIRESTARTER_LIGHT;
use crate::plugin::level_events;
use crate::world::{BlockFlags, World};
use pumpkin_data::fluid::Fluid;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::tag::Taggable;
use pumpkin_data::{Block, BlockStateId, tag};
use pumpkin_util::math::position::BlockPos;
use std::sync::Arc;

pub struct Ignition;

impl Ignition {
    /// Lights `block` at `location` itself if it can be lit (campfires, candles, candle
    /// cakes), otherwise places a fire block at `fire_pos`. `player` uses `item`.
    pub fn ignite_block<F>(
        ignite_logic: F,
        world: &Arc<World>,
        location: BlockPos,
        fire_pos: BlockPos,
        block: &Block,
        player: &Player,
        item: &ItemStack,
    ) -> bool
    where
        F: FnOnce(Arc<World>, BlockPos, BlockStateId),
    {
        if *world.get_fluid(&location) != Fluid::EMPTY {
            return false;
        }
        let fire_block = FireBlockBase::get_fire_type(world, &fire_pos);

        let state_id = world.get_block_state_id(&location);

        if let Some(new_state_id) = level_events::tool_modification(
            world,
            &location,
            Some(player),
            item,
            FIRESTARTER_LIGHT,
            || can_be_lit(block, state_id),
        ) {
            ignite_logic(world.clone(), location, new_state_id);
            return true;
        }

        let state_id = FireBlock.get_state_for_position(world, &fire_block, &fire_pos);
        if FireBlockBase::can_place_at(world, &fire_pos) {
            ignite_logic(world.clone(), fire_pos, state_id);
            return true;
        }

        false
    }

    /// A dispenser uses `item` on `block` at `pos`: it places a fire block if it can, otherwise
    /// it lights the block (`FlintAndSteelDispenseItemBehavior`, fire before the tool event).
    pub fn ignite_dispensed(
        world: &Arc<World>,
        pos: BlockPos,
        block: &Block,
        item: &ItemStack,
    ) -> bool {
        if FireBlockBase::can_place_at(world, &pos) {
            let fire_block = FireBlockBase::get_fire_type(world, &pos);
            let state_id = FireBlock.get_state_for_position(world, &fire_block, &pos);
            world.set_block_state(&pos, state_id, BlockFlags::NOTIFY_ALL);
            return true;
        }

        let state_id = world.get_block_state_id(&pos);
        let Some(new_state_id) =
            level_events::tool_modification(world, &pos, None, item, FIRESTARTER_LIGHT, || {
                if *world.get_fluid(&pos) == Fluid::EMPTY {
                    can_be_lit(block, state_id)
                } else {
                    None
                }
            })
        else {
            return false;
        };
        world.set_block_state(&pos, new_state_id, BlockFlags::NOTIFY_ALL);
        true
    }
}

fn can_be_lit(block: &Block, state_id: BlockStateId) -> Option<BlockStateId> {
    // Vanilla only lights the clicked block itself for campfires, candles and candle cakes.
    // See `CampfireBlock::canLight`, `CandleBlock::canLight` and `CandleCakeBlock::canLight`.
    // Everything else that merely carries a `lit` property (furnaces, redstone lamps, copper
    // bulbs, ...) must fall through to placing a fire block instead.
    if !block.has_tag(&tag::Block::MINECRAFT_CAMPFIRES)
        && !block.has_tag(&tag::Block::MINECRAFT_CANDLES)
        && !block.has_tag(&tag::Block::MINECRAFT_CANDLE_CAKES)
    {
        return None;
    }

    let mut props = block.properties(state_id)?.to_props();

    let value = props.iter_mut().find_map(|p| match p {
        pumpkin_data::Property::Lit(lit) => Some(lit),
        _ => None,
    })?;
    if *value {
        return None;
    }

    *value = true;
    Some(block.from_properties(&props).to_state_id(block))
}
