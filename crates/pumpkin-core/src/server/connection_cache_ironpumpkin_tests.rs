use super::{BasicConfiguration, CachedStatus};

fn status_json(advertise_modded: bool) -> serde_json::Value {
    let config = BasicConfiguration {
        use_favicon: false,
        advertise_modded,
        ..BasicConfiguration::default()
    };
    let response = CachedStatus::build_response(&config, "test", 20);
    serde_json::to_value(&response).unwrap()
}

#[test]
fn status_advertises_is_modded_when_enabled() {
    assert_eq!(status_json(true)["isModded"], serde_json::Value::Bool(true));
}

#[test]
fn status_omits_is_modded_when_disabled() {
    let json = status_json(false);
    assert!(json.get("isModded").is_none());
}

#[test]
fn status_uses_the_vanilla_enforces_secure_chat_key() {
    let json = status_json(false);
    assert_eq!(json["enforcesSecureChat"], serde_json::Value::Bool(true));
    assert!(json.get("enforceSecureChat").is_none());
}
