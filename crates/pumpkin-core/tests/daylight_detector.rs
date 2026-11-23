//! The daylight detector block entity ticks on a snapshot of the block entities, so its tick can
//! run after the detector is gone, as vanilla `BoundTickingBlockEntity` guards with
//! `BlockEntityType.isValid`.

mod support;

use pumpkin_core::block::blocks::redstone::daylight_detector::DaylightDetectorBlock;
use pumpkin_data::Block;
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::world::BlockFlags;

#[test]
fn a_signal_update_after_the_detector_is_gone_does_nothing() {
    let _serial = support::serial();
    let world = support::world();
    let pos = BlockPos::new(0, 100, 0);
    world.set_block_state(&pos, Block::AIR.default_state.id, BlockFlags::NOTIFY_ALL);
    DaylightDetectorBlock::update_signal_strength(&world, &pos);
    assert_eq!(world.get_block_state(&pos).id, Block::AIR.default_state.id);
}
