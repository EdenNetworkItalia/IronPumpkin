use pumpkin_data::dynamic::ContentIds;
use pumpkin_data::effect::StatusEffect;
use pumpkin_util::version::JavaMinecraftVersion;

use super::CUpdateMobEffect;
use crate::{ClientPacket, EncodingKey, VarInt, ser::WritingError};

#[test]
fn a_mod_effect_reaches_only_clients_that_synced_the_registries() {
    let packet =
        |id: u8| CUpdateMobEffect::new(VarInt(1), VarInt(i32::from(id)), VarInt(0), VarInt(20), 0);
    let write = |packet: CUpdateMobEffect, ids| {
        packet.write_packet_data(
            Vec::new(),
            &EncodingKey::new(JavaMinecraftVersion::V_26_3, ids),
        )
    };
    let custom = StatusEffect::COUNT;
    assert!(write(packet(custom), ContentIds::Real).is_ok());
    assert!(matches!(
        write(packet(custom), ContentIds::Display),
        Err(WritingError::UnknownToClient(_))
    ));
    assert!(write(packet(StatusEffect::SPEED.id), ContentIds::Display).is_ok());
}
