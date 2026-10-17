# Porting registration from NeoForge

In NeoForge, a mod creates one `DeferredRegister` for each registry. It adds entries as lambdas and registers the `DeferredRegister` on the mod bus. The game calls the lambdas when the registry opens. The mod keeps a `DeferredHolder` for each entry and calls `get()` later.

In IronPumpkin, a mod calls `ModInit::register_block`, `register_item`, `register_entity_type` or `register_tag` from `NativeMod::init`, or registers from a `RegisterEvent` handler. Each call takes a builder and checks the definition at once. The server freezes the registry after the lifecycle steps of all mods. The freeze gives each entry its numeric id. After the freeze, the module `ironpumpkin_mods::registry` looks up entries and tags.

---

## Key Differences

| Feature | NeoForge | IronPumpkin |
| :--- | :--- | :--- |
| **Register object** | One `DeferredRegister` for each registry (`Blocks`, `Items`, `Entities`) | `ModInit`, one handle for all registries |
| **When the code runs** | The mod bus fires `RegisterEvent`, and the game calls the lambdas | Each `register_*` call runs at once inside `NativeMod::init`, or inside a `RegisterEvent` handler |
| **Definition** | `BlockBehaviour.Properties`, `Item.Properties`, `EntityType.Builder` | `BlockBuilder`, `ItemBuilder`, `EntityTypeBuilder`, `TagBuilder` |
| **Handle** | `DeferredHolder`, `DeferredBlock`, `DeferredItem` | No handle. Keep the name and look it up after the freeze with `ironpumpkin_mods::registry`. |
| **Behaviour** | A subclass of `Block` or `Item` | An implementation of `BlockBehaviour` or `ItemBehaviour`, set with `.behaviour(..)`, or the server behaviour of a vanilla block or item, set with `.block_type(..)` or `.item_type(..)` |
| **Other registration events** | `EntityAttributeCreationEvent` and four more listeners on the mod bus | Methods of `NativeMod` that get a handle. See "Registration events". |
| **Look on a vanilla client** | A model and a texture from the mod | The `display` entry: the vanilla block state, item or entity type that the client shows. IronPumpkin only. |
| **Error** | An exception when the game opens the registry | `Err(RegistryError)`. The server stops after `init` returns. |
| **Tags** | JSON files in a data pack | `.tag(..)` on a builder, and `TagBuilder` for a tag of the mod |
| **Creative tab** | The `creative_mode_tab` registry | `ModInit::register_creative_mode_tab` accepts the tab, logs one line and ignores it. A creative tab is client UI. |
| **Other registries** | Sound events, mob effects, data components, attributes | `register_sound_event`, `register_mob_effect`, `register_attribute`, `register_data_component_type`. See [Data components, sounds, mob effects and attributes](./components-effects-attributes.md). Fluids, recipe types and similar registries wait for M5. |

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

The snippet is a doc-test of `ironpumpkin-mods`: `cargo test --doc -p ironpumpkin-mods` compiles it. It follows `examples/modpack/mods/hello-mod`. That mod keeps the text of `MODS_TOML` in the file `neoforge.mods.toml` and reads it with `include_str!`. The mod id is `hello_mod`, as in Java. The `namespace` key gives the registry namespace `hello-mod`.

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
const MODS_TOML: &str = r#"
[[mods]]
modId = "hello_mod"
namespace = "hello-mod"
version = "${file.jarVersion}"
displayName = "Hello Mod"
"#;

struct HelloMod;

impl NativeMod for HelloMod {
    fn id(&self) -> &'static str {
        "hello_mod"
    }

    fn display_name(&self) -> &'static str {
        "Hello Mod"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn mods_toml(&self) -> Option<&'static str> {
        Some(MODS_TOML)
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
| `DeferredRegister.createBlocks(NAMESPACE)` | No object. The name carries the namespace: `"hello-mod:greeter_lamp"`. The `namespace` key of `neoforge.mods.toml` names it too. |
| `BLOCKS.registerBlock(..)` | `cx.register_block(BlockBuilder::new(..))` |
| `BlockBehaviour.Properties.ofFullCopy(Blocks.REDSTONE_LAMP)` | `Block::REDSTONE_LAMP.default_state.id` as the `display` state |
| `RedstoneLampBlock`, the `lit` state property | `.bool_property("lit", false)` |
| `ITEMS.registerSimpleBlockItem(GREETER_LAMP)` | `ItemBuilder::new(..).places(LAMP)` |
| `ENTITY_TYPES.registerEntityType(..)` | `cx.register_entity_type(EntityTypeBuilder::new(..), factory)` |
| `builder.sized(0.9F, 0.9F)` | `.dimensions(0.9, 0.9)` |
| `BLOCKS.register(modBus)` | Nothing. `init` runs once, and each call applies at once. |
| `GREETER.get()` | A lookup by name after the freeze. See "The freeze and lookups". |
| `modBus.addListener(RegisterEvent.class, ..)` | `cx.register_event::<RegisterEvent, _>(..)`. See "RegisterEvent". |

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

