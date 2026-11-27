use crate::codec::data_component::{DataComponentCodec, deserialize, serialize};
use crate::ser::{NetworkReadExt, NetworkWriteExt, ReadingError, WritingError};
use crate::{EncodingKey, VarInt};
use pumpkin_data::data_component::DataComponent;
use pumpkin_data::data_component_impl::{
    CustomDataImpl, CustomNameImpl, DataComponentImpl, ItemNameImpl,
};
use pumpkin_data::dynamic::ContentIds;
use pumpkin_data::dynamic_registries::{self, DataComponentTypeEntry};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::item_stack::nesting::NestingGuard;
use pumpkin_nbt::serializer::NbtWriteHelperJava;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::text::TextComponent;
use pumpkin_util::version::JavaMinecraftVersion;
use std::borrow::Cow;
use std::io::Cursor;

#[derive(Clone)]
pub struct ItemStackSerializer<'a>(pub Cow<'a, ItemStack>);

/// The added and removed entry counts of a `DataComponentPatch`.
pub(crate) fn item_component_counts(
    stack: &ItemStack,
    ids: ContentIds,
) -> Result<(VarInt, VarInt), WritingError> {
    let removed = stack
        .patch
        .iter()
        .filter(|(_, data)| data.is_none())
        .count();
    let added = stack.patch.len() - removed + custom_components(stack, ids).len();
    let count = |count: usize| {
        i32::try_from(count)
            .map(VarInt)
            .map_err(|_| WritingError::Message("Too many item components".into()))
    };
    Ok((count(added)?, count(removed)?))
}

/// The entries of a decoded `DataComponentPatch`. Vanilla decodes a patch into a map, so a
/// later entry for a type replaces an earlier one, and a removal replaces a value.
#[derive(Default)]
pub(crate) struct DecodedPatch {
    patch: Vec<(DataComponent, Option<Box<dyn DataComponentImpl>>)>,
    custom: Vec<(u16, NbtTag)>,
}

impl DecodedPatch {
    pub(crate) fn put(&mut self, id: DataComponent, value: Option<Box<dyn DataComponentImpl>>) {
        match self.patch.iter_mut().find(|(entry, _)| *entry == id) {
            Some(entry) => entry.1 = value,
            None => self.patch.push((id, value)),
        }
    }

    pub(crate) fn put_custom(&mut self, id: u16, value: NbtTag) {
        match self.custom.iter_mut().find(|(entry, _)| *entry == id) {
            Some(entry) => entry.1 = value,
            None => self.custom.push((id, value)),
        }
    }

    /// A mod component type has no default on the item, so a removal only drops a value.
    pub(crate) fn remove_custom(&mut self, id: u16) {
        self.custom.retain(|(entry, _)| *entry != id);
    }

    pub(crate) fn into_stack(self, count: u8, item: &'static Item) -> ItemStack {
        let mut stack = ItemStack::new_with_component(count, item, self.patch);
        stack.custom_components = self.custom;
        stack
    }
}

/// The mod component values that a client gets. Only a client that synced the registries knows
/// the mod component types.
pub(crate) fn custom_components(stack: &ItemStack, ids: ContentIds) -> &[(u16, NbtTag)] {
    match ids {
        ContentIds::Real => &stack.custom_components,
        ContentIds::Display => &[],
    }
}

/// The NBT form of a mod component value on the network, as `ByteBufCodecs.fromCodecWithRegistries`
/// writes it.
fn custom_component_bytes(value: &NbtTag) -> Result<Vec<u8>, WritingError> {
    let mut bytes = Vec::new();
    value
        .clone()
        .serialize(&mut NbtWriteHelperJava::new(&mut bytes))
        .map_err(|e| WritingError::Message(e.to_string()))?;
    Ok(bytes)
}

/// Writes the id and the value of each mod component that a client with `ids` gets, with a
/// length prefix if `length_prefixed`.
pub(crate) fn write_custom_components(
    stack: &ItemStack,
    ids: ContentIds,
    length_prefixed: bool,
    write: &mut impl NetworkWriteExt,
) -> Result<(), WritingError> {
    for (id, value) in custom_components(stack, ids) {
        write.put_var_int(&VarInt(i32::from(*id)))?;
        let bytes = custom_component_bytes(value)?;
        if length_prefixed {
            write.put_var_int(&VarInt::from(bytes.len() as i32))?;
        }
        write.write_slice(&bytes)?;
    }
    Ok(())
}

/// The mod component type of a network component id, or `None` for a vanilla id.
pub(crate) fn custom_component_type(id: i32) -> Option<&'static DataComponentTypeEntry> {
    u16::try_from(id)
        .ok()
        .filter(|&id| id >= DataComponent::COUNT)
        .and_then(dynamic_registries::data_component_type)
}

fn decode_custom_component(component_data: &[u8]) -> Result<NbtTag, ReadingError> {
    let mut cursor = Cursor::new(component_data);
    let mut nbt_reader = pumpkin_nbt::deserializer::NbtReadHelperJava::new(&mut cursor);
    NbtTag::deserialize(&mut nbt_reader)
        .map_err(|err| ReadingError::Message(format!("Failed to decode mod component NBT: {err}")))
}

pub(crate) fn read_custom_component(
    read: &mut impl NetworkReadExt,
) -> Result<NbtTag, ReadingError> {
    read.get_nbt_with_version(&JavaMinecraftVersion::V_26_3)?
        .ok_or_else(|| ReadingError::Message("Missing mod component value".into()))
}

fn serialize_item_stack_with_id(
    stack: &ItemStack,
    item_id: u16,
    version: EncodingKey,
    write: &mut impl NetworkWriteExt,
) -> Result<(), WritingError> {
    if version >= JavaMinecraftVersion::V_1_20_5 {
        if stack.is_empty() {
            write.put_var_int(&VarInt(0))
        } else {
            let (to_add, to_remove) = item_component_counts(stack, version.content_ids())?;
            write.put_var_int(&VarInt::from(stack.item_count))?;
            write.put_var_int(&VarInt::from(item_id))?;
            write.put_var_int(&to_add)?;
            write.put_var_int(&to_remove)?;

            for (id, data) in &stack.patch {
                if let Some(data) = data {
                    write.put_var_int(&VarInt(i32::from(id.to_id())))?;
                    serialize(*id, data.as_ref(), write, version.content_ids())?;
                }
            }
            write_custom_components(stack, version.content_ids(), false, write)?;

            for (id, data) in &stack.patch {
                if data.is_none() {
                    write.put_var_int(&VarInt(i32::from(id.to_id())))?;
                }
            }

            Ok(())
        }
    } else if version >= JavaMinecraftVersion::V_1_13_2 {
        if stack.is_empty() {
            write.write_bool(false)
        } else {
            write.write_bool(true)?;
            write.put_var_int(&VarInt::from(item_id))?;
            write.write_i8(stack.item_count as i8)?;
            write.write_u8(0)?; // TAG_End (no NBT)
            Ok(())
        }
    } else if version >= JavaMinecraftVersion::V_1_13 {
        // 1.13 and 1.13.1: short id (-1 if empty), byte count, TAG_End
        if stack.is_empty() {
            write.write_i16_be(-1)
        } else {
            write.write_i16_be(item_id as i16)?;
            write.write_i8(stack.item_count as i8)?;
            write.write_u8(0)?; // TAG_End (no NBT)
            Ok(())
        }
    } else {
        // <= 1.12.2: short id (-1 if empty), byte count, short damage, TAG_End
        if stack.is_empty() {
            write.write_i16_be(-1)
        } else {
            write.write_i16_be(item_id as i16)?;
            write.write_i8(stack.item_count as i8)?;
            write.write_i16_be(0)?; // damage / metadata
            write.write_u8(0)?; // TAG_End (no NBT)
            Ok(())
        }
    }
}

