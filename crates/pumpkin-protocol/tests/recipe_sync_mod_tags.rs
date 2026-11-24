//! The recipe sync of a mod tag and a custom item. Vanilla `RecipeManager` sends the items of
//! each ingredient, and a vanilla client knows no mod tag. The content registry is process-wide
//! and freezes once, so this file is its own test binary.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use std::sync::Once;

use pumpkin_data::dynamic::{self, ContentIds, ContentKind, ItemDefinition, TagDefinition};
use pumpkin_data::item::Item;
use pumpkin_protocol::EncodingKey;
use pumpkin_protocol::codec::recipe::{OwnedRecipeIngredient, PropertySetSync, StonecutterSync};
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_protocol::java::client::play::CUpdateRecipes;
use pumpkin_protocol::ser::NetworkWriteExt;
use pumpkin_util::version::JavaMinecraftVersion;

fn ruby() -> &'static Item {
    static FREEZE: Once = Once::new();
    FREEZE.call_once(|| {
        dynamic::register_item(ItemDefinition {
            name: "test:ruby".to_string(),
            display: &Item::DIAMOND,
            block: None,
            tags: Vec::new(),
        })
        .unwrap();
        dynamic::register_tag(TagDefinition {
            kind: ContentKind::Item,
            name: "test:gems".to_string(),
            values: vec!["minecraft:emerald".to_string(), "test:ruby".to_string()],
        })
        .unwrap();
        dynamic::freeze().unwrap();
    });
    Item::from_registry_key("test:ruby").unwrap()
}

fn var_int(value: i32) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.write_var_int(&VarInt(value)).unwrap();
    bytes
}

/// The body of one property set and one stonecutter recipe, both with the `test:gems` tag.
fn body(ids: ContentIds) -> Vec<u8> {
    let gems = [OwnedRecipeIngredient::Tagged("test:gems".to_string())];
    CUpdateRecipes::encode(
        &[PropertySetSync {
            key: "minecraft:smithing_addition",
            ingredients: &gems,
        }],
        &[StonecutterSync {
            ingredient: &gems[0],
            result_item: "minecraft:stone",
            result_count: 1,
        }],
        EncodingKey::new(JavaMinecraftVersion::V_26_3, ids),
    )
    .unwrap()
}

/// The property set and the holder set hold `members`, as network ids, and no tag name.
fn expected_prefix(members: &[u16]) -> Vec<u8> {
    let mut expected = var_int(1);
    expected
        .write_string("minecraft:smithing_addition")
        .unwrap();
    expected.extend(var_int(members.len() as i32));
    for id in members {
        expected.extend(var_int(i32::from(*id)));
    }
    expected.extend(var_int(1));
    expected.extend(var_int(members.len() as i32 + 1));
    for id in members {
        expected.extend(var_int(i32::from(*id)));
    }
    expected
}

#[test]
fn a_display_client_gets_the_display_items_of_a_mod_tag() {
    let ruby = ruby();
    assert!(OwnedRecipeIngredient::Tagged("test:gems".to_string()).match_item(ruby));
    let body = body(ContentIds::Display);
    let expected = expected_prefix(&[Item::EMERALD.id, Item::DIAMOND.id]);
    assert_eq!(body[..expected.len()], expected[..]);
}

#[test]
fn a_real_client_gets_the_real_ids_of_a_mod_tag() {
    let ruby = ruby();
    let real = ruby.to_java_network_id(ContentIds::Real);
    assert_ne!(real, Item::DIAMOND.id);
    let body = body(ContentIds::Real);
    let expected = expected_prefix(&[Item::EMERALD.id, real]);
    assert_eq!(body[..expected.len()], expected[..]);
}
