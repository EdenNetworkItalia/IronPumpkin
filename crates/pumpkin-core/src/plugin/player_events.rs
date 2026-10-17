//! The `IronPumpkin` fire sites of the `NeoForge` player events (`event.entity.player`).
//!
//! Each function checks `has_handlers` before it builds an event, so a fire site with no
//! handler allocates nothing and returns the vanilla value. The upstream code calls these
//! functions with one line.
//!
//! No event fires under the lock of a screen handler or of a shard of the entity tracker: the
//! anvil craft events fire after the click released the screen handler, and the tracking and
//! trade events wait for the [`Deferral`] of their scope.

use std::cell::RefCell;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use dashmap::DashSet;
use pumpkin_data::{BlockState, item_stack::ItemStack};
use pumpkin_inventory::{anvil::AnvilScreenHandler, screen_handler::ScreenHandler};
use pumpkin_protocol::java::client::play::MerchantOffer;
use pumpkin_util::{Hand, math::position::BlockPos, math::vector3::Vector3, text::TextComponent};
use uuid::Uuid;

use crate::entity::{EntityBase, combat::AttackType, player::Player};
use crate::net::PlayerConfig;
use crate::plugin::api::events::{
    Payload,
    player::{
        anvil_craft::{AnvilCraftEventPost, AnvilCraftEventPre},
        arrow::{ArrowLooseEvent, ArrowNockEvent},
        attack::{CriticalHitEvent, SweepAttackEvent},
        break_speed::{PlayerEventBreakSpeed, PlayerEventHarvestCheck},
        client_information::ClientInformationUpdatedEvent,
        flyable_fall::PlayerFlyableFallEvent,
        hotbar::PlayerSwitchHotbarSlotEventPost,
        item_entity_pickup::{
            ItemEntityPickupEventPost, ItemEntityPickupEventPre, PlayerXpEventPickupXp,
        },
        name_format::{PlayerEventNameFormat, PlayerEventTabListNameFormat},
        player_file::{PlayerEventLoadFromFile, PlayerEventSaveToFile},
        player_spawn_location::PlayerSpawnLocationEvent,
        tracking::{PlayerEventStartTracking, PlayerEventStopTracking},
        trade_with_villager::TradeWithVillagerEvent,
    },
    results::TriState,
};
use crate::server::Server;
use crate::world::World;
use pumpkin_data::dimension::Dimension;

/// The server of `player` when a handler of `E` listens.
pub(super) fn listening<E: Payload + 'static>(player: &Player) -> Option<Arc<Server>> {
    let server = player.world().server.upgrade()?;
    server.plugin_manager.has_handlers::<E>().then_some(server)
}

/// The shared handle of `player`, for an event that owns its player. A player that leaves its
/// world is no longer in the player list: [`untrack_leaving_player`] provides it.
fn shared(player: &Player) -> Option<Arc<Player>> {
    player
        .world()
        .get_player_by_uuid(player.gameprofile.id)
        .or_else(|| leaving(player))
}

/// Fires `E` built by `make` when a handler listens, and returns it with the changes.
pub fn fire<E, F>(player: &Player, make: F) -> Option<E>
where
    E: Payload + Send + Sync + 'static,
    F: FnOnce(Arc<Player>) -> E,
{
    let server = listening::<E>(player)?;
    let player = shared(player)?;
    let mut event = make(player);
    server.plugin_manager.fire_blocking(&server, &mut event);
    Some(event)
}

/// Like [`fire`], for a caller that has the shared handle of `player`.
fn fire_shared<E, F>(player: &Arc<Player>, make: F) -> Option<E>
where
    E: Payload + Send + Sync + 'static,
    F: FnOnce(Arc<Player>) -> E,
{
    let server = listening::<E>(player)?;
    let mut event = make(player.clone());
    server.plugin_manager.fire_blocking(&server, &mut event);
    Some(event)
}

/// Fires `event`, whose changes nobody reads.
fn fire_event<E: Payload + Send + Sync + 'static>(server: &Arc<Server>, mut event: E) {
    server.plugin_manager.fire_blocking(server, &mut event);
}

/// The events that the active [`Deferral`] of a thread holds back.
#[derive(Default)]
struct Pending {
    events: Vec<Box<dyn FnOnce()>>,
    /// The player that [`untrack_leaving_player`] removes from the trackers.
    leaving: Option<Arc<Player>>,
}

thread_local! {
    static PENDING: RefCell<Option<Pending>> = const { RefCell::new(None) };
}

