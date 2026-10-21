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

`mapping-table.md` of the change folder (see the [index](./index.md)) lists the Pumpkin event for every other NeoForge event, with a status. The living entity events are in [the living section](#the-living-entity-events). The entity, mob and spawn events are in [their section](#the-entity-mob-and-spawn-events).

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
| `PlayerInteractEvent.LeftClickBlock` | `PlayerInteractEvent` with the action left click block | Fires for the start, the stop and the abort of a dig (`left_click_action`), not on the arm swing, with `face` and the main `hand`. `use_block` set to `False` skips the block attack at the start. A cancel stops the action and sends the block back to the client. |
| `PlayerInteractEvent.RightClickBlock` | `PlayerInteractEvent` with the action right click block | Fires before the use phases with `hand`, `face`, `hit_vector` and `item_stack()`. `use_block` and `use_item` are `TriState`s: `False` skips the block use or both item phases, `True` uses the block also while sneaking or the item also with an empty hand. A cancel ends the use with `cancellation_result`: `Success` swings the arm for the other players, `SuccessServer` for every player. |
| `PlayerInteractEvent.RightClickItem` | `PlayerInteractEvent` with the action right click air | Fires before every item use with a non-empty hand, whatever the player looks at, but not while the item is on cooldown. A cancel skips the use and ends it with `cancellation_result`. |
| `UseItemOnBlockEvent` | the same name | Fires once per phase that runs (`phase`: `ItemBeforeBlock`, `Block`, `ItemAfterBlock`). A cancel skips the phase and gives `cancellation_result`, `TryWithEmptyHand` by default. |
| `PlayerContainerEvent` | `PlayerContainerEvent` | A concrete parent: register a `ParentEventHandler` with `register_parent_event` to get `InventoryOpenEvent` and `InventoryCloseEvent`. |

`PlayerEvent` and the other abstract classes have no struct, as NeoForge rejects a listener on an abstract event. `PlayerEvent.Clone` stays planned: Pumpkin keeps the same `Player` across a respawn, so there is no old player to copy from. `PermissionsChangedEvent`, `CanContinueSleepingEvent`, `PlayerNegotiationEvent` and `AdvancementEvent.AdvancementProgressEvent` stay planned too; the mapping table gives the reason.

## The level and block events

The packages `event.level`, `event.level.block` and `event.tick` map to Pumpkin events. Where Pumpkin already had an event at the fire site, the NeoForge event maps to it and has the NeoForge fields: `BreakBlockEvent` is `block::BlockBreakEvent`, `BlockEvent.EntityPlaceEvent` is `block::BlockPlaceEvent`. The other events are in `ironpumpkin_mods::event::level` with the NeoForge name and the parent prefix: `PistonEvent.Post` is `PistonEventPost`. `mapping-table.md` names the Rust type of each row.

| NeoForge event | IronPumpkin event | Notes |
| :--- | :--- | :--- |
| `BreakBlockEvent` | `block::BlockBreakEvent` | Has `player`, `state` and `block_position`. `exp` and `drop` are settable. A cancel keeps the block, and the client gets it back. It also fires without a player where NeoForge posts nothing: for a block that loses its support, a piston that breaks a block, a bed or a respawn anchor that explodes, a hatched egg, a sheep that eats grass, and a machine or a mob that calls `World::destroy_block` without a player owner. A break by a machine, which a `FakePlayer` does in NeoForge, has `player == None`. A port that expects `getPlayer` to be non-null checks `player` first. |
| `BlockEvent.EntityPlaceEvent` | `block::BlockPlaceEvent` for a player, `level::BlockEventEntityPlaceEvent` for an enderman | Fire before the block changes, with `placed_state` and `replaced_state`. A cancel places nothing. Register both to see every placement. NeoForge also posts it for an item use that changes a block (fire from a flint and steel or a fire charge, an axe, hoe or shovel transform, bone meal, a shears trim); IronPumpkin fires it for block items only. |
| `BlockEvent.EntityMultiPlaceEvent` | `block::BlockMultiPlaceEvent` | Fires after `BlockPlaceEvent` for a bed, a door, a tall plant or a small dripleaf. A cancel of either event places no block. |
| `BlockEvent.BlockToolModificationEvent` | `level::BlockEventBlockToolModificationEvent` | Fires for a fire starter and for shears, as in NeoForge 26.3. Axes, hoes and shovels fire nothing. `final_state` is settable, and `None` is no change. |
| `CropGrowEvent.Post`, `NoteBlockEvent.Change`, `PistonEvent.Post`, `CreateFluidSourceEvent` | `level::CropGrowEventPost`, `NoteBlockEventChange`, `PistonEventPost`, `CreateFluidSourceEvent` | `note` and `can_convert` are settable. A cancel of the note change keeps the note. The crop events (`CropGrowEventPost` and `block::BlockGrowEvent` for `CropGrowEvent.Pre`) fire only for wheat, carrots, potatoes, beetroots and the torchflower, and `BlockGrowEvent` fires only after a successful growth roll. |
| `ExplosionEvent.Start` | `entity::EntityExplodeEvent` | A cancel stops the explosion. |
| `ExplosionEvent.Detonate`, `ExplosionKnockbackEvent` | `level::ExplosionEventDetonate`, `ExplosionKnockbackEvent` | The lists of blocks and entities and the knockback are settable. |
| `ChunkEvent.Load`, `ChunkEvent.Unload` | `world::ChunkLoad`, `world::ChunkUnloadEvent` | Fire in the world tick after the change. A cancel has no effect. |
| `ChunkWatchEvent.UnWatch` | `level::ChunkWatchEventUnWatch` | Once per chunk, also when the player changes world or leaves. |
| `GameRuleChangedEvent`, `LevelEvent.PotentialSpawns` | `level::GameRuleChangedEvent`, `LevelEventPotentialSpawns` | `spawns` is settable. A new entry is a `&'static Spawner`, for example a `static` of the mod. A cancel spawns nothing. |
| `EntityTickEvent.Pre` and `Post`, `PlayerTickEvent.Pre` and `Post` | `level::EntityTickEventPre`, `EntityTickEventPost`, `PlayerTickEventPre`, `PlayerTickEventPost` | Fire for each entity and player in each tick, in parallel. A cancel of `EntityTickEventPre` skips the tick. A player fires the player tick events only. Keep these handlers short. |

v0.2 Wasm plugins get these events, `UseItemOnBlockEvent` and the new fields of `PlayerInteractEvent` through the same dispatch as native mods. The WIT records are in `event.wit` and `neoforge-events.wit` of `crates/pumpkin-plugin-wit/v0.2`, with block states as state ids. A v0.2 handler of a `neoforge-events.wit` event writes back only the fields that NeoForge can set. The merged Pumpkin events of `event.wit` keep their upstream write-back of the old fields, such as the block and the position, and keep the new fields that NeoForge cannot set. A new entry in `spawns` names its entity type. An entry with an unknown type, a minimum count below 1, a maximum below the minimum or a maximum above the mob cap of the category is dropped. A Wasm listener of a tick event costs a guest call per entity or player and tick, about 9.3 µs each for one caller. The entities tick in parallel and queue on the one store of the plugin. Keep Wasm tick handlers for small worlds, or port the logic to a native mod.

These rows stay planned: `BlockEvent.FarmlandTrampleEvent` (Pumpkin never tramples farmland), `LevelEvent.CreateSpawnPosition` (Pumpkin picks the spawn before the server exists), `ChunkDataEvent.Load` and `Save` (the chunk I/O runs in `pumpkin-world`), `AlterGroundEvent` and `ChunkTicketLevelUpdatedEvent` (engine extension points of M5). The mapping table gives the reasons.

## The living entity events

The package `event.entity.living` maps to Pumpkin events. Where Pumpkin already had an event at the fire site, the NeoForge event maps to it: `LivingIncomingDamageEvent` is `entity::EntityDamageEvent`, `MobEffectEvent.Added` is `entity::EntityPotionEffectEvent`. The other events are in `ironpumpkin_mods::event::living` with the NeoForge name and the parent prefix: `LivingDamageEvent.Pre` is `LivingDamageEventPre`, `LivingEvent.LivingJumpEvent` is `LivingEventLivingJumpEvent`. The `entity` of each event is the living entity, a player included. `mapping-table.md` names the Rust type of each row.

| NeoForge event | IronPumpkin event | Notes |
| :--- | :--- | :--- |
| `LivingDamageEvent.Pre` | `living::LivingDamageEventPre` | Fires after the armor, the effects and the hurt cooldown, before the absorption. `new_damage` is settable. `source` has the damage type, the direct and the causing entity and the position. There are no reduction modifiers. |
| `LivingDamageEvent.Post` | `living::LivingDamageEventPost` | Fires after the health changed, also after the death. `reductions.magic` holds the enchantments and the mob effects together. |
| `LivingShieldBlockEvent` | `living::LivingShieldBlockEvent` | Fires for a damage to an entity that holds up a shield. `blocked`, `blocked_damage` and `shield_damage` are settable. The rest of a partly blocked damage goes on. |
| `LivingKnockBackEvent` | `living::LivingKnockBackEvent` | Fires before the knockback resistance, for the knockback of a damage and of a player attack. `strength`, `ratio_x` and `ratio_z` are settable. A cancel gives no knockback. |
| `LivingFallEvent` | `living::LivingFallEvent` | `distance` and `damage_multiplier` are settable. A cancel deals no fall damage. The fall distance of a player comes from its client. |
| `LivingDrownEvent` | `living::LivingDrownEvent` | Fires for players only: Pumpkin ticks the air of players only. `drowning` is `true` in the tick of the damage. A cancel deals no damage. |
| `LivingExperienceDropEvent` | `living::LivingExperienceDropEvent` | Fires when a player killed the mob. `dropped_experience` is settable. A cancel drops no experience. |
| `LivingEntityUseItemEvent.Start`, `Tick` and `Stop` | `living::LivingEntityUseItemEventStart`, `Tick` and `Stop` | `duration` is settable on `Start` and `Tick`. A cancel of `Start` starts no use. With no remaining tick, a consumable finishes in that tick and another item stops being used. A cancel of `Stop` releases nothing: a bow shoots no arrow. `LivingEntityUseItemEvent` is a concrete parent: register a `ParentEventHandler` with `register_parent_event` to get the three. `Finish` is `player::player_item_consume::PlayerItemConsumeEvent`. |
| `LivingGetProjectileEvent` | `living::LivingGetProjectileEvent` | Fires for the bows and crossbows of players and mobs. `projectile` is settable. A projectile that a handler sets is a copy: the server takes nothing from the inventory for it. |
| `LivingEvent.LivingJumpEvent` | `living::LivingEventLivingJumpEvent` | A mob fires it when it jumps. A player fires it when the movement packets show a jump: the client already moved, so a handler cannot change the jump. |
| `LivingEvent.LivingVisibilityEvent` | `living::LivingEventLivingVisibilityEvent` | `modify_visibility` multiplies `visibility_modifier`. |
| `LivingDestroyBlockEvent` | `living::LivingDestroyBlockEvent` | Fires for the blocks that a wither breaks and when a mob starts to break a door. A cancel keeps the block. |
| `LivingConversionEvent.Post` | `living::LivingConversionEventPost` | Fires for a hoglin, a piglin and a piglin brute that convert. Pumpkin has no other conversion. |
| `EffectParticleModificationEvent` | `living::EffectParticleModificationEvent` | `visible` and `particle_color` are settable. The particle is always `minecraft:entity_effect`, so its options are the ARGB colour. |
| `MobEffectEvent.Applicable` | `living::MobEffectEventApplicable` | `result` is settable: `Apply`, `Default` or `DoNotApply`. `effect_source` is always `None`. |

v0.2 Wasm plugins get these events through the same dispatch as native mods. The WIT records are in `neoforge-events.wit`, and a v0.2 handler writes back only the fields that NeoForge can set.

`LivingEquipmentChangeEvent` stays planned: Pumpkin has no equipment diff in the living tick, so the server knows no previous stack of a slot. The mapping table gives the reason.

## The entity, mob and spawn events

The package `event.entity` and the mob and spawn events of `event.entity.living` map to Pumpkin events. `EntityJoinLevelEvent` is `entity::EntitySpawnEvent`, as before. The other events are in `ironpumpkin_mods::event::level` with the NeoForge name and the parent prefix: `MobSpawnEvent.PositionCheck` is `MobSpawnEventPositionCheck`. Each event carries the live entity as `Arc<dyn EntityBase>`.

| NeoForge event | IronPumpkin event | Notes |
| :--- | :--- | :--- |
| `EntityJoinLevelEvent` | `entity::EntitySpawnEvent` | Fires for a spawn, for each rider that a spawned mob brings and for an entity that loads from its chunk. A cancel keeps the entity out of the world. A cancelled loaded entity is gone with the next save of its chunk, as on NeoForge. There is no `loadedFromDisk` field. |
| `EntityLeaveLevelEvent` | `level::EntityLeaveLevelEvent` | Fires after the entity left the entity list: a removal, the unload of its chunk, a player that leaves. A lookup in `world` no longer finds the entity. |
| `EntityMobGriefingEvent` | `level::EntityMobGriefingEvent` | `can_grief` is settable and starts at the `mobGriefing` game rule. Fires at the sites where Pumpkin reads the rule for an entity: the turtle egg trample, the enderman block goals, the remove block goal of zombies, the sheep eating grass, the wither and the piglin item pick-up. A mob explosion fires nothing, because `World::explode` has no source entity. |
| `EntityStruckByLightningEvent` | `level::EntityStruckByLightningEvent` | A cancel spares the entity from the bolt. Pumpkin strikes an entity once per bolt. |
| `EntityTeleportEvent.SpreadPlayersCommand` | `level::EntityTeleportEventSpreadPlayersCommand` | `target` is settable. A cancel leaves the entity where it is. A handler of `entity::EntityTeleportEvent` does not get it. |
| `ProjectileImpactEvent` | `level::ProjectileImpactEvent` | Fires for every projectile before it handles a hit. `ray` is a `ProjectileHit`: a block with its face, or an entity. A cancel skips the hit and the projectile flies on. |
| `EndermanAngerEvent` | `level::EndermanAngerEvent` | Fires when a player without a carved pumpkin stares at the enderman. A cancel makes the stare not count. |
| `MobDespawnEvent` | `level::MobDespawnEvent` | Once per mob and tick, also for a persistent mob. `result` is a `MobDespawnResult`: `Allow` removes the mob now, `Deny` keeps it for this tick. |
| `MobSpawnEvent.PositionCheck`, `MobSpawnEvent.SpawnPlacementCheck` | `level::MobSpawnEventPositionCheck`, `MobSpawnEventSpawnPlacementCheck` | `result` is a `MobSpawnResult`: `Succeed`, `Default` or `Fail`. `spawn_type` is an `EntitySpawnReason`. The placement check has `default_result`, the result of the spawn rules, and no random source. |
| `MobSplitEvent` | `level::MobSplitEvent` | `children` is settable. A child must be a new entity: the server skips a child that is already in the world. A cancel spawns no child. |
| `SpawnClusterSizeEvent` | `level::SpawnClusterSizeEvent` | `size` is settable and starts at the limit per chunk of the entity type. |
| `EntityInvulnerabilityCheckEvent` | `level::EntityInvulnerabilityCheckEvent` | `invulnerable` is settable. `damage_type` and `attacker` stand for the `DamageSource`. Fires for living entities, item entities, armor stands, item frames, paintings and end crystals. |

The despawn, griefing, invulnerability, spawn and cluster events fire for each mob and check, behind the handler check of the fire site. Keep these handlers short. v0.2 Wasm plugins get the events through the same dispatch; the records are in `neoforge-events.wit`. A Wasm handler of `MobSplitEvent` can only drop or reorder the children, and a spread target that is not finite is ignored.

`XpOrbTargetingEvent` stays planned: the experience orb of Pumpkin never follows a player. `EntityEvent.EnteringSection`, `EntityEvent.EntityConstructing` and `EntityEvent.Size` stay planned too; the mapping table gives the reasons.

## Causes: what a `FakePlayer` did

A NeoForge mod often acts through a `FakePlayer`: a machine breaks a block as a fake player, so the break events fire with that player. IronPumpkin has no fake players. A mod calls the Pumpkin method of the vanilla action with the entity that acts, and the method fires the Pumpkin event of that action, with the same dispatch as for a real player. Native mods and v0.2 Wasm plugins see the event and can cancel it.

The server resolves the cause of the event:

- A player is its own cause.
- Another entity resolves to its owner (`EntityBase::get_owner_id`). A projectile resolves to the player that shot it. `BlockBreakEvent` carries that player in `player`. An owner that is not a player gives `player = None`.
- A block entity has no entity. A machine calls the method without a cause, and the event has no player, as for a piston.

| NeoForge call | IronPumpkin method | Event |
| :--- | :--- | :--- |
| `level.destroyBlock(pos, drop, entity)` | `World::destroy_block(&pos, drop, Some(entity))` | `BlockBreakEvent`, which NeoForge does not post here. Returns `false` when cancelled. |
| `BlockItem.place` with a fake player, or `EventHooks.onBlockPlace` before `level.setBlock` | `World::place_block(&pos, state, entity)` | `block::BlockPlaceEvent` when the cause is a player, `level::BlockEventEntityPlaceEvent` for another entity, as for the placements of the game. Returns `false` when cancelled. |
| `level.addFreshEntity(entity)` | `World::spawn_entity(entity)` | `EntitySpawnEvent`. Returns `false` when cancelled. |
| `entity.hurtServer(level, source, amount)` | `damage_with_context(caller, amount, type, position, source, cause)` | `EntityDamageEvent`, then `EntityDamageByEntityEvent` or `EntityDamageByBlockEvent`, for a mob, a player or an item entity. A death fires `EntityDeathEvent`. Other entities fire no damage event yet. |
| `level.setBlock(pos, state, flags)` | `World::set_block_state(&pos, state, flags)` | No event, as in vanilla. Use it for the internal state of a machine. |

A mining laser that breaks the block it hits:

```java
@Override
protected void onHitBlock(BlockHitResult hit) {
    super.onHitBlock(hit);
    level().destroyBlock(hit.getBlockPos(), true, this);
}
```

```rust
use std::sync::Arc;

use ironpumpkin_mods::{entity::EntityBase, math::position::BlockPos, world::World};

// `BlockBreakEvent` fires with the player that shot the laser.
fn on_hit_block(world: &Arc<World>, laser: &dyn EntityBase, pos: BlockPos) {
    world.destroy_block(&pos, true, Some(laser));
}
```

The table "Mutators and their events" of `design.md` in the change folder lists every method and its event.

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
