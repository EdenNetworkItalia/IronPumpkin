//! The `NeoForge` player events (`event.entity.player`, task 2.12 of
//! `openspec/changes/neoforge-shaped-plugin-api`) as Pumpkin events: one test per row that
//! fires from its `NeoForge` fire site, the cancel and write-back tests of the player family and
//! the concrete parent `PlayerContainerEvent`, against a server of the support module.
//!
//! Each test filters its handlers by the names of its own players, because the handlers stay
//! registered on the shared server.
#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test fails on the first missing value"
)]

mod support;

use std::{
    marker::PhantomData,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

use pumpkin_core::{
    entity::{
        Entity, EntityBase, experience_orb::ExperienceOrbEntity, item::ItemEntity,
        passive::wandering_trader::WanderingTraderEntity, player::Player,
    },
    item::{ItemBehaviour, items::bow::BowItem},
    net::ClientPlatform,
    plugin::{
        BoxFuture, Cancellable, EventHandler, EventPriority, ParentEventHandler, Payload,
        block::block_damage::BlockDamageEvent,
        inventory::inventory_open::InventoryOpenEvent,
        player::{
            AnvilCraftEventPost, AnvilCraftEventPre, ArrowLooseEvent, ArrowNockEvent,
            ClientInformationUpdatedEvent, CriticalHitEvent, InventoryCloseEvent,
            ItemEntityPickupEventPost, ItemEntityPickupEventPre, PlayerContainerEvent,
            PlayerEventBreakSpeed, PlayerEventHarvestCheck, PlayerEventLoadFromFile,
            PlayerEventNameFormat, PlayerEventSaveToFile, PlayerEventStartTracking,
            PlayerEventStopTracking, PlayerEventTabListNameFormat, PlayerFlyableFallEvent,
            PlayerSwitchHotbarSlotEventPost, PlayerXpEventPickupXp, SweepAttackEvent,
            TradeWithVillagerEvent,
            item_held::PlayerItemHeldEvent,
            player_interact_event::{InteractAction, PlayerInteractEvent},
            player_spawn_location::PlayerSpawnLocationEvent,
        },
        results::TriState,
    },
    server::Server,
    world::entity_tracker::TrackedEntity,
};
use pumpkin_data::{Block, entity::EntityType, item::Item, item_stack::ItemStack};
use pumpkin_inventory::{
    SimpleInventory,
    anvil::AnvilScreenHandler,
    inventory::Inventory,
    merchant::merchant_screen_handler::MerchantScreenHandler,
    screen_handler::{InventoryPlayer, ScreenHandlerFactory, SharedScreenHandler},
};
use pumpkin_protocol::{
    codec::{
        item_stack_seralizer::{ItemStackSerializer, OptionalItemStackHash},
        var_int::VarInt,
    },
    java::{
        client::play::MerchantOffer,
        server::play::{
            SClickSlot, SClientInformationPlay, SPlayerAction, SSetHeldItem, SlotActionType, Status,
        },
    },
};
use pumpkin_util::{
    GameMode,
    math::{position::BlockPos, vector3::Vector3},
    text::TextComponent,
};

type Log = Arc<Mutex<Vec<String>>>;

fn log() -> Log {
    Arc::new(Mutex::new(Vec::new()))
}

fn push(log: &Log, entry: impl Into<String>) {
    log.lock().unwrap().push(entry.into());
}

fn entries(log: &Log) -> Vec<String> {
    log.lock().unwrap().clone()
}

/// A blocking handler that runs a closure on the event.
struct Blocking<E, F>(F, PhantomData<fn() -> E>);

impl<E, F> EventHandler<E> for Blocking<E, F>
where
    E: Payload + Send + Sync + 'static,
    F: Fn(&mut E) + Send + Sync,
{
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut E,
    ) -> BoxFuture<'a, ()> {
        (self.0)(event);
        Box::pin(async {})
    }
}

/// Registers a blocking handler for `E` with priority `Normal` and without receive-canceled.
fn on<E, F>(handler: F)
where
    E: Payload + Send + Sync + 'static,
    F: Fn(&mut E) + Send + Sync + 'static,
{
    on_with::<E, F>(EventPriority::Normal, false, handler);
}

fn on_with<E, F>(priority: EventPriority, receive_canceled: bool, handler: F)
where
    E: Payload + Send + Sync + 'static,
    F: Fn(&mut E) + Send + Sync + 'static,
{
    support::server().plugin_manager.register_with::<E, _>(
        Arc::new(Blocking(handler, PhantomData)),
        priority,
        true,
        receive_canceled,
    );
}

fn named(player: &Player, name: &str) -> bool {
    player.gameprofile.name == name
}

/// Moves `player` to its own spot, away from the players of the other tests.
fn place(player: &Player, x: f64) {
    player
        .living_entity
        .entity
        .set_pos(Vector3::new(x, 100.0, 0.0));
}

/// Runs `action` on another thread and waits for it: a handler that deadlocks fails the test
/// instead of hanging it.
fn finish_within(player: &Arc<Player>, action: impl FnOnce(&Arc<Player>) + Send + 'static) {
    let (done, finished) = mpsc::channel();
    let player = player.clone();
    std::thread::spawn(move || {
        action(&player);
        let _ = done.send(());
    });
    finished
        .recv_timeout(Duration::from_secs(30))
        .unwrap_or_else(|_| panic!("the action did not finish: a handler deadlocked or panicked"));
}

