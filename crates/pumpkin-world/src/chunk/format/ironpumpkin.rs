use super::{ChunkData, StructureKeys, StructureStart, Vector2, error, unpack_chunk_pos, warn};

impl ChunkData {
    /// Vanilla `SerializableChunkData.unpackStructureStart`: the valid starts this chunk owns.
    #[must_use]
    pub fn structure_starts(&self) -> Vec<StructureStart> {
        let Some(starts) = self.structures.get_compound("starts") else {
            return Vec::new();
        };
        starts
            .child_tags
            .iter()
            .filter_map(|(key, tag)| {
                if StructureKeys::from_name(key).is_none() {
                    error!("Unknown structure start: {key}");
                    return None;
                }
                StructureStart::load_static_start(tag.extract_compound()?)
            })
            .collect()
    }

    /// Vanilla `SerializableChunkData.unpackStructureReferences`: the structure and start chunk
    /// of every start whose bounding box reaches into this chunk.
    #[must_use]
    pub fn structure_references(&self) -> Vec<(StructureKeys, Vector2<i32>)> {
        let Some(references) = self.structures.get_compound("References") else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (key, tag) in &references.child_tags {
            let Some(structure) = StructureKeys::from_name(key) else {
                warn!(
                    "Found reference to unknown structure '{key}' in chunk [{}, {}], discarding",
                    self.x, self.z
                );
                continue;
            };
            for &packed in tag.extract_long_array().unwrap_or_default() {
                let start = unpack_chunk_pos(packed);
                if start.x.abs_diff(self.x).max(start.y.abs_diff(self.z)) > 8 {
                    warn!(
                        "Found invalid structure reference [ {key} @ [{}, {}] ] for chunk [{}, {}].",
                        start.x, start.y, self.x, self.z
                    );
                    continue;
                }
                out.push((structure, start));
            }
        }
        out
    }
}