/// Holds back the tracking and trade events while it lives. The outermost deferral of a thread
/// fires them when it drops, after the locks of its scope: create it before the lock (the screen
/// handler of a click, a shard of the entity map).
#[must_use]
pub struct Deferral {
    outermost: bool,
}

impl Deferral {
    pub fn start() -> Self {
        let outermost = PENDING.with_borrow_mut(|pending| {
            if pending.is_some() {
                false
            } else {
                *pending = Some(Pending::default());
                true
            }
        });
        Self { outermost }
    }
}

impl Drop for Deferral {
    fn drop(&mut self) {
        if !self.outermost {
            return;
        }
        let Some(pending) = PENDING.with_borrow_mut(Option::take) else {
            return;
        };
        if std::thread::panicking() {
            return;
        }
        for fire in pending.events {
            fire();
        }
    }
}

/// Runs `fire` after the active deferral of this thread, or now when there is none.
fn defer(fire: impl FnOnce() + 'static) {
    let now = PENDING.with_borrow_mut(|pending| match pending {
        Some(pending) => {
            pending.events.push(Box::new(fire));
            None
        }
        None => Some(fire),
    });
    if let Some(fire) = now {
        fire();
    }
}

fn leaving(player: &Player) -> Option<Arc<Player>> {
    PENDING.with_borrow(|pending| {
        pending
            .as_ref()?
            .leaving
            .as_ref()
            .filter(|leaving| leaving.gameprofile.id == player.gameprofile.id)
            .cloned()
    })
}

/// Removes `player` from the entity tracker of `world` when it leaves the world
/// (`World::remove_player`), and fires [`PlayerEventStopTracking`] for it although the player
/// list no longer has it.
pub fn untrack_leaving_player(world: &World, player: &Arc<Player>) {
    let _deferral = Deferral::start();
    let previous = PENDING.with_borrow_mut(|pending| {
        pending
            .as_mut()
            .and_then(|pending| pending.leaving.replace(player.clone()))
    });
    world.entity_tracker.remove_entity(player.as_ref(), world);
    PENDING.with_borrow_mut(|pending| {
        if let Some(pending) = pending {
            pending.leaving = previous;
        }
    });
}

/// The critical hit and the sweep of a player attack after [`CriticalHitEvent`] and
/// [`SweepAttackEvent`].
#[derive(Clone, Copy)]
pub struct AttackHit {
    pub critical: bool,
    /// The damage multiplier of a critical hit.
    pub multiplier: f32,
    pub sweep: bool,
}

/// Fires [`CriticalHitEvent`] and [`SweepAttackEvent`] for an attack of the vanilla type
/// `attack_type`, as `Player#attack` of `NeoForge` does. Returns the attack type for the sound
/// and the particle, and the hit. The knockback and the mace smash keep `attack_type`.
#[must_use]
pub fn attack_hit(
    player: &Player,
    target: &Arc<dyn EntityBase>,
    attack_type: AttackType,
) -> (AttackType, AttackHit) {
    let vanilla_critical = attack_type == AttackType::Critical;
    let mut hit = AttackHit {
        critical: vanilla_critical,
        multiplier: if vanilla_critical { 1.5 } else { 1.0 },
        sweep: attack_type == AttackType::Sweeping,
    };
    if let Some(event) = fire(player, |player| {
        CriticalHitEvent::new(player, target.clone(), hit.multiplier, vanilla_critical)
    }) {
        hit.critical = event.critical_hit;
        hit.multiplier = event.damage_multiplier.max(0.0);
        // A vanilla critical hit is in the air, where vanilla never sweeps.
        hit.sweep &= !(hit.critical && event.disable_sweep);
    }
    if let Some(event) = fire(player, |player| {
        SweepAttackEvent::new(player, target.clone(), hit.sweep)
    }) {
        hit.sweep = event.sweeping;
    }
    let effect_type = if hit.critical {
        AttackType::Critical
    } else if hit.sweep {
        AttackType::Sweeping
    } else if matches!(attack_type, AttackType::Critical | AttackType::Sweeping) {
        AttackType::Strong
    } else {
        attack_type
    };
    (effect_type, hit)
}

/// Fires [`PlayerEventBreakSpeed`] for the mining speed `speed` and returns the speed to use.
/// A cancel returns -1, so the block does not break.
#[must_use]
pub fn break_speed(player: &Player, state: &BlockState, speed: f32, pos: &BlockPos) -> f32 {
    fire(player, |player| {
        PlayerEventBreakSpeed::new(player, state.id.to_state(), speed, Some(*pos))
    })
    .map_or(speed, |event| {
        if event.cancelled {
            -1.0
        } else {
            event.new_speed
        }
    })
}

