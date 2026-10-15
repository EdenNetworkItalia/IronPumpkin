# Spec Delta

## Purpose

Lets a ported mod handle the NeoForge events as Pumpkin events. Each event has the NeoForge name, fire site, fields, cancel rule and result rule. A mod registers handlers with `ModInit::register_event`, with an `EventPriority`, the blocking flag and cancellation. Native mods and v0.2 Wasm plugins share the one Pumpkin dispatch. The lifecycle events of NeoForge are methods of the `NativeMod` trait.

## ADDED Requirements

### Requirement: Concrete parents receive subclasses
A mod that registers a handler for a concrete event class that has concrete subclasses SHALL also receive each subclass event, delivered as the subclass struct, as NeoForge delivers it to a listener on the parent type. A nested NeoForge class is one struct with the parent name as prefix. Status: supported (task 2.3, #102).

#### Scenario: Handler on a parent
- **WHEN** a mod registers a handler for `LivingEntityUseItemEvent` and an entity starts using an item
- **THEN** the handler receives a `LivingEntityUseItemEventStart`

### Requirement: Lifecycle methods are called per mod
The host SHALL call each lifecycle method of `NativeMod` on its own mod only, because NeoForge has one mod bus per `ModContainer`. A Pumpkin event that a mod fires or that the server fires SHALL reach the handlers of every mod. Status: supported (task 2.2, #103).

#### Scenario: Lifecycle method of two mods
- **WHEN** mods A and B both override the common setup method and the host runs the common setup step
- **THEN** the host calls the method of A on mod A once and the method of B on mod B once

#### Scenario: Game event reaches every mod
- **WHEN** mods A and B both register a handler for the same Pumpkin event and the server fires it
- **THEN** both handlers run

### Requirement: Reentrant calls of Wasm plugins
A call that runs Wasm plugin code before it returns SHALL be limited to the reentry depth of 64 of `pumpkin-plugin-runtime`, and a deeper call SHALL fail. This limit applies to Wasm plugins only. A native handler runs on the stack of the caller with no depth limit of its own, and a native mod SHALL NOT hold a lock across a fire. Status: planned (task 2.3).

#### Scenario: Too deep a nesting in a Wasm plugin
- **WHEN** a Wasm plugin nests calls that re-enter the plugin to a depth of 65
- **THEN** the call at depth 65 fails

#### Scenario: Fire from a native handler
- **WHEN** a native handler fires another event and reads its config, and the mod holds no lock of its own state across the fire
- **THEN** both calls complete

### Requirement: Handler registration
`ModInit::register_event` SHALL register a handler for one event struct with one of the five `EventPriority` values (Highest, High, Normal, Low, Lowest), the blocking flag and the receive-canceled flag. A handler that did not set receive-canceled SHALL NOT be called for a cancelled event. Status: supported (task 2.3, #102).

#### Scenario: Receive canceled
- **WHEN** two handlers for one cancellable event are registered, one with receive-canceled and one without, and an earlier handler cancels the event
- **THEN** only the handler with receive-canceled runs

### Requirement: Priority is honoured
`PluginManager::fire_dyn` SHALL call the blocking handlers of an event in the order of their `EventPriority`, Highest first and Lowest last, with registration order inside one priority, and then the non-blocking handlers in the same order. Status: supported (task 2.3, #102).

#### Scenario: Priority order
- **WHEN** two blocking handlers for one event type have priorities `High` and `Low`
- **THEN** the host calls the `High` handler first

#### Scenario: Blocking before non-blocking
- **WHEN** a non-blocking handler with priority `Highest` and a blocking handler with priority `Lowest` are registered for one event type
- **THEN** the host calls the blocking handler first

### Requirement: Handlers are synchronous
A handler of a native mod SHALL be synchronous inside the async host of Pumpkin. A handler that needs a world call SHALL use `block_on(unconstrained(..))`, as `ModInit::register_service` does, and SHALL NOT hold a lock across a fire. Status: supported (task 2.3, #102).

#### Scenario: World call from a handler
- **WHEN** a handler reads a block of the world while the server fires an event
- **THEN** the call completes, and the dispatch continues with the next handler

### Requirement: Dispatch and write-back
The host SHALL call each handler with the event by mutable reference, and SHALL act on the fields as the last handler left them. A change to a settable field or to `cancelled` SHALL be visible to the next handler and to the server code that fired the event. Status: supported (task 2.3, #102).

#### Scenario: Change a settable field
- **WHEN** a `LivingHealEvent` handler sets `amount` to 2.0 for a heal of 4.0
- **THEN** the entity heals 2.0

### Requirement: Cancellation
A cancellable event SHALL carry `cancelled` (the Pumpkin `Cancellable` trait). When a handler sets it, the host SHALL skip later handlers that did not set receive-canceled and SHALL NOT perform the action the event describes. Status: supported (task 2.3, #102).

#### Scenario: Cancel a block break
- **WHEN** a `BlockBreakEvent` handler (NeoForge `BreakBlockEvent`) sets `cancelled` to true
- **THEN** the block stays and no later handler without receive-canceled runs

### Requirement: Results
An event with a NeoForge result setter SHALL carry the result as a settable field of an enum type (`TriState`, `InteractionResult` or the event's own result enum), and the host SHALL apply the set value as NeoForge does. Status: planned (tasks 2.13 and 2.15; `TriState` and `InteractionResult` exist, task 2.12, #112).

#### Scenario: Deny a despawn
- **WHEN** a `MobDespawnEvent` handler sets the result to `Deny`
- **THEN** the mob does not despawn

### Requirement: Registration handles work only during their call
A lifecycle method of `NativeMod` that hands the mod a registration API SHALL pass a handle that works only during that call. A call on a handle that outlives the call SHALL fail with a `registration-closed` error. Status: planned (task 2.2).

#### Scenario: Late call
- **WHEN** a mod keeps the capability registration handle that the host passed to its capability registration method and calls it after the method returned
- **THEN** the call fails with a `registration-closed` error

### Requirement: One event per fire site
A NeoForge event whose row in `mapping-table.md` says supported SHALL fire from the NeoForge fire site, with the NeoForge fields. Where Pumpkin has an event for that fire site, the row SHALL map to that event and the missing fields SHALL be added to it. A new struct with the NeoForge name SHALL exist only where Pumpkin has no event for that fire site. A test SHALL cover each supported row. Status: planned (tasks 2.13 and 2.15; the rows of tasks 2.3 and 2.12 are done, #102 and #112).

#### Scenario: Player login
- **WHEN** a player joins and a mod handles the Pumpkin event that the row of `PlayerEvent.PlayerLoggedInEvent` names
- **THEN** the handler runs once with that player

#### Scenario: Missing field added to a Pumpkin event
- **WHEN** a NeoForge event has a field that the mapped Pumpkin event lacks
- **THEN** the Pumpkin event carries the field, and no second struct for the same fire site exists

### Requirement: Every supported event reaches every mod
Every event case that the host fires SHALL reach the handlers of every loaded native mod, with write-back of the settable fields and cancellation through `cancelled`. An event that the host fires only to v0.2 plugins SHALL NOT count as supported. Each event family SHALL have a test for cancel and one for write-back. Status: planned (tasks 2.13 and 2.15; the server and player families are done, #102 and #112).

#### Scenario: A mod cancels the break of another mod's block
- **WHEN** mod A registers the block `moda:ore` and places it, mod B handles `BlockBreakEvent` and cancels it for that block, and a player breaks it
- **THEN** the block stays, mod A's block drops nothing, and no later handler without receive-canceled runs

#### Scenario: One test per event family
- **WHEN** the tests of an event family (server, tick and root package events; player events; level and block events; entity, living and item entity events) run a handler that changes a settable field, and one that cancels the event
- **THEN** the server acts on the changed value, and skips the action of the cancelled event

### Requirement: Wasm plugins see the same events
Every event case that the host fires SHALL reach the handlers of every loaded v0.2 Wasm plugin that listens to it, through the existing Pumpkin event pipeline and the same dispatch as for native mods, with write-back and cancellation. Per-tick and per-entity events SHALL reach Wasm plugins only in batches or not at all, under the performance rules of the Wasm executor. Status: planned (tasks 2.3, 2.12, 2.13 and 2.15).

#### Scenario: A Wasm plugin cancels the break of a native mod's block
- **WHEN** native mod A registers the block `moda:ore` and places it, a Wasm plugin handles `BlockBreakEvent` and cancels it for that block, and a player breaks it
- **THEN** the block stays, and mod A's block drops nothing

### Requirement: One dispatch for native and Wasm handlers
Native handlers and Wasm handlers of one event SHALL run in the single dispatch of `PluginManager::fire_dyn`, in one priority order, so that each sees the changes and the cancellation of the handlers before it. Status: planned (tasks 2.3 and 2.18).

#### Scenario: A native handler cancels, a Wasm handler observes
- **WHEN** a blocking native handler on `BlockBreakEvent` with priority `High` cancels a break, and a blocking Wasm handler with receive-canceled and priority `Normal` listens to the same event
- **THEN** the Wasm handler gets the event cancelled, and the block stays

#### Scenario: A Wasm handler cancels, a native handler observes
- **WHEN** a blocking Wasm handler with priority `High` cancels a `LivingIncomingDamageEvent`, and a blocking native handler with receive-canceled and priority `Low` listens to it
- **THEN** the native handler gets the event cancelled, and the entity takes no damage

### Requirement: One event pipeline
A world mutation that a player or an entity causes through the API of a mod (a world mutator of `ironpumpkin-mods`, called from a handler or from other mod code, or a v0.2 import of a Wasm plugin), including one from a mod projectile, SHALL fire the Pumpkin events of the vanilla action before the change, SHALL make no change when a handler cancels one, and SHALL reach v0.2 plugins and mods alike. No API function SHALL skip that dispatch. Status: planned (tasks 2.4 and 2.18).

#### Scenario: Every mutator is listed
- **WHEN** a reviewer reads the `level-access` docs
- **THEN** every mutator names the Pumpkin event it fires, or says "no event"

#### Scenario: A projectile of a mod breaks a block
- **WHEN** a player fires a mining laser projectile of mod A, the projectile calls `level-access.destroy-block` with itself as the breaker, and a v0.2 plugin cancels the Pumpkin `BlockBreakEvent`
- **THEN** the block stays, and a handler of mod B on `BlockBreakEvent` with receive-canceled gets the same event, cancelled, with the player that owns the projectile

#### Scenario: A plugin sees a mod spawn
- **WHEN** a native mod calls `level-access.spawn-entity` and a v0.2 plugin listens to the Pumpkin `EntitySpawnEvent`
- **THEN** the plugin receives the event before the entity joins the level, and cancelling it returns none to the mod

#### Scenario: A v0.2 world call from a Wasm plugin
- **WHEN** a Wasm plugin calls the v0.2 `world.spawn-entity` and another v0.2 plugin cancels the Pumpkin `EntitySpawnEvent`
- **THEN** no entity spawns, as for `level-access.spawn-entity` without a cause

#### Scenario: A mod cancels, a plugin observes
- **WHEN** a player breaks a block, a native handler on `BlockBreakEvent` with priority `High` sets `cancelled` to true, and a v0.2 plugin listens to the same event at a lower priority
- **THEN** the plugin receives the event already cancelled, and the block stays

#### Scenario: Internal machine state
- **WHEN** a mod calls `level-access.set-block` without a cause to change a part of its multiblock
- **THEN** the block changes and no break, place or entity change event fires
