//! The play events of slice 2, as the Java probe traces them (`PlayEvents.java`): the player
//! events, the chat and the ticks that the client script `scripts/slice2.toml` fires. Each line
//! has the field `step`, the index of the script step. The window opens at `PlayerJoinEvent` and
//! closes at `ServerStoppingEvent`, and positions are relative to the block of the player's feet
//! at the join.
//!
//! The step of an event is the number of play `ping_request` packets, the barrier of each script
//! step, that arrived at least [`GRACE`] before it. The Java probe counts the same packet.
//!
//! `BreakBlockEvent` and `BlockEvent.EntityPlaceEvent` are not traced: the level and block events
//! are the task after this one (2.13 in `tasks.md`), so `diff.py` reports them as missing.
//! `PlayerTickEvent` has no Pumpkin event.

use std::{
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use ironpumpkin_mods::{
    ModInit,
    entity::EntityBase,
    event::{
        BoxFuture, EventHandler, EventPriority, Payload,
        player::{
            ClientInformationUpdatedEvent, PlayerEventBreakSpeed, PlayerEventHarvestCheck,
            PlayerEventLoadFromFile, PlayerEventNameFormat, PlayerEventSaveToFile,
            PlayerEventTabListNameFormat, PlayerSwitchHotbarSlotEventPost, UseItemOnBlockEvent,
            item_held::PlayerItemHeldEvent,
            player_chat::PlayerChatEvent,
            player_gamemode_change::PlayerGamemodeChangeEvent,
            player_interact_event::{InteractAction, PlayerInteractEvent},
            player_join::PlayerJoinEvent,
            player_leave::PlayerLeaveEvent,
        },
        server::{
            PacketReceivedEvent, ServerStoppingEvent, ServerTickEndEvent, ServerTickStartEvent,
        },
    },
    math::position::BlockPos,
    pumpkin_data::{
        BlockDirection,
        item_stack::ItemStack,
        packet::{CURRENT_MC_VERSION, serverbound::play::PING_REQUEST},
    },
    pumpkin_util::Hand,
    server::Server,
};

use crate::{Value, trace};

/// The time after a ping at which its step ends. It must stay below the `wait_ms` of the script
/// (1000 ms) and above the time a step takes to fire its events.
const GRACE: Duration = Duration::from_millis(500);

struct State {
    open: bool,
    origin: BlockPos,
    pings: Vec<Instant>,
    /// The step of the running server tick, taken at `ServerTickStartEvent`.
    tick_step: Option<u64>,
    traced_in_step: Vec<(&'static str, u64)>,
}

static STATE: Mutex<State> = Mutex::new(State {
    open: false,
    origin: BlockPos::ZERO,
    pings: Vec::new(),
    tick_step: None,
    traced_in_step: Vec::new(),
});

fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

impl State {
    fn step(&self) -> u64 {
        let Some(limit) = Instant::now().checked_sub(GRACE) else {
            return 0;
        };
        self.pings.iter().filter(|ping| **ping <= limit).count() as u64
    }

    fn relative(&self, pos: &BlockPos) -> String {
        let (pos, origin) = (pos.0, self.origin.0);
        format!(
            "{} {} {}",
            pos.x - origin.x,
            pos.y - origin.y,
            pos.z - origin.z
        )
    }
}

/// Traces `name` with `fields` and the current step, while the window is open. `fields` gets the
/// state, for the relative positions.
fn trace_play(name: &str, fields: impl FnOnce(&State) -> Vec<(&'static str, Value)>) {
    let line = {
        let state = state();
        if !state.open {
            return;
        }
        let mut fields = fields(&state);
        fields.push(("step", Value::Int(state.step())));
        fields
    };
    trace(name, &line);
}

/// Traces `name` the first time it fires in the step of the running tick.
fn trace_once_per_step(name: &'static str) {
    let step = {
        let mut state = state();
        let Some(step) = state.tick_step.filter(|_| state.open) else {
            return;
        };
        if state.traced_in_step.contains(&(name, step)) {
            return;
        }
        state.traced_in_step.push((name, step));
        step
    };
    trace(name, &[("step", Value::Int(step))]);
}

/// A handler that runs `f` on each event `E`.
struct On<E>(fn(&E));

impl<E: Payload + Send + Sync + 'static> EventHandler<E> for On<E> {
    fn handle<'a>(&'a self, _server: &'a Arc<Server>, event: &'a E) -> BoxFuture<'a, ()> {
        Box::pin(async move { (self.0)(event) })
    }
}