/// Left-clicks `slot` of the open screen with the sync id `sync_id`.
fn click(player: &Arc<Player>, sync_id: i32, slot: i16) {
    let packet = SClickSlot {
        sync_id: VarInt(sync_id),
        revision: VarInt(0),
        slot,
        button: SClickSlot::BUTTON_LEFT,
        mode: SlotActionType::Pickup,
        length_of_array: VarInt(0),
        array_of_changed_slots: Vec::new(),
        carried_item: OptionalItemStackHash(None),
    };
    finish_within(player, move |player| {
        player.on_slot_click(packet, support::server());
    });
}

fn java_client(player: &Player) -> &pumpkin_core::net::java::JavaClient {
    let ClientPlatform::Java(client) = player.client.as_ref() else {
        panic!("the support players are Java players");
    };
    client
}

fn health(player: &Player) -> f32 {
    player.living_entity.health.load()
}

/// Lets `attacker` attack `victim` with a full attack cooldown.
fn attack(attacker: &Arc<Player>, victim: &Arc<Player>) {
    attacker.last_attacked_ticks.store(100, Ordering::Relaxed);
    let victim: Arc<dyn EntityBase> = victim.clone();
    let attacker = attacker.clone();
    finish_within(&attacker, move |attacker| attacker.attack(&victim));
}

#[test]
fn change_a_settable_field_critical_hit_event() {
    let _serial = support::serial();
    let attacker = support::player("CritAttacker");
    let victim = support::player("CritVictim");
    place(&attacker, 1000.0);
    place(&victim, 1000.0);
    let seen = log();
    let seen_by_handler = seen.clone();
    let victim_id = victim.living_entity.entity.entity_id;
    on::<CriticalHitEvent, _>(move |event| {
        if !named(&event.player, "CritAttacker") {
            return;
        }
        push(
            &seen_by_handler,
            format!(
                "{} {} {}",
                event.target.get_entity().entity_id == victim_id,
                event.vanilla_critical,
                event.vanilla_multiplier
            ),
        );
        event.critical_hit = true;
        event.damage_multiplier = 3.0;
    });

    let before = health(&victim);
    attack(&attacker, &victim);

    // A fist deals 1 damage; the handler made the hit critical with a multiplier of 3.
    assert_eq!(entries(&seen), ["true false 1"]);
    assert!(
        (before - health(&victim) - 3.0).abs() < 1e-4,
        "{before} -> {}",
        health(&victim)
    );
}

#[test]
fn forced_critical_hit_keeps_the_sprint_knockback_and_the_mace_smash() {
    let _serial = support::serial();
    let sprinter = support::player("CritSprinter");
    let macer = support::player("CritMacer");
    let victim = support::player("CritTarget");
    let smashed = support::player("SmashTarget");
    for player in [&sprinter, &macer, &victim, &smashed] {
        place(player, 1500.0);
    }
    on::<CriticalHitEvent, _>(|event| {
        if named(&event.player, "CritSprinter") || named(&event.player, "CritMacer") {
            assert!(
                !event.vanilla_critical,
                "a sprint or mace hit is not critical"
            );
            event.critical_hit = true;
            event.damage_multiplier = 2.0;
        }
    });

    // A sprint hit pushes the victim (`combat::handle_knockback`), which slows the attacker.
    let entity = &sprinter.living_entity.entity;
    entity.set_sprinting(true);
    entity.velocity.store(Vector3::new(1.0, 0.0, 1.0));
    attack(&sprinter, &victim);
    let velocity = entity.velocity.load();
    assert!((velocity.x - 0.6).abs() < 1e-6, "{velocity:?}");

    // A falling mace hit smashes: 1.5 damage per fallen block, and the fall distance resets.
    macer
        .inventory()
        .set_held_item(ItemStack::new(1, &Item::MACE));
    macer
        .living_entity
        .entity
        .on_ground
        .store(false, Ordering::Relaxed);
    macer.living_entity.fall_distance.store(8.0);
    let before = health(&smashed);
    attack(&macer, &smashed);
    let dealt = before - health(&smashed);
    assert!(dealt >= 12.0, "{before} -> {}", health(&smashed));
    assert!(macer.living_entity.fall_distance.load().abs() < 1e-6);
}

#[test]
fn cancel_sweep_attack_event() {
    let _serial = support::serial();
    let attacker = support::player("SweepAttacker");
    let victim = support::player("SweepVictim");
    let neighbour = support::player("SweepNeighbour");
    for player in [&attacker, &victim, &neighbour] {
        place(player, 2000.0);
    }
    let later = Arc::new(AtomicBool::new(false));
    let later_ran = later.clone();
    on_with::<SweepAttackEvent, _>(EventPriority::High, false, |event| {
        if !named(&event.player, "SweepAttacker") {
            return;
        }
        assert!(!event.vanilla_sweep, "a fist never sweeps in vanilla");
        // A cancel keeps the value of `sweeping`, as in NeoForge.
        event.sweeping = true;
        event.set_cancelled(true);
    });
    on_with::<SweepAttackEvent, _>(EventPriority::Low, false, move |event| {
        if named(&event.player, "SweepAttacker") {
            later_ran.store(true, Ordering::Relaxed);
            event.sweeping = false;
        }
    });

    let before = health(&neighbour);
    attack(&attacker, &victim);

    assert!(!later.load(Ordering::Relaxed));
    assert!(
        health(&neighbour) < before,
        "the sweep reached the neighbour"
    );
}

