//! The `NeoForge` registration events and the behaviour by type of a native mod, on the live
//! registries of a real server (`openspec/changes/neoforge-shaped-plugin-api`, spec
//! `neoforge-registration`, requirements "Registration events" and "Behaviour by type"). The
//! server runs `init_mods` as its startup hook and freezes the content registry once per process,
//! so this file is its own test binary. It reuses the server and player support of the
//! `pumpkin-core` tests.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

use ironpumpkin_test_support as support;

use std::{
    sync::{Arc, Mutex, PoisonError, atomic::Ordering},
    time::{Duration, Instant},
};

use ironpumpkin_mods::{
    ModInit, NativeMod,
    content::{BlockBehaviour, BlockBuilder, ContentKind, EntityTypeBuilder, ItemBuilder},
    entity::{
        Entity, EntityBase,
        living::LivingEntity,
        r#type::{check_spawn_rules, from_type},
    },
    entity_attributes::AttributeBuilder,
    math::{position::BlockPos, vector3::Vector3},
    pumpkin_core::{
        block::PathComputationType,
        block::entities::barrel::BarrelBlockEntity,
        content::poi::profession_for_block,
        content::spawn_placements::spawn_restriction,
        entity::ai::brain::{
            behavior::job_site::acquire_poi::SCAN_RANGE, memory::types as memory_types,
        },
        plugin::{level::EntitySpawnReason, startup::STARTUP_HOOK},
    },
    pumpkin_data::{
        Block, BlockState, BlockStateId,
        attributes::Attributes,
        block_properties::{
            BLOCK_ENTITY_TYPES, Half, HorizontalFacing, StairsShape,
            WhiteWoolStairsLikeProperties as StairsProperties,
        },
        entity::EntityType,
        item::Item,
        item_stack::ItemStack,
        poi_type::PoiType,
        tag::{self, Taggable},
        villager::VillagerProfession,
    },
    pumpkin_util::{Hand, HeightMap},
    pumpkin_world::poi::Occupancy,
    pumpkin_world::world::BlockFlags,
    register_mod,
    registration::{
        AttributeSupplier, BlockEntityTypeAddBlocksEvent, EntityAttributeCreationEvent,
        EntityAttributeModificationEvent, ExtendPoiTypesEvent, Operation,
        RegisterSpawnPlacementsEvent, SpawnLocation, spawn_predicate,
    },
    registry,
};
use pumpkin_protocol::{codec::var_int::VarInt, java::server::play::SUseItemOn};

const GOLEM: &str = "regmod:golem";
const MANA: &str = "regmod:mana";
const STAIRS: &str = "regmod:tin_stairs";
const OWN_STAIRS: &str = "regmod:own_stairs";
const CRATE: &str = "regmod:crate";
const DESK: &str = "regmod:desk";
const BUCKET: &str = "regmod:tin_bucket";
/// The x of the spawn positions that the `AND` predicate of the golem rejects.
const REJECTED_X: i32 = 7;
/// The x of the spawn positions that the zombie's replacement predicate rejects.
const ZOMBIE_REJECTED_X: i32 = 3;

static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// The blocks, items and entity types that `registry::mod_entries` listed in `common_setup`.
type Entries = (Vec<String>, Vec<String>, Vec<String>);

static COMMON_SETUP_ENTRIES: Mutex<Option<Entries>> = Mutex::new(None);

fn log(entry: &str) {
    LOG.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(entry.to_owned());
}

fn living(entity: Entity) -> Arc<dyn EntityBase> {
    Arc::new(LivingEntity::new(entity))
}

/// A behaviour of the mod that only marks the block as walkable.
struct Walkable;

impl BlockBehaviour for Walkable {
    fn is_pathfindable(&self, _state: &BlockState, _computation_type: PathComputationType) -> bool {
        true
    }
}

/// Registers a living entity type, a typed stair, a typed stair with its own behaviour, a crate,
/// a desk and a typed bucket, and uses every registration event.
struct RegistrationMod;

