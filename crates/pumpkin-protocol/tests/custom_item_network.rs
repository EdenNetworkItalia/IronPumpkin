//! A custom item reaches vanilla clients as its display item. The content registry is
//! process-wide and freezes once, so this file is its own test binary.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails on the first missing value"
)]

use std::borrow::Cow;
use std::io::Cursor;
use std::sync::Once;

use pumpkin_data::dynamic::{self, ItemDefinition};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::recipes::RecipeCategoryTypes;
use pumpkin_data::tag::RegistryKey;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_protocol::ClientPacket;
use pumpkin_protocol::codec::item_stack_seralizer::{
    ItemStackSerializer, ItemStackTemplateSerializer, OptionalItemStackHash,
};
use pumpkin_protocol::codec::recipe::{
    DynamicRecipe, OwnedCraftingRecipe, OwnedRecipeIngredient, OwnedRecipeResult,
};
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_protocol::java::client::play::CRecipeBookAdd;
use pumpkin_util::version::JavaMinecraftVersion;

fn ruby() -> &'static Item {
    static FREEZE: Once = Once::new();
    FREEZE.call_once(|| {
        dynamic::register_item(ItemDefinition {
            name: "test:ruby".to_string(),
            display: &Item::DIAMOND,
            block: None,
            tags: vec!["minecraft:piglin_loved".to_string()],
        })
        .expect("register test item");
        // Shows as a gold ingot, which is in `minecraft:piglin_loved`.
        dynamic::register_item(ItemDefinition {
            name: "test:gold_coin".to_string(),
            display: &Item::GOLD_INGOT,
            block: None,
            tags: Vec::new(),
        })
        .expect("register test item");
        dynamic::freeze().expect("freeze registry");
    });
    Item::from_registry_key("test:ruby").expect("custom item resolves after the freeze")
}

fn stack(item: &'static Item) -> ItemStack {
    let mut stack = ItemStack::new(3, item);
    stack.set_custom_data("test", "charge", NbtTag::Int(7));
    stack
}

/// Every item stack encoding a packet can use, concatenated.
fn encodings(stack: ItemStack) -> Vec<u8> {
    let version = JavaMinecraftVersion::V_26_3;
    let serializer = ItemStackSerializer(Cow::Owned(stack.clone()));
    let mut out = Vec::new();
    serializer.write(&mut out).unwrap();
    serializer
        .write_length_prefixed_with_version(&mut out, &version)
        .unwrap();
    serializer
        .write_item_cost_with_version(&mut out, &version)
        .unwrap();
    serializer
        .write_template_with_version(&mut out, &version)
        .unwrap();
    ItemStackTemplateSerializer::from(stack)
        .write(&mut out)
        .unwrap();
    out
}

fn var_int(value: i32) -> Vec<u8> {
    let mut out = Vec::new();
    VarInt(value).encode(&mut out).unwrap();
    out
}

#[test]
fn custom_item_stack_encodes_as_its_display_item() {
    let ruby = ruby();
    assert_eq!(encodings(stack(ruby)), encodings(stack(&Item::DIAMOND)));
}

#[test]
fn tagged_ingredient_matches_a_custom_item() {
    let ruby = ruby();
    let tagged = |tag: &str| OwnedRecipeIngredient::Tagged(tag.to_string());
    // From the display item, then from the definition.
    assert!(tagged("#c:gems/diamond").match_item(ruby));
    assert!(tagged("minecraft:piglin_loved").match_item(ruby));
    assert!(!tagged("minecraft:piglin_loved").match_item(&Item::DIAMOND));
    assert!(!tagged("minecraft:swords").match_item(ruby));
}