#[test]
fn one_event_per_fire_site_break_speed_and_harvest_check() {
    let _serial = support::serial();
    let player = support::player("Miner");
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_flag = cancel.clone();
    on::<PlayerEventBreakSpeed, _>(move |event| {
        if !named(&event.player, "Miner") {
            return;
        }
        assert_eq!(event.pos, Some(BlockPos::new(3, 64, 3)));
        event.new_speed = event.original_speed * 2.0;
        if cancel_flag.load(Ordering::Relaxed) {
            event.set_cancelled(true);
        }
    });
    on::<PlayerEventHarvestCheck, _>(|event| {
        if named(&event.player, "Miner") && event.pos == BlockPos::new(3, 64, 3) {
            assert!(!event.can_harvest, "a fist does not harvest stone");
            event.can_harvest = true;
        }
    });

    let state = Block::STONE.default_state;
    let pos = BlockPos::new(3, 64, 3);
    assert!(player.can_harvest(state, &Block::STONE, &pos));
    assert!(!player.can_harvest(state, &Block::STONE, &BlockPos::new(4, 64, 4)));
    let speed = pumpkin_core::block::calc_block_breaking(&player, state, &Block::STONE, &pos);
    // The handler doubles the speed; stone has hardness 1.5, and the harvest check makes the
    // divisor 30.
    let expected = 2.0 * player.get_mining_speed(&Block::STONE) / 1.5 / 30.0;
    assert!((speed - expected).abs() < 1e-6, "{speed}");

    cancel.store(true, Ordering::Relaxed);
    let speed = pumpkin_core::block::calc_block_breaking(&player, state, &Block::STONE, &pos);
    assert!(speed < 0.0, "a cancel makes the block unbreakable: {speed}");
}

/// Gives `player` a bow in the selected slot and `arrows` arrows in the slot next to it.
fn bow_and_arrows(player: &Player, arrows: u8) {
    let inventory = player.inventory();
    inventory.set_selected_slot(0);
    inventory.set_stack(0, ItemStack::new(1, &Item::BOW));
    inventory.set_stack(1, ItemStack::new(arrows, &Item::ARROW));
}

fn arrows(player: &Player) -> u8 {
    player.inventory().get_stack(1).item_count
}

#[test]
fn cancel_arrow_nock_event() {
    let _serial = support::serial();
    let player = support::player("Archer");
    bow_and_arrows(&player, 4);
    on::<ArrowNockEvent, _>(|event| {
        if named(&event.player, "Archer") {
            assert!(event.has_ammo);
            assert_eq!(event.bow.item, &Item::BOW);
            event.set_cancelled(true);
        }
    });

    player
        .living_entity
        .item_use_time
        .store(0, Ordering::Relaxed);
    BowItem.normal_use(&Item::BOW, &player);

    assert_eq!(
        player.living_entity.item_use_time.load(Ordering::Relaxed),
        0,
        "a cancelled nock does not draw the bow"
    );
}

#[test]
fn change_a_settable_field_arrow_loose_event() {
    let _serial = support::serial();
    let player = support::player("Looser");
    place(&player, 3000.0);
    bow_and_arrows(&player, 4);
    let charge = Arc::new(Mutex::new(None::<i32>));
    let cancel = Arc::new(AtomicBool::new(false));
    let (charge_set, cancel_flag) = (charge.clone(), cancel.clone());
    on::<ArrowLooseEvent, _>(move |event| {
        if !named(&event.player, "Looser") {
            return;
        }
        assert_eq!(event.charge, 20);
        let set = *charge_set.lock().unwrap();
        if let Some(charge) = set {
            event.charge = charge;
        }
        if cancel_flag.load(Ordering::Relaxed) {
            event.set_cancelled(true);
        }
    });
    let release = || {
        player
            .living_entity
            .item_use_time
            .store(BowItem::USE_DURATION - 20, Ordering::Relaxed);
        BowItem::release_bow(&player, &ItemStack::new(1, &Item::BOW));
    };

    release();
    assert_eq!(arrows(&player), 3, "a full draw shoots an arrow");

    // A charge under 3 ticks shoots nothing.
    *charge.lock().unwrap() = Some(1);
    release();
    assert_eq!(arrows(&player), 3);

    *charge.lock().unwrap() = None;
    cancel.store(true, Ordering::Relaxed);
    release();
    assert_eq!(arrows(&player), 3, "a cancel shoots nothing");
}

fn count_of(player: &Player, item: &'static Item) -> u32 {
    let inventory = player.inventory();
    (0..36)
        .map(|slot| inventory.get_stack(slot))
        .filter(|stack| stack.item == item)
        .map(|stack| u32::from(stack.item_count))
        .sum()
}

