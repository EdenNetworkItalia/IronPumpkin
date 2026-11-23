# Porting data maps from NeoForge

In NeoForge, a data map attaches a value to the entries of one registry. A mod creates a `DataMapType` with a codec, registers it in `RegisterDataMapTypesEvent`, and data packs give the values in `data/<namespace>/data_maps/<registry>/<path>.json`. Code reads a value with `holder.getData(type)` or `registry.getData(type, key)`.

In IronPumpkin, the `data_maps` module of `ironpumpkin-mods` has the same types, builders, mergers, removers and file format. The value type is the codec: it implements `Encode` and `Decode` of `pumpkin_codecs`, which `ironpumpkin_mods::pumpkin_codecs` re-exports. The registration event is the `NativeMod::register_data_map_types` method. The rustdoc of `ironpumpkin_mods::data_maps` (`cargo doc -p ironpumpkin-mods --open`) lists every type and method.

---

## Key Differences

| Feature | NeoForge | IronPumpkin |
| :--- | :--- | :--- |
| **Type** | `DataMapType.builder(id, registry, codec).build()` | `DataMapType::<T>::builder(id, registry).build()`. The codec is `T`. |
| **Sync** | `.synced(networkCodec, mandatory)` | `.synced(mandatory)`. `Encode` of `T` writes the form that the client reads, so it is the network codec. |
| **Advanced type** | `AdvancedDataMapType.builder(..).merger(..).remover(codec).disableTagSupport(reason)` | `AdvancedDataMapType::builder(..).merger(..).remover::<R>().disable_tag_support(reason)`. `R` is the remover type and its codec. |
| **Registration** | `RegisterDataMapTypesEvent.register(type)` on the mod bus | `NativeMod::register_data_map_types(&self, event)` and `event.register(&TYPE)`. It returns `Err(DataMapError)` and the server does not start. |
| **Registries** | Any registry | A registry whose entries the server knows: blocks, items, entity types, sounds, mob effects, attributes, data component types, fluids, game events, villager professions and types, block transformers and the synced data pack registries. Other registries fail with `DataMapError::UnsupportedRegistry`. |
| **Read a value** | `holder.getData(TYPE)`, `registry.getData(TYPE, key)` | `TYPE.get_data(&id)` returns `Option<Arc<T>>`. `TYPE.with_data(&id, \|value\| ..)` reads without a reference count. |
| **Whole map** | `registry.getDataMap(TYPE)` | `TYPE.data_map()`: a view with `get`, `iter` and `len` over the values of one load. |
| **Mergers** | `DataMapValueMerger.listMerger()` and the others | `ListMerger`, `SetMerger`, `MapMerger`, `DefaultMerger`, and `WrappedMerger::new(unwrap, wrap, merger)` for the wrapped mergers. |
| **Removers** | `DataMapValueRemover<R, T>` | `DataMapValueRemover<T>`: `remove` returns `None` to drop the value. |
| **Updated event** | `DataMapsUpdatedEvent` on the game bus | `pumpkin_core::plugin::data_maps::DataMapsUpdatedEvent`, re-exported by `data_maps`, registered with `ModInit::register_event`. |
| **Built-in types** | `NeoForgeDataMaps.OXIDIZABLES` and the others | `data_maps::builtin::OXIDIZABLES` and the others. |

---

## Code Comparison: a fuel value per item

### 1. NeoForge implementation (Java)

```java
public static final DataMapType<Item, Integer> BURN_TIME = DataMapType.builder(
        Identifier.fromNamespaceAndPath("examplemod", "burn_time"), Registries.ITEM, Codec.INT)
        .synced(Codec.INT, false)
        .build();

@SubscribeEvent
public static void registerDataMaps(RegisterDataMapTypesEvent event) {
    event.register(BURN_TIME);
}

public static int burnTime(ItemStack stack) {
    Integer time = stack.getItemHolder().getData(BURN_TIME);
    return time == null ? 0 : time;
}
```

### 2. IronPumpkin implementation (Rust)

```rust
use std::sync::LazyLock;

use ironpumpkin_mods::{
    NativeMod,
    data_maps::{DataMapType, RegisterDataMapTypesEvent},
    identifier::Identifier,
};

static BURN_TIME: LazyLock<DataMapType<i32>> = LazyLock::new(|| {
    DataMapType::builder(
        Identifier::parse_static("examplemod:burn_time"),
        Identifier::vanilla_static("item"),
    )
    .synced(false)
    .build()
});

# struct ExampleMod;
#
impl NativeMod for ExampleMod {
    // id, display_name, version and init as usual.
#     fn id(&self) -> &'static str {
#         "examplemod"
#     }
#     fn display_name(&self) -> &'static str {
#         "Example Mod"
#     }
#     fn version(&self) -> &'static str {
#         "1.0.0"
#     }
#     fn init(&self, _cx: &mut ironpumpkin_mods::ModInit) {}

    fn register_data_map_types(&self, event: &mut RegisterDataMapTypesEvent) {
        // The event keeps a failed call, and the server does not start.
        let _ = event.register(&*BURN_TIME);
    }
}

fn burn_time(item: &Identifier) -> i32 {
    BURN_TIME.with_data(item, |time| *time).unwrap_or(0)
}
# let _ = burn_time;
```