fn on<E: Payload + Send + Sync + 'static>(cx: &mut ModInit, f: fn(&E)) {
    cx.register_event::<E, _>(Arc::new(On(f)), EventPriority::Normal, false, false);
}

/// Registers the slice 2 handlers.
pub fn register(cx: &mut ModInit) {
    on::<PacketReceivedEvent>(cx, |event| {
        if event.packet_id == PING_REQUEST.to_id(CURRENT_MC_VERSION) {
            state().pings.push(Instant::now());
        }
    });
    on::<PlayerJoinEvent>(cx, |event| {
        let mut joined = state();
        joined.open = true;
        joined.origin = event.player.get_entity().block_pos.load();
        drop(joined);
        trace_play("PlayerEvent.PlayerLoggedInEvent", |_| Vec::new());
    });
    on::<PlayerLeaveEvent>(cx, |_| {
        trace_play("PlayerEvent.PlayerLoggedOutEvent", |_| Vec::new());
    });
    on::<ServerStoppingEvent>(cx, |_| state().open = false);
    on::<PlayerEventLoadFromFile>(cx, |_| {
        trace_play("PlayerEvent.LoadFromFile", |_| Vec::new());
    });
    on::<PlayerEventSaveToFile>(cx, |_| {
        trace_play("PlayerEvent.SaveToFile", |_| Vec::new());
    });
    on::<PlayerEventNameFormat>(cx, |_| {
        trace_play("PlayerEvent.NameFormat", |_| Vec::new());
    });
    on::<PlayerEventTabListNameFormat>(cx, |_| {
        trace_play("PlayerEvent.TabListNameFormat", |_| Vec::new());
    });
    on::<PlayerGamemodeChangeEvent>(cx, |event| {
        trace_play("PlayerEvent.PlayerChangeGameModeEvent", |_| {
            vec![
                (
                    "current",
                    Value::Text(event.previous_gamemode.name().to_owned()),
                ),
                ("new", Value::Text(event.new_gamemode.name().to_owned())),
            ]
        });
    });
    on::<ClientInformationUpdatedEvent>(cx, |_| {
        trace_play("ClientInformationUpdatedEvent", |_| Vec::new());
    });
    on::<PlayerChatEvent>(cx, |event| {
        trace_play("ServerChatEvent", |_| {
            vec![("raw_text", Value::Text(event.raw_text.clone()))]
        });
    });
    on::<PlayerInteractEvent>(cx, interact);
    on::<UseItemOnBlockEvent>(cx, |event| {
        trace_play("UseItemOnBlockEvent", |state| {
            vec![
                ("face", face(Some(event.face))),
                ("hand", hand(event.hand)),
                ("item", item(&event.item)),
                ("phase", lower(event.phase)),
                ("pos", Value::Text(state.relative(&event.pos))),
            ]
        });
    });
    on::<PlayerItemHeldEvent>(cx, |event| {
        trace_play("PlayerSwitchHotbarSlotEvent.Pre", |_| {
            vec![
                ("new_slot", Value::Int(event.new_slot.into())),
                ("old_slot", Value::Int(event.previous_slot.into())),
            ]
        });
    });
    on::<PlayerSwitchHotbarSlotEventPost>(cx, |event| {
        trace_play("PlayerSwitchHotbarSlotEvent.Post", |_| {
            vec![
                ("new_slot", Value::Int(event.new_slot_index.into())),
                ("old_slot", Value::Int(event.old_slot_index.into())),
            ]
        });
    });
    on::<PlayerEventBreakSpeed>(cx, |_| {
        trace_play("PlayerEvent.BreakSpeed", |_| Vec::new());
    });
    on::<PlayerEventHarvestCheck>(cx, |_| {
        trace_play("PlayerEvent.HarvestCheck", |_| Vec::new());
    });
    on::<ServerTickStartEvent>(cx, |_| {
        {
            let mut state = state();
            if state.open {
                state.tick_step = Some(state.step());
            }
        }
        trace_once_per_step("ServerTickEvent.Pre");
    });
    on::<ServerTickEndEvent>(cx, |_| trace_once_per_step("ServerTickEvent.Post"));
}

