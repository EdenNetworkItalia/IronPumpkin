# Porting registration from NeoForge

In NeoForge, a mod creates one `DeferredRegister` for each registry. It adds entries as lambdas and registers the `DeferredRegister` on the mod bus. The game calls the lambdas when the registry opens. The mod keeps a `DeferredHolder` for each entry and calls `get()` later.

In IronPumpkin, a mod calls `ModInit::register_block`, `register_item`, `register_entity_type` or `register_tag` from `NativeMod::init`. Each call takes a builder and checks the definition at once. The server freezes the registry after the `init` of the last mod. The freeze gives each entry its numeric id.

---

## Key Differences

| Feature | NeoForge | IronPumpkin |
| :--- | :--- | :--- |
| **Register object** | One `DeferredRegister` for each registry (`Blocks`, `Items`, `Entities`) | `ModInit`, one handle for all registries |
| **When the code runs** | The mod bus fires `RegisterEvent`, and the game calls the lambdas | Each `register_*` call runs at once inside `NativeMod::init` |
| **Definition** | `BlockBehaviour.Properties`, `Item.Properties`, `EntityType.Builder` | `BlockBuilder`, `ItemBuilder`, `EntityTypeBuilder`, `TagBuilder` |
| **Handle** | `DeferredHolder`, `DeferredBlock`, `DeferredItem` | No handle. Keep the name and look it up after the freeze. |
| **Behaviour** | A subclass of `Block` or `Item` | An implementation of `BlockBehaviour` or `ItemBehaviour`, set with `.behaviour(..)` |
| **Look on a vanilla client** | A model and a texture from the mod | The `display` entry: the vanilla block state, item or entity type that the client shows. IronPumpkin only. |
| **Error** | An exception when the game opens the registry | `Err(RegistryError)`. The server stops after `init` returns. |
| **Tags** | JSON files in a data pack | `.tag(..)` on a builder, and `TagBuilder` for a tag of the mod |
| **Creative tab** | The `creative_mode_tab` registry | No function today. Accept-and-ignore for the registration is planned (task 2.5). |
| **Other registries** | Sound events, mob effects, data components, attributes | Planned (task 2.6). Fluids, recipe types and similar registries wait for M5. |

---

## Code Comparison: a block, its item and an entity type

### 1. NeoForge implementation (Java)

The code comes from `examples/modpack/client-mod`, a NeoForge 26.3 mod. It registers the same names as the `hello-mod` crate of the example pack.

```java
@Mod(HelloMod.MOD_ID)
public final class HelloMod {
    public static final String MOD_ID = "hello_mod";
    public static final String NAMESPACE = "hello-mod";

    private static final DeferredRegister.Blocks BLOCKS = DeferredRegister.createBlocks(NAMESPACE);
    private static final DeferredRegister.Items ITEMS = DeferredRegister.createItems(NAMESPACE);
    private static final DeferredRegister.Entities ENTITY_TYPES = DeferredRegister.createEntities(NAMESPACE);

    public static final DeferredBlock<RedstoneLampBlock> GREETER_LAMP = BLOCKS.registerBlock(
            "greeter_lamp", RedstoneLampBlock::new, () -> BlockBehaviour.Properties.ofFullCopy(Blocks.REDSTONE_LAMP));
    public static final DeferredItem<BlockItem> GREETER_LAMP_ITEM = ITEMS.registerSimpleBlockItem(GREETER_LAMP);
    public static final DeferredHolder<EntityType<?>, EntityType<Pig>> GREETER = ENTITY_TYPES.registerEntityType(
            "greeter", Pig::new, MobCategory.CREATURE, builder -> builder.sized(0.9F, 0.9F));

    public HelloMod(IEventBus modBus) {
        BLOCKS.register(modBus);
        ITEMS.register(modBus);
        ENTITY_TYPES.register(modBus);
    }
}
```

### 2. IronPumpkin implementation (Rust)

The snippet is a doc-test of `ironpumpkin-mods`: `cargo test --doc -p ironpumpkin-mods` compiles it. It follows `examples/modpack/mods/hello-mod`.

