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

use pumpkin_data::data_component::DataComponent;
use pumpkin_data::data_component_impl::{BundleContentsImpl, DataComponentImpl};
use pumpkin_data::dynamic::{self, ContentIds, ItemDefinition};
use pumpkin_data::dynamic_registries::{self, DataComponentTypeDefinition};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::recipes::RecipeCategoryTypes;
use pumpkin_data::tag::RegistryKey;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_protocol::ClientPacket;
use pumpkin_protocol::codec::data_component::{deserialize, serialize};
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
        dynamic_registries::register_data_component_type(DataComponentTypeDefinition {
            name: "test:charge".to_owned(),
            persistent: true,
            validate: |_| true,
        })
        .expect("register data component type");
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
        .write_length_prefixed_with_version(&mut out, &version.into())
        .unwrap();
    serializer
        .write_item_cost_with_version(&mut out, &version.into())
        .unwrap();
    serializer
        .write_template_with_version(&mut out, &version.into())
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
            .write_packet_data(&mut out, &JavaMinecraftVersion::V_26_3.into())
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
            components: None,
        },
    });
    let entry = dynamic_entries(&[recipe]);

    // The slot display shows each generated id once in tag order, then the ruby's display item;
    // no custom id. The holder set that the client matches holds the generated ids only: with
    // the diamond in it, the client would take a real diamond.
    let ids: Vec<i32> = generated
        .iter()
        .chain([&Item::DIAMOND.id])
        .map(|&id| i32::from(id))
        .collect();
    let count = i32::try_from(ids.len()).unwrap();
    let holder_set = [
        var_int(count),
        ids[..ids.len() - 1]
            .iter()
            .flat_map(|&id| var_int(id))
            .collect(),
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
    // A `Real` client hashes the custom id.
    assert!(hash_of(ruby.id).hash_equals(&custom));
    assert!(!hash_of(Item::EMERALD.id).hash_equals(&custom));
    assert!(hash_of(Item::DIAMOND.id).hash_equals(&ItemStack::new(3, &Item::DIAMOND)));
    assert!(!hash_of(ruby.id).hash_equals(&ItemStack::new(3, &Item::DIAMOND)));
}

/// A diamond stack with the `test:charge` mod component set to the int 7.
fn charged_stack() -> (ItemStack, u16) {
    ruby();
    let charge = dynamic_registries::data_component_type_by_name("test:charge")
        .expect("type binds")
        .id;
    let mut stack = ItemStack::new(3, &Item::DIAMOND);
    stack.custom_components.push((charge, NbtTag::Int(7)));
    (stack, charge)
}

/// `HashOps.createInt(7)`: the int tag, then the value in little endian.
fn int_seven_hash() -> i32 {
    crc32c::crc32c(&[8, 7, 0, 0, 0]) as i32
}

#[test]
fn client_hash_of_mod_component_matches_like_custom_data() {
    let (stack, charge) = charged_stack();
    let hash_of = |added: &[(u16, i32)]| {
        let mut bytes = [
            vec![1],
            var_int(i32::from(Item::DIAMOND.id)),
            var_int(3),
            var_int(added.len() as i32),
        ]
        .concat();
        for (id, hash) in added {
            bytes.extend(var_int(i32::from(*id)));
            bytes.extend(hash.to_be_bytes());
        }
        bytes.extend(var_int(0));
        OptionalItemStackHash::read(&mut Cursor::new(bytes)).unwrap()
    };
    // A `Real` client hashes the mod component.
    assert!(hash_of(&[(charge, int_seven_hash())]).hash_equals(&stack));
    assert!(!hash_of(&[(charge, int_seven_hash() ^ 1)]).hash_equals(&stack));
    // A `Display` client never gets the mod component.
    assert!(hash_of(&[]).hash_equals(&stack));
}

#[test]
fn nested_template_keeps_mod_components() {
    let (stack, charge) = charged_stack();
    let bundle = BundleContentsImpl {
        items: vec![stack],
        selected_item: -1,
    };

    let mut real = Vec::new();
    serialize(
        DataComponent::BundleContents,
        &bundle,
        &mut real,
        ContentIds::Real,
    )
    .unwrap();
    // The network NBT of an int tag: its type id, then the value in big endian.
    let expected = [
        var_int(1),
        var_int(i32::from(Item::DIAMOND.id)),
        var_int(3),
        var_int(1),
        var_int(0),
        var_int(i32::from(charge)),
        vec![3, 0, 0, 0, 7],
    ]
    .concat();
    assert_eq!(real, expected);

    let decoded = deserialize(DataComponent::BundleContents, &mut real.as_slice()).unwrap();
    let decoded = decoded
        .as_any()
        .downcast_ref::<BundleContentsImpl>()
        .unwrap();
    assert_eq!(
        decoded.items[0].custom_components,
        vec![(charge, NbtTag::Int(7))]
    );

    // A `Display` client does not know the mod component type.
    let mut display = Vec::new();
    serialize(
        DataComponent::BundleContents,
        &bundle,
        &mut display,
        ContentIds::Display,
    )
    .unwrap();
    assert_eq!(display[display.len() - 2..], [0, 0]);

    // `ItemStackTemplate.CODEC` of the entry, with the mod component hashed like `CustomData`.
    let string = |value: &str| {
        let mut bytes = vec![12];
        bytes.extend((value.len() as i32).to_le_bytes());
        bytes.extend(value.encode_utf16().flat_map(u16::to_le_bytes));
        crc32c::crc32c(&bytes)
    };
    let map = |entries: &[(&str, u32)]| {
        let mut entries: Vec<_> = entries.iter().map(|(k, v)| (string(k), *v)).collect();
        entries.sort_unstable();
        let mut bytes = vec![2];
        for (key, value) in entries {
            bytes.extend(key.to_le_bytes());
            bytes.extend(value.to_le_bytes());
        }
        bytes.push(3);
        crc32c::crc32c(&bytes)
    };
    let template = map(&[
        ("id", string("minecraft:diamond")),
        ("count", crc32c::crc32c(&[8, 3, 0, 0, 0])),
        (
            "components",
            map(&[("test:charge", int_seven_hash() as u32)]),
        ),
    ]);
    let list = crc32c::crc32c(&[[4].as_slice(), &template.to_le_bytes(), &[5]].concat());
    assert_eq!(bundle.get_hash() as u32, list);
}
