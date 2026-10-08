# Spec Delta

## Purpose

Defines how ported NeoForge mods run as native Rust crates compiled into the server binary of a modpack, against the API crate `ironpumpkin-neo`: the build model, the names, the three primitives that replace mixins and reflection, the performance rules, the API shape and the licence of linked mods.

## ADDED Requirements

### Requirement: Mods linked at build time
`crates/pumpkin` SHALL expose a library entry point, and a modpack SHALL be a Cargo workspace whose bin crate depends on the `pumpkin` library and on one crate per mod. Each mod SHALL register itself at link time, and the server SHALL call each registered mod once, in mod id order, in the startup content phase before the first world loads. Status: planned (task 2.1.1).

#### Scenario: Example pack
- **WHEN** the example modpack workspace links one mod crate and its binary starts
- **THEN** the log names the mod as loaded, and the mod's registration code ran before the first world loaded

#### Scenario: Server without mods
- **WHEN** `cargo run -p pumpkin` starts the server with no mod linked
- **THEN** the server behaves as the upstream Pumpkin binary

### Requirement: Names mirror NeoForge
Each item of `ironpumpkin-neo` that stands for a NeoForge or Minecraft API element SHALL have the Rust form of the NeoForge 26.3 name, and its doc comment SHALL name the Java class or member it mirrors. The other delta specs of this change SHALL name items in interface notation (`level-access.destroy-block`), and that notation SHALL map one to one to the Rust item (`level_access::destroy_block`). Status: planned (task 2.1.1).

#### Scenario: Searchable names
- **WHEN** a mod author searches the `ironpumpkin-neo` docs for `DeferredRegister` or `PayloadRegistrar`
- **THEN** the item with the Rust form of that name comes up, and its doc comment names the Java class

### Requirement: Callbacks are registered trait objects
Code that NeoForge calls through a Java interface or lambda (listeners, providers, handlers, loot modifiers, validators) SHALL be a method of a trait object or a closure that the mod registers. Status: planned (task 2.1.1).

#### Scenario: Payload handler
- **WHEN** a mod registers a payload handler for a channel
- **THEN** the host calls that handler for each payload of the channel

### Requirement: Hook points
A `#[hook]` attribute on a Pumpkin function SHALL create a hook point with a stable id and a typed context. A pre listener SHALL run at entry and SHALL be able to cancel or return a value where the hook point allows it, a post listener SHALL run at return and SHALL be able to change the return value, and a value point inside the function SHALL let a listener change the value of one expression. Each hook point SHALL name the vanilla 26.3 method it mirrors. Status: planned (task 2.1.2).

#### Scenario: Cancel at entry
- **WHEN** a mod adds a pre listener to a cancellable hook point and cancels it
- **THEN** the body of the function does not run and the function returns the value the listener set

#### Scenario: Change a return value
- **WHEN** a mod adds a post listener that doubles the return value of a hook point
- **THEN** the caller of the function gets the doubled value

### Requirement: No-listener fast path
A hook point with no listener SHALL cost one atomic check and no call, and SHALL build no context. Status: planned (task 2.1.2).

#### Scenario: Bench
- **WHEN** the bench of task 2.1.2 runs one hook point with no listener, with a native listener and with a call into a Wasm plugin
- **THEN** the run with no listener is within the noise of the same function without `#[hook]`, and the issue records the three costs

### Requirement: Batched hot hooks
Hot hook points and events (entity tick, item tick and the other per-object points of one tick phase) SHALL be delivered once per tick phase with the batch of objects, not once per object, and a pre listener SHALL cancel single entries of the batch. Status: planned (tasks 2.1.2 and 2.15).

#### Scenario: Entity tick
- **WHEN** a mod listens to the entity tick of one entity type and 500 entities of that type tick
- **THEN** the listener runs once per tick with the 500 entities, and an entity it cancels does not tick

### Requirement: Filters evaluated at registration
A listener registered with a filter (entity type, block id, damage type) SHALL be indexed by the host under its filter key at registration, and the host SHALL NOT call it for an object outside its filter. Status: planned (task 2.1.2).

#### Scenario: Entity type filter
- **WHEN** a mod listens to the living hurt hook with the filter `minecraft:zombie` and a skeleton takes damage
- **THEN** the listener does not run

