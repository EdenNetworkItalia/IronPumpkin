//! Vanilla `HashOps.CRC32C_INSTANCE`: the hash that a client sends for each component of a
//! clicked stack. Each function returns the 4-byte CRC-32C of one encoded value. A compound
//! value feeds the hashes of its parts into a new digest, as a Guava `HashCode` does.

use crate::data_component_impl::{IDSet, IDSetContent};
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
}

fn tag(value: &NbtTag, bytes: Bytes) -> u32 {
    match value {
        NbtTag::End => empty(),
        NbtTag::Byte(value) => match bytes {
            Bytes::Nbt => byte(*value),
            Bytes::Booleans => boolean(*value != 0),
        },
        NbtTag::Short(value) => short(*value),
        NbtTag::Int(value) => int(*value),
        NbtTag::Long(value) => long(*value),
        NbtTag::Float(value) => float(*value),
        NbtTag::Double(value) => double(*value),
        NbtTag::ByteArray(values) => match bytes {
            Bytes::Nbt => byte_array(values),
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
    codec_nbt(&component.to_nbt_tag_for_version(&JavaMinecraftVersion::V_26_3))
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

/// The hash of a mod component value. Only its NBT form is known, so it is hashed like
/// `CustomData`.
#[must_use]
pub fn mod_component(value: &NbtTag) -> u32 {
    nbt(value)
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
}
