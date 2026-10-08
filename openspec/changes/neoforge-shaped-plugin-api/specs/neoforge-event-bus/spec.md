# Spec Delta

## Purpose

Lets a ported mod subscribe to NeoForge events by their NeoForge names on the mod bus and the game bus, with priorities, cancellation and results, backed by the Pumpkin events.

## ADDED Requirements

### Requirement: Concrete parents receive subclasses
A listener on a concrete event class that has concrete subclasses SHALL also receive each subclass event, delivered as the subclass case, as NeoForge delivers it to a listener on the parent type. Status: planned (task 2.3).

#### Scenario: Listener on a parent
- **WHEN** a mod listens to `living-entity-use-item-event` and an entity starts using an item
- **THEN** the listener receives a `living-entity-use-item-event-start` case

### Requirement: Posting on the mod bus reaches the posting mod only
`post` on `bus::game` SHALL run the listeners of every mod; `post` on `bus::mod` SHALL run only the listeners of the posting mod, because NeoForge has one mod bus per `ModContainer`. Status: planned (task 2.3).

#### Scenario: Mod bus post
- **WHEN** mod A posts an event on `bus::mod` and mods A and B both listen to that event type on their mod bus
- **THEN** only mod A's listener runs

### Requirement: Reentrant callbacks
Imports that run mod code before they return (`post`, capability lookups and handle methods, `transaction.commit`, `attachment-holder.get-data`, `set-data` and `sync-data`, `register-config`, `mod-config.set`, `roll-loot-table`) SHALL call back into the calling mod while the import is on its stack, up to the reentry depth of 64 of `pumpkin-plugin-runtime`, and SHALL fail beyond it. Status: planned (task 2.1).

#### Scenario: Post from a listener
- **WHEN** a listener of the calling mod runs during its own `post` call and reads its config
- **THEN** both calls complete, provided the mod holds no borrow of its own state across `post`

### Requirement: Listener registration
`add-listener` SHALL register a listener for one event type on the bus that posts it, with one of the five NeoForge priorities and the `receiveCanceled` flag, and SHALL return a listener id. A listener on the wrong bus SHALL fail with `wrong-bus`. Status: planned (task 2.3).

#### Scenario: Wrong bus
- **WHEN** a mod adds a listener for `fml-common-setup-event` on `bus::game`
- **THEN** the call fails with `event-bus-error::wrong-bus`

#### Scenario: Priority order
- **WHEN** two listeners for one event type have priorities `high` and `low`
- **THEN** the host calls the `high` listener first

### Requirement: Dispatch and write-back
The host SHALL call `callbacks.handle-event` with the listener id and the event, and SHALL apply the fields marked settable and `canceled` from the returned event before the next listener and before the server acts. Status: planned (task 2.3).

#### Scenario: Change a settable field
- **WHEN** a `living-heal-event` listener returns `amount` 2.0 for a heal of 4.0
- **THEN** the entity heals 2.0

### Requirement: Cancellation
A cancellable event SHALL carry `canceled`. When a listener returns it as true, the host SHALL skip later listeners that did not set `receive-canceled` and SHALL NOT perform the action the event describes. Status: planned (task 2.3).

#### Scenario: Cancel a block break
- **WHEN** a `break-block-event` listener returns `canceled` true
- **THEN** the block stays and no later listener without `receive-canceled` runs

### Requirement: Results
An event with a NeoForge result setter SHALL carry the result as a settable field of an enum type (`tri-state`, `interaction-result` or the event's own result enum), and the host SHALL apply the returned value as NeoForge does. Status: planned (task 2.3).

#### Scenario: Deny a despawn
- **WHEN** a `mob-despawn-event` listener returns `despawn-result` `deny`
- **THEN** the mob does not despawn

### Requirement: Registration events carry a resource
A mod bus event that hands the listener a registration API SHALL carry a resource of the interface that owns the API, and calls on that resource SHALL work only while the host dispatches the event. Status: planned (task 2.3).

#### Scenario: Late call
- **WHEN** a mod keeps the `register-capabilities-event` handle and calls it after the dispatch returned
- **THEN** the call fails with a `registration-closed` error

### Requirement: Supported events fire from their Pumpkin event
Each case whose doc comment says supported SHALL fire from the Pumpkin event named there once the event bus host is implemented, and a test SHALL cover each one. Status: planned (tasks 2.3, 2.12, 2.13 and 2.15).

#### Scenario: Player login
- **WHEN** a player joins and a mod listens to `player-event-player-logged-in-event`
- **THEN** the listener runs once with that player

### Requirement: Every supported event reaches Wasm guests
Every event case that the host fires SHALL reach the listeners of every loaded Wasm mod through `callbacks.handle-event`, with write-back of the settable fields and cancellation through `canceled`. An event that the host fires only natively or only to v0.2 plugins SHALL NOT count as supported. Each event family SHALL have a guest test for cancel and one for write-back. Status: planned (tasks 2.3, 2.12, 2.13 and 2.15).

#### Scenario: A guest cancels the break of another mod's block
- **WHEN** mod A registers the block `moda:ore` and places it, mod B listens to `break-block-event` and returns `canceled` true for that block, and a player breaks it
- **THEN** the block stays, mod A's block drops nothing, and no later listener without `receive-canceled` runs

#### Scenario: One test per event family
- **WHEN** the tests of an event family (server, tick and root package events; player events; level and block events; entity, living and item entity events) run a guest listener that changes a settable field, and one that cancels the event
- **THEN** the server acts on the changed value, and skips the action of the canceled event

### Requirement: One event pipeline
A world mutation that a player or an entity causes through a neo host function or a v0.2 import of a `neo-plugin` component, including one from a mod projectile, SHALL fire the Pumpkin events of the vanilla action before the change, and SHALL make no change when a listener cancels one. That dispatch SHALL reach v0.2 plugins and neo mods alike. Every mutator SHALL appear in the `level-access` doc with its event or as "no event". Status: planned (tasks 2.4 and 2.18).

#### Scenario: A projectile of a mod breaks a block
- **WHEN** a player fires a mining laser projectile of mod A, the projectile calls `level-access.destroy-block` with itself as the breaker, and a v0.2 plugin cancels the Pumpkin `BlockBreakEvent`
- **THEN** the block stays, and a neo listener of mod B on `break-block-event` with `receive-canceled` gets the same event, canceled, with the player that owns the projectile

#### Scenario: A plugin sees a mod spawn
- **WHEN** a neo mod calls `level-access.spawn-entity` and a v0.2 plugin listens to the Pumpkin `EntitySpawnEvent`
- **THEN** the plugin receives the event before the entity joins the level, and cancelling it returns none to the mod

#### Scenario: A v0.2 world call from a neo mod
- **WHEN** a `neo-plugin` component calls the v0.2 `world.spawn-entity` and a v0.2 plugin cancels the Pumpkin `EntitySpawnEvent`
- **THEN** no entity spawns, as for `level-access.spawn-entity` without a cause

#### Scenario: A neo mod cancels, a plugin observes
- **WHEN** a player breaks a block, a neo listener on `break-block-event` returns `canceled` true, and a v0.2 plugin listens to the Pumpkin `BlockBreakEvent` at a lower priority
- **THEN** the plugin receives the event already cancelled, and the block stays

#### Scenario: Internal machine state
- **WHEN** a mod calls `level-access.set-block` without a cause to change a part of its multiblock
- **THEN** the block changes and no break, place or entity change event fires