```rust
use std::sync::Arc;

use ironpumpkin_mods::{
    ModInit, NativeMod,
    content::{BlockBuilder, EntityTypeBuilder, ItemBuilder, RegistryError},
    entity::{Entity, EntityBase},
    pumpkin_data::{Block, entity::EntityType, item::Item},
    register_mod,
};

const LAMP: &str = "hello-mod:greeter_lamp";
const GREETER: &str = "hello-mod:greeter";

struct HelloMod;

impl NativeMod for HelloMod {
    fn id(&self) -> &'static str {
        "hello-mod"
    }

    fn display_name(&self) -> &'static str {
        "Hello Mod"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn init(&self, cx: &mut ModInit) {
        // `ModInit` keeps the first error. The server logs it and stops after `init` returns.
        let _ = register_content(cx);
    }
}

fn register_content(cx: &mut ModInit) -> Result<(), RegistryError> {
    cx.register_block(
        BlockBuilder::new(LAMP, Block::REDSTONE_LAMP.default_state.id).bool_property("lit", false),
    )?;
    cx.register_item(ItemBuilder::new(LAMP, &Item::REDSTONE_LAMP).places(LAMP))?;
    cx.register_entity_type(
        EntityTypeBuilder::new(GREETER, &EntityType::PIG).dimensions(0.9, 0.9),
        spawn_greeter,
    )
}

fn spawn_greeter(entity: Entity) -> Arc<dyn EntityBase> {
    Arc::new(entity)
}

register_mod!(HelloMod);
```

The mapping from the Java code to the Rust code:

| Java | Rust |
| :--- | :--- |
| `DeferredRegister.createBlocks(NAMESPACE)` | No object. The name carries the namespace: `"hello-mod:greeter_lamp"`. |
| `BLOCKS.registerBlock(..)` | `cx.register_block(BlockBuilder::new(..))` |
| `BlockBehaviour.Properties.ofFullCopy(Blocks.REDSTONE_LAMP)` | `Block::REDSTONE_LAMP.default_state.id` as the `display` state |
| `RedstoneLampBlock`, the `lit` state property | `.bool_property("lit", false)` |
| `ITEMS.registerSimpleBlockItem(GREETER_LAMP)` | `ItemBuilder::new(..).places(LAMP)` |
| `ENTITY_TYPES.registerEntityType(..)` | `cx.register_entity_type(EntityTypeBuilder::new(..), factory)` |
| `builder.sized(0.9F, 0.9F)` | `.dimensions(0.9, 0.9)` |
| `BLOCKS.register(modBus)` | Nothing. `init` runs once, and each call applies at once. |
| `GREETER.get()` | A lookup by name after the freeze. See "The freeze and lookups". |

---

## Names

A name is `namespace:path`. The namespace is not `minecraft`. The path uses `a-z`, `0-9`, `_`, `-`, `.` and `/`. A name without a namespace fails with `RegistryError::ReservedNamespace`. A malformed name fails with `RegistryError::InvalidName`.

The registry name of a block and of its item can be the same, as in `hello-mod:greeter_lamp`. The two registries are separate.

## The `display` entry

A vanilla client does not know the content of a mod. Each builder takes a `display` entry, which is a vanilla block state, item or entity type:

- `BlockBuilder::new(name, display)` takes a `BlockStateId`. Every state of the block copies the physical data of that state.
- `ItemBuilder::new(name, display)` takes a `&'static Item`. The item copies its components.
- `EntityTypeBuilder::new(name, display)` takes a `&'static EntityType`. A vanilla client renders that type.

`display` is the one IronPumpkin-only property of a block, an item or an entity type. NeoForge has no such entry. A NeoForge client that has the Java half of the mod sees the real content (see [What is unsupported](./unsupported.md)).

## Behaviours

`.behaviour(Arc::new(..))` sets a `BlockBehaviour` on a block and an `ItemBehaviour` on an item. These are the traits of the vanilla blocks and items of Pumpkin, so the hook names are the names of Pumpkin. The server calls them from the same places as for vanilla content. The NeoForge extension hooks (`IBlockExtension`, `IItemExtension`) are only partly covered. The mapping table lists the gaps.

```rust
use std::sync::Arc;

use ironpumpkin_mods::{
    content::{BlockBehaviour, BlockBuilder},
    pumpkin_core::block::PlacedArgs,
    pumpkin_data::Block,
    tracing::info,
};

struct GreeterLamp;

impl BlockBehaviour for GreeterLamp {
    fn placed(&self, args: PlacedArgs<'_>) {
        info!("[hello-mod] {} placed at {}", args.block.name, args.position);
    }
}

fn lamp() -> BlockBuilder {
    BlockBuilder::new(
        "hello-mod:greeter_lamp",
        Block::REDSTONE_LAMP.default_state.id,
    )
    .behaviour(Arc::new(GreeterLamp))
}
```

