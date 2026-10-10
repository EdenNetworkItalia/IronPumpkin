use aes::cipher::KeyIvInit;
use bytes::Bytes;
use flate2::{Compress, Compression, FlushCompress, Status};
use pumpkin_util::version::JavaMinecraftVersion;
use thiserror::Error;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use crate::{
    Aes128Cfb8Enc, ClientPacket, CompressionLevel, CompressionThreshold, MAX_PACKET_DATA_SIZE,
    MAX_PACKET_SIZE, PacketEncodeError, StreamEncryptor, VarInt, WritingError,
    java::neoforge::{SplitLimits, write_split_part},
    ser::NetworkWriteExt,
};

// raw -> compress -> encrypt

pub enum EncryptionWriter<W: AsyncWrite + Unpin> {
    Encrypt(Box<StreamEncryptor<W>>),
    None(W),
}

impl<W: AsyncWrite + Unpin> EncryptionWriter<W> {
    #[must_use]
    pub fn upgrade(self, cipher: Aes128Cfb8Enc) -> Self {
        match self {
            Self::None(stream) => Self::Encrypt(Box::new(StreamEncryptor::new(cipher, stream))),
            Self::Encrypt(_) => self,
        }
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for EncryptionWriter<W> {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        match self.get_mut() {
            Self::Encrypt(writer) => {
                let writer = std::pin::Pin::new(writer);
                writer.poll_write(cx, buf)
            }
            Self::None(writer) => {
                let writer = std::pin::Pin::new(writer);
                writer.poll_write(cx, buf)
            }
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        match self.get_mut() {
            Self::Encrypt(writer) => {
                let writer = std::pin::Pin::new(writer);
                writer.poll_flush(cx)
            }
            Self::None(writer) => {
                let writer = std::pin::Pin::new(writer);
                writer.poll_flush(cx)
            }
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        match self.get_mut() {
            Self::Encrypt(writer) => {
                let writer = std::pin::Pin::new(writer);
                writer.poll_shutdown(cx)
            }
            Self::None(writer) => {
                let writer = std::pin::Pin::new(writer);
                writer.poll_shutdown(cx)
            }
        }
    }
}

/// Encoder: Server -> Client
/// Supports `ZLib` endecoding/compression
/// Supports Aes128 Encryption
pub struct TCPNetworkEncoder<W: AsyncWrite + Unpin> {
    writer: Option<EncryptionWriter<W>>,
    // compression and compression threshold
    compression: Option<(CompressionThreshold, CompressionLevel)>,
    // Reused compressor to avoid constructing zlib state per packet.
    compressor: Option<(CompressionLevel, Compress)>,
    // Reused compression buffer to avoid allocating a new Vec for each packet.
    compression_scratch: Vec<u8>,
    frame_scratch: Vec<u8>,
    /// The `custom_payload` packet id of the current protocol when the client declared
    /// `neoforge:split`: an encoded packet above the limit goes out as split parts in that packet.
    split_payload_id: Option<i32>,
}

impl<W: AsyncWrite + Unpin> TCPNetworkEncoder<W> {
    pub const fn new(writer: W) -> Self {
        Self {
            writer: Some(EncryptionWriter::None(writer)),
            compression: None,
            compressor: None,
            compression_scratch: Vec::new(),
            frame_scratch: Vec::new(),
            split_payload_id: None,
        }
    }

    /// Sends packets above the `GenericPacketSplitter` limit as `neoforge:split` parts in the
    /// `custom_payload` packet with this id, or never splits with `None`.
    pub const fn set_split_payload_id(&mut self, custom_payload_id: Option<i32>) {
        self.split_payload_id = custom_payload_id;
    }

    pub const fn set_compression(
        &mut self,
        compression_info: (CompressionThreshold, CompressionLevel),
    ) {
        self.compression = Some(compression_info);
    }

    /// NOTE: Encryption can only be set; a minecraft stream cannot go back to being unencrypted
    pub fn set_encryption(&mut self, key: &[u8; 16]) -> Result<(), PacketEncodeError> {
        if matches!(self.writer, Some(EncryptionWriter::Encrypt(_))) {
            return Err(PacketEncodeError::Message(
                "Encryption already enabled".into(),
            ));
        }
        let cipher = Aes128Cfb8Enc::new_from_slices(key, key)
            .map_err(|_| PacketEncodeError::Message("Invalid key".into()))?;

        if let Some(writer) = self.writer.take() {
            self.writer = Some(writer.upgrade(cipher));
        }
        Ok(())
    }

    fn compress_packet_data(
        &mut self,
        packet_data: &[u8],
        compression_level: CompressionLevel,
    ) -> Result<(), PacketEncodeError> {
        self.compression_scratch.clear();
        let reserve_hint = packet_data
            .len()
            .saturating_add(packet_data.len() / 16)
            .saturating_add(64);
        let current_capacity = self.compression_scratch.capacity();
        if reserve_hint > current_capacity {
            self.compression_scratch
                .reserve(reserve_hint.saturating_sub(current_capacity));
        }

        let needs_new_compressor = match self.compressor.as_ref() {
            Some((level, _)) => *level != compression_level,
            None => true,
        };
        if needs_new_compressor {
            self.compressor = Some((
                compression_level,
                Compress::new(Compression::new(compression_level), true),
            ));
        }

        let (_, compressor) = self.compressor.as_mut().ok_or_else(|| {
            PacketEncodeError::Message("compressor must be present after initialization".into())
        })?;
        compressor.reset();
        let status = compressor
            .compress_vec(
                packet_data,
                &mut self.compression_scratch,
                FlushCompress::Finish,
            )
            .map_err(|err| PacketEncodeError::CompressionFailed(err.to_string()))?;

        if !matches!(status, Status::StreamEnd) {
            return Err(PacketEncodeError::CompressionFailed(format!(
                "Unexpected compressor status: {status:?}"
            )));
        }
        Ok(())
    }

    /// Appends a Clientbound `ClientPacket` to the internal buffer and applies compression when needed.
    ///
    /// If compression is enabled and the packet size exceeds the threshold, the packet is compressed.
    /// The packet is prefixed with its length and, if compressed, the uncompressed data length.
    /// The packet format is as follows:
    ///
    /// **Uncompressed:**
    /// |-----------------------|
    /// | Packet Length (`VarInt`)|
    /// |-----------------------|
    /// | Packet ID (`VarInt`)    |
    /// |-----------------------|
    /// | Data (Byte Array)     |
    /// |-----------------------|
    ///
    /// **Compressed:**
    /// |------------------------|
    /// | Packet Length (`VarInt`) |
    /// |------------------------|
    /// | Data Length (`VarInt`)   |
    /// |------------------------|
    /// | Packet ID (`VarInt`)     |
    /// |------------------------|
    /// | Data (Byte Array)      |
    /// |------------------------|
    ///
    /// -   `Packet Length`: The total length of the packet *excluding* the `Packet Length` field itself.
    /// -   `Data Length`: (Only present in compressed packets) The length of the uncompressed `Packet ID` and `Data`.
    /// -   `Packet ID`: The ID of the packet.
    /// -   `Data`: The packet's data.
    ///
    /// NOTE: This method does not flush. Call [`Self::flush`] to flush buffered data.
    pub async fn write_packet(&mut self, packet_data: Bytes) -> Result<(), PacketEncodeError> {
        let mut frame = std::mem::take(&mut self.frame_scratch);
        frame.clear();
        let framed = self.frame_packet(&packet_data, &mut frame);
        let result = match framed {
            Ok(()) => self.write_frame(&frame).await,
            Err(err) => Err(err),
        };
        self.frame_scratch = frame;
        result
    }

    pub async fn write_frame(&mut self, frame: &[u8]) -> Result<(), PacketEncodeError> {
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| PacketEncodeError::Message("Writer missing".into()))?;
        writer
            .write_all(frame)
            .await
            .map_err(|err| PacketEncodeError::Message(err.to_string()))
    }

    #[must_use]
    pub fn is_compressing_packet(&self, packet_data: &Bytes) -> bool {
        self.compression
            .is_some_and(|(threshold, _)| packet_data.len() >= threshold)
    }

    /// Frames one encoded packet, or its `neoforge:split` parts when splitting is on and it
    /// exceeds the limit.
    pub fn frame_packet(
        &mut self,
        packet_data: &Bytes,
        out: &mut Vec<u8>,
    ) -> Result<(), PacketEncodeError> {
        let limits = SplitLimits::new(self.compression.is_some());
        let parts = self
            .split_payload_id
            .and_then(|id| Some((id, limits.split(packet_data)?)));
        let Some((custom_payload_id, parts)) = parts else {
            return self.frame_single_packet(packet_data, out);
        };
        let mut part_packet = Vec::with_capacity(limits.packet);
        for (state, slice) in parts {
            part_packet.clear();
            write_split_part(custom_payload_id, state, slice, &mut part_packet)
                .map_err(|err| PacketEncodeError::Message(err.to_string()))?;
            self.frame_single_packet(&part_packet, out)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn frame_single_packet(
        &mut self,
        packet_data: &[u8],
        out: &mut Vec<u8>,
    ) -> Result<(), PacketEncodeError> {
        let data_len = packet_data.len();
        if data_len > MAX_PACKET_DATA_SIZE {
            return Err(PacketEncodeError::TooLong(data_len));
        }

        let data_len_var_int: VarInt = data_len.try_into().map_err(|_| {
            PacketEncodeError::Message(format!(
                "Packet data length is too large to fit in VarInt! ({data_len})"
            ))
        })?;

        let mut header_buf = [0u8; 10];
        let mut header_cursor = std::io::Cursor::new(&mut header_buf[..]);

        let payload_to_write: &[u8] = if let Some((compression_threshold, compression_level)) =
            self.compression
        {
            if data_len >= compression_threshold {
                self.compress_packet_data(packet_data, compression_level)?;
                debug_assert!(!self.compression_scratch.is_empty());

                let full_packet_len_var_int: VarInt = (data_len_var_int.written_size()
                    + self.compression_scratch.len())
                .try_into()
                .map_err(|_| {
                    PacketEncodeError::Message(format!(
                        "Full packet length is too large to fit in VarInt! ({data_len})"
                    ))
                })?;

                let complete_serialization_length =
                    full_packet_len_var_int.written_size() + full_packet_len_var_int.0 as usize;
                if complete_serialization_length > MAX_PACKET_SIZE as usize {
                    return Err(PacketEncodeError::TooLong(complete_serialization_length));
                }

                full_packet_len_var_int
                    .encode(&mut header_cursor)
                    .map_err(|err| PacketEncodeError::Message(err.to_string()))?;
                data_len_var_int
                    .encode(&mut header_cursor)
                    .map_err(|err| PacketEncodeError::Message(err.to_string()))?;

                self.compression_scratch.as_slice()
            } else {
                let data_len_var_int: VarInt = 0.into();
                let full_packet_len_var_int: VarInt = (data_len_var_int.written_size() + data_len)
                    .try_into()
                    .map_err(|_| {
                        PacketEncodeError::Message(format!(
                            "Full packet length is too large to fit in VarInt! ({data_len})"
                        ))
                    })?;

                let complete_serialization_length =
                    full_packet_len_var_int.written_size() + full_packet_len_var_int.0 as usize;
                if complete_serialization_length > MAX_PACKET_SIZE as usize {
                    return Err(PacketEncodeError::TooLong(complete_serialization_length));
                }

                full_packet_len_var_int
                    .encode(&mut header_cursor)
                    .map_err(|err| PacketEncodeError::Message(err.to_string()))?;
                data_len_var_int
                    .encode(&mut header_cursor)
                    .map_err(|err| PacketEncodeError::Message(err.to_string()))?;

                packet_data
            }
        } else {
            let full_packet_len_var_int: VarInt = data_len_var_int;

            let complete_serialization_length =
                full_packet_len_var_int.written_size() + full_packet_len_var_int.0 as usize;
            if complete_serialization_length > MAX_PACKET_SIZE as usize {
                return Err(PacketEncodeError::TooLong(complete_serialization_length));
            }

            full_packet_len_var_int
                .encode(&mut header_cursor)
                .map_err(|err| PacketEncodeError::Message(err.to_string()))?;

            packet_data
        };

        let header_len = header_cursor.position() as usize;
        let header_bytes = &header_buf[..header_len];

        out.reserve(header_len + payload_to_write.len());
        out.extend_from_slice(header_bytes);
        out.extend_from_slice(payload_to_write);

        Ok(())
    }

    pub async fn flush(&mut self) -> Result<(), PacketEncodeError> {
        self.writer
            .as_mut()
            .ok_or_else(|| PacketEncodeError::Message("Writer missing".into()))?
            .flush()
            .await
            .map_err(|err| PacketEncodeError::Message(err.to_string()))
    }
}

pub fn write_packet<P: ClientPacket + ?Sized>(
    packet: &P,
    version: &JavaMinecraftVersion,
    mut write: impl std::io::Write,
) -> Result<(), WritingError> {
    let version_number = P::to_id(*version);
    if version_number == -1 {
        return Err(WritingError::UnsupportedVersion(*version));
    }
    write.write_var_int(&VarInt(version_number))?;
    packet.write_packet_data(write, version)
}

pub fn serialize_packet<P: ClientPacket + ?Sized>(
    packet: &P,
    version: &JavaMinecraftVersion,
) -> Result<Bytes, WritingError> {
    let mut packet_buf = Vec::new();
    write_packet(packet, version, &mut packet_buf)?;
    Ok(packet_buf.into())
}

#[derive(Error, Debug)]
#[error("Invalid compression Level")]
pub struct CompressionLevelError;

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;
    use crate::java::client::status::CStatusResponse;
    use crate::packet::MultiVersionJavaPacket;
    use crate::ser::{NetworkReadExt, NetworkWriteExt};
    use crate::{ClientPacket, ReadingError};
    use aes::Aes128;
    use cfb8::Decryptor as Cfb8Decryptor;
    use flate2::read::ZlibDecoder;
    use pumpkin_data::packet::clientbound::status::STATUS_RESPONSE;
    use pumpkin_macros::java_packet;
    use pumpkin_util::version::JavaMinecraftVersion;

    /// Define a custom packet for testing maximum packet size
    #[java_packet(STATUS_RESPONSE)]
    pub struct MaxSizePacket {
        data: Vec<u8>,
    }

    impl MaxSizePacket {
        pub fn new(size: usize) -> Self {
            Self {
                data: vec![0xAB; size], // Fill with arbitrary data
            }
        }
    }

    impl ClientPacket for MaxSizePacket {
        fn write_packet_data(
            &self,
            mut write: impl std::io::Write,
            _version: &JavaMinecraftVersion,
        ) -> Result<(), crate::WritingError> {
            write
                .write_all(&self.data)
                .map_err(crate::WritingError::IoError)?;
            Ok(())
        }
    }

    /// Helper function to decode a `VarInt` from bytes
    fn decode_varint(buffer: &mut &[u8]) -> Result<i32, ReadingError> {
        Ok(buffer.get_var_int()?.0)
    }

    /// Helper function to decompress data using libdeflater's Zlib decompressor
    fn decompress_zlib(data: &[u8], expected_size: usize) -> Result<Vec<u8>, std::io::Error> {
        assert!(!data.is_empty());
        let mut decompressed = vec![0u8; expected_size];
        ZlibDecoder::new(data).read_exact(&mut decompressed)?;
        Ok(decompressed)
    }

    /// Helper function to decrypt data using AES-128 CFB-8 mode
    fn decrypt_aes128(
        encrypted_data: &mut [u8],
        key: &[u8; 16],
        iv: &[u8; 16],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut decryptor =
            Cfb8Decryptor::<Aes128>::new_from_slices(key, iv).map_err(|_| "Invalid key/iv")?;
        decryptor.decrypt(encrypted_data);
        Ok(())
    }

    /// Helper function to build a packet with optional compression and encryption
    async fn build_packet_with_encoder<T: ClientPacket>(
        packet: &T,
        compression_info: Option<(CompressionThreshold, CompressionLevel)>,
        key: Option<&[u8; 16]>,
    ) -> Result<Box<[u8]>, Box<dyn std::error::Error>> {
        let mut buf = Vec::new();
        let mut encoder = TCPNetworkEncoder::new(&mut buf);
        if let Some(compression_info) = compression_info {
            encoder.set_compression(compression_info);
        }

        if let Some(key) = key {
            encoder.set_encryption(key).map_err(|e| e.to_string())?;
        }

        let mut packet_buf = Vec::new();
        let writer = &mut packet_buf;
        writer.write_var_int(&VarInt(T::to_id(JavaMinecraftVersion::V_1_21_11)))?;
        packet.write_packet_data(writer, &JavaMinecraftVersion::V_1_21_11)?;

        encoder
            .write_packet(packet_buf.into())
            .await
            .map_err(|e| e.to_string())?;

        Ok(buf.into_boxed_slice())
    }

    /// Test encoding without compression and encryption
    #[tokio::test]
    async fn encode_without_compression_and_encryption() -> Result<(), Box<dyn std::error::Error>> {
        // Create a CStatusResponse packet
        let packet =
            CStatusResponse::new(String::from("{\"description\": \"A Minecraft Server\"}"));

        // Build the packet without compression and encryption
        let packet_bytes = build_packet_with_encoder(&packet, None, None).await?;

        // Decode the packet manually to verify correctness
        let mut buffer = &packet_bytes[..];

        // Read packet length VarInt
        let packet_length = decode_varint(&mut buffer).map_err(|e| e.to_string())?;
        assert_eq!(
            packet_length as usize,
            buffer.len(),
            "Packet length mismatch"
        );

        // Read packet ID VarInt
        let decoded_packet_id = decode_varint(&mut buffer).map_err(|e| e.to_string())?;
        assert_eq!(
            decoded_packet_id,
            CStatusResponse::to_id(JavaMinecraftVersion::V_1_21_11)
        );

        // Remaining buffer is the payload
        // We need to obtain the expected payload
        let mut expected_payload = Vec::new();
        packet.write_packet_data(&mut expected_payload, &JavaMinecraftVersion::V_1_21_11)?;

        assert_eq!(buffer, expected_payload);
        Ok(())
    }

    /// Test encoding with compression
    #[tokio::test]
    async fn encode_with_compression() -> Result<(), Box<dyn std::error::Error>> {
        // Create a CStatusResponse packet
        let packet = CStatusResponse::new("{\"description\": \"A Minecraft Server\"}".to_string());

        // Build the packet with compression enabled
        let packet_bytes = build_packet_with_encoder(&packet, Some((0, 6)), None).await?;

        // Decode the packet manually to verify correctness
        let mut buffer = &packet_bytes[..];

        // Read packet length VarInt
        let packet_length = decode_varint(&mut buffer).map_err(|e| e.to_string())?;
        assert_eq!(
            packet_length as usize,
            buffer.len(),
            "Packet length mismatch"
        );

        // Read data length VarInt (uncompressed data length)
        let data_length = decode_varint(&mut buffer).map_err(|e| e.to_string())?;
        let mut expected_payload = Vec::new();
        packet.write_packet_data(&mut expected_payload, &JavaMinecraftVersion::V_1_21_11)?;
        let uncompressed_data_length =
            VarInt(CStatusResponse::to_id(JavaMinecraftVersion::V_1_21_11)).written_size()
                + expected_payload.len();
        assert_eq!(data_length as usize, uncompressed_data_length);

        // Remaining buffer is the compressed data
        let compressed_data = buffer;

        // Decompress the data
        let decompressed_data = decompress_zlib(compressed_data, data_length as usize)?;

        // Verify packet ID and payload
        let mut decompressed_buffer = &decompressed_data[..];

        // Read packet ID VarInt
        let decoded_packet_id =
            decode_varint(&mut decompressed_buffer).map_err(|e| e.to_string())?;
        assert_eq!(
            decoded_packet_id,
            CStatusResponse::to_id(JavaMinecraftVersion::V_1_21_11)
        );

        // Remaining buffer is the payload
        assert_eq!(decompressed_buffer, expected_payload);
        Ok(())
    }

    /// Test encoding with encryption
    #[tokio::test]
    async fn encode_with_encryption() -> Result<(), Box<dyn std::error::Error>> {
        // Create a CStatusResponse packet
        let packet = CStatusResponse::new("{\"description\": \"A Minecraft Server\"}".to_string());

        // Encryption key and IV (IV is the same as key in this case)
        let key = [0x00u8; 16]; // Example key

        // Build the packet with encryption enabled (no compression)
        let mut packet_bytes = build_packet_with_encoder(&packet, None, Some(&key)).await?;

        // Decrypt the packet
        decrypt_aes128(&mut packet_bytes, &key, &key)?;

        // Decode the packet manually to verify correctness
        let mut buffer = &packet_bytes[..];

        // Read packet length VarInt
        let packet_length = decode_varint(&mut buffer).map_err(|e| e.to_string())?;
        assert_eq!(
            packet_length as usize,
            buffer.len(),
            "Packet length mismatch"
        );

        // Read packet ID VarInt
        let decoded_packet_id = decode_varint(&mut buffer).map_err(|e| e.to_string())?;
        assert_eq!(
            decoded_packet_id,
            CStatusResponse::to_id(JavaMinecraftVersion::V_1_21_11)
        );

        // Remaining buffer is the payload
        let mut expected_payload = Vec::new();
        packet.write_packet_data(&mut expected_payload, &JavaMinecraftVersion::V_1_21_11)?;
        assert_eq!(buffer, expected_payload);
        Ok(())
    }

    /// Test encoding with both compression and encryption
    #[tokio::test]
    async fn encode_with_compression_and_encryption() -> Result<(), Box<dyn std::error::Error>> {
        // Create a CStatusResponse packet
        let packet = CStatusResponse::new("{\"description\": \"A Minecraft Server\"}".to_string());

        // Encryption key and IV (IV is the same as key in this case)
        let key = [0x01u8; 16]; // Example key

        // Build the packet with both compression and encryption enabled
        // Compression threshold is set to 0 to force compression
        let mut packet_bytes = build_packet_with_encoder(&packet, Some((0, 6)), Some(&key)).await?;

        // Decrypt the packet
        decrypt_aes128(&mut packet_bytes, &key, &key)?;

        // Decode the packet manually to verify correctness
        let mut buffer = &packet_bytes[..];

        // Read packet length VarInt
        let packet_length = decode_varint(&mut buffer).map_err(|e| e.to_string())?;
        assert_eq!(
            packet_length as usize,
            buffer.len(),
            "Packet length mismatch"
        );

        // Read data length VarInt (uncompressed data length)
        let data_length = decode_varint(&mut buffer).map_err(|e| e.to_string())?;
        let mut expected_payload = Vec::new();
        packet.write_packet_data(&mut expected_payload, &JavaMinecraftVersion::V_1_21_11)?;
        let uncompressed_data_length =
            VarInt(CStatusResponse::to_id(JavaMinecraftVersion::V_1_21_11)).written_size()
                + expected_payload.len();
        assert_eq!(data_length as usize, uncompressed_data_length);

        // Remaining buffer is the compressed data
        let compressed_data = buffer;

        // Decompress the data
        let decompressed_data = decompress_zlib(compressed_data, data_length as usize)?;

        // Verify packet ID and payload
        let mut decompressed_buffer = &decompressed_data[..];

        // Read packet ID VarInt
        let decoded_packet_id =
            decode_varint(&mut decompressed_buffer).map_err(|e| e.to_string())?;
        assert_eq!(
            decoded_packet_id,
            CStatusResponse::to_id(JavaMinecraftVersion::V_1_21_11)
        );

        // Remaining buffer is the payload
        assert_eq!(decompressed_buffer, expected_payload);
        Ok(())
    }

    /// Test encoding with zero-length payload
    #[tokio::test]
    async fn encode_with_zero_length_payload() -> Result<(), Box<dyn std::error::Error>> {
        // Create a CStatusResponse packet with empty payload
        let packet = CStatusResponse::new(String::new());

        // Build the packet without compression and encryption
        let packet_bytes = build_packet_with_encoder(&packet, None, None).await?;

        // Decode the packet manually to verify correctness
        let mut buffer = &packet_bytes[..];

        // Read packet length VarInt
        let packet_length = decode_varint(&mut buffer).map_err(|e| e.to_string())?;
        assert_eq!(
            packet_length as usize,
            buffer.len(),
            "Packet length mismatch"
        );

        // Read packet ID VarInt
        let decoded_packet_id = decode_varint(&mut buffer).map_err(|e| e.to_string())?;
        assert_eq!(
            decoded_packet_id,
            CStatusResponse::to_id(JavaMinecraftVersion::V_1_21_11)
        );

        // Remaining buffer is the payload (empty)
        let mut expected_payload = Vec::new();
        packet.write_packet_data(&mut expected_payload, &JavaMinecraftVersion::V_1_21_11)?;

        assert_eq!(
            buffer.len(),
            expected_payload.len(),
            "Payload length mismatch"
        );
        assert_eq!(buffer, expected_payload);
        Ok(())
    }

    /// Test encoding with maximum length payload
    #[tokio::test]
    async fn encode_with_maximum_string_length() -> Result<(), Box<dyn std::error::Error>> {
        // Maximum allowed string length is 32767 bytes
        let max_string_length = 32767;
        let payload_str = "A".repeat(max_string_length);
        let packet = CStatusResponse::new(payload_str);

        // Build the packet without compression and encryption
        let packet_bytes = build_packet_with_encoder(&packet, None, None).await?;

        // Verify that the packet size does not exceed MAX_PACKET_SIZE as usize
        assert!(
            packet_bytes.len() <= MAX_PACKET_SIZE as usize,
            "Packet size exceeds maximum allowed size"
        );

        // Decode the packet manually to verify correctness
        let mut buffer = &packet_bytes[..];

        // Read packet length VarInt
        let packet_length = decode_varint(&mut buffer).map_err(|e| e.to_string())?;
        assert_eq!(
            packet_length as usize,
            buffer.len(),
            "Packet length mismatch"
        );

        // Read packet ID VarInt
        let decoded_packet_id = decode_varint(&mut buffer).map_err(|e| e.to_string())?;
        // Assume packet ID is 0 for CStatusResponse
        assert_eq!(
            decoded_packet_id,
            CStatusResponse::to_id(JavaMinecraftVersion::V_1_21_11)
        );

        // Remaining buffer is the payload
        let mut expected_payload = Vec::new();
        packet.write_packet_data(&mut expected_payload, &JavaMinecraftVersion::V_1_21_11)?;

        assert_eq!(buffer, expected_payload);
        Ok(())
    }

    /// Test encoding a packet that exceeds `MAX_PACKET_SIZE` as usize
    #[tokio::test]
    async fn encode_packet_exceeding_maximum_size() -> Result<(), Box<dyn std::error::Error>> {
        // Create a custom packet with data exceeding MAX_PACKET_SIZE as usize
        let data_size = MAX_PACKET_SIZE as usize + 1; // Exceed by 1 byte
        let packet = MaxSizePacket::new(data_size);

        // Build the packet without compression and encryption
        // This should return PacketEncodeError::TooLong
        let result = build_packet_with_encoder(&packet, None, None).await;
        assert!(result.is_err());
        // We can't easily check for TooLong since it's boxed, but we verified it returns an error
        Ok(())
    }

    /// Test encoding with a small payload that should not be compressed
    #[tokio::test]
    async fn encode_small_payload_no_compression() -> Result<(), Box<dyn std::error::Error>> {
        // Create a CStatusResponse packet with small payload
        let packet = CStatusResponse::new(String::from("Hi"));

        // Build the packet with compression enabled
        // Compression threshold is set to a value higher than payload length
        let packet_bytes = build_packet_with_encoder(&packet, Some((10, 6)), None).await?;

        // Decode the packet manually to verify that it was not compressed
        let mut buffer = &packet_bytes[..];

        // Read packet length VarInt
        let packet_length = decode_varint(&mut buffer).map_err(|e| e.to_string())?;
        assert_eq!(
            packet_length as usize,
            buffer.len(),
            "Packet length mismatch"
        );

        // Read data length VarInt (should be 0 indicating no compression)
        let data_length = decode_varint(&mut buffer).map_err(|e| e.to_string())?;
        assert_eq!(
            data_length, 0,
            "Data length should be 0 indicating no compression"
        );

        // Read packet ID VarInt
        let decoded_packet_id = decode_varint(&mut buffer).map_err(|e| e.to_string())?;
        assert_eq!(
            decoded_packet_id,
            CStatusResponse::to_id(JavaMinecraftVersion::V_1_21_11)
        );

        // Remaining buffer is the payload
        let mut expected_payload = Vec::new();
        packet.write_packet_data(&mut expected_payload, &JavaMinecraftVersion::V_1_21_11)?;

        assert_eq!(buffer, expected_payload);
        Ok(())
    }

    const SPLIT_PAYLOAD_ID: i32 = 0x18;

    /// An encoded packet of `len` bytes: id 0x2a, then a counting body.
    fn encoded_packet(len: usize) -> Bytes {
        let mut packet = vec![0x2a];
        packet.extend((1..len).map(|i| i as u8));
        packet.into()
    }

    /// Encodes `packet` and reads the frames back with the decoder.
    async fn encode_and_decode(
        packet: &Bytes,
        compression: Option<CompressionThreshold>,
        split: bool,
    ) -> Result<Vec<crate::RawPacket>, PacketEncodeError> {
        let mut buf = Vec::new();
        let mut encoder = TCPNetworkEncoder::new(&mut buf);
        if let Some(threshold) = compression {
            encoder.set_compression((threshold, 6));
        }
        encoder.set_split_payload_id(split.then_some(SPLIT_PAYLOAD_ID));
        encoder.write_packet(packet.clone()).await?;
        let mut decoder = crate::java::packet_decoder::TCPNetworkDecoder::new(&buf[..]);
        if let Some(threshold) = compression {
            decoder.set_compression(threshold);
        }
        let mut packets = Vec::new();
        loop {
            match decoder.get_raw_packet().await {
                Ok(packet) => packets.push(packet),
                Err(crate::PacketDecodeError::ConnectionClosed) => break,
                Err(err) => panic!("undecodable frame: {err}"),
            }
        }
        Ok(packets)
    }

    /// The states of the parts and the packet their slices join to. The server's reassembler caps
    /// the join at 8 MiB, so the slices are joined here.
    fn join_parts(parts: Vec<crate::RawPacket>) -> (Vec<u8>, Vec<u8>) {
        use crate::java::neoforge::{SplitPacketPayload, decode_exact};
        let mut states = Vec::new();
        let mut joined = Vec::new();
        for part in parts {
            assert_eq!(part.id, SPLIT_PAYLOAD_ID);
            let data = SplitPacketPayload::data_of(&part.payload).unwrap();
            let payload = decode_exact(data, SplitPacketPayload::read)
                .unwrap()
                .payload;
            states.push(payload[0]);
            joined.extend_from_slice(&payload[1..]);
        }
        (states, joined)
    }

    #[tokio::test]
    async fn splits_a_9_mib_packet() {
        let packet = encoded_packet(9 * 1024 * 1024);
        for (compression, expected_states) in [(None, &[1, 0, 0, 0, 2][..]), (Some(256), &[1, 2])] {
            let parts = encode_and_decode(&packet, compression, true).await.unwrap();
            let (states, joined) = join_parts(parts);
            assert_eq!(states, expected_states);
            assert_eq!(joined, packet);
        }
    }

    #[tokio::test]
    async fn sends_a_packet_below_the_limit_unsplit() {
        // The largest frozen registry payload of the NeoForge capture.
        for (len, compression) in [
            (71_788, None),
            (71_788, Some(256)),
            (3 * 1024 * 1024, Some(256)),
        ] {
            let packet = encoded_packet(len);
            let packets = encode_and_decode(&packet, compression, true).await.unwrap();
            assert_eq!(packets.len(), 1);
            assert_eq!(packets[0].id, 0x2a);
            assert_eq!(packets[0].payload, packet[1..]);
        }
    }

    #[tokio::test]
    async fn never_splits_without_the_channel() {
        let packet = encoded_packet(9 * 1024 * 1024);
        for compression in [None, Some(256)] {
            assert!(matches!(
                encode_and_decode(&packet, compression, false).await,
                Err(PacketEncodeError::TooLong(_))
            ));
        }
    }
}
