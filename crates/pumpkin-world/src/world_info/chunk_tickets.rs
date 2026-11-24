//! `<dimension>/data/minecraft/chunk_tickets.dat`, the vanilla `TicketStorage` saved data.
//!
//! The file holds `data.tickets`, a list of `{chunk_pos: [I; x, z], type, level, ticks_left?}`
//! compounds. Only the `minecraft:forced` tickets are owned here. Any other ticket and any other
//! key of `data` (for example the `neo_ticket_data` of `NeoForge`) stay as they are on disk.

use std::{
    collections::HashSet,
    fs::File,
    hash::BuildHasher,
    path::{Path, PathBuf},
};

use pumpkin_nbt::{compound::NbtCompound, nbt_compress::read_gzip_compound_tag, tag::NbtTag};
use pumpkin_util::math::vector2::Vector2;
use rustc_hash::FxHashSet;
use tracing::warn;

use crate::{
    CURRENT_MC_DATA_VERSION,
    chunk::io::write_file_atomically,
    world_info::{WorldInfoError, data_files::minecraft_data_dir},
};

pub const CHUNK_TICKETS_FILE_NAME: &str = "chunk_tickets.dat";
const FORCED_TICKET_TYPE: &str = "minecraft:forced";
/// `ChunkMap.FORCED_TICKET_LEVEL`: the level of `FullChunkStatus.ENTITY_TICKING`.
const FORCED_TICKET_LEVEL: i32 = 31;

#[must_use]
pub fn chunk_tickets_path(dimension_folder: &Path) -> PathBuf {
    minecraft_data_dir(dimension_folder).join(CHUNK_TICKETS_FILE_NAME)
}

fn read_root(path: &Path) -> Result<Option<NbtCompound>, WorldInfoError> {
    if !path.exists() {
        return Ok(None);
    }
    read_gzip_compound_tag(File::open(path)?)
        .map(Some)
        .map_err(|e| WorldInfoError::DeserializationError(e.to_string()))
}

fn is_forced_ticket(ticket: &NbtTag) -> bool {
    ticket
        .extract_compound()
        .and_then(|ticket| ticket.get_string("type"))
        .is_some_and(|ty| ty == FORCED_TICKET_TYPE || ty == "forced")
}

fn forced_chunk_pos(ticket: &NbtTag) -> Option<Vector2<i32>> {
    if !is_forced_ticket(ticket) {
        return None;
    }
    match ticket.extract_compound()?.get_int_array("chunk_pos")? {
        [x, z] => Some(Vector2::new(*x, *z)),
        _ => None,
    }
}

fn forced_chunks_of(root: &NbtCompound) -> FxHashSet<Vector2<i32>> {
    root.get_compound("data")
        .and_then(|data| data.get_list("tickets"))
        .map(|tickets| tickets.iter().filter_map(forced_chunk_pos).collect())
        .unwrap_or_default()
}

/// Reads the chunks that hold a forced ticket. A missing or unreadable file gives an empty set.
#[must_use]
pub fn read_forced_chunks(dimension_folder: &Path) -> FxHashSet<Vector2<i32>> {
    let path = chunk_tickets_path(dimension_folder);
    match read_root(&path) {
        Ok(root) => root.as_ref().map(forced_chunks_of).unwrap_or_default(),
        Err(e) => {
            warn!("Failed to read {}, forcing no chunks: {e}", path.display());
            FxHashSet::default()
        }
    }
}

fn forced_ticket(pos: Vector2<i32>) -> NbtTag {
    let mut ticket = NbtCompound::new();
    ticket.put("chunk_pos", NbtTag::IntArray(vec![pos.x, pos.y]));
    ticket.put_string("type", FORCED_TICKET_TYPE.to_string());
    ticket.put_int("level", FORCED_TICKET_LEVEL);
    NbtTag::Compound(ticket)
}

