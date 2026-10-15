//! The play state: enter play after the configuration phase, stay connected like the vanilla
//! `ClientPacketListener`, and run a script of player actions. Every packet is recorded with the
//! index of the step that was running.
//!
//! The client has no world, no physics and no inventory. It keeps the position the server sends,
//! confirms each teleport, answers keep-alive, ping and chunk batches, and discards chunk and
//! light data.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::OnceLock;
use std::time::Duration;

use pumpkin_data::{
    item_stack::ItemStack,
    packet::{CURRENT_MC_VERSION, clientbound::play},
};
use pumpkin_protocol::{
    ClientPacket, PacketDecodeError, RawPacket, VarInt,
    codec::item_stack_seralizer::ItemStackSerializer,
    java::server::play::{
        FLAG_ON_GROUND, SChatCommand, SChatMessage, SChunkBatch, SClientCommand, SConfirmTeleport,
        SCustomPayload, SKeepAlive, SPlayPingRequest, SPlayPong, SPlayerAction, SPlayerLoaded,
        SPlayerPosition, SPlayerPositionRotation, SSetCreativeSlot, SSetHeldItem, SUseItem,
        SUseItemOn, Status,
    },
    ser::NetworkReadExt,
};
use pumpkin_util::{
    math::{position::BlockPos, vector3::Vector3},
    text::TextComponent,
};
use serde_json::Value;
use tokio::{sync::mpsc, task::JoinHandle, time::Instant};
use tracing::info;

use super::{
    Decoder, Encoder, Options, Outcome, Recorder, Session, join_split, read_custom_payload,
};
use crate::{
    Error,
    record::{Direction, Entry, State, read_disconnect_reason},
    script::{Action, Script, Step},
};

/// The first hotbar slot in the inventory menu, the slot of `hold`.
const FIRST_HOTBAR_SLOT: i16 = 36;
/// `ClientboundPlayerPositionPacket` relative flags: `Relative.X`, `Y`, `Z`, `Y_ROT`, `X_ROT`.
const RELATIVE_X: i32 = 1;
const RELATIVE_Y: i32 = 1 << 1;
const RELATIVE_Z: i32 = 1 << 2;
const RELATIVE_YAW: i32 = 1 << 3;
const RELATIVE_PITCH: i32 = 1 << 4;
/// The chunk batch rate the client asks for, the upper bound of the vanilla
/// `ChunkBatchSizeCalculator`.
const CHUNKS_PER_TICK: f32 = 64.0;
const TICK: Duration = Duration::from_millis(50);
/// Packets recorded without their body, which only a world model would read.
const BODY_SKIPPED: [&str; 3] = ["level_chunk_with_light", "light_update", "chunks_biomes"];
/// The light data that `NeoForge` sends with each chunk, also recorded without its body.
const AUXILIARY_LIGHT: &str = "neoforge:auxiliary_light_data";

/// The clientbound play packets of one step, in the order they arrived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepReport {
    pub index: usize,
    /// The step as written in the script.
    pub text: String,
    /// The action name, such as `break`.
    pub action: &'static str,
    /// Packet names, such as `block_changed_ack`.
    pub packets: Vec<String>,
}

impl StepReport {
    fn new(index: usize, step: &Step) -> Self {
        Self {
            index,
            text: step.text.clone(),
            action: step.action.name(),
            packets: Vec::new(),
        }
    }

    /// The packet names in the order of their first arrival, with a count after a repeated one.
    #[must_use]
    pub fn summary(&self) -> String {
        let mut counts: Vec<(&str, usize)> = Vec::new();
        for packet in &self.packets {
            match counts.iter_mut().find(|(name, _)| name == packet) {
                Some((_, count)) => *count += 1,
                None => counts.push((packet, 1)),
            }
        }
        let names: Vec<String> = counts
            .into_iter()
            .map(|(name, count)| {
                if count == 1 {
                    name.to_owned()
                } else {
                    format!("{name} x{count}")
                }
            })
            .collect();
        names.join(", ")
    }
}

/// How a play run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayOutcome {
    /// The script ran to its `leave` and the client closed the connection.
    Left,
    /// The configuration phase ended without entering play.
    NotJoined(Outcome),
    /// The server ended the play state with a `disconnect`.
    Kicked(TextComponent),
    /// The server closed the connection without a `disconnect`.
    Closed,
}

/// The outcome and the steps that ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayReport {
    pub outcome: PlayOutcome,
    pub steps: Vec<StepReport>,
}