### Requirement: Build-time accessors
A member of a Pumpkin type that a mod needs and that the API does not expose SHALL be reachable through an accessor (getter, setter or invoker) generated at build time from the accessor catalogue, in safe Rust, into one generated module of the crate that owns the type, and re-exported by `ironpumpkin-neo` under the vanilla name. Status: planned (task 2.1.3).

#### Scenario: Private field
- **WHEN** the catalogue has an entry for a private field of a Pumpkin entity type and a mod calls its getter and setter
- **THEN** the mod reads and changes the field, and the generated code has no `unsafe`

### Requirement: Service seams
A service (recipes, loot, spawner, world generation, explosion, enchantment, brewing, tags, reload) SHALL be a trait that the server holds as a trait object with Pumpkin's code as the default implementation. A mod SHALL be able to decorate a service, which wraps the current implementation in mod id order, or replace it; a second replacement of one service SHALL stop the startup with an error that names both mods. Status: planned (task 2.1.4).

#### Scenario: Decorate the loot service
- **WHEN** a mod decorates the loot service to add one item to every chest loot roll
- **THEN** a chest loot roll returns the vanilla items and the added item

#### Scenario: Two replacements
- **WHEN** two mods replace the recipe service
- **THEN** the server does not start, and the error names both mods

### Requirement: Wasm service seams are coarse
A service seam implemented in Wasm SHALL be allowed only for a service that runs per reload, per command or per world load, and data formats (loot tables, recipes as JSON, tags) SHALL be preferred to any hook. Status: planned (task 2.1.4).

#### Scenario: Per-tick service
- **WHEN** a Wasm plugin tries to implement a service that runs per tick or per object
- **THEN** the registration fails with an error that names the service

### Requirement: Mixin state as attachments
State that a NeoForge mod adds to a vanilla class through `@Unique` fields SHALL be ported as an attachment of the entity, block entity, chunk or level (spec `neoforge-attachments`), not as a field of a Pumpkin type. Status: planned (task 2.14).

#### Scenario: Per-entity counter
- **WHEN** a mod keeps a counter per player that its Java version kept in a `@Unique` field
- **THEN** the port stores it in an attachment of the player, and it survives a save and load when the attachment type is serializable

### Requirement: API shape
Public types of `ironpumpkin-neo` on the mod boundary SHALL NOT be generic and SHALL have shapes that can be `repr(C)`: no generic types or methods, no `impl Trait` in signatures and no stored closure types. Listener, hook and service traits SHALL be synchronous; async work SHALL stay on the host side or behind a synchronous call or a completion callback. Status: planned (task 2.1.1).

#### Scenario: Precompiled mods stay possible
- **WHEN** a reviewer checks the public items of `ironpumpkin-neo` on the mod boundary
- **THEN** none is generic, returns `impl Trait` or stores a closure type

### Requirement: Versioned API dependency
`ironpumpkin-neo` SHALL be versioned with semver, and mods SHALL depend on `ironpumpkin-mods` and `ironpumpkin-neo`, which re-export the server types a mod needs. Status: planned (task 2.1.1).

#### Scenario: Upstream merge
- **WHEN** a `git merge upstream/master` changes a Pumpkin internal type that the API maps
- **THEN** only the host side changes, and an example mod builds unchanged against the same `ironpumpkin-neo` version

### Requirement: Licence of linked mods
`ironpumpkin-neo`, its macro crate and `ironpumpkin-mods` SHALL be licensed MIT OR Apache-2.0 and SHALL contain no server or NeoForge source. A mod linked into a pack binary SHALL have a GPL-3.0-compatible licence, because the binary is a derivative work of the GPL-3.0 server. Wasm plugins SHALL have no such constraint. Status: planned (task 2.1.1).

#### Scenario: Licence stated
- **WHEN** a mod author reads the README of `ironpumpkin-neo` or of the blueprint repository
- **THEN** it states that the API crate is MIT OR Apache-2.0 and that a mod linked into a pack binary must have a GPL-3.0-compatible licence

### Requirement: Long-tail requests
A hook point, accessor or service seam SHALL enter the catalogue only through a request that names the mixin it replaces (target class, method, injection point, intent) and the mod that needs it, and SHALL come with a test and the vanilla 26.3 source it mirrors. Status: planned (tasks 2.1.2, 2.1.3 and 2.1.4).

#### Scenario: New hook point
- **WHEN** a maintainer adds a hook point for a request
- **THEN** the hook point names the request, the vanilla 26.3 method and its test
