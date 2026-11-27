// The guest of both build targets. The including file defines `PLUGIN_NAME`, because the v0.2
// host reads the name once, from `get-metadata`, before any call that could configure it.

#[allow(
    clippy::collection_is_never_read,
    clippy::large_stack_frames,
    clippy::same_length_and_capacity,
    clippy::too_many_arguments,
    reason = "the code of wit-bindgen"
)]
mod bindings {
    wit_bindgen::generate!({
        path: "../pumpkin-plugin-wit/v0.2",
        world: "plugin",
    });
}

use std::{cell::RefCell, collections::HashMap};

use bindings::{
    ChunkBuffer, CommandError, CommandSender, CommandSuggestions, ConsumedArgs, Context, Entity,
    Event, GenerationPhase, Guest, ServerInstance, SuggestionRequest,
    exports::pumpkin::plugin::{gametest_callbacks, metadata},
};
use bindings::pumpkin::plugin::{
    common::BlockPos,
    entity_types::EntityType,
    event::{EventPriority, EventType, RawPacket},
    item_stack::ItemStack,
    gametest::{
        AsyncTestCallbackId, BlockPermutation, BlockPredicateCallbackId, EntityPredicateCallbackId,
        Test, TestCallbackId, VoidCallbackId,
    },
    game_rules::{GameRule, GameRuleValue},
    neoforge_events::MobEffectEventApplicableResult,
    player::{
        BanPlayerOptions, JavaKickOptions, KnownServerLink, ServerLink, ServerLinkLabel,
        SocketTeardownPolicy,
    },
    sounds::{Sound, SoundCategory},
    statistics::{CustomStatistic, StatisticCategory},
    status_effect::StatusEffectType,
    world::{BlockDirection, EquipmentSlot, MobData, ShulkerData},
};
use bindings::pumpkin::plugin::{
    block_entity::BlockEntityType, command::Command, text::TextComponent,
};

/// The events of the table "Mutators and their events" of
/// `openspec/changes/neoforge-shaped-plugin-api/design.md`.
const EVENTS: [EventType; 32] = [
    EventType::CommandEvent,
    EventType::BlockBreakEvent,
    EventType::BlockPlaceEvent,
    EventType::BlockEventEntityPlaceEvent,
    EventType::EntitySpawnEvent,
    EventType::CreatureSpawnEvent,
    EventType::ItemSpawnEvent,
    EventType::LightningStrikeEvent,
    EventType::EntityExplodeEvent,
    EventType::ExplosionEventDetonate,
    EventType::ExplosionKnockbackEvent,
    EventType::EntityInvulnerabilityCheckEvent,
    EventType::EntityDamageEvent,
    EventType::EntityDamageByEntityEvent,
    EventType::EntityDamageByBlockEvent,
    EventType::LivingShieldBlockEvent,
    EventType::LivingKnockBackEvent,
    EventType::LivingDamageEventPre,
    EventType::LivingDamageEventPost,
    EventType::EntityDeathEvent,
    EventType::PlayerDeathEvent,
    EventType::LivingExperienceDropEvent,
    EventType::EntityRegainHealthEvent,
    EventType::EntityLeaveLevelEvent,
    EventType::LivingEntityUseItemEventStart,
    EventType::LivingEntityUseItemEventStop,
    EventType::ArrowLooseEvent,
    EventType::MobEffectEventApplicable,
    EventType::EntityPotionEffectEvent,
    EventType::PlayLevelSoundEventAtPosition,
    EventType::ItemAttributeModifierEvent,
    EventType::GetEnchantmentLevelEvent,
];

/// The events that synchronous imports fire, through the queue of the host. Only the first
/// handler listens, and it logs only while the test arms it: other tests also fire these events.
const IMPORT_EVENTS: [EventType; 13] = [
    EventType::PlayerOpenSignEvent,
    EventType::PlayerVelocityEvent,
    EventType::EntityKnockbackEvent,
    EventType::PlayerLinksSendEvent,
    EventType::PlayerStatisticIncrementEvent,
    EventType::GameRuleChangedEvent,
    EventType::GenericGameEvent,
    EventType::ChunkLoadEvent,
    EventType::EffectParticleModificationEvent,
    EventType::PacketSentEvent,
    EventType::PlayerEventHarvestCheck,
    EventType::PlayerEventTabListNameFormat,
    EventType::PlayerPermissionCheckEvent,
];

/// A field that the test reads and sets as text, the same text as the native side.
trait Field {
    fn show(&self) -> String;
    fn assign(&mut self, value: &str) -> Result<(), String>;
}

macro_rules! parsed_field {
    ($($ty:ty),*) => {$(
        impl Field for $ty {
            fn show(&self) -> String {
                self.to_string()
            }

            fn assign(&mut self, value: &str) -> Result<(), String> {
                *self = value.parse().map_err(|_| format!("not a value: {value}"))?;
                Ok(())
            }
        }
    )*};
}

parsed_field!(bool, u8, u32, i32, f32);

/// A sound event name, or `none`.
impl Field for Option<String> {
    fn show(&self) -> String {
        self.clone().unwrap_or_else(|| "none".to_owned())
    }

