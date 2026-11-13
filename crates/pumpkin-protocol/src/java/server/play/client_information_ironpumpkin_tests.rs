use super::*;
use crate::ClientPacket;

/// Written by the vanilla 26.3 `ClientInformation.write` for `en_us`, view distance 12,
/// chat visibility `SYSTEM`, no chat colors, skin parts 0x5a, left hand, text filtering on,
/// no listing and `ParticleStatus.MINIMAL`.
const VANILLA_MINIMAL: [u8; 14] = [
    0x05, b'e', b'n', b'_', b'u', b's', 0x0c, 0x01, 0x00, 0x5a, 0x00, 0x01, 0x00, 0x02,
];

/// `VANILLA_MINIMAL` with its `en_us` locale replaced by `locale`.
fn packet_with_locale(locale: &str) -> Vec<u8> {
    let mut buf = vec![u8::try_from(locale.len()).expect("a short locale")];
    buf.extend_from_slice(locale.as_bytes());
    buf.extend_from_slice(&VANILLA_MINIMAL[6..]);
    buf
}

#[test]
fn reads_locale_at_vanilla_limit() {
    let locale = "a".repeat(16);
    let buf = packet_with_locale(&locale);

    let packet = SClientInformationPlay::read(&mut &buf[..], &JavaMinecraftVersion::V_26_3)
        .expect("a locale of 16 characters is the vanilla maximum");

    assert_eq!(packet.locale, locale);
}

#[test]
fn rejects_locale_over_vanilla_limit() {
    let buf = packet_with_locale(&"a".repeat(17));

    let result = SClientInformationPlay::read(&mut &buf[..], &JavaMinecraftVersion::V_26_3);

    assert!(matches!(result, Err(ReadingError::TooLarge(_))));
}

#[test]
fn reads_vanilla_client_information() {
    let mut bytes = &VANILLA_MINIMAL[..];
    let packet = SClientInformationPlay::read(&mut bytes, &JavaMinecraftVersion::V_26_3)
        .expect("a vanilla client information packet should be readable");

    assert_eq!(packet.locale, "en_us");
    assert_eq!(packet.view_distance, 12);
    assert_eq!(packet.chat_mode.0, 1);
    assert!(!packet.chat_colors);
    assert_eq!(packet.skin_parts, 0x5a);
    assert_eq!(packet.main_hand.0, 0);
    assert!(packet.text_filtering);
    assert!(!packet.server_listing);
    assert_eq!(packet.particle_status.0, 2);
    assert!(bytes.is_empty(), "the reader left {} bytes", bytes.len());
}

#[test]
fn writes_vanilla_client_information() {
    let packet = SClientInformationPlay {
        locale: "en_us",
        view_distance: 12,
        chat_mode: VarInt(1),
        chat_colors: false,
        skin_parts: 0x5a,
        main_hand: VarInt(0),
        text_filtering: true,
        server_listing: false,
        particle_status: VarInt(2),
    };
    let mut buf = Vec::new();
    packet
        .write_packet_data(&mut buf, &JavaMinecraftVersion::V_26_3.into())
        .expect("write client information");

    assert_eq!(buf, VANILLA_MINIMAL);
}
