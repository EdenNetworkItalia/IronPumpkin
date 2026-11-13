use super::*;

// Serverbound `minecraft:brand` bodies recorded by `pumpkin-neoforge-client` from a
// vanilla client and a NeoForge 26.3 client.
#[test]
fn brand_decodes_recorded_bytes() {
    let vanilla = [0x07, 0x76, 0x61, 0x6e, 0x69, 0x6c, 0x6c, 0x61];
    assert_eq!(read_brand(&vanilla, false).unwrap(), "vanilla");
    let neoforge = [0x08, 0x6e, 0x65, 0x6f, 0x66, 0x6f, 0x72, 0x67, 0x65];
    assert_eq!(read_brand(&neoforge, false).unwrap(), "neoforge");
}

#[test]
fn brand_rejects_bytes_after_the_string() {
    let data = [0x07, 0x76, 0x61, 0x6e, 0x69, 0x6c, 0x6c, 0x61, 0x00];
    assert!(matches!(
        read_brand(&data, false),
        Err(ReadingError::TooLarge(_))
    ));
    // A payload of a joined packet can end with the padding of the last part.
    assert_eq!(read_brand(&data, true).unwrap(), "vanilla");
}
