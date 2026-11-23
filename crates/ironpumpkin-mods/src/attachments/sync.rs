//! `neoforge:sync_attachments`: the payload and its recipients (`AttachmentSync`).

use std::sync::Arc;

use pumpkin_core::{
    entity::{Entity, player::Player},
    net::ClientPlatform,
    plugin::{
        api::events::network::ConnectionProtocol,
        attachments::{AttachmentCell, Attachments},
        network_events::{negotiated_channels, send_to_player},
    },
    world::World,
};
use pumpkin_data::entity::EntityType;
use pumpkin_protocol::{codec::var_int::VarInt, ser::NetworkWriteExt};
use pumpkin_util::{
    identifier::Identifier,
    math::{position::BlockPos, vector2::Vector2},
};

use super::{AttachmentHolder, cell, types::ErasedType};

/// The channel of the attachment sync. `NeoForge`: `SyncAttachmentsPayload.TYPE`.
pub const SYNC_ATTACHMENTS_CHANNEL: &str = "neoforge:sync_attachments";

/// `SyncAttachmentsPayload.Target`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Target {
    BlockEntity(BlockPos),
    Chunk(Vector2<i32>),
    Entity(i32),
    Level,
}

impl Target {
    pub(super) fn of(holder: &AttachmentHolder<'_>) -> Self {
        match holder {
            AttachmentHolder::Entity(entity) => Self::Entity(entity.entity_id),
            AttachmentHolder::BlockEntity(block_entity) => {
                Self::BlockEntity(block_entity.get_position())
            }
            AttachmentHolder::Chunk(chunk) => Self::Chunk(Vector2::new(chunk.x, chunk.z)),
            AttachmentHolder::Level(_) => Self::Level,
        }
    }
}

/// The body of a `neoforge:sync_attachments` payload, as `SyncAttachmentsPayload.STREAM_CODEC`
/// writes it: the target, the list of synced type ids (`ByteBufCodecs.registry` with
/// `ByteBufCodecs.list`) and the data (`NeoForgeStreamCodecs.UNBOUNDED_BYTE_ARRAY`).
pub(super) fn encode_payload(target: Target, synced_ids: &[u32], data: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(16 + data.len());
    // Writes into a `Vec` cannot fail.
    let _ = write_payload(&mut buf, target, synced_ids, data);
    buf
}

fn write_payload(
    buf: &mut Vec<u8>,
    target: Target,
    synced_ids: &[u32],
    data: &[u8],
) -> Result<(), pumpkin_protocol::ser::WritingError> {
    match target {
        Target::BlockEntity(pos) => {
            buf.write_u8(0)?;
            buf.write_i64_be(pos.as_long())?;
        }
        Target::Chunk(pos) => {
            buf.write_u8(1)?;
            // `ChunkPos.pack`.
            let packed =
                (i64::from(pos.x) & 0xFFFF_FFFF) | ((i64::from(pos.y) & 0xFFFF_FFFF) << 32);
            buf.write_i64_be(packed)?;
        }
        Target::Entity(id) => {
            buf.write_u8(2)?;
            buf.write_var_int(&VarInt(id))?;
        }
        Target::Level => buf.write_u8(3)?,
    }
    buf.write_var_int(&VarInt(synced_ids.len() as i32))?;
    for id in synced_ids {
        buf.write_var_int(&VarInt(*id as i32))?;
    }
    buf.write_var_int(&VarInt(data.len() as i32))?;
    buf.write_slice(data)
}

/// Whether the client of `player` negotiated `neoforge:sync_attachments`, so a vanilla client
/// never gets it.
pub(super) fn has_channel(player: &Player) -> bool {
    negotiated_channels(player.negotiated_state()).has_channel(
        ConnectionProtocol::Play,
        &Identifier::parse_static(SYNC_ATTACHMENTS_CHANNEL),
    )
}

fn send(player: &Player, payload: &[u8]) {
    // A client without the channel is filtered out before.
    let _ = send_to_player(player, SYNC_ATTACHMENTS_CHANNEL, payload);
}

