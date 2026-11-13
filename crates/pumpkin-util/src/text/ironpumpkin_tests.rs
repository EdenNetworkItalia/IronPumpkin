use crate::text::{TextComponent, TextContent, color::NamedColor};
use crate::translation::{Locale, get_translation};
use crate::version::JavaMinecraftVersion;
use pumpkin_nbt::tag::NbtTag;

/// A vanilla key resolves through the `minecraft:` table. A key with no entry
/// prints as the bare key, like vanilla `Language.getOrDefault`.
#[test]
fn missing_translation_key_prints_bare() {
    #[allow(deprecated)]
    let translate =
        |key: &'static str, with: Vec<TextComponent>| TextComponent::translate(key, with);

    let joined = translate(
        "multiplayer.player.joined",
        vec![TextComponent::text("NAME".to_string())],
    );
    assert_eq!(joined.clone().to_pretty_console(), "NAME joined the game");
    assert_eq!(joined.get_text(), "NAME joined the game");

    let pig = translate("entity.minecraft.pig", vec![]);
    assert_eq!(pig.clone().to_pretty_console(), "Pig");
    assert_eq!(pig.to_legacy_string(Locale::EnUs), "Pig");
    assert_eq!(pig.get_text(), "Pig");

    // The miss branch is shared with the legacy serializer and the wasm hosts.
    let missing = translate("entity.acc-mod.golem", vec![]);
    assert_eq!(missing.clone().to_pretty_console(), "entity.acc-mod.golem");
    assert_eq!(
        missing.to_legacy_string(Locale::EnUs),
        "entity.acc-mod.golem"
    );
    assert_eq!(missing.get_text(), "entity.acc-mod.golem");
    assert_eq!(
        get_translation("minecraft:entity.acc-mod.golem", Locale::EnUs),
        "entity.acc-mod.golem"
    );
}

#[test]
fn translate_with_plain_string_arguments_parses_from_nbt() {
    // The configuration disconnect body of a NeoForge 26.3.0.64-beta server (run c): network
    // NBT of {with:["NeoForge 26.3.0.64-beta"], translate:"multiplayer.disconnect.incompatible"}.
    const BODY: &str = "0a09000477697468080000000100174e656f466f7267652032362e332e302e36342d\
                        626574610800097472616e736c61746500236d756c7469706c617965722e646973\
                        636f6e6e6563742e696e636f6d70617469626c6500";
    let bytes: Vec<u8> = (0..BODY.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&BODY[i..i + 2], 16).unwrap())
        .collect();
    assert_eq!(bytes.len(), 88);
    let mut reader =
        pumpkin_nbt::deserializer::NbtReadHelperJava::new(std::io::Cursor::new(bytes.as_slice()));
    let tag = pumpkin_nbt::tag::NbtTag::deserialize(&mut reader).unwrap();

    let component: TextComponent = serde_json::from_value(super::nbt_tag_to_json(&tag)).unwrap();
    let TextContent::Translate {
        translate, with, ..
    } = *component.0.content
    else {
        panic!("not a translate component: {component:?}");
    };
    assert_eq!(translate, "multiplayer.disconnect.incompatible");
    assert_eq!(with.len(), 1);
    assert_eq!(
        TextComponent(with[0].clone()).get_text(),
        "NeoForge 26.3.0.64-beta"
    );
}

#[test]
fn translate_with_mixed_arguments_round_trips_through_nbt() {
    #[allow(deprecated)]
    let component = TextComponent::translate(
        "chat.type.text",
        [
            TextComponent::text("a"),
            TextComponent::text("b").color_named(NamedColor::Red),
        ],
    );
    let bytes = component.encode();
    let mut reader =
        pumpkin_nbt::deserializer::NbtReadHelperJava::new(std::io::Cursor::new(&bytes[..]));
    let tag = pumpkin_nbt::tag::NbtTag::deserialize(&mut reader).unwrap();
    assert_eq!(TextComponent::from_nbt(&tag), component);
}

#[test]
fn translate_with_number_argument_parses_from_nbt() {
    let mut compound = pumpkin_nbt::compound::NbtCompound::new();
    compound.put_string("translate", "commands.time.query".to_string());
    compound.put_list("with", vec![pumpkin_nbt::tag::NbtTag::Int(1234)]);
    let component = TextComponent::from_nbt(&pumpkin_nbt::tag::NbtTag::Compound(compound));
    let TextContent::Translate { with, .. } = *component.0.content else {
        panic!("not a translate component: {component:?}");
    };
    assert_eq!(with.len(), 1);
    assert_eq!(TextComponent(with[0].clone()).get_text(), "1234");
}