impl NativeMod for RegistrationMod {
    fn id(&self) -> &'static str {
        "regmod"
    }

    fn display_name(&self) -> &'static str {
        "Registration mod"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn init(&self, cx: &mut ModInit) {
        log("init");
        cx.register_attribute(AttributeBuilder::ranged(MANA, 20.0, 0.0, 1000.0))
            .unwrap();
        cx.register_entity_type(EntityTypeBuilder::new(GOLEM, &EntityType::ZOMBIE), living)
            .unwrap();
        // Scenario "Stair block": no properties and no behaviour, only the block type.
        cx.register_block(
            BlockBuilder::new(STAIRS, Block::OAK_STAIRS.default_state.id)
                .block_type("minecraft:oak_stairs"),
        )
        .unwrap();
        // Scenario "Own behaviour": the mod's behaviour wins over the block type.
        cx.register_block(
            BlockBuilder::new(OWN_STAIRS, Block::STONE.default_state.id)
                .block_type("minecraft:oak_stairs")
                .behaviour(Arc::new(Walkable)),
        )
        .unwrap();
        cx.register_block(BlockBuilder::new(CRATE, Block::BARREL.default_state.id))
            .unwrap();
        cx.register_block(BlockBuilder::new(DESK, Block::LECTERN.default_state.id))
            .unwrap();
        cx.register_item(ItemBuilder::new(BUCKET, &Item::BUCKET).item_type("minecraft:bucket"))
            .unwrap();
    }

    fn common_setup(&self) {
        log("common_setup");
        let entries = |kind| {
            registry::mod_entries("regmod", kind)
                .iter()
                .map(ToString::to_string)
                .collect()
        };
        *COMMON_SETUP_ENTRIES.lock().unwrap() = Some((
            entries(ContentKind::Block),
            entries(ContentKind::Item),
            entries(ContentKind::EntityType),
        ));
    }

    fn dedicated_server_setup(&self) {
        log("dedicated_server_setup");
    }

    fn load_complete(&self) {
        log("load_complete");
    }

    fn entity_attribute_creation(&self, event: &mut EntityAttributeCreationEvent) {
        log("EntityAttributeCreationEvent");
        // Scenario "Attributes of a mod entity".
        event
            .put(
                GOLEM,
                // A mod attribute goes in by name: it has no id before the freeze.
                AttributeSupplier::of(&EntityType::ZOMBIE)
                    .add(Attributes::MAX_HEALTH, 40.0)
                    .add(MANA, 30.0),
            )
            .unwrap();
    }

    fn entity_attribute_modification(&self, event: &mut EntityAttributeModificationEvent) {
        log("EntityAttributeModificationEvent");
        event
            .add("minecraft:zombie", Attributes::ARMOR, 5.0)
            .unwrap();
        event.add("minecraft:zombie", MANA, 7.0).unwrap();
        assert!(event.has("minecraft:zombie", &Attributes::ARMOR));
        assert!(event.has("minecraft:zombie", MANA));
        assert!(event.has(GOLEM, MANA));
        assert!(event.has(GOLEM, &Attributes::MAX_HEALTH));
    }

    fn register_spawn_placements(&self, event: &mut RegisterSpawnPlacementsEvent) {
        log("RegisterSpawnPlacementsEvent");
        event
            .register_placement(
                GOLEM,
                Some(SpawnLocation::InWater),
                Some(HeightMap::OceanFloor),
                spawn_predicate(|_, _, _| true),
                Operation::Or,
            )
            .unwrap();
        event
            .register(
                GOLEM,
                spawn_predicate(|_, _, pos| pos.0.x != REJECTED_X),
                Operation::And,
            )
            .unwrap();
        event
            .register(
                "minecraft:zombie",
                spawn_predicate(|_, _, pos| pos.0.x != ZOMBIE_REJECTED_X),
                Operation::Replace,
            )
            .unwrap();
    }

    fn block_entity_type_add_blocks(&self, event: &mut BlockEntityTypeAddBlocksEvent) {
        log("BlockEntityTypeAddBlocksEvent");
        event.modify("minecraft:barrel", &[CRATE]).unwrap();
    }

    fn extend_poi_types(&self, event: &mut ExtendPoiTypesEvent) {
        log("ExtendPoiTypesEvent");
        event.add_block_to_poi("minecraft:librarian", DESK).unwrap();
    }
}

register_mod!(RegistrationMod);

