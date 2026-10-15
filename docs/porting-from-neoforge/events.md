# Porting events from NeoForge

A NeoForge mod listens to events on two buses. The mod bus carries the lifecycle and registration events. The game bus, `NeoForge.EVENT_BUS`, carries the game events. A listener is a method with `@SubscribeEvent`, or a call to `IEventBus#addListener`. It has a priority and a `receiveCanceled` flag. A listener can change an event and cancel it.

In IronPumpkin, a game event is a Pumpkin event with the NeoForge fire site, fields and cancel rule. Many have the NeoForge name. A mod registers a handler for an event struct with `ModInit::register_event` in `NativeMod::init`. The mod bus events are `NativeMod` methods. See [Lifecycle](./lifecycle.md).

---

## Key Differences

| Feature | NeoForge | IronPumpkin |
| :--- | :--- | :--- |
| **Game bus** | `NeoForge.EVENT_BUS` | The one Pumpkin dispatch. Every mod shares it. |
| **Mod bus** | `IEventBus modBus` | `NativeMod` methods. See [Lifecycle](./lifecycle.md). |
| **Listener** | A method with `@SubscribeEvent`, or `addListener(..)` | A type that implements `EventHandler<E>`, registered with `ModInit::register_event` |
| **Event type** | A class | A struct that implements `Payload` |
| **Priority** | `EventPriority.HIGHEST` to `LOWEST` | `EventPriority::Highest` to `Lowest`. The caller always passes one. |
| **Order** | By priority, then by registration order | Blocking handlers by priority, then non-blocking handlers by priority. Registration order inside one priority. |
| **Change an event** | Every listener can | A blocking handler: `handle_blocking` gets `&mut E` |
| **Observe an event** | Any listener | A non-blocking handler: `handle` gets `&E` and runs after every blocking handler |
| **Cancel** | `ICancellableEvent#setCanceled(true)` | `Cancellable::set_cancelled(true)` on an event that is cancellable |
| **Receive canceled** | `receiveCanceled = true` | The `receive_canceled` argument |
| **Listener on a parent class** | A listener for the parent type gets the subclasses | `ModInit::register_parent_event` with a `ParentEventHandler`. The handler gets a `dyn Payload` and downcasts it. |
| **Result** | `setResult(..)` on some events | Planned |
| **Unregister** | `IEventBus#unregister` | Not supported. A handler lives as long as the server. |

---

## Code Comparison: cancel a chat message and observe the rest

### 1. NeoForge implementation (Java)

```java
@Mod(HelloMod.MOD_ID)
public final class HelloMod {
    public static final String MOD_ID = "hello_mod";

    public HelloMod(IEventBus modBus) {
        NeoForge.EVENT_BUS.register(HelloMod.class);
    }

    @SubscribeEvent(priority = EventPriority.HIGH)
    public static void blockSpam(ServerChatEvent event) {
        if (event.getRawText().equals("spam")) {
            event.setCanceled(true);
        }
    }

    @SubscribeEvent(priority = EventPriority.LOWEST, receiveCanceled = true)
    public static void audit(ServerChatEvent event) {
        LOGGER.info("[{}] {}: {} (canceled: {})",
                MOD_ID, event.getUsername(), event.getRawText(), event.isCanceled());
    }

    @SubscribeEvent
    public static void onTick(ServerTickEvent.Post event) {
        // Runs at the end of every server tick.
    }
}
```

### 2. IronPumpkin implementation (Rust)

The snippet is a doc-test of `ironpumpkin-mods`.

```rust
use std::sync::Arc;

use ironpumpkin_mods::{
    ModInit, NativeMod,
    event::{
        BoxFuture, Cancellable, EventHandler, EventPriority, player::PlayerChatEvent,
        server::ServerTickEndEvent,
    },
    register_mod,
    server::Server,
    tracing::info,
};

const MOD_ID: &str = "hello_mod";

/// Cancels a chat message. A blocking handler gets the event by mutable reference.
struct BlockSpam;

impl EventHandler<PlayerChatEvent> for BlockSpam {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut PlayerChatEvent,
    ) -> BoxFuture<'a, ()> {
        if event.raw_text == "spam" {
            event.set_cancelled(true);
        }
        Box::pin(async {})
    }
}

/// Logs every message, also a cancelled one. It is registered with `receive_canceled`.
struct Audit;

impl EventHandler<PlayerChatEvent> for Audit {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut PlayerChatEvent,
    ) -> BoxFuture<'a, ()> {
        info!(
            "[{MOD_ID}] {}: {} (canceled: {})",
            event.username(),
            event.raw_text,
            event.cancelled()
        );
        Box::pin(async {})
    }
}

/// Reads the tick. A non-blocking handler gets the event by shared reference.
struct EveryMinute;

impl EventHandler<ServerTickEndEvent> for EveryMinute {
    fn handle<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a ServerTickEndEvent,
    ) -> BoxFuture<'a, ()> {
        if event.tick % 1200 == 0 {
            info!("[{MOD_ID}] tick {}", event.tick);
        }
        Box::pin(async {})
    }
}

struct HelloMod;

impl NativeMod for HelloMod {
    fn id(&self) -> &'static str {
        MOD_ID
    }

    fn display_name(&self) -> &'static str {
        "Hello Mod"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn init(&self, cx: &mut ModInit) {
        // Arguments: the handler, the priority, `blocking` and `receive_canceled`.
        cx.register_event::<PlayerChatEvent, _>(
            Arc::new(BlockSpam),
            EventPriority::High,
            true,
            false,
        );
        cx.register_event::<PlayerChatEvent, _>(
            Arc::new(Audit),
            EventPriority::Lowest,
            true,
            true,
        );
        cx.register_event::<ServerTickEndEvent, _>(
            Arc::new(EveryMinute),
            EventPriority::Normal,
            false,
            false,
        );
    }
}

register_mod!(HelloMod);
```