Content does not inherit the behaviour of its `display` entry. Without `.behaviour(..)`, the server uses the default of each hook. Attach only a behaviour of the mod, or a server behaviour that reads no block properties. A server behaviour that reads the properties of its own block panics on a custom block, and the panic stops the server.

## The freeze and lookups

The server runs the registration in this order:

1. It calls `init` of each mod, in the order of the mod ids. Each `register_*` call applies at once.
2. It freezes the registry after the last `init`. A mod tag that no mod registers fails here. An item that places an unknown block fails here.
3. The freeze gives numeric ids to the custom entries, after the ids of the vanilla entries. The order is the order of the names, so the mod load order does not change an id.
4. The first world loads.

After the freeze, the generated lookups of Pumpkin answer for custom entries too. A mod looks up an entry by name. It does not hold a handle.

```rust
use ironpumpkin_mods::{
    content::DynamicTaggable,
    pumpkin_data::{Block, item::Item},
};

/// Call this after the freeze, for example from a command.
fn lamp_is_registered() -> bool {
    Block::from_name("hello-mod:greeter_lamp").is_some()
        && Item::from_registry_key("hello-mod:greeter_lamp").is_some()
}

fn lamp_breaks_with_a_pickaxe() -> bool {
    Block::from_name("hello-mod:greeter_lamp")
        .is_some_and(|block| block.has_tag_dynamic("minecraft:mineable/pickaxe"))
}
```

Lookups by tag work for generated tags and for tags of mods. A tag of a mod is not visible to clients and data packs.

## Errors

A registration returns `Result<(), RegistryError>`. `ModInit` keeps the first error, even if the mod ignores the `Result`. The server logs the error with the mod id, and it stops after `init` returns. A mod cannot recover from a registration error and start with less content.

For example, a mod `greeter` registers the block `greeter:lamp` twice. The server logs this line and exits with status 1:

```text
ERROR [ironpumpkin] native mod "greeter" cannot register the block "greeter:lamp": block "greeter:lamp" is already registered; refusing to start
```

| Error | Cause | When |
| :--- | :--- | :--- |
| `Duplicate` | The name is already registered in this registry. | At the call |
| `RegistryFrozen` | The call runs after the freeze. | At the call |
| `ReservedNamespace` | The namespace is `minecraft`, or the name has none. | At the call |
| `InvalidName` | The name is not `namespace:path`. | At the call |
| `DisplayNotVanilla` | The `display` entry is not a vanilla entry. | At the call |
| `InvalidProperty` | A block property has a wrong name, too few values or a default outside the values. | At the call |
| `InvalidDimensions` | A width, a height or an eye height is not positive and finite. | At the call |
| `UnknownTag` | A `.tag(..)` names a tag that the registry does not have. | At the call for a vanilla tag. At the freeze for a mod tag. |
| `InvalidTagName`, `DuplicateTag`, `UnknownTagMember` | A `TagBuilder` has a wrong name, a taken name, or a vanilla member that does not exist. | At the call |
| `UnknownBlock`, `BlockAlreadyHasItem` | An item places a block that is not registered, or that has an item already. | At the freeze |

---

## Do and don't

**Do**

- Register all content from `NativeMod::init`. The registry accepts content only until the freeze.
- Return the `RegistryError` with `?` and stop at the first error.
- Use the namespace and the names of the Java mod. A NeoForge client matches the registry by name.
- Choose a `display` entry that looks like the content. A vanilla client shows it.
- Give a block that needs a pickaxe `.tag("minecraft:mineable/pickaxe")`.
- Look up content by name after the freeze.
- Put the content of the mod in a function that returns `Result<(), RegistryError>`, as `register_content` does above.

**Don't**

- Do not call `unwrap` or `expect` on a registration `Result`, and do not panic. The panic hook of the server writes a crash report and exits at once.
- Do not register content from an event handler or a command. The registry is frozen by then, and the call fails with `RegistryFrozen`.
- Do not store numeric ids. The server allocates them at the freeze.
- Do not use the `minecraft` namespace.
- Do not expect a handle like `DeferredHolder`. No such type exists today.
- Do not register a creative tab. No function exists for it. Accept-and-ignore is planned (task 2.5).
- Do not expect a custom block to run the behaviour of its `display` block.
