//! The loot context parameters that the call sites of vanilla 26.3 set, seen from a global loot
//! modifier on a real server: a capture modifier records the parameters of every roll, and each
//! test runs one call site (hoe, brush, decorated pot, vault, trial spawner) and checks what the
//! roll got. A second modifier tests an `entity_properties` condition on `interacting_entity`.
//! The modifier types freeze once per process, so this file is its own test binary.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

#[path = "../../pumpkin-core/tests/support/mod.rs"]
mod support;

use std::{
    sync::{Arc, LazyLock, Mutex, PoisonError},
    time::{Duration, Instant},
};

use ironpumpkin_mods::{
    ModInit, NativeMod,
    loot::{GlobalLootModifier, ItemStack, LootContext, Value},
    pumpkin_core::{
        block::{
            BlockBehaviour, BlockHitResult, UseWithItemArgs,
            blocks::vault::VaultBlock,
            collect_loot,
            entities::{
                BlockEntity,
                brushable_block::BrushableBlockBlockEntity,
                decorated_pot::DecoratedPotBlockEntity,
                trial_spawner::{
                    SpawnData, TrialSpawnerBlockEntity, TrialSpawnerFullConfig, WeightedSpawnData,
                },
                vault::VaultBlockEntity,
            },
            registry::BlockActionResult,
        },
        entity::EntityBase,
        item::{
            ItemBehaviour,
            items::{brush::BrushItem, hoe::HoeItem},
        },
        plugin::startup::STARTUP_HOOK,
        world::{
            loot::{LootContextParameters, RollRandom, player_luck},
            random_sequences::RandomSequences,
        },
    },
    pumpkin_data::{
        Block, BlockDirection, BlockStateId,
        block_properties::{SuspiciousSandLikeProperties, VaultLikeProperties, VaultState},
        entity::EntityType,
        item::Item,
    },
    pumpkin_util::{
        math::{position::BlockPos, vector3::Vector3},
        random::RandomImpl,
    },
    register_mod,
};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_world::world::BlockFlags;

/// What a roll got, in values that compare.
#[derive(Debug, Clone, PartialEq)]
struct Roll {
    table: String,
    origin: Option<[f64; 3]>,
    block_state: Option<BlockStateId>,
    block_entity: Option<BlockPos>,
    tool: Option<&'static str>,
    luck: f32,
    this_entity: Option<i32>,
    interacting_entity: Option<i32>,
}

static ROLLS: Mutex<Vec<Roll>> = Mutex::new(Vec::new());
/// The tables that the `entity_properties` marker modifier passed for.
static MARKED: Mutex<Vec<String>> = Mutex::new(Vec::new());

struct Capture;

impl GlobalLootModifier for Capture {
    fn apply(&self, loot: Vec<ItemStack>, context: &mut LootContext<'_, '_>) -> Vec<ItemStack> {
        let params = context.params();
        let entity_id = |entity: Option<&dyn EntityBase>| entity.map(|e| e.get_entity().entity_id);
        let roll = Roll {
            table: context.queried_loot_table_id().to_owned(),
            origin: params.position.map(|p| [p.x, p.y, p.z]),
            block_state: params.block_state.map(|state| state.id),
            block_entity: params.block_entity.map(BlockEntity::get_position),
            tool: params.tool.as_ref().map(|tool| tool.item.registry_key),
            luck: params.luck,
            this_entity: entity_id(params.this_entity),
            interacting_entity: entity_id(params.interacting_entity),
        };
        ROLLS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(roll);
        loot
    }
}

struct Mark;

impl GlobalLootModifier for Mark {
    fn apply(&self, loot: Vec<ItemStack>, context: &mut LootContext<'_, '_>) -> Vec<ItemStack> {
        MARKED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(context.queried_loot_table_id().to_owned());
        loot
    }
}

struct ContextMod;

