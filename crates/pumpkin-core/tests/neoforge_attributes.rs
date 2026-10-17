//! The `NeoForge` attributes that `IronPumpkin` registers: `neoforge:swim_speed` scales the swim
//! speed of a mob in water (`LivingEntity.travelInWater` of `NeoForge`),
//! `neoforge:creative_flight` lets a survival player fly (`IPlayerExtension#mayFly`), and an
//! equipped glider item gives `neoforge:gliding_flight` (`NeoForgeMod.onItemAttributeModifiers`).
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
use pumpkin_data::{
    Block, attributes::Attributes, data_component_impl::EquipmentSlot, dynamic::ContentIds,
    entity::EntityType, item::Item, item_stack::ItemStack, packet::CURRENT_MC_VERSION,
};
use pumpkin_protocol::{
    ServerPacket,
    java::{
        client::play::{CPlayerAbilities, CUpdateAttributes, Property},
        packet_decoder::TCPNetworkDecoder,
        server::play::SPlayerAbilities,
    },
    packet::MultiVersionJavaPacket,
};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};
use pumpkin_world::world::BlockFlags;

/// Loads the chunk of `pos` for the rest of the process and sets `pos` and the block above it to
/// `fluid`, in a stone tank with air on top. The world seed is random, so the terrain must not
/// reach the zombie: a block above stops its upward push and a fluid beside it pushes it.
fn fluid_column(pos: BlockPos, fluid: &Block) {
    let local = (pos.0.x & 15, pos.0.z & 15);
    assert!(
        (1..15).contains(&local.0) && (1..15).contains(&local.1),
        "the tank of {pos} crosses a chunk edge"
    );
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
    for y in -1..=2 {
        for x in -1..=1 {
            for z in -1..=1 {
                let block = match (x, y, z) {
                    (0, 0 | 1, 0) => fluid,
                    (0, 2, 0) => &Block::AIR,
                    _ => &Block::STONE,
                };
                let at = pos.offset(Vector3::new(x, y, z));
                world.set_block_state(&at, block.default_state.id, BlockFlags::empty());
            }
        }
    }
}

/// A zombie without AI goals in a water column at `pos`, swimming forward for one tick. Returns
/// its forward velocity.
fn forward_swim_velocity(pos: BlockPos, swim_speed_bonus: Option<f64>) -> f64 {
    fluid_column(pos, &Block::WATER);
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
    let pos = BlockPos::new(3000, 120, 3000);
    let plain = forward_swim_velocity(pos, None);
    let fast = forward_swim_velocity(pos, Some(1.0));
    assert!(plain > 0.0, "{plain}");
    // A multiply-base modifier of 1 doubles the attribute, and the swim speed with it.
    assert!((fast / plain - 2.0).abs() < 1e-9, "{plain} {fast}");
}

/// A zombie without AI goals in a column of `fluid` at `pos`, jumping for one tick, with a swim
/// speed of 2 if `fast`. Returns its vertical velocity.
fn fluid_jump_velocity(pos: BlockPos, fluid: &Block, fast: bool) -> f64 {
    fluid_column(pos, fluid);
    let world = support::world();
    let center = Vector3::new(
        f64::from(pos.0.x) + 0.5,
        f64::from(pos.0.y),
        f64::from(pos.0.z) + 0.5,
    );
    let zombie = LivingEntity::new(Entity::new(world, center, &EntityType::ZOMBIE));
    if fast {
        zombie.update_attribute(&Attributes::NEOFORGE_SWIM_SPEED, |instance| {
            instance.add_or_replace_modifier(Modifier {
                id: "test:fins".to_owned(),
                amount: 1.0,
                operation: ModifierOperation::MultiplyBase,
            });
        });
    }
    zombie
        .jumping
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let _guard = support::RUNTIME.enter();
    zombie.tick(&zombie, support::server());
    let entity = &zombie.entity;
    let in_fluid = if fluid.id == Block::LAVA.id {
        &entity.touching_lava
    } else {
        &entity.touching_water
    };
    assert!(
        in_fluid.load(std::sync::atomic::Ordering::SeqCst),
        "the zombie at {pos} is not in {}",
        fluid.name
    );
    entity.velocity.load().y
}

