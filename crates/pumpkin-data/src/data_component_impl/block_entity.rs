use crate::data_component_impl::{DataComponentImpl, hash_ops};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;

#[derive(Clone, Debug, PartialEq)]
pub struct BlockEntityDataImpl {
    pub nbt: NbtCompound,
}
impl BlockEntityDataImpl {
    pub fn read_data(tag: &NbtTag) -> Option<Self> {
        if let NbtTag::Compound(c) = tag {
            Some(Self { nbt: c.clone() })
        } else {
            None
        }
    }
}
impl DataComponentImpl for BlockEntityDataImpl {
    fn write_data(&self) -> NbtTag {
        NbtTag::Compound(self.nbt.clone())
    }
    /// `TypedEntityData.codec` encodes the tag with its `id` as `CustomData`.
    fn get_hash(&self) -> i32 {
        hash_ops::nbt_compound(&self.nbt) as i32
    }
    default_impl!(BlockEntityData);
}

/// Entity NBT carried by an item, e.g. a spawn egg (`TypedEntityData<EntityType<?>>`).
#[derive(Clone, Debug)]
pub struct EntityDataImpl {
    pub entity_type: Option<&'static crate::entity_type::EntityType>,
    pub nbt: Option<NbtCompound>,
}
impl PartialEq for EntityDataImpl {
    fn eq(&self, other: &Self) -> bool {
        if self.get_entity_type().map(|e| e.id) != other.get_entity_type().map(|e| e.id) {
            return false;
        }
        let strip_id = |opt: &Option<NbtCompound>| -> NbtCompound {
            let mut c = opt.clone().unwrap_or_default();
            c.child_tags.remove("id");
            c
        };
        strip_id(&self.nbt) == strip_id(&other.nbt)
    }
}
impl EntityDataImpl {
    #[must_use]
    pub fn get_entity_type(&self) -> Option<&'static crate::entity_type::EntityType> {
        self.entity_type.or_else(|| {
            let id = self.nbt.as_ref()?.get_string("id")?;
            crate::entity_type::EntityType::from_name(id.strip_prefix("minecraft:").unwrap_or(id))
        })
    }

    pub fn read_data(tag: &NbtTag) -> Option<Self> {
        let NbtTag::Compound(c) = tag else {
            return None;
        };
        let id = c.get_string("id")?;
        let entity_type =
            crate::entity_type::EntityType::from_name(id.strip_prefix("minecraft:").unwrap_or(id))?;
        let mut nbt = c.clone();
        nbt.child_tags.remove("id");
        Some(Self {
            entity_type: Some(entity_type),
            nbt: (!nbt.is_empty()).then_some(nbt),
        })
    }
}
impl DataComponentImpl for EntityDataImpl {
    fn write_data(&self) -> NbtTag {
        let mut out = self.nbt.clone().unwrap_or_default();
        if let Some(et) = self.get_entity_type() {
            out.put_string(
                "id",
                crate::dynamic::namespaced_name(et.resource_name).into_owned(),
            );
            NbtTag::Compound(out)
        } else if out.is_empty() {
            NbtTag::End
        } else {
            NbtTag::Compound(out)
        }
    }
    /// `TypedEntityData.codec` encodes the tag with its `id` as `CustomData`.
    fn get_hash(&self) -> i32 {
        let id = self
            .get_entity_type()
            .map(|entity| crate::dynamic::namespaced_name(entity.resource_name));
        let tags = self.nbt.iter().flat_map(|nbt| nbt.child_tags.iter());
        let mut entries: Vec<(u32, u32)> = tags
            .filter(|(key, _)| id.is_none() || &***key != "id")
            .map(|(key, value)| (hash_ops::string(key), hash_ops::nbt(value)))
            .collect();
        match id {
            Some(id) => entries.push((hash_ops::string("id"), hash_ops::string(&id))),
            None if entries.is_empty() => return hash_ops::empty() as i32,
            None => {}
        }
        hash_ops::map(entries) as i32
    }
    default_impl!(EntityData);
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct BucketEntityDataImpl {
    pub nbt: Option<NbtCompound>,
}
#[allow(non_upper_case_globals)]
pub const BucketEntityDataImpl: BucketEntityDataImpl = BucketEntityDataImpl { nbt: None };
impl BucketEntityDataImpl {
    pub fn read_data(data: &NbtTag) -> Option<Self> {
        if let NbtTag::Compound(c) = data {
            Some(Self {
                nbt: Some(c.clone()),
            })
        } else {
            Some(Self { nbt: None })
        }
    }
}
impl DataComponentImpl for BucketEntityDataImpl {
    fn write_data(&self) -> NbtTag {
        NbtTag::Compound(self.nbt.clone().unwrap_or_default())
    }
    fn get_hash(&self) -> i32 {
        self.nbt
            .as_ref()
            .map_or_else(|| hash_ops::map(Vec::new()), hash_ops::nbt_compound) as i32
    }
    default_impl!(BucketEntityData);
}

#[derive(Clone)]
pub struct ContainerImpl {
    pub items: Vec<(u8, crate::item_stack::ItemStack)>,
}
impl PartialEq for ContainerImpl {
    fn eq(&self, _other: &Self) -> bool {
        false
    }
}
impl Eq for ContainerImpl {}
impl std::fmt::Debug for ContainerImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ContainerImpl")
    }
}
impl ContainerImpl {
    pub fn read_data(tag: &NbtTag) -> Option<Self> {
        let mut items = Vec::new();
        if let NbtTag::List(l) = tag {
            for item_tag in l {
                if let NbtTag::Compound(c) = item_tag
                    && let Some(slot) = c.get_numeric_int("slot").and_then(|slot| u8::try_from(slot).ok())
                    && let Some(item_compound) = c.get_compound("item")
                    && let Some(stack) =
                        crate::item_stack::ItemStack::read_item_stack(item_compound)
                {
                    items.push((slot, stack));
                }
            }
        }
        Some(Self { items })
    }

