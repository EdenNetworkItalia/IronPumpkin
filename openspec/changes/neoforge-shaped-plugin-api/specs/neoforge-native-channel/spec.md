# Spec Delta

## Purpose

Defines how ported NeoForge mods run as native Rust crates compiled into the server binary of a modpack, with two primitives: the NeoForge-shaped API crate `ironpumpkin-neo`, and source patches that a mod carries and the modpack build applies before it compiles. It covers the build model, the names, the API surface, the patch contract and its governance, the API shape and the licence of linked mods and patches.

## ADDED Requirements

### Requirement: Mods linked at build time
`crates/pumpkin` SHALL expose a library entry point, and a modpack SHALL be a Cargo workspace whose bin crate depends on the `pumpkin` library and on one crate per mod. Each mod SHALL register itself at link time, and the server SHALL call each registered mod once, in mod id order, in the startup content phase before the first world loads. Status: supported (task 2.1.1, landed with #52).

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

### Requirement: NeoForge public API and nothing more
The surface of `ironpumpkin-neo` that mods see SHALL be the NeoForge public API of the inventory in design.md and nothing more: no hook, accessor or service seam of IronPumpkin's own. A hook point for a mod SHALL be a NeoForge event. Internal hooks that the host puts on Pumpkin methods to fire NeoForge events SHALL NOT be reachable from a mod. Status: planned (tasks 2.2 to 2.18).

#### Scenario: No IronPumpkin-only hook
- **WHEN** a reviewer checks the public items of `ironpumpkin-neo`
- **THEN** each item mirrors a NeoForge class, member or event of the inventory, and no item names a Pumpkin function as a hook point

### Requirement: The API grows from patches
An event that the NeoForge inventory does not have SHALL enter `ironpumpkin-neo` only in the NeoForge shape, only to replace an accepted source patch, and with a row in the inventory. Status: planned (tasks 2.2 to 2.18).

#### Scenario: A new event comes from a patch
- **WHEN** a maintainer adds an event that the inventory does not have
- **THEN** the event has the NeoForge shape, its doc comment names the accepted patch it replaces, and the inventory has its row

### Requirement: Event fire sites with no listener
A fire site of a NeoForge event in the server SHALL cost one atomic check and no call when the event has no listener, and SHALL build no event object. Status: planned (task 2.3).

#### Scenario: Server without listeners
- **WHEN** the server runs with no mod that listens to the living hurt event, and an entity takes damage
- **THEN** the damage code builds no event object and calls no dispatch

### Requirement: Source patches declared by the mod
A mod crate that needs a change to the server SHALL ship it as `patches/<name>.patch` files against the IronPumpkin source tree (unified diff, paths from the repository root) and SHALL declare the IronPumpkin commit that the patches are written for in `[package.metadata.ironpumpkin] commit` of its `Cargo.toml`. Status: planned (task 2.1.2).

#### Scenario: Patch without commit
- **WHEN** a mod ships a patch and its `Cargo.toml` has no `[package.metadata.ironpumpkin] commit`
- **THEN** the modpack build fails before it compiles, and the message names the mod and the patch

### Requirement: Mixins become source patches
A mixin, a use of reflection or an access transformer of the Java mod SHALL be ported as a NeoForge event listener or API call when the inventory has one that does the same, and as a source patch otherwise. Status: planned (task 2.1.2).

#### Scenario: Mixin into a method that fires an event
- **WHEN** a Java mod cancels `LivingEntity.hurt` with a mixin at its head
- **THEN** the port listens to `living-incoming-damage-event` and cancels it, and ships no patch for it

### Requirement: Patch application
The modpack build SHALL check out IronPumpkin at the commit that `modpack.toml` pins, SHALL apply the patches of every mod in the order of the keys of `modpack.toml`, which are the crate package names, in byte order, then by file name, with `git apply` and no fuzz, no whitespace leniency and no three-way merge. It SHALL fail the build on a patch that does not apply, and on a patch written for another commit unless the pack allows drift. Status: planned (task 2.1.2).

#### Scenario: A valid patch builds
- **WHEN** a pack pins commit C and lists a mod whose patch is written for C, applies cleanly and has its justification file
- **THEN** the build applies the patch, compiles the server with the mod, and the binary boots with the changed code

#### Scenario: Order of the mods
- **WHEN** a pack lists the mod crates `hello_mod` and `hello-mod`, and both ship a patch
- **THEN** the patch of `hello-mod` applies first, because `-` comes before `_` in byte order

#### Scenario: A patch for another commit
- **WHEN** a pack pins commit C, does not set `allow-drift`, and lists a mod whose `[package.metadata.ironpumpkin] commit` is D
- **THEN** the build fails before it compiles, and the message names the mod, the patch, commit D and commit C

#### Scenario: Drift allowed
- **WHEN** the same pack sets `allow-drift = true` under `[ironpumpkin]` in `modpack.toml`, and the patch written for D applies cleanly on C
- **THEN** the build prints a warning that names the mod, the patch and both commits, applies the patch and builds

#### Scenario: Two mods change the same hunk
- **WHEN** mod `a` and mod `b` both ship a patch that changes the same lines of one file
- **THEN** the patch of `a` applies, the build fails on the patch of `b`, and the message names both mods and the file

### Requirement: Patch limits
A source patch SHALL change the server source only. The modpack build SHALL reject a patch that touches a file under `crates/pumpkin-data/src/generated` or a `Cargo.lock`, and SHALL NOT apply it. Status: planned (task 2.1.2).

#### Scenario: A patch touches a generated file
- **WHEN** a mod ships a patch that changes `crates/pumpkin-data/src/generated/block.rs`
- **THEN** the build fails, and the message names the mod, the patch and the file

#### Scenario: A patch touches Cargo.lock
- **WHEN** a mod ships a patch that changes `Cargo.lock`
- **THEN** the build fails, and the message names the mod, the patch and the file

### Requirement: Patch justification
A source patch SHALL be accepted in a mod only with a justification file `patches/<name>.md` next to it that says what the patch does, why a NeoForge event or API is not enough, and that the patch is bound to the IronPumpkin commit it names. Every accepted patch SHALL be a candidate NeoForge-shaped event for a later version of `ironpumpkin-neo`. Status: planned (task 2.1.2).

#### Scenario: Missing justification
- **WHEN** a mod ships `patches/fall-damage.patch` and no `patches/fall-damage.md`
- **THEN** the build fails before it applies the patch, and the message names the mod and the patch

### Requirement: Mixin state as attachments
State that a NeoForge mod adds to a vanilla class through `@Unique` fields SHALL be ported as an attachment of the entity, block entity, chunk or level (spec `neoforge-attachments`), not as a field of a Pumpkin type. Status: planned (task 2.14).

#### Scenario: Per-entity counter
- **WHEN** a mod keeps a counter per player that its Java version kept in a `@Unique` field
- **THEN** the port stores it in an attachment of the player, and it survives a save and load when the attachment type is serializable

### Requirement: API shape
Public types of `ironpumpkin-neo` on the mod boundary SHALL NOT be generic and SHALL have shapes that can be `repr(C)`: no generic types or methods, no `impl Trait` in signatures and no stored closure types. Listener and callback traits SHALL be synchronous; async work SHALL stay on the host side or behind a synchronous call or a completion callback. Status: planned (task 2.1.1).

#### Scenario: Precompiled mods stay possible
- **WHEN** a reviewer checks the public items of `ironpumpkin-neo` on the mod boundary
- **THEN** none is generic, returns `impl Trait` or stores a closure type

### Requirement: Versioned API dependency
`ironpumpkin-neo` SHALL be versioned with semver, and mods SHALL depend on `ironpumpkin-mods` and `ironpumpkin-neo`, which re-export the server types a mod needs. Status: planned (task 2.1.1).

#### Scenario: Upstream merge
- **WHEN** a `git merge upstream/master` changes a Pumpkin internal type that the API maps
- **THEN** only the host side changes, and an example mod builds unchanged against the same `ironpumpkin-neo` version

### Requirement: Licence of linked mods
`ironpumpkin-neo`, its macro crate and `ironpumpkin-mods` SHALL be licensed MIT OR Apache-2.0 and SHALL contain no server or NeoForge source. A mod linked into a pack binary SHALL have a GPL-3.0-compatible licence, because the binary is a derivative work of the GPL-3.0 server, and a source patch SHALL be distributed under terms compatible with GPL-3.0 for the same reason. Wasm plugins SHALL have no such constraint. Status: planned (task 2.1.1).

#### Scenario: Licence stated
- **WHEN** a mod author reads the README of `ironpumpkin-neo` or of the blueprint repository
- **THEN** it states that the API crate is MIT OR Apache-2.0, that a mod linked into a pack binary must have a GPL-3.0-compatible licence, and that a source patch is a derivative work of the server