    fn assign(&mut self, value: &str) -> Result<(), String> {
        *self = (value != "none").then(|| value.to_owned());
        Ok(())
    }
}

/// The affected blocks of an explosion: the length, and a truncation.
impl Field for Vec<BlockPos> {
    fn show(&self) -> String {
        self.len().to_string()
    }

    fn assign(&mut self, value: &str) -> Result<(), String> {
        self.truncate(value.parse().map_err(|_| format!("not a length: {value}"))?);
        Ok(())
    }
}

/// A knockback: the vertical part, and a push straight up.
impl Field for (f64, f64, f64) {
    fn show(&self) -> String {
        self.1.to_string()
    }

    fn assign(&mut self, value: &str) -> Result<(), String> {
        *self = (
            0.0,
            value.parse().map_err(|_| format!("not a value: {value}"))?,
            0.0,
        );
        Ok(())
    }
}

impl Field for MobEffectEventApplicableResult {
    fn show(&self) -> String {
        match self {
            Self::Apply => "apply",
            Self::Default => "default",
            Self::DoNotApply => "do-not-apply",
        }
        .to_owned()
    }

    fn assign(&mut self, value: &str) -> Result<(), String> {
        *self = match value {
            "apply" => Self::Apply,
            "default" => Self::Default,
            "do-not-apply" => Self::DoNotApply,
            _ => return Err(format!("not a result: {value}")),
        };
        Ok(())
    }
}

/// The name of an event, the field that the test changes, and the cancel flag.
type View<'a> = (&'static str, Option<&'a mut dyn Field>, Option<&'a mut bool>);

#[expect(clippy::too_many_lines, reason = "one arm for each event of the table")]
fn view(event: &mut Event) -> Option<View<'_>> {
    Some(match event {
        Event::BlockBreakEvent(data) => (
            "BlockBreakEvent",
            Some(&mut data.exp),
            Some(&mut data.cancelled),
        ),
        Event::CommandEvent(data) => ("CommandEvent", None, Some(&mut data.cancelled)),
        Event::BlockPlaceEvent(data) => ("BlockPlaceEvent", None, Some(&mut data.cancelled)),
        Event::BlockEventEntityPlaceEvent(data) => (
            "BlockEventEntityPlaceEvent",
            None,
            Some(&mut data.cancelled),
        ),
        Event::EntitySpawnEvent(data) => ("EntitySpawnEvent", None, Some(&mut data.cancelled)),
        Event::CreatureSpawnEvent(data) => {
            ("CreatureSpawnEvent", None, Some(&mut data.cancelled))
        }
        Event::ItemSpawnEvent(data) => ("ItemSpawnEvent", None, Some(&mut data.cancelled)),
        Event::LightningStrikeEvent(data) => {
            ("LightningStrikeEvent", None, Some(&mut data.cancelled))
        }
        Event::EntityExplodeEvent(data) => (
            "EntityExplodeEvent",
            Some(&mut data.yield_rate),
            Some(&mut data.cancelled),
        ),
        Event::ExplosionEventDetonate(data) => (
            "ExplosionEventDetonate",
            Some(&mut data.affected_blocks),
            None,
        ),
        Event::ExplosionKnockbackEvent(data) => {
            ("ExplosionKnockbackEvent", Some(&mut data.knockback), None)
        }
        Event::EntityInvulnerabilityCheckEvent(data) => (
            "EntityInvulnerabilityCheckEvent",
            Some(&mut data.invulnerable),
            None,
        ),
        Event::EntityDamageEvent(data) => (
            "EntityDamageEvent",
            Some(&mut data.damage),
            Some(&mut data.cancelled),
        ),
        Event::EntityDamageByEntityEvent(data) => (
            "EntityDamageByEntityEvent",
            Some(&mut data.damage),
            Some(&mut data.cancelled),
        ),
        Event::EntityDamageByBlockEvent(data) => (
            "EntityDamageByBlockEvent",
            Some(&mut data.damage),
            Some(&mut data.cancelled),
        ),
        Event::LivingShieldBlockEvent(data) => {
            ("LivingShieldBlockEvent", Some(&mut data.blocked_damage), None)
        }
        Event::LivingKnockBackEvent(data) => (
            "LivingKnockBackEvent",
            Some(&mut data.strength),
            Some(&mut data.cancelled),
        ),
        Event::LivingDamageEventPre(data) => {
            ("LivingDamageEventPre", Some(&mut data.new_damage), None)
        }
        Event::LivingDamageEventPost(_) => ("LivingDamageEventPost", None, None),
        Event::EntityDeathEvent(data) => ("EntityDeathEvent", Some(&mut data.dropped_exp), None),
        Event::PlayerDeathEvent(data) => (
            "PlayerDeathEvent",
            Some(&mut data.dropped_exp),
            Some(&mut data.cancelled),
        ),
        Event::LivingExperienceDropEvent(data) => (
            "LivingExperienceDropEvent",
            Some(&mut data.dropped_experience),
            Some(&mut data.cancelled),
        ),
        Event::EntityRegainHealthEvent(data) => (
            "EntityRegainHealthEvent",
            Some(&mut data.amount),
            Some(&mut data.cancelled),
        ),
        Event::EntityLeaveLevelEvent(_) => ("EntityLeaveLevelEvent", None, None),
        Event::LivingEntityUseItemEventStart(data) => (
            "LivingEntityUseItemEventStart",
            Some(&mut data.duration),
            Some(&mut data.cancelled),
        ),
        Event::LivingEntityUseItemEventStop(data) => (
            "LivingEntityUseItemEventStop",
            None,
            Some(&mut data.cancelled),
        ),
        Event::ArrowLooseEvent(data) => (
            "ArrowLooseEvent",
            Some(&mut data.charge),
            Some(&mut data.cancelled),
        ),
        Event::MobEffectEventApplicable(data) => {
            ("MobEffectEventApplicable", Some(&mut data.result), None)
        }
        Event::EntityPotionEffectEvent(data) => (
            "EntityPotionEffectEvent",
            Some(&mut data.amplifier),
            Some(&mut data.cancelled),
        ),
        Event::PlayLevelSoundEventAtPosition(data) => (
            "PlayLevelSoundEventAtPosition",
            Some(&mut data.sound),
            Some(&mut data.cancelled),
        ),
        Event::ItemAttributeModifierEvent(_) => ("ItemAttributeModifierEvent", None, None),
        Event::GetEnchantmentLevelEvent(_) => ("GetEnchantmentLevelEvent", None, None),
        _ => return import_view(event),
    })
}

