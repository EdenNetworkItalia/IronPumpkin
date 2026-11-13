use super::{CUpdateAttributes, Property};
use crate::{ClientPacket, VarInt, ser::NetworkReadExt};
use pumpkin_data::attributes::Attributes;
use pumpkin_data::dynamic::ContentIds;
use pumpkin_util::version::JavaMinecraftVersion;

#[test]
fn neoforge_attributes_reach_only_clients_that_synced_the_registries() {
    let packet = CUpdateAttributes::new(
        VarInt(1),
        vec![
            Property::new(VarInt(i32::from(Attributes::ARMOR.id)), 0.0, Vec::new()),
            Property::new(
                VarInt(i32::from(Attributes::NEOFORGE_CREATIVE_FLIGHT.id)),
                1.0,
                Vec::new(),
            ),
        ],
    );
    let count = |ids| {
        let mut buf = Vec::new();
        packet
            .write_packet_data(
                &mut buf,
                &crate::EncodingKey::new(JavaMinecraftVersion::V_26_3, ids),
            )
            .unwrap();
        let mut cursor = &buf[..];
        cursor.get_var_int().unwrap();
        cursor.get_var_int().unwrap().0
    };
    assert_eq!(count(ContentIds::Real), 2);
    assert_eq!(count(ContentIds::Display), 1);
}
