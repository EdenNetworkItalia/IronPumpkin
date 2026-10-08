use std::io::Write;

use super::{read_byte_array, write_byte_array};
use crate::ser::{ReadingError, WritingError};

/// Mirrors `SplitPacketPayload`: one slice of a payload too large for a single packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitPacketPayload {
    pub payload: Box<[u8]>,
}

impl SplitPacketPayload {
    pub const CHANNEL: &'static str = "neoforge:split";

    pub fn read(read: &mut &[u8]) -> Result<Self, ReadingError> {
        Ok(Self {
            payload: read_byte_array(read)?,
        })
    }

    pub fn write(&self, mut write: impl Write) -> Result<(), WritingError> {
        write_byte_array(&mut write, &self.payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> (SplitPacketPayload, Vec<u8>) {
        let payload = SplitPacketPayload {
            payload: vec![0xab; 200].into_boxed_slice(),
        };
        // UNBOUNDED_BYTE_ARRAY: VarInt length 200 (0x48 | 0x80, 200 >> 7 = 1), then the bytes.
        let bytes = [&[0xc8, 0x01][..], &[0xab; 200]].concat();
        (payload, bytes)
    }

    #[test]
    fn decodes_java_bytes() {
        let (expected, bytes) = sample();
        let mut read = bytes.as_slice();
        assert_eq!(SplitPacketPayload::read(&mut read).unwrap(), expected);
        assert!(read.is_empty());
    }

    #[test]
    fn writes_java_bytes() {
        let (payload, expected) = sample();
        let mut bytes = Vec::new();
        payload.write(&mut bytes).unwrap();
        assert_eq!(bytes, expected);
    }

    #[test]
    fn rejects_length_beyond_input() {
        // VarInt length 0x7fffffff with two bytes behind it.
        let bytes = [0xff, 0xff, 0xff, 0xff, 0x07, 0xab, 0xab];
        assert!(SplitPacketPayload::read(&mut &bytes[..]).is_err());
    }
}