/// Connects, runs the configuration phase and then the script in the play state.
///
/// Every recorded packet goes to `sink`, with its step. `options.timeout` covers the
/// configuration phase and the wait for the first position of the player.
pub async fn play(
    options: &Options,
    script: &Script,
    sink: &mut dyn FnMut(&Entry),
) -> Result<PlayReport, Error> {
    let first = script.steps.first().ok_or("the script has no steps")?;
    info!("step 0: {}", first.text);
    let (session, outcome) = tokio::time::timeout(options.timeout, async {
        let mut session = Session::connect(options, Recorder::new(sink, Some(0))).await?;
        let outcome = session.drive().await?;
        Ok::<_, Error>((session, outcome))
    })
    .await
    .map_err(|_| format!("the configuration did not end within {:?}", options.timeout))??;
    if outcome != Outcome::Finished {
        return Ok(PlayReport {
            outcome: PlayOutcome::NotJoined(outcome),
            steps: vec![StepReport::new(0, first)],
        });
    }
    Play::new(session).run(script).await
}

/// What the packet reader gave.
enum Next {
    Packet(RawPacket, bool),
    Deadline,
    Closed,
}

/// The result of reading one packet.
enum Read {
    Handled,
    Deadline,
    End(PlayOutcome),
}

struct Play<'a> {
    options: &'a Options,
    recorder: Recorder<'a>,
    encoder: Encoder,
    packets: mpsc::UnboundedReceiver<Result<RawPacket, PacketDecodeError>>,
    reader: JoinHandle<()>,
    split_buffer: Vec<u8>,
    pending: VecDeque<RawPacket>,
    position: Vector3<f64>,
    yaw: f32,
    pitch: f32,
    /// The block of the player's feet after the first position, the origin of `~` coordinates.
    origin: Option<BlockPos>,
    /// The last prediction sequence number, as `BlockStatePredictionHandler` counts.
    sequence: i32,
    /// The payload of the last `ping_request` that ended a step.
    barrier: i64,
    /// The payload of the last `pong_response` from the server.
    pong: Option<i64>,
    /// The channels of the play payloads that the server sent, for `await_payload`.
    payload_channels: HashSet<String>,
    steps: Vec<StepReport>,
}

impl Drop for Play<'_> {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

/// Reads packets on its own task, because a read cannot stop in the middle of a packet when a
/// step's wait ends.
async fn read_packets(
    mut decoder: Decoder,
    packets: mpsc::UnboundedSender<Result<RawPacket, PacketDecodeError>>,
) {
    loop {
        let packet = decoder.get_raw_packet().await;
        let end = packet.is_err();
        if packets.send(packet).is_err() || end {
            return;
        }
    }
}

impl<'a> Play<'a> {
    fn new(session: Session<'a>) -> Self {
        let Session {
            options,
            recorder,
            decoder,
            encoder,
            ..
        } = session;
        let (sender, packets) = mpsc::unbounded_channel();
        Self {
            options,
            recorder,
            encoder,
            packets,
            reader: tokio::spawn(read_packets(decoder, sender)),
            split_buffer: Vec::new(),
            pending: VecDeque::new(),
            position: Vector3::new(0.0, 0.0, 0.0),
            yaw: 0.0,
            pitch: 0.0,
            origin: None,
            sequence: 0,
            barrier: 0,
            pong: None,
            payload_channels: HashSet::new(),
            steps: Vec::new(),
        }
    }

    async fn run(mut self, script: &Script) -> Result<PlayReport, Error> {
        for (index, step) in script.steps.iter().enumerate() {
            if index > 0 {
                info!("step {index}: {}", step.text);
            }
            self.recorder.step = Some(index);
            self.steps.push(StepReport::new(index, step));
            if let Some(outcome) = self.step(&step.action, script.wait).await? {
                return Ok(self.report(outcome));
            }
            let report = &self.steps[index];
            info!(
                "step {index} {} received: {}",
                report.action,
                report.summary()
            );
        }
        Ok(self.report(PlayOutcome::Left))
    }

    fn report(&mut self, outcome: PlayOutcome) -> PlayReport {
        self.reader.abort();
        PlayReport {
            outcome,
            steps: std::mem::take(&mut self.steps),
        }
    }