A data pack gives the values in `data/examplemod/data_maps/item/burn_time.json`:

```json
{
  "values": {
    "minecraft:stick": 100,
    "#minecraft:planks": { "value": 300, "replace": true }
  },
  "remove": ["minecraft:bamboo"]
}
```

---

## A value type with a merger and a remover

A value type implements `Encode`, `Decode` and `Clone`. `Decode` reads the value of a file. A remover type implements `Decode` and `DataMapValueRemover<T>`.

```rust
use std::sync::LazyLock;

use ironpumpkin_mods::{
    data_maps::{AdvancedDataMapType, DataMapSource, DataMapValueRemover, ListMerger},
    identifier::Identifier,
    pumpkin_codecs::{DataResult, Decode, DynamicOps, Encode},
};

/// Removes one number from the list of an entry.
#[derive(Clone)]
struct Without(i32);

impl Encode for Without {
    fn encode<O: DynamicOps>(&self, ops: &'static O, prefix: O::Value) -> DataResult<O::Value> {
        self.0.encode(ops, prefix)
    }
}

impl Decode for Without {
    fn decode<O: DynamicOps>(input: O::Value, ops: &'static O) -> DataResult<(Self, O::Value)> {
        i32::decode(input, ops).map(|(value, rest)| (Self(value), rest))
    }
}

impl DataMapValueRemover<Vec<i32>> for Without {
    fn remove(
        &self,
        value: &Vec<i32>,
        _registry: &Identifier,
        _source: &DataMapSource,
        _entry: &Identifier,
    ) -> Option<Vec<i32>> {
        let kept: Vec<i32> = value.iter().copied().filter(|n| *n != self.0).collect();
        (!kept.is_empty()).then_some(kept)
    }
}

static LEVELS: LazyLock<AdvancedDataMapType<Vec<i32>, Without>> = LazyLock::new(|| {
    AdvancedDataMapType::builder(
        Identifier::parse_static("examplemod:levels"),
        Identifier::vanilla_static("block"),
    )
    .merger(ListMerger)
    .remover::<Without>()
    .build()
});
# let _ = &*LEVELS;
```

A file then removes one number with `"remove": { "minecraft:stone": 2 }` or `"remove": [{ "key": "minecraft:stone", "remover": 2 }]`.

---

## Loading and sync

The server reads the data map files of every enabled data pack at startup and on `/reload`: the mod data packs, the vanilla data and the world data packs, in the order of [Data packs](./data-packs.md). A mod ships its own values in its mod data pack. After each load, `DataMapsUpdatedEvent` fires for every registry that had a data map file, with cause `UpdateCause::ServerReload`. A registry with no file keeps its values, as on NeoForge.

A synced map goes to NeoForge clients that know it, on join and to every player after `/reload`. A vanilla client gets nothing. When a map is mandatory, the server refuses a vanilla client, and a NeoForge client that lacks the map refuses itself, as on NeoForge. A mandatory map turns on the NeoForge client detection even when `detect_neoforge_clients` is `false`, with a warning at startup, as a client-required mod does.

The loader drops a value of a synced map whose JSON is longer than 32767 characters, with an error that names the map and the entry: the sync payload cannot carry it.

---

## Built-in data maps

The host registers the eleven types of `NeoForgeDataMaps` in `data_maps::builtin`, so data packs and mods can give their values: `ACCEPTABLE_VILLAGER_DISTANCES`, `BLOCK_TRANSFORM_APPENDERS`, `MONSTER_ROOM_MOBS`, `OXIDIZABLES`, `PARROT_IMITATIONS`, `RAID_HERO_GIFTS`, `TRANSFORMABLES`, `VIBRATION_FREQUENCIES`, `VILLAGER_COMPOSTABLES`, `VILLAGER_TYPES` and `WAXABLES`. The server does not ship the default values of NeoForge, and no vanilla behaviour reads these maps yet: a value that a pack gives is synced and readable, but copper still oxidizes and a parrot still imitates with the built-in vanilla data.

---

## Differences from NeoForge

- A value with `neoforge:conditions` does not load, with a warning: IronPumpkin evaluates no load conditions yet.
- A block transform (`BlockTransformData`) is kept as its JSON object, after the check of every field that `BlockTransformData.CODEC` reads. Inside a block state provider, a block predicate and a typed int provider get only a check of their `type`.
- A registry whose entries the server does not know cannot carry a data map.

## Do and don't

**Do**

- Create each data map type once, in a `LazyLock`, and register that handle.
- Use `with_data` on a hot path, such as a tick or a block update.
- Listen to `DataMapsUpdatedEvent` to rebuild a cache that depends on the values.

**Don't**

- Do not keep the `Arc` of `get_data` or a `data_map` view across a `/reload`: it holds the old values.
- Do not register a type after the event: the call fails with `DataMapError::Closed`.
- Do not expect the vanilla behaviour to read the built-in maps yet.
