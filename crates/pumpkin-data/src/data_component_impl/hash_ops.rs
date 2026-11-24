//! Vanilla `HashOps.CRC32C_INSTANCE`: the hash that a client sends for each component of a
//! clicked stack. Each function returns the 4-byte CRC-32C of one encoded value. A compound
//! value feeds the hashes of its parts into a new digest, as a Guava `HashCode` does.

use crate::data_component_impl::basic::SoundEvent;
use crate::data_component_impl::{IDSet, IDSetContent, IdOr};
use crate::dynamic::namespaced_name;
use crate::item_stack::ItemStack;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::text::TextComponent;
use pumpkin_util::version::JavaMinecraftVersion;

const TAG_EMPTY: u8 = 1;
const TAG_MAP_START: u8 = 2;
const TAG_MAP_END: u8 = 3;
const TAG_LIST_START: u8 = 4;
const TAG_LIST_END: u8 = 5;
const TAG_BYTE: u8 = 6;
const TAG_SHORT: u8 = 7;
const TAG_INT: u8 = 8;
const TAG_LONG: u8 = 9;
const TAG_FLOAT: u8 = 10;
const TAG_DOUBLE: u8 = 11;
const TAG_STRING: u8 = 12;
const TAG_BOOLEAN: u8 = 13;
const TAG_BYTE_ARRAY_START: u8 = 14;
const TAG_BYTE_ARRAY_END: u8 = 15;
const TAG_INT_ARRAY_START: u8 = 16;
const TAG_INT_ARRAY_END: u8 = 17;
const TAG_LONG_ARRAY_START: u8 = 18;
const TAG_LONG_ARRAY_END: u8 = 19;

fn crc(bytes: &[u8]) -> u32 {
    crc32c::crc32c(bytes)
}

#[must_use]
pub fn empty() -> u32 {
    crc(&[TAG_EMPTY])
}

#[must_use]
pub fn byte(value: i8) -> u32 {
    crc(&[TAG_BYTE, value as u8])
}

#[must_use]
pub fn short(value: i16) -> u32 {
    let [a, b] = value.to_le_bytes();
    crc(&[TAG_SHORT, a, b])
}

#[must_use]
pub fn int(value: i32) -> u32 {
    let [a, b, c, d] = value.to_le_bytes();
    crc(&[TAG_INT, a, b, c, d])
}

#[must_use]
pub fn long(value: i64) -> u32 {
    let mut bytes = [TAG_LONG; 9];
    bytes[1..].copy_from_slice(&value.to_le_bytes());
    crc(&bytes)
}

#[must_use]
pub fn float(value: f32) -> u32 {
    let [a, b, c, d] = value.to_bits().to_le_bytes();
    crc(&[TAG_FLOAT, a, b, c, d])
}

#[must_use]
pub fn double(value: f64) -> u32 {
    let mut bytes = [TAG_DOUBLE; 9];
    bytes[1..].copy_from_slice(&value.to_bits().to_le_bytes());
    crc(&bytes)
}

