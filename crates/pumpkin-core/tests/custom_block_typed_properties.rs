//! The `block_state` component placement and the debug stick keep the properties of a custom block
//! that the typed `Property` cannot hold, against a server of the support module. The content
//! registry is process-wide and freezes once at the server start, so this file is its own test
//! binary.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

mod support;

use std::borrow::Cow;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use pumpkin_core::block::{BlockBehaviour, OnPlaceArgs};
use pumpkin_core::content::behaviour;
use pumpkin_core::entity::player::Player;
use pumpkin_core::item::ItemBehaviour;
use pumpkin_core::item::items::debug_stick::DebugStickItem;
use pumpkin_data::data_component_impl::BlockStateImpl;
use pumpkin_data::dynamic::{BlockDefinition, BlockPropertyDefinition};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::{Block, BlockDirection, BlockStateId};
use pumpkin_protocol::{codec::var_int::VarInt, java::server::play::SUseItemOn};
use pumpkin_util::GameMode;
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};
use pumpkin_util::permission::PermissionLvl;
use pumpkin_world::world::BlockFlags;

/// Places an old blue sprout facing south, a state the typed default does not have.
struct SproutBlock;

impl BlockBehaviour for SproutBlock {
    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        old_blue(args.block, "south")
    }
}

/// A custom block with an int property above the table of `Property::value_str`, a property
/// without a `Property` variant, and two typed properties. Registered before the server starts.
static SPROUT: LazyLock<&'static Block> = LazyLock::new(|| {
    behaviour::register_block(
        BlockDefinition {
            name: "test:sprout".to_string(),
            display: Block::STONE.default_state.id,
            properties: vec![
                BlockPropertyDefinition::int("age", 0, 30, 0),
                BlockPropertyDefinition::enumeration("color", &["red", "green", "blue"], "red"),
                BlockPropertyDefinition::enumeration(
                    "facing",
                    &["north", "east", "south", "west"],
                    "north",
                ),
                BlockPropertyDefinition::bool("waterlogged", false),
            ],
            tags: Vec::new(),
        },
        Some(Arc::new(SproutBlock)),
    )
    .unwrap();
    support::server();
    Block::from_name("test:sprout").unwrap()
});

fn old_blue(block: &Block, facing: &str) -> BlockStateId {
    block
        .from_string_properties(&[("age", "30"), ("color", "blue"), ("facing", facing)])
        .to_state_id(block)
}

fn props(id: BlockStateId) -> Vec<(&'static str, &'static str)> {
    Block::from_state_id(id).string_properties(id).unwrap()
}

/// Loads the chunk of `pos` for the rest of the process.
fn load(pos: BlockPos) {
    let world = support::world();
    let chunk = pos.chunk_position();
    {
        let mut loading = world.level.chunk_loading.lock().unwrap();
        loading.add_force_ticket(chunk);
        loading.send_change();
    };
    let deadline = Instant::now() + support::SLOW_RUNNER;
    while !world.level.is_chunk_loaded(&chunk) {
        assert!(Instant::now() < deadline, "the chunk of {pos} did not load");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn set(pos: BlockPos, state: BlockStateId) {
    load(pos);
    support::world().set_block_state(&pos, state, BlockFlags::empty());
}

fn state_at(pos: BlockPos) -> BlockStateId {
    support::world().get_block_state_id(&pos)
}

/// A player that neither the spawn protection nor the game master check stops.
fn creative(name: &str) -> Arc<Player> {
    let player = support::player(name);
    player.permission_lvl.store(PermissionLvl::Four);
    player.gamemode.store(GameMode::Creative);
    player
}

/// A component with a valid `waterlogged` and a `facing` that the sprout does not have.
#[test]
fn block_state_component_placement_keeps_the_custom_properties() {
    let sprout = *SPROUT;
    let _serial = support::serial();
    let player = creative("Planter");
    let floor = BlockPos::new(8, 120, 8);
    set(floor, Block::STONE.default_state.id);
    let pos = floor.up();
    set(pos, Block::AIR.default_state.id);
    player.living_entity.entity.set_pos(Vector3::new(
        f64::from(pos.0.x) - 2.5,
        f64::from(pos.0.y),
        f64::from(pos.0.z) + 0.5,
    ));
    let mut stack = ItemStack::new(1, &Item::STONE);
    stack.set_data_component(BlockStateImpl {
        properties: Cow::Owned(vec![
            (Cow::Borrowed("waterlogged"), Cow::Borrowed("true")),
            (Cow::Borrowed("facing"), Cow::Borrowed("up")),
        ]),
    });
    player.inventory().set_held_item(stack);

    let server = support::server();
    let placed = server
        .block_registry
        .place_block(
            &player,
            sprout,
            server,
            &SUseItemOn {
                hand: VarInt(0),
                position: floor,
                face: VarInt(1),
                cursor_pos: Vector3::new(0.5, 1.0, 0.5),
                inside_block: false,
                is_against_world_border: false,
                sequence: VarInt(1),
            },
            floor,
            BlockDirection::Up,
        )
        .unwrap();

    // As `BlockItemStateProperties.apply`: the invalid facing keeps the placed value.
    let expected = [
        ("age", "30"),
        ("color", "blue"),
        ("facing", "south"),
        ("waterlogged", "true"),
    ];
    assert_eq!(props(placed.unwrap().1), expected);
    assert_eq!(props(state_at(pos)), expected);
}

#[test]
fn debug_stick_keeps_the_custom_properties() {
    let sprout = *SPROUT;
    let _serial = support::serial();
    let player = creative("Debugger");
    let pos = BlockPos::new(24, 120, 8);
    set(pos, old_blue(sprout, "north"));

    // The first property the stick can select is `facing`: `age` 30 and `color` have no
    // typed value.
    let mut stick = ItemStack::new(1, &Item::DEBUG_STICK);
    DebugStickItem.use_on_block(
        &mut stick,
        &player,
        pos,
        BlockDirection::Up,
        Vector3::new(0.5, 1.0, 0.5),
        sprout,
        support::server(),
    );

    assert_eq!(
        props(state_at(pos)),
        [
            ("age", "30"),
            ("color", "blue"),
            ("facing", "east"),
            ("waterlogged", "false")
        ]
    );
}