/// The arms of [`view`] for [`IMPORT_EVENTS`].
fn import_view(event: &mut Event) -> Option<View<'_>> {
    Some(match event {
        Event::PlayerOpenSignEvent(data) => ("PlayerOpenSignEvent", None, Some(&mut data.cancelled)),
        Event::PlayerVelocityEvent(data) => {
            ("PlayerVelocityEvent", Some(&mut data.velocity), Some(&mut data.cancelled))
        }
        Event::EntityKnockbackEvent(data) => {
            ("EntityKnockbackEvent", Some(&mut data.knockback), Some(&mut data.cancelled))
        }
        Event::PlayerLinksSendEvent(data) => {
            ("PlayerLinksSendEvent", None, Some(&mut data.cancelled))
        }
        Event::PlayerStatisticIncrementEvent(data) => (
            "PlayerStatisticIncrementEvent",
            Some(&mut data.amount),
            Some(&mut data.cancelled),
        ),
        Event::GameRuleChangedEvent(_) => ("GameRuleChangedEvent", None, None),
        Event::GenericGameEvent(data) => ("GenericGameEvent", None, Some(&mut data.cancelled)),
        Event::ChunkLoadEvent(data) => ("ChunkLoadEvent", None, Some(&mut data.cancelled)),
        Event::EffectParticleModificationEvent(_) => ("EffectParticleModificationEvent", None, None),
        Event::PacketSentEvent(data) => {
            ("PacketSentEvent", Some(&mut data.packet_id), Some(&mut data.cancelled))
        }
        Event::PlayerEventHarvestCheck(data) => {
            ("PlayerEventHarvestCheck", Some(&mut data.can_harvest), None)
        }
        Event::PlayerEventTabListNameFormat(_) => ("PlayerEventTabListNameFormat", None, None),
        Event::PlayerPermissionCheckEvent(_) => ("PlayerPermissionCheckEvent", None, None),
        _ => return None,
    })
}

/// What one handler does with one event: on its fire number `nth` only, or on every fire.
#[derive(Default)]
struct Action {
    nth: Option<u32>,
    value: Option<String>,
    cancel: Option<bool>,
    /// Damages a golden sword through the v0.2 `damage-item` import, which reads its enchantment
    /// levels on the server.
    damage: bool,
    /// Plays a stone sound and an anvil sound through the v0.2 world import, which defers them.
    sounds: bool,
}