    /// Runs one action and ends the step with a barrier. See [`Play::settle`].
    async fn step(
        &mut self,
        action: &Action,
        wait: Duration,
    ) -> Result<Option<PlayOutcome>, Error> {
        let origin = self.origin.unwrap_or(BlockPos::new(0, 0, 0));
        match action {
            Action::Join => {
                let deadline = Instant::now() + self.options.timeout;
                while self.origin.is_none() {
                    match self.read_one(deadline).await? {
                        Read::Handled => {}
                        Read::End(outcome) => return Ok(Some(outcome)),
                        Read::Deadline => {
                            return Err(format!(
                                "no player position within {:?} of the play state",
                                self.options.timeout
                            )
                            .into());
                        }
                    }
                }
            }
            Action::Chat(message) => {
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_millis();
                self.send(&SChatMessage {
                    message,
                    timestamp: i64::try_from(timestamp)?,
                    salt: rand::random(),
                    signature: None,
                    message_count: VarInt(0),
                    acknowledged: &[0; 3],
                    checksum: 0,
                })
                .await?;
            }
            Action::Command(command) => self.send(&SChatCommand { command }).await?,
            Action::Move(offset) => {
                self.position += *offset;
                self.send(&SPlayerPosition {
                    position: self.position,
                    collision: FLAG_ON_GROUND,
                })
                .await?;
            }
            Action::Break { target, ticks } => {
                let position = target.resolve(origin);
                self.destroy(Status::StartedDigging, position).await?;
                if let Some(ticks) = ticks {
                    let deadline = Instant::now() + TICK * *ticks;
                    if let Some(outcome) = self.read_until(deadline).await? {
                        return Ok(Some(outcome));
                    }
                    self.destroy(Status::FinishedDigging, position).await?;
                }
            }
            Action::Place { target, face } => {
                self.sequence += 1;
                self.send(&SUseItemOn {
                    hand: VarInt(0),
                    position: target.resolve(origin),
                    face: VarInt(*face as i32),
                    cursor_pos: face.cursor(),
                    inside_block: false,
                    is_against_world_border: false,
                    sequence: VarInt(self.sequence),
                })
                .await?;
            }
            Action::Hold(item) => {
                let stack = ItemStackSerializer(Cow::Owned(ItemStack::new(1, item)));
                self.send(&SSetCreativeSlot::new(FIRST_HOTBAR_SLOT, stack))
                    .await?;
                self.send(&SSetHeldItem { slot: 0 }).await?;
            }
            Action::UseItem => {
                self.sequence += 1;
                self.send(&SUseItem {
                    hand: VarInt(0),
                    sequence: VarInt(self.sequence),
                    yaw: self.yaw,
                    pitch: self.pitch,
                })
                .await?;
            }
            Action::Respawn => {
                self.send(&SClientCommand::new(VarInt(
                    SClientCommand::PERFORM_RESPAWN,
                )))
                .await?;
            }
            Action::Wait(duration) => return self.read_until(Instant::now() + *duration).await,
            Action::AwaitPayload(channel) => return self.await_payload(channel).await,
            // Like the vanilla client, which closes the connection without a packet.
            Action::Leave => return Ok(None),
        }
        self.settle(wait).await
    }

    /// Ends a step. The client sends a `ping_request` and reads until the matching
    /// `pong_response`, so the server has read the action. It then reads for `wait` more, for the
    /// packets that trail the pong: `IronPumpkin` sends the chat echo and the block change ack after
    /// the pong. A server that sends no pong within `options.timeout` fails the run.
    async fn settle(&mut self, wait: Duration) -> Result<Option<PlayOutcome>, Error> {
        self.barrier += 1;
        let payload = self.barrier;
        self.send(&SPlayPingRequest { payload }).await?;
        let deadline = Instant::now() + self.options.timeout;
        while self.pong != Some(payload) {
            match self.read_one(deadline).await? {
                Read::Handled => {}
                Read::End(outcome) => return Ok(Some(outcome)),
                Read::Deadline => {
                    return Err(format!(
                        "no pong_response within {:?} of the ping_request",
                        self.options.timeout
                    )
                    .into());
                }
            }
        }
        self.read_until(Instant::now() + wait).await
    }

    /// Reads until the server has sent a play payload on `channel`. A server that sends none
    /// within `options.timeout` fails the run.
    async fn await_payload(&mut self, channel: &str) -> Result<Option<PlayOutcome>, Error> {
        let deadline = Instant::now() + self.options.timeout;
        while !self.payload_channels.contains(channel) {
            match self.read_one(deadline).await? {
                Read::Handled => {}
                Read::End(outcome) => return Ok(Some(outcome)),
                Read::Deadline => {
                    return Err(
                        format!("no {channel} payload within {:?}", self.options.timeout).into(),
                    );
                }
            }
        }
        Ok(None)
    }