impl NativeMod for ContextMod {
    fn id(&self) -> &'static str {
        "ctxmod"
    }

    fn display_name(&self) -> &'static str {
        "Context mod"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn init(&self, cx: &mut ModInit) {
        cx.register_loot_modifier_type("ctxmod:capture", |_: &Value| Ok(Box::new(Capture) as _))
            .unwrap();
        cx.register_loot_modifier_type("ctxmod:mark", |_: &Value| Ok(Box::new(Mark) as _))
            .unwrap();
    }
}

register_mod!(ContextMod);

/// Starts the mod, writes the data pack with the two modifiers and reloads.
static SETUP: LazyLock<()> = LazyLock::new(|| {
    STARTUP_HOOK.set(ironpumpkin_mods::init_mods).unwrap();
    let server = support::server();
    let pack = server.basic_config.get_world_path().join("datapacks/ctx");
    let modifiers = pack.join("data/ctxmod/loot_modifiers");
    std::fs::create_dir_all(&modifiers).unwrap();
    std::fs::write(
        modifiers.join("capture.json"),
        r#"{"type": "ctxmod:capture"}"#,
    )
    .unwrap();
    std::fs::write(
        modifiers.join("mark.json"),
        r#"{"type": "ctxmod:mark", "condition": {"condition": "minecraft:entity_properties",
            "entity": "interacting_entity", "predicate": {"type": "minecraft:player"}}}"#,
    )
    .unwrap();
    std::fs::write(
        pack.join("pack.mcmeta"),
        r#"{"pack": {"pack_format": 94, "description": "loot context"}}"#,
    )
    .unwrap();
    let mut level = (**server.level_info.load()).clone();
    level.data_packs.enabled.push("file/ctx".to_owned());
    server.level_info.store(Arc::new(level));
    server.reload_datapacks(server);
    assert_eq!(server.datapack_manager.get_loot_modifiers().len(), 2);
});

/// Runs `action` and returns the rolls of the table `table` that it caused.
fn rolls_of(table: &str, action: impl FnOnce()) -> Vec<Roll> {
    LazyLock::force(&SETUP);
    ROLLS.lock().unwrap_or_else(PoisonError::into_inner).clear();
    action();
    ROLLS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter(|roll| roll.table == table)
        .cloned()
        .collect()
}

/// Loads the chunk of `pos` for the rest of the process, then sets the block.
fn set(pos: BlockPos, state: BlockStateId) {
    let world = support::world();
    let chunk = pos.chunk_position();
    let mut loading = world.level.chunk_loading.lock().unwrap();
    loading.add_force_ticket(chunk);
    loading.send_change();
    drop(loading);
    let deadline = Instant::now() + support::SLOW_RUNNER;
    while !world.level.is_chunk_loaded(&chunk) {
        assert!(Instant::now() < deadline, "the chunk of {pos} did not load");
        std::thread::sleep(Duration::from_millis(10));
    }
    world.set_block_state(&pos, state, BlockFlags::NOTIFY_ALL);
}

fn center(pos: &BlockPos) -> [f64; 3] {
    [
        f64::from(pos.0.x) + 0.5,
        f64::from(pos.0.y) + 0.5,
        f64::from(pos.0.z) + 0.5,
    ]
}

fn entity_id(entity: &dyn EntityBase) -> i32 {
    entity.get_entity().entity_id
}

#[test]
fn hoe_rolls_the_block_interact_set() {
    let _serial = support::serial();
    LazyLock::force(&SETUP);
    let server = support::server();
    let player = support::player("hoe_user");
    let pos = BlockPos::new(10, 100, 0);
    let old_state = Block::ROOTED_DIRT.default_state;
    set(pos, old_state.id);
    MARKED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();

    let mut hoe = ItemStack::new(1, &Item::DIAMOND_HOE);
    let mut result = None;
    let rolls = rolls_of("minecraft:till/rooted_dirt", || {
        result = Some(HoeItem.use_on_block(
            &mut hoe,
            &player,
            pos,
            BlockDirection::Up,
            Vector3::new(0.5, 1.0, 0.5),
            &Block::ROOTED_DIRT,
            server,
        ));
    });
    assert!(matches!(result, Some(BlockActionResult::Success)));
    assert_eq!(
        rolls,
        [Roll {
            table: "minecraft:till/rooted_dirt".to_owned(),
            origin: Some(center(&pos)),
            // The state from before the transformation.
            block_state: Some(old_state.id),
            block_entity: None,
            tool: Some("diamond_hoe"),
            luck: 0.0,
            this_entity: None,
            interacting_entity: Some(entity_id(player.as_ref())),
        }]
    );
    // The `entity_properties` condition on `interacting_entity` passes for the player.
    assert!(
        MARKED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(&"minecraft:till/rooted_dirt".to_owned())
    );
}