fn spawn_item(player: &Player, stack: ItemStack) -> Arc<ItemEntity> {
    let world = player.world();
    let entity = Entity::new(
        world.clone(),
        player.living_entity.entity.pos.load(),
        &EntityType::ITEM,
    );
    let item = Arc::new(ItemEntity::new(entity, stack));
    world.spawn_entity(item.clone());
    item
}

#[test]
fn one_event_per_fire_site_item_entity_pickup_events() {
    let _serial = support::serial();
    let player = support::player("Picker");
    let decision = Arc::new(Mutex::new(TriState::False));
    let decided = decision.clone();
    let posts = log();
    let (pre, posted) = (posts.clone(), posts.clone());
    on::<ItemEntityPickupEventPre, _>(move |event| {
        if named(&event.player, "Picker") {
            push(&pre, "pre");
            event.can_pickup = *decided.lock().unwrap();
        }
    });
    on::<ItemEntityPickupEventPost, _>(move |event| {
        if named(&event.player, "Picker") {
            push(
                &posted,
                format!(
                    "{} {} {}",
                    event.original_stack.item.registry_key,
                    event.original_stack.item_count,
                    event.current_stack.item_count
                ),
            );
        }
    });

    // Vanilla calls `playerTouch` only for a living player that is not a spectator.
    let item = spawn_item(&player, ItemStack::new(5, &Item::EMERALD));
    player.gamemode.store(GameMode::Spectator);
    item.on_player_collision(&player);
    player.gamemode.store(GameMode::Survival);
    assert!(entries(&posts).is_empty());

    // `False` refuses the pickup, `True` ignores the pickup delay of a new item entity.
    assert!(item.get_pickup_delay() > 0);
    item.on_player_collision(&player);
    assert_eq!(count_of(&player, &Item::EMERALD), 0);

    *decision.lock().unwrap() = TriState::True;
    item.on_player_collision(&player);
    assert_eq!(count_of(&player, &Item::EMERALD), 5);
    assert_eq!(entries(&posts), ["pre", "pre", "emerald 5 0"]);
}

#[test]
fn cancel_pickup_xp_event() {
    let _serial = support::serial();
    let player = support::player("XpPicker");
    let cancel = Arc::new(AtomicBool::new(true));
    let cancel_flag = cancel.clone();
    on::<PlayerXpEventPickupXp, _>(move |event| {
        if named(&event.player, "XpPicker") {
            assert_eq!(event.amount, 5);
            event.set_cancelled(cancel_flag.load(Ordering::Relaxed));
        }
    });
    let world = player.world();
    let orb = Arc::new(ExperienceOrbEntity::new(
        Entity::new(
            world.clone(),
            player.living_entity.entity.pos.load(),
            &EntityType::EXPERIENCE_ORB,
        ),
        5,
    ));
    world.spawn_entity(orb.clone());
    *player.experience_pick_up_delay.lock().unwrap() = 0;

    orb.on_player_collision(&player);
    assert_eq!(player.experience_points.load(Ordering::Relaxed), 0);
    assert_eq!(*player.experience_pick_up_delay.lock().unwrap(), 0);

    cancel.store(false, Ordering::Relaxed);
    orb.on_player_collision(&player);
    assert_eq!(player.experience_points.load(Ordering::Relaxed), 5);
}

#[test]
fn one_event_per_fire_site_switch_hotbar_slot_event_post() {
    let _serial = support::serial();
    let player = support::player("SlotSwitcher");
    player.inventory().set_selected_slot(2);
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<PlayerSwitchHotbarSlotEventPost, _>(move |event| {
        if named(&event.player, "SlotSwitcher") {
            push(
                &seen_by_handler,
                format!(
                    "{} {} {}",
                    event.old_slot_index,
                    event.new_slot_index,
                    event.player.inventory().get_selected_slot()
                ),
            );
        }
    });

    // The second packet selects the selected slot: `NeoForge` fires only on a change.
    for _ in 0..2 {
        java_client(&player).handle_set_held_item(
            support::server(),
            &player,
            &SSetHeldItem { slot: 5 },
        );
    }

    assert_eq!(entries(&seen), ["2 5 5"]);
}

/// `PlayerInteractEvent.LeftClickBlock` fires first at the start of a dig, as in
/// `ServerPlayerGameMode.handleBlockBreakAction` of `NeoForge` 26.3, and a cancel stops the dig
/// before `BlockDamageEvent`. The stop of a dig fires nothing.
#[test]
fn left_click_block_fires_at_the_dig_start() {
    let _serial = support::serial();
    let player = support::player("Digger");
    player.set_client_loaded(true);
    let pos = player
        .get_entity()
        .block_pos
        .load()
        .offset(Vector3::new(1, -1, 0));
    let cancel = Arc::new(AtomicBool::new(false));
    let seen = log();
    let (clicks, cancel_flag) = (seen.clone(), cancel.clone());
    on::<PlayerInteractEvent, _>(move |event| {
        if named(&event.player, "Digger") && event.action == InteractAction::LeftClickBlock {
            push(&clicks, format!("click {}", event.clicked_pos.unwrap()));
            event.set_cancelled(cancel_flag.load(Ordering::Relaxed));
        }
    });
    let damage = seen.clone();
    on::<BlockDamageEvent, _>(move |event| {
        if named(&event.player, "Digger") {
            push(&damage, "damage".to_owned());
            event.set_cancelled(true);
        }
    });

    let dig = |status: Status| SPlayerAction {
        status: VarInt(status as i32),
        position: pos,
        face: 1,
        sequence: VarInt(1),
    };
    let client = java_client(&player);
    client.handle_player_action(&player, &dig(Status::StartedDigging), support::server());
    client.handle_player_action(&player, &dig(Status::CancelledDigging), support::server());
    cancel.store(true, Ordering::Relaxed);
    client.handle_player_action(&player, &dig(Status::StartedDigging), support::server());

    let click = format!("click {pos}");
    assert_eq!(entries(&seen), [click.as_str(), "damage", click.as_str()]);
}

