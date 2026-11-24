//! Vanilla `CommandBlock.useWithoutItem` opens the command block only for a player who
//! `canUseGameMasterBlocks`: creative mode and the game master permission level.

#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails on the first missing value"
)]

mod support;

use std::sync::Arc;

use pumpkin_core::block::blocks::command::CommandBlock;
use pumpkin_core::block::entities::command_block::CommandBlockEntity;
use pumpkin_core::block::registry::BlockActionResult;
use pumpkin_core::block::{BlockBehaviour, BlockHitResult, NormalUseArgs};
use pumpkin_core::net::ClientPlatform;
use pumpkin_data::{Block, BlockDirection};
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_protocol::java::server::play::SSetCommandBlock;
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

/// Vanilla `ServerGamePacketListenerImpl.handleSetCommandBlock` changes the command block only
/// for a player who `canUseGameMasterBlocks`.
#[test]
fn only_a_game_master_in_creative_mode_edits_the_command_block() {
    let _serial = support::serial();
    let world = support::world();
    let player = support::player("cmd_block_editor");
    let pos = BlockPos::new(5, 100, 5);
    world.set_block_state(
        &pos,
        Block::COMMAND_BLOCK.default_state.id,
        BlockFlags::NOTIFY_ALL,
    );
    world.add_block_entity(Arc::new(CommandBlockEntity::new(pos, true, false)));
    let ClientPlatform::Java(client) = player.client.as_ref() else {
        panic!("the support players are Java players");
    };

    let edit = |gamemode, permission, command| {
        player.gamemode.store(gamemode);
        player.permission_lvl.store(permission);
        client.handle_set_command_block(
            &player,
            &SSetCommandBlock {
                pos,
                command,
                mode: VarInt(2),
                flags: 0,
            },
        );
        let block_entity = world.get_block_entity(&pos).unwrap();
        let command_block = block_entity
            .as_any()
            .downcast_ref::<CommandBlockEntity>()
            .unwrap();
        command_block.command.lock().unwrap().clone()
    };
    let by_survival_operator = edit(GameMode::Survival, PermissionLvl::Four, "say survival");
    let by_creative_operator = edit(GameMode::Creative, PermissionLvl::Two, "say creative");

    world.remove_block_entity(&pos);
    world.set_block_state(&pos, Block::AIR.default_state.id, BlockFlags::NOTIFY_ALL);
    assert_eq!(by_survival_operator, "");
    assert_eq!(by_creative_operator, "say creative");
}
