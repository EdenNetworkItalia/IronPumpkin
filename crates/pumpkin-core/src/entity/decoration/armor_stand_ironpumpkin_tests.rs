use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::vector3::Vector3;

use crate::entity::r#type::load_entity_from_nbt;

#[tokio::test]
async fn base_plate_follows_the_vanilla_no_base_plate_flag() {
    // `ArmorStand.readAdditionalSaveData` reads `NoBasePlate` with a default of false.
    let folder = tempfile::tempdir().unwrap();
    let world = crate::entity::ai::test_support::world(&folder);
    for (no_base_plate, expected) in [(None, false), (Some(false), false), (Some(true), true)] {
        let mut nbt = NbtCompound::new();
        nbt.put_string("id", "minecraft:armor_stand".to_string());
        if let Some(no_base_plate) = no_base_plate {
            nbt.put_bool("NoBasePlate", no_base_plate);
        }
        let stand = load_entity_from_nbt(&nbt, Vector3::new(0.5, 64.0, 0.5), &world).unwrap();
        let mut written = NbtCompound::new();
        stand.write_nbt(&mut written);
        assert_eq!(
            written.get_bool("NoBasePlate"),
            Some(expected),
            "{no_base_plate:?}"
        );
    }
}