/// `AttachmentSync.syncUpdate`: the current value of `entry` in `slot`, or its absence, to
/// every player of `players` whose client has the channel and that the sync handler accepts.
/// The sync handler chooses the players before any lock is taken, so it can read the holder;
/// only the write of the value holds its lock. A failed write sends nothing.
pub(super) fn sync_update(
    holder: &AttachmentHolder<'_>,
    slot: &Attachments,
    entry: &dyn ErasedType,
    players: &[Arc<Player>],
) {
    let Some(synced_id) = entry.synced_id() else {
        return;
    };
    let recipients: Vec<&Arc<Player>> = players
        .iter()
        .filter(|player| has_channel(player) && entry.send_to_player(holder, player))
        .collect();
    if recipients.is_empty() {
        return;
    }
    let mut data = Vec::new();
    if let Some(cell) = slot.cell(entry.index()) {
        data.push(1);
        let written = entry.write_sync(&mut data, cell::lock(&cell, entry.name()).as_ref(), false);
        if let Err(message) = written {
            tracing::error!(
                "Failed to sync data attachment {}. Skipping. {message}",
                entry.name()
            );
            return;
        }
    } else {
        data.push(0);
    }
    let payload = encode_payload(Target::of(holder), &[synced_id], &data);
    for player in recipients {
        send(player, &payload);
    }
}

/// `AttachmentSync.syncInitialAttachments`: every synced attachment of `map` for `to`, `None`
/// when `to` lacks the channel or `map` has no synced type. As on `NeoForge`, the sync handler
/// is not asked whether `to` gets the value, and a type whose handler writes nothing is left
/// out of the list.
pub(super) fn initial_payload(
    holder: &AttachmentHolder<'_>,
    slot: &Attachments,
    types: &super::types::Types,
    to: &Player,
) -> Option<Vec<u8>> {
    if !has_channel(to) {
        return None;
    }
    let (synced_ids, data) = initial_data(&slot.cells(), types)?;
    Some(encode_payload(Target::of(holder), &synced_ids, &data))
}

/// The synced type ids and the data of an initial payload of `cells`, `None` without a synced
/// type. A type whose write fails is left out.
pub(super) fn initial_data(
    cells: &[(u32, AttachmentCell)],
    types: &super::types::Types,
) -> Option<(Vec<u32>, Vec<u8>)> {
    let entries: Vec<_> = cells
        .iter()
        .filter_map(|(index, cell)| {
            let entry = types.get(*index)?;
            Some((entry, entry.synced_id()?, cell))
        })
        .collect();
    if entries.is_empty() {
        return None;
    }
    let mut synced_ids = Vec::new();
    let mut data = Vec::new();
    for (entry, synced_id, cell) in entries {
        let before = data.len();
        data.push(1);
        let between = data.len();
        let written = entry.write_sync(&mut data, cell::lock(cell, entry.name()).as_ref(), true);
        if let Err(message) = &written {
            tracing::error!(
                "Failed to sync data attachment {}. Skipping. {message}",
                entry.name()
            );
        }
        if written.is_ok() && data.len() > between {
            synced_ids.push(synced_id);
        } else {
            data.truncate(before);
        }
    }
    Some((synced_ids, data))
}

/// Sends an initial payload to `to`.
pub(super) fn send_initial(to: &Player, payload: Option<Vec<u8>>) {
    if let Some(payload) = payload {
        send(to, &payload);
    }
}

/// `ChunkMap#getPlayersWatching` and the entity itself when it is a player.
pub(super) fn entity_recipients(world: &World, entity: &Entity) -> Vec<Arc<Player>> {
    let mut players = world
        .entity_tracker
        .get_tracked_entity(entity.entity_id)
        .map_or_else(Vec::new, |tracked| tracked.java_viewers());
    if entity.entity_type == &EntityType::PLAYER
        && let Some(player) = world.get_player_by_id(entity.entity_id)
        && players.iter().all(|p| p.entity_id() != entity.entity_id)
    {
        players.push(player);
    }
    players
}

/// `ChunkMap#getPlayers(pos, false)`: the Java players that got the chunk.
pub(super) fn chunk_recipients(world: &World, pos: Vector2<i32>) -> Vec<Arc<Player>> {
    world
        .players
        .load()
        .iter()
        .filter(|player| {
            matches!(player.client.as_ref(), ClientPlatform::Java(_))
                && player
                    .watched_section
                    .load()
                    .is_within_distance(pos.x, pos.y)
                && player
                    .chunk_sender
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .is_chunk_ready(&pos)
        })
        .cloned()
        .collect()
}

/// `ServerLevel#players`.
pub(super) fn level_recipients(world: &World) -> Vec<Arc<Player>> {
    world.players.load().iter().cloned().collect()
}
