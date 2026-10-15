//! The `IronPumpkin` fire sites of the `NeoForge` player events (`event.entity.player`).
//!
//! Each function checks `has_handlers` before it builds an event, so a fire site with no
//! handler allocates nothing and returns the vanilla value. The upstream code calls these
//! functions with one line.

use std::sync::{Arc, Mutex, PoisonError};

use dashmap::DashSet;
use pumpkin_data::{BlockState, item_stack::ItemStack};
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
fn listening<E: Payload + 'static>(player: &Player) -> Option<Arc<Server>> {
    let server = player.world().server.upgrade()?;
    server.plugin_manager.has_handlers::<E>().then_some(server)
}

/// The shared handle of `player`, for an event that owns its player.
fn shared(player: &Player) -> Option<Arc<Player>> {
    player.world().get_player_by_uuid(player.gameprofile.id)
}

/// Fires `E` built by `make` when a handler listens, and returns it with the changes.
fn fire<E, F>(player: &Player, make: F) -> Option<E>
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
/// and the knockback, and the hit.
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
    let attack_type = if hit.critical {
        AttackType::Critical
    } else if hit.sweep {
        AttackType::Sweeping
    } else if matches!(attack_type, AttackType::Critical | AttackType::Sweeping) {
        AttackType::Strong
    } else {
        attack_type
    };
    (attack_type, hit)
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

/// Fires [`PlayerSwitchHotbarSlotEventPost`].
pub fn hotbar_post(player: &Player, old_slot: u8, new_slot: u8) {
    let _ = fire(player, |player| {
        PlayerSwitchHotbarSlotEventPost::new(player, old_slot, new_slot)
    });
}

/// Fires [`PlayerEventStartTracking`].
pub fn start_tracking(player: &Arc<Player>, target: &Arc<dyn EntityBase>) {
    let _ = fire(player, |player| {
        PlayerEventStartTracking::new(player, target.clone())
    });
}

/// Fires [`PlayerEventStopTracking`].
pub fn stop_tracking(player: &Player, target: &Arc<dyn EntityBase>) {
    let _ = fire(player, |player| {
        PlayerEventStopTracking::new(player, target.clone())
    });
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
/// The flag starts as `true` when the player still has a respawn point.
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

/// Fires [`PlayerEventSaveToFile`] for a player whose data the server saves to `directory`.
pub fn save_to_file(player: &Player, directory: &std::path::Path) {
    let _ = fire(player, |player| {
        let uuid = player.gameprofile.id.to_string();
        PlayerEventSaveToFile::new(player, directory.to_path_buf(), uuid)
    });
}

/// Fires [`AnvilCraftEventPre`]. Returns `true` for a cancel.
#[must_use]
pub fn anvil_craft_pre(
    player: &Player,
    left: &ItemStack,
    right: &ItemStack,
    output: &ItemStack,
) -> bool {
    fire(player, |player| {
        AnvilCraftEventPre::new(player, left.clone(), right.clone(), output.clone())
    })
    .is_some_and(|event| event.cancelled)
}

/// Fires [`AnvilCraftEventPost`].
pub fn anvil_craft_post(player: &Player, left: &ItemStack, right: &ItemStack, output: &ItemStack) {
    let _ = fire(player, |player| {
        AnvilCraftEventPost::new(player, left.clone(), right.clone(), output.clone())
    });
}

/// Fires [`TradeWithVillagerEvent`] for the offer `offer_index` of the merchant
/// `merchant_id`, which `player_uuid` used.
pub fn trade_with_villager(
    world: &Arc<World>,
    player_uuid: Uuid,
    merchant_id: i32,
    offers: &Mutex<Vec<MerchantOffer>>,
    offer_index: usize,
) {
    let Some(player) = world.get_player_by_uuid(player_uuid) else {
        return;
    };
    if listening::<TradeWithVillagerEvent>(&player).is_none() {
        return;
    }
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
    let _ = fire(&player, |player| {
        TradeWithVillagerEvent::new(player, offer, merchant)
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