/// Fires [`PlayerEventHarvestCheck`] with the vanilla answer and returns the answer to use.
#[must_use]
pub fn harvest_check(player: &Player, state: &BlockState, pos: &BlockPos, vanilla: bool) -> bool {
    fire(player, |player| {
        PlayerEventHarvestCheck::new(player, state.id.to_state(), *pos, vanilla)
    })
    .map_or(vanilla, |event| event.can_harvest)
}

/// Fires [`ArrowNockEvent`] for the bow in the main hand. Returns `true` when the draw must not
/// start: a cancel or a set action.
#[must_use]
pub fn arrow_nock(player: &Player, has_ammo: bool) -> bool {
    fire(player, |shared| {
        ArrowNockEvent::new(
            shared,
            player.inventory().held_item(),
            Hand::Right,
            player.world(),
            has_ammo,
        )
    })
    .is_some_and(|event| event.cancelled || event.action.is_some())
}

/// Fires [`ArrowLooseEvent`] and returns the charge to use, or `None` for a cancel.
#[must_use]
pub fn arrow_loose(player: &Player, bow: &ItemStack, charge: i32) -> Option<i32> {
    match fire(player, |shared| {
        ArrowLooseEvent::new(shared, bow.clone(), player.world(), charge, true)
    }) {
        None => Some(charge),
        Some(event) if event.cancelled => None,
        Some(event) => Some(event.charge),
    }
}

/// The item or experience orb entity with the id `entity_id` in the world of `player`.
fn entity(player: &Player, entity_id: i32) -> Option<Arc<dyn EntityBase>> {
    player.world().get_entity_by_id(entity_id)
}

/// Fires [`ItemEntityPickupEventPre`] and returns the pickup decision of the handlers.
#[must_use]
pub fn item_pickup_pre(player: &Player, item_entity_id: i32) -> TriState {
    if listening::<ItemEntityPickupEventPre>(player).is_none() {
        return TriState::Default;
    }
    let Some(item) = entity(player, item_entity_id) else {
        return TriState::Default;
    };
    fire(player, |player| ItemEntityPickupEventPre::new(player, item))
        .map_or(TriState::Default, |event| event.can_pickup)
}

/// Fires [`ItemEntityPickupEventPost`]. `stack` is the stack the item entity keeps, which had
/// `count_before` items before the pickup.
pub fn item_pickup_post(
    player: &Player,
    item_entity_id: i32,
    stack: &Mutex<ItemStack>,
    count_before: u8,
) {
    if listening::<ItemEntityPickupEventPost>(player).is_none() {
        return;
    }
    let Some(item) = entity(player, item_entity_id) else {
        return;
    };
    let left = stack.lock().unwrap_or_else(PoisonError::into_inner).clone();
    let _ = fire(player, |player| {
        ItemEntityPickupEventPost::new(player, item, left.copy_with_count(count_before), left)
    });
}

/// Fires [`PlayerXpEventPickupXp`]. Returns `true` for a cancel.
#[must_use]
pub fn pickup_xp_cancelled(player: &Player, orb_entity_id: i32, amount: u32) -> bool {
    if listening::<PlayerXpEventPickupXp>(player).is_none() {
        return false;
    }
    let Some(orb) = entity(player, orb_entity_id) else {
        return false;
    };
    fire(player, |player| {
        PlayerXpEventPickupXp::new(player, orb, amount)
    })
    .is_some_and(|event| event.cancelled)
}

/// Fires [`PlayerSwitchHotbarSlotEventPost`] when the selected slot changed, as `NeoForge`
/// does.
pub fn hotbar_post(player: &Player, old_slot: u8, new_slot: u8) {
    if old_slot == new_slot {
        return;
    }
    let _ = fire(player, |player| {
        PlayerSwitchHotbarSlotEventPost::new(player, old_slot, new_slot)
    });
}

/// Fires [`PlayerEventStartTracking`] after the active [`Deferral`].
pub fn start_tracking(player: &Arc<Player>, target: &Arc<dyn EntityBase>) {
    let Some(server) = listening::<PlayerEventStartTracking>(player) else {
        return;
    };
    let (player, target) = (player.clone(), target.clone());
    defer(move || fire_event(&server, PlayerEventStartTracking::new(player, target)));
}