/// `PlayerSwitchHotbarSlotEvent.Pre` (`PlayerItemHeldEvent`) fires only when the slot changes,
/// as in `ServerGamePacketListenerImpl.handleSetCarriedItem` of `NeoForge` 26.3.
#[test]
fn switch_hotbar_slot_event_pre_fires_only_on_a_change() {
    let _serial = support::serial();
    let player = support::player("SlotHolder");
    player.inventory().set_selected_slot(3);
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<PlayerItemHeldEvent, _>(move |event| {
        if named(&event.player, "SlotHolder") {
            push(
                &seen_by_handler,
                format!("{} {}", event.previous_slot, event.new_slot),
            );
        }
    });

    for slot in [3, 6, 6] {
        java_client(&player).handle_set_held_item(
            support::server(),
            &player,
            &SSetHeldItem { slot },
        );
    }

    assert_eq!(entries(&seen), ["3 6"]);
    assert_eq!(player.inventory().get_selected_slot(), 6);
}

#[test]
fn one_event_per_fire_site_tracking_events() {
    let _serial = support::serial();
    let viewer = support::player("TrackViewer");
    let target = support::player("TrackTarget");
    let seen = log();
    let (start, stop) = (seen.clone(), seen.clone());
    on::<PlayerEventStartTracking, _>(move |event| {
        if named(&event.player, "TrackViewer") {
            push(
                &start,
                format!("start {}", event.target.get_entity().entity_id),
            );
        }
    });
    on::<PlayerEventStopTracking, _>(move |event| {
        if named(&event.player, "TrackViewer") {
            push(
                &stop,
                format!("stop {}", event.target.get_entity().entity_id),
            );
        }
    });
    let target_entity: Arc<dyn EntityBase> = target.clone();
    let tracked = TrackedEntity::new(target_entity, 10, 3, true);
    let id = target.living_entity.entity.entity_id;

    tracked.seen_by.insert(viewer.gameprofile.id);
    tracked.add_pairing(&viewer);
    tracked.remove_player(&viewer);
    tracked.seen_by.insert(viewer.gameprofile.id);
    tracked.forget_player(&viewer);

    assert_eq!(
        entries(&seen),
        [
            format!("start {id}"),
            format!("stop {id}"),
            format!("stop {id}")
        ]
    );
}

#[test]
fn tracking_events_fire_after_the_entity_map_iteration() {
    let _serial = support::serial();
    let viewer = support::player("TrackRepairer");
    let world = support::world();
    let item = spawn_item(&viewer, ItemStack::new(1, &Item::STICK));
    let id = item.get_entity().entity_id;
    let seen = log();
    let stopped = seen.clone();
    on::<PlayerEventStopTracking, _>(move |event| {
        if named(&event.player, "TrackRepairer") {
            let target = event.target.get_entity().entity_id;
            // A write to the shard that the iteration of the entity map reads.
            support::world().entity_tracker.entity_map.remove(&target);
            push(&stopped, format!("stop {target}"));
        }
    });
    world
        .entity_tracker
        .get_tracked_entity(id)
        .unwrap()
        .seen_by
        .insert(viewer.gameprofile.id);

    finish_within(&viewer, move |viewer| {
        world.entity_tracker.repair_respawned_player(viewer, &world);
    });

    assert_eq!(entries(&seen), [format!("stop {id}")]);
    assert!(
        support::world()
            .entity_tracker
            .get_tracked_entity(id)
            .is_none()
    );
}

#[test]
fn one_event_per_fire_site_logout_save_and_stop_tracking() {
    let _serial = support::serial();
    let leaver = support::player("Leaver");
    let world = support::world();
    let item = spawn_item(&leaver, ItemStack::new(1, &Item::STICK));
    let id = item.get_entity().entity_id;
    let seen = log();
    let (saved, stopped) = (seen.clone(), seen.clone());
    on::<PlayerEventSaveToFile, _>(move |event| {
        if named(&event.player, "Leaver") {
            push(&saved, "save");
        }
    });
    on::<PlayerEventStopTracking, _>(move |event| {
        if named(&event.player, "Leaver") {
            push(
                &stopped,
                format!("stop {}", event.target.get_entity().entity_id),
            );
        }
    });
    world
        .entity_tracker
        .get_tracked_entity(id)
        .unwrap()
        .seen_by
        .insert(leaver.gameprofile.id);

    // The disconnect of `lib.rs`: the world drops the player, then the server saves its data.
    support::RUNTIME.block_on(world.remove_player(&leaver, false));
    assert!(world.get_player_by_uuid(leaver.gameprofile.id).is_none());
    support::server()
        .player_data_storage
        .handle_player_leave(&leaver)
        .unwrap();

    assert_eq!(entries(&seen), [format!("stop {id}"), "save".to_owned()]);
}

