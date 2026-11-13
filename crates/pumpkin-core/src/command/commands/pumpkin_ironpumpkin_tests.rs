use super::ironpumpkin::EMPTY_CACHE_DURATION;
use super::*;

#[test]
fn empty_result_is_cached_for_a_few_minutes() {
    assert_eq!(EMPTY_CACHE_DURATION, Duration::from_mins(5));
    let cache = cache_after_fetch(None, Vec::new());
    assert!(cache.data.is_empty());
    assert_eq!(cache.ttl, EMPTY_CACHE_DURATION);
}

#[test]
fn empty_result_keeps_the_previous_list() {
    let previous = ContributorCache {
        fetched_at: Instant::now(),
        ttl: CACHE_DURATION,
        data: vec![Contributor {
            login: "test_user".to_string(),
        }],
    };
    let cache = cache_after_fetch(Some(previous), Vec::new());
    assert_eq!(cache.data.len(), 1);
    assert_eq!(cache.ttl, EMPTY_CACHE_DURATION);

    let fresh = cache_after_fetch(
        Some(cache),
        vec![Contributor {
            login: "other".to_string(),
        }],
    );
    assert_eq!(fresh.data[0].login, "other");
    assert_eq!(fresh.ttl, CACHE_DURATION);
}

#[test]
fn version_translation_names_ironpumpkin_in_every_language() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/translations");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let json = std::fs::read_to_string(&path).unwrap();
        let map: std::collections::HashMap<String, String> = serde_json::from_str(&json).unwrap();
        if let Some(version) = map.get("commands.pumpkin.version") {
            assert!(
                version.starts_with("IronPumpkin "),
                "{} has {version:?}",
                path.display()
            );
            checked += 1;
        }
    }
    assert!(checked > 0);
}