    /// `ItemContainerContents.fromSlots`: a later entry for a slot replaces an earlier one.
    /// Returns the filled slots in index order, which the client gets and hashes.
    #[must_use]
    pub fn filled_slots(&self) -> Vec<(u8, &crate::item_stack::ItemStack)> {
        let mut slots = vec![None; usize::from(u8::MAX) + 1];
        for (slot, stack) in &self.items {
            slots[usize::from(*slot)] = Some(stack);
        }
        slots
            .into_iter()
            .enumerate()
            .filter_map(|(slot, stack)| {
                Some((
                    u8::try_from(slot).ok()?,
                    stack.filter(|stack| !stack.is_empty())?,
                ))
            })
            .collect()
    }
}
impl DataComponentImpl for ContainerImpl {
    fn write_data(&self) -> NbtTag {
        let mut list = Vec::new();
        for (slot, stack) in &self.items {
            let Some(item_compound) = stack.to_nbt() else {
                continue;
            };
            let mut entry = NbtCompound::new();
            entry.put_int("slot", *slot as i32);
            entry.put_compound("item", item_compound);
            list.push(NbtTag::Compound(entry));
        }
        NbtTag::List(list)
    }
    /// `ItemContainerContents.asSlots`: the non-empty slots in index order.
    fn get_hash(&self) -> i32 {
        hash_ops::list(self.filled_slots().into_iter().map(|(slot, stack)| {
            hash_ops::MapHasher::new()
                .put("slot", hash_ops::int(i32::from(slot)))
                .put("item", hash_ops::item_stack_template(stack))
                .finish()
        })) as i32
    }
    default_impl!(Container);
}

use std::borrow::Cow;

