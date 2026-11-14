# Porting capabilities and the transfer API from NeoForge

In NeoForge, a block, an entity or an item exposes a handler to other mods through a capability. A mod creates the capability key with `BlockCapability.create`, registers providers in `RegisterCapabilitiesEvent`, and another mod gets the handler with `level.getCapability(cap, pos, side)`. The handlers of the transfer API (`ResourceHandler<ItemResource>`, `ResourceHandler<FluidResource>`, `EnergyHandler`) take a `Transaction`: the changes take effect when the root transaction commits.

In IronPumpkin, the `capabilities` and `transfer` modules of `ironpumpkin-mods` have the same keys, providers, lookups, caches and transactions, with Rust types. A handler type is a trait object, and a lookup returns an `Arc` of it. The registration event is the `NativeMod::register_capabilities` method.

---

## Key Differences

| Feature | NeoForge | IronPumpkin |
| :--- | :--- | :--- |
| **Key** | `BlockCapability.create(name, type, context)`, `createSided`, `createVoid` | `BlockCapability::<dyn MyHandler, Option<BlockDirection>>::create(name)`, `create_sided`, `create_void`. The same for `EntityCapability` and `ItemCapability`. |
| **Type check** | `IllegalStateException` for an existing name with other classes | `Err(CapabilityError::TypeMismatch)` |
| **Built-in keys** | `Capabilities.Item.BLOCK`, `Capabilities.Fluid.ITEM`, `Capabilities.Energy.ENTITY` and the others | `capabilities::item::BLOCK`, `capabilities::fluid::ITEM`, `capabilities::energy::ENTITY` and the others |
| **Registration** | `RegisterCapabilitiesEvent` on the mod bus, with a listener priority | `NativeMod::register_capabilities(&self, event)`. The host calls it once per priority, from `Highest` to `Lowest`; `event.phase()` names it. |
| **Providers** | `registerBlock`, `registerBlockEntity`, `registerEntity`, `registerItem` | `register_block`, `register_block_entity`, `register_entity`, `register_item` with closures and registry names such as `"mymod:tank"` |
| **Lookup** | `level.getCapability(cap, pos, side)`, `entity.getCapability(cap, ctx)`, `stack.getCapability(cap, ctx)` | `cap.get_capability(&world, &pos, side)`, `cap.get_capability(&entity, ctx)`, `cap.get_capability(&stack, ctx)` |
| **Cache** | `BlockCapabilityCache.create` | `BlockCapabilityCache::create` and `create_with_listener`. `get_capability` returns `Result`. |
| **Invalidation** | `level.invalidateCapabilities(pos)` | `capabilities::invalidate_capabilities(&world, &pos)` |
| **Transactions** | `try (var tx = Transaction.openRoot())`, `tx.commit()` | `let mut tx = Transaction::open_root()?;` and `tx.commit()`. A drop without a commit rolls back. |
| **Nested transaction** | `Transaction.open(parent)` | `Transaction::open(Some(&mut parent))` |
| **Indexed insert** | `insert(index, resource, amount, tx)` | `insert_at(index, &resource, amount, &mut tx)`. `insert(&resource, amount, &mut tx)` fills the indices in order. |
| **Amounts** | `int`, negative amounts throw | `u32` |
| **Own storage** | `ItemStacksResourceHandler`, `SnapshotJournal` | `ItemStacksResourceHandler`, `FluidStacksResourceHandler`, `SimpleEnergyHandler`, and `SnapshotJournal::new` over a type that implements `Journal` |

---

## Code Comparison: a tank that exposes fluids

### 1. NeoForge implementation (Java)

```java
@SubscribeEvent
public static void registerCapabilities(RegisterCapabilitiesEvent event) {
    event.registerBlock(Capabilities.Fluid.BLOCK,
            (level, pos, state, blockEntity, side) -> TankStorage.at(level, pos),
            ExampleBlocks.TANK.get());
}

// Another mod pours a bucket into whatever is below.
ResourceHandler<FluidResource> handler = level.getCapability(Capabilities.Fluid.BLOCK, pos.below(), Direction.UP);
if (handler != null) {
    try (var tx = Transaction.openRoot()) {
        if (handler.insert(FluidResource.of(Fluids.WATER), FluidType.BUCKET_VOLUME, tx) == FluidType.BUCKET_VOLUME) {
            tx.commit();
        }
    }
}
```

