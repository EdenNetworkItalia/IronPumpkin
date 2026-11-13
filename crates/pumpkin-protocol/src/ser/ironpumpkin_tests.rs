use super::{NetworkReadSliceExt, NetworkWriteExt};
use pumpkin_data::packet::CURRENT_MC_VERSION;
use pumpkin_util::text::{TextComponent, color::NamedColor};

#[test]
fn translate_with_mixed_arguments_round_trips() {
    #[allow(deprecated)]
    let component = TextComponent::translate(
        "chat.type.text",
        [
            TextComponent::text("a"),
            TextComponent::text("b").color_named(NamedColor::Red),
        ],
    );
    let mut data = Vec::new();
    data.write_component(&component, &CURRENT_MC_VERSION)
        .unwrap();
    let mut read = data.as_slice();
    assert_eq!(read.get_component(&CURRENT_MC_VERSION).unwrap(), component);
    assert!(read.is_empty());
}