Content does not inherit the behaviour of its `display` entry. Without `.behaviour(..)` or a type, the server uses the default of each hook. Attach only a behaviour of the mod, or a server behaviour that reads no block properties. A server behaviour that reads the properties of its own block panics on a custom block, and the panic stops the server. Use a block type for such a behaviour.

### Behaviour by type

A NeoForge mod block often extends a vanilla class, such as `StairBlock`, and gets its states, shapes and behaviour. In IronPumpkin, `.block_type(..)` names the vanilla block whose server behaviour the block runs. Minecraft 26.3 has no `minecraft:block_type` registry, so a vanilla block names the type. Name the vanilla block of the same kind and material: `minecraft:oak_stairs` for a wooden `StairBlock`, `minecraft:stone_stairs` for a stone one, `minecraft:oak_door` for a wooden `DoorBlock`. The block then:

- runs the server behaviour of the type when the builder sets no `.behaviour(..)`. A behaviour of the mod still wins.
- gets the properties of the type when the builder adds none. A builder that adds properties must list the same names and values. The registry rejects other properties with `RegistryError::InvalidBlockType`.
- copies the physical data of the matching state of the type: the shapes, the hardness, the instrument, the piston behaviour and the light. A stair has the stair shape in each state.
- joins the generated tags of the type, so the server sees a stair as `minecraft:stairs`. The tags follow the material: `minecraft:oak_stairs` is in `minecraft:wooden_stairs` and `minecraft:mineable/axe`, so a stone stair named with it breaks fastest with an axe.
- does not get the block entity of the type. A type with a block entity, such as `minecraft:barrel`, `minecraft:chest`, `minecraft:furnace` or a sign, also needs `BlockEntityTypeAddBlocksEvent` for the block. Without it the behaviour of the type finds no block entity and does nothing, and the server logs a warning at the freeze.
- shows a vanilla client the matching state of its `display` block when that block has the same properties as the type, else the `display` state.

`.item_type(..)` gives an item the server behaviour of a vanilla item, such as `minecraft:bucket`. A server behaviour that compares its block or item with a vanilla constant treats the custom entry as another block or item.

```rust
use ironpumpkin_mods::{
    ModInit,
    content::{BlockBuilder, ItemBuilder, RegistryError},
    pumpkin_data::{Block, item::Item},
};

fn register_content(cx: &mut ModInit) -> Result<(), RegistryError> {
    // `new StairBlock(..)`: the states, shapes and placement of an oak stair.
    cx.register_block(
        BlockBuilder::new("hello-mod:tin_stairs", Block::OAK_STAIRS.default_state.id)
            .block_type("minecraft:oak_stairs"),
    )?;
    // `new BucketItem(Fluids.EMPTY, ..)`: picks up water and lava.
    cx.register_item(
        ItemBuilder::new("hello-mod:tin_bucket", &Item::BUCKET).item_type("minecraft:bucket"),
    )
}
```

## The freeze and lookups

The server runs the registration in this order:

1. It calls `init` of each mod, in load order: dependency order, with the mod id as tie-break. Each `register_*` call applies at once.
2. It fires `RegisterEvent` once for each registry: `minecraft:block`, then `minecraft:entity_type`, then `minecraft:item`. A handler can register content. See "RegisterEvent".
3. It runs the other lifecycle steps and loads the configs. See [Lifecycle](./lifecycle.md). A mod has no `ModInit` there, so it registers no content.
4. It freezes the registry. A mod tag that no mod registers fails here. An item that places an unknown block fails here.
5. The freeze gives numeric ids to the custom entries, after the ids of the vanilla entries. The order is the order of the names, so the mod load order does not change an id.
6. The first world loads. Look up custom content from this point, for example in a `ServerStartingEvent` handler.

After the freeze, the lookups answer for custom entries too. A mod looks up an entry by name. It does not hold a handle. The module `ironpumpkin_mods::registry` has the `Registry` and tag lookups of NeoForge for the block, item and entity type registries:

| NeoForge | `ironpumpkin_mods::registry` |
| :--- | :--- |
| `Registry#containsKey` | `contains(kind, name)` |
| `Registry#getId` | `id_of(kind, name)`, `None` for an unknown name |
| `Registry#getKey` | `key_of(kind, id)`, `None` for an unknown id |
| `Registry#keySet` | `keys(kind)` |
| `Holder#is(TagKey)`, `ItemStack#is(TagKey)` | `is_in_tag(kind, name, tag)` |
| `Registry#get(TagKey)`, a `HolderSet.Named` | `tag_entries(kind, tag)`, `None` for an unknown tag |