fn serialize_length_prefixed_item_stack_with_id(
    stack: &ItemStack,
    item_id: u16,
    version: EncodingKey,
    write: &mut impl NetworkWriteExt,
) -> Result<(), WritingError> {
    if version >= JavaMinecraftVersion::V_1_20_5 {
        if stack.is_empty() {
            write.put_var_int(&VarInt(0))
        } else {
            let (to_add, to_remove) = item_component_counts(stack, version.content_ids())?;
            write.put_var_int(&VarInt::from(stack.item_count))?;
            write.put_var_int(&VarInt::from(item_id))?;
            write.put_var_int(&to_add)?;
            write.put_var_int(&to_remove)?;

            for (id, data) in &stack.patch {
                if let Some(data) = data {
                    write.put_var_int(&VarInt(i32::from(id.to_id())))?;
                    let mut comp_buf = Vec::new();
                    serialize(*id, data.as_ref(), &mut comp_buf, version.content_ids())?;
                    write.put_var_int(&VarInt::from(comp_buf.len() as i32))?;
                    write.write_slice(&comp_buf)?;
                }
            }
            write_custom_components(stack, version.content_ids(), true, write)?;

            for (id, data) in &stack.patch {
                if data.is_none() {
                    write.put_var_int(&VarInt(i32::from(id.to_id())))?;
                }
            }

            Ok(())
        }
    } else {
        serialize_item_stack_with_id(stack, item_id, version, write)
    }
}

fn serialize_item_cost_with_id(
    stack: &ItemStack,
    item_id: u16,
    version: EncodingKey,
    write: &mut impl NetworkWriteExt,
) -> Result<(), WritingError> {
    let component_count = stack
        .patch
        .iter()
        .filter(|(_, data)| data.is_some())
        .count()
        + custom_components(stack, version.content_ids()).len();
    let component_count = i32::try_from(component_count)
        .map_err(|_| WritingError::Message("Too many item cost components".into()))?;

    write.put_var_int(&VarInt::from(item_id))?;
    write.put_var_int(&VarInt::from(stack.item_count))?;
    write.put_var_int(&VarInt(component_count))?;
    for (id, data) in &stack.patch {
        if let Some(data) = data {
            write.put_var_int(&VarInt(i32::from(id.to_id())))?;
            serialize(*id, data.as_ref(), write, version.content_ids())?;
        }
    }
    write_custom_components(stack, version.content_ids(), false, write)
}

/// Reads a registry id, and fails on a value outside the id type instead of truncating it.
pub(crate) fn read_id<T: TryFrom<i32>>(read: &mut impl NetworkReadExt) -> Result<T, ReadingError> {
    let id = read.get_var_int()?.0;
    T::try_from(id).map_err(|_| ReadingError::Message(format!("Invalid id: {id}")))
}

/// Enters one level of item stack nesting, or fails the decode of a value nested too deep.
pub(crate) fn enter_item_nesting() -> Result<NestingGuard, ReadingError> {
    NestingGuard::enter().ok_or_else(|| ReadingError::Message("Item stacks nested too deep".into()))
}

pub(crate) fn component_id(id_val: i32) -> Result<DataComponent, ReadingError> {
    let id_u8 = id_val
        .try_into()
        .map_err(|_| ReadingError::Message(format!("Invalid component ID: {id_val}")))?;
    DataComponent::try_from_id(id_u8)
        .ok_or_else(|| ReadingError::Message(format!("Unknown component ID: {id_val}")))
}

fn decode_custom_name(component_data: &[u8]) -> Result<Box<dyn DataComponentImpl>, ReadingError> {
    let mut cursor = Cursor::new(component_data);
    let mut nbt_reader = pumpkin_nbt::deserializer::NbtReadHelperJava::new(&mut cursor);
    let tag = NbtTag::deserialize(&mut nbt_reader)
        .map_err(|err| ReadingError::Message(format!("Failed to decode CustomName NBT: {err}")))?;
    let name = TextComponent::from_nbt(&tag);
    Ok(CustomNameImpl { name }.to_dyn())
}

fn decode_item_name(component_data: &[u8]) -> Result<Box<dyn DataComponentImpl>, ReadingError> {
    let mut cursor = Cursor::new(component_data);
    let mut nbt_reader = pumpkin_nbt::deserializer::NbtReadHelperJava::new(&mut cursor);
    let tag = NbtTag::deserialize(&mut nbt_reader)
        .map_err(|err| ReadingError::Message(format!("Failed to decode ItemName NBT: {err}")))?;
    let name = match tag {
        NbtTag::String(name) => name.to_string(),
        NbtTag::Compound(compound) => compound
            .get_string("translate")
            .or_else(|| compound.get_string("text"))
            .unwrap_or_default()
            .to_owned(),
        _ => String::new(),
    };
    Ok(ItemNameImpl {
        name: Cow::Owned(name),
    }
    .to_dyn())
}

fn decode_custom_data(component_data: &[u8]) -> Result<Box<dyn DataComponentImpl>, ReadingError> {
    let mut cursor = Cursor::new(component_data);
    let mut nbt_reader = pumpkin_nbt::deserializer::NbtReadHelperJava::new(&mut cursor);
    let tag = NbtTag::deserialize(&mut nbt_reader)
        .map_err(|err| ReadingError::Message(format!("Failed to decode CustomData NBT: {err}")))?;
    let data = match tag {
        NbtTag::Compound(compound) => compound,
        _ => pumpkin_nbt::compound::NbtCompound::new(),
    };
    Ok(CustomDataImpl::new(data).to_dyn())
}

fn decode_component(
    id: DataComponent,
    component_data: &[u8],
) -> Result<Box<dyn DataComponentImpl>, ReadingError> {
    match id {
        DataComponent::CustomName => decode_custom_name(component_data),
        DataComponent::ItemName => decode_item_name(component_data),
        DataComponent::CustomData => decode_custom_data(component_data),
        _ => {
            let mut cursor = Cursor::new(component_data);
            deserialize(id, &mut cursor)
        }
    }
}

