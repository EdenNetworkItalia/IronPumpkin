//! The `sound` enum of the v0.1 and v0.2 WIT files has the cases of `Sound::NAMES` in the same
//! order. The hosts map a case to a sound by its index, and the v0.1 file has no generator.

#![expect(clippy::expect_used, reason = "a test fails on the first missing part")]

use pumpkin_data::sound::Sound;

fn wit_cases(wit: &str) -> Vec<String> {
    let start = wit.find("enum sound {").expect("the sound enum") + "enum sound {".len();
    let body = &wit[start..];
    let body = &body[..body.find('}').expect("the end of the sound enum")];
    body.split(',')
        .map(str::trim)
        .filter(|case| !case.is_empty())
        .map(str::to_owned)
        .collect()
}

fn check(wit: &str) {
    let cases = wit_cases(wit);
    assert_eq!(cases.len(), Sound::NAMES.len());
    for (index, (case, name)) in cases.iter().zip(Sound::NAMES).enumerate() {
        assert_eq!(
            *case,
            name.replace(['.', '_'], "-"),
            "the case at index {index} differs"
        );
    }
}

#[test]
fn the_v0_2_sound_enum_follows_the_sound_names() {
    check(include_str!("../../pumpkin-plugin-wit/v0.2/sounds.wit"));
}

#[test]
fn the_v0_1_sound_enum_follows_the_sound_names() {
    check(include_str!("../../pumpkin-plugin-wit/v0.1/sounds.wit"));
}