/// Java hashes the UTF-16 length and code units of the string.
#[must_use]
pub fn string(value: &str) -> u32 {
    let units: Vec<u16> = value.encode_utf16().collect();
    let mut bytes = Vec::with_capacity(5 + units.len() * 2);
    bytes.push(TAG_STRING);
    bytes.extend_from_slice(&(units.len() as i32).to_le_bytes());
    for unit in units {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    crc(&bytes)
}

#[must_use]
pub fn boolean(value: bool) -> u32 {
    crc(&[TAG_BOOLEAN, u8::from(value)])
}

#[must_use]
pub fn list(values: impl IntoIterator<Item = u32>) -> u32 {
    let mut bytes = vec![TAG_LIST_START];
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.push(TAG_LIST_END);
    crc(&bytes)
}

#[must_use]
pub fn byte_array(values: &[i8]) -> u32 {
    let mut bytes = Vec::with_capacity(values.len() + 2);
    bytes.push(TAG_BYTE_ARRAY_START);
    bytes.extend(values.iter().map(|&value| value as u8));
    bytes.push(TAG_BYTE_ARRAY_END);
    crc(&bytes)
}

#[must_use]
pub fn int_array(values: &[i32]) -> u32 {
    let mut bytes = Vec::with_capacity(values.len() * 4 + 2);
    bytes.push(TAG_INT_ARRAY_START);
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.push(TAG_INT_ARRAY_END);
    crc(&bytes)
}

#[must_use]
pub fn long_array(values: &[i64]) -> u32 {
    let mut bytes = Vec::with_capacity(values.len() * 8 + 2);
    bytes.push(TAG_LONG_ARRAY_START);
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.push(TAG_LONG_ARRAY_END);
    crc(&bytes)
}

/// Vanilla sorts the entries by the unsigned key hash, then by the unsigned value hash.
#[must_use]
pub fn map(mut entries: Vec<(u32, u32)>) -> u32 {
    entries.sort_unstable();
    let mut bytes = Vec::with_capacity(entries.len() * 8 + 2);
    bytes.push(TAG_MAP_START);
    for (key, value) in entries {
        bytes.extend_from_slice(&key.to_le_bytes());
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.push(TAG_MAP_END);
    crc(&bytes)
}

/// The fields of a record codec, hashed as a map with string keys.
#[derive(Default)]
pub struct MapHasher(Vec<(u32, u32)>);

impl MapHasher {
    #[must_use]
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    pub fn put(&mut self, key: &str, value: u32) -> &mut Self {
        self.0.push((string(key), value));
        self
    }

    #[must_use]
    pub fn finish(&mut self) -> u32 {
        map(std::mem::take(&mut self.0))
    }
}

/// How a byte tag maps to a `HashOps` value.
#[derive(Clone, Copy)]
enum Bytes {
    /// `NbtOps.convertTo`: a byte tag stays a byte.
    Nbt,
    /// A codec value saved through `NbtOps`, which writes `Codec.BOOL` as a byte tag and a list
    /// of booleans as a byte array.
    Booleans,
    /// A byte 0 or 1 is a boolean, any other byte stays a byte.
    ZeroOneBooleans,
}

fn tag(value: &NbtTag, bytes: Bytes) -> u32 {
    match value {
        NbtTag::End => empty(),
        NbtTag::Byte(value) => match bytes {
            Bytes::Nbt => byte(*value),
            Bytes::Booleans => boolean(*value != 0),
            Bytes::ZeroOneBooleans if matches!(value, 0 | 1) => boolean(*value != 0),
            Bytes::ZeroOneBooleans => byte(*value),
        },
        NbtTag::Short(value) => short(*value),
        NbtTag::Int(value) => int(*value),
        NbtTag::Long(value) => long(*value),
        NbtTag::Float(value) => float(*value),
        NbtTag::Double(value) => double(*value),
        NbtTag::ByteArray(values) => match bytes {
            Bytes::Nbt | Bytes::ZeroOneBooleans => byte_array(values),
            Bytes::Booleans => list(values.iter().map(|value| boolean(*value != 0))),
        },
        NbtTag::String(value) => string(value),
        NbtTag::List(values) => list(values.iter().map(|value| tag(value, bytes))),
        NbtTag::Compound(compound) => compound_tag(compound, bytes),
        NbtTag::IntArray(values) => int_array(values),
        NbtTag::LongArray(values) => long_array(values),
    }
}

fn compound_tag(compound: &NbtCompound, bytes: Bytes) -> u32 {
    map(compound
        .child_tags
        .iter()
        .map(|(key, value)| (string(key), tag(value, bytes)))
        .collect())
}

/// The hash of a `CompoundTag.CODEC` value, such as `CustomData`: vanilla converts the tag with
/// `NbtOps.convertTo`.
#[must_use]
pub fn nbt(value: &NbtTag) -> u32 {
    tag(value, Bytes::Nbt)
}

/// [`nbt`] of a compound.
#[must_use]
pub fn nbt_compound(compound: &NbtCompound) -> u32 {
    compound_tag(compound, Bytes::Nbt)
}

/// The hash of a codec value in its saved NBT form, where each byte tag is a `Codec.BOOL`.
///
/// The saved form loses the difference between a list of numbers and a number array, so a
/// value that has a list of ints or longs gets a different hash than on the client. The client
/// then resends the slot.
#[must_use]
pub fn codec_nbt(value: &NbtTag) -> u32 {
    tag(value, Bytes::Booleans)
}

/// The hash of a `ComponentSerialization.CODEC` value, from the NBT that the client gets.
#[must_use]
pub fn text(component: &TextComponent) -> u32 {
    codec_nbt(&component.to_nbt_tag_for_version(&TEXT_VERSION))
}

/// The text encoding that the component codecs send and hash. A component codec does not know
/// the version of the client, so both use the current protocol.
pub const TEXT_VERSION: JavaMinecraftVersion = JavaMinecraftVersion::V_26_3;

/// The hash of `ItemPredicate.CODEC` or `BlockPredicate.CODEC` in its saved form. The expected
/// components hash with the codec of each component. The other fields hash as saved codec
/// values: they hold ids, numbers, strings and the boolean flags of partial predicates.
#[must_use]
pub fn predicate(compound: &NbtCompound) -> u32 {
    map(compound
        .child_tags
        .iter()
        .map(|(key, value)| {
            let value = match (&**key, value) {
                ("components", NbtTag::Compound(exact)) => exact_components(exact),
                (_, value) => codec_nbt(value),
            };
            (string(key), value)
        })
        .collect())
}

/// The hash of `AdventureModePredicate.CODEC`: one block predicate, or a list of them.
#[must_use]
pub fn adventure_predicate(tag: &NbtTag) -> u32 {
    let entry = |tag: &NbtTag| match tag {
        NbtTag::Compound(compound) => predicate(compound),
        tag => codec_nbt(tag),
    };
    match tag {
        NbtTag::List(predicates) => list(predicates.iter().map(entry)),
        tag => entry(tag),
    }
}

/// `DataComponentExactPredicate.CODEC`: a map from the component id to its value.
fn exact_components(exact: &NbtCompound) -> u32 {
    map(exact
        .child_tags
        .iter()
        .map(|(name, value)| {
            let hash = crate::data_component::DataComponent::try_from_name(name)
                .and_then(|id| crate::data_component_impl::read_data(id, value))
                .map_or_else(|| codec_nbt(value), |component| component.get_hash() as u32);
            (string(name), hash)
        })
        .collect())
}

/// The hash of a registry id string, such as a `Holder` reference or a `ResourceKey`.
#[must_use]
pub fn id(name: &str) -> u32 {
    string(&namespaced_name(name))
}

/// The hash of `RegistryCodecs.holderSet`: a tag as `#name`, one element as its id, and more
/// elements as a list of ids (`ExtraCodecs.compactListCodec`).
#[must_use]
pub fn holder_set<T: IDSetContent>(set: &IDSet<T>) -> u32 {
    match set {
        IDSet::Tag(tag) => string(&format!("#{}", namespaced_name(tag))),
        IDSet::IDs(ids) => match &**ids {
            [single] => id(&single.to_string()),
            ids => list(ids.iter().map(|entry| id(&entry.to_string()))),
        },
    }
}

/// Whether a float equals the default of an `optionalFieldOf` field. `Float.equals` compares
/// the bits.
#[must_use]
pub const fn same_float(value: f32, default: f32) -> bool {
    value.to_bits() == default.to_bits()
}

/// The hash of `SoundEvent.CODEC`: a registry sound as its id, an inline sound as
/// `SoundEvent.DIRECT_CODEC`.
#[must_use]
pub fn sound(sound: &IdOr<SoundEvent>) -> u32 {
    match sound {
        IdOr::Id(sound) => id(sound.to_name()),
        IdOr::Value(event) => {
            let mut fields = MapHasher::new();
            fields.put("sound_id", id(&event.sound_name));
            if let Some(range) = event.range {
                fields.put("range", float(range));
            }
            fields.finish()
        }
    }
}

/// Whether `sound` is the registry sound `name`, the default of an `optionalFieldOf` field.
#[must_use]
pub fn is_registry_sound(sound: &IdOr<SoundEvent>, name: &str) -> bool {
    matches!(sound, IdOr::Id(sound) if sound.to_name() == name)
}

/// The hash of `ItemStackTemplate.CODEC`, which encodes a template as its map form.
#[must_use]
pub fn item_stack_template(stack: &ItemStack) -> u32 {
    let mut fields = MapHasher::new();
    fields.put("id", string(&namespaced_name(stack.item.registry_key)));
    if stack.item_count != 1 {
        fields.put("count", int(i32::from(stack.item_count)));
    }
    let components = component_patch(stack);
    if !components.is_empty() {
        fields.put("components", map(components));
    }
    fields.finish()
}

/// The entries of `DataComponentPatch.CODEC`, which skips transient components.
fn component_patch(stack: &ItemStack) -> Vec<(u32, u32)> {
    let mut entries = Vec::with_capacity(stack.patch.len() + stack.custom_components.len());
    for (id, data) in &stack.patch {
        if !id.is_persistent() {
            continue;
        }
        entries.push(match data {
            Some(data) => (string(id.to_name()), data.get_hash() as u32),
            // `Removed.CODEC` is a unit codec.
            None => (string(&format!("!{}", id.to_name())), map(Vec::new())),
        });
    }
    for (id, value) in &stack.custom_components {
        if let Some(entry) = crate::dynamic_registries::data_component_type(*id)
            && entry.persistent
        {
            entries.push((string(entry.name), mod_component(value)));
        }
    }
    entries
}

/// The hash of a mod component value. Only its NBT form is known. A NeoForge client hashes the
/// value with the codec of the mod, where a `Codec.BOOL` flag is a boolean, but the save stores
/// it as a byte. Flags are more common than byte fields that hold only 0 or 1, so a byte 0 or 1
/// hashes as a boolean.
#[must_use]
pub fn mod_component(value: &NbtTag) -> u32 {
    tag(value, Bytes::ZeroOneBooleans)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Values built from the decompiled `HashOps` by hand.
    #[test]
    fn primitives_match_vanilla_byte_layout() {
        assert_eq!(empty(), crc32c::crc32c(&[1]));
        assert_eq!(map(Vec::new()), crc32c::crc32c(&[2, 3]));
        assert_eq!(list([]), crc32c::crc32c(&[4, 5]));
        assert_eq!(boolean(true), crc32c::crc32c(&[13, 1]));
        assert_eq!(boolean(false), crc32c::crc32c(&[13, 0]));
        assert_eq!(byte(-2), crc32c::crc32c(&[6, 0xFE]));
        assert_eq!(short(0x0102), crc32c::crc32c(&[7, 2, 1]));
        assert_eq!(int(0x0102_0304), crc32c::crc32c(&[8, 4, 3, 2, 1]));
        assert_eq!(
            long(0x0102_0304_0506_0708),
            crc32c::crc32c(&[9, 8, 7, 6, 5, 4, 3, 2, 1])
        );
        let one = 1.0f32.to_bits().to_le_bytes();
        assert_eq!(
            float(1.0),
            crc32c::crc32c(&[10, one[0], one[1], one[2], one[3]])
        );
        let mut double_bytes = vec![11];
        double_bytes.extend_from_slice(&0.5f64.to_bits().to_le_bytes());
        assert_eq!(double(0.5), crc32c::crc32c(&double_bytes));
        // `putInt(length)` then `putUnencodedChars`: UTF-16 code units, little endian.
        assert_eq!(
            string("a\u{e8}"),
            crc32c::crc32c(&[12, 2, 0, 0, 0, b'a', 0, 0xE8, 0])
        );
        assert_eq!(int_array(&[1]), crc32c::crc32c(&[16, 1, 0, 0, 0, 17]));
        assert_eq!(
            long_array(&[1]),
            crc32c::crc32c(&[18, 1, 0, 0, 0, 0, 0, 0, 0, 19])
        );
        assert_eq!(byte_array(&[1, 2]), crc32c::crc32c(&[14, 1, 2, 15]));
    }

    /// A NeoForge client hashes a `Codec.BOOL` flag of a mod component with the boolean tag.
    #[test]
    fn mod_component_bytes_zero_and_one_hash_as_booleans() {
        assert_eq!(mod_component(&NbtTag::Byte(1)), crc32c::crc32c(&[13, 1]));
        assert_eq!(mod_component(&NbtTag::Byte(0)), crc32c::crc32c(&[13, 0]));
        assert_eq!(mod_component(&NbtTag::Byte(2)), crc32c::crc32c(&[6, 2]));
        assert_eq!(
            mod_component(&NbtTag::List(vec![NbtTag::Byte(1)])),
            crc32c::crc32c(
                &[
                    [4].as_slice(),
                    &crc32c::crc32c(&[13, 1]).to_le_bytes(),
                    &[5]
                ]
                .concat()
            )
        );
    }

    #[test]
    fn map_entries_sort_by_unsigned_key_then_value() {
        let a = (0x8000_0000, 1);
        let b = (0x0000_0001, 2);
        let c = (0x0000_0001, 1);
        let mut expected = vec![2];
        for (key, value) in [c, b, a] {
            expected.extend_from_slice(&u32::to_le_bytes(key));
            expected.extend_from_slice(&u32::to_le_bytes(value));
        }
        expected.push(3);
        assert_eq!(map(vec![a, b, c]), crc32c::crc32c(&expected));
    }

    #[test]
    fn nbt_keeps_bytes_and_codec_nbt_reads_booleans() {
        let mut compound = NbtCompound::new();
        compound.put_byte("flag", 1);
        let key = string("flag");
        let as_byte = crc32c::crc32c(
            &[
                [2].as_slice(),
                &key.to_le_bytes(),
                &byte(1).to_le_bytes(),
                &[3],
            ]
            .concat(),
        );
        let as_bool = crc32c::crc32c(
            &[
                [2].as_slice(),
                &key.to_le_bytes(),
                &boolean(true).to_le_bytes(),
                &[3],
            ]
            .concat(),
        );
        let value = NbtTag::Compound(compound);
        assert_eq!(nbt(&value), as_byte);
        assert_eq!(codec_nbt(&value), as_bool);
    }

    /// `HashOps.CRC32C_INSTANCE` encoders written again from the decompiled class.
    mod vanilla {
        fn crc(parts: &[&[u8]]) -> u32 {
            crc32c::crc32c(&parts.concat())
        }

        pub fn int(value: i32) -> u32 {
            crc(&[&[8], &value.to_le_bytes()])
        }

        pub fn float(value: f32) -> u32 {
            crc(&[&[10], &value.to_bits().to_le_bytes()])
        }

        pub fn boolean(value: bool) -> u32 {
            crc(&[&[13, u8::from(value)]])
        }

        pub fn byte(value: i8) -> u32 {
            crc(&[&[6], &value.to_le_bytes()])
        }

        pub fn string(value: &str) -> u32 {
            let units: Vec<u8> = value.encode_utf16().flat_map(u16::to_le_bytes).collect();
            let length = (value.encode_utf16().count() as i32).to_le_bytes();
            crc(&[&[12], &length, &units])
        }

        pub fn list(values: &[u32]) -> u32 {
            let values: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
            crc(&[&[4], &values, &[5]])
        }

        pub fn int_array(values: &[i32]) -> u32 {
            let values: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
            crc(&[&[16], &values, &[17]])
        }

        /// `HASH_COMPARATOR` compares `padToLong`, the unsigned value of the hash.
        pub fn map(entries: &[(&str, u32)]) -> u32 {
            let mut entries: Vec<(u32, u32)> = entries
                .iter()
                .map(|(key, value)| (string(key), *value))
                .collect();
            entries.sort_unstable();
            let bytes: Vec<u8> = entries
                .iter()
                .flat_map(|(key, value)| [key.to_le_bytes(), value.to_le_bytes()])
                .flatten()
                .collect();
            crc(&[&[2], &bytes, &[3]])
        }
    }

    use crate::data_component::DataComponent;
    use crate::data_component_impl::{
        BundleContentsImpl, CustomDataImpl, DamageImpl, DataComponentImpl, IDSet,
        PotDecorationsImpl, ToolImpl, ToolRule, UnbreakableImpl,
    };
    use crate::item::Item;
    use std::borrow::Cow;

    /// `ItemStackTemplate.MAP_CODEC` of a count 1 template without components.
    fn template(id: &str) -> u32 {
        vanilla::map(&[("id", vanilla::string(id))])
    }

    #[test]
    fn pot_decorations_hash_has_the_present_sides_as_templates() {
        let pot = PotDecorationsImpl {
            back: Some(ItemStack::new(1, &Item::BRICK)),
            left: None,
            right: None,
            front: Some(ItemStack::new(1, &Item::ARCHER_POTTERY_SHERD)),
        };
        let expected = vanilla::map(&[
            ("back", template("minecraft:brick")),
            ("front", template("minecraft:archer_pottery_sherd")),
        ]);
        assert_eq!(pot.get_hash() as u32, expected);
        assert_ne!(pot.get_hash(), 0);
    }

    #[test]
    fn template_hash_has_count_and_patch_with_removed_keys() {
        let stack = ItemStack::new_with_component(
            2,
            &Item::IRON_SWORD,
            vec![
                (
                    DataComponent::Damage,
                    Some(DamageImpl { damage: 5 }.to_dyn()),
                ),
                (DataComponent::Unbreakable, None),
            ],
        );
        let bundle = BundleContentsImpl {
            items: vec![stack],
            selected_item: -1,
        };
        let expected = vanilla::list(&[vanilla::map(&[
            ("id", vanilla::string("minecraft:iron_sword")),
            ("count", vanilla::int(2)),
            (
                "components",
                vanilla::map(&[
                    ("minecraft:damage", vanilla::int(5)),
                    // `Removed.CODEC` is `MapCodec.unitCodec`: an empty map.
                    ("!minecraft:unbreakable", vanilla::map(&[])),
                ]),
            ),
        ])]);
        assert_eq!(bundle.get_hash() as u32, expected);
        assert_eq!(UnbreakableImpl.get_hash() as u32, vanilla::map(&[]));
    }

    #[test]
    fn custom_data_hash_converts_the_tag_with_nbt_ops() {
        let mut data = NbtCompound::new();
        data.put_byte("flag", 1);
        data.put_string("name", "x".to_owned());
        data.put("list", NbtTag::List(vec![NbtTag::Int(1), NbtTag::Int(2)]));
        let expected = vanilla::map(&[
            ("flag", vanilla::byte(1)),
            ("name", vanilla::string("x")),
            ("list", vanilla::list(&[vanilla::int(1), vanilla::int(2)])),
        ]);
        assert_eq!(CustomDataImpl::new(data).get_hash() as u32, expected);
    }

    #[test]
    fn tool_hash_omits_default_fields() {
        let tool = ToolImpl {
            rules: Cow::Owned(vec![ToolRule {
                blocks: IDSet::Tag(Cow::Borrowed("minecraft:mineable/pickaxe")),
                speed: Some(4.0),
                correct_for_drops: Some(true),
            }]),
            default_mining_speed: 1.0,
            damage_per_block: 1,
            can_destroy_blocks_in_creative: true,
        };
        let rule = vanilla::map(&[
            ("blocks", vanilla::string("#minecraft:mineable/pickaxe")),
            ("speed", vanilla::float(4.0)),
            ("correct_for_drops", vanilla::boolean(true)),
        ]);
        assert_eq!(
            tool.get_hash() as u32,
            vanilla::map(&[("rules", vanilla::list(&[rule]))])
        );
    }

    mod codec_defaults {
        use super::vanilla;
        use crate::Enchantment;
        use crate::data_component_impl::basic::{BreakSoundImpl, SoundEvent};
        use crate::data_component_impl::block_entity::BlockStateImpl;
        use crate::data_component_impl::combat::{
            AttackRangeImpl, BlocksAttacksImpl, DamageReduction, DamageResistantImpl,
            DeathProtectionImpl, EnchantmentsImpl, EquippableImpl, ItemDamageFunction,
            KineticConditionImpl, KineticWeaponImpl, MobVisibilityImpl, PiercingWeaponImpl,
            StoredEnchantmentsImpl, WeaponImpl,
        };
        use crate::data_component_impl::food::{
            ConsumableImpl, ConsumeAnimation, ConsumeEffect, PotionContentsImpl,
            StatusEffectInstance, SuspiciousStewEffect, SuspiciousStewEffectsImpl, UseCooldownImpl,
            UseEffectsImpl,
        };
        use crate::data_component_impl::utility::{
            BrewingFuelImpl, CookingFuelImpl, FireworkExplosionImpl, FireworkExplosionShape,
            FireworksImpl, FloatProvider, IntProvider, ProfileImpl, ProfileProperty,
        };
        use crate::data_component_impl::{DataComponentImpl, EquipmentSlot, IDSet, IdOr, LoreImpl};
        use crate::entity_type::EntityType;
        use crate::potion::Potion;
        use crate::sound::Sound;
        use pumpkin_util::text::TextComponent;
        use pumpkin_util::text::color::NamedColor;
        use std::borrow::Cow;

        fn hash(component: &impl DataComponentImpl) -> u32 {
            component.get_hash() as u32
        }

        fn tag(name: &'static str) -> IDSet<crate::damage::DamageType> {
            IDSet::Tag(Cow::Borrowed(name))
        }

        #[test]
        fn enchantments_hash_a_sorted_map_of_ids_to_levels() {
            let levels: Cow<'static, [(&'static Enchantment, i32)]> = Cow::Owned(vec![
                (&Enchantment::SHARPNESS, 5),
                (&Enchantment::UNBREAKING, 3),
            ]);
            let expected = vanilla::map(&[
                ("minecraft:unbreaking", vanilla::int(3)),
                ("minecraft:sharpness", vanilla::int(5)),
            ]);
            let stored = StoredEnchantmentsImpl {
                enchantment: levels.clone(),
            };
            assert_eq!(
                hash(&EnchantmentsImpl {
                    enchantment: levels
                }),
                expected
            );
            assert_eq!(hash(&stored), expected);
        }

        #[test]
        fn weapon_and_attack_range_omit_default_fields() {
            let weapon = |damage, seconds| WeaponImpl {
                item_damage_per_attack: damage,
                disable_blocking_for_seconds: seconds,
            };
            assert_eq!(hash(&weapon(1, 0.0)), vanilla::map(&[]));
            assert_eq!(
                hash(&weapon(2, 5.0)),
                vanilla::map(&[
                    ("item_damage_per_attack", vanilla::int(2)),
                    ("disable_blocking_for_seconds", vanilla::float(5.0)),
                ])
            );
            let range = AttackRangeImpl {
                min_reach: 0.0,
                max_reach: 4.5,
                min_creative_reach: 0.0,
                max_creative_reach: 5.0,
                hitbox_margin: 0.3,
                mob_factor: 1.0,
            };
            assert_eq!(
                hash(&range),
                vanilla::map(&[("max_reach", vanilla::float(4.5))])
            );
        }

        #[test]
        fn holder_set_components_hash_tags_and_ids() {
            let resistant = DamageResistantImpl {
                types: tag("minecraft:is_fire"),
            };
            assert_eq!(
                hash(&resistant),
                vanilla::map(&[("types", vanilla::string("#minecraft:is_fire"))])
            );
            let visibility = MobVisibilityImpl {
                targeting_entity_types: IDSet::IDs(Cow::Owned(vec![
                    &EntityType::ZOMBIE,
                    &EntityType::SKELETON,
                ])),
                visibility: 0.5,
            };
            // `ExtraCodecs.compactListCodec`: two ids are a list.
            let ids = vanilla::list(&[
                vanilla::string("minecraft:zombie"),
                vanilla::string("minecraft:skeleton"),
            ]);
            assert_eq!(
                hash(&visibility),
                vanilla::map(&[
                    ("targeting_entity_types", ids),
                    ("visibility", vanilla::float(0.5)),
                ])
            );
        }

        #[test]
        fn equippable_omits_default_sounds_and_flags() {
            let mut equippable = EquippableImpl {
                slot: &EquipmentSlot::HEAD,
                equip_sound: IdOr::Id(Sound::ItemArmorEquipGeneric),
                asset_id: None,
                camera_overlay: None,
                allowed_entities: None,
                dispensable: true,
                swappable: true,
                damage_on_hurt: true,
                equip_on_interact: false,
                can_be_sheared: false,
                shearing_sound: IdOr::Id(Sound::ItemShearsSnip),
            };
            assert_eq!(
                hash(&equippable),
                vanilla::map(&[("slot", vanilla::string("head"))])
            );
            equippable.equip_sound = IdOr::Id(Sound::ItemArmorEquipDiamond);
            equippable.asset_id = Some(Cow::Borrowed("diamond"));
            equippable.swappable = false;
            assert_eq!(
                hash(&equippable),
                vanilla::map(&[
                    ("slot", vanilla::string("head")),
                    (
                        "equip_sound",
                        vanilla::string("minecraft:item.armor.equip_diamond")
                    ),
                    ("asset_id", vanilla::string("minecraft:diamond")),
                    ("swappable", vanilla::boolean(false)),
                ])
            );
        }

        /// `MobEffectInstance.CODEC` with amplifier 1 and duration 200: the amplifier is an
        /// `ExtraCodecs.UNSIGNED_BYTE` and `show_icon` is always written.
        fn speed() -> (StatusEffectInstance, u32) {
            let effect = StatusEffectInstance {
                effect_id: Cow::Borrowed("minecraft:speed"),
                amplifier: 1,
                duration: 200,
                ambient: false,
                show_particles: true,
                show_icon: true,
            };
            let expected = vanilla::map(&[
                ("id", vanilla::string("minecraft:speed")),
                ("amplifier", vanilla::byte(1)),
                ("duration", vanilla::int(200)),
                ("show_icon", vanilla::boolean(true)),
            ]);
            (effect, expected)
        }

        #[test]
        fn consumable_and_death_protection_hash_consume_effects() {
            let (effect, effect_hash) = speed();
            let effects = vec![
                ConsumeEffect::ApplyEffects((Cow::Owned(vec![effect]), 0.5)),
                ConsumeEffect::ClearAllEffects,
                ConsumeEffect::TeleportRandomly(16.0),
                ConsumeEffect::PlaySound(IdOr::Value(SoundEvent {
                    sound_name: "custom:ding".to_owned(),
                    range: Some(8.0),
                })),
            ];
            // `ConsumeEffect.CODEC` dispatches on `type` and puts the fields next to it.
            let expected_effects = vanilla::list(&[
                vanilla::map(&[
                    ("type", vanilla::string("minecraft:apply_effects")),
                    ("effects", vanilla::list(&[effect_hash])),
                    ("probability", vanilla::float(0.5)),
                ]),
                vanilla::map(&[("type", vanilla::string("minecraft:clear_all_effects"))]),
                vanilla::map(&[("type", vanilla::string("minecraft:teleport_randomly"))]),
                vanilla::map(&[
                    ("type", vanilla::string("minecraft:play_sound")),
                    (
                        "sound",
                        vanilla::map(&[
                            ("sound_id", vanilla::string("custom:ding")),
                            ("range", vanilla::float(8.0)),
                        ]),
                    ),
                ]),
            ]);
            let consumable = ConsumableImpl {
                consume_seconds: 1.6,
                animation: ConsumeAnimation::Drink,
                sound_event: IdOr::Id(Sound::EntityGenericDrink),
                consume_particles: true,
                effects: Cow::Owned(effects.clone()),
            };
            assert_eq!(
                hash(&consumable),
                vanilla::map(&[
                    ("animation", vanilla::string("drink")),
                    ("sound", vanilla::string("minecraft:entity.generic.drink")),
                    ("on_consume_effects", expected_effects),
                ])
            );
            let protection = DeathProtectionImpl {
                death_effects: Cow::Owned(effects),
            };
            assert_eq!(
                hash(&protection),
                vanilla::map(&[("death_effects", expected_effects)])
            );
            let default_consumable = ConsumableImpl {
                consume_seconds: 1.6,
                animation: ConsumeAnimation::Eat,
                sound_event: IdOr::Id(Sound::EntityGenericEat),
                consume_particles: true,
                effects: Cow::Borrowed(&[]),
            };
            assert_eq!(hash(&default_consumable), vanilla::map(&[]));
        }

        #[test]
        fn blocks_attacks_omit_the_default_reduction_and_item_damage() {
            let mut blocks = BlocksAttacksImpl {
                block_delay_seconds: 0.25,
                disable_cooldown_scale: 1.0,
                damage_reductions: Cow::Owned(vec![DamageReduction {
                    horizontal_blocking_angle: 90.0,
                    damage_type: None,
                    base: 0.0,
                    factor: 1.0,
                }]),
                item_damage: ItemDamageFunction {
                    threshold: 1.0,
                    base: 0.0,
                    factor: 1.0,
                },
                bypassed_by: Some(tag("minecraft:bypasses_shield")),
                block_sound: Some(IdOr::Id(Sound::ItemShieldBlock)),
                disabled_sound: None,
            };
            let block_delay = ("block_delay_seconds", vanilla::float(0.25));
            let bypassed_by = ("bypassed_by", vanilla::string("#minecraft:bypasses_shield"));
            let block_sound = (
                "block_sound",
                vanilla::string("minecraft:item.shield.block"),
            );
            assert_eq!(
                hash(&blocks),
                vanilla::map(&[block_delay, bypassed_by, block_sound])
            );
            blocks.damage_reductions = Cow::Owned(vec![DamageReduction {
                horizontal_blocking_angle: 45.0,
                damage_type: Some(tag("minecraft:is_explosion")),
                base: 1.0,
                factor: 0.5,
            }]);
            blocks.item_damage.threshold = 3.0;
            let reduction = vanilla::map(&[
                ("horizontal_blocking_angle", vanilla::float(45.0)),
                ("type", vanilla::string("#minecraft:is_explosion")),
                ("base", vanilla::float(1.0)),
                ("factor", vanilla::float(0.5)),
            ]);
            let item_damage = vanilla::map(&[
                ("threshold", vanilla::float(3.0)),
                ("base", vanilla::float(0.0)),
                ("factor", vanilla::float(1.0)),
            ]);
            assert_eq!(
                hash(&blocks),
                vanilla::map(&[
                    block_delay,
                    ("damage_reductions", vanilla::list(&[reduction])),
                    ("item_damage", item_damage),
                    bypassed_by,
                    block_sound,
                ])
            );
        }

        #[test]
        fn piercing_and_kinetic_weapons_omit_default_fields() {
            let piercing = PiercingWeaponImpl {
                deals_knockback: false,
                dismounts: true,
                sound: Some(IdOr::Id(Sound::ItemTridentThrow)),
                hit_sound: None,
            };
            assert_eq!(
                hash(&piercing),
                vanilla::map(&[
                    ("deals_knockback", vanilla::boolean(false)),
                    ("dismounts", vanilla::boolean(true)),
                    ("sound", vanilla::string("minecraft:item.trident.throw")),
                ])
            );
            let kinetic = KineticWeaponImpl {
                contact_cooldown_ticks: 10,
                delay_ticks: 5,
                dismount_conditions: Some(KineticConditionImpl {
                    max_duration_ticks: 100,
                    min_speed: 0.0,
                    min_relative_speed: 4.0,
                }),
                knockback_conditions: None,
                damage_conditions: None,
                forward_movement: 0.0,
                damage_multiplier: 2.0,
                sound: None,
                hit_sound: None,
            };
            assert_eq!(
                hash(&kinetic),
                vanilla::map(&[
                    ("delay_ticks", vanilla::int(5)),
                    (
                        "dismount_conditions",
                        vanilla::map(&[
                            ("max_duration_ticks", vanilla::int(100)),
                            ("min_relative_speed", vanilla::float(4.0)),
                        ]),
                    ),
                    ("damage_multiplier", vanilla::float(2.0)),
                ])
            );
        }

        #[test]
        fn use_components_hash_like_their_codecs() {
            let effects = UseEffectsImpl {
                can_sprint: false,
                interact_vibrations: true,
                speed_multiplier: 0.2,
            };
            assert_eq!(hash(&effects), vanilla::map(&[]));
            let effects = UseEffectsImpl {
                can_sprint: true,
                interact_vibrations: false,
                speed_multiplier: 1.0,
            };
            assert_eq!(
                hash(&effects),
                vanilla::map(&[
                    ("can_sprint", vanilla::boolean(true)),
                    ("interact_vibrations", vanilla::boolean(false)),
                    ("speed_multiplier", vanilla::float(1.0)),
                ])
            );
            let cooldown = UseCooldownImpl {
                seconds: 0.5,
                cooldown_group: Some("ender_pearl".to_owned()),
            };
            assert_eq!(
                hash(&cooldown),
                vanilla::map(&[
                    ("seconds", vanilla::float(0.5)),
                    ("cooldown_group", vanilla::string("minecraft:ender_pearl")),
                ])
            );
        }

        #[test]
        fn potion_and_stew_effects_hash_their_full_codecs() {
            let (effect, effect_hash) = speed();
            let potion = PotionContentsImpl {
                potion_id: Some(i32::from(Potion::SWIFTNESS.id)),
                custom_color: Some(0x00FF_0000),
                custom_effects: vec![effect],
                custom_name: None,
            };
            assert_eq!(
                hash(&potion),
                vanilla::map(&[
                    ("potion", vanilla::string("minecraft:swiftness")),
                    ("custom_color", vanilla::int(0x00FF_0000)),
                    ("custom_effects", vanilla::list(&[effect_hash])),
                ])
            );
            let stew = SuspiciousStewEffectsImpl {
                effects: Cow::Owned(vec![
                    SuspiciousStewEffect {
                        effect: Cow::Borrowed("minecraft:night_vision"),
                        duration: 160,
                    },
                    SuspiciousStewEffect {
                        effect: Cow::Borrowed("minecraft:poison"),
                        duration: 220,
                    },
                ]),
            };
            assert_eq!(
                hash(&stew),
                vanilla::list(&[
                    vanilla::map(&[("id", vanilla::string("minecraft:night_vision"))]),
                    vanilla::map(&[
                        ("id", vanilla::string("minecraft:poison")),
                        ("duration", vanilla::int(220)),
                    ]),
                ])
            );
        }

        #[test]
        fn fuels_hash_resolvable_numbers() {
            let cooking = CookingFuelImpl {
                burn_time: IntProvider::Inline(200),
                speed_multiplier: FloatProvider::Id(Cow::Borrowed("custom:fast")),
            };
            assert_eq!(
                hash(&cooking),
                vanilla::map(&[
                    ("burn_time", vanilla::int(200)),
                    ("speed_multiplier", vanilla::string("custom:fast")),
                ])
            );
            let brewing = BrewingFuelImpl {
                uses: IntProvider::Id(Cow::Borrowed("blaze_powder_uses")),
                speed_multiplier: FloatProvider::Inline(1.0),
            };
            assert_eq!(
                hash(&brewing),
                vanilla::map(&[
                    ("uses", vanilla::string("minecraft:blaze_powder_uses")),
                    ("speed_multiplier", vanilla::float(1.0)),
                ])
            );
        }

        #[test]
        fn fireworks_hash_explosions_and_a_byte_duration() {
            let explosion = FireworkExplosionImpl {
                shape: FireworkExplosionShape::Star,
                colors: vec![0x00FF_0000],
                fade_colors: Vec::new(),
                has_trail: true,
                has_twinkle: false,
            };
            let explosion_hash = vanilla::map(&[
                ("shape", vanilla::string("star")),
                ("colors", vanilla::list(&[vanilla::int(0x00FF_0000)])),
                ("has_trail", vanilla::boolean(true)),
            ]);
            assert_eq!(hash(&explosion), explosion_hash);
            let fireworks = FireworksImpl {
                flight_duration: 2,
                explosions: vec![explosion],
            };
            assert_eq!(
                hash(&fireworks),
                vanilla::map(&[
                    ("flight_duration", vanilla::byte(2)),
                    ("explosions", vanilla::list(&[explosion_hash])),
                ])
            );
        }

        #[test]
        fn profile_hashes_its_full_codec() {
            let profile = ProfileImpl {
                name: Some("Steve".to_owned()),
                id: Some([1, 2, 3, 4]),
                properties: vec![ProfileProperty {
                    name: "textures".to_owned(),
                    value: "abc".to_owned(),
                    signature: None,
                }],
                texture: Some("entity/player/wide/steve".to_owned()),
                cape: None,
                elytra: None,
                model: Some("slim".to_owned()),
            };
            assert_eq!(
                hash(&profile),
                vanilla::map(&[
                    ("name", vanilla::string("Steve")),
                    ("id", vanilla::int_array(&[1, 2, 3, 4])),
                    (
                        "properties",
                        vanilla::list(&[vanilla::map(&[
                            ("name", vanilla::string("textures")),
                            ("value", vanilla::string("abc")),
                        ])]),
                    ),
                    (
                        "texture",
                        vanilla::string("minecraft:entity/player/wide/steve"),
                    ),
                    ("model", vanilla::string("slim")),
                ])
            );
        }

        #[test]
        fn break_sound_and_block_state_hash_like_their_codecs() {
            let sound = BreakSoundImpl {
                sound: IdOr::Id(Sound::ItemShieldBreak),
            };
            assert_eq!(hash(&sound), vanilla::string("minecraft:item.shield.break"));
            let state = BlockStateImpl {
                properties: Cow::Owned(vec![
                    (Cow::Borrowed("half"), Cow::Borrowed("upper")),
                    (Cow::Borrowed("open"), Cow::Borrowed("true")),
                ]),
            };
            assert_eq!(
                hash(&state),
                vanilla::map(&[
                    ("half", vanilla::string("upper")),
                    ("open", vanilla::string("true")),
                ])
            );
        }

        /// `ComponentSerialization.CODEC` writes plain text as a string and styled text through
        /// its fuzzy map codec, with no `type` field.
        #[test]
        fn lore_hashes_plain_and_styled_text() {
            let lore = LoreImpl {
                lines: vec![
                    TextComponent::text("plain"),
                    TextComponent::text("red")
                        .color_named(NamedColor::Red)
                        .italic(),
                ],
            };
            assert_eq!(
                hash(&lore),
                vanilla::list(&[
                    vanilla::string("plain"),
                    vanilla::map(&[
                        ("text", vanilla::string("red")),
                        ("color", vanilla::string("red")),
                        ("italic", vanilla::boolean(true)),
                    ]),
                ])
            );
        }

        #[test]
        fn rarity_item_name_and_villager_food_hash_their_codecs() {
            use crate::data_component_impl::{ItemNameImpl, Rarity, RarityImpl, VillagerFoodImpl};
            // `Rarity.CODEC` is a `StringRepresentable` codec.
            let rarity = RarityImpl {
                rarity: Rarity::Epic,
            };
            assert_eq!(hash(&rarity), vanilla::string("epic"));
            // The server sends the item name as a translatable text.
            let name = ItemNameImpl {
                name: Cow::Borrowed("item.minecraft.stick"),
            };
            assert_eq!(
                hash(&name),
                vanilla::map(&[("translate", vanilla::string("item.minecraft.stick"))])
            );
            let food = VillagerFoodImpl { nutrition: 4 };
            assert_eq!(hash(&food), vanilla::map(&[("nutrition", vanilla::int(4))]));
        }

        #[test]
        fn damage_type_saves_and_hashes_its_registry_id() {
            use crate::damage::DamageType;
            use crate::data_component_impl::DamageTypeImpl;
            use pumpkin_nbt::tag::NbtTag;
            let fire = DamageTypeImpl {
                damage_type: DamageType::IN_FIRE,
            };
            assert_eq!(
                fire.write_data(),
                NbtTag::String("minecraft:in_fire".into())
            );
            assert_eq!(hash(&fire), vanilla::string("minecraft:in_fire"));
            // Older IronPumpkin saves wrote the message id.
            for saved in ["minecraft:in_fire", "in_fire", "minecraft:inFire"] {
                let loaded = DamageTypeImpl::read_data(&NbtTag::String(saved.into()));
                assert_eq!(
                    loaded.map(|d| d.damage_type.id),
                    Some(DamageType::IN_FIRE.id)
                );
            }
            // `campfire` shares the message id `inFire`; its own id loads it.
            let campfire = DamageTypeImpl::read_data(&NbtTag::String("minecraft:campfire".into()));
            assert_eq!(
                campfire.map(|d| d.damage_type.id),
                Some(DamageType::CAMPFIRE.id)
            );
        }

        /// `Identifier.CODEC` and registry holders encode the full id, so a value saved without
        /// a namespace hashes as `minecraft:` plus the path.
        #[test]
        fn id_components_hash_the_namespaced_id() {
            use crate::data_component_impl::{
                BlockTransformerImpl, CatVariantImpl, FoxVariantImpl, InstrumentImpl,
                ItemModelImpl, NoteBlockSoundImpl, ProvidesPotteryPatternImpl,
                ProvidesTrimMaterialImpl, TooltipStyleImpl,
            };
            let id = |path: &str| vanilla::string(&format!("minecraft:{path}"));
            let model = ItemModelImpl {
                id: Cow::Borrowed("stick"),
            };
            assert_eq!(hash(&model), id("stick"));
            let style = TooltipStyleImpl {
                id: "custom:fancy".to_owned(),
            };
            assert_eq!(hash(&style), vanilla::string("custom:fancy"));
            let note = NoteBlockSoundImpl {
                sound: "block.note_block.bell".to_owned(),
            };
            assert_eq!(hash(&note), id("block.note_block.bell"));
            let instrument = InstrumentImpl {
                instrument: Cow::Borrowed("ponder_goat_horn"),
            };
            assert_eq!(hash(&instrument), id("ponder_goat_horn"));
            let trim = ProvidesTrimMaterialImpl {
                material: Cow::Borrowed("gold"),
            };
            assert_eq!(hash(&trim), id("gold"));
            let pottery = ProvidesPotteryPatternImpl {
                pattern: Cow::Borrowed("archer"),
            };
            assert_eq!(hash(&pottery), id("archer"));
            let transformer = BlockTransformerImpl {
                transformer: Cow::Borrowed("waxing"),
            };
            assert_eq!(hash(&transformer), id("waxing"));
            // A registry holder variant hashes its id, a `StringRepresentable` variant its name.
            let cat = CatVariantImpl {
                value: Cow::Borrowed("tabby"),
            };
            assert_eq!(hash(&cat), id("tabby"));
            let fox = FoxVariantImpl {
                value: Cow::Borrowed("snow"),
            };
            assert_eq!(hash(&fox), vanilla::string("snow"));
        }

        /// `SignText.CODEC` omits `filtered_messages` when they equal the messages.
        #[test]
        fn sign_text_omits_filtered_messages_equal_to_the_messages() {
            use crate::data_component_impl::SignTextFrontImpl;
            let messages = [
                Cow::Borrowed("a"),
                Cow::Borrowed(""),
                Cow::Borrowed(""),
                Cow::Borrowed(""),
            ];
            let sign = SignTextFrontImpl {
                messages: messages.clone(),
                filtered_messages: Some(messages),
                color: crate::dye_color::DyeColor::Black,
                has_glowing_text: false,
            };
            let lines = vanilla::list(&[
                vanilla::string("a"),
                vanilla::string(""),
                vanilla::string(""),
                vanilla::string(""),
            ]);
            assert_eq!(
                hash(&sign),
                vanilla::map(&[
                    ("messages", lines),
                    ("color", vanilla::string("black")),
                    ("has_glowing_text", vanilla::boolean(false)),
                ])
            );
            let saved = sign.write_data();
            assert!(
                saved
                    .extract_compound()
                    .unwrap()
                    .get("filtered_messages")
                    .is_none()
            );
        }

        /// The server translates its own text keys before it sends a text. The hash must cover
        /// the translated text.
        #[test]
        fn lore_hashes_server_translations_as_sent() {
            use pumpkin_util::translation::Locale;
            let lore = LoreImpl {
                lines: vec![TextComponent::custom(
                    "pumpkin",
                    "commands.pumpkin.github",
                    Locale::EnUs,
                    vec![],
                )],
            };
            assert_eq!(
                hash(&lore),
                vanilla::list(&[vanilla::string("[Github Repository]")])
            );
        }

        /// `DataComponentExactPredicate.CODEC` encodes each expected component with its own
        /// codec, so a byte field such as `flight_duration` stays a byte.
        #[test]
        fn predicates_hash_exact_components_with_their_codecs() {
            use crate::data_component_impl::{CanPlaceOnImpl, LockImpl};
            use pumpkin_nbt::compound::NbtCompound;
            use pumpkin_nbt::tag::NbtTag;
            let mut fireworks = NbtCompound::new();
            fireworks.put_byte("flight_duration", 2);
            let mut components = NbtCompound::new();
            components.put_compound("minecraft:fireworks", fireworks);
            let mut predicate = NbtCompound::new();
            predicate.put_string("items", "minecraft:firework_rocket".to_owned());
            predicate.put_compound("components", components.clone());
            let exact = vanilla::map(&[(
                "minecraft:fireworks",
                vanilla::map(&[("flight_duration", vanilla::byte(2))]),
            )]);
            let lock = LockImpl {
                predicate: predicate.clone(),
            };
            assert_eq!(
                hash(&lock),
                vanilla::map(&[
                    ("items", vanilla::string("minecraft:firework_rocket")),
                    ("components", exact),
                ])
            );
            // `AdventureModePredicate.CODEC` is a compact list of block predicates.
            let mut block = NbtCompound::new();
            block.put_string("blocks", "#minecraft:logs".to_owned());
            block.put_compound("components", components);
            let block_hash = vanilla::map(&[
                ("blocks", vanilla::string("#minecraft:logs")),
                ("components", exact),
            ]);
            let one = CanPlaceOnImpl {
                predicate: NbtTag::Compound(block.clone()),
            };
            assert_eq!(hash(&one), block_hash);
            let two = CanPlaceOnImpl {
                predicate: NbtTag::List(vec![
                    NbtTag::Compound(block.clone()),
                    NbtTag::Compound(block),
                ]),
            };
            assert_eq!(hash(&two), vanilla::list(&[block_hash, block_hash]));
        }
    }
}
