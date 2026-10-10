//! A custom block state reaches vanilla clients as its display state, and a custom block as its
//! display block. Vanilla ids keep their bytes. The content registry is process-wide and freezes
//! once, so this file is its own test binary.
#![expect(
    clippy::expect_used,
    reason = "a test fails on the first missing value"
)]

use std::sync::Once;

use pumpkin_data::dynamic::{self, BlockDefinition, BlockPropertyDefinition};
use pumpkin_data::entity::EntityType;
use pumpkin_data::particle::Particle;
use pumpkin_data::tracked_data::{TrackedData, area_effect_cloud, block_display, enderman};
use pumpkin_data::world::WorldEvent;
use pumpkin_data::{Block, BlockStateId};
use pumpkin_protocol::ClientPacket;
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_protocol::java::client::play::{
    CBlockEvent, CBlockUpdate, CLevelEvent, CMultiBlockUpdate, CParticle, CSpawnEntity,
    CWorldEvent, Metadata,
};
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_util::version::JavaMinecraftVersion;

const VERSION: JavaMinecraftVersion = JavaMinecraftVersion::V_26_3;

/// The lit state of the custom lamp, which shows as an unlit redstone lamp.
fn lit_lamp() -> BlockStateId {
    static FREEZE: Once = Once::new();
    FREEZE.call_once(|| {
        dynamic::register_block(BlockDefinition {
            name: "test:lamp".to_string(),
            display: Block::REDSTONE_LAMP.default_state.id,
            properties: vec![BlockPropertyDefinition::bool("lit", false)],
            tags: Vec::new(),
        })
        .expect("register test block");
        dynamic::freeze().expect("freeze registry");
    });
    let lamp = Block::from_name("test:lamp").expect("custom block resolves after the freeze");
    let lit = lamp.states[0].id;
    assert!(lit.as_u16() >= BlockStateId::COUNT);
    lit
}

fn display() -> BlockStateId {
    Block::REDSTONE_LAMP.default_state.id
}

fn raw(id: BlockStateId) -> i32 {
    i32::from(id.as_u16())
}

fn bytes(packet: &impl ClientPacket, version: JavaMinecraftVersion) -> Vec<u8> {
    let mut buf = Vec::new();
    packet
        .write_packet_data(&mut buf, &version.into())
        .expect("packet writes");
    buf
}

fn metadata(tracked: TrackedData, value: VarInt) -> Vec<u8> {
    let mut buf = Vec::new();
    Metadata::new(tracked, value)
        .write(&mut buf, &VERSION.into())
        .expect("metadata writes");
    buf
}

fn var_int(value: i32) -> Vec<u8> {
    let mut buf = Vec::new();
    VarInt(value).encode(&mut buf).expect("var int encodes");
    buf
}

#[test]
fn the_state_egress_maps_only_custom_states() {
    let lit = lit_lamp();
    assert_eq!(
        lit.to_java_network_id(pumpkin_data::dynamic::ContentIds::Display),
        display().as_u16()
    );
    for state in [BlockStateId::AIR, Block::STONE.default_state.id, display()] {
        assert_eq!(
            state.to_java_network_id(pumpkin_data::dynamic::ContentIds::Display),
            state.as_u16()
        );
    }
}

#[test]
fn block_updates_carry_the_display_state() {
    let lit = lit_lamp();
    let pos = BlockPos::new(1, 64, -3);
    assert_eq!(
        bytes(&CBlockUpdate::new(pos, VarInt(raw(lit))), VERSION),
        bytes(&CBlockUpdate::new(pos, VarInt(raw(display()))), VERSION)
    );
    let stone = Block::STONE.default_state.id;
    let mut expected = bytes(&CBlockUpdate::new(pos, VarInt(0)), VERSION);
    expected.truncate(expected.len() - 1);
    expected.extend(var_int(raw(stone)));
    assert_eq!(
        bytes(&CBlockUpdate::new(pos, VarInt(raw(stone))), VERSION),
        expected
    );

    let other = BlockPos::new(2, 64, -3);
    for version in [
        VERSION,
        JavaMinecraftVersion::V_1_8,
        JavaMinecraftVersion::V_1_7_6,
    ] {
        assert_eq!(
            bytes(
                &CMultiBlockUpdate::new(&[(pos, lit), (other, stone)]),
                version
            ),
            bytes(
                &CMultiBlockUpdate::new(&[(pos, display()), (other, stone)]),
                version
            )
        );
    }
}

#[test]
fn block_events_carry_the_display_block() {
    let lamp = Block::from_state_id(lit_lamp());
    let pos = BlockPos::new(0, 70, 0);
    let event = |block: &Block| CBlockEvent::new(pos, 1, 2, VarInt(i32::from(block.id.as_u16())));
    assert_eq!(
        bytes(&event(lamp), VERSION),
        bytes(&event(&Block::REDSTONE_LAMP), VERSION)
    );
    let chest = bytes(&event(&Block::CHEST), VERSION);
    assert!(chest.ends_with(&var_int(i32::from(Block::CHEST.id.as_u16()))));
}

