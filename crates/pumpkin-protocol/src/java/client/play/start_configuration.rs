use pumpkin_data::packet::clientbound::play::START_CONFIGURATION;
use pumpkin_macros::java_packet;

use crate::ClientPacket;
use crate::EncodingKey;

#[java_packet(START_CONFIGURATION)]
pub struct CStartConfiguration;

impl ClientPacket for CStartConfiguration {
    fn write_packet_data(
        &self,
        _write: impl std::io::Write,
        _version: &EncodingKey,
    ) -> Result<(), crate::ser::WritingError> {
        Ok(())
    }
}
