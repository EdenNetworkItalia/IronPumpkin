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

A handler that did not set `receive_canceled` does not run once an earlier handler cancelled the event. This rule holds for blocking and non-blocking handlers. `ModInit::register_event` takes the flag. A Wasm plugin that registers through `Context::register_event` always receives cancelled events.

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
| `RegisterEvent` | `registries::RegisterEvent` | Once for `minecraft:block`, `minecraft:entity_type` and `minecraft:item`, after the last `init` and before `common_setup`. Register content with `content::RegisterEventExt`. See [Registration](./registration.md). |
| `ServerAboutToStartEvent` | `server::ServerAboutToStartEvent` | After the synced configs load and the content freeze, before the worlds load. Only native mods get it. The datapack recipes and tags are not loaded yet. See [Lifecycle](./lifecycle.md). |
| `ServerStartingEvent` | `server::ServerStartingEvent` | After the worlds load, before the first tick and the first connection |
| `ServerStartedEvent` | `server::ServerLoadEvent` | Before the first tick |
| `ServerStoppingEvent` | `server::ServerStoppingEvent` | After the ticker thread exits (bound 30 s), before the server saves the players and the worlds |
| `ServerStoppedEvent` | `server::ServerStoppedEvent` | After the save. Only native mods get it. |
| `ServerTickEvent.Pre` and `Post` | `server::ServerTickStartEvent` and `ServerTickEndEvent` | `has_time()` works as `hasTime` |
| `LevelTickEvent.Pre` and `Post` | `world::LevelTickEventPre` and `LevelTickEventPost` | Once for each world and tick, also while the game is frozen. In a normal tick the worlds tick in parallel, so handlers of two worlds can run at the same time. |
| `VanillaGameEvent` | `world::GenericGameEvent` | Fires from a few call sites |
| `ServerChatEvent` | `player::PlayerChatEvent` | A cancel sends the message to nobody. `message` is plain text. `raw_text` is filled only when a chat handler was registered when the event is created. |
| `StatAwardEvent` | `player::PlayerStatisticIncrementEvent` | Fires on an increment and on `set_stat`, also for a reset to 0. `value` is the new total. `category`, `stat` and `value` are settable. A cancel stores nothing. See the contract note below the table. |
| `AnvilUpdateEvent` | `inventory::PrepareAnvilEvent` | Fires on every result update, outside the screen-handler lock. `output`, `repair_cost` and `material_cost` are settable. A cancel clears the output and the cost. The result applies only if the inputs did not change meanwhile. |
| `GrindstoneEvent.OnPlaceItem` | `inventory::PrepareGrindstoneEvent` | Fires on every result update, outside the screen-handler lock. `result_item` is the vanilla result. `output` and `xp` are settable, and `xp` applies when it is 0 or more. A cancel clears the result. |
| `GrindstoneEvent.OnTakeItem` | `inventory::GrindstoneEventOnTakeItem` | Fires on every take of the result, outside the screen-handler lock. `xp` is settable. `new_top` and `new_bottom` are the inputs after the take and are empty by default. `pos` is the grindstone. A cancel gives no experience, puts the inputs back and leaves the result empty. The player keeps the taken item. |
| `PotionBrewEvent.Pre` | `inventory::BrewEvent` | A cancel skips the vanilla brewing. It takes `stacks` only if the stand inventory did not change meanwhile. |
| `PotionBrewEvent.Post` | `inventory::PotionBrewEventPost` | Fires after the stand brewed the bottles and before it consumes the ingredient. It also fires after a cancelled `PotionBrewEvent.Pre`. `stacks` is a copy of the stand inventory. The stand takes changed `stacks` only if its inventory did not change meanwhile. Cannot be cancelled. Only native mods get it. |
| `EnchantmentLevelSetEvent` | `enchantment::PrepareItemEnchantEvent` | One event for the three rows of the table. `pos` is the open container. `original_level_requirements()` returns the vanilla levels. |
| `PlayerEnchantItemEvent` | `enchantment::PlayerEnchantItemEvent` | Fires before the lapis is consumed. The table writes `enchanted_item` back. Cannot be cancelled. |

No event of the table fires under the screen-handler lock. The anvil, grindstone, enchanting and trade events fire outside it, so a handler can close the screen or lock a screen handler. For the events that compute a result (anvil, grindstone and enchanting), the server applies the result only if the inputs did not change while the event fired.

