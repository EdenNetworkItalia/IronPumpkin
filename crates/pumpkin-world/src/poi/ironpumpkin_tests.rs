use super::*;

#[test]
fn poi_records_store_the_vanilla_pos_array() {
    use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};

    let section = |record: NbtCompound| {
        let mut section = NbtCompound::new();
        section.put_byte("Valid", 1);
        section.put_list("Records", vec![NbtTag::Compound(record)]);
        let mut sections = NbtCompound::new();
        sections.put_compound("4", section);
        let mut root = NbtCompound::new();
        root.put_int("DataVersion", 5023);
        root.put_compound("Sections", sections);
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(&pumpkin_nbt::Nbt::from(root).write())
            .unwrap();
        encoder.finish().unwrap()
    };
    let read_position = |compressed: &[u8]| {
        let data = PoiRegion::decompress_chunk_data(compressed).unwrap();
        let record = &data.sections["4"].records[0];
        (record.pos(), record.poi_type.clone(), record.free_tickets)
    };

    // 26.3 `PoiRecord.Packed.CODEC`: `pos` with `BlockPos.CODEC`, an int array.
    let mut vanilla = NbtCompound::new();
    vanilla.put("pos", NbtTag::IntArray(vec![12, 70, -3]));
    vanilla.put_string("type", "minecraft:meeting".to_string());
    vanilla.put_int("free_tickets", 32);
    let expected = (
        BlockPos(Vector3::new(12, 70, -3)),
        "minecraft:meeting".to_string(),
        32,
    );
    assert_eq!(read_position(&section(vanilla)), expected);

    let mut old = NbtCompound::new();
    old.put_int("x", 12);
    old.put_int("y", 70);
    old.put_int("z", -3);
    old.put_string("type", "minecraft:meeting".to_string());
    old.put_int("free_tickets", 32);
    assert_eq!(read_position(&section(old)), expected);

    let mut vanilla = NbtCompound::new();
    vanilla.put("pos", NbtTag::IntArray(vec![12, 70, -3]));
    vanilla.put_string("type", "minecraft:meeting".to_string());
    let data = PoiRegion::decompress_chunk_data(&section(vanilla)).unwrap();
    let written = PoiRegion::compress_chunk_data(&data).unwrap();
    let mut uncompressed = Vec::new();
    ZlibDecoder::new(&written[..])
        .read_to_end(&mut uncompressed)
        .unwrap();
    let mut cursor = Cursor::new(uncompressed);
    let mut reader = pumpkin_nbt::deserializer::NbtReadHelperJava::new(
        pumpkin_nbt::deserializer::NbtStreamReader(&mut cursor),
    );
    let root = pumpkin_nbt::Nbt::read(&mut reader).unwrap().root_tag;
    let records = root
        .get_compound("Sections")
        .and_then(|sections| sections.get_compound("4"))
        .and_then(|section| section.get_list("Records"))
        .unwrap();
    let record = records[0].extract_compound().unwrap();
    assert_eq!(record.get_int_array("pos"), Some(&[12, 70, -3][..]));
    assert!(record.get_int("x").is_none());
}
