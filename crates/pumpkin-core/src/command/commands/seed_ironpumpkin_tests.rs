use super::create_copy_on_click_text;
use pumpkin_util::version::JavaMinecraftVersion;

/// Vanilla `ComponentUtils.copyOnClickText` inside `commands.seed.success`.
#[test]
fn seed_text_matches_vanilla_copy_on_click_text() {
    let component = create_copy_on_click_text("12 64 -3".to_string());
    let json = component.to_json_value_for_version(&JavaMinecraftVersion::V_26_3);
    assert_eq!(json["translate"], "commands.seed.success");
    let brackets = &json["with"][0];
    assert_eq!(brackets["translate"], "chat.square_brackets");
    let inner = &brackets["with"][0];
    assert_eq!(inner["text"], "12 64 -3");
    assert_eq!(inner["color"], "green");
    assert_eq!(inner["insertion"], "12 64 -3");
    assert_eq!(
        inner["click_event"],
        serde_json::json!({"action": "copy_to_clipboard", "value": "12 64 -3"})
    );
    assert_eq!(inner["hover_event"]["action"], "show_text");
    assert_eq!(
        inner["hover_event"]["value"]["translate"],
        "chat.copy.click"
    );
}
