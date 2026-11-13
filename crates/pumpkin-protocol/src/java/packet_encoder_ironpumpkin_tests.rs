use super::*;

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