    async fn destroy(&mut self, status: Status, position: BlockPos) -> Result<(), Error> {
        self.sequence += 1;
        self.send(&SPlayerAction {
            status: VarInt(status as i32),
            position,
            face: 1,
            sequence: VarInt(self.sequence),
        })
        .await
    }

    async fn send<P: ClientPacket>(&mut self, packet: &P) -> Result<(), Error> {
        let version = CURRENT_MC_VERSION.into();
        let mut body = Vec::new();
        packet.write_packet_data(&mut body, &version)?;
        let name = packet_name(Direction::Serverbound, P::to_id(CURRENT_MC_VERSION));
        self.recorder.record(
            Direction::Serverbound,
            State::Play,
            name,
            None,
            &body,
            false,
        );
        self.encoder
            .write_packet(packet.serialize_packet(&version)?)
            .await?;
        self.encoder.flush().await?;
        Ok(())
    }

    async fn next(&mut self, deadline: Instant) -> Result<Next, Error> {
        if let Some(packet) = self.pending.pop_front() {
            return Ok(Next::Packet(packet, true));
        }
        match tokio::time::timeout_at(deadline, self.packets.recv()).await {
            Err(_) => Ok(Next::Deadline),
            Ok(Some(Ok(packet))) => Ok(Next::Packet(packet, false)),
            Ok(None | Some(Err(PacketDecodeError::ConnectionClosed))) => Ok(Next::Closed),
            Ok(Some(Err(e))) => Err(e.into()),
        }
    }

    async fn read_one(&mut self, deadline: Instant) -> Result<Read, Error> {
        Ok(match self.next(deadline).await? {
            Next::Packet(packet, reassembled) => self
                .handle(&packet, reassembled)
                .await?
                .map_or(Read::Handled, Read::End),
            Next::Deadline => Read::Deadline,
            Next::Closed => Read::End(PlayOutcome::Closed),
        })
    }

    /// Reads until `deadline`, or until the connection ends.
    async fn read_until(&mut self, deadline: Instant) -> Result<Option<PlayOutcome>, Error> {
        loop {
            match self.read_one(deadline).await? {
                Read::Handled => {}
                Read::Deadline => return Ok(None),
                Read::End(outcome) => return Ok(Some(outcome)),
            }
        }
    }

    async fn handle(
        &mut self,
        packet: &RawPacket,
        reassembled: bool,
    ) -> Result<Option<PlayOutcome>, Error> {
        let name = packet_name(Direction::Clientbound, packet.id);
        let mut body = &packet.payload[..];
        if let Some(step) = self.steps.last_mut() {
            step.packets.push(name.clone());
        }
        if packet.id == play::CUSTOM_PAYLOAD {
            let (channel, data) = read_custom_payload(body)?;
            self.payload_channels.insert(channel.to_owned());
            self.recorder.record_body(
                Direction::Clientbound,
                State::Play,
                name,
                Some(channel),
                data,
                reassembled,
                channel != AUXILIARY_LIGHT,
            );
            if channel == pumpkin_protocol::java::neoforge::SplitPacketPayload::CHANNEL
                && let Some(joined) = join_split(&mut self.split_buffer, data)?
            {
                self.pending.push_back(joined);
            }
            if self.options.channels.as_ref().is_some_and(|channels| {
                channels.echoes(
                    pumpkin_protocol::java::neoforge::ConnectionProtocol::Play,
                    channel,
                )
            }) {
                self.send(&SCustomPayload { channel, data }).await?;
            }
            return Ok(None);
        }
        let keep_body = !BODY_SKIPPED.contains(&name.as_str());
        self.recorder.record_body(
            Direction::Clientbound,
            State::Play,
            name,
            None,
            body,
            reassembled,
            keep_body,
        );

        if packet.id == play::KEEP_ALIVE {
            let keep_alive_id = body.get_i64_be()?;
            self.send(&SKeepAlive { keep_alive_id }).await?;
        } else if packet.id == play::PING {
            let id = body.get_i32_be()?;
            self.send(&SPlayPong { id }).await?;
        } else if packet.id == play::PONG_RESPONSE {
            self.pong = Some(body.get_i64_be()?);
        } else if packet.id == play::PLAYER_POSITION {
            self.on_position(body).await?;
        } else if packet.id == play::CHUNK_BATCH_FINISHED {
            self.send(&SChunkBatch {
                chunks_per_tick: CHUNKS_PER_TICK,
            })
            .await?;
        } else if packet.id == play::DISCONNECT {
            return Ok(Some(PlayOutcome::Kicked(read_disconnect_reason(body)?)));
        } else if packet.id == play::START_CONFIGURATION {
            return Err(
                "the server started a new configuration phase, which the client does not support"
                    .into(),
            );
        }
        Ok(None)
    }

