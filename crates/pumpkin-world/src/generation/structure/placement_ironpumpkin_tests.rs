use crate::generation::structure::placement::GlobalStructureCache;

#[test]
fn structure_starts_at_reads_computed_and_saved_starts() {
    use std::sync::{Arc, Mutex};

    use pumpkin_data::structures::StructureKeys;
    use pumpkin_util::math::{block_box::BlockBox, position::BlockPos, vector2::Vector2};

    use crate::generation::structure::{
        piece::StructurePieceType,
        start::StructureStart,
        structures::{StructurePiece, StructurePiecesCollector, StructurePosition},
    };

    let position = |piece_box: BlockBox| {
        let mut collector = StructurePiecesCollector::new();
        collector.add_piece(Box::new(StructurePiece::new(
            StructurePieceType::Jigsaw,
            piece_box,
            0,
        )));
        Some(StructurePosition {
            start_pos: BlockPos::new(piece_box.min.x, piece_box.min.y, piece_box.min.z),
            collector: Arc::new(Mutex::new(collector)),
        })
    };

    let cache = GlobalStructureCache::new();
    let _ = cache.get_or_compute_structure_start(StructureKeys::VillagePlains, 0, 0, || {
        position(BlockBox::new(0, 60, 0, 20, 70, 20))
    });
    let _ = cache.get_or_compute_structure_start(StructureKeys::Igloo, 0, 1, || None);
    cache.add_saved_start(StructureStart {
        structure: StructureKeys::Mineshaft,
        chunk_pos: Vector2::new(1, 1),
        bounding_box: BlockBox::new(16, 10, 16, 40, 30, 40),
    });

    let references = [
        (StructureKeys::VillagePlains, Vector2::new(0, 0)),
        (StructureKeys::Mineshaft, Vector2::new(1, 1)),
        (StructureKeys::Igloo, Vector2::new(0, 1)),
        (StructureKeys::Monument, Vector2::new(2, 2)),
    ];

    // A village adapts the terrain, so its box grows by 12 blocks like vanilla's.
    let village = cache.structure_starts_at(&BlockPos::new(-5, 55, -5), &references);
    assert_eq!(village.len(), 1);
    assert_eq!(village[0].structure, StructureKeys::VillagePlains);
    assert_eq!(village[0].chunk_pos, Vector2::new(0, 0));
    assert_eq!(
        village[0].bounding_box,
        BlockBox::new(-12, 48, -12, 32, 82, 32)
    );

    let mineshaft = cache.structure_starts_at(&BlockPos::new(20, 20, 20), &references);
    assert_eq!(
        mineshaft
            .iter()
            .map(|start| start.structure)
            .collect::<Vec<_>>(),
        [StructureKeys::Mineshaft]
    );
    assert!(
        cache
            .structure_starts_at(&BlockPos::new(100, 60, 100), &references)
            .is_empty()
    );

    // A start is only found through a reference to it.
    assert!(
        cache
            .structure_starts_at(&BlockPos::new(20, 20, 20), &references[..1])
            .is_empty()
    );

    // A start read from chunk data wins over the computed one.
    cache.add_saved_start(StructureStart {
        structure: StructureKeys::VillagePlains,
        chunk_pos: Vector2::new(0, 0),
        bounding_box: BlockBox::new(0, 0, 0, 1, 1, 1),
    });
    assert_eq!(
        cache
            .get_start_for_structure(StructureKeys::VillagePlains, Vector2::new(0, 0))
            .map(|start| start.bounding_box),
        Some(BlockBox::new(0, 0, 0, 1, 1, 1))
    );
}