/// A component of a length-prefixed patch: a vanilla component, or the value of a mod component
/// type.
enum PatchEntry {
    Vanilla(DataComponent, Box<dyn DataComponentImpl>),
    Custom(u16, NbtTag),
}

fn read_component_length(read: &mut impl NetworkReadExt) -> Result<usize, ReadingError> {
    let byte_len = read.get_var_int()?.0;
    let byte_len: usize = byte_len
        .try_into()
        .map_err(|_| ReadingError::Message("Negative component data length".into()))?;
    if byte_len > crate::MAX_PACKET_DATA_SIZE {
        return Err(ReadingError::TooLarge("Component data too large".into()));
    }
    Ok(byte_len)
}

fn read_length_prefixed_component(
    read: &mut impl NetworkReadExt,
) -> Result<PatchEntry, ReadingError> {
    let id_val = read.get_var_int()?.0;
    if let Some(entry) = custom_component_type(id_val) {
        let mut component_data = vec![0u8; read_component_length(read)?];
        read.read_bytes_to_buf(&mut component_data)?;
        return Ok(PatchEntry::Custom(
            entry.id,
            decode_custom_component(&component_data)?,
        ));
    }
    let id = component_id(id_val)?;
    let byte_len = read_component_length(read)?;

    let component_impl = if byte_len <= 256 {
        let mut stack_buf = [0u8; 256];
        let slice = &mut stack_buf[..byte_len];
        read.read_bytes_to_buf(slice)?;
        decode_component(id, slice)?
    } else {
        let mut component_data = vec![0u8; byte_len];
        read.read_bytes_to_buf(&mut component_data)?;
        decode_component(id, &component_data)?
    };

    Ok(PatchEntry::Vanilla(id, component_impl))
}

