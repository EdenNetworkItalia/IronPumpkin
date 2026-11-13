use super::{
    Arc, Block, BlockFlags, BlockPos, FIRESTARTER_LIGHT, FireBlock, FireBlockBase, Fluid, Ignition,
    ItemStack, World, can_be_lit, level_events,
};

impl Ignition {
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