#[derive(Clone, Debug)]
pub struct BlockStateImpl {
    pub properties: Cow<'static, [(Cow<'static, str>, Cow<'static, str>)]>,
}
impl PartialEq for BlockStateImpl {
    fn eq(&self, other: &Self) -> bool {
        let mut self_props = self.properties.to_vec();
        self_props.sort_by(|a, b| a.0.cmp(&b.0));
        let mut other_props = other.properties.to_vec();
        other_props.sort_by(|a, b| a.0.cmp(&b.0));
        self_props == other_props
    }
}
impl Eq for BlockStateImpl {}
impl std::hash::Hash for BlockStateImpl {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let mut props = self.properties.to_vec();
        props.sort_by(|a, b| a.0.cmp(&b.0));
        for (k, v) in props {
            k.hash(state);
            v.hash(state);
        }
    }
}
impl BlockStateImpl {
    pub fn read_data(data: &NbtTag) -> Option<Self> {
        let compound = data.extract_compound()?;
        let mut properties = Vec::new();
        for (key, val) in compound.child_tags.iter() {
            if let Some(s) = val.extract_string() {
                properties.push((Cow::Owned(key.to_string()), Cow::Owned(s.to_string())));
            }
        }
        Some(Self {
            properties: Cow::Owned(properties),
        })
    }
}
impl DataComponentImpl for BlockStateImpl {
    fn write_data(&self) -> NbtTag {
        let mut compound = NbtCompound::new();
        for (k, v) in self.properties.iter() {
            compound.put_string(k.as_ref(), v.to_string());
        }
        NbtTag::Compound(compound)
    }
    /// A map from the property name to its value.
    fn get_hash(&self) -> i32 {
        hash_ops::map(
            self.properties
                .iter()
                .map(|(name, value)| (hash_ops::string(name), hash_ops::string(value)))
                .collect(),
        ) as i32
    }
    default_impl!(BlockState);
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct BeesImpl {
    pub bees: Vec<NbtTag>,
}
#[allow(non_upper_case_globals)]
pub const BeesImpl: BeesImpl = BeesImpl { bees: Vec::new() };
impl BeesImpl {
    pub fn read_data(data: &NbtTag) -> Option<Self> {
        let bees = if let NbtTag::List(list) = data {
            list.clone()
        } else {
            Vec::new()
        };
        Some(Self { bees })
    }
}
impl DataComponentImpl for BeesImpl {
    fn write_data(&self) -> NbtTag {
        NbtTag::List(self.bees.clone())
    }
    /// Each occupant has the `CustomData` form of its entity and two ints.
    fn get_hash(&self) -> i32 {
        hash_ops::list(self.bees.iter().map(hash_ops::nbt)) as i32
    }
    default_impl!(Bees);
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct ContainerLootImpl {
    pub loot_table: String,
    pub seed: i64,
}
impl ContainerLootImpl {
    pub fn read_data(data: &NbtTag) -> Option<Self> {
        let compound = data.extract_compound()?;
        let loot_table = compound.get_string("loot_table")?.to_string();
        let seed = compound.get_numeric_long("seed").unwrap_or(0);
        Some(Self { loot_table, seed })
    }
}
impl DataComponentImpl for ContainerLootImpl {
    fn write_data(&self) -> NbtTag {
        let mut compound = NbtCompound::new();
        compound.put_string("loot_table", self.loot_table.clone());
        compound.put_long("seed", self.seed);
        NbtTag::Compound(compound)
    }
    fn get_hash(&self) -> i32 {
        let mut fields = hash_ops::MapHasher::new();
        fields.put(
            "loot_table",
            hash_ops::string(&crate::dynamic::namespaced_name(&self.loot_table)),
        );
        if self.seed != 0 {
            fields.put("seed", hash_ops::long(self.seed));
        }
        fields.finish() as i32
    }
    default_impl!(ContainerLoot);
}

#[derive(Clone)]
pub struct SulfurCubeContentImpl {
    pub item: crate::item_stack::ItemStack,
}
impl PartialEq for SulfurCubeContentImpl {
    fn eq(&self, other: &Self) -> bool {
        self.item.are_equal(&other.item)
    }
}
impl Eq for SulfurCubeContentImpl {}
impl std::fmt::Debug for SulfurCubeContentImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SulfurCubeContentImpl")
    }
}
impl SulfurCubeContentImpl {
    pub fn read_data(data: &NbtTag) -> Option<Self> {
        let compound = data.extract_compound()?;
        let item = crate::item_stack::ItemStack::read_item_stack(compound)?;
        Some(Self { item })
    }
}
impl DataComponentImpl for SulfurCubeContentImpl {
    fn write_data(&self) -> NbtTag {
        // Unsaveable item: empty compound, read back as no component.
        NbtTag::Compound(self.item.to_nbt().unwrap_or_default())
    }
    fn get_hash(&self) -> i32 {
        hash_ops::item_stack_template(&self.item) as i32
    }
    default_impl!(SulfurCubeContent);
}

fn read_sign_messages(list: &[NbtTag]) -> [Cow<'static, str>; 4] {
    std::array::from_fn(|i| {
        list.get(i)
            .and_then(|tag| match tag {
                NbtTag::String(s) => Some(Cow::Owned(s.to_string())),
                NbtTag::Compound(c) => c
                    .get_string("text")
                    .or_else(|| c.get_string("translate"))
                    .map(|s| Cow::Owned(s.to_string())),
                _ => None,
            })
            .unwrap_or(Cow::Borrowed(""))
    })
}

