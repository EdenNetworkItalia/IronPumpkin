//! Broadcasts serialize once per encoding key.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use std::io::Write;
use std::sync::Weak;
use std::sync::atomic::{AtomicUsize, Ordering};

use arc_swap::ArcSwap;
use bytes::Bytes;
use pumpkin_config::PacketLimiterConfig;
use pumpkin_data::dynamic::ContentIds;
use pumpkin_protocol::ser::{NetworkWriteExt, WritingError};
use pumpkin_protocol::{ClientPacket, EncodingKey, MultiVersionJavaPacket};
use pumpkin_util::version::JavaMinecraftVersion;
use tokio::net::{TcpListener, TcpStream};
use uuid::Uuid;

use super::outgoing::OutgoingPacket;
use super::{JavaClient, PendingConnection};
use crate::net::java::neoforge::NegotiatedState;
use crate::net::packet_limiter::PacketRateLimiter;
use crate::net::{GameProfile, PlayerConfig};
use crate::world::World;

/// Counts its serializations and writes the content id mode it was encoded for.
struct CountedPacket(AtomicUsize);

impl MultiVersionJavaPacket for CountedPacket {
    fn to_id(_version: JavaMinecraftVersion) -> i32 {
        0
    }
}

impl ClientPacket for CountedPacket {
    fn write_packet_data(
        &self,
        mut write: impl Write,
        version: &EncodingKey,
    ) -> Result<(), WritingError> {
        self.0.fetch_add(1, Ordering::Relaxed);
        write.write_u8(u8::from(version.content_ids() == ContentIds::Real))
    }
}

/// A play client with this content id mode. The peer socket is returned so the connection
/// stays open.
async fn client(content_ids: ContentIds) -> (JavaClient, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let peer = TcpStream::connect(listener.local_addr().unwrap())
        .await
        .unwrap();
    let (stream, address) = listener.accept().await.unwrap();
    let pending = PendingConnection::new(
        stream,
        address,
        0,
        PacketRateLimiter::from_config(&PacketLimiterConfig::default()),
        Weak::new(),
    );
    let profile = GameProfile {
        id: Uuid::nil(),
        name: "test".to_string(),
        properties: ArcSwap::from_pointee(Vec::new()),
        profile_actions: None,
    };
    let negotiated = NegotiatedState {
        content_ids,
        ..NegotiatedState::default()
    };
    let client = JavaClient::from_pending(pending, profile, PlayerConfig::default(), negotiated);
    (client, peer)
}

fn queued(client: &mut JavaClient) -> Vec<Bytes> {
    let receiver = client.outgoing_packet_queue_recv.as_mut().unwrap();
    let mut packets = Vec::new();
    while let Ok(packet) = receiver.try_recv() {
        if let OutgoingPacket::Data { data, .. } = packet {
            packets.push(data);
        }
    }
    packets
}

#[tokio::test]
async fn mixed_recipients_serialize_once_per_mode() {
    let (mut real_a, _peer_a) = client(ContentIds::Real).await;
    let (mut real_b, _peer_b) = client(ContentIds::Real).await;
    let (mut display, _peer_c) = client(ContentIds::Display).await;
    let packet = CountedPacket(AtomicUsize::new(0));

    World::broadcast_java_clients(&packet, [&real_a, &display, &real_b].into_iter());

    assert_eq!(packet.0.load(Ordering::Relaxed), 2);
    let real_a = queued(&mut real_a);
    assert_eq!(real_a, queued(&mut real_b));
    assert_eq!(real_a, [Bytes::from_static(&[0, 1])]);
    assert_eq!(queued(&mut display), [Bytes::from_static(&[0, 0])]);
}

#[tokio::test]
async fn one_mode_serializes_once() {
    let (first, _peer_a) = client(ContentIds::Display).await;
    let (second, _peer_b) = client(ContentIds::Display).await;
    let packet = CountedPacket(AtomicUsize::new(0));

    World::broadcast_java_clients(&packet, [&first, &second].into_iter());

    assert_eq!(packet.0.load(Ordering::Relaxed), 1);
}