#[derive(Default)]
struct State {
    context: Option<Context>,
    actions: HashMap<(String, u32), Action>,
    /// Whether a handler of the sound event plays one more sound.
    replay: bool,
    /// Whether the handler of [`IMPORT_EVENTS`] logs.
    armed: bool,
    /// `(id, extra)`: the handler of `PacketSentEvent` cancels the packets with the id `id` and
    /// adds a packet with the id `extra` after each.
    packet_rule: Option<(i32, i32)>,
    /// Whether the handler of `PacketSentEvent` answers a packet with the text `reply-me` with a
    /// title.
    packet_reply: bool,
    /// `(on, reply)`: the handler of `PacketSentEvent` answers a packet with the text `on` with a
    /// message `reply`.
    relay: Option<(String, String)>,
    fires: HashMap<(String, u32), u32>,
    log: Vec<String>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

/// Logs what handler `slot` sees, then runs its action.
fn handle(slot: u32, event: &mut Event) -> Result<(bool, bool), String> {
    let (armed, packet_rule, packet_reply, relay) = STATE.with_borrow(|state| {
        (
            state.armed,
            state.packet_rule,
            state.packet_reply,
            state.relay.clone(),
        )
    });
    if let (Some((on, reply)), Event::PacketSentEvent(data)) = (&relay, &*event)
        && data
            .raw_payload
            .windows(on.len())
            .any(|window| window == on.as_bytes())
    {
        data.player
            .send_system_message(TextComponent::text(reply), false);
    }
    if !armed && import_view(event).is_some() {
        return Ok((false, false));
    }
    if let (true, Event::PacketSentEvent(data)) = (packet_reply, &*event)
        && data.raw_payload.windows(8).any(|window| window == b"reply-me")
    {
        data.player.show_title(TextComponent::text("replied"));
    }
    if let (Some((id, extra)), Event::PacketSentEvent(data)) = (packet_rule, &mut *event)
        && data.packet_id == id
    {
        data.cancelled = true;
        data.extra_packets.push(RawPacket {
            packet_id: extra,
            payload: vec![7],
        });
    }
    let Some((name, mut field, mut cancelled)) = view(event) else {
        return Ok((false, false));
    };
    STATE.with_borrow_mut(|state| {
        let key = (name.to_owned(), slot);
        let fire = state.fires.entry(key.clone()).or_default();
        *fire += 1;
        let fire = *fire;
        state.log.push(format!(
            "{name} W{} {} {}",
            slot + 1,
            field.as_ref().map_or_else(|| "-".to_owned(), |field| field.show()),
            cancelled
                .as_ref()
                .map_or_else(|| "-".to_owned(), ToString::to_string),
        ));
        let Some(action) = state.actions.get(&key) else {
            return Ok((false, false));
        };
        if action.nth.is_some_and(|nth| nth != fire) {
            return Ok((false, false));
        }
        if let (Some(value), Some(field)) = (&action.value, field.as_mut()) {
            field.assign(value)?;
        }
        if let (Some(cancel), Some(cancelled)) = (action.cancel, cancelled.as_mut()) {
            **cancelled = cancel;
        }
        Ok((action.damage, action.sounds))
    })
}

/// `on <event> <slot> [nth=<n>] [value=<v>] [cancel=<bool>]`: the action of handler `slot`.
fn configure(words: &[&str]) -> Result<(), String> {
    let [event, slot, settings @ ..] = words else {
        return Err("on <event> <slot> [nth=<n>] [value=<v>] [cancel=<bool>]".to_owned());
    };
    let slot: u32 = slot.parse().map_err(|_| format!("not a slot: {slot}"))?;
    let mut action = Action::default();
    for setting in settings {
        match setting.split_once('=') {
            Some(("nth", nth)) => action.nth = Some(nth.parse().map_err(|_| nth.to_owned())?),
            Some(("value", value)) => action.value = Some(value.to_owned()),
            Some(("cancel", cancel)) => {
                action.cancel = Some(cancel.parse().map_err(|_| cancel.to_owned())?);
            }
            Some(("damage", damage)) => {
                action.damage = damage.parse().map_err(|_| damage.to_owned())?;
            }
            Some(("sounds", sounds)) => {
                action.sounds = sounds.parse().map_err(|_| sounds.to_owned())?;
            }
            _ => return Err(format!("not a setting: {setting}")),
        }
    }
    let slot = slot.checked_sub(1).ok_or("the slots start at 1")?;
    STATE.with_borrow_mut(|state| state.actions.insert(((*event).to_owned(), slot), action));
    Ok(())
}

/// `perform <player> <command...>`: the player performs a command through the v0.2 player
/// import, and the reply is what the import returns.
async fn perform(words: &[&str]) -> Result<String, String> {
    let [name, command @ ..] = words else {
        return Err("perform <player> <command...>".to_owned());
    };
    let server = STATE
        .with_borrow(|state| state.context.as_ref().map(Context::get_server))
        .ok_or("no context")?;
    let player = server
        .get_player_by_name(name)
        .ok_or_else(|| format!("no player {name}"))?;
    Ok(player.perform_command(command.join(" ")).await.to_string())
}

/// `spawn <x> <y> <z>`: spawns a pig through the v0.2 world import, and returns its entity id.
async fn spawn(words: &[&str]) -> Result<String, String> {
    let [x, y, z] = words else {
        return Err("spawn <x> <y> <z>".to_owned());
    };
    let coordinate = |text: &str| text.parse::<f64>().map_err(|_| format!("not a number: {text}"));
    let position = (coordinate(x)?, coordinate(y)?, coordinate(z)?);
    let server = STATE
        .with_borrow(|state| state.context.as_ref().map(Context::get_server))
        .ok_or("no context")?;
    let world = server
        .get_all_worlds()
        .into_iter()
        .next()
        .ok_or("no world")?;
    let pig = world.spawn_entity(EntityType::Pig, position).await;
    Ok(pig.get_id().to_string())
}

/// `sound <x> <y> <z>`: plays a stone sound through the v0.2 world import.
async fn sound(words: &[&str]) -> Result<String, String> {
    let [x, y, z] = words else {
        return Err("sound <x> <y> <z>".to_owned());
    };
    let coordinate = |text: &str| text.parse::<f64>().map_err(|_| format!("not a number: {text}"));
    let position = (coordinate(x)?, coordinate(y)?, coordinate(z)?);
    let server = STATE
        .with_borrow(|state| state.context.as_ref().map(Context::get_server))
        .ok_or("no context")?;
    let world = server
        .get_all_worlds()
        .into_iter()
        .next()
        .ok_or("no world")?;
    world.play_sound(Sound::BlockStoneBreak, SoundCategory::Players, position, 1.0, 1.0);
    Ok(String::new())
}

/// `order <player> <x> <y> <z>`: in one call, plays a stone sound and an anvil sound through the
/// world, stops the stone sound for the player, and plays a pig sound to the player.
async fn order(words: &[&str]) -> Result<String, String> {
    let [name, x, y, z] = words else {
        return Err("order <player> <x> <y> <z>".to_owned());
    };
    let coordinate = |text: &str| text.parse::<f64>().map_err(|_| format!("not a number: {text}"));
    let position = (coordinate(x)?, coordinate(y)?, coordinate(z)?);
    let server = STATE
        .with_borrow(|state| state.context.as_ref().map(Context::get_server))
        .ok_or("no context")?;
    let world = server
        .get_all_worlds()
        .into_iter()
        .next()
        .ok_or("no world")?;
    let player = server
        .get_player_by_name(name)
        .ok_or_else(|| format!("no player {name}"))?;
    world.play_sound(Sound::BlockStoneBreak, SoundCategory::Players, position, 1.0, 1.0);
    world.play_sound(Sound::BlockAnvilLand, SoundCategory::Players, position, 1.0, 1.0);
    player.stop_sound(Some(Sound::BlockStoneBreak), None);
    player.play_sound(Sound::EntityPigAmbient, SoundCategory::Players, 1.0, 1.0);
    Ok(String::new())
}

/// `equip <x> <y> <z>`: spawns a pig and sets a golden sword in its main hand through the v0.2
/// `set-equipment` import, which reads the attribute modifiers of the sword and so fires
/// `ItemAttributeModifierEvent` at the handlers of this plugin. Returns the entity id.
async fn equip(words: &[&str]) -> Result<String, String> {
    let [x, y, z] = words else {
        return Err("equip <x> <y> <z>".to_owned());
    };
    let coordinate = |text: &str| text.parse::<f64>().map_err(|_| format!("not a number: {text}"));
    let position = (coordinate(x)?, coordinate(y)?, coordinate(z)?);
    let server = STATE
        .with_borrow(|state| state.context.as_ref().map(Context::get_server))
        .ok_or("no context")?;
    let world = server
        .get_all_worlds()
        .into_iter()
        .next()
        .ok_or("no world")?;
    let pig = world.spawn_entity(EntityType::Pig, position).await;
    let living = pig.as_living().ok_or("a pig is not living")?;
    living
        .set_equipment(
            EquipmentSlot::MainHand,
            Some(ItemStack::new("minecraft:golden_sword", 1)),
        )
        .await;
    Ok(pig.get_id().to_string())
}

/// `ordered <case> <name>`: synchronous and asynchronous imports that send packets to player
/// `name` in one guest call. The packets must reach the client in call order.
async fn ordered(words: &[&str]) -> Result<String, String> {
    let [case, name, ..] = words else {
        return Err("ordered <case> <name>".to_owned());
    };
    let server = STATE
        .with_borrow(|state| state.context.as_ref().map(Context::get_server))
        .ok_or("no context")?;
    let player = server
        .get_player_by_name(name)
        .ok_or_else(|| format!("no player {name}"))?;
    let say = |text: &str| player.send_system_message(TextComponent::text(text), false);
    match *case {
        // A message, then a ban that kicks the player: the message arrives before the kick.
        "ban" => {
            say("ordered-banned");
            player
                .ban(BanPlayerOptions {
                    reason: Some(TextComponent::text("ordered-ban-reason")),
                    source: None,
                    expires_at_utc: None,
                    duration_seconds: None,
                    kick_if_online: true,
                    log_to_console: false,
                })
                .await;
        }
        // A synchronous message, an asynchronous broadcast, a synchronous message.
        "broadcast" => {
            say("ordered-one");
            server.broadcast("ordered-two".to_owned()).await;
            say("ordered-three");
        }
        // A kick of the player, then a game rule change `value`: the change does not wait for
        // the flush of the disconnect packet.
        "kick" => {
            let value = words.get(2) == Some(&"true");
            let java = player.as_java().ok_or("not a java player")?;
            java.kick(JavaKickOptions {
                reason: TextComponent::text("ordered-kick"),
                log_to_console: false,
                teardown_policy: SocketTeardownPolicy::Graceful,
            })
            .await;
            let world = server
                .get_all_worlds()
                .into_iter()
                .next()
                .ok_or("no world")?;
            world.set_game_rule(GameRule::SpawnPhantoms, GameRuleValue::Bool(value));
        }
        // `count` messages `bulk-0` to `bulk-<count - 1>`.
        "bulk" => {
            let count: u32 = words
                .get(2)
                .and_then(|count| count.parse().ok())
                .ok_or("bulk <name> <count>")?;
            for index in 0..count {
                say(&format!("bulk-{index};"));
            }
        }
        // The message `text`.
        "say" => say(words.get(2).ok_or("say <name> <text>")?),
        // A message, then a kick that drops the connection without a disconnect packet.
        "drop-kick" => {
            say("drop-kick-message");
            let java = player.as_java().ok_or("not a java player")?;
            java.kick(JavaKickOptions {
                reason: TextComponent::text("drop-kick"),
                log_to_console: false,
                teardown_policy: SocketTeardownPolicy::DropConnection,
            })
            .await;
        }
        // A message that the handler of `PacketSentEvent` answers with a title, after `packet-reply`.
        "reply" => say("reply-me"),
        // `container <x> <y> <z>`: a clear, then a stack in slot 0, of the container at the
        // position.
        "container" => {
            let number = |index: usize| {
                words
                    .get(index)
                    .and_then(|text| text.parse::<i32>().ok())
                    .ok_or("container <name> <x> <y> <z>")
            };
            let pos = BlockPos {
                x: number(2)?,
                y: number(3)?,
                z: number(4)?,
            };
            let world = server
                .get_all_worlds()
                .into_iter()
                .next()
                .ok_or("no world")?;
            let container = match world.get_block_entity(pos) {
                Some(BlockEntityType::ChestBlockEntity(chest)) => chest.get_container(),
                Some(BlockEntityType::JukeboxBlockEntity(jukebox)) => {
                    // A stop, then a start: the jukebox plays at the end.
                    jukebox.stop_playing();
                    jukebox.start_playing(1000);
                    return Ok(String::new());
                }
                _ => return Err("no container".to_owned()),
            };
            container.clear();
            container
                .set_stack(0, Some(ItemStack::new("minecraft:stone", 1)))
                .await;
        }
        _ => return Err(format!("not an ordered case: {case}")),
    }
    Ok(String::new())
}

/// `import <name> <args...>`: calls one group of synchronous imports in one guest call. Each
/// import fires an event that a Wasm handler listens to, so the call hangs unless the host defers
/// the work until the guest call ends.
fn import(words: &[&str]) -> Result<String, String> {
    let number = |text: &str| text.parse::<i32>().map_err(|_| format!("not a number: {text}"));
    let server = STATE
        .with_borrow(|state| state.context.as_ref().map(Context::get_server))
        .ok_or("no context")?;
    let world = server
        .get_all_worlds()
        .into_iter()
        .next()
        .ok_or("no world")?;
    let player = |name: &str| {
        server
            .get_player_by_name(name)
            .ok_or_else(|| format!("no player {name}"))
    };
    let links = || {
        vec![ServerLink {
            label: ServerLinkLabel::Known(KnownServerLink::Support),
            url: "https://example.org".to_owned(),
        }]
    };
    match words {
        ["sign", name, x, y, z] => player(name)?.open_sign_editor(
            BlockPos {
                x: number(x)?,
                y: number(y)?,
                z: number(z)?,
            },
            true,
        ),
        // One guest call sends three packets: a message, a title and a message.
        ["messages", name] => {
            let player = player(name)?;
            player.send_system_message(TextComponent::text("one"), false);
            player.show_title(TextComponent::text("two"));
            player.send_system_message(TextComponent::text("three"), false);
        }
        // A raw packet with the id `id` between two messages, in one guest call.
        ["raw-between", name, id] => {
            let player = player(name)?;
            let java = player.as_java().ok_or("not a java player")?;
            player.send_system_message(TextComponent::text("one"), false);
            java.send_raw_packet(number(id)?, &[9]);
            player.send_system_message(TextComponent::text("three"), false);
        }
        ["harvest", name, x, y, z] => {
            let pos = BlockPos {
                x: number(x)?,
                y: number(y)?,
                z: number(z)?,
            };
            return Ok(player(name)?.can_harvest_block(pos).to_string());
        }
        ["tab-name", name] => {
            return Ok(player(name)?
                .get_tab_list_name()
                .map_or_else(|| "none".to_owned(), |name| name.get_text()));
        }
        ["remove-effect", name] => {
            let living = player(name)?.as_entity().as_living().ok_or("not living")?;
            return Ok(living.remove_effect(StatusEffectType::Speed).to_string());
        }
        ["remove-stack", x, y, z] => {
            let pos = BlockPos {
                x: number(x)?,
                y: number(y)?,
                z: number(z)?,
            };
            let Some(BlockEntityType::JukeboxBlockEntity(jukebox)) = world.get_block_entity(pos)
            else {
                return Err("no jukebox".to_owned());
            };
            let removed = jukebox.get_container().remove_stack(0);
            return Ok(if removed.is_some() { "some" } else { "none" }.to_owned());
        }
        // A peek of a shulker from 0 plays the open sound.
        ["mob-data", id] => {
            let mob = world
                .get_entity_by_id(number(id)?)
                .ok_or("no entity")?
                .as_mob()
                .ok_or("not a mob")?;
            return Ok(mob
                .set_mob_data(MobData::Shulker(ShulkerData {
                    attached_face: BlockDirection::Down,
                    peek_amount: 100,
                    color: None,
                }))
                .to_string());
        }
        ["remove", id] => world
            .get_entity_by_id(number(id)?)
            .ok_or("no entity")?
            .remove(),
        ["clear", x, y, z] => {
            let pos = BlockPos {
                x: number(x)?,
                y: number(y)?,
                z: number(z)?,
            };
            let Some(BlockEntityType::JukeboxBlockEntity(jukebox)) = world.get_block_entity(pos)
            else {
                return Err("no jukebox".to_owned());
            };
            jukebox.get_container().clear();
        }
        ["mining", name, x, y, z] => {
            let pos = BlockPos {
                x: number(x)?,
                y: number(y)?,
                z: number(z)?,
            };
            return Ok(player(name)?.get_mining_speed(pos).to_string());
        }
        // A command that needs a permission, registered while a player is online.
        ["register-command", name] => {
            let command = Command::new(&[(*name).to_owned()], "a test command");
            STATE
                .with_borrow(|state| {
                    state
                        .context
                        .as_ref()
                        .map(|context| context.register_command(command, "test.command"))
                })
                .ok_or("no context")?;
        }
        ["clear-equipment", id] => world
            .get_entity_by_id(number(id)?)
            .ok_or("no entity")?
            .as_living()
            .ok_or("not living")?
            .clear_equipment(),
        ["velocity", name] => player(name)?.set_velocity((0.0, 0.5, 0.0)),
        ["knockback", name] => player(name)?.apply_knockback(0.4, 1.0, 0.0),
        ["links", name] => player(name)?.set_server_links(links()),
        // The set and the increment of each statistic fire in call order: 5, then 2.
        ["statistics", name] => {
            let player = player(name)?;
            player.set_statistic(StatisticCategory::Mined, 1, 5);
            player.increment_statistic(StatisticCategory::Mined, 1, 2);
            player.set_custom_statistic(CustomStatistic::Jump, 5);
            player.increment_custom_statistic(CustomStatistic::Jump, 2);
        }
        ["game-rule", value] => world.set_game_rule(
            GameRule::SpawnPhantoms,
            GameRuleValue::Bool(*value == "true"),
        ),
        ["jukebox", x, y, z] => {
            let pos = BlockPos {
                x: number(x)?,
                y: number(y)?,
                z: number(z)?,
            };
            let Some(BlockEntityType::JukeboxBlockEntity(jukebox)) = world.get_block_entity(pos)
            else {
                return Err("no jukebox".to_owned());
            };
            jukebox.stop_playing();
        }
        ["force", x, z] => world.set_chunk_force_loaded(number(x)?, number(z)?, true),
        ["effects", name] => {
            let player = player(name)?;
            player.remove_effect(StatusEffectType::Speed);
            player.clear_effects();
            player
                .as_entity()
                .as_living()
                .ok_or("not living")?
                .reset_all_attributes();
        }
        _ => return Err(format!("not an import: {words:?}")),
    }
    Ok(String::new())
}

struct Component;

impl Guest for Component {
    fn init_plugin() {}