The contract of `StatAwardEvent`: `value` is the new total, and the server stores it in the statistic that `category` and `stat` name. `amount` is the change, that is the new total minus the total before the event. A handler that changes only `amount` has no effect, because `value` wins. To change the result, set `value`.

`mapping-table.md` of the change folder (see the [index](./index.md)) lists the Pumpkin event for every other NeoForge event, with a status. The page for the entity family is planned (task 2.15). Until it lands, a handler for such an event runs only where Pumpkin already fires it.

## The player events

The package `event.entity.player` maps to `ironpumpkin_mods::event::player`. A nested class takes its parent name as a prefix: `PlayerEvent.BreakSpeed` is `PlayerEventBreakSpeed`, `PlayerXpEvent.PickupXp` is `PlayerXpEventPickupXp`. Where Pumpkin already had an event at the fire site, the NeoForge event maps to it: `PlayerRespawnPositionEvent` is `PlayerSpawnLocationEvent`, `PlayerSwitchHotbarSlotEvent.Pre` is `PlayerItemHeldEvent`. `mapping-table.md` names the Rust type of each row.

| NeoForge event | IronPumpkin event | Notes |
| :--- | :--- | :--- |
| `PlayerEvent.BreakSpeed` | `PlayerEventBreakSpeed` | Fires on every mining step. `new_speed` is settable. A cancel makes the block unbreakable for that step. |
| `PlayerEvent.HarvestCheck` | `PlayerEventHarvestCheck` | `can_harvest` is settable. It decides the mining speed and the drops. |
| `CriticalHitEvent` and `SweepAttackEvent` | the same names | `critical_hit`, `damage_multiplier`, `disable_sweep` and `sweeping` are settable. A cancelled sweep event keeps the value of `sweeping`. |
| `ArrowNockEvent` and `ArrowLooseEvent` | the same names | A cancel or a set `action` stops the draw. `charge` is settable, and a cancel shoots nothing. |
| `ItemEntityPickupEvent.Pre` and `Post` | `ItemEntityPickupEventPre` and `ItemEntityPickupEventPost` | `can_pickup` is a `TriState`: `False` refuses, `True` ignores the pickup delay. |
| `PlayerXpEvent.PickupXp` | `PlayerXpEventPickupXp` | A cancel leaves the orb. |
| `PlayerEvent.NameFormat` and `TabListNameFormat` | `PlayerEventNameFormat` and `PlayerEventTabListNameFormat` | Fire only when no name was set with `set_display_name` or `set_tab_list_name`. The display name is not cached: the event fires on every read. |
| `PlayerRespawnPositionEvent` | `PlayerSpawnLocationEvent` | Position, rotation, dimension and `copy_original_spawn_position` are settable. |
| `AnvilCraftEvent.Pre` and `Post` | `AnvilCraftEventPre` and `AnvilCraftEventPost` | Fire outside the screen-handler lock. A cancel of the Pre event skips the cost and the use of the inputs. |
| `TradeWithVillagerEvent` | the same name | Fires outside the screen-handler lock, for a villager and for a wandering trader. A villager sends the refreshed offers from its next tick, as vanilla does. |
| `PlayerInteractEvent.LeftClickBlock` | `PlayerInteractEvent` with the action left click block | Fires at the dig start only, not on the arm swing. It has no action (start, stop or abort), face or hand. A cancel stops the break and sends the block back to the client. |
| `PlayerContainerEvent` | `PlayerContainerEvent` | A concrete parent: register a `ParentEventHandler` with `register_parent_event` to get `InventoryOpenEvent` and `InventoryCloseEvent`. |

`PlayerEvent` and the other abstract classes have no struct, as NeoForge rejects a listener on an abstract event. `PlayerEvent.Clone` stays planned: Pumpkin keeps the same `Player` across a respawn, so there is no old player to copy from. `PermissionsChangedEvent`, `CanContinueSleepingEvent`, `PlayerNegotiationEvent` and `AdvancementEvent.AdvancementProgressEvent` stay planned too; the mapping table gives the reason.

## The level and block events

