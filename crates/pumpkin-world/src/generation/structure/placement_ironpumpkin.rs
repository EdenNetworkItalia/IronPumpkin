use super::{BlockPos, DashMap, GlobalStructureCache, StructureKeys, StructureStart, Vector2};

impl GlobalStructureCache {
    /// Records a structure start read from chunk data, so lookups find it after a restart.
    pub fn add_saved_start(&self, start: StructureStart) {
        self.saved_starts.get_or_init(DashMap::new).insert(
            (start.structure, start.chunk_pos.x, start.chunk_pos.y),
            start,
        );
    }

    /// Vanilla `StructureManager.getStartForStructure`: the start of `structure` owned by
    /// `chunk_pos`, read from chunk data or computed by the generator in this run.
    #[must_use]
    pub fn get_start_for_structure(
        &self,
        structure: StructureKeys,
        chunk_pos: Vector2<i32>,
    ) -> Option<StructureStart> {
        let key = (structure, chunk_pos.x, chunk_pos.y);
        if let Some(saved) = self.saved_starts.get().and_then(|saved| saved.get(&key)) {
            return Some(*saved);
        }
        let computed = self
            .structure_starts
            .get()?
            .get(&key)?
            .value()
            .get()?
            .clone()?;
        StructureStart::from_position(structure, chunk_pos, &computed)
    }

    /// The starts that `references` point to and whose bounding box contains `pos`.
    ///
    /// Mirrors vanilla `StructureManager.startsForStructure` over the references of the chunk
    /// that holds `pos`, filtered by `BoundingBox.isInside` as `getStructureAt` does.
    #[must_use]
    pub fn structure_starts_at(
        &self,
        pos: &BlockPos,
        references: &[(StructureKeys, Vector2<i32>)],
    ) -> Vec<StructureStart> {
        references
            .iter()
            .filter_map(|&(structure, chunk_pos)| {
                self.get_start_for_structure(structure, chunk_pos)
            })
            .filter(|start| start.is_inside(pos))
            .collect()
    }
}