impl ItemStackSerializer<'_> {
    pub fn read(
        read: &mut impl NetworkReadExt,
    ) -> Result<ItemStackSerializer<'static>, ReadingError> {
        const MAX_COMPONENTS: i32 = 256;

        let item_count = read.get_var_int()?;
        if item_count.0 == 0 {
            return Ok(ItemStackSerializer(Cow::Borrowed(ItemStack::EMPTY)));
        }
        let _nesting = enter_item_nesting()?;

        let item_id = read.get_var_int()?;
        let num_to_add = read.get_var_int()?.0;
        let num_to_remove = read.get_var_int()?.0;

        if num_to_add < 0 || num_to_remove < 0 {
            return Err(ReadingError::Message("Negative component count".into()));
        }

        let total_components = num_to_add
            .checked_add(num_to_remove)
            .ok_or_else(|| ReadingError::Message("Component count overflow".into()))?;

        if total_components > MAX_COMPONENTS {
            return Err(ReadingError::Message(
                "Too many components in ItemStack patch".into(),
            ));
        }

        let mut patch = DecodedPatch::default();
        for _ in 0..num_to_add {
            let id_val = read.get_var_int()?.0;
            if let Some(entry) = custom_component_type(id_val) {
                patch.put_custom(entry.id, read_custom_component(read)?);
                continue;
            }
            let id = component_id(id_val)?;

            let component_impl = if id == DataComponent::CustomData {
                CustomDataImpl::deserialize(read)?.to_dyn()
            } else {
                deserialize(id, read)?
            };
            patch.put(id, Some(component_impl));
        }

        for _ in 0..num_to_remove {
            let id_val = read.get_var_int()?.0;
            if let Some(entry) = custom_component_type(id_val) {
                patch.remove_custom(entry.id);
                continue;
            }
            let id = component_id(id_val)?;
            patch.put(id, None);
        }

        let item_id_u16: u16 = item_id
            .0
            .try_into()
            .map_err(|_| ReadingError::Message("Invalid item id!".into()))?;

        let item_count = u8::try_from(item_count.0)
            .map_err(|_| ReadingError::Message("Invalid item count!".into()))?;
        let stack = patch.into_stack(item_count, Item::from_id(item_id_u16).unwrap_or(&Item::AIR));
        Ok(ItemStackSerializer(Cow::Owned(stack)))
    }

    pub fn read_with_version(
        read: &mut impl NetworkReadExt,
        version: &JavaMinecraftVersion,
    ) -> Result<ItemStackSerializer<'static>, ReadingError> {
        if *version >= JavaMinecraftVersion::V_1_20_5 {
            let serializer = Self::read(read)?;
            if *version < JavaMinecraftVersion::V_26_3 {
                Ok(ItemStackSerializer(Cow::Owned(
                    serializer.to_stack_for_version(version),
                )))
            } else {
                Ok(serializer)
            }
        } else if *version >= JavaMinecraftVersion::V_1_13_2 {
            let present = read.get_bool()?;
            if !present {
                return Ok(ItemStackSerializer(Cow::Borrowed(ItemStack::EMPTY)));
            }
            let raw_item_id: u16 = read_id(read)?;
            let count = read.get_i8()? as u8;
            let nbt_type = read.get_u8()?;
            if nbt_type != 0 {
                // TAG_End is 0 when no NBT is present
            }
            let item = Item::from_id(raw_item_id).unwrap_or(&Item::AIR);
            Ok(ItemStackSerializer(Cow::Owned(ItemStack::new(count, item))))
        } else if *version >= JavaMinecraftVersion::V_1_13 {
            let raw_item_id = read.get_i16_be()?;
            if raw_item_id == -1 || raw_item_id < 0 {
                return Ok(ItemStackSerializer(Cow::Borrowed(ItemStack::EMPTY)));
            }
            let count = read.get_i8()? as u8;
            let nbt_type = read.get_u8()?;
            if nbt_type != 0 {
                // TAG_End is 0 when no NBT is present
            }
            let item = Item::from_id(raw_item_id as u16).unwrap_or(&Item::AIR);
            Ok(ItemStackSerializer(Cow::Owned(ItemStack::new(count, item))))
        } else {
            let raw_item_id = read.get_i16_be()?;
            if raw_item_id == -1 || raw_item_id < 0 {
                return Ok(ItemStackSerializer(Cow::Borrowed(ItemStack::EMPTY)));
            }
            let count = read.get_i8()? as u8;
            let _damage = read.get_i16_be()?;
            let nbt_type = read.get_u8()?;
            if nbt_type != 0 {
                // TAG_End is 0 when no NBT is present
            }
            let item = Item::from_id(raw_item_id as u16).unwrap_or(&Item::AIR);
            Ok(ItemStackSerializer(Cow::Owned(ItemStack::new(count, item))))
        }
    }

    pub fn read_untrusted_with_version(
        read: &mut impl NetworkReadExt,
        version: &JavaMinecraftVersion,
    ) -> Result<ItemStackSerializer<'static>, ReadingError> {
        if *version >= JavaMinecraftVersion::V_1_21_5 {
            Self::read_length_prefixed_optional(read)
        } else {
            Self::read_with_version(read, version)
        }
    }

    pub fn read_template_with_version(
        read: &mut impl NetworkReadExt,
        version: &JavaMinecraftVersion,
    ) -> Result<ItemStackSerializer<'static>, ReadingError> {
        if *version < JavaMinecraftVersion::V_26_1 {
            Self::read_with_version(read, version)
        } else {
            Self::read_template0(read, version)
        }
    }

    pub fn read_optional_template_with_version(
        read: &mut impl NetworkReadExt,
        version: &JavaMinecraftVersion,
    ) -> Result<ItemStackSerializer<'static>, ReadingError> {
        if *version < JavaMinecraftVersion::V_26_1 {
            Self::read_with_version(read, version)
        } else if read.get_bool()? {
            Self::read_template0(read, version)
        } else {
            Ok(ItemStackSerializer(Cow::Borrowed(ItemStack::EMPTY)))
        }
    }

    pub fn read_template0(
        read: &mut impl NetworkReadExt,
        _version: &JavaMinecraftVersion,
    ) -> Result<ItemStackSerializer<'static>, ReadingError> {
        const MAX_COMPONENTS: i32 = 256;

        let _nesting = enter_item_nesting()?;
        let raw_item_id = read.get_var_int()?;
        let item_count = read.get_var_int()?;

        let item_id_u16: u16 = raw_item_id
            .0
            .try_into()
            .map_err(|_| ReadingError::Message("Invalid item id!".into()))?;
        let item = Item::from_id(item_id_u16).unwrap_or(&Item::AIR);

        let num_to_add = read.get_var_int()?.0;
        let num_to_remove = read.get_var_int()?.0;

        if num_to_add < 0 || num_to_remove < 0 {
            return Err(ReadingError::Message("Negative component count".into()));
        }

        let total_components = num_to_add
            .checked_add(num_to_remove)
            .ok_or_else(|| ReadingError::Message("Component count overflow".into()))?;

        if total_components > MAX_COMPONENTS {
            return Err(ReadingError::Message(
                "Too many components in ItemStack patch".into(),
            ));
        }

        let mut patch = DecodedPatch::default();
        for _ in 0..num_to_add {
            let id_val = read.get_var_int()?.0;
            if let Some(entry) = custom_component_type(id_val) {
                patch.put_custom(entry.id, read_custom_component(read)?);
                continue;
            }
            let id = component_id(id_val)?;

            let component_impl = if id == DataComponent::CustomData {
                CustomDataImpl::deserialize(read)?.to_dyn()
            } else {
                deserialize(id, read)?
            };
            patch.put(id, Some(component_impl));
        }

        for _ in 0..num_to_remove {
            let id_val = read.get_var_int()?.0;
            if let Some(entry) = custom_component_type(id_val) {
                patch.remove_custom(entry.id);
                continue;
            }
            let id = component_id(id_val)?;
            patch.put(id, None);
        }

        let item_count_u8: u8 = item_count
            .0
            .try_into()
            .map_err(|_| ReadingError::Message("Invalid item count!".into()))?;

        let stack = patch.into_stack(item_count_u8, item);
        if stack.is_empty() {
            return Err(ReadingError::Message(
                "Can't read empty item stack template".into(),
            ));
        }

        Ok(ItemStackSerializer(Cow::Owned(stack)))
    }

    pub fn write(&self, write: &mut impl NetworkWriteExt) -> Result<(), WritingError> {
        self.write_with_version(write, &JavaMinecraftVersion::V_26_3.into())
    }

    pub fn read_length_prefixed_optional(
        read: &mut impl NetworkReadExt,
    ) -> Result<ItemStackSerializer<'static>, ReadingError> {
        const MAX_COMPONENTS: i32 = 256;

        let item_count = read.get_var_int()?;
        if item_count.0 == 0 {
            return Ok(ItemStackSerializer(Cow::Borrowed(ItemStack::EMPTY)));
        }
        let item_count_u8 = item_count
            .0
            .try_into()
            .map_err(|_| ReadingError::Message("Invalid item count!".into()))?;
        let _nesting = enter_item_nesting()?;

        let item_id = read.get_var_int()?;
        let num_to_add = read.get_var_int()?.0;
        let num_to_remove = read.get_var_int()?.0;

        if num_to_add < 0 || num_to_remove < 0 {
            return Err(ReadingError::Message("Negative component count".into()));
        }

        let total_components = num_to_add
            .checked_add(num_to_remove)
            .ok_or_else(|| ReadingError::Message("Component count overflow".into()))?;

        if total_components > MAX_COMPONENTS {
            return Err(ReadingError::Message(
                "Too many components in ItemStack patch".into(),
            ));
        }

        let mut patch = DecodedPatch::default();
        for _ in 0..num_to_add {
            match read_length_prefixed_component(read)? {
                PatchEntry::Vanilla(id, component_impl) => patch.put(id, Some(component_impl)),
                PatchEntry::Custom(id, value) => patch.put_custom(id, value),
            }
        }

        for _ in 0..num_to_remove {
            let id_val = read.get_var_int()?.0;
            match custom_component_type(id_val) {
                Some(entry) => patch.remove_custom(entry.id),
                None => patch.put(component_id(id_val)?, None),
            }
        }

        let item_id_u16 = item_id
            .0
            .try_into()
            .map_err(|_| ReadingError::Message("Invalid item id!".into()))?;

        let stack = patch.into_stack(
            item_count_u8,
            Item::from_id(item_id_u16).unwrap_or(&Item::AIR),
        );
        Ok(ItemStackSerializer(Cow::Owned(stack)))
    }

    pub fn write_with_version(
        &self,
        write: &mut impl NetworkWriteExt,
        version: &EncodingKey,
    ) -> Result<(), WritingError> {
        serialize_item_stack_with_id(
            self.0.as_ref(),
            self.0.item.to_java_network_id(version.content_ids()),
            *version,
            write,
        )
    }

    pub fn write_length_prefixed_with_version(
        &self,
        write: &mut impl NetworkWriteExt,
        version: &EncodingKey,
    ) -> Result<(), WritingError> {
        serialize_length_prefixed_item_stack_with_id(
            self.0.as_ref(),
            self.0.item.to_java_network_id(version.content_ids()),
            *version,
            write,
        )
    }

    pub fn write_item_cost_with_version(
        &self,
        write: &mut impl NetworkWriteExt,
        version: &EncodingKey,
    ) -> Result<(), WritingError> {
        serialize_item_cost_with_id(
            self.0.as_ref(),
            self.0.item.to_java_network_id(version.content_ids()),
            *version,
            write,
        )
    }

    pub fn write_untrusted_with_version(
        &self,
        write: &mut impl NetworkWriteExt,
        version: &EncodingKey,
    ) -> Result<(), WritingError> {
        if *version >= JavaMinecraftVersion::V_1_21_5 {
            self.write_length_prefixed_with_version(write, version)
        } else {
            self.write_with_version(write, version)
        }
    }

    pub fn write_template_with_version(
        &self,
        write: &mut impl NetworkWriteExt,
        version: &EncodingKey,
    ) -> Result<(), WritingError> {
        if *version < JavaMinecraftVersion::V_26_1 {
            self.write_with_version(write, version)
        } else {
            self.write_template0(write, version)
        }
    }

    pub fn write_optional_template_with_version(
        &self,
        write: &mut impl NetworkWriteExt,
        version: &EncodingKey,
    ) -> Result<(), WritingError> {
        if *version < JavaMinecraftVersion::V_26_1 {
            self.write_with_version(write, version)
        } else if !self.0.is_empty() {
            write.write_bool(true)?;
            self.write_template0(write, version)
        } else {
            write.write_bool(false)
        }
    }

    pub fn write_template0(
        &self,
        write: &mut impl NetworkWriteExt,
        version: &EncodingKey,
    ) -> Result<(), WritingError> {
        if self.0.is_empty() {
            return Err(WritingError::Message(
                "Can't write empty item stack template".into(),
            ));
        }
        let (to_add, to_remove) = item_component_counts(self.0.as_ref(), version.content_ids())?;
        write.put_var_int(&VarInt::from(
            self.0.item.to_java_network_id(version.content_ids()),
        ))?;
        write.put_var_int(&VarInt::from(self.0.item_count))?;
        write.put_var_int(&to_add)?;
        write.put_var_int(&to_remove)?;

        for (id, data) in &self.0.patch {
            if let Some(data) = data {
                write.put_var_int(&VarInt(i32::from(id.to_id())))?;
                serialize(*id, data.as_ref(), write, version.content_ids())?;
            }
        }
        write_custom_components(self.0.as_ref(), version.content_ids(), false, write)?;

        for (id, data) in &self.0.patch {
            if data.is_none() {
                write.put_var_int(&VarInt(i32::from(id.to_id())))?;
            }
        }

        Ok(())
    }

    #[must_use]
    pub fn to_stack(self) -> ItemStack {
        self.0.into_owned()
    }

    #[must_use]
    pub fn to_stack_for_version(self, _version: &JavaMinecraftVersion) -> ItemStack {
        self.0.into_owned()
    }
}