The packages `event.level`, `event.level.block` and `event.tick` map to Pumpkin events. Where Pumpkin already had an event at the fire site, the NeoForge event maps to it and has the NeoForge fields: `BreakBlockEvent` is `block::BlockBreakEvent`, `BlockEvent.EntityPlaceEvent` is `block::BlockPlaceEvent`. The other events are in `ironpumpkin_mods::event::level` with the NeoForge name and the parent prefix: `PistonEvent.Post` is `PistonEventPost`. `mapping-table.md` names the Rust type of each row.

| NeoForge event | IronPumpkin event | Notes |
| :--- | :--- | :--- |
| `BreakBlockEvent` | `block::BlockBreakEvent` | Has `player`, `state` and `block_position`. `exp` and `drop` are settable. A cancel keeps the block, and the client gets it back. It also fires without a player for a block that loses its support. Skip those events: `getPlayer` is never null in NeoForge. |
| `BlockEvent.EntityPlaceEvent` | `block::BlockPlaceEvent` for a player, `level::BlockEventEntityPlaceEvent` for an enderman | Fire before the block changes, with `placed_state` and `replaced_state`. A cancel places nothing. Register both to see every placement. |
| `BlockEvent.EntityMultiPlaceEvent` | `block::BlockMultiPlaceEvent` | Fires after `BlockPlaceEvent` for a bed, a door, a tall plant or a small dripleaf. A cancel of either event places no block. |
| `BlockEvent.BlockToolModificationEvent` | `level::BlockEventBlockToolModificationEvent` | Fires for a fire starter and for shears, as in NeoForge 26.3. Axes, hoes and shovels fire nothing. `final_state` is settable, and `None` is no change. |
| `CropGrowEvent.Post`, `NoteBlockEvent.Change`, `PistonEvent.Post`, `CreateFluidSourceEvent` | `level::CropGrowEventPost`, `NoteBlockEventChange`, `PistonEventPost`, `CreateFluidSourceEvent` | `note` and `can_convert` are settable. A cancel of the note change keeps the note. |
| `ExplosionEvent.Start` | `entity::EntityExplodeEvent` | A cancel stops the explosion. |
| `ExplosionEvent.Detonate`, `ExplosionKnockbackEvent` | `level::ExplosionEventDetonate`, `ExplosionKnockbackEvent` | The lists of blocks and entities and the knockback are settable. |
| `ChunkEvent.Load`, `ChunkEvent.Unload` | `world::ChunkLoad`, `world::ChunkUnloadEvent` | Fire in the world tick after the change. A cancel has no effect. |
| `ChunkWatchEvent.UnWatch` | `level::ChunkWatchEventUnWatch` | Once per chunk, also when the player changes world or leaves. |
| `GameRuleChangedEvent`, `LevelEvent.PotentialSpawns` | `level::GameRuleChangedEvent`, `LevelEventPotentialSpawns` | `spawns` is settable. A new entry is a `&'static Spawner`, for example a `static` of the mod. A cancel spawns nothing. |
| `EntityTickEvent.Pre` and `Post`, `PlayerTickEvent.Pre` and `Post` | `level::EntityTickEventPre`, `EntityTickEventPost`, `PlayerTickEventPre`, `PlayerTickEventPost` | Fire for each entity and player in each tick, in parallel. A cancel of `EntityTickEventPre` skips the tick. A player fires the player tick events only. Keep these handlers short. |

Only native mods get these events, except the events that WIT v0.2 already had: `BlockBreakEvent`, `BlockPlaceEvent`, `BlockMultiPlaceEvent`, `EntityExplodeEvent`, `ChunkLoad` and `ChunkUnloadEvent`. A Wasm plugin gets them without the new fields.

These rows stay planned: `BlockEvent.FarmlandTrampleEvent` (Pumpkin never tramples farmland), `LevelEvent.CreateSpawnPosition` (Pumpkin picks the spawn before the server exists), `ChunkDataEvent.Load` and `Save` (the chunk I/O runs in `pumpkin-world`), `AlterGroundEvent` and `ChunkTicketLevelUpdatedEvent` (engine extension points of M5). The mapping table gives the reasons.

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
- Do not wait for a result setter. Events with a result are planned, except `ItemEntityPickupEventPre` (`TriState`) and `ArrowNockEvent` (`InteractionResult`).
- Do not expect a handler to run for an event that no fire site fires.
- Do not panic in a handler. The panic hook of the server writes a crash report and exits at once.
