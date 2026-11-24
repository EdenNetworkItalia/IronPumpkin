//! The Java send path inside a Wasm guest call (`pumpkin_core::plugin::guest_call`): a send
//! defers only when a Wasm plugin handles `PacketSentEvent`. With native handlers only, the send
//! and its event run now. The Wasm handler here is a native handler that reports itself as one.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

mod support;

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use pumpkin_core::{
    entity::player::Player,
    net::{ClientPlatform, java::neoforge::NegotiatedState},
    plugin::{
        BoxFuture, EventHandler, EventPriority,
        guest_call::{self, GuestCall},
        server::packet::PacketSentEvent,
    },
    server::Server,
};
use pumpkin_protocol::{
    ConnectionState,
    java::{client::play::CSystemChatMessage, packet_decoder::TCPNetworkDecoder},
    packet::MultiVersionJavaPacket,
};
use pumpkin_util::text::TextComponent;

type Work = Box<dyn FnOnce() + Send>;

/// A guest call that keeps the work that the sends defer.
#[derive(Default)]
struct Recorder(Mutex<Vec<Work>>);

impl GuestCall for Recorder {
    fn defer(&self, _label: &str, run: Work) {
        self.0.lock().unwrap().push(run);
    }
}

/// A handler of `PacketSentEvent` that counts the chat packets of the player `name`.
struct CountChat {
    name: &'static str,
    wasm: bool,
    seen: Arc<AtomicUsize>,
}

impl EventHandler<PacketSentEvent> for CountChat {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut PacketSentEvent,
    ) -> BoxFuture<'a, ()> {
        if event.player.gameprofile.name == self.name
            && event.packet_id
                == CSystemChatMessage::to_id(pumpkin_data::packet::CURRENT_MC_VERSION)
        {
            self.seen.fetch_add(1, Ordering::SeqCst);
        }
        Box::pin(async {})
    }

    fn is_wasm_plugin(&self) -> bool {
        self.wasm
    }
}

/// A player in the play state, linked to its connection as a join does, and the counter of a
/// `PacketSentEvent` handler for it.
fn player(
    name: &'static str,
    wasm: bool,
) -> (Arc<Player>, tokio::net::TcpStream, Arc<AtomicUsize>) {
    let (player, peer) = support::player_with_peer(name, NegotiatedState::default());
    player.client.set_player(player.clone());
    if let ClientPlatform::Java(java) = player.client.as_ref() {
        java.connection_state.store(ConnectionState::Play);
    }
    let seen = Arc::new(AtomicUsize::new(0));
    support::server()
        .plugin_manager
        .register::<PacketSentEvent, _>(
            Arc::new(CountChat {
                name,
                wasm,
                seen: seen.clone(),
            }),
            EventPriority::Normal,
            true,
        );
    (player, peer, seen)
}

/// Waits until a chat packet arrives at `peer`.
fn chat_arrives(peer: tokio::net::TcpStream) {
    let chat = CSystemChatMessage::to_id(pumpkin_data::packet::CURRENT_MC_VERSION);
    let mut peer = TCPNetworkDecoder::new(tokio::io::BufReader::new(peer));
    support::RUNTIME.block_on(async {
        loop {
            let packet = tokio::time::timeout(Duration::from_secs(30), peer.get_raw_packet())
                .await
                .unwrap()
                .unwrap();
            if packet.id == chat {
                return;
            }
        }
    });
}

#[test]
fn a_send_in_a_guest_call_stays_direct_with_native_handlers_only() {
    let _serial = support::serial();
    let (player, peer, seen) = player("NativeSendPlayer", false);
    let recorder = Arc::new(Recorder::default());
    let call: Arc<dyn GuestCall> = recorder.clone();

    guest_call::enter(&call, || {
        player.send_system_message(&TextComponent::text("direct"));
    });

    assert!(recorder.0.lock().unwrap().is_empty(), "the send deferred");
    assert_eq!(seen.load(Ordering::SeqCst), 1);
    chat_arrives(peer);
}

#[test]
fn a_send_in_a_guest_call_defers_while_a_wasm_handler_listens() {
    let _serial = support::serial();
    let (player, peer, seen) = player("WasmSendPlayer", true);
    let recorder = Arc::new(Recorder::default());
    let call: Arc<dyn GuestCall> = recorder.clone();

    guest_call::enter(&call, || {
        player.send_system_message(&TextComponent::text("deferred"));
    });

    assert_eq!(
        seen.load(Ordering::SeqCst),
        0,
        "the event fired in the guest call"
    );
    let work: Vec<Work> = recorder.0.lock().unwrap().drain(..).collect();
    assert_eq!(work.len(), 1);
    for run in work {
        run();
    }
    assert_eq!(seen.load(Ordering::SeqCst), 1);
    chat_arrives(peer);
}