impl From<ItemStack> for ItemStackSerializer<'_> {
    fn from(item: ItemStack) -> Self {
        ItemStackSerializer(Cow::Owned(item))
    }
}

impl From<Option<ItemStack>> for ItemStackSerializer<'_> {
    fn from(item: Option<ItemStack>) -> Self {
        item.map_or_else(
            || ItemStackSerializer(Cow::Borrowed(ItemStack::EMPTY)),
            ItemStackSerializer::from,
        )
    }
}

#[derive(Debug, Clone)]
pub struct ItemComponentHash {
    pub added: Vec<(VarInt, i32)>,
    pub removed: Vec<VarInt>,
}

impl ItemComponentHash {
    pub fn read(read: &mut impl NetworkReadExt) -> Result<Self, ReadingError> {
        const MAX_COMPONENTS: i32 = 256;

        let added_length = read.get_var_int()?;
        if added_length.0 < 0 || added_length.0 > MAX_COMPONENTS {
            return Err(ReadingError::Message("added_length out of bounds".into()));
        }
        // `HashedPatchMap.STREAM_CODEC` reads the added hashes into a map and the removed
        // types into a set.
        let mut added: Vec<(VarInt, i32)> = Vec::with_capacity(added_length.0 as usize);
        for _ in 0..added_length.0 {
            let component_id = read.get_var_int()?;
            let component_value = read.get_i32()?;
            match added.iter_mut().find(|(id, _)| *id == component_id) {
                Some(entry) => entry.1 = component_value,
                None => added.push((component_id, component_value)),
            }
        }

        let removed_length = read.get_var_int()?;
        if removed_length.0 < 0 || removed_length.0 > MAX_COMPONENTS {
            return Err(ReadingError::Message("removed_length out of bounds".into()));
        }
        let mut removed = Vec::with_capacity(removed_length.0 as usize);
        for _ in 0..removed_length.0 {
            let component_id = read.get_var_int()?;
            if !removed.contains(&component_id) {
                removed.push(component_id);
            }
        }

        Ok(Self { added, removed })
    }

    pub fn write(&self, write: &mut impl NetworkWriteExt) -> Result<(), WritingError> {
        write.put_var_int(&VarInt::from(self.added.len() as i32))?;
        for (id, val) in &self.added {
            write.put_var_int(id)?;
            write.put_i32(*val)?;
        }
        write.put_var_int(&VarInt::from(self.removed.len() as i32))?;
        for id in &self.removed {
            write.put_var_int(id)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ItemStackHash {
    item_id: VarInt,
    count: VarInt,
    components: ItemComponentHash,
}

#[derive(Debug, Clone)]
pub struct OptionalItemStackHash(pub Option<ItemStackHash>);

impl OptionalItemStackHash {
    pub fn read(read: &mut impl NetworkReadExt) -> Result<Self, ReadingError> {
        let is_some = read.get_bool()?;
        if is_some {
            let item_id = read.get_var_int()?;
            let count = read.get_var_int()?;
            let components = ItemComponentHash::read(read)?;

            Ok(Self(Some(ItemStackHash {
                item_id,
                count,
                components,
            })))
        } else {
            Ok(Self(None))
        }
    }

    pub fn write(&self, write: &mut impl NetworkWriteExt) -> Result<(), WritingError> {
        if let Some(hash) = &self.0 {
            write.put_bool(true)?;
            write.put_var_int(&hash.item_id)?;
            write.put_var_int(&hash.count)?;
            hash.components.write(write)?;
        } else {
            write.put_bool(false)?;
        }
        Ok(())
    }

    /// `HashedStack.matches`: the item, the count and `HashedPatchMap.matches` of the patch.
    #[must_use]
    pub fn hash_equals(&self, other: &ItemStack) -> bool {
        let Some(hash) = &self.0 else {
            return other.is_empty();
        };
        if hash.count != other.item_count.into() {
            return false;
        }
        // A `Real` client hashes the custom id and the mod components, any other client the
        // display id and no mod components.
        [ContentIds::Display, ContentIds::Real]
            .into_iter()
            .any(|ids| {
                hash.item_id == other.item.to_java_network_id(ids).into()
                    && hash.components.matches(other, ids)
            })
    }
}

impl ItemComponentHash {
    fn matches(&self, stack: &ItemStack, ids: ContentIds) -> bool {
        let mut removed = Vec::new();
        let mut added = Vec::new();
        for (id, data) in &stack.patch {
            let id = VarInt::from(id.to_id());
            match data {
                Some(data) => added.push((id, data.get_hash())),
                None => removed.push(id),
            }
        }
        for (id, value) in custom_components(stack, ids) {
            added.push((
                VarInt(i32::from(*id)),
                pumpkin_data::data_component_impl::hash_ops::mod_component(value) as i32,
            ));
        }
        removed.len() == self.removed.len()
            && removed.iter().all(|id| self.removed.contains(id))
            && added.len() == self.added.len()
            && added
                .iter()
                .all(|entry| self.added.iter().any(|sent| sent == entry))
    }
}

pub struct ItemStackTemplateSerializer<'a>(pub Cow<'a, ItemStack>);

impl ItemStackTemplateSerializer<'_> {
    pub fn write_with_version(
        &self,
        write: &mut impl NetworkWriteExt,
        version: &EncodingKey,
    ) -> Result<(), WritingError> {
        let serializer = ItemStackSerializer(Cow::Borrowed(self.0.as_ref()));
        serializer.write_template_with_version(write, version)
    }

    pub fn write(&self, write: &mut impl NetworkWriteExt) -> Result<(), WritingError> {
        self.write_with_version(write, &JavaMinecraftVersion::V_26_3.into())
    }
}

impl From<ItemStack> for ItemStackTemplateSerializer<'_> {
    fn from(item: ItemStack) -> Self {
        ItemStackTemplateSerializer(Cow::Owned(item))
    }
}

