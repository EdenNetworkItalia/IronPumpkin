//! The `NeoForge` attributes that `IronPumpkin` registers: `neoforge:swim_speed` scales the swim
//! speed of a mob in water (`LivingEntity.travelInWater` of `NeoForge`), and
//! `neoforge:creative_flight` lets a survival player fly (`IPlayerExtension#mayFly`).
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails on the first missing value"
)]

mod support;

use std::time::{Duration, Instant};

use pumpkin_core::{
    entity::{
        Entity, EntityBase,
        attributes::{Modifier, ModifierOperation, send_attribute_updates_for_living},
        living::LivingEntity,
        player::Player,
    },
    net::{ClientPlatform, java::neoforge::NegotiatedState},
};
use pumpkin_data::{Block, attributes::Attributes, entity::EntityType, packet::CURRENT_MC_VERSION};
use pumpkin_protocol::{
    java::{
        client::play::CPlayerAbilities, packet_decoder::TCPNetworkDecoder,
        server::play::SPlayerAbilities,
    },
    packet::MultiVersionJavaPacket,
};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};
use pumpkin_world::world::BlockFlags;

/// Loads the chunk of `pos` for the rest of the process and sets `pos` and the block above it to
/// water.
fn water_column(pos: BlockPos) {
    let world = support::world();
    let chunk = pos.chunk_position();
    {
        let mut loading = world.level.chunk_loading.lock().unwrap();
        loading.add_force_ticket(chunk);
        loading.send_change();
    };
    let deadline = Instant::now() + support::SLOW_RUNNER;
    while !world.level.is_chunk_loaded(&chunk) {
        assert!(Instant::now() < deadline, "the chunk of {pos} did not load");
        std::thread::sleep(Duration::from_millis(10));
    }
    for pos in [pos, pos.up()] {
        world.set_block_state(&pos, Block::WATER.default_state.id, BlockFlags::empty());
    }
}

/// A zombie without AI goals in a water column at `pos`, swimming forward for one tick. Returns
/// its forward velocity.
fn forward_swim_velocity(pos: BlockPos, swim_speed_bonus: Option<f64>) -> f64 {
    water_column(pos);
    let world = support::world();
    let center = Vector3::new(
        f64::from(pos.0.x) + 0.5,
        f64::from(pos.0.y),
        f64::from(pos.0.z) + 0.5,
    );
    let zombie = LivingEntity::new(Entity::new(world, center, &EntityType::ZOMBIE));
    if let Some(bonus) = swim_speed_bonus {
        zombie.update_attribute(&Attributes::NEOFORGE_SWIM_SPEED, |instance| {
            instance.add_or_replace_modifier(Modifier {
                id: "test:fins".to_owned(),
                amount: bonus,
                operation: ModifierOperation::MultiplyBase,
            });
        });
    }
    zombie.movement_input.store(Vector3::new(0.0, 0.0, 1.0));
    let _guard = support::RUNTIME.enter();
    zombie.tick(&zombie, support::server());
    assert!(
        zombie
            .entity
            .touching_water
            .load(std::sync::atomic::Ordering::SeqCst),
        "the zombie at {pos} is not in water"
    );
    zombie.entity.velocity.load().z
}

#[test]
fn swim_speed_scales_the_swim_speed_of_a_mob_in_water() {
    let plain = forward_swim_velocity(BlockPos::new(3000, 120, 3000), None);
    let fast = forward_swim_velocity(BlockPos::new(3004, 120, 3000), Some(1.0));
    assert!(plain > 0.0, "{plain}");
    // A multiply-base modifier of 1 doubles the attribute, and the swim speed with it.
    assert!((fast / plain - 2.0).abs() < 1e-9, "{plain} {fast}");
}

fn java_client(player: &Player) -> &pumpkin_core::net::java::JavaClient {
    let ClientPlatform::Java(client) = player.client.as_ref() else {
        panic!("the support players are Java players");
    };
    client
}

fn request_flight(player: &std::sync::Arc<Player>) {
    // Bit 1 (0x02) is flying.
    java_client(player).handle_player_abilities(
        player,
        &SPlayerAbilities::new(0x02, None, None),
        support::server(),
    );
}

fn flying(player: &Player) -> bool {
    player.abilities.lock().unwrap().flying
}

/// The flags of the next abilities packet that the client end of `peer` receives.
fn next_abilities_flags(peer: tokio::net::TcpStream) -> i8 {
    support::RUNTIME.block_on(async {
        let mut decoder = TCPNetworkDecoder::new(tokio::io::BufReader::new(peer));
        loop {
            let packet = tokio::time::timeout(Duration::from_secs(10), decoder.get_raw_packet())
                .await
                .unwrap()
                .unwrap();
            if packet.id == CPlayerAbilities::to_id(CURRENT_MC_VERSION) {
                return packet.payload[0] as i8;
            }
        }
    })
}

#[test]
fn creative_flight_lets_a_survival_player_fly() {
    let (player, peer) = support::player_with_peer("Wings", NegotiatedState::default());
    let living = &player.living_entity;
    assert!(!player.may_fly());
    request_flight(&player);
    assert!(!flying(&player));

    living.update_attribute(&Attributes::NEOFORGE_CREATIVE_FLIGHT, |instance| {
        instance.add_or_replace_modifier(Modifier {
            id: "test:wings".to_owned(),
            amount: 1.0,
            operation: ModifierOperation::Add,
        });
    });
    send_attribute_updates_for_living(living, vec![Attributes::NEOFORGE_CREATIVE_FLIGHT]);
    assert!(player.may_fly());
    // A vanilla client never gets the attribute: the abilities tell it that it may fly.
    assert_eq!(next_abilities_flags(peer) & 0x04, 0x04);
    request_flight(&player);
    assert!(flying(&player));

    living.update_attribute(&Attributes::NEOFORGE_CREATIVE_FLIGHT, |instance| {
        instance.remove_modifier("test:wings");
    });
    assert!(!player.may_fly());
    let _guard = support::RUNTIME.enter();
    player.tick(support::server());
    assert!(!flying(&player));
}