#[test]
fn brush_rolls_the_archaeology_set() {
    let _serial = support::serial();
    LazyLock::force(&SETUP);
    let server = support::server();
    let world = support::world();
    let player = support::player("brush_user");
    let pos = BlockPos::new(20, 100, 0);
    let mut props = SuspiciousSandLikeProperties::default(&Block::SUSPICIOUS_SAND);
    props.dusted = 3;
    set(pos, props.to_state_id(&Block::SUSPICIOUS_SAND));
    let entity = BrushableBlockBlockEntity::from_nbt(&NbtCompound::new(), pos);
    *entity.loot_table.lock().unwrap() = Some("minecraft:archaeology/desert_pyramid".to_owned());
    *entity.loot_table_seed.lock().unwrap() = 7;
    world.add_block_entity(Arc::new(entity));

    let mut brush = ItemStack::new(1, &Item::BRUSH);
    let rolls = rolls_of("minecraft:archaeology/desert_pyramid", || {
        BrushItem.use_on_block(
            &mut brush,
            &player,
            pos,
            BlockDirection::Up,
            Vector3::new(0.5, 1.0, 0.5),
            &Block::SUSPICIOUS_SAND,
            server,
        );
    });
    assert_eq!(
        rolls,
        [Roll {
            table: "minecraft:archaeology/desert_pyramid".to_owned(),
            origin: Some(center(&pos)),
            block_state: None,
            block_entity: None,
            tool: Some("brush"),
            luck: player_luck(&player),
            this_entity: Some(entity_id(player.as_ref())),
            interacting_entity: None,
        }]
    );
}

#[test]
fn decorated_pot_rolls_from_the_center_without_a_player() {
    let _serial = support::serial();
    LazyLock::force(&SETUP);
    let world = support::world();
    let pos = BlockPos::new(30, 100, 0);
    let pot = Arc::new(DecoratedPotBlockEntity::new(pos));
    *pot.loot_table.lock().unwrap() = Some("minecraft:chests/simple_dungeon".to_owned());

    let rolls = rolls_of("minecraft:chests/simple_dungeon", || {
        DecoratedPotBlockEntity::unpack_loot_table(&pot, &world);
    });
    // `DecoratedPotBlockEntity` calls `unpackLootTable(null)`: no player, no luck.
    assert_eq!(
        rolls,
        [Roll {
            table: "minecraft:chests/simple_dungeon".to_owned(),
            origin: Some(center(&pos)),
            block_state: None,
            block_entity: None,
            tool: None,
            luck: 0.0,
            this_entity: None,
            interacting_entity: None,
        }]
    );
}