/// The hash of `SignText.CODEC`, which writes each line as a plain text.
fn sign_text_hash(
    messages: &[Cow<'static, str>; 4],
    filtered_messages: Option<&[Cow<'static, str>; 4]>,
    color: crate::dye_color::DyeColor,
    has_glowing_text: bool,
) -> u32 {
    let lines = |lines: &[Cow<'static, str>; 4]| {
        hash_ops::list(lines.iter().map(|line| hash_ops::string(line)))
    };
    let mut fields = hash_ops::MapHasher::new();
    fields.put("messages", lines(messages));
    if let Some(filtered) = filtered_messages.filter(|filtered| *filtered != messages) {
        fields.put("filtered_messages", lines(filtered));
    }
    fields
        .put("color", hash_ops::string(color.name()))
        .put("has_glowing_text", hash_ops::boolean(has_glowing_text))
        .finish()
}

fn write_sign_text(
    messages: &[Cow<'static, str>; 4],
    filtered_messages: Option<&[Cow<'static, str>; 4]>,
    color: crate::dye_color::DyeColor,
    has_glowing_text: bool,
) -> NbtTag {
    let mut compound = NbtCompound::new();
    compound.put_list(
        "messages",
        messages
            .iter()
            .map(|m| NbtTag::String(m.to_string().into()))
            .collect(),
    );
    // `SignText.CODEC` omits the filtered messages when they equal the messages.
    if let Some(filtered) = filtered_messages.filter(|filtered| *filtered != messages) {
        compound.put_list(
            "filtered_messages",
            filtered
                .iter()
                .map(|m| NbtTag::String(m.to_string().into()))
                .collect(),
        );
    }
    compound.put_string("color", color.name().to_string());
    compound.put_bool("has_glowing_text", has_glowing_text);
    NbtTag::Compound(compound)
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct SignTextFrontImpl {
    pub messages: [Cow<'static, str>; 4],
    pub filtered_messages: Option<[Cow<'static, str>; 4]>,
    pub color: crate::dye_color::DyeColor,
    pub has_glowing_text: bool,
}
impl Default for SignTextFrontImpl {
    fn default() -> Self {
        Self::DEFAULT
    }
}
impl SignTextFrontImpl {
    pub const DEFAULT: Self = Self {
        messages: [
            Cow::Borrowed(""),
            Cow::Borrowed(""),
            Cow::Borrowed(""),
            Cow::Borrowed(""),
        ],
        filtered_messages: None,
        color: crate::dye_color::DyeColor::Black,
        has_glowing_text: false,
    };

    pub fn read_data(data: &NbtTag) -> Option<Self> {
        let Some(compound) = data.extract_compound() else {
            return Some(Self::DEFAULT);
        };
        let messages = compound
            .get_list("messages")
            .map(|l| read_sign_messages(l))
            .unwrap_or(Self::DEFAULT.messages);
        let filtered_messages = compound
            .get_list("filtered_messages")
            .map(|l| read_sign_messages(l));
        let color = compound
            .get_string("color")
            .and_then(crate::dye_color::DyeColor::by_name)
            .unwrap_or(crate::dye_color::DyeColor::Black);
        let has_glowing_text = compound.get_bool("has_glowing_text").unwrap_or(false);
        Some(Self {
            messages,
            filtered_messages,
            color,
            has_glowing_text,
        })
    }
}
impl DataComponentImpl for SignTextFrontImpl {
    fn write_data(&self) -> NbtTag {
        write_sign_text(
            &self.messages,
            self.filtered_messages.as_ref(),
            self.color,
            self.has_glowing_text,
        )
    }
    /// `SignText.CODEC` always writes the color and the glow flag.
    fn get_hash(&self) -> i32 {
        sign_text_hash(
            &self.messages,
            self.filtered_messages.as_ref(),
            self.color,
            self.has_glowing_text,
        ) as i32
    }
    default_impl!(SignTextFront);
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct SignTextBackImpl {
    pub messages: [Cow<'static, str>; 4],
    pub filtered_messages: Option<[Cow<'static, str>; 4]>,
    pub color: crate::dye_color::DyeColor,
    pub has_glowing_text: bool,
}
impl Default for SignTextBackImpl {
    fn default() -> Self {
        Self::DEFAULT
    }
}
impl SignTextBackImpl {
    pub const DEFAULT: Self = Self {
        messages: [
            Cow::Borrowed(""),
            Cow::Borrowed(""),
            Cow::Borrowed(""),
            Cow::Borrowed(""),
        ],
        filtered_messages: None,
        color: crate::dye_color::DyeColor::Black,
        has_glowing_text: false,
    };

    pub fn read_data(data: &NbtTag) -> Option<Self> {
        let front = SignTextFrontImpl::read_data(data)?;
        Some(Self {
            messages: front.messages,
            filtered_messages: front.filtered_messages,
            color: front.color,
            has_glowing_text: front.has_glowing_text,
        })
    }
}
impl DataComponentImpl for SignTextBackImpl {
    fn write_data(&self) -> NbtTag {
        write_sign_text(
            &self.messages,
            self.filtered_messages.as_ref(),
            self.color,
            self.has_glowing_text,
        )
    }
    /// `SignText.CODEC` always writes the color and the glow flag.
    fn get_hash(&self) -> i32 {
        sign_text_hash(
            &self.messages,
            self.filtered_messages.as_ref(),
            self.color,
            self.has_glowing_text,
        ) as i32
    }
    default_impl!(SignTextBack);
}
