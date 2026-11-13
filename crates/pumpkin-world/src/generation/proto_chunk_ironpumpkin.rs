use super::{NbtCompound, NbtTag, ProtoChunk, StructureInstance, StructureStart, Vector2, start};

impl ProtoChunk {
    /// The vanilla `structures` tag of this chunk, as `SerializableChunkData.packStructureData`
    /// builds it.
    #[must_use]
    pub fn structure_data(&self) -> NbtCompound {
        if let Some(saved) = &self.saved_structure_data {
            return saved.clone();
        }
        let chunk_pos = Vector2::new(self.x, self.z);
        let mut starts = NbtCompound::new();
        for (structure, instance) in &self.structure_starts {
            if let StructureInstance::Start(position) = instance
                && StructureStart::from_position(*structure, chunk_pos, position).is_some()
            {
                starts.put_compound(
                    &format!("minecraft:{}", structure.to_name()),
                    start::create_tag(*structure, chunk_pos, position),
                );
            }
        }
        let mut references = NbtCompound::new();
        for (structure, chunks) in &self.packed_structure_references {
            references.put(
                &format!("minecraft:{}", structure.to_name()),
                NbtTag::LongArray(chunks.clone()),
            );
        }
        let mut tag = NbtCompound::new();
        tag.put_compound("starts", starts);
        tag.put_compound("References", references);
        tag
    }
}
