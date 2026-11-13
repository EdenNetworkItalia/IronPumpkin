use super::*;
use crate::ser::NetworkReadSliceExt;
use pumpkin_data::packet::CURRENT_MC_VERSION;
use pumpkin_nbt::tag::NbtTag;

#[test]
fn translatable_reason_is_network_nbt_with_key_and_argument() {
    #[allow(deprecated)]
    let reason = TextComponent::translate(
        "multiplayer.disconnect.incompatible",
        [TextComponent::text("NeoForge 26.3.0.64-beta")],
    );
    let mut data = Vec::new();
    CConfigDisconnect::new(&reason)
        .write_packet_data(&mut data, &CURRENT_MC_VERSION.into())
        .unwrap();

    let mut read = data.as_slice();
    let Some(NbtTag::Compound(compound)) = read.get_nbt(&CURRENT_MC_VERSION).unwrap() else {
        panic!("reason is not an NBT compound: {data:02x?}");
    };
    assert!(read.is_empty());
    assert_eq!(
        compound.get_string("translate"),
        Some("multiplayer.disconnect.incompatible")
    );
    let Some([NbtTag::String(argument)]) = compound.get_list("with") else {
        panic!("unexpected with list: {compound:?}");
    };
    assert_eq!(&**argument, "NeoForge 26.3.0.64-beta");
}