    async fn on_load(context: Context) -> Result<(), String> {
        for event in EVENTS {
            context.register_event(0, event, EventPriority::High, true);
            context.register_event(1, event, EventPriority::Normal, true);
        }
        for event in IMPORT_EVENTS {
            context.register_event(0, event, EventPriority::High, true);
        }
        STATE.with_borrow_mut(|state| state.context = Some(context));
        Ok(())
    }

    async fn on_unload(_context: Context) -> Result<(), String> {
        Ok(())
    }

    async fn handle_event(event_id: u32, _server: ServerInstance, mut event: Event) -> Event {
        match handle(event_id, &mut event) {
            Ok((damage, sounds)) => {
                if damage {
                    ItemStack::new("minecraft:golden_sword", 1).damage_item(1).await;
                }
                if sounds
                    && let Some(server) = STATE
                        .with_borrow(|state| state.context.as_ref().map(Context::get_server))
                    && let Some(world) = server.get_all_worlds().into_iter().next()
                {
                    let position = (9000.5, 100.0, 9000.5);
                    world.play_sound(Sound::BlockStoneBreak, SoundCategory::Players, position, 1.0, 1.0);
                    world.play_sound(Sound::BlockAnvilLand, SoundCategory::Players, position, 1.0, 1.0);
                }
            }
            Err(error) => STATE.with_borrow_mut(|state| state.log.push(format!("error {error}"))),
        }
        // Only the first handler replays, so each sound causes one more sound.
        if event_id == 0
            && let Event::PlayLevelSoundEventAtPosition(data) = &event
            && STATE.with_borrow(|state| state.replay)
            && let Some(server) = STATE
                .with_borrow(|state| state.context.as_ref().map(Context::get_server))
            && let Some(world) = server.get_all_worlds().into_iter().next()
        {
            world.play_sound(Sound::BlockStoneBreak, SoundCategory::Players, data.position, 1.0, 1.0);
        }
        event
    }

