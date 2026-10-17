# Porting data components, sounds, mob effects and attributes from NeoForge

In NeoForge, a mod registers data component types, sound events, mob effects and attributes with a `DeferredRegister` of each registry. It reads and writes a component on an `ItemStack`, plays a sound with `Level#playSound`, applies a `MobEffectInstance` and changes an `AttributeInstance`.

In IronPumpkin, a mod registers them with `ModInit::register_data_component_type`, `register_sound_event`, `register_mob_effect` and `register_attribute`, or from a `RegisterEvent` handler, as blocks and items (see [Registration](./registration.md)). The modules `data_components`, `sound_events`, `mob_effects` and `entity_attributes` of `ironpumpkin-mods` use them. The entries get their ids at the freeze, after the vanilla ids. Saves name them, so the ids never reach a world file.

---

## Key Differences

| Feature | NeoForge | IronPumpkin |
| :--- | :--- | :--- |
| **Component type** | `DataComponentType<T>` with a `Codec<T>` and a `StreamCodec` | `DataComponentTypeBuilder::<T>::new(name)`. `T` is the codec: it implements `Encode` and `Decode` of `pumpkin_codecs`. |
| **Component value** | `stack.get(TYPE)`, `stack.set(TYPE, value)`, `stack.remove(TYPE)` | `TYPE.get(&stack)`, `TYPE.set(&mut stack, &value)`, `TYPE.remove(&mut stack)` on a `DataComponentType<T>` handle, and `data_components::get_json`, `set_json` and `remove` by name |
| **Component on the network** | The `StreamCodec` of the type | The NBT form of the codec, as `ByteBufCodecs.fromCodec` writes it, to a NeoForge client only |
| **Sound event** | `SoundEvent.createVariableRangeEvent`, `createFixedRangeEvent` | `SoundEventBuilder::new(name)`, `.fixed_range(range)` |
| **Play a sound** | `level.playSound(null, x, y, z, sound, source, volume, pitch)` | `sound_events::play_sound(&world, &SOUND, category, &position, volume, pitch)`. The server sends the sound by name, so no client needs a registry entry. |
| **Mob effect** | A subclass of `MobEffect` | `MobEffectBuilder::new(name, category, color)` and an implementation of the `MobEffect` trait, set with `.behaviour(..)` |
| **Effect tick** | `MobEffect#shouldApplyEffectTickThisTick`, `applyEffectTick` | `MobEffect::should_apply_effect_tick`, `apply_effect_tick` |
| **Effect on an entity** | `entity.addEffect(new MobEffectInstance(..))`, `hasEffect`, `getEffect`, `removeEffect` | `mob_effects::add_effect(entity, mob_effects::instance(..))`, `has_effect`, `get_effect`, `remove_effect` |
| **Attribute** | `RangedAttribute`, `PercentageAttribute`, `BooleanAttribute`, `setSyncable` | `AttributeBuilder::ranged`, `AttributeBuilder::boolean`, `AttributeBuilder::new`, `.syncable(..)` |
| **Attribute on an entity** | `entity.getAttribute(ATTR)` and its `AttributeInstance` | `entity_attributes::value`, `base_value`, `set_base_value`, `modifiers`, `add_modifier`, `remove_modifier` |
| **NeoForge attributes** | `NeoForgeMod.SWIM_SPEED`, `CREATIVE_FLIGHT`, `GLIDING_FLIGHT` | `entity_attributes::SWIM_SPEED`, `CREATIVE_FLIGHT`, `GLIDING_FLIGHT`. The server registers them with the ids of a NeoForge server. |
| **Vanilla client** | Not supported by NeoForge with these mods | Gets no mod component, mod effect or mod attribute. It plays a mod sound if its resource pack has it, and creative flight reaches it as the may-fly ability. |

---

## Code Comparison: a component, a sound, an effect and an attribute

### 1. NeoForge implementation (Java)

