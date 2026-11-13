use super::*;
use crate::data_component::DataComponent;
use crate::data_component_impl::{
    CustomDataImpl, CustomNameImpl, DataComponentImpl, EnchantmentsImpl, ItemNameImpl, LoreImpl,
    MapDecorationsImpl, UnbreakableImpl,
};
use pumpkin_nbt::Nbt;
use pumpkin_nbt::deserializer::NbtReadHelperJava;
use std::io::Cursor;

#[test]
fn read_item_stack_applies_the_26_3_component_removals() {
    use pumpkin_nbt::compound::NbtCompound;

    let mut swing = NbtCompound::new();
    swing.put_string("type", "stab".to_string());
    swing.put_int("duration", 9);
    let mut components = NbtCompound::new();
    components.put_int("minecraft:map_color", 46_402);
    components.put_compound("minecraft:swing_animation", swing);
    components.put_int("minecraft:map_id", 3);
    let mut nbt = NbtCompound::new();
    nbt.put_string("id", "minecraft:filled_map".to_string());
    nbt.put_int("count", 1);
    nbt.put_compound("components", components);

    let stack = ItemStack::read_item_stack(&nbt).expect("the stack survives the old keys");
    let ids: Vec<DataComponent> = stack.patch.iter().map(|(id, _)| *id).collect();
    assert!(ids.contains(&DataComponent::MapId));
    assert!(ids.contains(&DataComponent::AttackAnimation));
    assert!(ids.contains(&DataComponent::InteractAnimation));

    let mut written = NbtCompound::new();
    stack.write_item_stack(&mut written);
    let written = written.get_compound("components").unwrap();
    assert!(written.get("minecraft:map_color").is_none());
    assert!(written.get("minecraft:swing_animation").is_none());
    for key in ["minecraft:attack_animation", "minecraft:interact_animation"] {
        let animation = written.get_compound(key).unwrap();
        assert_eq!(animation.get_string("type"), Some("stab"), "{key}");
        assert_eq!(animation.get_int("duration"), Some(9), "{key}");
    }

    // The removal keys: `!map_color` is dropped, `!swing_animation` removes both halves.
    let mut components = NbtCompound::new();
    components.put_compound("!minecraft:map_color", NbtCompound::new());
    components.put_compound("!minecraft:swing_animation", NbtCompound::new());
    let mut nbt = NbtCompound::new();
    nbt.put_string("id", "minecraft:filled_map".to_string());
    nbt.put_int("count", 1);
    nbt.put_compound("components", components);
    let stack = ItemStack::read_item_stack(&nbt).expect("the stack survives the old keys");
    assert_eq!(
        stack
            .patch
            .iter()
            .map(|(id, value)| (*id, value.is_none()))
            .collect::<Vec<_>>(),
        [
            (DataComponent::AttackAnimation, true),
            (DataComponent::InteractAnimation, true)
        ]
    );
}