The generated lookups of Pumpkin, such as `Block::from_name`, answer for custom entries too. `Block::from_name` also returns a placeholder entry, which stands in for the content of a mod that the world has and the server lacks. `registry::contains` excludes placeholders, so use it to test that an entry exists.

```rust
use ironpumpkin_mods::{
    content::ContentKind,
    pumpkin_data::{Block, item::Item},
    registry,
};

/// Call this after the freeze, for example from a command.
fn lamp_is_registered() -> bool {
    registry::contains(ContentKind::Block, "hello-mod:greeter_lamp")
        && Item::from_registry_key("hello-mod:greeter_lamp").is_some()
}

fn lamp_breaks_with_a_pickaxe() -> bool {
    registry::is_in_tag(
        ContentKind::Block,
        "hello-mod:greeter_lamp",
        "minecraft:mineable/pickaxe",
    )
}

fn lamp_state() -> Option<u16> {
    Block::from_name("hello-mod:greeter_lamp").map(|block| block.default_state.id.as_u16())
}
```

Lookups by tag work for generated tags (vanilla, and the `c:` and `neoforge:` tags that Pumpkin generates) and for tags of mods. A tag of a mod is not visible to clients and data packs.

### RegisterEvent

A mod that registers its content from a `RegisterEvent` listener keeps that shape. The handler gets `RegisterEvent` once per registry, and the methods of `content::RegisterEventExt` register only on the fire of their own registry, as `RegisterEvent#register(ResourceKey, ..)` does. Register the handler in `init`.

```rust
use std::sync::Arc;

use ironpumpkin_mods::{
    ModInit,
    content::{BlockBuilder, ItemBuilder, RegisterEventExt},
    event::{BoxFuture, EventHandler, EventPriority, registries::RegisterEvent},
    pumpkin_data::{Block, item::Item},
    server::Server,
};

const LAMP: &str = "hello-mod:greeter_lamp";

struct RegisterContent;

impl EventHandler<RegisterEvent> for RegisterContent {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut RegisterEvent,
    ) -> BoxFuture<'a, ()> {
        // The event keeps the first error. The server logs it and stops after the last fire.
        let _ = event.register_block(BlockBuilder::new(LAMP, Block::REDSTONE_LAMP.default_state.id));
        let _ = event.register_item(ItemBuilder::new(LAMP, &Item::REDSTONE_LAMP).places(LAMP));
        Box::pin(async {})
    }
}

fn init(cx: &mut ModInit) {
    cx.register_event::<RegisterEvent, _>(Arc::new(RegisterContent), EventPriority::Normal, true, false);
}
```

`RegisterEvent` fires for `minecraft:attribute`, `minecraft:data_component_type`, `minecraft:sound_event`, `minecraft:mob_effect`, `minecraft:block`, `minecraft:entity_type` and `minecraft:item`, in that order, as `GameData.getRegistrationOrder` orders them. NeoForge fires it for every registry, and IronPumpkin backs these seven. The traits `RegisterSoundEvent`, `RegisterMobEffect`, `RegisterAttribute` and `RegisterDataComponentType` register the four newer ones from a handler.

### Registration events

The other registration events of NeoForge are methods of `NativeMod` with a default that does nothing. Each method gets a handle from `ironpumpkin_mods::registration`. The server calls the method of every mod in load order, at the NeoForge position:

| NeoForge event | `NativeMod` method | When | What the handle does |
| :--- | :--- | :--- | :--- |
| `EntityAttributeCreationEvent` | `entity_attribute_creation` | After the last `RegisterEvent` | `put(entity_type, AttributeSupplier)`: the default attributes of a mod entity type |
| `EntityAttributeModificationEvent` | `entity_attribute_modification` | After the creation event | `add`, `add_default`, `has`, `types`: attributes of any entity type, vanilla ones included |
| `RegisterSpawnPlacementsEvent` | `register_spawn_placements` | After the modification event | `register`, `register_placement`: placement type, heightmap and spawn predicate, with the `And`, `Or` and `Replace` operations |
| `BlockEntityTypeAddBlocksEvent` | `block_entity_type_add_blocks` | After the spawn placements, before the local configs load | `modify(block_entity_type, blocks)`: mod blocks join a vanilla block entity type, and placing them creates the block entity |
| `ExtendPoiTypesEvent` | `extend_poi_types` | After `dedicated_server_setup`, before `inter_mod_enqueue` | `add_block_to_poi(poi_type, block)`: a block joins a job site POI type, and villagers take it as a job site |