```java
@Mod(ExampleMod.MOD_ID)
public final class ExampleMod {
    public static final String MOD_ID = "examplemod";

    private static final DeferredRegister.DataComponents COMPONENTS =
            DeferredRegister.createDataComponents(Registries.DATA_COMPONENT_TYPE, MOD_ID);
    private static final DeferredRegister<SoundEvent> SOUNDS = DeferredRegister.create(Registries.SOUND_EVENT, MOD_ID);
    private static final DeferredRegister<MobEffect> EFFECTS = DeferredRegister.create(Registries.MOB_EFFECT, MOD_ID);
    private static final DeferredRegister<Attribute> ATTRIBUTES = DeferredRegister.create(Registries.ATTRIBUTE, MOD_ID);

    public static final DeferredHolder<DataComponentType<?>, DataComponentType<Integer>> CHARGE =
            COMPONENTS.registerComponentType("charge", builder -> builder.persistent(Codec.INT));
    public static final DeferredHolder<SoundEvent, SoundEvent> ZAP =
            SOUNDS.register("zap", SoundEvent::createVariableRangeEvent);
    public static final DeferredHolder<Attribute, Attribute> MANA = ATTRIBUTES.register("mana",
            () -> new RangedAttribute("attribute.examplemod.mana", 20.0, 0.0, 1000.0).setSyncable(true));
    public static final DeferredHolder<MobEffect, MobEffect> SHOCK = EFFECTS.register("shock",
            () -> new ShockEffect().addAttributeModifier(MANA, Identifier.fromNamespaceAndPath(MOD_ID, "shock"),
                    -2.0, AttributeModifier.Operation.ADD_VALUE));

    static final class ShockEffect extends MobEffect {
        ShockEffect() {
            super(MobEffectCategory.HARMFUL, 0x00FFFF);
        }

        @Override
        public boolean shouldApplyEffectTickThisTick(int duration, int amplifier) {
            return duration % 20 == 0;
        }

        @Override
        public boolean applyEffectTick(ServerLevel level, LivingEntity entity, int amplifier) {
            level.playSound(null, entity.getX(), entity.getY(), entity.getZ(), ZAP.get(), SoundSource.HOSTILE, 1.0F, 1.0F);
            return true;
        }
    }

    public ExampleMod(IEventBus modBus) {
        COMPONENTS.register(modBus);
        SOUNDS.register(modBus);
        EFFECTS.register(modBus);
        ATTRIBUTES.register(modBus);
    }
}
```

### 2. IronPumpkin implementation (Rust)

The snippet is a doc-test of `ironpumpkin-mods`: `cargo test --doc -p ironpumpkin-mods` compiles it.

```rust
use std::sync::Arc;

use ironpumpkin_mods::{
    ModInit, NativeMod,
    content::RegistryError,
    data_components::{DataComponentType, DataComponentTypeBuilder},
    entity::living::LivingEntity,
    entity_attributes::AttributeBuilder,
    mob_effects::{MobEffect, MobEffectBuilder, MobEffectCategory, Operation},
    pumpkin_data::{item_stack::ItemStack, sound::SoundCategory},
    register_mod,
    sound_events::{self, SoundEvent, SoundEventBuilder},
};

static CHARGE: DataComponentType<i32> = DataComponentType::new("examplemod:charge");
static ZAP: SoundEvent = SoundEvent::new("examplemod:zap");
const SHOCK: &str = "examplemod:shock";
const MANA: &str = "examplemod:mana";

struct ShockEffect;

impl MobEffect for ShockEffect {
    fn should_apply_effect_tick(&self, duration: i32, _amplifier: u8) -> bool {
        duration % 20 == 0
    }

    fn apply_effect_tick(&self, living: &LivingEntity, _amplifier: u8) {
        let world = living.entity.world.load();
        let position = living.entity.pos.load();
        let _ = sound_events::play_sound(&world, &ZAP, SoundCategory::Hostile, &position, 1.0, 1.0);
    }
}

struct ExampleMod;

impl NativeMod for ExampleMod {
    fn id(&self) -> &'static str {
        "examplemod"
    }

    fn display_name(&self) -> &'static str {
        "Example Mod"
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
    cx.register_data_component_type(DataComponentTypeBuilder::<i32>::new(CHARGE.name()))?;
    cx.register_sound_event(SoundEventBuilder::new(ZAP.name()))?;
    cx.register_attribute(AttributeBuilder::ranged(MANA, 20.0, 0.0, 1000.0).syncable(true))?;
    cx.register_mob_effect(
        MobEffectBuilder::new(SHOCK, MobEffectCategory::Harmful, 0x00_FF_FF)
            .attribute_modifier(MANA, "examplemod:shock", -2.0, Operation::AddValue)
            .behaviour(Arc::new(ShockEffect)),
    )
}

/// After the freeze: adds one charge to a stack.
fn charge(stack: &mut ItemStack) {
    let charge = CHARGE.get(stack).ok().flatten().unwrap_or(0);
    let _ = CHARGE.set(stack, &(charge + 1));
}

register_mod!(ExampleMod);
```