/// Fires [`PlayerEventStopTracking`] after the active [`Deferral`].
pub fn stop_tracking(player: &Player, target: &Arc<dyn EntityBase>) {
    let Some(server) = listening::<PlayerEventStopTracking>(player) else {
        return;
    };
    let Some(player) = shared(player) else {
        return;
    };
    let target = target.clone();
    defer(move || fire_event(&server, PlayerEventStopTracking::new(player, target)));
}

/// Fires [`PlayerEventStopTracking`] for each player of `players` in `seen_by`.
pub fn stop_tracking_all(
    players: &[Arc<Player>],
    seen_by: &DashSet<Uuid>,
    target: &Arc<dyn EntityBase>,
) {
    let Some(first) = players.first() else {
        return;
    };
    if listening::<PlayerEventStopTracking>(first).is_none() {
        return;
    }
    for player in players
        .iter()
        .filter(|p| seen_by.contains(&p.gameprofile.id))
    {
        stop_tracking(player, target);
    }
}

/// Fires [`PlayerEventNameFormat`] and returns the display name to use.
#[must_use]
pub fn name_format(player: &Player, username: TextComponent) -> TextComponent {
    if listening::<PlayerEventNameFormat>(player).is_none() {
        return username;
    }
    match fire(player, |player| {
        PlayerEventNameFormat::new(player, username.clone())
    }) {
        Some(event) => event.display_name,
        None => username,
    }
}

/// Fires [`PlayerEventTabListNameFormat`] and returns the tab list name to use.
#[must_use]
pub fn tab_list_name_format(player: &Player) -> Option<TextComponent> {
    fire(player, PlayerEventTabListNameFormat::new).and_then(|event| event.display_name)
}

/// The respawn position after [`PlayerSpawnLocationEvent`] (`NeoForge`
/// `PlayerRespawnPositionEvent`).
pub struct RespawnPosition {
    pub position: Vector3<f64>,
    pub yaw: f32,
    pub pitch: f32,
    pub dimension: Dimension,
}

/// Fires [`PlayerSpawnLocationEvent`] for a respawn and returns the position to use. When a
/// handler sets `copy_original_spawn_position` to `false`, the player loses its respawn point.
/// The flag starts as `true` when the player still has a respawn point. A cancel keeps the
/// vanilla position and the respawn point.
pub async fn respawn_position(
    server: Option<&Arc<Server>>,
    player: &Arc<Player>,
    original: RespawnPosition,
    from_end_fight: bool,
) -> RespawnPosition {
    let Some(server) = server else {
        return original;
    };
    let has_respawn_point = || {
        player
            .respawn_point
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    };
    let Some(event) = server
        .plugin_manager
        .fire_with(server, || {
            PlayerSpawnLocationEvent::new(
                player.clone(),
                original.position,
                original.yaw,
                original.pitch,
                original.dimension.clone(),
                from_end_fight,
                has_respawn_point(),
            )
        })
        .await
    else {
        return original;
    };
    if event.cancelled {
        return original;
    }
    if !event.copy_original_spawn_position {
        *player
            .respawn_point
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
    }
    RespawnPosition {
        position: event.spawn_pos,
        yaw: event.yaw,
        pitch: event.pitch,
        dimension: event.dimension,
    }
}

/// Fires [`PlayerEventLoadFromFile`] for a player whose data the server loaded.
pub fn load_from_file(server: &Arc<Server>, player: &Arc<Player>) {
    let _ = server.plugin_manager.fire_blocking_with(server, || {
        PlayerEventLoadFromFile::new(
            player.clone(),
            server.player_data_storage.data_path().to_path_buf(),
            player.gameprofile.id.to_string(),
        )
    });
}

/// Fires [`PlayerEventSaveToFile`] for a player whose data the server saves to `directory`. The
/// player of a disconnect is no longer in the player list, so the caller passes its handle.
pub fn save_to_file(player: &Arc<Player>, directory: &std::path::Path) {
    let _ = fire_shared(player, |player| {
        let uuid = player.gameprofile.id.to_string();
        PlayerEventSaveToFile::new(player, directory.to_path_buf(), uuid)
    });
}

type Guard<'a> = MutexGuard<'a, dyn ScreenHandler + 'static>;
type Handler = Mutex<dyn ScreenHandler + 'static>;

fn lock(handler: &Handler) -> Guard<'_> {
    handler.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Whether `handler` is still the open screen of `player`.