/// The bytes of the dynamic recipe entries: the packet without them is the same up to the entry
/// count, which keeps its length.
fn dynamic_entries(recipes: &[DynamicRecipe]) -> Vec<u8> {
    let encode = |recipes: &[DynamicRecipe]| {
        let mut out = Vec::new();
        CRecipeBookAdd::new(true, recipes)
            .write_packet_data(&mut out, &JavaMinecraftVersion::V_26_3)
            .unwrap();
        out
    };
    let (base, full) = (encode(&[]), encode(recipes));
    let count_len = base.iter().position(|byte| byte & 0x80 == 0).unwrap() + 1;
    let vanilla = &base[count_len..base.len() - 1];
    assert_eq!(&full[count_len..count_len + vanilla.len()], vanilla);
    full[count_len + vanilla.len()..full.len() - 1].to_vec()
}

#[test]
fn recipe_book_tag_sends_each_network_id_once() {
    let ruby = ruby();
    let coin = Item::from_registry_key("test:gold_coin").unwrap();
    let tag = "minecraft:piglin_loved";
    let (generated, custom) = dynamic::tag_ids(RegistryKey::Item, tag).unwrap();
    // The coin joins through its display item, the ruby through its explicit tag.
    assert!(generated.contains(&Item::GOLD_INGOT.id));
    assert!(!generated.contains(&Item::DIAMOND.id));
    assert_eq!(custom, [coin.id, ruby.id]);

    let recipe = DynamicRecipe::Crafting(OwnedCraftingRecipe::Shapeless {
        recipe_id: None,
        category: RecipeCategoryTypes::Misc,
        group: None,
        ingredients: vec![OwnedRecipeIngredient::Tagged(format!("#{tag}"))],
        result: OwnedRecipeResult {
            item_id: "minecraft:stick".to_string(),
            count: 1,
        },
    });
    let entry = dynamic_entries(&[recipe]);

    // Each generated id once in tag order, then the ruby's display item; no custom id.
    let ids: Vec<i32> = generated
        .iter()
        .chain([&Item::DIAMOND.id])
        .map(|&id| i32::from(id))
        .collect();
    let count = i32::try_from(ids.len()).unwrap();
    let holder_set = [
        var_int(count + 1),
        ids.iter().flat_map(|&id| var_int(id)).collect(),
    ]
    .concat();
    // Composite slot display (10) of item slot displays (4).
    let slot_display = [
        var_int(10),
        var_int(count),
        ids.iter()
            .flat_map(|&id| [var_int(4), var_int(id)].concat())
            .collect(),
    ]
    .concat();
    let contains = |needle: &[u8]| entry.windows(needle.len()).any(|window| window == needle);
    assert!(entry[..entry.len() - 1].ends_with(&holder_set));
    assert!(contains(&slot_display));
    for custom_id in [coin.id, ruby.id] {
        let raw = [var_int(4), var_int(i32::from(custom_id))].concat();
        assert!(!contains(&raw));
    }
}

#[test]
fn vanilla_item_stack_bytes_are_unchanged() {
    ruby();
    let mut out = Vec::new();
    ItemStackSerializer::from(ItemStack::new(3, &Item::DIAMOND))
        .write(&mut out)
        .unwrap();
    // count, item id, components to add, components to remove
    let expected = [
        var_int(3),
        var_int(i32::from(Item::DIAMOND.id)),
        var_int(0),
        var_int(0),
    ]
    .concat();
    assert_eq!(out, expected);
}

#[test]
fn client_hash_of_display_item_matches_custom_stack() {
    let ruby = ruby();
    let hash_of = |item_id: u16| {
        let bytes = [
            vec![1],
            var_int(i32::from(item_id)),
            var_int(3),
            var_int(0),
            var_int(0),
        ]
        .concat();
        OptionalItemStackHash::read(&mut Cursor::new(bytes)).unwrap()
    };
    let custom = ItemStack::new(3, ruby);
    assert!(hash_of(Item::DIAMOND.id).hash_equals(&custom));
    assert!(!hash_of(ruby.id).hash_equals(&custom));
    assert!(hash_of(Item::DIAMOND.id).hash_equals(&ItemStack::new(3, &Item::DIAMOND)));
}
