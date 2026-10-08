use std::io::Write;

use super::{read_byte_array, write_byte_array};
use crate::ser::{NetworkReadSliceExt, NetworkWriteExt, ReadingError, WritingError};

/// Mirrors `ConfigFilePayload`: one synced config file sent to the client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigFilePayload {
    pub file_name: String,
    pub contents: Box<[u8]>,
}

impl ConfigFilePayload {
    pub const CHANNEL: &'static str = "neoforge:config_file";

    pub fn read(read: &mut &[u8]) -> Result<Self, ReadingError> {
        Ok(Self {
            file_name: read.get_str_borrowed()?.to_owned(),
            contents: read_byte_array(read)?,
        })
    }

    pub fn write(&self, mut write: impl Write) -> Result<(), WritingError> {
        write.write_string(&self.file_name)?;
        write_byte_array(&mut write, &self.contents)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> (ConfigFilePayload, Vec<u8>) {
        let payload = ConfigFilePayload {
            file_name: "mymod-server.toml".to_owned(),
            contents: Box::from(&b"a=1\n"[..]),
        };
        let bytes = [
            &[0x11][..], // fileName: STRING_UTF8 length 17
            b"mymod-server.toml",
            &[0x04], // contents: UNBOUNDED_BYTE_ARRAY, VarInt length 4
            b"a=1\n",
        ]
        .concat();
        (payload, bytes)
    }

    #[test]
    fn decodes_java_bytes() {
        let (expected, bytes) = sample();
        let mut read = bytes.as_slice();
        assert_eq!(ConfigFilePayload::read(&mut read).unwrap(), expected);
        assert!(read.is_empty());
    }

    #[test]
    fn writes_java_bytes() {
        let (payload, expected) = sample();
        let mut bytes = Vec::new();
        payload.write(&mut bytes).unwrap();
        assert_eq!(bytes, expected);
    }
}