The mapping from the Java code to the Rust code:

| Java | Rust |
| :--- | :--- |
| `@SubscribeEvent` on a method | A type that implements `EventHandler<E>`, and a `register_event` call |
| `NeoForge.EVENT_BUS.register(HelloMod.class)` | Nothing. Rust has no annotation scan: list each handler in `init`. |
| `priority = EventPriority.HIGH` | `EventPriority::High` |
| `receiveCanceled = true` | `receive_canceled = true` |
| A listener that changes or cancels the event | `blocking = true` and `handle_blocking` |
| A listener that only reads the event | `blocking = false` and `handle` |
| `event.setCanceled(true)` | `event.set_cancelled(true)`. Import the `Cancellable` trait. |
| `event.isCanceled()` | `event.cancelled()` |
| `event.getRawText()` | The field `event.raw_text` |
| `event.getUsername()` | `event.username()` |
| `ServerTickEvent.Post` | `ServerTickEndEvent` |

---

## The order of the handlers

The dispatch calls the handlers of an event in this order:

1. The blocking handlers, from `Highest` to `Lowest`.
2. The non-blocking handlers, from `Highest` to `Lowest`.

Inside one priority, the order is the registration order. The server applies the registrations of a mod when its `init` returns. So the mods in load order come first, and the calls in `init` come next. See [Lifecycle](./lifecycle.md).

A handler for a parent event runs after the handlers of the event itself, inside the same priority.

A handler that did not set `receive_canceled` does not run once an earlier handler cancelled the event. This rule holds for blocking and non-blocking handlers.

The server acts on the fields as the last blocking handler left them.

## Blocking and non-blocking handlers

The `blocking` argument of `register_event` decides which method the server calls. A blocking handler gets `handle_blocking`. A non-blocking handler gets `handle`. The other method keeps its empty default, so a handler that implements the wrong method never runs.

A NeoForge listener can change the event. Port it with `blocking = true`. Use `blocking = false` only to read the final state, because the non-blocking handlers run after all blocking handlers.

A handler is synchronous inside the async server. The future that it returns runs to the end before the next handler starts. A handler that needs a world call blocks on that call. The documentation of `ModInit::register_event` shows how.

A handler must not hold a lock of its own state while it fires another event.

## The events with NeoForge semantics

The table lists the events that have the NeoForge fire site, fields and cancel rule today. The paths are inside `ironpumpkin_mods::event`. The documentation comment of each event names the NeoForge class and the fire site.

| NeoForge event | IronPumpkin event | Notes |
| :--- | :--- | :--- |
| `ServerStartingEvent` | `server::ServerStartingEvent` | After the worlds load, before the first tick and the first connection |
| `ServerStartedEvent` | `server::ServerLoadEvent` | Before the first tick |
| `ServerStoppingEvent` | `server::ServerStoppingEvent` | Before the server saves the players and the worlds |
| `ServerStoppedEvent` | `server::ServerStoppedEvent` | After the save. Only native mods get it. |
| `ServerTickEvent.Pre` and `Post` | `server::ServerTickStartEvent` and `ServerTickEndEvent` | `has_time()` works as `hasTime` |
| `LevelTickEvent.Pre` and `Post` | `world::LevelTickEventPre` and `LevelTickEventPost` | Once for each world and tick. The worlds tick in parallel. |
| `VanillaGameEvent` | `world::GenericGameEvent` | Fires from a few call sites |
| `ServerChatEvent` | `player::PlayerChatEvent` | A cancel sends the message to nobody. `message` is plain text. |
| `StatAwardEvent` | `player::PlayerStatisticIncrementEvent` | `stat` and `amount` are settable. A cancel adds nothing. |
| `AnvilUpdateEvent` | `inventory::PrepareAnvilEvent` | `output`, `repair_cost` and `material_cost` are settable. A cancel empties the output. |
| `GrindstoneEvent.OnPlaceItem` | `inventory::PrepareGrindstoneEvent` | `output` and `xp` are settable. A cancel empties the output. |
| `PotionBrewEvent.Pre` | `inventory::BrewEvent` | A cancel skips the vanilla brewing |
| `EnchantmentLevelSetEvent` | `enchantment::PrepareItemEnchantEvent` | One event for the three rows of the table |
| `PlayerEnchantItemEvent` | `enchantment::PlayerEnchantItemEvent` | Cannot be cancelled |

`mapping-table.md` of the change folder (see the [index](./index.md)) lists the Pumpkin event for every other NeoForge event, with a status. The pages for the player, the level and block, and the entity families are planned (tasks 2.12, 2.13 and 2.15). Until they land, a handler for such an event runs only where Pumpkin already fires it.

---

## Do and don't

**Do**

- Register all handlers in `NativeMod::init`. The server applies them when `init` returns.
- Port a NeoForge listener that changes or cancels an event with `blocking = true`.
- Use a non-blocking handler to read the final state of an event.
- Set `receive_canceled = true` for a listener that had `receiveCanceled = true`.
- Import the `Cancellable` trait to call `set_cancelled`.
- Read the documentation comment of a Pumpkin event. It names the NeoForge class, the fire site and the fields that the server reads back.

**Don't**

- Do not implement `handle` for a handler that you register as blocking. The server never calls it.
- Do not hold a lock of your own state while you fire an event from a handler.
- Do not change a field that the server does not read back. The change has no effect.
- Do not wait for a result setter. Events with a result are planned.
- Do not expect a handler to run for an event that no fire site fires.
- Do not panic in a handler. The panic hook of the server writes a crash report and exits at once.
