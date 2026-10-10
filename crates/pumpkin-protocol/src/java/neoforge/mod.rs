//! Bodies of the custom payloads `NeoForge` 26.3.x exchanges during the configuration phase.
//!
//! These are not packets: each body travels inside the vanilla `custom_payload` packet
//! ([`crate::java::server::config::SPluginMessage`] and [`crate::java::client::config::CPluginMessage`])
//! under the channel in its `CHANNEL` constant. Each `read` and `write` mirrors the field order of
//! the payload's `STREAM_CODEC` in `NeoForge`.
//!
//! `read` stops after the last field and leaves any extra bytes in the slice. Decode a whole
//! payload body with [`decode_exact`], which rejects leftovers like vanilla's packet decoder.

use std::io::Write;

use pumpkin_util::identifier::Identifier;

use crate::{
    VarInt,
    ser::{NetworkReadExt, NetworkReadSliceExt, NetworkWriteExt, ReadingError, WritingError},
};

mod common;
mod config_file;
mod data_maps;
mod extensible_enums;
mod feature_flags;
mod network;
mod register;
mod registry;
mod split;

pub use common::{CommonRegisterPayload, CommonVersionPayload};
pub use config_file::ConfigFilePayload;
pub use data_maps::{
    KnownDataMap, KnownRegistryDataMapsPayload, KnownRegistryDataMapsReplyPayload,
};
pub use extensible_enums::{
    EnumEntry, ExtensibleEnumAcknowledgePayload, ExtensibleEnumDataPayload, ExtensionData,
    NetworkCheck,
};
pub use feature_flags::{FeatureFlagAcknowledgePayload, FeatureFlagDataPayload};
pub use network::{
    ModdedNetworkPayload, ModdedNetworkQueryComponent, ModdedNetworkQueryPayload,
    ModdedNetworkSetupFailedPayload, NetworkChannel, NetworkPayloadSetup,
};
pub use register::{MinecraftRegisterPayload, MinecraftUnregisterPayload};
pub use registry::{
    FrozenRegistryPayload, FrozenRegistrySyncCompletedPayload, FrozenRegistrySyncStartPayload,
    RegistrySnapshot,
};
pub use split::SplitPacketPayload;

/// Vanilla `net.minecraft.network.ConnectionProtocol`, sent by ordinal (`idMapper`) or by `id()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConnectionProtocol {
    Handshaking,
    Play,
    Status,
    Login,
    Configuration,
}

impl ConnectionProtocol {
    /// Declaration order of the Java enum, which is what `ordinal()` returns.
    const VALUES: [Self; 5] = [
        Self::Handshaking,
        Self::Play,
        Self::Status,
        Self::Login,
        Self::Configuration,
    ];

    #[must_use]
    pub const fn ordinal(self) -> i32 {
        self as i32
    }

    #[must_use]
    pub fn from_ordinal(ordinal: i32) -> Option<Self> {
        usize::try_from(ordinal)
            .ok()
            .and_then(|i| Self::VALUES.get(i).copied())
    }

    /// The string returned by `ConnectionProtocol.id()`.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Handshaking => "handshake",
            Self::Play => "play",
            Self::Status => "status",
            Self::Login => "login",
            Self::Configuration => "configuration",
        }
    }

    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::VALUES.into_iter().find(|p| p.id() == id)
    }

    fn read(read: &mut &[u8]) -> Result<Self, ReadingError> {
        let ordinal = read.get_var_int()?.0;
        Self::from_ordinal(ordinal).ok_or_else(|| {
            ReadingError::Message(format!("Invalid ConnectionProtocol ordinal {ordinal}"))
        })
    }

    fn write(self, write: &mut impl Write) -> Result<(), WritingError> {
        write.write_var_int(&VarInt(self.ordinal()))
    }
}

/// Vanilla `net.minecraft.network.protocol.PacketFlow`, sent by ordinal (`FriendlyByteBuf.writeEnum`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PacketFlow {
    Serverbound,
    Clientbound,
}