#[test]
fn vault_rolls_with_the_key_as_the_tool() {
    let _serial = support::serial();
    LazyLock::force(&SETUP);
    let server = support::server();
    let world = support::world();
    let player = support::player("vault_user");
    let pos = BlockPos::new(40, 100, 0);
    let mut props = VaultLikeProperties::default(&Block::VAULT);
    props.vault_state = VaultState::Active;
    set(pos, props.to_state_id(&Block::VAULT));
    world.add_block_entity(Arc::new(VaultBlockEntity::new(pos)));

    let mut key = ItemStack::new(1, &Item::TRIAL_KEY);
    let rolls = rolls_of("minecraft:chests/trial_chambers/reward", || {
        VaultBlock.use_with_item(UseWithItemArgs {
            server,
            world: &world,
            block: &Block::VAULT,
            position: &pos,
            player: &player,
            hit: &BlockHitResult {
                face: &BlockDirection::Up,
                cursor_pos: &Vector3::new(0.5, 1.0, 0.5),
            },
            item_stack: &mut key,
            equipment_slot:
                &ironpumpkin_mods::pumpkin_data::data_component_impl::EquipmentSlot::MAIN_HAND,
        });
    });
    assert_eq!(
        rolls,
        [Roll {
            table: "minecraft:chests/trial_chambers/reward".to_owned(),
            origin: Some(center(&pos)),
            block_state: None,
            block_entity: None,
            // The key as it was before the vault consumed it.
            tool: Some("trial_key"),
            luck: player_luck(&player),
            this_entity: Some(entity_id(player.as_ref())),
            interacting_entity: None,
        }]
    );
}

#[test]
fn trial_spawner_rolls_the_equipment_set_for_the_spawned_mob() {
    let _serial = support::serial();
    LazyLock::force(&SETUP);
    let world = support::world();
    let pos = BlockPos::new(50, 100, 0);
    let mut equipment = NbtCompound::new();
    equipment.put_string(
        "loot_table",
        "minecraft:equipment/trial_chamber_melee".to_owned(),
    );
    let mut config = TrialSpawnerFullConfig::default();
    config.normal.spawn_potentials = vec![WeightedSpawnData {
        data: SpawnData {
            equipment: Some(equipment),
            ..SpawnData::from_entity_type(&EntityType::ZOMBIE)
        },
        weight: 1,
    }];
    let spawner = TrialSpawnerBlockEntity::with_config(pos, config);

    let mut spawned = None;
    let rolls = rolls_of("minecraft:equipment/trial_chamber_melee", || {
        spawned = spawner.trial_spawner.lock().unwrap().spawn_mob(&world, pos);
    });
    let mob = spawned.unwrap();
    assert_eq!(rolls.len(), 1);
    let roll = &rolls[0];
    assert_eq!(
        roll.this_entity,
        world.get_entity_by_uuid(mob).map(|e| entity_id(e.as_ref()))
    );
    let origin = roll.origin.unwrap();
    // `Mob.createEquipmentParams`: the position of the mob, in the spawn range of the spawner.
    assert!((origin[0] - f64::from(pos.0.x)).abs() <= 5.0);
    assert!(roll.tool.is_none() && roll.block_state.is_none());
}

const DUNGEON: &str = "minecraft:chests/simple_dungeon";

/// The first `count` values of the sequence `DUNGEON` of a fresh manager. The seeds of the
/// sequences are the ones of Pumpkin, not of vanilla (issue 193), so the tests compare Pumpkin
/// with itself.
fn dungeon_stream(count: usize) -> Vec<i64> {
    let id = DUNGEON.try_into().unwrap();
    let world_seed = support::server().level_info.load().world_gen_settings.seed;
    let mut sequences = RandomSequences::new();
    let mut sequence = sequences
        .get_or_create(&id, world_seed)
        .handle()
        .check_out();
    (0..count).map(|_| sequence.rng().next_i64()).collect()
}

fn reset_dungeon_sequence() {
    let server = support::server();
    let world_seed = server.level_info.load().world_gen_settings.seed;
    server
        .random_sequences
        .lock()
        .unwrap()
        .reset(&DUNGEON.try_into().unwrap(), world_seed);
}

