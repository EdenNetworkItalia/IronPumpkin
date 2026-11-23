# Porting global loot modifiers from NeoForge

In NeoForge, a global loot modifier changes the loot of every loot table roll. A mod registers a `MapCodec` in `NeoForgeRegistries.GLOBAL_LOOT_MODIFIER_SERIALIZERS`, usually for a subclass of `LootModifier`. A data pack has one file per modifier in `data/<namespace>/loot_modifiers/`, and `LootModifierManager` loads the files on startup and on `/reload`. After each roll, `CommonHooks.modifyLoot` runs the modifiers whose condition passes.

In IronPumpkin, the [`loot`](https://docs.rs/ironpumpkin-mods/latest/ironpumpkin_mods/loot/index.html) module of `ironpumpkin-mods` has the same files, fields, order and hook. A mod registers a modifier type with `ModInit::register_loot_modifier_type`: an id and a closure that decodes the JSON of a file into a `GlobalLootModifier`. The host reads `condition` and `priority`, as `LootModifier` does, and calls `apply` only when the condition passes.

---

## Key Differences

| Feature | NeoForge | IronPumpkin |
| :--- | :--- | :--- |
| **Modifier** | `IGlobalLootModifier`, or a subclass of `LootModifier` with `doApply` | `impl GlobalLootModifier` with `apply(&self, loot: Vec<ItemStack>, context: &mut LootContext) -> Vec<ItemStack>`. The host tests the condition first, as `LootModifier.apply` does. |
| **Type registration** | `DeferredRegister.create(NeoForgeRegistries.Keys.GLOBAL_LOOT_MODIFIER_SERIALIZERS, MODID)` and a `MapCodec` | `cx.register_loot_modifier_type("examplemod:remove_item", decode)` in `NativeMod::init`. `decode` gets the whole JSON object as a `serde_json::Value` and returns `Result<Box<dyn GlobalLootModifier>, String>`. |
| **Duplicate or late registration** | The registry throws | `Err(LootModifierError::Duplicate)`, and `Err(LootModifierError::RegistrationClosed)` after the last `RegisterEvent` |
| **Base fields** | `codecStart(instance)`: `condition` and `priority` | The host reads them. The decoder does not need to. |
| **Condition** | `LootItemCondition.CODEC`: an inline condition or a predicate id | The same, with the loot conditions of Pumpkin and `neoforge:loot_table_id`. An unknown condition type fails the file. |
| **Order** | Highest `priority` first, default 1000; equal priorities in an undefined order | Highest `priority` first, default `DEFAULT_PRIORITY`; equal priorities by modifier id |
| **A failed file** | Logged, skipped | Logged with the modifier id, skipped. The other files load. |
| **`neoforge:conditions`** | Skips the file when a condition fails | Ignored: the file loads. The data load conditions wait for M5. |
| **Context** | `LootContext`: `getRandom`, `getQueriedLootTableId`, `getParameter` | `LootContext`: `random()`, `queried_loot_table_id()`, `params()` |
| **Roll another table** | `table.getRandomItemsRaw(context, output)` | `context.roll_loot_table_raw("examplemod:extra")`. No modifier runs on that roll. |
| **Exceptions** | `modifyLoot` logs an exception and goes on | `apply` has no error path. Return the list unchanged when the modifier cannot act. |

---

## Code Comparison: a modifier that removes an item

The data file is the same for both, `data/examplemod/loot_modifiers/no_diamonds.json`:

```json
{
  "type": "examplemod:remove_item",
  "condition": {
    "type": "neoforge:loot_table_id",
    "loot_table_id": "minecraft:blocks/diamond_ore"
  },
  "priority": 1000,
  "item": "minecraft:diamond"
}
```

### 1. NeoForge implementation (Java)

```java
public class RemoveItemModifier extends LootModifier {
    public static final MapCodec<RemoveItemModifier> CODEC = RecordCodecBuilder.mapCodec(instance ->
            codecStart(instance)
                    .and(BuiltInRegistries.ITEM.byNameCodec().fieldOf("item").forGetter(m -> m.item))
                    .apply(instance, RemoveItemModifier::new));

    private final Item item;

    public RemoveItemModifier(Optional<Holder<LootItemCondition>> condition, int priority, Item item) {
        super(condition, priority);
        this.item = item;
    }

    @Override
    protected ObjectArrayList<ItemStack> doApply(ObjectArrayList<ItemStack> loot, LootContext context) {
        loot.removeIf(stack -> stack.is(item));
        return loot;
    }

    @Override
    public MapCodec<? extends IGlobalLootModifier> codec() {
        return CODEC;
    }
}

public static final DeferredRegister<MapCodec<? extends IGlobalLootModifier>> LOOT_MODIFIERS =
        DeferredRegister.create(NeoForgeRegistries.Keys.GLOBAL_LOOT_MODIFIER_SERIALIZERS, MODID);
public static final Supplier<MapCodec<RemoveItemModifier>> REMOVE_ITEM =
        LOOT_MODIFIERS.register("remove_item", () -> RemoveItemModifier.CODEC);
```

### 2. IronPumpkin implementation (Rust)

The snippet is a doc-test of `ironpumpkin-mods`: `cargo test --doc -p ironpumpkin-mods` compiles it.

```rust
use ironpumpkin_mods::{
    ModInit, NativeMod,
    loot::{GlobalLootModifier, ItemStack, LootContext, Value},
    pumpkin_data::item::Item,
    register_mod,
};

struct RemoveItem {
    item: &'static Item,
}

impl GlobalLootModifier for RemoveItem {
    fn apply(&self, mut loot: Vec<ItemStack>, _context: &mut LootContext<'_, '_>) -> Vec<ItemStack> {
        loot.retain(|stack| stack.item != self.item);
        loot
    }
}

fn decode_remove_item(json: &Value) -> Result<Box<dyn GlobalLootModifier>, String> {
    let item = json
        .get("item")
        .and_then(Value::as_str)
        .and_then(Item::from_registry_key)
        .ok_or("no item")?;
    Ok(Box::new(RemoveItem { item }))
}

struct ExampleMod;

impl NativeMod for ExampleMod {
#     fn id(&self) -> &'static str {
#         "examplemod"
#     }
#
#     fn display_name(&self) -> &'static str {
#         "Example Mod"
#     }
#
#     fn version(&self) -> &'static str {
#         "1.0.0"
#     }
#
    // id, display_name and version as usual.

    fn init(&self, cx: &mut ModInit) {
        if let Err(error) = cx.register_loot_modifier_type("examplemod:remove_item", decode_remove_item) {
            ironpumpkin_mods::tracing::error!("examplemod: {error}");
        }
    }
}

register_mod!(ExampleMod);
```

The test `crates/ironpumpkin-mods/tests/loot_modifiers.rs` registers this type, ships the files in a world data pack, and checks the block, chest and `/reload` paths.

---

## Files and order

A file `data/<namespace>/loot_modifiers/<path>.json` is the modifier `<namespace>:<path>`. A pack higher in the pack order replaces a file with the same id. There is no list file: the `global_loot_modifiers.json` of NeoForge 1.20 is gone.

The host runs the modifiers from the highest `priority` to the lowest. Inside one priority it runs them by modifier id, so the order is the same on each start. Each modifier gets the list that the one before returned.

## Built-in type and condition

`neoforge:add_table` adds the loot of another table, rolled with the same context:

```json
{
  "type": "neoforge:add_table",
  "condition": {
    "type": "neoforge:loot_table_id",
    "loot_table_id": "minecraft:chests/simple_dungeon"
  },
  "table": "examplemod:chests/dungeon_extra"
}
```

The added loot is rolled without modifiers. The modifiers after this one see it, so a modifier of the dungeon chest does not change it twice.

`neoforge:loot_table_id` passes when the rolled table has the id `loot_table_id`. A roll of a table that no id names has the id `neoforge:unknown_loot_table`. In IronPumpkin the condition works inside the `condition` of a modifier, also inside `all_of`, `any_of`, `inverted` and a predicate file that the condition names. It does not work inside a loot table.

## Which rolls run the modifiers

Every roll of a table by its id runs the modifiers: block drops, entity drops, chests and other containers, fishing, archaeology, bartering, gifts, vaults and trial spawners, and `/loot`. A table that another table nests runs no modifiers, as `getRandomItemsRaw` does in NeoForge. When no data pack has a modifier file, a roll does no extra work.

## Do and don't

**Do**

- Register every modifier type in `init`. The registration closes after the last `RegisterEvent`.
- Read your own fields in the decoder and return an `Err` with the reason when one is missing or wrong. The log names the file.
- Use `context.random()` for random choices, so the roll and the modifier share one random source.

**Don't**

- Do not read `condition` or `priority` in the decoder: the host does.
- Do not roll the target table again in a modifier with modifiers on: `roll_loot_table_raw` is the only roll a `LootContext` gives.
- Do not rely on `neoforge:conditions` to switch a modifier off yet.
