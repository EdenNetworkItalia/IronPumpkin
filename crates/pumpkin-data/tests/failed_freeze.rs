//! A failed freeze installs no tables. An integration test has its own process, so it can run the
//! process-wide freeze.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use pumpkin_data::dynamic::{self, ItemDefinition, RegistryError};
use pumpkin_data::item::Item;

#[test]
fn failed_freeze_leaves_no_tables() {
    dynamic::register_item(ItemDefinition {
        name: "test:orphan".to_string(),
        display: &Item::STICK,
        block: Some("test:missing".to_string()),
        tags: Vec::new(),
    })
    .unwrap();
    assert!(dynamic::tables().is_none());
    assert_eq!(
        dynamic::freeze().unwrap_err(),
        RegistryError::UnknownBlock {
            item: "test:orphan".to_string(),
            block: "test:missing".to_string(),
        }
    );
    assert!(dynamic::tables().is_none());
    assert_eq!(
        dynamic::freeze().unwrap_err(),
        RegistryError::RegistryFrozen
    );
    assert!(dynamic::tables().is_none());
}