/// The name of a Rust enum value in the lower snake case of the Java probe (`lower`).
fn lower(value: impl std::fmt::Debug) -> Value {
    let name = format!("{value:?}");
    let mut snake = String::new();
    for (index, letter) in name.chars().enumerate() {
        if letter.is_ascii_uppercase() && index > 0 {
            snake.push('_');
        }
        snake.push(letter.to_ascii_lowercase());
    }
    Value::Text(snake)
}

/// The Java `InteractionHand` of a Pumpkin hand: `Hand::Right` is the main hand.
fn hand(hand: Hand) -> Value {
    Value::Text(
        match hand {
            Hand::Right => "main_hand",
            Hand::Left => "off_hand",
        }
        .to_owned(),
    )
}

/// The serialized name of a face, `none` without one, as the Java probe writes it.
fn face(face: Option<BlockDirection>) -> Value {
    face.map_or_else(|| Value::Text("none".to_owned()), lower)
}

fn item(stack: &ItemStack) -> Value {
    Value::Text(format!("minecraft:{}", stack.item.registry_key))
}

/// `PlayerInteractEvent` carries `PlayerInteractEvent.LeftClickBlock`, `RightClickBlock` and
/// `RightClickItem` (design.md). `UseItemOnBlockEvent` is its own event.
fn interact(event: &PlayerInteractEvent) {
    let pos = |state: &State| {
        event
            .clicked_pos
            .map(|pos| ("pos", Value::Text(state.relative(&pos))))
    };
    match event.action {
        InteractAction::LeftClickBlock => {
            trace_play("PlayerInteractEvent.LeftClickBlock", |state| {
                let mut fields = vec![
                    (
                        "action",
                        event
                            .left_click_action
                            .map_or(Value::Text("none".to_owned()), lower),
                    ),
                    ("face", face(event.face)),
                    ("hand", hand(event.hand)),
                ];
                fields.extend(pos(state));
                fields
            });
        }
        InteractAction::RightClickBlock => {
            trace_play("PlayerInteractEvent.RightClickBlock", |state| {
                let mut fields = vec![
                    ("face", face(event.face)),
                    ("hand", hand(event.hand)),
                    ("item", item(&event.item_stack())),
                ];
                fields.extend(pos(state));
                fields
            });
        }
        InteractAction::RightClickAir => {
            trace_play("PlayerInteractEvent.RightClickItem", |_| {
                vec![
                    ("hand", hand(event.hand)),
                    ("item", item(&event.item_stack())),
                ]
            });
        }
        InteractAction::LeftClickAir => {}
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use ironpumpkin_mods::math::position::BlockPos;

    use super::{GRACE, State};
    use crate::{Value, line};

    #[test]
    fn a_step_counts_the_pings_older_than_the_grace() {
        let now = Instant::now();
        let state = State {
            open: true,
            origin: BlockPos::new(8, 120, 5),
            pings: [
                GRACE * 3,
                GRACE + Duration::from_millis(100),
                Duration::from_millis(10),
            ]
            .iter()
            .map(|age| now.checked_sub(*age).unwrap_or(now))
            .collect(),
            tick_step: None,
            traced_in_step: Vec::new(),
        };
        assert_eq!(state.step(), 2);
        assert_eq!(state.relative(&BlockPos::new(9, 118, 5)), "1 -2 0");
        assert_eq!(
            line(3, "ServerChatEvent", &[("step", Value::Int(1))]),
            r#"{"ordinal":3,"name":"ServerChatEvent","fields":{"step":1}}"#
        );
    }
}