fn decode_nbt(bytes: &[u8]) -> pumpkin_nbt::tag::NbtTag {
    let mut reader = pumpkin_nbt::deserializer::NbtReadHelperJava::new(std::io::Cursor::new(bytes));
    pumpkin_nbt::tag::NbtTag::deserialize(&mut reader).unwrap()
}

#[test]
fn translate_fallback_round_trips_through_nbt_and_json() {
    let component = TextComponent::translate_with_fallback(
        "test.key",
        "Install version %s",
        [TextComponent::text("1.0")],
    );

    // Vanilla `TranslatableContents.MAP_CODEC`: `fallback` is a string next to `translate`.
    let tag = decode_nbt(&component.encode());
    let pumpkin_nbt::tag::NbtTag::Compound(compound) = &tag else {
        panic!("not a compound: {tag:?}");
    };
    assert_eq!(compound.get_string("translate"), Some("test.key"));
    assert_eq!(compound.get_string("fallback"), Some("Install version %s"));
    assert_eq!(TextComponent::from_nbt(&tag), component);

    let json = component.to_json_value_for_version(&JavaMinecraftVersion::V_26_3);
    assert_eq!(json["fallback"], "Install version %s");
    assert_eq!(json["with"][0], "1.0");
    let serde_json = serde_json::to_value(&component).unwrap();
    assert_eq!(serde_json["fallback"], "Install version %s");
    assert_eq!(
        serde_json::from_value::<TextComponent>(serde_json).unwrap(),
        component
    );
}

#[test]
fn an_unknown_key_renders_the_fallback() {
    let unknown = TextComponent::translate_with_fallback(
        "test.unknown.key",
        "fallback %s",
        [TextComponent::text("1.0")],
    );
    let locale = crate::translation::Locale::EnUs;
    assert_eq!(unknown.clone().get_text(), "fallback 1.0");
    assert!(
        unknown
            .0
            .clone()
            .to_pretty_console()
            .contains("fallback 1.0")
    );
    assert!(unknown.0.to_bedrock_legacy(locale).contains("fallback 1.0"));
    assert!(unknown.to_legacy_string(locale).contains("fallback 1.0"));

    // A known key ignores the fallback.
    let known = TextComponent::translate_with_fallback(
        "multiplayer.disconnect.kicked",
        "fallback",
        Vec::<TextComponent>::new(),
    );
    assert_eq!(known.get_text(), "Kicked by an operator");
}

#[test]
fn translate_without_fallback_writes_no_fallback() {
    #[allow(deprecated)]
    let component = TextComponent::translate("test.key", [TextComponent::text("1.0")]);

    let tag = decode_nbt(&component.encode());
    let pumpkin_nbt::tag::NbtTag::Compound(compound) = &tag else {
        panic!("not a compound: {tag:?}");
    };
    assert_eq!(compound.get_string("fallback"), None);
    assert_eq!(TextComponent::from_nbt(&tag), component);

    let json = component.to_json_value_for_version(&JavaMinecraftVersion::V_26_3);
    assert!(json.get("fallback").is_none());
    let serde_json = serde_json::to_value(&component).unwrap();
    assert!(serde_json.get("fallback").is_none());
    assert_eq!(
        serde_json::from_value::<TextComponent>(serde_json).unwrap(),
        component
    );
}

#[test]
fn translation_arguments_format_floats_like_java() {
    let mut name = pumpkin_nbt::compound::NbtCompound::new();
    name.put_string("translate", "chat.type.text".to_string());
    name.put(
        "with",
        NbtTag::List(vec![
            NbtTag::Float(6.0),
            NbtTag::Double(6.0),
            NbtTag::Double(1e21),
            NbtTag::Float(1.5e-7),
            NbtTag::Double(-0.0),
            NbtTag::Double(1e7),
            NbtTag::Double(0.001),
            NbtTag::Double(9_999_999.0),
            NbtTag::Float(0.1),
            NbtTag::Double(f64::NAN),
            NbtTag::Float(f32::NEG_INFINITY),
            NbtTag::Double(9.999e-4),
        ]),
    );
    let component = TextComponent::from_nbt(&NbtTag::Compound(name));
    let super::TextContent::Translate { with, .. } = *component.0.content else {
        panic!("not a translation");
    };
    let texts: Vec<String> = with
        .into_iter()
        .map(|arg| super::TextComponent(arg).get_text())
        .collect();
    // `Float.toString` and `Double.toString` of the same values, printed by Java 27.
    assert_eq!(
        texts,
        [
            "6.0",
            "6.0",
            "1.0E21",
            "1.5E-7",
            "-0.0",
            "1.0E7",
            "0.001",
            "9999999.0",
            "0.1",
            "NaN",
            "-Infinity",
            "9.999E-4"
        ]
    );
}