    async fn handle_command(
        _command_id: u32,
        _sender: CommandSender,
        _server: ServerInstance,
        _args: ConsumedArgs,
    ) -> Result<i32, CommandError> {
        Ok(0)
    }

    async fn handle_command_suggestion(
        _handler_id: u32,
        _sender: CommandSender,
        _server: ServerInstance,
        _request: SuggestionRequest,
    ) -> CommandSuggestions {
        CommandSuggestions {
            start: 0,
            length: 0,
            values: Vec::new(),
        }
    }

    async fn handle_task(_handler_id: u32, _server: ServerInstance) {}

    /// The test protocol: `reset`, `on ...` (see [`configure`]), `log` (returns and clears the
    /// log), `spawn ...` (see [`spawn`]), `equip ...` (see [`equip`]) and `perform ...` (see
    /// [`perform`]).
    async fn handle_ipc_message(_sender: String, message: Vec<u8>) -> Result<Vec<u8>, String> {
        let message = String::from_utf8(message).map_err(|error| error.to_string())?;
        let words: Vec<&str> = message.split_whitespace().collect();
        let reply = match words.as_slice() {
            ["reset"] => {
                STATE.with_borrow_mut(|state| {
                    state.actions.clear();
                    state.fires.clear();
                    state.log.clear();
                    state.armed = false;
                    state.packet_rule = None;
                    state.packet_reply = false;
                    state.relay = None;
                });
                String::new()
            }
            ["on", rest @ ..] => {
                configure(rest)?;
                String::new()
            }
            ["log"] => STATE.with_borrow_mut(|state| std::mem::take(&mut state.log).join("\n")),
            ["spawn", rest @ ..] => spawn(rest).await?,
            ["perform", rest @ ..] => perform(rest).await?,
            ["sound", rest @ ..] => sound(rest).await?,
            ["order", rest @ ..] => order(rest).await?,
            ["import", rest @ ..] => import(rest)?,
            ["arm"] => {
                STATE.with_borrow_mut(|state| state.armed = true);
                String::new()
            }
            ["packet-rule", id, extra] => {
                let parse = |text: &str| text.parse::<i32>().map_err(|_| format!("not a number: {text}"));
                let rule = (parse(id)?, parse(extra)?);
                STATE.with_borrow_mut(|state| state.packet_rule = Some(rule));
                String::new()
            }
            ["replay", flag] => {
                STATE.with_borrow_mut(|state| state.replay = *flag == "on");
                String::new()
            }
            ["equip", rest @ ..] => equip(rest).await?,
            ["ordered", rest @ ..] => ordered(rest).await?,
            ["relay", on, reply] => {
                let relay = Some(((*on).to_owned(), (*reply).to_owned()));
                STATE.with_borrow_mut(|state| state.relay = relay);
                String::new()
            }
            ["relay-off"] => {
                STATE.with_borrow_mut(|state| state.relay = None);
                String::new()
            }
            ["packet-reply"] => {
                STATE.with_borrow_mut(|state| state.packet_reply = true);
                String::new()
            }
            _ => return Err(format!("unknown message: {message}")),
        };
        Ok(reply.into_bytes())
    }