fn with_forced_chunks<S>(
    existing: Option<NbtCompound>,
    forced: &HashSet<Vector2<i32>, S>,
) -> NbtCompound {
    let mut root = existing.unwrap_or_default();
    let mut data = root.get_compound("data").cloned().unwrap_or_default();

    let mut tickets: Vec<NbtTag> = data
        .get_list("tickets")
        .unwrap_or_default()
        .iter()
        .filter(|ticket| !is_forced_ticket(ticket))
        .cloned()
        .collect();
    let mut forced: Vec<_> = forced.iter().copied().collect();
    forced.sort_unstable_by_key(|pos| (pos.x, pos.y));
    tickets.extend(forced.into_iter().map(forced_ticket));

    data.put_list("tickets", tickets);
    root.put_compound("data", data);
    root.put_int("DataVersion", CURRENT_MC_DATA_VERSION);
    root
}

/// Writes `forced` as the forced tickets of the dimension and keeps the rest of the file.
///
/// Writes nothing when there is no file and no forced chunk.
pub fn write_forced_chunks<S: BuildHasher>(
    dimension_folder: &Path,
    forced: &HashSet<Vector2<i32>, S>,
) -> Result<(), WorldInfoError> {
    let path = chunk_tickets_path(dimension_folder);
    let existing = match read_root(&path) {
        Ok(existing) => existing,
        Err(e) => {
            warn!(
                "Replacing the unreadable {} with the forced chunks: {e}",
                path.display()
            );
            None
        }
    };
    if existing.is_none() && forced.is_empty() {
        return Ok(());
    }
    if existing.as_ref().is_some_and(|root| {
        let stored = forced_chunks_of(root);
        stored.len() == forced.len() && forced.iter().all(|pos| stored.contains(pos))
    }) {
        return Ok(());
    }

    let root = with_forced_chunks(existing, forced);
    let bytes = pumpkin_nbt::nbt_compress::write_gzip_compound_tag_to_bytes(root)
        .map_err(|e| WorldInfoError::SerializationError(e.to_string()))?;
    write_file_atomically(&path, &bytes, false)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tempfile::TempDir;

    fn set(chunks: &[(i32, i32)]) -> FxHashSet<Vector2<i32>> {
        chunks.iter().map(|&(x, z)| Vector2::new(x, z)).collect()
    }

    fn read_raw(dimension_folder: &Path) -> NbtCompound {
        read_root(&chunk_tickets_path(dimension_folder))
            .unwrap()
            .unwrap()
    }

    /// What `TicketStorage.CODEC` writes for two forced chunks and a portal ticket (a persisted
    /// ticket type with a timeout), wrapped by `SavedDataStorage.encodeUnchecked`.
    fn vanilla_file() -> Vec<u8> {
        let mut forced_a = NbtCompound::new();
        forced_a.put("chunk_pos", NbtTag::IntArray(vec![-3, 7]));
        forced_a.put_string("type", "minecraft:forced".to_string());
        forced_a.put_int("level", 31);
        let mut forced_b = NbtCompound::new();
        forced_b.put("chunk_pos", NbtTag::IntArray(vec![0, 0]));
        forced_b.put_string("type", "minecraft:forced".to_string());
        forced_b.put_int("level", 31);
        let mut portal = NbtCompound::new();
        portal.put("chunk_pos", NbtTag::IntArray(vec![5, 5]));
        portal.put_string("type", "minecraft:portal".to_string());
        portal.put_int("level", 31);
        portal.put_long("ticks_left", 200);

        let mut data = NbtCompound::new();
        data.put_list(
            "tickets",
            vec![
                NbtTag::Compound(forced_a),
                NbtTag::Compound(portal),
                NbtTag::Compound(forced_b),
            ],
        );
        let mut root = NbtCompound::new();
        root.put_compound("data", data);
        root.put_int("DataVersion", 5023);
        pumpkin_nbt::nbt_compress::write_gzip_compound_tag_to_bytes(root).unwrap()
    }

    #[test]
    fn writes_the_vanilla_shape() {
        let dir = TempDir::new().unwrap();
        write_forced_chunks(dir.path(), &set(&[(4, -2), (-1, 9)])).unwrap();

        assert_eq!(
            dir.path()
                .join("data")
                .join("minecraft")
                .join("chunk_tickets.dat"),
            chunk_tickets_path(dir.path())
        );
        let root = read_raw(dir.path());
        assert_eq!(root.get_int("DataVersion"), Some(CURRENT_MC_DATA_VERSION));
        let tickets = root
            .get_compound("data")
            .unwrap()
            .get_list("tickets")
            .unwrap();
        assert_eq!(tickets.len(), 2);

        let first = tickets[0].extract_compound().unwrap();
        assert_eq!(first.get_int_array("chunk_pos"), Some([-1, 9].as_slice()));
        assert_eq!(first.get_string("type"), Some("minecraft:forced"));
        assert_eq!(first.get_int("level"), Some(31));
        assert!(!first.has("ticks_left"));
        let second = tickets[1].extract_compound().unwrap();
        assert_eq!(second.get_int_array("chunk_pos"), Some([4, -2].as_slice()));
    }

    #[test]
    fn reads_back_what_it_writes() {
        let dir = TempDir::new().unwrap();
        let forced = set(&[(0, 0), (-30, 12), (7, -7)]);
        write_forced_chunks(dir.path(), &forced).unwrap();
        assert_eq!(read_forced_chunks(dir.path()), forced);
    }

    #[test]
    fn reads_a_file_in_the_shape_vanilla_writes() {
        let dir = TempDir::new().unwrap();
        let path = chunk_tickets_path(dir.path());
        write_file_atomically(&path, &vanilla_file(), false).unwrap();

        assert_eq!(read_forced_chunks(dir.path()), set(&[(-3, 7), (0, 0)]));
    }

    #[test]
    fn keeps_the_tickets_and_keys_it_does_not_own() {
        let dir = TempDir::new().unwrap();
        let path = chunk_tickets_path(dir.path());
        let mut root = read_gzip_compound_tag(Cursor::new(vanilla_file())).unwrap();
        let mut data = root.get_compound("data").cloned().unwrap();
        data.put_list("neo_ticket_data", vec![]);
        root.put_compound("data", data);
        let bytes = pumpkin_nbt::nbt_compress::write_gzip_compound_tag_to_bytes(root).unwrap();
        write_file_atomically(&path, &bytes, false).unwrap();

        write_forced_chunks(dir.path(), &set(&[(1, 1)])).unwrap();

        assert_eq!(read_forced_chunks(dir.path()), set(&[(1, 1)]));
        let root = read_raw(dir.path());
        let data = root.get_compound("data").unwrap();
        assert!(data.has("neo_ticket_data"));
        let tickets = data.get_list("tickets").unwrap();
        assert_eq!(tickets.len(), 2);
        let portal = tickets[0].extract_compound().unwrap();
        assert_eq!(portal.get_string("type"), Some("minecraft:portal"));
        assert_eq!(portal.get_long("ticks_left"), Some(200));
    }

    #[test]
    fn writes_no_file_when_nothing_is_forced() {
        let dir = TempDir::new().unwrap();
        write_forced_chunks(dir.path(), &FxHashSet::default()).unwrap();
        assert!(!chunk_tickets_path(dir.path()).exists());
        assert!(read_forced_chunks(dir.path()).is_empty());
    }

    #[test]
    fn unforcing_the_last_chunk_empties_the_file() {
        let dir = TempDir::new().unwrap();
        write_forced_chunks(dir.path(), &set(&[(2, 2)])).unwrap();
        write_forced_chunks(dir.path(), &FxHashSet::default()).unwrap();
        assert!(read_forced_chunks(dir.path()).is_empty());
    }

    #[test]
    fn an_unreadable_file_gives_no_forced_chunks_and_is_replaced_on_write() {
        let dir = TempDir::new().unwrap();
        let path = chunk_tickets_path(dir.path());
        write_file_atomically(&path, b"not nbt", false).unwrap();
        assert!(read_forced_chunks(dir.path()).is_empty());

        write_forced_chunks(dir.path(), &set(&[(3, 3)])).unwrap();
        assert_eq!(read_forced_chunks(dir.path()), set(&[(3, 3)]));
    }
}