fn is_open(player: &Player, handler: &Handler) -> bool {
    let current = player
        .current_screen_handler
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    std::ptr::addr_eq(Arc::as_ptr(&current), std::ptr::from_ref(handler))
}

/// Whether a handler listens to [`AnvilCraftEventPre`] or [`AnvilCraftEventPost`]
/// (`InventoryPlayer::listens_to_anvil_craft`).
pub fn listens_to_anvil_craft(player: &Player) -> bool {
    player.world().server.upgrade().is_some_and(|server| {
        server.plugin_manager.has_handlers::<AnvilCraftEventPre>()
            || server.plugin_manager.has_handlers::<AnvilCraftEventPost>()
    })
}

/// Fires [`AnvilCraftEventPre`] and [`AnvilCraftEventPost`] for the take of the anvil result
/// that the last click on `handler` left pending (`AnvilMenu#onTake` of `NeoForge`). It
/// releases `guard` while the handlers run and returns the guard it locks again.
///
/// After a Pre without a cancel, the anvil pays the levels and uses the inputs only when it is
/// still the open screen of `player` and its inputs did not change; then the anvil update event
/// of the new inputs and the Post fire. A cancel, a closed anvil or changed inputs leave the
/// inputs to the handlers, and the Post does not fire.
pub fn fire_anvil_take<'a>(
    player: &Arc<Player>,
    handler: &'a Handler,
    guard: Guard<'a>,
) -> Guard<'a> {
    let Some(take) = guard
        .as_any()
        .downcast_ref::<AnvilScreenHandler>()
        .and_then(AnvilScreenHandler::take_craft_event)
    else {
        return guard;
    };
    drop(guard);
    let cancelled = fire_shared(player, |shared| {
        AnvilCraftEventPre::new(
            shared,
            take.left.clone(),
            take.right.clone(),
            take.output.clone(),
        )
    })
    .is_some_and(|event| event.cancelled);
    let open = is_open(player, handler);
    let mut guard = lock(handler);
    let Some(anvil) = guard.as_any_mut().downcast_mut::<AnvilScreenHandler>() else {
        return guard;
    };
    let applied = !cancelled && open && anvil.apply_craft_take(&take, player.as_ref());
    if open {
        anvil.update_result(player.as_ref());
        anvil.send_content_updates();
    }
    if !applied {
        return guard;
    }
    drop(crate::plugin::container_events::fire_pending(
        player, handler, guard,
    ));
    let _ = fire_shared(player, |shared| {
        AnvilCraftEventPost::new(shared, take.left, take.right, take.output)
    });
    lock(handler)
}

/// Fires [`TradeWithVillagerEvent`] for the offer `offer_index` of the merchant
/// `merchant_id`, which `player_uuid` used. The trade runs under the lock of the trade screen,
/// so the event fires after the active [`Deferral`] (the slot click).
pub fn trade_with_villager(
    world: &Arc<World>,
    player_uuid: Uuid,
    merchant_id: i32,
    offers: &Mutex<Vec<MerchantOffer>>,
    offer_index: usize,
) {
    let Some(server) = world.server.upgrade() else {
        return;
    };
    if !server
        .plugin_manager
        .has_handlers::<TradeWithVillagerEvent>()
    {
        return;
    }
    let Some(player) = world.get_player_by_uuid(player_uuid) else {
        return;
    };
    let Some(offer) = offers
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(offer_index)
        .cloned()
    else {
        return;
    };
    let Some(merchant) = world.get_entity_by_id(merchant_id) else {
        return;
    };
    defer(move || {
        fire_event(
            &server,
            TradeWithVillagerEvent::new(player, offer, merchant),
        );
    });
}

/// Fires [`ClientInformationUpdatedEvent`] after `player` stored new client settings.
pub fn client_information_updated(
    server: &Arc<Server>,
    player: &Arc<Player>,
    old: &Arc<PlayerConfig>,
) {
    let _ = server.plugin_manager.fire_blocking_with(server, || {
        ClientInformationUpdatedEvent::new(player.clone(), old.clone(), player.config.load_full())
    });
}

/// Fires [`PlayerFlyableFallEvent`] when `caller` is a player.
pub fn flyable_fall(caller: &dyn EntityBase, distance: f32, multiplier: f32) {
    if let Some(player) = caller.get_player() {
        let _ = fire(player, |player| {
            PlayerFlyableFallEvent::new(player, distance, multiplier)
        });
    }
}