pub struct ItemStackOptionalTemplateSerializer<'a>(pub Cow<'a, ItemStack>);

impl ItemStackOptionalTemplateSerializer<'_> {
    pub fn write_with_version(
        &self,
        write: &mut impl NetworkWriteExt,
        version: &EncodingKey,
    ) -> Result<(), WritingError> {
        let serializer = ItemStackSerializer(Cow::Borrowed(self.0.as_ref()));
        serializer.write_optional_template_with_version(write, version)
    }

    pub fn write(&self, write: &mut impl NetworkWriteExt) -> Result<(), WritingError> {
        self.write_with_version(write, &JavaMinecraftVersion::V_26_3.into())
    }
}

impl From<ItemStack> for ItemStackOptionalTemplateSerializer<'_> {
    fn from(item: ItemStack) -> Self {
        ItemStackOptionalTemplateSerializer(Cow::Owned(item))
    }
}

impl From<Option<ItemStack>> for ItemStackOptionalTemplateSerializer<'_> {
    fn from(item: Option<ItemStack>) -> Self {
        item.map_or_else(
            || ItemStackOptionalTemplateSerializer(Cow::Borrowed(ItemStack::EMPTY)),
            ItemStackOptionalTemplateSerializer::from,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_data::data_component_impl::PotDecorationsImpl;

    /// `HashOps.createString`: the tag, the UTF-16 length and the UTF-16 code units.
    fn string_hash(value: &str) -> u32 {
        let mut bytes = vec![12];
        bytes.extend((value.encode_utf16().count() as i32).to_le_bytes());
        bytes.extend(value.encode_utf16().flat_map(u16::to_le_bytes));
        crc32c::crc32c(&bytes)
    }

    /// `HashOps.createMap` of one entry.
    fn map_hash(key: &str, value: u32) -> u32 {
        let mut bytes = vec![2];
        bytes.extend(string_hash(key).to_le_bytes());
        bytes.extend(value.to_le_bytes());
        bytes.push(3);
        crc32c::crc32c(&bytes)
    }

    /// The bytes of `HashedStack.STREAM_CODEC` for a stack with one added component.
    fn client_hash(item: &Item, component: DataComponent, hash: i32) -> OptionalItemStackHash {
        let mut bytes = vec![1];
        bytes.write_var_int(&VarInt(i32::from(item.id))).unwrap();
        bytes.write_var_int(&VarInt(1)).unwrap();
        bytes.write_var_int(&VarInt(1)).unwrap();
        bytes
            .write_var_int(&VarInt(i32::from(component.to_id())))
            .unwrap();
        bytes.extend(hash.to_be_bytes());
        bytes.write_var_int(&VarInt(0)).unwrap();
        OptionalItemStackHash::read(&mut bytes.as_slice()).unwrap()
    }

    #[test]
    fn client_hash_of_decorated_pot_matches() {
        let pot = PotDecorationsImpl {
            back: Some(ItemStack::new(1, &Item::BRICK)),
            left: None,
            right: None,
            front: None,
        };
        let stack = ItemStack::new_with_component(
            1,
            &Item::DECORATED_POT,
            vec![(DataComponent::PotDecorations, Some(pot.to_dyn()))],
        );
        // `PotDecorations.CODEC` with one side, an `ItemStackTemplate.CODEC` map with its id.
        let expected = map_hash("back", map_hash("id", string_hash("minecraft:brick"))) as i32;
        let pot = DataComponent::PotDecorations;
        assert!(client_hash(&Item::DECORATED_POT, pot, expected).hash_equals(&stack));
        assert!(!client_hash(&Item::DECORATED_POT, pot, expected ^ 1).hash_equals(&stack));
        // The same number of components with another type does not match.
        let other = DataComponent::Damage;
        assert!(!client_hash(&Item::DECORATED_POT, other, expected).hash_equals(&stack));
    }

    /// A creative slot stack in the 26.3 shape: a bundle that holds `levels` bundles inside
    /// each other, each an `ItemStackTemplate` with one `bundle_contents` entry.
    fn nested_bundles(levels: usize) -> Vec<u8> {
        let var_int = |value: i32| {
            let mut bytes = Vec::new();
            bytes.write_var_int(&VarInt(value)).unwrap();
            bytes
        };
        let bundle = i32::from(Item::BUNDLE.id);
        let contents = i32::from(DataComponent::BundleContents.to_id());
        // Item, count, one added and no removed component, then one bundle entry.
        let level = [
            var_int(bundle),
            var_int(1),
            var_int(1),
            var_int(0),
            var_int(contents),
            var_int(1),
        ]
        .concat();
        let innermost = [var_int(bundle), var_int(1), var_int(0), var_int(0)].concat();
        let mut data = var_int(1);
        data.extend(level.repeat(levels - 1));
        data.extend(innermost);
        [
            var_int(1),
            var_int(bundle),
            var_int(1),
            var_int(0),
            var_int(contents),
            var_int(data.len() as i32),
            data,
        ]
        .concat()
    }

    fn read_creative_stack(bytes: &[u8]) -> Result<ItemStackSerializer<'static>, ReadingError> {
        ItemStackSerializer::read_length_prefixed_optional(&mut &*bytes)
    }

    #[test]
    fn nested_templates_up_to_the_limit_decode_on_a_small_stack() {
        use pumpkin_data::item_stack::nesting::MAX_ITEM_NESTING;
        // The top stack and `MAX_ITEM_NESTING - 1` templates inside it.
        let bytes = nested_bundles(MAX_ITEM_NESTING as usize - 1);
        let decoded = std::thread::Builder::new()
            .stack_size(1024 * 1024)
            .spawn(move || read_creative_stack(&bytes).map(super::ItemStackSerializer::to_stack))
            .unwrap()
            .join()
            .expect("decode does not overflow a 1 MiB stack");
        assert_eq!(decoded.unwrap().item, &Item::BUNDLE);
        assert!(read_creative_stack(&nested_bundles(MAX_ITEM_NESTING as usize)).is_err());
    }

    #[test]
    fn deeply_nested_templates_fail_to_decode() {
        let bytes = nested_bundles(100_000);
        assert!(read_creative_stack(&bytes).is_err());
        // The failed decode releases every level.
        assert!(read_creative_stack(&nested_bundles(2)).is_ok());
    }

    /// A creative slot stick whose `can_place_on` holds a block predicate whose exact components
    /// hold `can_place_on` again, `levels` predicates deep.
    fn nested_block_predicates(levels: usize) -> Vec<u8> {
        let can_place_on = i32::from(DataComponent::CanPlaceOn.to_id());
        // One predicate: no blocks, no properties, no nbt, one exact component.
        let level = var_ints(&[1, 0, 0, 0, 1, can_place_on]);
        let mut data = level.repeat(levels);
        data.push(0);
        // The partial predicates of each level.
        data.extend(std::iter::repeat_n(0, levels));
        let stick = i32::from(Item::STICK.id);
        let mut bytes = var_ints(&[1, stick, 1, 0, can_place_on, data.len() as i32]);
        bytes.extend(data);
        bytes
    }

    #[test]
    fn block_predicates_count_in_the_nesting_limit() {
        use pumpkin_data::item_stack::nesting::MAX_ITEM_NESTING;
        // The stick is one level, and each predicate is one more.
        let within = nested_block_predicates(MAX_ITEM_NESTING as usize - 1);
        assert!(read_creative_stack(&within).is_ok());
        assert!(read_creative_stack(&nested_block_predicates(MAX_ITEM_NESTING as usize)).is_err());
        assert!(read_creative_stack(&within).is_ok());
    }

    fn var_ints(values: &[i32]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for value in values {
            bytes.write_var_int(&VarInt(*value)).unwrap();
        }
        bytes
    }

    /// `DataComponentPatch.STREAM_CODEC` decodes into a map: a later entry for the same type
    /// replaces an earlier one.
    #[test]
    fn duplicate_components_decode_into_one_entry() {
        let unbreakable = i32::from(DataComponent::Unbreakable.to_id());
        let sword = i32::from(Item::DIAMOND_SWORD.id);
        let mut bytes = var_ints(&[1, sword, 256, 0]);
        for _ in 0..256 {
            // The type, then the length of its empty `Unit` value.
            bytes.extend(var_ints(&[unbreakable, 0]));
        }
        let stack = read_creative_stack(&bytes).unwrap().to_stack();
        let entries = |stack: &ItemStack| {
            stack
                .patch
                .iter()
                .filter(|(id, _)| *id == DataComponent::Unbreakable)
                .count()
        };
        assert_eq!(entries(&stack), 1);

        let mut written = Vec::new();
        ItemStackSerializer::from(stack)
            .write_length_prefixed_with_version(&mut written, &JavaMinecraftVersion::V_26_3.into())
            .unwrap();
        assert_eq!(
            written,
            [var_ints(&[1, sword, 1, 0]), var_ints(&[unbreakable, 0])].concat()
        );
    }

    #[test]
    fn a_later_template_entry_replaces_an_earlier_one() {
        use crate::codec::data_component::deserialize;
        use pumpkin_data::data_component_impl::{BundleContentsImpl, DamageImpl};
        let damage = i32::from(DataComponent::Damage.to_id());
        let sword = i32::from(Item::DIAMOND_SWORD.id);
        // One template: two `damage` values, then `damage` removed; a second template: two
        // `damage` values.
        let bytes = [
            var_ints(&[2]),
            var_ints(&[sword, 1, 2, 1, damage, 5, damage, 7, damage]),
            var_ints(&[sword, 1, 2, 0, damage, 5, damage, 7]),
        ]
        .concat();
        let bundle = deserialize(DataComponent::BundleContents, &mut bytes.as_slice()).unwrap();
        let bundle = bundle
            .as_any()
            .downcast_ref::<BundleContentsImpl>()
            .unwrap();
        let [removed, replaced] = bundle.items.as_slice() else {
            panic!("two templates expected");
        };
        assert_eq!(removed.patch.len(), 1);
        assert!(matches!(removed.patch[0], (DataComponent::Damage, None)));
        assert_eq!(replaced.patch.len(), 1);
        assert_eq!(
            replaced
                .get_data_component::<DamageImpl>()
                .map(|d| d.damage),
            Some(7)
        );
    }

    /// `ItemContainerContents.STREAM_CODEC` is a list of optional templates, one per slot up to
    /// the last filled one.
    #[test]
    fn container_writes_one_entry_per_slot() {
        use crate::codec::data_component::serialize;
        use pumpkin_data::data_component_impl::ContainerImpl;
        let container = ContainerImpl {
            items: vec![(2, ItemStack::new(3, &Item::DIAMOND))],
        };
        let mut bytes = Vec::new();
        serialize(
            DataComponent::Container,
            &container,
            &mut bytes,
            pumpkin_data::dynamic::ContentIds::Display,
        )
        .unwrap();
        let diamond = i32::from(Item::DIAMOND.id);
        // Three slots: two empty, then the template with its item, count and empty patch.
        let expected = [var_ints(&[3]), vec![0, 0, 1], var_ints(&[diamond, 3, 0, 0])].concat();
        assert_eq!(bytes, expected);
        let decoded = crate::codec::data_component::deserialize(
            DataComponent::Container,
            &mut bytes.as_slice(),
        )
        .unwrap();
        let decoded = decoded.as_any().downcast_ref::<ContainerImpl>().unwrap();
        assert_eq!(decoded.items.len(), 1);
        assert_eq!(decoded.items[0].0, 2);
    }

    /// `ChargedProjectiles.STREAM_CODEC` is a list of `ItemStackTemplate.STREAM_CODEC` values.
    #[test]
    fn charged_projectiles_write_their_templates() {
        use crate::codec::data_component::{deserialize, serialize};
        use pumpkin_data::data_component_impl::ChargedProjectilesImpl;
        let arrow = ItemStack::new(1, &Item::ARROW).write_item_stack_template();
        let pumpkin_nbt::tag::NbtTag::Compound(arrow) = arrow else {
            panic!("a template with one item saves as a compound or an id");
        };
        let projectiles = ChargedProjectilesImpl {
            projectiles: vec![arrow],
        };
        let mut bytes = Vec::new();
        serialize(
            DataComponent::ChargedProjectiles,
            &projectiles,
            &mut bytes,
            pumpkin_data::dynamic::ContentIds::Display,
        )
        .unwrap();
        let expected = var_ints(&[1, i32::from(Item::ARROW.id), 1, 0, 0]);
        assert_eq!(bytes, expected);
        let decoded =
            deserialize(DataComponent::ChargedProjectiles, &mut bytes.as_slice()).unwrap();
        let decoded = decoded
            .as_any()
            .downcast_ref::<ChargedProjectilesImpl>()
            .unwrap();
        let stack = ItemStack::read_item_stack(&decoded.projectiles[0]).unwrap();
        assert_eq!(stack.item, &Item::ARROW);
    }

    /// Saved crossbows whose charged projectile is a crossbow, `levels` deep.
    fn saved_crossbow_chain(levels: usize) -> pumpkin_nbt::compound::NbtCompound {
        use pumpkin_nbt::compound::NbtCompound;
        let mut stack = NbtCompound::new();
        stack.put_string("id", "minecraft:crossbow".to_owned());
        for _ in 1..levels {
            let mut components = NbtCompound::new();
            components.put_list(
                "minecraft:charged_projectiles",
                vec![pumpkin_nbt::tag::NbtTag::Compound(stack)],
            );
            let mut outer = NbtCompound::new();
            outer.put_string("id", "minecraft:crossbow".to_owned());
            outer.put_compound("components", components);
            stack = outer;
        }
        stack
    }

    /// Charged projectiles keep their saved form and load one level at a time, so the write and
    /// the hash count each level in the nesting limit. The client gets the levels within the
    /// limit, and its hash of them matches the hash of the saved stack.
    #[test]
    fn charged_projectiles_count_in_the_nesting_limit() {
        use crate::codec::data_component::{deserialize, serialize};
        use pumpkin_data::data_component_impl::ChargedProjectilesImpl;
        use pumpkin_data::item_stack::nesting::MAX_ITEM_NESTING;
        let crossbow =
            ItemStack::read_item_stack(&saved_crossbow_chain(MAX_ITEM_NESTING as usize + 8))
                .unwrap();
        let charged = crossbow
            .get_data_component::<ChargedProjectilesImpl>()
            .unwrap();
        let mut bytes = Vec::new();
        serialize(
            DataComponent::ChargedProjectiles,
            charged,
            &mut bytes,
            ContentIds::Display,
        )
        .unwrap();
        let decoded = deserialize(DataComponent::ChargedProjectiles, &mut bytes.as_slice())
            .expect("the levels within the limit decode");
        assert_eq!(decoded.get_hash(), charged.get_hash());
    }

    /// A var int id above the id type is an error, not a truncated id of another entry.
    #[test]
    fn out_of_range_ids_fail_to_decode() {
        let unbreakable = i32::from(DataComponent::Unbreakable.to_id());
        let sword = i32::from(Item::DIAMOND_SWORD.id);
        // A non-length-prefixed stack with the component id `unbreakable + 256`.
        let bytes = var_ints(&[1, sword, 1, 0, unbreakable + 256]);
        assert!(ItemStackSerializer::read(&mut bytes.as_slice()).is_err());
        // A bundle template with the item id `diamond + 65536`.
        let bytes = var_ints(&[1, i32::from(Item::DIAMOND.id) + 65_536, 1, 0, 0]);
        assert!(
            crate::codec::data_component::deserialize(
                DataComponent::BundleContents,
                &mut bytes.as_slice()
            )
            .is_err()
        );
    }

    /// `HashedPatchMap.STREAM_CODEC` reads the added hashes into a map and the removed types
    /// into a set, so a repeated entry counts once.
    #[test]
    fn repeated_click_hash_entries_count_once() {
        let damage = i32::from(DataComponent::Damage.to_id());
        let sword = i32::from(Item::DIAMOND_SWORD.id);
        let stack = ItemStack::new_with_component(
            1,
            &Item::DIAMOND_SWORD,
            vec![(DataComponent::Damage, None)],
        );
        // Item, count, no added hash, then `damage` removed twice.
        let bytes = [vec![1], var_ints(&[sword, 1, 0, 2, damage, damage])].concat();
        let hash = OptionalItemStackHash::read(&mut bytes.as_slice()).unwrap();
        assert!(hash.hash_equals(&stack));
    }

    /// `MobEffectInstance.Details.STREAM_CODEC` writes each hidden effect after an optional
    /// flag, and `TeleportRandomlyConsumeEffect.STREAM_CODEC` writes the diameter and the
    /// `directional_particles` flag.
    #[test]
    fn hidden_effects_and_teleport_flags_reach_the_wire() {
        use crate::codec::data_component::{deserialize, serialize};
        use pumpkin_data::data_component_impl::{
            ConsumableImpl, ConsumeAnimation, ConsumeEffect, PotionContentsImpl,
            StatusEffectInstance,
        };
        use pumpkin_data::effect::StatusEffect;
        let details = |amplifier, duration, hidden_effect| StatusEffectInstance {
            effect_id: std::borrow::Cow::Borrowed("minecraft:speed"),
            amplifier,
            duration,
            ambient: false,
            show_particles: true,
            show_icon: true,
            hidden_effect,
        };
        let potion = PotionContentsImpl {
            potion_id: None,
            custom_color: None,
            custom_effects: vec![details(1, 200, Some(Box::new(details(0, 1000, None))))],
            custom_name: None,
        };
        let mut bytes = Vec::new();
        let ids = pumpkin_data::dynamic::ContentIds::Display;
        serialize(DataComponent::PotionContents, &potion, &mut bytes, ids).unwrap();
        let speed = i32::from(StatusEffect::SPEED.id);
        // No potion, no color, one effect: id, then amplifier, duration and three flags per
        // level, each followed by the hidden effect flag; then no custom name.
        let expected = [
            vec![0, 0],
            var_ints(&[1, speed, 1, 200]),
            vec![0, 1, 1, 1],
            var_ints(&[0, 1000]),
            vec![0, 1, 1, 0],
            vec![0],
        ]
        .concat();
        assert_eq!(bytes, expected);
        let decoded = deserialize(DataComponent::PotionContents, &mut bytes.as_slice()).unwrap();
        let decoded = decoded
            .as_any()
            .downcast_ref::<PotionContentsImpl>()
            .unwrap();
        assert_eq!(decoded.custom_effects, potion.custom_effects);

        let consumable = ConsumableImpl {
            consume_seconds: 1.6,
            animation: ConsumeAnimation::Eat,
            sound_event: pumpkin_data::data_component_impl::IdOr::Id(
                pumpkin_data::sound::Sound::EntityGenericEat,
            ),
            consume_particles: true,
            effects: std::borrow::Cow::Owned(vec![ConsumeEffect::TeleportRandomly {
                diameter: 8.0,
                directional_particles: false,
            }]),
        };
        let mut bytes = Vec::new();
        serialize(DataComponent::Consumable, &consumable, &mut bytes, ids).unwrap();
        let tail = [var_ints(&[1, 3]), 8.0f32.to_be_bytes().to_vec(), vec![0]].concat();
        assert!(bytes.ends_with(&tail));
        let decoded = deserialize(DataComponent::Consumable, &mut bytes.as_slice()).unwrap();
        let decoded = decoded.as_any().downcast_ref::<ConsumableImpl>().unwrap();
        assert_eq!(decoded.effects, consumable.effects);
    }
}
