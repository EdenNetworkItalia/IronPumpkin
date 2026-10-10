use std::collections::BTreeSet;
use std::io::Write;

use bytes::Bytes;
use pumpkin_util::identifier::Identifier;

use super::{NetworkPayloadSetup, read_byte_array, read_count, write_byte_array, write_count};
use crate::{
    MAX_PACKET_DATA_SIZE, MAX_PACKET_SIZE, RawPacket, VarInt,
    ser::{NetworkReadExt, NetworkReadSliceExt, NetworkWriteExt, ReadingError, WritingError},
};

/// `GenericPacketSplitter.STATE_FIRST`.
const STATE_FIRST: u8 = 1;
/// The state `GenericPacketSplitter.encode` gives every part between the first and the last.
const STATE_MIDDLE: u8 = 0;
/// `GenericPacketSplitter.STATE_LAST`.
const STATE_LAST: u8 = 2;

/// Mirrors `SplitPacketPayload`: one slice of a payload too large for a single packet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitPacketPayload {
    pub payload: Box<[u8]>,
}

impl SplitPacketPayload {
    pub const CHANNEL: &'static str = "neoforge:split";

    /// `determineMaxPayloadSize`: the bytes a part packet adds around its slice. A 1-byte packet
    /// id, the channel, the state byte, a 5-byte `VarInt` and a 4-byte `Int`.
    const PART_PREFIX_SIZE: usize = 1 + 1 + Self::CHANNEL.len() + 1 + 5 + 4;

    pub fn read(read: &mut &[u8]) -> Result<Self, ReadingError> {
        Ok(Self {
            payload: read_byte_array(read)?,
        })
    }

    pub fn write(&self, mut write: impl Write) -> Result<(), WritingError> {
        write_byte_array(&mut write, &self.payload)
    }

    /// `GenericPacketSplitter.isRemoteCompatible`: `NetworkRegistry.hasChannel` without a
    /// protocol, so the channel counts when it was negotiated for any protocol or registered ad hoc.
    #[must_use]
    pub fn is_declared(setup: &NetworkPayloadSetup, ad_hoc: &BTreeSet<Identifier>) -> bool {
        let id = Identifier::parse_static(Self::CHANNEL);
        setup.channels.values().any(|map| map.contains_key(&id)) || ad_hoc.contains(&id)
    }

    /// The data of a `custom_payload` packet body (channel, then data) on this channel.
    ///
    /// `None` for any other channel or a body without a readable channel. The data has no vanilla
    /// custom payload bound: `NeoForge` reads it with the payload's own codec.
    #[must_use]
    pub fn data_of(mut custom_payload_body: &[u8]) -> Option<&[u8]> {
        let channel = custom_payload_body.get_str_borrowed().ok()?;
        (channel == Self::CHANNEL).then_some(custom_payload_body)
    }
}

/// The size limits of `GenericPacketSplitter.encode`: `MAXIMUM_UNCOMPRESSED_LENGTH` when the
/// connection compresses (the compressor runs after the splitter), `MAXIMUM_COMPRESSED_LENGTH`
/// when it does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitLimits {
    /// The largest encoded packet that goes out unsplit.
    pub packet: usize,
    /// The largest slice in one part.
    pub part: usize,
}

impl SplitLimits {
    #[must_use]
    pub const fn new(compressed: bool) -> Self {
        let packet = if compressed {
            MAX_PACKET_DATA_SIZE
        } else {
            MAX_PACKET_SIZE as usize
        };
        Self {
            packet,
            part: packet - SplitPacketPayload::PART_PREFIX_SIZE,
        }
    }

    /// `GenericPacketSplitter.encode`: the slices of an encoded packet (id and body) with their
    /// state, or `None` when the packet goes out unsplit.
    #[must_use]
    pub fn split(self, packet: &[u8]) -> Option<impl Iterator<Item = (u8, &[u8])>> {
        if packet.len() <= self.packet {
            return None;
        }
        let last = packet.len().div_ceil(self.part) - 1;
        Some(
            packet
                .chunks(self.part)
                .enumerate()
                .map(move |(index, slice)| {
                    let state = match index {
                        0 => STATE_FIRST,
                        i if i == last => STATE_LAST,
                        _ => STATE_MIDDLE,
                    };
                    (state, slice)
                }),
        )
    }
}