The mapping from the Java code to the Rust code:

| Java | Rust |
| :--- | :--- |
| `COMPONENTS.registerComponentType("charge", builder -> builder.persistent(Codec.INT))` | `cx.register_data_component_type(DataComponentTypeBuilder::<i32>::new(..))`. A type is persistent unless `.persistent(false)`. |
| `CHARGE.get()` used with `stack.get(..)` | `CHARGE.get(&stack)` on the `DataComponentType<i32>` handle |
| `SoundEvent::createVariableRangeEvent` | `SoundEventBuilder::new(..)`. `.fixed_range(..)` is `createFixedRangeEvent`. |
| `level.playSound(null, x, y, z, ZAP.get(), ..)` | `sound_events::play_sound(&world, &ZAP, ..)` |
| `new RangedAttribute(.., 20.0, 0.0, 1000.0).setSyncable(true)` | `AttributeBuilder::ranged(MANA, 20.0, 0.0, 1000.0).syncable(true)` |
| `extends MobEffect`, `super(MobEffectCategory.HARMFUL, 0x00FFFF)` | `MobEffectBuilder::new(SHOCK, MobEffectCategory::Harmful, 0x00_FF_FF)` and `impl MobEffect` |
| `addAttributeModifier(MANA, .., -2.0, ADD_VALUE)` | `.attribute_modifier(MANA, "examplemod:shock", -2.0, Operation::AddValue)` |
| `applyEffectTick` returns `false` to remove the effect | `apply_effect_tick` returns nothing. Remove the effect with `mob_effects::remove_effect`. |

---

## The JSON form of a component

`data_components::get_json(&stack, name)` returns the value of a mod component type in the JSON form of its codec, and `set_json(&mut stack, name, value)` decodes JSON through the codec before it sets the value. A value that the codec rejects fails with `ComponentError::Codec`, and a name that no mod registers with `ComponentError::UnknownType`. The functions answer for mod component types. The vanilla components of a stack keep their typed Pumpkin API, `ItemStack::get_data_component`.

## What a client gets

A NeoForge client that synced the registries gets the ids of the mod effects, attributes and component types in the `neoforge:frozen_registry` sync, and then the effect packets, the syncable attributes and the components with those ids. The Java half of the mod must register the same names. It must keep the default stream codec of a component type, because IronPumpkin writes the NBT form of the codec: a component type with its own `networkSynchronized` stream codec is not supported.

A vanilla client gets none of them. It still sees the particles of a mod effect, and a mod sound plays if its resource pack has the sound. When a vanilla client in creative mode sends a stack back unchanged, the server keeps the mod components of its own stack.

## The NeoForge attributes

| Attribute | Default | What reads it |
| :--- | :--- | :--- |
| `neoforge:swim_speed` | 1, from 0 to 1024 | The swim speed of a mob in water and its upward push when it jumps in water. The player moves on its client: a NeoForge client applies it, a vanilla client does not. |
| `neoforge:creative_flight` | 0 (off) | `Player::may_fly`. A survival player with the attribute may fly, also on a vanilla client: the server sends the may-fly ability. A player who loses it stops flying. |
| `neoforge:gliding_flight` | 0 (off) | A NeoForge client, before it lets the player glide. An equipped item with the `glider` component that the next damage does not break adds the modifier `neoforge:glider_component_flight` (+1), as on a NeoForge server. A vanilla client reads the `glider` component. |

---

## Do and don't

**Do**

- Register the types, sounds, effects and attributes from `NativeMod::init` or from a `RegisterEvent` handler, with the names of the Java half.
- Keep a `DataComponentType<T>` or a `SoundEvent` handle in a `static` and use it after the freeze.
- Use a value type whose `Encode` and `Decode` give the same form as the `Codec` of the Java half.
- Read the `NeoForge` attributes through `entity_attributes::SWIM_SPEED` and the other constants.

**Don't**

- Do not store the ids. The server allocates them at the freeze, after the vanilla ids.
- Do not give a component type a custom binary stream codec on the Java side.
- Do not expect a vanilla client to show a mod component, a mod effect or a mod attribute.
- Do not expect attributes in a save. Pumpkin does not save the attributes of an entity yet, so a base value or a modifier lasts until the entity unloads.