The events fire before the freeze, so a mod entry has no id yet. The handles take names, and the registrations take effect from the freeze on. A call that NeoForge rejects with an exception, or a call that names an unknown entry, returns a `RegistrationError`. The handle keeps the first error. The server logs it with the mod id and stops after the step.

```rust
use ironpumpkin_mods::{
    ModInit, NativeMod,
    pumpkin_data::{attributes::Attributes, entity::EntityType},
    registration::{
        AttributeSupplier, EntityAttributeCreationEvent, Operation, RegisterSpawnPlacementsEvent,
        SpawnLocation, spawn_predicate,
    },
    pumpkin_util::HeightMap,
};

const GREETER: &str = "hello-mod:greeter";

struct HelloMod;

impl NativeMod for HelloMod {
    fn id(&self) -> &'static str {
        "hello_mod"
    }

    fn display_name(&self) -> &'static str {
        "Hello Mod"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn init(&self, _cx: &mut ModInit) {}

    // `@SubscribeEvent static void attributes(EntityAttributeCreationEvent event)`
    fn entity_attribute_creation(&self, event: &mut EntityAttributeCreationEvent) {
        // `Pig.createAttributes().add(Attributes.MAX_HEALTH, 20)`
        let _ = event.put(
            GREETER,
            AttributeSupplier::of(&EntityType::PIG).add(Attributes::MAX_HEALTH, 20.0),
        );
    }

    fn register_spawn_placements(&self, event: &mut RegisterSpawnPlacementsEvent) {
        // `event.register(GREETER, SpawnPlacementTypes.ON_GROUND,
        //     Heightmap.Types.MOTION_BLOCKING_NO_LEAVES, Animal::checkAnimalSpawnRules,
        //     Operation.REPLACE)`, with a predicate of the mod.
        let _ = event.register_placement(
            GREETER,
            Some(SpawnLocation::OnGround),
            Some(HeightMap::MotionBlockingNoLeaves),
            spawn_predicate(|_, _, pos| pos.0.y > 60),
            Operation::Replace,
        );
    }
}
```

Gaps:

- `ExtendPoiTypesEvent#addStatesToPoi` with a part of the states has no counterpart: a mod block has no state ids before the freeze, and the server tracks job sites by block. `add_block_to_poi` adds every state.
- The server tracks only the job site POI types as block POIs. A block added to `minecraft:home`, `minecraft:meeting` or another POI type without a profession is accepted with a warning and changes nothing.
- `BlockEntityTypeAddBlocksEvent` gives a block state one block entity type. A vanilla block keeps its own, and a mod block keeps the first one, with a warning. A block entity type that is not registered changes nothing, with a warning, as NeoForge skips it.
- `EntityAttributeCreationEvent#put` and `EntityAttributeModificationEvent#add` take the vanilla and NeoForge `Attributes`. A mod attribute binds at the freeze, after these events, so they cannot name it. Set its base value on the entity with `entity_attributes::set_base_value`.
- A spawn predicate gets the entity type, the world and the position. The spawn reason and the random source of the NeoForge predicate have no Pumpkin counterpart.

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
| `InvalidBlockType` | `.block_type(..)` names no vanilla block, or the block lists other properties than its type. | At the call |
| `UnknownItemType` | `.item_type(..)` names no vanilla item. | At the call |
| `RangeExceeded` | The custom entries of a registry do not fit in the free id range. | At the freeze |

---

## Do and don't

**Do**

- Register all content from `NativeMod::init` or from a `RegisterEvent` handler. The registry accepts content only until the freeze.
- Return the `RegistryError` with `?` and stop at the first error.
- Use the namespace and the names of the Java mod. A NeoForge client matches the registry by name.
- Choose a `display` entry that looks like the content. A vanilla client shows it.
- Give a block that needs a pickaxe `.tag("minecraft:mineable/pickaxe")`.
- Look up content by name after the freeze.
- Put the content of the mod in a function that returns `Result<(), RegistryError>`, as `register_content` does above.

**Don't**

- Do not call `unwrap` or `expect` on a registration `Result`, and do not panic. The panic hook of the server writes a crash report and exits at once.
- Do not register content from another event handler or from a command. The registry is frozen by then, and the call fails with `RegistryFrozen`.
- Do not store numeric ids. The server allocates them at the freeze.
- Do not use the `minecraft` namespace.
- Do not expect a handle like `DeferredHolder`. Keep the name and use `ironpumpkin_mods::registry`.
- Do not expect a creative tab on the server. `register_creative_mode_tab` accepts it and ignores it.
- Do not expect a custom block to run the behaviour of its `display` block. Name a block type with `.block_type(..)` for that.