### 2. IronPumpkin implementation (Rust)

```rust
use ironpumpkin_mods::{
    capabilities::{RegisterCapabilitiesEvent, fluid},
    event::EventPriority,
    transfer::{BUCKET_VOLUME, FluidResource, ResourceHandler, Transaction},
};

impl NativeMod for ExampleMod {
    // id, display_name, version and init as usual.

    fn register_capabilities(&self, event: &mut RegisterCapabilitiesEvent) {
        if *event.phase() != EventPriority::Normal {
            return;
        }
        // The event keeps a failed call, and the server does not start.
        let _ = event.register_block(
            &fluid::BLOCK,
            |world, pos, _state, _block_entity, _side| {
                let tank: Arc<dyn ResourceHandler<FluidResource>> = tank_storage(world, *pos);
                Some(tank)
            },
            &["examplemod:tank"],
        );
    }
}

// Another mod pours a bucket into whatever is below.
if let Some(handler) = fluid::BLOCK.get_capability(&world, &pos.down(), Some(BlockDirection::Up)) {
    let mut tx = Transaction::open_root()?;
    if handler.insert(&FluidResource::of(&Fluid::WATER), BUCKET_VOLUME, &mut tx) == BUCKET_VOLUME {
        tx.commit();
    }
}
```

`crates/ironpumpkin-test-mod` exposes the item storage of its block this way, and its test `tests/neoforge_capabilities.rs` reads it from a second mod, a hopper and a dropper.

---

## Phases of the registration event

NeoForge posts `RegisterCapabilitiesEvent` priority by priority across all mods. IronPumpkin calls `register_capabilities` of every mod in load order for `EventPriority::Highest`, then for `High`, and so on to `Lowest`. The host acts as the `neoforge` mod, first in each phase. It marks `item::BLOCK`, `fluid::BLOCK` and `energy::BLOCK` proxyable at `High` and registers the vanilla providers at `Normal`.

- Register most providers at `Normal`.
- Register a fallback at `Low`, after `event.is_block_registered(&cap, block)` says no other mod provides it. The answer does not depend on the load order.
- Mark a capability with `set_proxyable` or `set_non_proxyable` at `High` or earlier. `capabilities::get_all_proxyable()` is complete after `High`.

The event fires once per process, after `dedicated_server_setup` and before `ExtendPoiTypesEvent`, as `RegistrationEvents.init` does. A call after the last phase fails with `CapabilityError::RegistrationClosed`.

## Transactions

A transaction belongs to the thread that opened it, and each thread has one root at a time. `Transaction::open_root` fails with `TransactionError::RootAlreadyOpen` while a root is open. `Transaction` is not `Send`, so it cannot live across an `.await` in a Pumpkin event handler: open it, do the transfer, and commit or drop it in one synchronous block. A nested transaction borrows its parent, so the parent is unusable until the nested one closes.

Worlds and block entities tick in parallel, so roots of two threads can reach the same storage. A journal belongs to the root that touched it first until that root closes, on commit, on abort and on a panic. A transaction of another thread transfers nothing in it meanwhile: `insert` and `extract` return 0, and the caller tries again on a later tick, as a hopper does when the target is full. Nothing blocks.

A handler of your own makes its state transactional with a journal. Implement `Journal` (`create_snapshot`, `revert_to_snapshot`, and `on_root_commit` to mark a block dirty) and wrap the state in `SnapshotJournal::new`. In `insert` and `extract`, call `claim(tx)` before you read the state that the change depends on, and `update_snapshots(tx)` before each change. Both return `false` while another root owns the journal: then return 0 and change nothing. `ItemStacksResourceHandler`, `FluidStacksResourceHandler` and `SimpleEnergyHandler` do this already, with one journal per index, and `with_on_contents_changed(|index, previous| ..)` is `onContentsChanged`.

