use pumpkin_data::structures::{Structure, StructureKeys, TerrainAdaptation};
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use pumpkin_util::{
    BlockDirection,
    math::{block_box::BlockBox, position::BlockPos, vector2::Vector2},
};
use tracing::error;

use super::structures::StructurePosition;

/// A placed structure as a lookup sees it: vanilla `StructureStart` without the pieces' content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StructureStart {
    pub structure: StructureKeys,
    /// The chunk that owns the start.
    pub chunk_pos: Vector2<i32>,
    /// Vanilla `StructureStart.getBoundingBox`: the box around every piece, after
    /// `Structure.adjustBoundingBox`.
    pub bounding_box: BlockBox,
}

impl StructureStart {
    /// Returns `None` for a start without pieces, which vanilla treats as invalid.
    #[must_use]
    pub fn from_position(
        structure: StructureKeys,
        chunk_pos: Vector2<i32>,
        position: &StructurePosition,
    ) -> Option<Self> {
        let bounding_box = {
            let mut collector = position
                .collector
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if collector.is_empty() {
                return None;
            }
            collector.get_bounding_box()
        };
        Some(Self {
            structure,
            chunk_pos,
            bounding_box: adjust_bounding_box(structure, bounding_box),
        })
    }

    /// Vanilla `StructureStart.loadStaticStart`. Only the piece boxes are read, so a piece
    /// type that Pumpkin does not know still counts towards the bounding box.
    #[must_use]
    pub fn load_static_start(tag: &NbtCompound) -> Option<Self> {
        let id = tag.get_string("id").unwrap_or_default();
        if id == "INVALID" {
            return None;
        }
        let Some(structure) = StructureKeys::from_name(id) else {
            error!("Unknown structure id: {id}");
            return None;
        };
        let chunk_pos = Vector2::new(
            tag.get_int("ChunkX").unwrap_or(0),
            tag.get_int("ChunkZ").unwrap_or(0),
        );
        let pieces = tag
            .get_list("Children")
            .unwrap_or_default()
            .iter()
            .filter_map(|child| {
                let &[min_x, min_y, min_z, max_x, max_y, max_z] =
                    child.extract_compound()?.get_int_array("BB")?
                else {
                    return None;
                };
                Some(BlockBox::new(min_x, min_y, min_z, max_x, max_y, max_z))
            });
        let bounding_box = BlockBox::encompass_all(pieces)?;
        Some(Self {
            structure,
            chunk_pos,
            bounding_box: adjust_bounding_box(structure, bounding_box),
        })
    }

    #[must_use]
    pub const fn is_inside(&self, pos: &BlockPos) -> bool {
        self.bounding_box.contains_pos(&pos.0)
    }
}

/// Vanilla `Structure.adjustBoundingBox`: structures that adapt the terrain around them claim
/// 12 more blocks on every side.
#[must_use]
pub fn adjust_bounding_box(structure: StructureKeys, bounding_box: BlockBox) -> BlockBox {
    if Structure::get(&structure).terrain_adaptation == TerrainAdaptation::None {
        bounding_box
    } else {
        bounding_box.expand(12, 12, 12)
    }
}

/// Vanilla `StructureStart.createTag` for a start that Pumpkin generated.
///
/// Each piece carries the fields of `StructurePiece.createTag` (`id`, `BB`, `O`, `GD`) but not
/// the type-specific fields of `addAdditionalSaveData`, which Pumpkin does not track. Vanilla
/// reads such a chunk without a crash, but it drops every jigsaw piece ("Invalid pool element
/// found"), so villages, outposts, bastions, ancient cities, trail ruins and trial chambers load
/// as invalid starts, and template pieces rebuild a wrong box from an empty template.
#[must_use]
pub fn create_tag(
    structure: StructureKeys,
    chunk_pos: Vector2<i32>,
    position: &StructurePosition,
) -> NbtCompound {
    let collector = position
        .collector
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let children = collector
        .pieces
        .iter()
        .map(|piece| {
            let bounding_box = piece.bounding_box();
            let base = piece.get_structure_piece();
            let mut child = NbtCompound::new();
            child.put_string("id", base.r#type.id().to_string());
            child.put(
                "BB",
                NbtTag::IntArray(vec![
                    bounding_box.min.x,
                    bounding_box.min.y,
                    bounding_box.min.z,
                    bounding_box.max.x,
                    bounding_box.max.y,
                    bounding_box.max.z,
                ]),
            );
            child.put_int("O", data_2d_value(base.facing));
            child.put_int("GD", base.chain_length as i32);
            NbtTag::Compound(child)
        })
        .collect();

    let mut tag = NbtCompound::new();
    tag.put_string("id", format!("minecraft:{}", structure.to_name()));
    tag.put_int("ChunkX", chunk_pos.x);
    tag.put_int("ChunkZ", chunk_pos.y);
    tag.put_int("references", 0);
    tag.put_list("Children", children);
    tag
}

/// Vanilla `Direction.get2DDataValue`, with -1 for no orientation as `StructurePiece` writes it.
const fn data_2d_value(facing: Option<BlockDirection>) -> i32 {
    match facing {
        Some(BlockDirection::South) => 0,
        Some(BlockDirection::West) => 1,
        Some(BlockDirection::North) => 2,
        Some(BlockDirection::East) => 3,
        _ => -1,
    }
}

/// Vanilla `ChunkPos.pack`.
#[must_use]
pub const fn pack_chunk_pos(x: i32, z: i32) -> i64 {
    (x as u32 as i64) | ((z as u32 as i64) << 32)
}

/// Vanilla `ChunkPos.unpack`.
#[must_use]
pub const fn unpack_chunk_pos(packed: i64) -> Vector2<i32> {
    Vector2::new(packed as i32, (packed >> 32) as i32)
}
