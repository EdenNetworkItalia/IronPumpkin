use pumpkin_data::packet::clientbound::config::DISCONNECT;
use pumpkin_macros::java_packet;
use pumpkin_util::text::TextComponent;

use crate::ClientPacket;
use crate::EncodingKey;
use crate::ser::NetworkWriteExt;

/// Forces the client to disconnect from the server while in the "Configuration" state.
///
/// Vanilla registers the same `ClientboundDisconnectPacket` in configuration and play, so the
/// reason is a text component and a translatable reason keeps its key and arguments.
#[java_packet(DISCONNECT)]
pub struct CConfigDisconnect<'a> {
    pub reason: &'a TextComponent,
}

impl<'a> CConfigDisconnect<'a> {
    #[must_use]
    pub const fn new(reason: &'a TextComponent) -> Self {
        Self { reason }
    }
}

impl ClientPacket for CConfigDisconnect<'_> {
    fn write_packet_data(
        &self,
        mut write: impl std::io::Write,
        version: &EncodingKey,
    ) -> Result<(), crate::ser::WritingError> {
        write.write_component(self.reason, version)
    }
}

#[cfg(test)]
#[path = "config_disconnect_ironpumpkin_tests.rs"]
mod tests;