    /// `ClientPacketListener.handleMovePlayer`: take the position, confirm the teleport and send
    /// the position back. The first position also ends the level loading.
    async fn on_position(&mut self, mut body: &[u8]) -> Result<(), Error> {
        let teleport_id = body.get_var_int()?;
        let x = body.get_f64_be()?;
        let y = body.get_f64_be()?;
        let z = body.get_f64_be()?;
        for _delta in 0..3 {
            body.get_f64_be()?;
        }
        let yaw = body.get_f32_be()?;
        let pitch = body.get_f32_be()?;
        let relative = body.get_i32_be()?;
        let pick = |flag: i32, current: f64, value: f64| {
            if relative & flag == 0 {
                value
            } else {
                current + value
            }
        };
        self.position = Vector3::new(
            pick(RELATIVE_X, self.position.x, x),
            pick(RELATIVE_Y, self.position.y, y),
            pick(RELATIVE_Z, self.position.z, z),
        );
        if relative & RELATIVE_YAW == 0 {
            self.yaw = yaw;
        } else {
            self.yaw += yaw;
        }
        if relative & RELATIVE_PITCH == 0 {
            self.pitch = pitch;
        } else {
            self.pitch += pitch;
        }
        self.send(&SConfirmTeleport {
            teleport_id,
            position: self.position,
            yaw: self.yaw,
            pitch: self.pitch,
        })
        .await?;
        self.send(&SPlayerPositionRotation {
            position: self.position,
            yaw: self.yaw,
            pitch: self.pitch,
            collision: 0,
        })
        .await?;
        if self.origin.is_none() {
            self.send(&SPlayerLoaded).await?;
            let origin = BlockPos::floored_v(self.position);
            info!(
                "entered play at {} {} {}",
                origin.0.x, origin.0.y, origin.0.z
            );
            self.origin = Some(origin);
        }
        Ok(())
    }
}

/// The names of the play packets, from `assets/packets.json`.
fn packet_name(direction: Direction, id: i32) -> String {
    static NAMES: OnceLock<[HashMap<i32, String>; 2]> = OnceLock::new();
    let names = NAMES.get_or_init(|| {
        let packets: Value = serde_json::from_str(include_str!("../../../../assets/packets.json"))
            .unwrap_or_default();
        ["clientbound", "serverbound"].map(|direction| {
            packets[direction]["play"]
                .as_object()
                .into_iter()
                .flatten()
                .filter_map(|(name, id)| Some((i32::try_from(id.as_i64()?).ok()?, name.clone())))
                .collect()
        })
    });
    let table = &names[usize::from(direction == Direction::Serverbound)];
    table
        .get(&id)
        .cloned()
        .unwrap_or_else(|| format!("unknown_0x{id:02x}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_data::packet::serverbound;

    #[test]
    fn names_play_packets_from_the_packet_table() {
        let clientbound = |id| packet_name(Direction::Clientbound, id);
        let serverbound = |id| packet_name(Direction::Serverbound, id);
        assert_eq!(clientbound(play::LOGIN.0), "login");
        assert_eq!(clientbound(play::PLAYER_POSITION.0), "player_position");
        assert_eq!(clientbound(play::BLOCK_CHANGED_ACK.0), "block_changed_ack");
        assert_eq!(
            serverbound(serverbound::play::ACCEPT_TELEPORTATION.0),
            "accept_teleportation"
        );
        assert_eq!(
            serverbound(serverbound::play::PLAYER_ACTION.0),
            "player_action"
        );
        assert_eq!(clientbound(1000), "unknown_0x3e8");
    }

    #[test]
    fn summary_counts_repeated_packets() {
        let report = StepReport {
            index: 1,
            text: "chat hi".to_owned(),
            action: "chat",
            packets: ["system_chat", "keep_alive", "system_chat"]
                .map(str::to_owned)
                .to_vec(),
        };
        assert_eq!(report.summary(), "system_chat x2, keep_alive");
    }
}