#[test]
fn block_state_level_events_carry_the_display_state() {
    let lit = raw(lit_lamp());
    let pos = BlockPos::new(5, 60, 5);
    for event in [
        WorldEvent::ParticlesAndSoundDestroyBlock,
        WorldEvent::ParticlesDestroyBlock,
        WorldEvent::ParticlesAndSoundBrushBlockComplete,
    ] {
        let event = event as i32;
        assert_eq!(
            bytes(&CWorldEvent::new(event, pos, lit, false), VERSION),
            bytes(
                &CWorldEvent::new(event, pos, raw(display()), false),
                VERSION
            )
        );
        assert_eq!(
            bytes(&CLevelEvent::new(event, pos, lit, false), VERSION),
            bytes(
                &CLevelEvent::new(event, pos, raw(display()), false),
                VERSION
            )
        );
    }
    // Other events keep their data, even when it equals a custom state id.
    let growth = WorldEvent::ParticlesAndSoundPlantGrowth as i32;
    let written = bytes(&CWorldEvent::new(growth, pos, lit, false), VERSION);
    assert_eq!(
        written[written.len() - 5..written.len() - 1],
        lit.to_be_bytes()
    );
}

#[test]
fn block_particles_carry_the_display_state() {
    let lit = var_int(raw(lit_lamp()));
    let shown = var_int(raw(display()));
    let particle = |particle: Particle, data: &[u8]| {
        let packet = CParticle::new(
            false,
            false,
            Vector3::new(0.5, 65.0, 0.5),
            Vector3::new(0.1, 0.1, 0.1),
            0.0,
            4,
            VarInt(i32::from(particle.to_id())),
            data,
        );
        bytes(&packet, VERSION)
    };
    for block_particle in [
        Particle::Block,
        Particle::BlockMarker,
        Particle::FallingDust,
        Particle::DustPillar,
        Particle::BlockCrumble,
    ] {
        assert_eq!(
            particle(block_particle, &lit),
            particle(block_particle, &shown)
        );
    }
    assert_ne!(
        particle(Particle::Flame, &lit),
        particle(Particle::Flame, &shown)
    );
    // Data that is no block state id passes as it is.
    assert_eq!(
        particle(Particle::Block, &[]).len(),
        particle(Particle::Flame, &[]).len()
    );

    let mut block_particle = var_int(i32::from(Particle::Block.to_id()));
    block_particle.extend(&lit);
    let mut shown_particle = var_int(i32::from(Particle::Block.to_id()));
    shown_particle.extend(&shown);
    let mut written = Vec::new();
    Metadata::new(area_effect_cloud::DATA_PARTICLE, RawValue(block_particle))
        .write(&mut written, &VERSION.into())
        .expect("metadata writes");
    let mut expected = Vec::new();
    Metadata::new(area_effect_cloud::DATA_PARTICLE, RawValue(shown_particle))
        .write(&mut expected, &VERSION.into())
        .expect("metadata writes");
    assert_eq!(written, expected);
}

#[test]
fn entity_block_states_carry_the_display_state() {
    let lit = raw(lit_lamp());
    let shown = raw(display());
    for tracked in [
        block_display::DATA_BLOCK_STATE_ID,
        enderman::DATA_CARRY_STATE,
    ] {
        assert_eq!(
            metadata(tracked, VarInt(lit)),
            metadata(tracked, VarInt(shown))
        );
    }
    let stone = raw(Block::STONE.default_state.id);
    let written = metadata(block_display::DATA_BLOCK_STATE_ID, VarInt(stone));
    assert!(written.ends_with(&var_int(stone)));

    let spawn = |entity_type: &EntityType, data: i32| {
        let packet = CSpawnEntity::new(
            VarInt(7),
            uuid::Uuid::nil(),
            VarInt(i32::from(entity_type.id)),
            Vector3::new(0.5, 70.0, 0.5),
            0.0,
            0.0,
            0.0,
            VarInt(data),
            Vector3::new(0.0, 0.0, 0.0),
        );
        bytes(&packet, VERSION)
    };
    assert_eq!(
        spawn(&EntityType::FALLING_BLOCK, lit),
        spawn(&EntityType::FALLING_BLOCK, shown)
    );
    assert_ne!(
        spawn(&EntityType::ARROW, lit),
        spawn(&EntityType::ARROW, shown)
    );
}

/// Metadata whose value is already encoded.
struct RawValue(Vec<u8>);

impl pumpkin_protocol::java::client::play::MetadataSerializer for RawValue {
    fn write_metadata(
        &self,
        writer: &mut impl std::io::Write,
        _version: &pumpkin_protocol::EncodingKey,
    ) -> Result<(), pumpkin_protocol::ser::WritingError> {
        writer
            .write_all(&self.0)
            .map_err(pumpkin_protocol::ser::WritingError::IoError)
    }
}