    async fn handle_ai_goal_can_start(
        _goal_id: u32,
        _server: ServerInstance,
        _entity: Entity,
    ) -> bool {
        false
    }

    async fn handle_ai_goal_should_continue(
        _goal_id: u32,
        _server: ServerInstance,
        _entity: Entity,
    ) -> bool {
        false
    }

    async fn handle_ai_goal_start(_goal_id: u32, _server: ServerInstance, _entity: Entity) {}

    async fn handle_ai_goal_tick(_goal_id: u32, _server: ServerInstance, _entity: Entity) {}

    async fn handle_ai_goal_stop(_goal_id: u32, _server: ServerInstance, _entity: Entity) {}

    async fn handle_generate_phase(
        _generator_id: u32,
        _phase: GenerationPhase,
        _chunk: ChunkBuffer,
    ) {
    }
}

impl metadata::Guest for Component {
    fn get_metadata() -> metadata::PluginMetadata {
        metadata::PluginMetadata {
            name: PLUGIN_NAME.to_owned(),
            version: "1.0.0".to_owned(),
            authors: Vec::new(),
            description: "Test guest of the cross-channel dispatch tests".to_owned(),
            dependencies: Vec::new(),
            permissions: Vec::new(),
        }
    }
}

impl gametest_callbacks::Guest for Component {
    fn invoke_void(_callback: VoidCallbackId) {}

    fn invoke_test(_callback: TestCallbackId, _test: Test) {}

    async fn invoke_async_test(_callback: AsyncTestCallbackId, _test: Test) {}

    fn invoke_block_predicate(
        _callback: BlockPredicateCallbackId,
        _permutation: BlockPermutation,
    ) -> bool {
        false
    }

    fn invoke_entity_predicate(_callback: EntityPredicateCallbackId, _entity: &Entity) -> bool {
        false
    }
}

bindings::export!(Component with_types_in bindings);