fn boot() {
    let _ = STARTUP_HOOK.set(ironpumpkin_mods::init_mods);
    support::server();
}

fn block(name: &str) -> &'static Block {
    Block::from_name(name).unwrap()
}

/// Loads the chunk of `pos` for the rest of the process.
fn load_chunk(pos: BlockPos) {
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

#[test]
fn each_registration_event_fires_once_at_its_neoforge_position() {
    boot();
    assert_eq!(
        *LOG.lock().unwrap(),
        [
            "init",
            "EntityAttributeCreationEvent",
            "EntityAttributeModificationEvent",
            "RegisterSpawnPlacementsEvent",
            "BlockEntityTypeAddBlocksEvent",
            "common_setup",
            "dedicated_server_setup",
            "ExtendPoiTypesEvent",
            "load_complete",
        ]
    );
}

/// `DeferredRegister#getEntries` is a live view: a mod reads its entries in `common_setup`, after
/// the `init` that registered them.
#[test]
fn a_mod_reads_its_entries_in_common_setup() {
    boot();
    let (blocks, items, entity_types) = COMMON_SETUP_ENTRIES.lock().unwrap().clone().unwrap();
    assert_eq!(blocks, [STAIRS, OWN_STAIRS, CRATE, DESK]);
    assert_eq!(items, [BUCKET]);
    assert_eq!(entity_types, [GOLEM]);
    // The lookup stays after the lifecycle.
    assert_eq!(
        registry::mod_entries("regmod", ContentKind::Item)
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        [BUCKET]
    );
    assert_eq!(
        registry::mod_entries("regmod", ContentKind::Attribute)
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        [MANA]
    );
    assert!(registry::mod_entries("unknown", ContentKind::Block).is_empty());
}

/// Scenario "Attributes of a mod entity".
#[test]
fn a_spawned_mod_entity_has_the_attributes_of_the_creation_event() {
    boot();
    let world = support::world();
    let golem = EntityType::from_name(GOLEM).unwrap();
    let entity = from_type(
        golem,
        Vector3::new(0.0, 100.0, 0.0),
        &world,
        uuid::Uuid::new_v4(),
    );
    let living = entity.get_living_entity().unwrap();
    assert_eq!(living.get_attribute_value(&Attributes::MAX_HEALTH), 40.0);
    assert_eq!(living.health.load(), 40.0);
    // The rest of the zombie set stays.
    assert_eq!(
        living.get_attribute_base(&Attributes::SPAWN_REINFORCEMENTS),
        EntityType::ZOMBIE
            .attributes
            .iter()
            .find(|(attribute, _)| *attribute == Attributes::SPAWN_REINFORCEMENTS)
            .unwrap()
            .1
    );
}

/// A mod attribute named in the creation and the modification event binds at the freeze, and the
/// default changes for every entity of the type.
#[test]
fn a_mod_attribute_named_in_a_registration_event_is_a_default_of_every_entity_of_the_type() {
    boot();
    let world = support::world();
    let mana = Attributes::from_name(MANA).unwrap();
    let golem = EntityType::from_name(GOLEM).unwrap();
    for _ in 0..2 {
        let entity = from_type(
            golem,
            Vector3::new(0.0, 100.0, 0.0),
            &world,
            uuid::Uuid::new_v4(),
        );
        assert_eq!(
            entity.get_living_entity().unwrap().get_attribute_base(mana),
            30.0
        );
        let zombie = from_type(
            &EntityType::ZOMBIE,
            Vector3::new(0.0, 100.0, 0.0),
            &world,
            uuid::Uuid::new_v4(),
        );
        assert_eq!(
            zombie.get_living_entity().unwrap().get_attribute_base(mana),
            7.0
        );
    }
    // An entity type that no event names keeps the default of the attribute.
    let skeleton = from_type(
        &EntityType::SKELETON,
        Vector3::new(0.0, 100.0, 0.0),
        &world,
        uuid::Uuid::new_v4(),
    );
    assert_eq!(
        skeleton
            .get_living_entity()
            .unwrap()
            .get_attribute_base(mana),
        20.0
    );
}

#[test]
fn the_modification_event_adds_an_attribute_to_a_vanilla_entity() {
    boot();
    let world = support::world();
    let zombie = from_type(
        &EntityType::ZOMBIE,
        Vector3::new(0.0, 100.0, 0.0),
        &world,
        uuid::Uuid::new_v4(),
    );
    let living = zombie.get_living_entity().unwrap();
    assert_eq!(living.get_attribute_base(&Attributes::ARMOR), 5.0);
    assert_eq!(living.get_attribute_value(&Attributes::MAX_HEALTH), 20.0);
}

#[test]
fn a_spawn_placement_rejects_a_position() {
    boot();
    let world = support::world();
    let golem = EntityType::from_name(GOLEM).unwrap();
    let at = |x| BlockPos::new(x, 64, 0);
    // The original predicate passes and the `AND` predicate rejects one x.
    let natural = EntitySpawnReason::Natural;
    assert!(check_spawn_rules(
        golem,
        &world,
        natural,
        &at(REJECTED_X + 1),
        false
    ));
    assert!(!check_spawn_rules(
        golem,
        &world,
        natural,
        &at(REJECTED_X),
        false
    ));
    let restriction = spawn_restriction(golem);
    assert!(matches!(restriction.location, SpawnLocation::InWater));
    assert_eq!(restriction.heightmap, HeightMap::OceanFloor);
    // The replacement wins over the zombie's darkness rule and keeps its placement type.
    assert!(check_spawn_rules(
        &EntityType::ZOMBIE,
        &world,
        natural,
        &at(ZOMBIE_REJECTED_X + 1),
        false
    ));
    assert!(!check_spawn_rules(
        &EntityType::ZOMBIE,
        &world,
        natural,
        &at(ZOMBIE_REJECTED_X),
        false
    ));
    assert!(matches!(
        spawn_restriction(&EntityType::ZOMBIE).location,
        SpawnLocation::OnGround
    ));
}

#[test]
fn a_block_entity_type_accepts_the_mod_block() {
    boot();
    let barrel = BLOCK_ENTITY_TYPES
        .iter()
        .position(|ty| *ty == "barrel")
        .unwrap();
    let crate_block = block(CRATE);
    assert!(
        crate_block
            .states
            .iter()
            .all(|state| usize::from(state.block_entity_type) == barrel)
    );
    let pos = BlockPos::new(1605, 200, 10);
    load_chunk(pos);
    let world = support::world();
    world.set_block_state(&pos, crate_block.default_state.id, BlockFlags::NOTIFY_ALL);
    let entity = world.get_block_entity(&pos).unwrap();
    assert_eq!(entity.resource_location(), "minecraft:barrel");
}

/// The block entity of a mod block that names a vanilla type goes with the block: it is removed
/// on break, and its saved NBT loads again with its contents.
#[test]
fn a_mod_block_entity_is_removed_on_break_and_survives_a_save_and_load() {
    boot();
    let crate_block = block(CRATE);
    let pos = BlockPos::new(1607, 200, 10);
    load_chunk(pos);
    let world = support::world();
    world.set_block_state(&pos, crate_block.default_state.id, BlockFlags::NOTIFY_ALL);
    let entity = world.get_block_entity(&pos).unwrap();
    let barrel = entity.as_any().downcast_ref::<BarrelBlockEntity>().unwrap();
    barrel.items.write().unwrap()[3] = ItemStack::new(5, &Item::DIAMOND);
    let saved = world.get_block_entity_nbt(&pos).unwrap();
    assert_eq!(saved.get_string("id"), Some("minecraft:barrel"));

    // A break removes the block entity.
    world.set_block_state(&pos, Block::AIR.default_state.id, BlockFlags::NOTIFY_ALL);
    assert!(world.get_block_entity(&pos).is_none());
    assert!(world.get_block_entity_nbt(&pos).is_none());

    // The block placed again has an empty barrel; the saved NBT loads in its place.
    world.set_block_state(&pos, crate_block.default_state.id, BlockFlags::NOTIFY_ALL);
    world.remove_block_entity(&pos);
    world.add_block_entity_nbt(pos, &saved);
    let loaded = world.get_block_entity(&pos).unwrap();
    assert_eq!(loaded.resource_location(), "minecraft:barrel");
    let barrel = loaded.as_any().downcast_ref::<BarrelBlockEntity>().unwrap();
    let items = barrel.items.read().unwrap();
    assert_eq!(items[3].item.id, Item::DIAMOND.id);
    assert_eq!(items[3].item_count, 5);
}

/// A villager takes the job site POI of a mod block as one of the vanilla block: its brain runs
/// `AcquirePoi` until it paths to the desk and claims it, and no other villager finds it then.
#[test]
fn a_villager_acquires_the_mod_poi() {
    boot();
    let desk = block(DESK);
    let world = support::world();
    // A floor of stone, so the villager walks to the desk.
    let (x0, y, z0) = (1620, 200, 20);
    load_chunk(BlockPos::new(x0, y, z0));
    for x in x0 - 1..=x0 + 6 {
        for z in z0 - 1..=z0 + 4 {
            world.set_block_state(
                &BlockPos::new(x, y - 1, z),
                Block::STONE.default_state.id,
                BlockFlags::empty(),
            );
            for dy in 0..3 {
                world.set_block_state(
                    &BlockPos::new(x, y + dy, z),
                    Block::AIR.default_state.id,
                    BlockFlags::empty(),
                );
            }
        }
    }
    let desk_pos = BlockPos::new(x0 + 4, y, z0 + 1);
    world.set_block_state(&desk_pos, desk.default_state.id, BlockFlags::NOTIFY_ALL);
    let free_desks = || {
        world
            .portal_poi
            .lock()
            .unwrap()
            .find_all_closest_first_with_type(
                &|poi| poi == PoiType::Librarian,
                &|_| true,
                desk_pos,
                SCAN_RANGE,
                Occupancy::HasSpace,
            )
    };
    assert_eq!(free_desks(), [(PoiType::Librarian, desk_pos)]);

    let villager = from_type(
        &EntityType::VILLAGER,
        Vector3::new(f64::from(x0) + 0.5, f64::from(y), f64::from(z0) + 1.5),
        &world,
        uuid::Uuid::new_v4(),
    );
    // A walking mob paths only from the ground, and a mob that no tick settled is not on it yet.
    villager
        .get_entity()
        .on_ground
        .store(true, Ordering::Relaxed);
    let mob = villager.get_mob().unwrap();
    let mob_entity = mob.get_mob_entity();
    let potential_job_site = memory_types::POTENTIAL_JOB_SITE.id();
    // `AcquirePoi` first waits up to 20 ticks, then scans every 20 to 39 ticks.
    for time in 1..=100 {
        mob_entity.with_brain(mob, |brain| brain.tick(&world, mob, time));
        if mob_entity.brain_has_memory_value(potential_job_site) {
            break;
        }
    }
    assert!(mob_entity.brain_has_memory_value(potential_job_site));
    // The claim took the only ticket of the desk.
    assert!(free_desks().is_empty());
}

#[test]
fn the_poi_storage_records_the_mod_poi() {
    boot();
    let desk = block(DESK);
    assert_eq!(
        profession_for_block(desk),
        Some(VillagerProfession::Librarian)
    );
    let pos = BlockPos::new(1606, 200, 12);
    load_chunk(pos);
    let world = support::world();
    world.set_block_state(&pos, desk.default_state.id, BlockFlags::NOTIFY_ALL);
    assert_eq!(
        world.portal_poi.lock().unwrap().get_type(&pos),
        Some(PoiType::Librarian)
    );
}

/// Places `placed` as a player facing north, clicking the top face of the block below `pos`.
fn place_facing_north(placed: &'static Block, pos: BlockPos) -> BlockStateId {
    let world = support::world();
    let below = pos.down();
    world.set_block_state(&below, Block::STONE.default_state.id, BlockFlags::empty());
    let player = support::player(&format!("Placer{}", pos.0.x));
    player
        .living_entity
        .entity
        .set_pos(Vector3::new(1610.5, 200.0, 8.5));
    player.get_entity().set_rotation(180.0, 0.0);
    let use_item_on = SUseItemOn {
        hand: VarInt(0),
        position: below,
        face: VarInt(1),
        cursor_pos: Vector3::new(0.5, 1.0, 0.5),
        inside_block: false,
        is_against_world_border: false,
        sequence: VarInt(0),
    };
    let server = support::server();
    let (placed_at, state) = server
        .block_registry
        .place_block(
            &player,
            placed,
            server,
            &use_item_on,
            below,
            ironpumpkin_mods::pumpkin_data::BlockDirection::Up,
        )
        .unwrap()
        .unwrap();
    assert_eq!(placed_at, pos);
    assert_eq!(world.get_block_state_id(&pos), state);
    state
}

/// A bottom stair facing east at `pos`.
fn stair_facing_east(block: &Block) -> BlockStateId {
    let mut props = StairsProperties::default(block);
    props.facing = HorizontalFacing::East;
    props.half = Half::Bottom;
    props.to_state_id(block)
}

/// Scenario "Stair block": a mod stair placed by a player takes the stair shape and connects
/// like a vanilla stair, in both directions.
#[test]
fn a_mod_stair_places_and_connects_like_a_vanilla_stair() {
    boot();
    let stairs = block(STAIRS);
    // The block took the properties and the tags of its type.
    assert_eq!(stairs.states.len(), Block::OAK_STAIRS.states.len());
    assert!(stairs.has_tag(&tag::Block::MINECRAFT_STAIRS));
    let world = support::world();
    load_chunk(BlockPos::new(1601, 200, 3));

    // A mod stair in front of the placed mod stair, facing its clockwise side.
    world.set_block_state(
        &BlockPos::new(1601, 200, 2),
        stair_facing_east(stairs),
        BlockFlags::empty(),
    );
    let placed = place_facing_north(stairs, BlockPos::new(1601, 200, 3));
    let props = StairsProperties::from_state_id(placed);
    assert_eq!(props.facing, HorizontalFacing::North);
    assert_eq!(props.half, Half::Bottom);
    assert_eq!(props.shape, StairsShape::OuterRight);
    // Each state copies the physical data of the matching oak stair state.
    let oak_state = props.to_state_id(&Block::OAK_STAIRS);
    assert_eq!(
        BlockState::from_id(placed).collision_shapes,
        BlockState::from_id(oak_state).collision_shapes
    );
    // A vanilla client sees the matching oak stair state.
    assert_eq!(placed.display_state(), oak_state);

    // A vanilla stair connects to a mod stair in front of it.
    world.set_block_state(
        &BlockPos::new(1604, 200, 2),
        stair_facing_east(stairs),
        BlockFlags::empty(),
    );
    let vanilla = place_facing_north(&Block::OAK_STAIRS, BlockPos::new(1604, 200, 3));
    assert_eq!(
        StairsProperties::from_state_id(vanilla).shape,
        StairsShape::OuterRight
    );
}

/// Scenario "Own behaviour": the behaviour that the mod sets wins over its block type.
#[test]
fn the_own_behaviour_of_a_typed_block_wins_over_its_type() {
    boot();
    let blocks = &support::server().block_registry;
    let own = block(OWN_STAIRS);
    assert!(blocks.is_pathfindable(own, own.default_state, PathComputationType::Land));
    let typed = block(STAIRS);
    assert!(!blocks.is_pathfindable(typed, typed.default_state, PathComputationType::Land));
}

/// A mod item that names the `minecraft:bucket` behaviour picks up water.
#[test]
fn a_mod_bucket_picks_up_water() {
    boot();
    let water = BlockPos::new(1612, 200, 12);
    load_chunk(water);
    let world = support::world();
    world.set_block_state(&water, Block::WATER.default_state.id, BlockFlags::empty());
    let player = support::player("Fetcher");
    player
        .living_entity
        .entity
        .set_pos(Vector3::new(1612.5, 201.0, 12.5));
    player.get_entity().set_rotation(0.0, 90.0);
    let bucket = Item::from_registry_key(BUCKET).unwrap();
    player.inventory.set_held_item(ItemStack::new(1, bucket));
    support::server()
        .item_registry
        .on_use(&ItemStack::new(1, bucket), &player, Hand::Right);
    assert_eq!(
        world.get_block_state_id(&water),
        Block::AIR.default_state.id
    );
    assert_eq!(player.inventory.held_item().item.id, Item::WATER_BUCKET.id);
}
