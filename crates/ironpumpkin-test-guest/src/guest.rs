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
    event::{EventPriority, EventType},
    gametest::{
        AsyncTestCallbackId, BlockPermutation, BlockPredicateCallbackId, EntityPredicateCallbackId,
        Test, TestCallbackId, VoidCallbackId,
    },
    neoforge_events::MobEffectEventApplicableResult,
};

/// The events of the table "Mutators and their events" of
/// `openspec/changes/neoforge-shaped-plugin-api/design.md`.
const EVENTS: [EventType; 28] = [
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
        _ => return None,
    })
}

/// What one handler does with one event: on its fire number `nth` only, or on every fire.
#[derive(Default)]
struct Action {
    nth: Option<u32>,
    value: Option<String>,
    cancel: Option<bool>,
}

#[derive(Default)]
struct State {
    context: Option<Context>,
    actions: HashMap<(String, u32), Action>,
    fires: HashMap<(String, u32), u32>,
    log: Vec<String>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

/// Logs what handler `slot` sees, then runs its action.
fn handle(slot: u32, event: &mut Event) -> Result<(), String> {
    let Some((name, mut field, mut cancelled)) = view(event) else {
        return Ok(());
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
            return Ok(());
        };
        if action.nth.is_some_and(|nth| nth != fire) {
            return Ok(());
        }
        if let (Some(value), Some(field)) = (&action.value, field.as_mut()) {
            field.assign(value)?;
        }
        if let (Some(cancel), Some(cancelled)) = (action.cancel, cancelled.as_mut()) {
            **cancelled = cancel;
        }
        Ok(())
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
            _ => return Err(format!("not a setting: {setting}")),
        }
    }
    let slot = slot.checked_sub(1).ok_or("the slots start at 1")?;
    STATE.with_borrow_mut(|state| state.actions.insert(((*event).to_owned(), slot), action));
    Ok(())
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

struct Component;

impl Guest for Component {
    fn init_plugin() {}

    async fn on_load(context: Context) -> Result<(), String> {
        for event in EVENTS {
            context.register_event(0, event, EventPriority::High, true);
            context.register_event(1, event, EventPriority::Normal, true);
        }
        STATE.with_borrow_mut(|state| state.context = Some(context));
        Ok(())
    }

    async fn on_unload(_context: Context) -> Result<(), String> {
        Ok(())
    }

    async fn handle_event(event_id: u32, _server: ServerInstance, mut event: Event) -> Event {
        if let Err(error) = handle(event_id, &mut event) {
            STATE.with_borrow_mut(|state| state.log.push(format!("error {error}")));
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
    /// log) and `spawn ...` (see [`spawn`]).
    async fn handle_ipc_message(_sender: String, message: Vec<u8>) -> Result<Vec<u8>, String> {
        let message = String::from_utf8(message).map_err(|error| error.to_string())?;
        let words: Vec<&str> = message.split_whitespace().collect();
        let reply = match words.as_slice() {
            ["reset"] => {
                STATE.with_borrow_mut(|state| {
                    state.actions.clear();
                    state.fires.clear();
                    state.log.clear();
                });
                String::new()
            }
            ["on", rest @ ..] => {
                configure(rest)?;
                String::new()
            }
            ["log"] => STATE.with_borrow_mut(|state| std::mem::take(&mut state.log).join("\n")),
            ["spawn", rest @ ..] => spawn(rest).await?,
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