/// Writes the encoded `custom_payload` packet of one part: the packet id, the channel, then the
/// payload byte array of the state and the slice.
pub fn write_split_part(
    custom_payload_id: i32,
    state: u8,
    slice: &[u8],
    out: &mut Vec<u8>,
) -> Result<(), WritingError> {
    out.write_var_int(&VarInt(custom_payload_id))?;
    out.write_string(SplitPacketPayload::CHANNEL)?;
    write_count(out, slice.len() + 1)?;
    out.write_u8(state)?;
    out.write_slice(slice)
}

/// A packet that [`SplitPacketReassembler::accept`] gives back.
pub enum Accepted {
    /// A packet that was not split: its body ends where the packet ends.
    Whole(RawPacket),
    /// A packet joined from `neoforge:split` parts: bytes can follow its body.
    Joined(RawPacket),
}

impl Accepted {
    #[must_use]
    pub const fn is_joined(&self) -> bool {
        matches!(self, Self::Joined(_))
    }

    #[must_use]
    pub fn into_packet(self) -> RawPacket {
        match self {
            Self::Whole(packet) | Self::Joined(packet) => packet,
        }
    }
}

/// The receiving side of `GenericPacketSplitter`, stricter than `NeoForge`.
///
/// A part out of order or a joined packet above 8 MiB is an error, and the caller disconnects the
/// client. Each connection has its own reassembler, so one client can make the server buffer up
/// to 8 MiB.
///
/// A joined packet can end with padding. `GenericPacketSplitter.encode` counts the parts from
/// `readableBytes()` but slices `buf.array()`, the whole backing array of the buffer, so the last
/// part runs to `min(part size, capacity - offset)` and carries the unused capacity. `NeoForge`
/// decodes the joined packet with the packet codec and never checks for bytes left over. A reader
/// of a joined packet must likewise stop at the end of the body: [`Accepted::Joined`] marks it,
/// and the payload of a joined `custom_payload` can carry the padding after its own data.
#[derive(Debug, Default)]
pub struct SplitPacketReassembler {
    /// The slices since the first part. `None` between packets.
    joined: Option<Vec<u8>>,
}

impl SplitPacketReassembler {
    /// Takes a received packet. A `custom_payload` (id `custom_payload_id`) on `neoforge:split` is
    /// a part: it gives `None` until the last part, which gives the joined packet. Any other packet
    /// comes back unchanged.
    pub fn accept(
        &mut self,
        packet: RawPacket,
        custom_payload_id: i32,
    ) -> Result<Option<Accepted>, ReadingError> {
        if packet.id != custom_payload_id {
            return Ok(Some(Accepted::Whole(packet)));
        }
        let Some(data) = SplitPacketPayload::data_of(&packet.payload) else {
            return Ok(Some(Accepted::Whole(packet)));
        };
        Ok(self.accept_part(data)?.map(Accepted::Joined))
    }

