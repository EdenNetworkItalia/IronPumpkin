use pumpkin_data::packet::serverbound::config::CLIENT_INFORMATION;
use pumpkin_macros::java_packet;

use crate::VarInt;

use crate::{
    ServerPacket,
    ser::{NetworkReadExt, NetworkReadSliceExt, ReadingError},
};
use pumpkin_util::version::JavaMinecraftVersion;

/// Maximum length of the locale in UTF-16 units, `ClientInformation.MAX_LANGUAGE_LENGTH` in vanilla
const MAX_LANGUAGE_LENGTH: usize = 16;

/// Sent by the client to inform the server about its local settings
#[java_packet(CLIENT_INFORMATION)]
pub struct SClientInformationConfig<'a> {
    /// The language code used by the client (e.g., "`en_us`")
    pub locale: &'a str,
    /// The maximum number of chunks the client renders
    pub view_distance: i8,
    /// Visibility of chat messages (0: Enabled, 1: Commands Only, 2: Hidden)
    pub chat_mode: VarInt,
    /// Whether the client wants chat colors/formatting rendered
    pub chat_colors: bool,
    /// Bitmask representing displayed skin parts (e.g., cape, jacket, sleeves)
    pub skin_parts: u8,
    /// The player's dominant hand (0: Left, 1: Right)
    pub main_hand: VarInt,
    /// Whether the client wants text filtering (e.g., for profanity) enabled
    pub text_filtering: bool,
    /// Whether the player should appear in the server's online player list
    pub server_listing: bool,
    /// Particle level the client renders (0: All, 1: Decreased, 2: Minimal)
    pub particle_status: VarInt,
}

impl<'a> ServerPacket<'a> for SClientInformationConfig<'a> {
    fn read(bytebuf: &mut &'a [u8], version: &JavaMinecraftVersion) -> Result<Self, ReadingError> {
        let locale = bytebuf.get_str_bounded_borrowed(MAX_LANGUAGE_LENGTH)?;
        let view_distance = bytebuf.get_i8()?;
        let chat_mode = bytebuf.get_var_int()?;
        let chat_colors = bytebuf.get_bool()?;
        let skin_parts = bytebuf.get_u8()?;
        let main_hand = if version >= &JavaMinecraftVersion::V_1_9 {
            bytebuf.get_var_int()?
        } else {
            VarInt(1)
        };
        let text_filtering = if version >= &JavaMinecraftVersion::V_1_17 {
            bytebuf.get_bool()?
        } else {
            false
        };
        let server_listing = if version >= &JavaMinecraftVersion::V_1_18 {
            bytebuf.get_bool()?
        } else {
            true
        };
        let particle_status = if version >= &JavaMinecraftVersion::V_1_21_2 {
            bytebuf.get_var_int()?
        } else {
            VarInt(0)
        };

        Ok(Self {
            locale,
            view_distance,
            chat_mode,
            chat_colors,
            skin_parts,
            main_hand,
            text_filtering,
            server_listing,
            particle_status,
        })
    }
}

impl crate::ClientPacket for SClientInformationConfig<'_> {
    fn write_packet_data(
        &self,
        mut write: impl std::io::Write,
        version: &JavaMinecraftVersion,
    ) -> Result<(), crate::ser::WritingError> {
        use crate::ser::NetworkWriteExt;
        write.write_string(self.locale)?;
        write.write_i8(self.view_distance)?;
        write.write_var_int(&self.chat_mode)?;
        write.write_bool(self.chat_colors)?;
        write.write_u8(self.skin_parts)?;
        if version >= &JavaMinecraftVersion::V_1_9 {
            write.write_var_int(&self.main_hand)?;
        }
        if version >= &JavaMinecraftVersion::V_1_17 {
            write.write_bool(self.text_filtering)?;
        }
        if version >= &JavaMinecraftVersion::V_1_18 {
            write.write_bool(self.server_listing)?;
        }
        if version >= &JavaMinecraftVersion::V_1_21_2 {
            write.write_var_int(&self.particle_status)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
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

        let packet = SClientInformationConfig::read(&mut &buf[..], &JavaMinecraftVersion::V_26_3)
            .expect("a locale of 16 characters is the vanilla maximum");

        assert_eq!(packet.locale, locale);
    }

    #[test]
    fn rejects_locale_over_vanilla_limit() {
        let buf = packet_with_locale(&"a".repeat(17));

        let result = SClientInformationConfig::read(&mut &buf[..], &JavaMinecraftVersion::V_26_3);

        assert!(matches!(result, Err(ReadingError::TooLarge(_))));
    }

    #[test]
    fn reads_vanilla_client_information() {
        let mut bytes = &VANILLA_MINIMAL[..];
        let packet = SClientInformationConfig::read(&mut bytes, &JavaMinecraftVersion::V_26_3)
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
        let packet = SClientInformationConfig {
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
            .write_packet_data(&mut buf, &JavaMinecraftVersion::V_26_3)
            .expect("write client information");

        assert_eq!(buf, VANILLA_MINIMAL);
    }
}