#[test]
fn swim_speed_scales_the_jump_in_water_and_not_in_lava() {
    let pos = BlockPos::new(3016, 120, 3000);
    let water = fluid_jump_velocity(pos, &Block::WATER, false);
    let fast_water = fluid_jump_velocity(pos, &Block::WATER, true);
    assert!(fast_water > water, "{water} {fast_water}");
    let lava = fluid_jump_velocity(pos, &Block::LAVA, false);
    let fast_lava = fluid_jump_velocity(pos, &Block::LAVA, true);
    assert_eq!(lava.to_bits(), fast_lava.to_bits(), "{lava} {fast_lava}");
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
    // A Bedrock client asks through `Player::request_flying`.
    player.request_flying(false);
    assert!(!flying(&player));
    player.request_flying(true);
    assert!(flying(&player));

    living.update_attribute(&Attributes::NEOFORGE_CREATIVE_FLIGHT, |instance| {
        instance.remove_modifier("test:wings");
    });
    assert!(!player.may_fly());
    let _guard = support::RUNTIME.enter();
    player.tick(support::server());
    assert!(!flying(&player));
}

/// The next `count` `neoforge:gliding_flight` properties of `entity_id` in the attributes packets
/// that the client end of `peer` receives.
fn gliding_flight_properties(
    peer: tokio::net::TcpStream,
    entity_id: i32,
    count: usize,
) -> Vec<Property> {
    support::RUNTIME.block_on(async {
        let mut decoder = TCPNetworkDecoder::new(tokio::io::BufReader::new(peer));
        let mut properties = Vec::new();
        while properties.len() < count {
            let packet = tokio::time::timeout(Duration::from_secs(10), decoder.get_raw_packet())
                .await
                .unwrap()
                .unwrap();
            if packet.id != CUpdateAttributes::to_id(CURRENT_MC_VERSION) {
                continue;
            }
            let attributes =
                CUpdateAttributes::read(&mut &packet.payload[..], &CURRENT_MC_VERSION).unwrap();
            if attributes.entity_id.0 != entity_id {
                continue;
            }
            properties.extend(attributes.properties.into_iter().filter(|property| {
                property.id.0 == i32::from(Attributes::NEOFORGE_GLIDING_FLIGHT.id)
            }));
        }
        properties
    })
}

#[test]
fn an_equipped_glider_gives_gliding_flight_to_a_neoforge_client() {
    let negotiated = NegotiatedState {
        content_ids: ContentIds::Real,
        ..NegotiatedState::default()
    };
    let (player, peer) = support::player_with_peer("Glider", negotiated);
    let living = &player.living_entity;
    let gliding_flight = || living.get_attribute_value(&Attributes::NEOFORGE_GLIDING_FLIGHT);
    let elytra = ItemStack::new(1, &Item::ELYTRA);

    // The modifier applies in the slot of the `equippable` component only.
    living.send_equipment_changes(&[(EquipmentSlot::MAIN_HAND, elytra.clone())]);
    assert_eq!(gliding_flight(), 0.0);
    living.send_equipment_changes(&[(EquipmentSlot::CHEST, elytra.clone())]);
    assert_eq!(gliding_flight(), 1.0);
    living.send_equipment_changes(&[(EquipmentSlot::CHEST, ItemStack::EMPTY.clone())]);
    assert_eq!(gliding_flight(), 0.0);
    let mut worn = elytra;
    worn.set_damage(worn.get_max_damage().unwrap() - 1);
    living.send_equipment_changes(&[(EquipmentSlot::CHEST, worn)]);
    assert_eq!(gliding_flight(), 0.0);

    let properties = gliding_flight_properties(peer, living.entity_id(), 2);
    let modifiers: Vec<(&str, f64, i8)> = properties[0]
        .modifiers
        .iter()
        .map(|modifier| (modifier.id.as_str(), modifier.amount, modifier.operation))
        .collect();
    assert_eq!(modifiers, [("neoforge:glider_component_flight", 1.0, 0)]);
    assert!(properties[1].modifiers.is_empty());
}