impl PacketFlow {
    fn read(read: &mut &[u8]) -> Result<Self, ReadingError> {
        match read.get_var_int()?.0 {
            0 => Ok(Self::Serverbound),
            1 => Ok(Self::Clientbound),
            ordinal => Err(ReadingError::Message(format!(
                "Invalid PacketFlow ordinal {ordinal}"
            ))),
        }
    }

    fn write(self, write: &mut impl Write) -> Result<(), WritingError> {
        write.write_var_int(&VarInt(self as i32))
    }
}

/// Upper bound for capacity reserved from a network count, like the
/// `Math.min(count, 65536)` in vanilla `ByteBufCodecs.collection`.
const MAX_PREALLOCATED: usize = 65536;

/// Decodes a whole payload body with `read` and fails when bytes are left over.
pub fn decode_exact<T>(
    mut data: &[u8],
    read: impl FnOnce(&mut &[u8]) -> Result<T, ReadingError>,
) -> Result<T, ReadingError> {
    let value = read(&mut data)?;
    if !data.is_empty() {
        return Err(ReadingError::TooLarge(format!(
            "{} bytes left over after the payload",
            data.len()
        )));
    }
    Ok(value)
}

/// Reads a collection, map or byte array length, like vanilla `ByteBufCodecs.readCount`.
///
/// Vanilla allows up to `Integer.MAX_VALUE` and only stops at the end of the buffer. Every element
/// of the payloads in this module takes at least one byte, so a count larger than the remaining
/// input can never decode and is rejected before anything is allocated.
fn read_count(read: &mut &[u8]) -> Result<usize, ReadingError> {
    let count = read.get_var_int()?.0;
    let count = usize::try_from(count)
        .map_err(|_| ReadingError::Message(format!("Negative length {count}")))?;
    if count > read.len() {
        return Err(ReadingError::TooLarge(format!(
            "Length {count} exceeds the {} remaining bytes",
            read.len()
        )));
    }
    Ok(count)
}

fn write_count(write: &mut impl Write, count: usize) -> Result<(), WritingError> {
    let count = i32::try_from(count)
        .map_err(|_| WritingError::Message(format!("{count} isn't representable as a VarInt")))?;
    write.write_var_int(&VarInt(count))
}

/// `Identifier.STREAM_CODEC`: `ByteBufCodecs.STRING_UTF8` mapped through `Identifier.parse`.
fn read_identifier(read: &mut &[u8]) -> Result<Identifier, ReadingError> {
    let id = read.get_str_borrowed()?;
    Identifier::parse(id).map_err(|e| ReadingError::Message(e.to_string()))
}

fn write_identifier(write: &mut impl Write, id: &Identifier) -> Result<(), WritingError> {
    write.write_string(&id.to_string())
}

/// `NeoForgeStreamCodecs.UNBOUNDED_BYTE_ARRAY`: `FriendlyByteBuf.readByteArray()`, a `VarInt`
/// length bounded by the readable bytes, then the bytes.
fn read_byte_array(read: &mut &[u8]) -> Result<Box<[u8]>, ReadingError> {
    let len = read_count(read)?;
    Ok(read.read_slice_borrowed(len)?.into())
}

fn write_byte_array(write: &mut impl Write, data: &[u8]) -> Result<(), WritingError> {
    write_count(write, data.len())?;
    write.write_slice(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decodes a capture fixture: hex digits, surrounding whitespace ignored.
    pub(super) fn hex_fixture(text: &str) -> Vec<u8> {
        let text = text.trim();
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn decode_exact_rejects_trailing_bytes() {
        // A unit body followed by one stray byte, then the same body alone.
        let read = |_: &mut &[u8]| Ok(());
        assert!(decode_exact(&[0x00], read).is_err());
        assert!(decode_exact(&[], read).is_ok());
    }

    #[test]
    fn negative_count_is_rejected() {
        // VarInt -1 = ff ff ff ff 0f.
        let bytes = [0xff, 0xff, 0xff, 0xff, 0x0f];
        assert!(read_count(&mut &bytes[..]).is_err());
    }
}