#[test]
fn change_a_settable_field_name_format_events() {
    let _serial = support::serial();
    let player = support::player("Formatted");
    on::<PlayerEventNameFormat, _>(|event| {
        if named(&event.player, "Formatted") {
            assert_eq!(event.username.clone().get_text(), "Formatted");
            event.display_name = TextComponent::text("Sir Formatted");
        }
    });
    on::<PlayerEventTabListNameFormat, _>(|event| {
        if named(&event.player, "Formatted") {
            event.display_name = Some(TextComponent::text("[tab] Formatted"));
        }
    });

    assert_eq!(player.get_display_name().get_text(), "Sir Formatted");
    assert_eq!(
        player.get_tab_list_name().map(TextComponent::get_text),
        Some("[tab] Formatted".to_owned())
    );
    // A name set through the API wins, and the event does not fire.
    player.set_tab_list_name(Some(TextComponent::text("set")));
    assert_eq!(
        player.get_tab_list_name().map(TextComponent::get_text),
        Some("set".to_owned())
    );
}

#[test]
fn change_a_settable_field_respawn_position_event() {
    let _serial = support::serial();
    let player = support::player_with_writer("Respawner");
    on::<PlayerSpawnLocationEvent, _>(|event| {
        if named(&event.player, "Respawner") {
            assert!(!event.from_end_fight);
            assert_eq!(event.original_spawn_pos, event.spawn_pos);
            event.spawn_pos = Vector3::new(8.5, 120.0, 8.5);
            event.yaw = 90.0;
        }
    });

    let world = support::world();
    let respawned = player.clone();
    // The respawn then waits for the client to take the center chunk, which no client does
    // here; the position is set before that wait.
    support::RUNTIME.spawn(async move { world.respawn_player(&respawned, false).await });
    let expected = Vector3::new(8.5, 120.0, 8.5);
    for _ in 0..600 {
        if player.living_entity.entity.pos.load() == expected {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    assert_eq!(player.living_entity.entity.pos.load(), expected);
    assert!((player.living_entity.entity.yaw.load() - 90.0).abs() < 1e-4);
}

#[test]
fn one_event_per_fire_site_player_file_events() {
    let _serial = support::serial();
    let server = support::server();
    let seen = log();
    let (saved, loaded) = (seen.clone(), seen.clone());
    on::<PlayerEventSaveToFile, _>(move |event| {
        if named(&event.player, "FileSaver") {
            assert_eq!(event.player_file("dat"), None);
            let file = event.player_file("mymod").unwrap();
            assert!(file.ends_with(format!("{}.mymod", event.player.gameprofile.id)));
            push(&saved, "save");
        }
    });
    on::<PlayerEventLoadFromFile, _>(move |event| {
        if named(&event.player, "FileLoader") {
            assert_eq!(event.player_uuid, event.player.gameprofile.id.to_string());
            push(&loaded, "load");
        }
    });

    let saver = support::player("FileSaver");
    server
        .player_data_storage
        .extract_data_and_save_player(&saver)
        .unwrap();

    // `Server::add_player` loads the data of a joining player.
    let template = support::player("FileTemplate");
    let profile = pumpkin_core::net::GameProfile {
        id: uuid::Uuid::new_v4(),
        name: "FileLoader".to_owned(),
        properties: arc_swap::ArcSwap::from_pointee(Vec::new()),
        profile_actions: None,
    };
    let client = template.client.clone();
    let joined = server.add_player(client, profile, None);
    assert!(joined.is_some());

    assert_eq!(entries(&seen), ["save", "load"]);
}

/// Opens an anvil with a damaged sword and 3 diamonds, and gives the player levels to pay.
fn anvil(player: &Player) -> Arc<SimpleInventory> {
    let inventory = Arc::new(SimpleInventory::new(3));
    let mut sword = ItemStack::new(1, &Item::DIAMOND_SWORD);
    sword.set_damage(1000);
    inventory.set_stack(0, sword);
    inventory.set_stack(1, ItemStack::new(3, &Item::DIAMOND));
    let handler = AnvilScreenHandler::new(1, &player.inventory, inventory.clone());
    *player.current_screen_handler.lock().unwrap() = Arc::new(Mutex::new(handler));
    player.add_experience_levels(30);
    inventory
}

#[test]
fn cancel_anvil_craft_event_pre() {
    let _serial = support::serial();
    let player = support::player("AnvilCrafter");
    let cancel = Arc::new(AtomicBool::new(false));
    let seen = log();
    let (cancel_flag, pre, post) = (cancel.clone(), seen.clone(), seen.clone());
    on::<AnvilCraftEventPre, _>(move |event| {
        if named(&event.player, "AnvilCrafter") {
            push(
                &pre,
                format!(
                    "pre {} {} {}",
                    event.left.item.registry_key,
                    event.right.item_count,
                    event.output.item.registry_key
                ),
            );
            event.set_cancelled(cancel_flag.load(Ordering::Relaxed));
        }
    });
    on::<AnvilCraftEventPost, _>(move |event| {
        if named(&event.player, "AnvilCrafter") {
            push(&post, format!("post {}", event.right.item_count));
        }
    });

    // The first click computes the result, the second takes it.
    let inventory = anvil(&player);
    click(&player, 1, 2);
    click(&player, 1, 2);
    assert!(inventory.get_stack(0).is_empty(), "the take used the sword");
    assert_eq!(
        entries(&seen),
        ["pre diamond_sword 3 diamond_sword", "post 3"]
    );

    seen.lock().unwrap().clear();
    cancel.store(true, Ordering::Relaxed);
    let inventory = anvil(&player);
    click(&player, 1, 2);
    click(&player, 1, 2);
    assert!(
        !inventory.get_stack(0).is_empty(),
        "a cancel leaves the inputs"
    );
    assert_eq!(entries(&seen), ["pre diamond_sword 3 diamond_sword"]);
}

/// Whether the open screen of `player` is its inventory.
fn inventory_screen_open(player: &Player) -> bool {
    let current = player.current_screen_handler.lock().unwrap().clone();
    let inventory: SharedScreenHandler = player.player_screen_handler.clone();
    Arc::ptr_eq(&current, &inventory)
}

fn levels(player: &Player) -> i32 {
    player.experience_level.load(Ordering::Relaxed)
}

#[test]
fn anvil_craft_events_handler_closes_the_screen_or_changes_the_inputs() {
    const NAMES: [&str; 3] = ["AnvilPreCloser", "AnvilPostCloser", "AnvilChanger"];
    let _serial = support::serial();
    let seen = log();
    let (pre, post) = (seen.clone(), seen.clone());
    on::<AnvilCraftEventPre, _>(move |event| {
        let name = event.player.gameprofile.name.clone();
        if !NAMES.contains(&name.as_str()) {
            return;
        }
        push(&pre, format!("pre {name}"));
        if name == "AnvilPreCloser" {
            // It locks the screen handler: the event fires outside that lock.
            event.player.close_handled_screen();
        } else if name == "AnvilChanger" {
            let screen = event.player.current_screen_handler.lock().unwrap().clone();
            let mut screen = screen.lock().unwrap();
            let anvil = screen
                .as_any_mut()
                .downcast_mut::<AnvilScreenHandler>()
                .unwrap();
            anvil
                .inventory
                .set_stack(0, ItemStack::new(1, &Item::STICK));
        }
    });
    on::<AnvilCraftEventPost, _>(move |event| {
        let name = event.player.gameprofile.name.clone();
        if NAMES.contains(&name.as_str()) {
            push(&post, format!("post {name}"));
            if name == "AnvilPostCloser" {
                event.player.close_handled_screen();
            }
        }
    });

    // Closed during the Pre: the anvil gave the inputs back, and nothing was paid or used.
    let player = support::player("AnvilPreCloser");
    let inventory = anvil(&player);
    click(&player, 1, 2);
    let before = levels(&player);
    click(&player, 1, 2);
    assert!(inventory_screen_open(&player));
    assert_eq!(levels(&player), before);
    assert!(inventory.get_stack(0).is_empty() && inventory.get_stack(1).is_empty());
    assert_eq!(count_of(&player, &Item::DIAMOND), 3);

    // Closed during the Post: the take was paid and used the inputs first.
    let player = support::player("AnvilPostCloser");
    let inventory = anvil(&player);
    click(&player, 1, 2);
    let before = levels(&player);
    click(&player, 1, 2);
    assert!(inventory_screen_open(&player));
    assert!(levels(&player) < before);
    assert!(inventory.get_stack(0).is_empty() && inventory.get_stack(1).is_empty());
    assert_eq!(count_of(&player, &Item::DIAMOND), 0);

    // The handler changed an input: the anvil leaves the inputs to it and costs nothing.
    let player = support::player("AnvilChanger");
    let inventory = anvil(&player);
    click(&player, 1, 2);
    let before = levels(&player);
    click(&player, 1, 2);
    assert_eq!(levels(&player), before);
    assert_eq!(inventory.get_stack(0).item, &Item::STICK);
    assert_eq!(inventory.get_stack(1).item_count, 3);

    assert_eq!(
        entries(&seen),
        [
            "pre AnvilPreCloser",
            "pre AnvilPostCloser",
            "post AnvilPostCloser",
            "pre AnvilChanger"
        ]
    );
}

/// Opens the trade screen of a new wandering trader with one offer and pays it, and returns the
/// sync id. `VillagerEntity::complete_trade` locks the trade screen that the click holds (see
/// `resend_offers_to_player`, issue #115), so the tests trade with a wandering trader.
fn open_trade(player: &Arc<Player>) -> i32 {
    let world = player.world();
    let villager = WanderingTraderEntity::new(Entity::new(
        world.clone(),
        player.living_entity.entity.pos.load(),
        &EntityType::WANDERING_TRADER,
    ));
    world.spawn_entity(villager.clone());
    villager.offers.lock().unwrap().push(MerchantOffer {
        base_cost_a: ItemStackSerializer(std::borrow::Cow::Owned(ItemStack::new(
            1,
            &Item::EMERALD,
        ))),
        output: ItemStackSerializer(std::borrow::Cow::Owned(ItemStack::new(1, &Item::BREAD))),
        cost_b: None,
        reward_exp: false,
        uses: 0,
        max_uses: 12,
        xp: 1,
        special_price: 0,
        price_multiplier: 0.05,
        demand: 0,
    });

    let sync_id = player.open_handled_screen(villager.as_ref(), None).unwrap();
    villager
        .merchant_inventory
        .set_stack(0, ItemStack::new(1, &Item::EMERALD));
    let screen = player.current_screen_handler.lock().unwrap().clone();
    screen
        .lock()
        .unwrap()
        .as_any_mut()
        .downcast_mut::<MerchantScreenHandler>()
        .unwrap()
        .update_result_slot();
    i32::from(sync_id)
}

#[test]
fn one_event_per_fire_site_trade_with_villager_event() {
    let _serial = support::serial();
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<TradeWithVillagerEvent, _>(move |event| {
        if named(&event.player, "Trader") || named(&event.player, "TradeCloser") {
            push(
                &seen_by_handler,
                format!(
                    "{} {} {}",
                    event.player.gameprofile.name,
                    event.offer.output.0.item.registry_key,
                    event.villager.get_entity().entity_type.resource_name
                ),
            );
        }
        if named(&event.player, "TradeCloser") {
            // It locks the trade screen: the event fires after the click released it.
            event.player.close_handled_screen();
        }
    });

    let player = support::player("Trader");
    let sync_id = open_trade(&player);
    click(&player, sync_id, 2);

    let closer = support::player("TradeCloser");
    let sync_id = open_trade(&closer);
    click(&closer, sync_id, 2);
    assert!(inventory_screen_open(&closer));

    assert_eq!(
        entries(&seen),
        [
            "Trader bread wandering_trader",
            "TradeCloser bread wandering_trader"
        ]
    );
}

#[test]
fn one_event_per_fire_site_client_information_updated_event() {
    let _serial = support::serial();
    let player = support::player("Configured");
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<ClientInformationUpdatedEvent, _>(move |event| {
        if named(&event.player, "Configured") {
            push(
                &seen_by_handler,
                format!(
                    "{} {}",
                    event.old_information.locale, event.updated_information.locale
                ),
            );
        }
    });
    let packet = SClientInformationPlay {
        locale: "it_it",
        view_distance: 8,
        chat_mode: VarInt(0),
        chat_colors: true,
        skin_parts: 0,
        main_hand: VarInt(1),
        text_filtering: false,
        server_listing: true,
        particle_status: VarInt(0),
    };

    java_client(&player).handle_client_information(support::server(), &player, &packet);

    assert_eq!(entries(&seen), ["en_us it_it"]);
}

#[test]
fn one_event_per_fire_site_player_flyable_fall_event() {
    let _serial = support::serial();
    let player = support::player("Flyer");
    player.abilities.lock().unwrap().allow_flying = true;
    let seen = log();
    let seen_by_handler = seen.clone();
    on::<PlayerFlyableFallEvent, _>(move |event| {
        if named(&event.player, "Flyer") {
            push(
                &seen_by_handler,
                format!("{} {}", event.distance, event.multiplier),
            );
        }
    });
    let before = health(&player);

    player
        .living_entity
        .handle_fall_damage(player.as_ref(), 10.0, 1.0);

    assert_eq!(entries(&seen), ["10 1"]);
    assert!((health(&player) - before).abs() < f32::EPSILON);
}

/// A screen that opens nothing: `Player::open_handled_screen` fires the open event first.
struct NoScreen;

impl ScreenHandlerFactory for NoScreen {
    fn create_screen_handler(
        &self,
        _sync_id: u8,
        _player_inventory: &Arc<pumpkin_inventory::player::player_inventory::PlayerInventory>,
        _player: &dyn InventoryPlayer,
    ) -> Option<SharedScreenHandler> {
        None
    }

    fn get_display_name(&self) -> TextComponent {
        TextComponent::text("none")
    }
}

struct ContainerProbe(Log);

impl ParentEventHandler for ContainerProbe {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut (dyn Payload + Send + Sync),
    ) -> BoxFuture<'a, ()> {
        let name = event.get_name();
        let player = <dyn Payload>::downcast_ref::<InventoryOpenEvent>(event).map_or_else(
            || {
                <dyn Payload>::downcast_ref::<InventoryCloseEvent>(event)
                    .map(|close| close.player.gameprofile.name.clone())
                    .unwrap_or_default()
            },
            |open| open.player.gameprofile.name.clone(),
        );
        if player == "ContainerUser" {
            push(&self.0, name);
        }
        Box::pin(async {})
    }
}

#[test]
fn handler_on_a_parent_player_container_event() {
    let _serial = support::serial();
    let player = support::player("ContainerUser");
    let seen = log();
    let manager = &support::server().plugin_manager;
    manager.register_parent::<PlayerContainerEvent, _>(
        Arc::new(ContainerProbe(seen.clone())),
        EventPriority::Normal,
        true,
        false,
    );
    assert!(manager.has_handlers::<InventoryOpenEvent>());
    assert!(manager.has_handlers::<InventoryCloseEvent>());

    let _ = player.open_handled_screen(&NoScreen, None);
    player.close_handled_screen();

    assert_eq!(
        entries(&seen),
        ["InventoryOpenEvent", "InventoryCloseEvent"]
    );
}
