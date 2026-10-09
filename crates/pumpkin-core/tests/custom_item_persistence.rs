//! A custom item survives the save and load paths a relog and a world reload run. The content
//! registry is process-wide and freezes once, so this file is its own test binary.
#![expect(
    clippy::expect_used,
    reason = "a test fails on the first missing value"
)]

use std::sync::{Arc, Mutex, Once};

use pumpkin_core::entity::NBTStorage;
use pumpkin_data::dynamic::{self, ItemDefinition};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_inventory::build_equipment_slots;
use pumpkin_inventory::entity_equipment::EntityEquipment;
use pumpkin_inventory::inventory::{Inventory, sync_read_items_from_nbt, sync_write_items_to_nbt};
use pumpkin_inventory::player::player_inventory::PlayerInventory;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_world::data::player_data::PlayerDataStorage;
use uuid::Uuid;

const RUBY: &str = "test:ruby";
const OFFHAND: usize = 40;

fn ruby() -> &'static Item {
    static FREEZE: Once = Once::new();
    FREEZE.call_once(|| {
        dynamic::register_item(ItemDefinition {
            name: RUBY.to_string(),
            display: &Item::DIAMOND,
            block: None,
            tags: Vec::new(),
        })
        .expect("register test item");
        dynamic::freeze().expect("freeze registry");
    });
    Item::from_registry_key(RUBY).expect("custom item resolves after the freeze")
}

fn ruby_stack(count: u8) -> ItemStack {
    let mut stack = ItemStack::new(count, ruby());
    stack.set_custom_data("test", "charge", NbtTag::Int(3));
    stack
}

fn player_inventory() -> PlayerInventory {
    PlayerInventory::new(
        Arc::new(Mutex::new(EntityEquipment::new())),
        Arc::new(build_equipment_slots()),
    )
}

fn item_ids(nbt: &NbtCompound, list: &str) -> Vec<String> {
    nbt.get_list(list)
        .expect("item list")
        .iter()
        .filter_map(|tag| tag.extract_compound()?.get_string("id").map(str::to_string))
        .collect()
}

fn assert_ruby(stack: &ItemStack, count: u8) {
    assert_eq!(stack.item, ruby());
    assert_eq!(stack.item_count, count);
    assert_eq!(
        stack.get_custom_data("test", "charge"),
        Some(NbtTag::Int(3))
    );
}

/// The path a disconnect and reconnect run: `PlayerInventory::write_nbt` into the player data
/// file (`ServerPlayerData::handle_player_leave`), then the file back into a new inventory
/// (`Player::read_nbt` on join).
#[test]
fn custom_item_in_player_inventory_survives_relog() {
    let data_dir = tempfile::tempdir().expect("temp dir");
    let storage = PlayerDataStorage::new(data_dir.path(), true);
    let uuid = Uuid::new_v4();

    let before = player_inventory();
    before.set_stack(3, ruby_stack(5));
    before.set_stack(OFFHAND, ruby_stack(1));
    before.set_stack(4, ItemStack::new(2, &Item::DIAMOND));
    let mut nbt = NbtCompound::new();
    before.write_nbt(&mut nbt);
    let mut ids = item_ids(&nbt, "Inventory");
    ids.sort();
    assert_eq!(ids, ["minecraft:diamond", RUBY, RUBY]);
    storage
        .save_player_data(&uuid, nbt)
        .expect("save player data");

    let (found, loaded) = storage.load_player_data(&uuid).expect("load player data");
    assert!(found);
    let after = player_inventory();
    after.read_nbt_non_mut(&loaded);
    assert_ruby(&after.get_stack(3), 5);
    assert_ruby(&after.get_stack(OFFHAND), 1);
    assert_eq!(after.get_stack(4).item, &Item::DIAMOND);
}

/// The `Items` list that chests and the other container block entities save in chunk data.
#[test]
fn custom_item_in_container_saves_by_name() {
    let items = [ruby_stack(7), ItemStack::EMPTY.clone(), ruby_stack(2)];
    let mut nbt = NbtCompound::new();
    sync_write_items_to_nbt(&items, &mut nbt);
    assert_eq!(item_ids(&nbt, "Items"), [RUBY, RUBY]);

    let mut loaded: [ItemStack; 3] = std::array::from_fn(|_| ItemStack::EMPTY.clone());
    sync_read_items_from_nbt(&nbt, &mut loaded);
    assert_ruby(&loaded[0], 7);
    assert!(loaded[1].is_empty());
    assert_ruby(&loaded[2], 2);
}