A panic in a journal callback does not stop the other callbacks of the transaction: every journal reverts or commits, and the first panic goes on after the thread is consistent again.

The handlers over Pumpkin inventories write to the inventory at once and roll back on an abort. Code outside the transaction, such as a Pumpkin hopper or a player click, can change the slot in between, so a rollback adds or takes back only the amount that the transaction moved. When another writer replaced the item, the rollback keeps that item and logs a warning. NeoForge restores the saved stack instead.

The wrappers apply the vanilla container rules (`canPlaceItem`, `getMaxStackSize`, the faces of `WorldlyContainer`): one potion input per brewing stand bottle slot and brewing reagents on top, one record in a jukebox, one book per chiseled bookshelf slot, the even fill of a crafter. A transactional write does not trigger the side effects of `setItem` early, as `insideTransaction` does on NeoForge: the cooking progress of a furnace resets at the root commit, only when the input became empty or was empty, and an insert into an empty hopper sets its cooldown inside the transaction. The Pumpkin jukebox and chiseled bookshelf have no side effect in `set_stack`, so a record that a capability inserts does not start a song and a book does not show on the shelf, as with a Pumpkin hopper.

## Built-in capabilities and vanilla providers

| Capability | Vanilla providers in IronPumpkin |
| :--- | :--- |
| `item::BLOCK` | Chests, trapped chests and copper chests (a double chest gives both halves), furnaces, blast furnaces, smokers, brewing stands and shulker boxes with the vanilla slots of each face, barrels, chiseled bookshelves, dispensers, droppers, hoppers, jukeboxes, crafters and shelves. The composter has none: Pumpkin has no composter container. |
| `item::ENTITY` | Players. Chest boats, minecarts with a chest or a hopper, horses and the hands and armor of other living entities have none yet. |
| `item::ENTITY_AUTOMATION` | None: Pumpkin hoppers do not reach entities yet. |
| `item::ITEM` | None: the shulker box and bundle items need a handler over their container component. |
| `fluid::BLOCK`, `fluid::ENTITY`, `fluid::ITEM` | None: the cauldron and the buckets need fluid storage that Pumpkin lacks. Mods provide them. |
| `energy::BLOCK`, `energy::ENTITY`, `energy::ITEM` | None, as in vanilla. Mods provide them. |

A hopper or a dropper that faces a block without a vanilla inventory uses the `item::BLOCK` capability of that block, as NeoForge does in `VanillaInventoryCodeHooks`. A hopper pushes one item from the side it faces and pulls one item from the block above, looked up from below. A dropper drops one item through the side that faces it.

## Caches

`BlockCapabilityCache::create_with_listener(cap, &world, pos, context, is_valid, listener)` keeps the handler until the host invalidates the position. The host does so when a block entity is placed or removed, when a chest changes its type and when the chunk loads or unloads. A chunk load or unload invalidates at the next tick of the world, where Pumpkin collects its chunk changes. The listener runs only when the cache was read since the last invalidation. A block without a block entity calls `invalidate_capabilities(&world, &pos)` itself when its handler changes. After `is_valid` returns `false`, `get_capability` returns `Err(CapabilityError::InvalidCache)`.

## Do and don't

**Do**

- Create each capability key once, in a `LazyLock`, and use the same handler and context types everywhere.
- Check the phase in `register_capabilities`: the method runs five times.
- Open the transaction, transfer and commit in one synchronous block.
- Call `invalidate_capabilities` when the handler of a plain block changes or goes away.

**Don't**

- Do not keep a `Transaction` across an `.await`. It does not compile.
- Do not ignore a `false` from `claim` or `update_snapshots`: another thread owns the journal, and a change now would be lost on its abort.
- Do not open a root transaction inside a handler call: use `Transaction::open(Some(tx))` on the transaction you got.
- Do not cache a handler from `get_capability` across ticks without a `BlockCapabilityCache`: the block can change.
- Do not expect `item::ENTITY_AUTOMATION` or the item providers of shulker boxes and bundles yet.