#[test]
fn a_roll_with_seed_zero_goes_on_with_the_random_sequence_of_the_table() {
    let _serial = support::serial();
    LazyLock::force(&SETUP);
    let world = support::world();
    let params = LootContextParameters {
        world: Some(&world),
        ..Default::default()
    };
    let table = world.get_loot_table(DUNGEON).unwrap();
    reset_dungeon_sequence();
    let expected = dungeon_stream(3);

    // Two rolls go on with one stream.
    let first = {
        let mut random = RollRandom::new(&params, &table, 0);
        [random.rng().next_i64(), random.rng().next_i64()]
    };
    let second = RollRandom::new(&params, &table, 0).rng().next_i64();
    assert_eq!([first[0], first[1], second], expected[..]);

    // A seed other than 0 is a source of its own and leaves the sequence alone.
    let seeded = RollRandom::new(&params, &table, 5).rng().next_i64();
    assert_eq!(seeded, RollRandom::seeded(5).rng().next_i64());
    reset_dungeon_sequence();
    assert_eq!(
        RollRandom::new(&params, &table, 0).rng().next_i64(),
        expected[0]
    );

    // A table with no sequence, or a roll with no world, gets the level random: two sources differ.
    let params_without_world = LootContextParameters::default();
    let a = RollRandom::new(&params_without_world, &table, 0)
        .rng()
        .next_i64();
    let b = RollRandom::new(&params_without_world, &table, 0)
        .rng()
        .next_i64();
    assert_ne!(a, b);
}

/// Review 171: a roll copied the state of the sequence and wrote it back, so two overlapping rolls
/// drew the same values.
#[test]
fn overlapping_rolls_of_one_sequence_draw_different_values() {
    let _serial = support::serial();
    LazyLock::force(&SETUP);
    let world = support::world();
    reset_dungeon_sequence();
    let expected = dungeon_stream(2);

    let params = LootContextParameters {
        world: Some(&world),
        ..Default::default()
    };
    let table = world.get_loot_table(DUNGEON).unwrap();
    let mut first = RollRandom::new(&params, &table, 0);
    let first_value = first.rng().next_i64();

    let (sender, receiver) = std::sync::mpsc::channel();
    let second_world = world.clone();
    let second_table = table.clone();
    let thread = std::thread::spawn(move || {
        let params = LootContextParameters {
            world: Some(&second_world),
            ..Default::default()
        };
        let mut second = RollRandom::new(&params, &second_table, 0);
        sender.send(second.rng().next_i64()).unwrap();
    });
    // The second roll waits for the first one.
    assert!(receiver.recv_timeout(Duration::from_millis(300)).is_err());
    drop(first);
    let second_value = receiver.recv_timeout(support::SLOW_RUNNER).unwrap();
    thread.join().unwrap();
    assert_eq!([first_value, second_value], expected[..]);
}

/// Review 171: the write back of a roll overwrote a `/random reset` made during the roll.
#[test]
fn a_reset_during_a_roll_is_not_overwritten() {
    let _serial = support::serial();
    LazyLock::force(&SETUP);
    let world = support::world();
    reset_dungeon_sequence();
    let expected = dungeon_stream(1);

    let params = LootContextParameters {
        world: Some(&world),
        ..Default::default()
    };
    let table = world.get_loot_table(DUNGEON).unwrap();
    let mut roll = RollRandom::new(&params, &table, 0);
    roll.rng().next_i64();
    // The reset does not wait for the roll.
    reset_dungeon_sequence();
    drop(roll);
    assert_eq!(
        RollRandom::new(&params, &table, 0).rng().next_i64(),
        expected[0]
    );
}

#[test]
fn a_block_break_roll_has_the_block_entity() {
    let _serial = support::serial();
    LazyLock::force(&SETUP);
    let world = support::world();
    let pos = BlockPos::new(60, 100, 0);
    set(pos, Block::DECORATED_POT.default_state.id);
    world.add_block_entity(Arc::new(DecoratedPotBlockEntity::new(pos)));

    let params = LootContextParameters {
        world: Some(&world),
        ..Default::default()
    };
    let rolls = rolls_of("minecraft:blocks/decorated_pot", || {
        collect_loot(&world, &Block::DECORATED_POT, &pos, &params);
    });
    assert_eq!(rolls.len(), 1);
    assert_eq!(rolls[0].block_entity, Some(pos));
}
