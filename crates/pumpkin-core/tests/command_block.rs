//! Vanilla `CommandBlock.useWithoutItem` opens the command block only for a player who
//! `canUseGameMasterBlocks`: creative mode and the game master permission level.

mod support;

use std::sync::Arc;

use pumpkin_core::block::blocks::command::CommandBlock;
use pumpkin_core::block::entities::command_block::CommandBlockEntity;
use pumpkin_core::block::registry::BlockActionResult;
use pumpkin_core::block::{BlockBehaviour, BlockHitResult, NormalUseArgs};
use pumpkin_data::{Block, BlockDirection};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};
use pumpkin_util::{GameMode, PermissionLvl};
use pumpkin_world::world::BlockFlags;

#[test]
fn only_a_game_master_in_creative_mode_opens_the_command_block() {
    let _serial = support::serial();
    let world = support::world();
    let player = support::player("cmd_block_user");
    let pos = BlockPos::new(3, 100, 3);
    world.set_block_state(
        &pos,
        Block::COMMAND_BLOCK.default_state.id,
        BlockFlags::NOTIFY_ALL,
    );
    world.add_block_entity(Arc::new(CommandBlockEntity::new(pos, true, false)));

    let open = |gamemode, permission| {
        player.gamemode.store(gamemode);
        player.permission_lvl.store(permission);
        CommandBlock.normal_use(NormalUseArgs {
            server: support::server(),
            world: &world,
            block: &Block::COMMAND_BLOCK,
            position: &pos,
            player: &player,
            hit: &BlockHitResult {
                face: &BlockDirection::Up,
                cursor_pos: &Vector3::new(0.5, 1.0, 0.5),
            },
        })
    };
    let survival_operator = open(GameMode::Survival, PermissionLvl::Four);
    let creative_player = open(GameMode::Creative, PermissionLvl::Zero);
    let creative_operator = open(GameMode::Creative, PermissionLvl::Two);

    world.remove_block_entity(&pos);
    world.set_block_state(&pos, Block::AIR.default_state.id, BlockFlags::NOTIFY_ALL);
    assert!(matches!(survival_operator, BlockActionResult::Pass));
    assert!(matches!(creative_player, BlockActionResult::Pass));
    assert!(matches!(
        creative_operator,
        BlockActionResult::SuccessServer
    ));
}