    fn accept_part(&mut self, mut data: &[u8]) -> Result<Option<RawPacket>, ReadingError> {
        let len = read_count(&mut data)?;
        let slice = data.read_slice_borrowed(len)?;
        if !data.is_empty() {
            return Err(ReadingError::TooLarge(format!(
                "{} bytes left over after the neoforge:split payload",
                data.len()
            )));
        }
        let Some((&state, content)) = slice.split_first() else {
            return Err(ReadingError::Message("Empty neoforge:split part".into()));
        };
        let joined = match (state, self.joined.as_mut()) {
            (STATE_FIRST, None) => self.joined.insert(Vec::new()),
            (STATE_MIDDLE | STATE_LAST, Some(joined)) => joined,
            (STATE_FIRST, Some(_)) => {
                return Err(ReadingError::Message(
                    "neoforge:split first part before the last part of the previous packet".into(),
                ));
            }
            (STATE_MIDDLE | STATE_LAST, None) => {
                return Err(ReadingError::Message(
                    "neoforge:split part without a first part".into(),
                ));
            }
            (state, _) => {
                return Err(ReadingError::Message(format!(
                    "Invalid neoforge:split state {state}"
                )));
            }
        };
        if joined.len() + content.len() > MAX_PACKET_DATA_SIZE {
            return Err(ReadingError::TooLarge(format!(
                "neoforge:split packet above {MAX_PACKET_DATA_SIZE} bytes"
            )));
        }
        joined.extend_from_slice(content);
        if state != STATE_LAST {
            return Ok(None);
        }
        let joined = Bytes::from(self.joined.take().unwrap_or_default());
        let mut read = &joined[..];
        let id = read.get_var_int()?.0;
        let payload = joined.slice(joined.len() - read.len()..);
        Ok(Some(RawPacket { id, payload }))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::java::neoforge::{ConnectionProtocol, NetworkChannel};

    const CUSTOM_PAYLOAD: i32 = 0x18;

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

    /// The values `determineMaxPayloadSize` returns for the two `CompressionDecoder` limits.
    #[test]
    fn part_size_matches_neoforge() {
        assert_eq!(SplitLimits::new(true).part, 8_388_608 - 26);
        assert_eq!(SplitLimits::new(false).part, 2_097_152 - 26);
    }

    /// An encoded packet of `len` bytes: id 0x2a, then a counting body.
    fn encoded_packet(len: usize) -> Vec<u8> {
        let mut packet = vec![0x2a];
        packet.extend((1..len).map(|i| i as u8));
        packet
    }

    fn part_packets(limits: SplitLimits, packet: &[u8]) -> Vec<RawPacket> {
        limits
            .split(packet)
            .unwrap()
            .map(|(state, slice)| {
                let mut bytes = Vec::new();
                write_split_part(CUSTOM_PAYLOAD, state, slice, &mut bytes).unwrap();
                let mut read = &bytes[..];
                let id = read.get_var_int().unwrap().0;
                RawPacket {
                    id,
                    payload: Bytes::copy_from_slice(read),
                }
            })
            .collect()
    }

    #[test]
    fn splits_like_neoforge() {
        let limits = SplitLimits::new(false);
        assert!(limits.split(&encoded_packet(limits.packet)).is_none());
        let packet = encoded_packet(9 * 1024 * 1024);
        let parts: Vec<(u8, &[u8])> = limits.split(&packet).unwrap().collect();
        let states: Vec<u8> = parts.iter().map(|(state, _)| *state).collect();
        assert_eq!(states, [1, 0, 0, 0, 2]);
        assert!(parts.iter().all(|(_, slice)| slice.len() <= limits.part));
        let joined: Vec<u8> = parts
            .iter()
            .flat_map(|(_, slice)| *slice)
            .copied()
            .collect();
        assert_eq!(joined, packet);

        // With compression on, 9 MiB needs only a first and a last part.
        let states: Vec<u8> = SplitLimits::new(true)
            .split(&packet)
            .unwrap()
            .map(|(state, _)| state)
            .collect();
        assert_eq!(states, [1, 2]);
    }

    #[test]
    fn part_packet_fits_the_limit() {
        for compressed in [false, true] {
            let limits = SplitLimits::new(compressed);
            let packet = encoded_packet(limits.packet + 1);
            let (state, slice) = limits.split(&packet).unwrap().next().unwrap();
            let mut bytes = Vec::new();
            write_split_part(CUSTOM_PAYLOAD, state, slice, &mut bytes).unwrap();
            assert!(bytes.len() <= limits.packet);
        }
    }

    #[test]
    fn joins_three_parts_once() {
        let packet = encoded_packet(5 * 1024 * 1024);
        let parts = part_packets(SplitLimits::new(false), &packet);
        assert_eq!(parts.len(), 3);
        let mut reassembler = SplitPacketReassembler::default();
        let handled: Vec<RawPacket> = parts
            .into_iter()
            .filter_map(|part| reassembler.accept(part, CUSTOM_PAYLOAD).unwrap())
            .inspect(|accepted| assert!(accepted.is_joined()))
            .map(Accepted::into_packet)
            .collect();
        assert_eq!(handled.len(), 1);
        assert_eq!(handled[0].id, 0x2a);
        assert_eq!(handled[0].payload, packet[1..]);
        assert!(reassembler.joined.is_none());
    }

    /// The last part of `GenericPacketSplitter.encode` carries the unused capacity of its buffer.
    #[test]
    fn keeps_the_padding_of_the_last_part_after_the_body() {
        let packet = encoded_packet(5 * 1024 * 1024);
        let limits = SplitLimits::new(false);
        let mut padded = packet.clone();
        padded.resize(8 * 1024 * 1024, 0);
        let mut parts = Vec::new();
        let count = packet.len().div_ceil(limits.part);
        for (index, slice) in padded.chunks(limits.part).take(count).enumerate() {
            let state = match index {
                0 => STATE_FIRST,
                i if i == count - 1 => STATE_LAST,
                _ => STATE_MIDDLE,
            };
            let mut bytes = Vec::new();
            write_split_part(CUSTOM_PAYLOAD, state, slice, &mut bytes).unwrap();
            let mut read = &bytes[..];
            let id = read.get_var_int().unwrap().0;
            parts.push(RawPacket {
                id,
                payload: Bytes::copy_from_slice(read),
            });
        }
        let mut reassembler = SplitPacketReassembler::default();
        let joined: Vec<Accepted> = parts
            .into_iter()
            .filter_map(|part| reassembler.accept(part, CUSTOM_PAYLOAD).unwrap())
            .collect();
        let [Accepted::Joined(joined)] = &joined[..] else {
            panic!("the parts join into one packet");
        };
        assert_eq!(joined.id, 0x2a);
        assert!(joined.payload.len() > packet.len() - 1);
        assert_eq!(joined.payload[..packet.len() - 1], packet[1..]);
    }

    #[test]
    fn passes_other_packets_through() {
        let mut reassembler = SplitPacketReassembler::default();
        let mut body = Vec::new();
        body.write_string("minecraft:brand").unwrap();
        body.extend_from_slice(b"\x07vanilla");
        for id in [CUSTOM_PAYLOAD, 0x0c] {
            let packet = RawPacket {
                id,
                payload: body.clone().into(),
            };
            let back = reassembler.accept(packet, CUSTOM_PAYLOAD).unwrap().unwrap();
            assert!(!back.is_joined());
            let back = back.into_packet();
            assert_eq!((back.id, &back.payload[..]), (id, &body[..]));
        }
    }

    #[test]
    fn rejects_parts_out_of_order() {
        let packet = encoded_packet(5 * 1024 * 1024);
        let parts = part_packets(SplitLimits::new(false), &packet);
        let copy = |index: usize| RawPacket {
            id: parts[index].id,
            payload: parts[index].payload.clone(),
        };

        // A middle or a last part with no first part.
        for index in [1, 2] {
            let mut reassembler = SplitPacketReassembler::default();
            assert!(reassembler.accept(copy(index), CUSTOM_PAYLOAD).is_err());
        }
        // A first part while a packet is unfinished.
        let mut reassembler = SplitPacketReassembler::default();
        assert!(
            reassembler
                .accept(copy(0), CUSTOM_PAYLOAD)
                .unwrap()
                .is_none()
        );
        assert!(reassembler.accept(copy(0), CUSTOM_PAYLOAD).is_err());
    }

    #[test]
    fn rejects_a_join_above_8_mib() {
        let limits = SplitLimits::new(false);
        let parts = part_packets(limits, &encoded_packet(MAX_PACKET_DATA_SIZE + 1));
        let mut reassembler = SplitPacketReassembler::default();
        // The parts up to 8 MiB are taken; the one that crosses it is refused.
        let refused = parts
            .into_iter()
            .map(|part| reassembler.accept(part, CUSTOM_PAYLOAD))
            .find(Result::is_err);
        assert!(matches!(refused, Some(Err(ReadingError::TooLarge(_)))));

        let mut reassembler = SplitPacketReassembler::default();
        let joined = part_packets(limits, &encoded_packet(MAX_PACKET_DATA_SIZE))
            .into_iter()
            .filter_map(|part| reassembler.accept(part, CUSTOM_PAYLOAD).unwrap())
            .count();
        assert_eq!(joined, 1);
    }

    #[test]
    fn rejects_an_unknown_state_and_an_empty_part() {
        for data in [&[0x02, 0x03, 0xab][..], &[0x00]] {
            let mut body = Vec::new();
            body.write_string(SplitPacketPayload::CHANNEL).unwrap();
            body.extend_from_slice(data);
            let packet = RawPacket {
                id: CUSTOM_PAYLOAD,
                payload: body.into(),
            };
            let mut reassembler = SplitPacketReassembler::default();
            assert!(reassembler.accept(packet, CUSTOM_PAYLOAD).is_err());
        }
    }

    #[test]
    fn declared_by_setup_or_ad_hoc() {
        let id = Identifier::parse_static(SplitPacketPayload::CHANNEL);
        let mut setup = NetworkPayloadSetup::default();
        let none = BTreeSet::new();
        assert!(!SplitPacketPayload::is_declared(&setup, &none));
        assert!(SplitPacketPayload::is_declared(
            &setup,
            &BTreeSet::from([id.clone()])
        ));
        setup.channels.insert(
            ConnectionProtocol::Play,
            BTreeMap::from([(
                id.clone(),
                NetworkChannel {
                    id,
                    chosen_version: "1".to_owned(),
                },
            )]),
        );
        assert!(SplitPacketPayload::is_declared(&setup, &none));
    }
}
