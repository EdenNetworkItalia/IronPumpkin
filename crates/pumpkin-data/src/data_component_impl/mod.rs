#![allow(dead_code)]

use crate::Block;
use crate::BlockId;
use crate::data_component::DataComponent;
use crate::entity_type::EntityType;
use crate::sound::Sound;
use crate::tag::Taggable;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use std::any::Any;
use std::borrow::Cow;

pub trait DataComponentImpl: Send + Sync {
    fn write_data(&self) -> NbtTag {
        NbtTag::End
    }
    fn get_hash(&self) -> i32 {
        0
    }
    /// make sure other is the same type component, or it will panic
    fn equal(&self, other: &dyn DataComponentImpl) -> bool;
    fn get_enum() -> DataComponent
    where
        Self: Sized;
    fn get_self_enum(&self) -> DataComponent; // only for debugging
    fn to_dyn(self) -> Box<dyn DataComponentImpl>;
    fn clone_dyn(&self) -> Box<dyn DataComponentImpl>;
    fn as_any(&self) -> &dyn Any;
    fn as_mut_any(&mut self) -> &mut dyn Any;
}

impl Clone for Box<dyn DataComponentImpl> {
    fn clone(&self) -> Self {
        self.clone_dyn()
    }
}

#[inline]
pub fn get<T: DataComponentImpl + 'static>(value: &dyn DataComponentImpl) -> &T {
    if value.get_self_enum() == T::get_enum() {
        // SAFETY: `DataComponent` variants have a 1-to-1 mapping to their `DataComponentImpl` type `T`.
        // Checking `get_self_enum()` instead of `TypeId` works across `cdylib` plugin boundaries.
        unsafe { &*(std::ptr::from_ref::<dyn DataComponentImpl>(value).cast::<T>()) }
    } else {
        panic!(
            "you are trying to cast {} ({}) to {} ({})",
            value.get_self_enum().to_name(),
            std::any::type_name_of_val(value),
            T::get_enum().to_name(),
            std::any::type_name::<T>()
        )
    }
}

#[inline]
pub fn get_mut<T: DataComponentImpl + 'static>(value: &mut dyn DataComponentImpl) -> &mut T {
    let name = value.get_self_enum().to_name();
    let val_type = std::any::type_name_of_val(value);
    if value.get_self_enum() == T::get_enum() {
        // SAFETY: `DataComponent` variants have a 1-to-1 mapping to their `DataComponentImpl` type `T`.
        // Checking `get_self_enum()` instead of `TypeId` works across `cdylib` plugin boundaries.
        unsafe { &mut *(std::ptr::from_mut::<dyn DataComponentImpl>(value).cast::<T>()) }
    } else {
        panic!(
            "you are trying to cast {name} ({val_type}) to {} ({})",
            T::get_enum().to_name(),
            std::any::type_name::<T>()
        )
    }
}

macro_rules! default_impl {
    ($t: ident) => {
        fn equal(&self, other: &dyn crate::data_component_impl::DataComponentImpl) -> bool {
            if other.get_self_enum() == Self::get_enum() {
                let other = unsafe {
                    &*(std::ptr::from_ref::<dyn crate::data_component_impl::DataComponentImpl>(
                        other,
                    )
                    .cast::<Self>())
                };
                self == other
            } else {
                false
            }
        }
        #[inline]
        fn get_enum() -> crate::data_component::DataComponent
        where
            Self: Sized,
        {
            crate::data_component::DataComponent::$t
        }
        fn get_self_enum(&self) -> crate::data_component::DataComponent {
            crate::data_component::DataComponent::$t
        }
        fn to_dyn(self) -> Box<dyn crate::data_component_impl::DataComponentImpl> {
            Box::new(self)
        }
        fn clone_dyn(&self) -> Box<dyn crate::data_component_impl::DataComponentImpl> {
            Box::new(self.clone())
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
            self
        }
    };
}

pub fn get_str_hash(val: &str) -> u32 {
    hash_ops::string(val)
}

pub fn get_i32_hash(val: i32) -> u32 {
    hash_ops::int(val)
}

pub fn get_f32_hash(val: f32) -> u32 {
    hash_ops::float(val)
}

pub fn put_idor(nbt: &mut NbtCompound, key: &str, val: &IdOr<basic::SoundEvent>) {
    match val {
        IdOr::Id(id) => nbt.put_string(key, format!("minecraft:{}", id.to_name())),
        IdOr::Value(sound) => {
            let mut sound_compound = NbtCompound::new();

            sound_compound.put_string("sound_id", sound.sound_name.clone());
            if let Some(range) = sound.range {
                sound_compound.put_float("range", range);
            }
            nbt.put(key, NbtTag::Compound(sound_compound));
        }
    }
}

pub fn get_idor(nbt: &NbtCompound, key: &str, default: Sound) -> IdOr<basic::SoundEvent> {
    if let Some(sound) = nbt.get_string(key) {
        let sound = sound.strip_prefix("minecraft:").unwrap_or(sound);
        IdOr::Id(Sound::from_name(sound).unwrap_or(default))
    } else if let Some(sound_compound) = nbt.get_compound(key) {
        let Some(sound_name) = sound_compound.get_string("sound_id") else {
            return IdOr::Id(default);
        };
        let range = sound_compound.get_numeric_float("range");
        IdOr::Value(basic::SoundEvent {
            sound_name: sound_name.to_string(),
            range,
        })
    } else {
        IdOr::Id(default)
    }
}

pub trait IDSetContent {
    /// The id a client in this mode knows.
    fn registry_id(&self, ids: crate::dynamic::ContentIds) -> u16;
    fn to_string(&self) -> String;
    fn from_id(id: u16) -> Option<&'static Self>;
    fn from_str(name: &str) -> Option<&'static Self>;
}

impl IDSetContent for Block {
    fn registry_id(&self, ids: crate::dynamic::ContentIds) -> u16 {
        self.to_java_network_id(ids)
    }

    fn from_id(id: u16) -> Option<&'static Self> {
        BlockId::new(id).map(Self::from_id)
    }

    fn from_str(name: &str) -> Option<&'static Self> {
        Block::from_name(name)
    }

    fn to_string(&self) -> String {
        self.name.to_string()
    }
}

impl IDSetContent for crate::item::Item {
    fn registry_id(&self, ids: crate::dynamic::ContentIds) -> u16 {
        self.to_java_network_id(ids)
    }

    fn from_id(id: u16) -> Option<&'static Self> {
        crate::item::Item::from_id(id)
    }

    fn from_str(name: &str) -> Option<&'static Self> {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        crate::item::Item::from_registry_key(name)
    }

    fn to_string(&self) -> String {
        self.registry_key.to_string()
    }
}

const DAMAGE_TYPES_BY_ID: [(crate::damage::DamageType, &str); 51] = [
    (crate::damage::DamageType::ARROW, "arrow"),
    (
        crate::damage::DamageType::BAD_RESPAWN_POINT,
        "bad_respawn_point",
    ),
    (crate::damage::DamageType::CACTUS, "cactus"),
    (crate::damage::DamageType::CAMPFIRE, "campfire"),
    (crate::damage::DamageType::CRAMMING, "cramming"),
    (crate::damage::DamageType::DRAGON_BREATH, "dragon_breath"),
    (crate::damage::DamageType::DROWN, "drown"),
    (crate::damage::DamageType::DRY_OUT, "dry_out"),
    (crate::damage::DamageType::ENDER_PEARL, "ender_pearl"),
    (crate::damage::DamageType::EXPLOSION, "explosion"),
    (crate::damage::DamageType::FALL, "fall"),
    (crate::damage::DamageType::FALLING_ANVIL, "falling_anvil"),
    (crate::damage::DamageType::FALLING_BLOCK, "falling_block"),
    (
        crate::damage::DamageType::FALLING_STALACTITE,
        "falling_stalactite",
    ),
    (crate::damage::DamageType::FIREBALL, "fireball"),
    (crate::damage::DamageType::FIREWORKS, "fireworks"),
    (crate::damage::DamageType::FLY_INTO_WALL, "fly_into_wall"),
    (crate::damage::DamageType::FREEZE, "freeze"),
    (crate::damage::DamageType::GENERIC, "generic"),
    (crate::damage::DamageType::GENERIC_KILL, "generic_kill"),
    (crate::damage::DamageType::HOT_FLOOR, "hot_floor"),
    (crate::damage::DamageType::IN_FIRE, "in_fire"),
    (crate::damage::DamageType::IN_WALL, "in_wall"),
    (crate::damage::DamageType::INDIRECT_MAGIC, "indirect_magic"),
    (crate::damage::DamageType::LAVA, "lava"),
    (crate::damage::DamageType::LIGHTNING_BOLT, "lightning_bolt"),
    (crate::damage::DamageType::MACE_SMASH, "mace_smash"),
    (crate::damage::DamageType::MAGIC, "magic"),
    (crate::damage::DamageType::MOB_ATTACK, "mob_attack"),
    (
        crate::damage::DamageType::MOB_ATTACK_NO_AGGRO,
        "mob_attack_no_aggro",
    ),
    (crate::damage::DamageType::MOB_PROJECTILE, "mob_projectile"),
    (crate::damage::DamageType::ON_FIRE, "on_fire"),
    (crate::damage::DamageType::OUT_OF_WORLD, "out_of_world"),
    (crate::damage::DamageType::OUTSIDE_BORDER, "outside_border"),
    (crate::damage::DamageType::PLAYER_ATTACK, "player_attack"),
    (
        crate::damage::DamageType::PLAYER_EXPLOSION,
        "player_explosion",
    ),
    (crate::damage::DamageType::SONIC_BOOM, "sonic_boom"),
    (crate::damage::DamageType::SPEAR, "spear"),
    (crate::damage::DamageType::SPIT, "spit"),
    (crate::damage::DamageType::STALAGMITE, "stalagmite"),
    (crate::damage::DamageType::STARVE, "starve"),
    (crate::damage::DamageType::STING, "sting"),
    (
        crate::damage::DamageType::SULFUR_CUBE_HOT,
        "sulfur_cube_hot",
    ),
    (
        crate::damage::DamageType::SWEET_BERRY_BUSH,
        "sweet_berry_bush",
    ),
    (crate::damage::DamageType::THORNS, "thorns"),
    (crate::damage::DamageType::THROWN, "thrown"),
    (crate::damage::DamageType::TRIDENT, "trident"),
    (
        crate::damage::DamageType::UNATTRIBUTED_FIREBALL,
        "unattributed_fireball",
    ),
    (crate::damage::DamageType::WIND_CHARGE, "wind_charge"),
    (crate::damage::DamageType::WITHER, "wither"),
    (crate::damage::DamageType::WITHER_SKULL, "wither_skull"),
];

impl IDSetContent for crate::damage::DamageType {
    // Damage types have no content-registry entries, so every client knows the same id.
    fn registry_id(&self, _ids: crate::dynamic::ContentIds) -> u16 {
        u16::from(self.id)
    }

    fn from_id(id: u16) -> Option<&'static Self> {
        DAMAGE_TYPES_BY_ID.get(usize::from(id)).map(|(dt, _)| dt)
    }

    fn from_str(name: &str) -> Option<&'static Self> {
        let key = name.strip_prefix("minecraft:").unwrap_or(name);
        DAMAGE_TYPES_BY_ID
            .iter()
            .find(|(_, k)| *k == key)
            .map(|(dt, _)| dt)
    }

    fn to_string(&self) -> String {
        DAMAGE_TYPES_BY_ID.get(usize::from(self.id)).map_or_else(
            || format!("minecraft:{}", self.message_id),
            |(_, key)| format!("minecraft:{key}"),
        )
    }
}

#[derive(Clone, Hash, PartialEq, Debug)]
pub enum IDSet<T: IDSetContent + 'static> {
    Tag(Cow<'static, str>),
    IDs(Cow<'static, [&'static T]>),
}

impl<T: IDSetContent + 'static> IDSet<T> {
    pub fn read(data: &NbtTag) -> Option<Self> {
        match data.clone() {
            NbtTag::String(tag) => {
                if tag.starts_with("#") {
                    Some(Self::Tag(Cow::Owned(tag.strip_prefix("#")?.to_string())))
                } else {
                    Some(Self::IDs(Cow::Owned([T::from_str(tag.as_ref())?].to_vec())))
                }
            }
            NbtTag::List(nbt_tags) => {
                let mut ids = Vec::<&T>::new();
                for nbt in nbt_tags {
                    if let NbtTag::String(id) = nbt
                        && let Some(instance) = T::from_str(id.as_ref())
                    {
                        ids.push(instance);
                    }
                }
                Some(Self::IDs(Cow::Owned(ids)))
            }
            _ => None,
        }
    }

    pub fn write(&self, compound: &mut NbtCompound, key: &str) {
        match self {
            Self::Tag(cow) => {
                let mut tag = cow.to_string();
                tag.insert(0, '#');
                compound.put_string(key, tag);
            }
            Self::IDs(arr) => {
                let id_vec: Vec<NbtTag> = arr
                    .iter()
                    .map(|x| NbtTag::String(x.to_string().into()))
                    .collect();
                compound.put_list(key, id_vec);
            }
        }
    }
}

impl IDSet<EntityType> {
    /// Membership by id or entity type tag.
    #[must_use]
    pub fn contains(&self, entity_type: &EntityType) -> bool {
        match self {
            Self::Tag(tag) => crate::dynamic::DynamicTaggable::has_tag_dynamic(entity_type, tag),
            Self::IDs(types) => types.iter().any(|t| t.id == entity_type.id),
        }
    }
}

impl IDSet<crate::damage::DamageType> {
    /// Membership by id or damage type tag.
    #[must_use]
    pub fn contains(&self, damage_type: &crate::damage::DamageType) -> bool {
        match self {
            Self::Tag(tag) => damage_type.is_tagged_with(tag).unwrap_or(false),
            Self::IDs(types) => types.iter().any(|t| t.id == damage_type.id),
        }
    }
}

impl IDSet<crate::item::Item> {
    /// Membership by id or item tag.
    #[must_use]
    pub fn contains(&self, item: &crate::item::Item) -> bool {
        match self {
            Self::Tag(tag) => crate::dynamic::DynamicTaggable::has_tag_dynamic(item, tag),
            Self::IDs(items) => items.iter().any(|i| i.id == item.id),
        }
    }
}

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub enum EquipmentType {
    Hand,
    HumanoidArmor,
    AnimalArmor,
    Saddle,
}

#[derive(Clone, Hash, Eq, PartialEq)]
pub struct EquipmentSlotData {
    pub slot_type: EquipmentType,
    pub entity_id: i32,
    pub max_count: i32,
    pub index: i32,
    pub name: Cow<'static, str>,
}

#[derive(Clone, Hash, Eq, PartialEq)]
#[repr(i8)]
pub enum EquipmentSlot {
    MainHand(EquipmentSlotData),
    OffHand(EquipmentSlotData),
    Feet(EquipmentSlotData),
    Legs(EquipmentSlotData),
    Chest(EquipmentSlotData),
    Head(EquipmentSlotData),
    Body(EquipmentSlotData),
    Saddle(EquipmentSlotData),
}

impl EquipmentSlot {
    pub const MAIN_HAND: Self = Self::MainHand(EquipmentSlotData {
        slot_type: EquipmentType::Hand,
        entity_id: 0,
        index: 0,
        max_count: 0,
        name: Cow::Borrowed("mainhand"),
    });
    pub const OFF_HAND: Self = Self::OffHand(EquipmentSlotData {
        slot_type: EquipmentType::Hand,
        entity_id: 1,
        index: 5,
        max_count: 0,
        name: Cow::Borrowed("offhand"),
    });
    pub const FEET: Self = Self::Feet(EquipmentSlotData {
        slot_type: EquipmentType::HumanoidArmor,
        entity_id: 0,
        index: 1,
        max_count: 1,
        name: Cow::Borrowed("feet"),
    });
    pub const LEGS: Self = Self::Legs(EquipmentSlotData {
        slot_type: EquipmentType::HumanoidArmor,
        entity_id: 1,
        index: 2,
        max_count: 1,
        name: Cow::Borrowed("legs"),
    });
    pub const CHEST: Self = Self::Chest(EquipmentSlotData {
        slot_type: EquipmentType::HumanoidArmor,
        entity_id: 2,
        index: 3,
        max_count: 1,
        name: Cow::Borrowed("chest"),
    });
    pub const HEAD: Self = Self::Head(EquipmentSlotData {
        slot_type: EquipmentType::HumanoidArmor,
        entity_id: 3,
        index: 4,
        max_count: 1,
        name: Cow::Borrowed("head"),
    });
    pub const BODY: Self = Self::Body(EquipmentSlotData {
        slot_type: EquipmentType::AnimalArmor,
        entity_id: 0,
        index: 6,
        max_count: 1,
        name: Cow::Borrowed("body"),
    });
    pub const SADDLE: Self = Self::Saddle(EquipmentSlotData {
        slot_type: EquipmentType::Saddle,
        entity_id: 0,
        index: 7,
        max_count: 1,
        name: Cow::Borrowed("saddle"),
    });

    #[must_use]
    pub const fn get_entity_slot_id(&self) -> i32 {
        match self {
            Self::MainHand(data) => data.entity_id,
            Self::OffHand(data) => data.entity_id,
            Self::Feet(data) => data.entity_id,
            Self::Legs(data) => data.entity_id,
            Self::Chest(data) => data.entity_id,
            Self::Head(data) => data.entity_id,
            Self::Body(data) => data.entity_id,
            Self::Saddle(data) => data.entity_id,
        }
    }

    #[must_use]
    pub const fn get_slot_index(&self) -> i32 {
        match self {
            Self::MainHand(data) => data.index,
            Self::OffHand(data) => data.index,
            Self::Feet(data) => data.index,
            Self::Legs(data) => data.index,
            Self::Chest(data) => data.index,
            Self::Head(data) => data.index,
            Self::Body(data) => data.index,
            Self::Saddle(data) => data.index,
        }
    }

    #[must_use]
    pub fn from_slot_index(name: i32) -> Option<&'static Self> {
        match name {
            0 => Some(&Self::MAIN_HAND),
            1 => Some(&Self::FEET),
            2 => Some(&Self::LEGS),
            3 => Some(&Self::CHEST),
            4 => Some(&Self::HEAD),
            5 => Some(&Self::OFF_HAND),
            6 => Some(&Self::BODY),
            7 => Some(&Self::SADDLE),
            _ => None,
        }
    }

    #[must_use]
    pub fn get_from_name(name: &str) -> Option<&'static Self> {
        match name {
            "mainhand" => Some(&Self::MAIN_HAND),
            "offhand" => Some(&Self::OFF_HAND),
            "feet" => Some(&Self::FEET),
            "legs" => Some(&Self::LEGS),
            "chest" => Some(&Self::CHEST),
            "head" => Some(&Self::HEAD),
            "body" => Some(&Self::BODY),
            "saddle" => Some(&Self::SADDLE),
            _ => None,
        }
    }

    #[must_use]
    pub fn to_name(&self) -> &Cow<'static, str> {
        match self {
            Self::MainHand(data) => &data.name,
            Self::OffHand(data) => &data.name,
            Self::Feet(data) => &data.name,
            Self::Legs(data) => &data.name,
            Self::Chest(data) => &data.name,
            Self::Head(data) => &data.name,
            Self::Body(data) => &data.name,
            Self::Saddle(data) => &data.name,
        }
    }

    #[must_use]
    pub const fn get_offset_entity_slot_id(&self, offset: i32) -> i32 {
        self.get_entity_slot_id() + offset
    }

    #[must_use]
    pub const fn slot_type(&self) -> EquipmentType {
        match self {
            Self::MainHand(data) => data.slot_type,
            Self::OffHand(data) => data.slot_type,
            Self::Feet(data) => data.slot_type,
            Self::Legs(data) => data.slot_type,
            Self::Chest(data) => data.slot_type,
            Self::Head(data) => data.slot_type,
            Self::Body(data) => data.slot_type,
            Self::Saddle(data) => data.slot_type,
        }
    }

    #[must_use]
    pub const fn is_armor_slot(&self) -> bool {
        matches!(
            self.slot_type(),
            EquipmentType::HumanoidArmor | EquipmentType::AnimalArmor
        )
    }

    #[must_use]
    pub const fn discriminant(&self) -> i8 {
        match self {
            Self::MainHand(_) => 0,
            Self::OffHand(_) => 1,
            Self::Feet(_) => 2,
            Self::Legs(_) => 3,
            Self::Chest(_) => 4,
            Self::Head(_) => 5,
            Self::Body(_) => 6,
            Self::Saddle(_) => 7,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum EntityTypeOrTag {
    Tag(&'static crate::tag::Tag),
    Single(&'static EntityType),
}

impl std::hash::Hash for EntityTypeOrTag {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Self::Tag(tag) => {
                for x in tag.0 {
                    x.hash(state);
                }
            }
            Self::Single(entity_type) => {
                entity_type.id.hash(state);
            }
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum IdOr<T> {
    Id(Sound),
    Value(T),
}

pub mod basic;
pub mod block_entity;
pub mod book;
pub mod combat;
pub mod entity_variant;
pub mod food;
pub mod hash_ops;
pub mod utility;

// Re-export all component implementations
pub use basic::*;
pub use block_entity::*;
pub use book::*;
pub use combat::*;
pub use entity_variant::*;
pub use food::*;
pub use utility::*;

#[must_use]
pub fn read_data(id: DataComponent, data: &NbtTag) -> Option<Box<dyn DataComponentImpl>> {
    match id {
        DataComponent::MaxStackSize => boxed(MaxStackSizeImpl::read_data, data),
        DataComponent::CustomData => boxed(CustomDataImpl::read_data, data),
        DataComponent::Enchantments => boxed(EnchantmentsImpl::read_data, data),
        DataComponent::Damage => boxed(DamageImpl::read_data, data),
        DataComponent::MaxDamage => boxed(MaxDamageImpl::read_data, data),
        DataComponent::Unbreakable => boxed(UnbreakableImpl::read_data, data),
        DataComponent::Food => boxed(FoodImpl::read_data, data),
        DataComponent::Tool => boxed(ToolImpl::read_data, data),
        DataComponent::Enchantable => boxed(EnchantableImpl::read_data, data),
        DataComponent::DamageResistant => boxed(DamageResistantImpl::read_data, data),
        DataComponent::PotionContents => boxed(PotionContentsImpl::read_data, data),
        DataComponent::PotionDurationScale => boxed(PotionDurationScaleImpl::read_data, data),
        DataComponent::SuspiciousStewEffects => boxed(SuspiciousStewEffectsImpl::read_data, data),
        DataComponent::Fireworks => boxed(FireworksImpl::read_data, data),
        DataComponent::FireworkExplosion => boxed(FireworkExplosionImpl::read_data, data),
        DataComponent::CustomName => boxed(CustomNameImpl::read_data, data),
        DataComponent::Lore => boxed(LoreImpl::read_data, data),
        DataComponent::ItemName => boxed(ItemNameImpl::read_data, data),
        DataComponent::ItemModel => boxed(ItemModelImpl::read_data, data),
        DataComponent::Consumable => boxed(ConsumableImpl::read_data, data),
        DataComponent::Equippable => boxed(EquippableImpl::read_data, data),
        DataComponent::AttackRange => boxed(AttackRangeImpl::read_data, data),
        DataComponent::KineticWeapon => boxed(KineticWeaponImpl::read_data, data),
        DataComponent::PiercingWeapon => boxed(PiercingWeaponImpl::read_data, data),
        DataComponent::MinimumAttackCharge => boxed(MinimumAttackChargeImpl::read_data, data),
        DataComponent::DamageType => boxed(DamageTypeImpl::read_data, data),
        DataComponent::StoredEnchantments => boxed(StoredEnchantmentsImpl::read_data, data),
        DataComponent::UseCooldown => boxed(UseCooldownImpl::read_data, data),
        DataComponent::RepairCost => boxed(RepairCostImpl::read_data, data),
        DataComponent::Repairable => boxed(RepairableImpl::read_data, data),
        DataComponent::MapId => boxed(MapIdImpl::read_data, data),
        DataComponent::MapPostProcessing => boxed(MapPostProcessingImpl::read_data, data),
        DataComponent::ChargedProjectiles => boxed(ChargedProjectilesImpl::read_data, data),
        DataComponent::BlockEntityData => boxed(BlockEntityDataImpl::read_data, data),
        DataComponent::BundleContents => boxed(BundleContentsImpl::read_data, data),
        DataComponent::Container => boxed(ContainerImpl::read_data, data),
        DataComponent::WrittenBookContent => boxed(WrittenBookContentImpl::read_data, data),
        DataComponent::WritableBookContent => boxed(WritableBookContentImpl::read_data, data),
        DataComponent::OminousBottleAmplifier => boxed(OminousBottleAmplifierImpl::read_data, data),
        DataComponent::BlockState => boxed(BlockStateImpl::read_data, data),
        DataComponent::Profile => boxed(ProfileImpl::read_data, data),
        DataComponent::ChickenVariant => boxed(ChickenVariantImpl::read_data, data),
        DataComponent::VillagerVariant => boxed(VillagerVariantImpl::read_data, data),
        DataComponent::WolfVariant => boxed(WolfVariantImpl::read_data, data),
        DataComponent::WolfSoundVariant => boxed(WolfSoundVariantImpl::read_data, data),
        DataComponent::WolfCollar => boxed(WolfCollarImpl::read_data, data),
        DataComponent::FoxVariant => boxed(FoxVariantImpl::read_data, data),
        DataComponent::SalmonSize => boxed(SalmonSizeImpl::read_data, data),
        DataComponent::ParrotVariant => boxed(ParrotVariantImpl::read_data, data),
        DataComponent::TropicalFishPattern => boxed(TropicalFishPatternImpl::read_data, data),
        DataComponent::TropicalFishBaseColor => boxed(TropicalFishBaseColorImpl::read_data, data),
        DataComponent::TropicalFishPatternColor => {
            boxed(TropicalFishPatternColorImpl::read_data, data)
        }
        DataComponent::MooshroomVariant => boxed(MooshroomVariantImpl::read_data, data),
        DataComponent::RabbitVariant => boxed(RabbitVariantImpl::read_data, data),
        DataComponent::PigVariant => boxed(PigVariantImpl::read_data, data),
        DataComponent::PigSoundVariant => boxed(PigSoundVariantImpl::read_data, data),
        DataComponent::CowVariant => boxed(CowVariantImpl::read_data, data),
        DataComponent::CowSoundVariant => boxed(CowSoundVariantImpl::read_data, data),
        DataComponent::ChickenSoundVariant => boxed(ChickenSoundVariantImpl::read_data, data),
        DataComponent::ZombieNautilusVariant => boxed(ZombieNautilusVariantImpl::read_data, data),
        DataComponent::FrogVariant => boxed(FrogVariantImpl::read_data, data),
        DataComponent::HorseVariant => boxed(HorseVariantImpl::read_data, data),
        DataComponent::PaintingVariant => boxed(PaintingVariantImpl::read_data, data),
        DataComponent::LlamaVariant => boxed(LlamaVariantImpl::read_data, data),
        DataComponent::AxolotlVariant => boxed(AxolotlVariantImpl::read_data, data),
        DataComponent::CatVariant => boxed(CatVariantImpl::read_data, data),
        DataComponent::CatSoundVariant => boxed(CatSoundVariantImpl::read_data, data),
        DataComponent::CatCollar => boxed(CatCollarImpl::read_data, data),
        DataComponent::SheepColor => boxed(SheepColorImpl::read_data, data),
        DataComponent::ShulkerColor => boxed(ShulkerColorImpl::read_data, data),
        DataComponent::DyedColor => boxed(DyedColorImpl::read_data, data),
        DataComponent::BaseColor => boxed(BaseColorImpl::read_data, data),
        DataComponent::NoteBlockSound => boxed(NoteBlockSoundImpl::read_data, data),
        DataComponent::TooltipStyle => boxed(TooltipStyleImpl::read_data, data),
        DataComponent::Lock => boxed(LockImpl::read_data, data),
        DataComponent::ContainerLoot => boxed(ContainerLootImpl::read_data, data),
        DataComponent::CustomModelData => boxed(CustomModelDataImpl::read_data, data),
        DataComponent::LodestoneTracker => boxed(LodestoneTrackerImpl::read_data, data),
        DataComponent::Glider => boxed(GliderImpl::read_data, data),
        DataComponent::IntangibleProjectile => boxed(IntangibleProjectileImpl::read_data, data),
        DataComponent::Trim => boxed(TrimImpl::read_data, data),
        DataComponent::CanPlaceOn => boxed(CanPlaceOnImpl::read_data, data),
        DataComponent::CanBreak => boxed(CanBreakImpl::read_data, data),
        DataComponent::AttackAnimation => boxed(SwingAnimationImpl::read_data, data),
        DataComponent::Rarity => boxed(RarityImpl::read_data, data),
        DataComponent::BannerPatterns => boxed(BannerPatternsImpl::read_data, data),
        DataComponent::UseEffects => boxed(UseEffectsImpl::read_data, data),
        DataComponent::UseRemainder => boxed(UseRemainderImpl::read_data, data),
        DataComponent::Weapon => boxed(WeaponImpl::read_data, data),
        DataComponent::TooltipDisplay => boxed(TooltipDisplayImpl::read_data, data),
        DataComponent::CreativeSlotLock => boxed(CreativeSlotLockImpl::read_data, data),
        DataComponent::EnchantmentGlintOverride => {
            boxed(EnchantmentGlintOverrideImpl::read_data, data)
        }
        DataComponent::DeathProtection => boxed(DeathProtectionImpl::read_data, data),
        DataComponent::BlocksAttacks => boxed(BlocksAttacksImpl::read_data, data),
        DataComponent::AdditionalTradeCost => boxed(AdditionalTradeCostImpl::read_data, data),
        DataComponent::Dye => boxed(DyeImpl::read_data, data),
        DataComponent::MapDecorations => boxed(MapDecorationsImpl::read_data, data),
        DataComponent::DebugStickState => boxed(DebugStickStateImpl::read_data, data),
        DataComponent::EntityData => boxed(EntityDataImpl::read_data, data),
        DataComponent::BucketEntityData => boxed(BucketEntityDataImpl::read_data, data),
        DataComponent::Instrument => boxed(InstrumentImpl::read_data, data),
        DataComponent::ProvidesTrimMaterial => boxed(ProvidesTrimMaterialImpl::read_data, data),
        DataComponent::JukeboxPlayable => boxed(JukeboxPlayableImpl::read_data, data),
        DataComponent::ProvidesBannerPatterns => boxed(ProvidesBannerPatternsImpl::read_data, data),
        DataComponent::Recipes => boxed(RecipesImpl::read_data, data),
        DataComponent::PotDecorations => boxed(PotDecorationsImpl::read_data, data),
        DataComponent::Bees => boxed(BeesImpl::read_data, data),
        DataComponent::SulfurCubeContent => boxed(SulfurCubeContentImpl::read_data, data),
        DataComponent::BreakSound => boxed(BreakSoundImpl::read_data, data),
        DataComponent::AttributeModifiers => boxed(AttributeModifiersImpl::read_data, data),
        DataComponent::BrewingFuel => boxed(BrewingFuelImpl::read_data, data),
        DataComponent::CookingFuel => boxed(CookingFuelImpl::read_data, data),
        DataComponent::Compostable => boxed(CompostableImpl::read_data, data),
        DataComponent::Waxed => boxed(WaxedImpl::read_data, data),
        DataComponent::InteractAnimation => boxed(InteractAnimationImpl::read_data, data),
        DataComponent::BlockTransformer => boxed(BlockTransformerImpl::read_data, data),
        DataComponent::VillagerFood => boxed(VillagerFoodImpl::read_data, data),
        DataComponent::MobVisibility => boxed(MobVisibilityImpl::read_data, data),
        DataComponent::ProvidesPotteryPattern => boxed(ProvidesPotteryPatternImpl::read_data, data),
        DataComponent::SignTextFront => boxed(SignTextFrontImpl::read_data, data),
        DataComponent::SignTextBack => boxed(SignTextBackImpl::read_data, data),
        DataComponent::CushionColor => boxed(CushionColorImpl::read_data, data),
    }
}
/// Loads one component into a box, in a frame of its own. A nested saved stack loads through
/// [`read_data`] once per level, so its frame must not hold a value of every component type.
#[inline(never)]
fn boxed<T: DataComponentImpl>(
    read: fn(&NbtTag) -> Option<T>,
    data: &NbtTag,
) -> Option<Box<dyn DataComponentImpl>> {
    Some(read(data)?.to_dyn())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash() {
        assert_eq!(get_str_hash("minecraft:sharpness"), 2734053906u32);
        assert_eq!(get_i32_hash(3), 3795317917u32);
        assert_eq!(
            EnchantmentsImpl {
                enchantment: Cow::Borrowed(&[(&crate::Enchantment::SHARPNESS, 2)]),
            }
            .get_hash(),
            -1580618251i32
        );
        assert_eq!(MaxStackSizeImpl { size: 99 }.get_hash(), -1632321551i32);
        assert_eq!(MapIdImpl { id: 10 }.get_hash(), -919192125i32);
    }

    fn assert_round_trip<T: DataComponentImpl + Clone + 'static>(
        value: T,
        read: impl Fn(&NbtTag) -> Option<T>,
    ) {
        let restored = read(&value.write_data()).expect("read_data returned None");
        assert!(value.equal(&restored));
    }

    #[test]
    fn max_damage_round_trip() {
        assert_round_trip(MaxDamageImpl { max_damage: 1561 }, MaxDamageImpl::read_data);
    }

    #[test]
    fn enchantable_round_trip() {
        assert_round_trip(EnchantableImpl { value: 14 }, EnchantableImpl::read_data);
    }

    #[test]
    fn food_round_trip() {
        assert_round_trip(
            FoodImpl {
                nutrition: 4,
                saturation: 2.4,
                can_always_eat: true,
            },
            FoodImpl::read_data,
        );
    }

    #[test]
    fn block_entity_data_round_trip() {
        let mut nbt = NbtCompound::new();
        nbt.put_string("id", "minecraft:chest".to_string());
        nbt.put_int("x", 12);
        assert_round_trip(BlockEntityDataImpl { nbt }, BlockEntityDataImpl::read_data);
    }

    #[test]
    fn entity_data_round_trip() {
        let mut nbt = NbtCompound::new();
        nbt.put_bool("PlayerCreated", true);
        assert_round_trip(
            EntityDataImpl {
                entity_type: Some(&EntityType::IRON_GOLEM),
                nbt: Some(nbt),
            },
            EntityDataImpl::read_data,
        );
        assert_round_trip(
            EntityDataImpl {
                entity_type: Some(&EntityType::CHICKEN),
                nbt: None,
            },
            EntityDataImpl::read_data,
        );
    }

    #[test]
    fn tool_round_trip() {
        assert_round_trip(
            ToolImpl {
                rules: Cow::Owned(vec![
                    ToolRule {
                        blocks: IDSet::Tag(Cow::Borrowed("mineable/pickaxe")),
                        speed: Some(6.0),
                        correct_for_drops: Some(true),
                    },
                    ToolRule {
                        blocks: IDSet::Tag(Cow::Borrowed("incorrect_for_wooden_tool")),
                        speed: None,
                        correct_for_drops: Some(false),
                    },
                ]),
                default_mining_speed: 1.0,
                damage_per_block: 2,
                can_destroy_blocks_in_creative: false,
            },
            ToolImpl::read_data,
        );
    }

    #[test]
    fn dyed_color_round_trip() {
        assert_round_trip(DyedColorImpl { rgb: 0x00A0F0 }, DyedColorImpl::read_data);
    }

    #[test]
    fn base_color_round_trip() {
        assert_round_trip(
            BaseColorImpl {
                color: "light_blue".to_string(),
            },
            BaseColorImpl::read_data,
        );
    }

    #[test]
    fn note_block_sound_round_trip() {
        assert_round_trip(
            NoteBlockSoundImpl {
                sound: "minecraft:block.note_block.pling".to_string(),
            },
            NoteBlockSoundImpl::read_data,
        );
    }

    #[test]
    fn tooltip_style_round_trip() {
        assert_round_trip(
            TooltipStyleImpl {
                id: "minecraft:my_style".to_string(),
            },
            TooltipStyleImpl::read_data,
        );
    }

    #[test]
    fn container_loot_round_trip() {
        assert_round_trip(
            ContainerLootImpl {
                loot_table: "minecraft:chests/simple_dungeon".to_string(),
                seed: 123_456_789,
            },
            ContainerLootImpl::read_data,
        );
    }

    #[test]
    fn custom_model_data_round_trip() {
        assert_round_trip(
            CustomModelDataImpl {
                floats: vec![1.5, -2.0],
                flags: vec![true, false, true],
                strings: vec!["a".to_string(), "b".to_string()],
                colors: vec![0xFF0000, 0x00FF00],
            },
            CustomModelDataImpl::read_data,
        );
    }

    #[test]
    fn lodestone_tracker_round_trip() {
        assert_round_trip(
            LodestoneTrackerImpl {
                target: Some(LodestoneTarget {
                    dimension: "minecraft:overworld".to_string(),
                    x: 10,
                    y: 64,
                    z: -30,
                }),
                tracked: true,
            },
            LodestoneTrackerImpl::read_data,
        );
        assert_round_trip(
            LodestoneTrackerImpl {
                target: None,
                tracked: false,
            },
            LodestoneTrackerImpl::read_data,
        );
    }

    #[test]
    fn marker_components_round_trip() {
        assert_round_trip(GliderImpl, GliderImpl::read_data);
        assert_round_trip(
            IntangibleProjectileImpl,
            IntangibleProjectileImpl::read_data,
        );
        assert_round_trip(BrewingFuelImpl::DEFAULT, BrewingFuelImpl::read_data);
        assert_round_trip(
            BrewingFuelImpl {
                uses: IntProvider::Inline(10),
                speed_multiplier: FloatProvider::Inline(2.0),
            },
            BrewingFuelImpl::read_data,
        );
        assert_round_trip(BlocksAttacksImpl::DEFAULT, BlocksAttacksImpl::read_data);
        assert_round_trip(
            DeathProtectionImpl {
                death_effects: Cow::Owned(vec![ConsumeEffect::ClearAllEffects]),
            },
            DeathProtectionImpl::read_data,
        );
        assert_round_trip(
            TooltipDisplayImpl {
                hide_tooltip: true,
                hidden_components: Cow::Owned(vec![DataComponent::Enchantments]),
            },
            TooltipDisplayImpl::read_data,
        );
        assert_round_trip(WaxedImpl, WaxedImpl::read_data);
        assert_round_trip(
            CookingFuelImpl {
                burn_time: IntProvider::Id(Cow::Borrowed("minecraft:cooking/time_coal")),
                speed_multiplier: FloatProvider::Id(Cow::Borrowed(
                    "minecraft:cooking/speed_default",
                )),
            },
            CookingFuelImpl::read_data,
        );
        assert_round_trip(
            CookingFuelImpl {
                burn_time: IntProvider::Inline(1600),
                speed_multiplier: FloatProvider::Inline(1.5),
            },
            CookingFuelImpl::read_data,
        );
    }

    #[test]
    fn raw_nbt_components_round_trip() {
        let mut material = NbtCompound::new();
        material.put_string("material", "minecraft:diamond".to_string());
        material.put_string("pattern", "minecraft:coast".to_string());
        assert_round_trip(
            TrimImpl::read_data(&NbtTag::Compound(material)).unwrap(),
            TrimImpl::read_data,
        );

        let mut predicate = NbtCompound::new();
        predicate.put_string("blocks", "minecraft:stone".to_string());
        assert_round_trip(
            CanPlaceOnImpl {
                predicate: NbtTag::Compound(predicate.clone()),
            },
            CanPlaceOnImpl::read_data,
        );
        assert_round_trip(LockImpl { predicate }, LockImpl::read_data);
    }

    #[test]
    fn equal_with_different_types_returns_false() {
        let enc = EnchantmentsImpl {
            enchantment: Cow::Borrowed(&[(&crate::Enchantment::SHARPNESS, 2)]),
        };
        let max_stack = MaxStackSizeImpl { size: 64 };
        assert!(!enc.equal(&max_stack));
    }

    #[test]
    fn enchantments_read_data_formats() {
        let mut direct = NbtCompound::new();
        direct.put_int("sharpness", 2);
        let enc1 = EnchantmentsImpl::read_data(&NbtTag::Compound(direct)).unwrap();
        assert!(enc1.enchantment[0].0 == &crate::Enchantment::SHARPNESS);
        assert_eq!(enc1.enchantment[0].1, 2);

        let mut levels = NbtCompound::new();
        levels.put_int("minecraft:sharpness", 2);
        let mut wrapped = NbtCompound::new();
        wrapped
            .child_tags
            .insert("levels".into(), NbtTag::Compound(levels));
        let enc2 = EnchantmentsImpl::read_data(&NbtTag::Compound(wrapped)).unwrap();
        assert_eq!(enc2.enchantment.len(), 1);
        assert!(enc2.enchantment[0].0 == &crate::Enchantment::SHARPNESS);
        assert_eq!(enc2.enchantment[0].1, 2);
    }

    #[test]
    fn swing_animation_round_trip() {
        assert_round_trip(
            SwingAnimationImpl {
                animation_type: SwingAnimationType::Stab,
                duration: 19,
            },
            SwingAnimationImpl::read_data,
        );
        assert_round_trip(SwingAnimationImpl::DEFAULT, SwingAnimationImpl::read_data);
    }

    #[test]
    fn generated_item_components_complete_for_26_3() {
        use crate::item::Item;
        use crate::item_stack::ItemStack;

        let cushion = ItemStack::new(1, &Item::ORANGE_CUSHION);
        assert_eq!(
            cushion
                .get_data_component::<CushionColorImpl>()
                .map(|c| c.color),
            Some(crate::dye_color::DyeColor::Orange)
        );

        let shield = ItemStack::new(1, &Item::SHIELD);
        let blocks = shield.get_data_component::<BlocksAttacksImpl>().unwrap();
        assert_eq!(blocks.block_delay_ticks(), 5);
        assert_eq!(blocks.item_damage.threshold, 3.0);
        assert_eq!(blocks.item_damage.apply(2.5), 0);
        assert_eq!(blocks.item_damage.apply(3.0), 4);
        assert!(
            blocks
                .bypassed_by
                .as_ref()
                .is_some_and(|set| set.contains(&crate::damage::DamageType::GENERIC_KILL))
        );

        let totem = ItemStack::new(1, &Item::TOTEM_OF_UNDYING);
        let dp = totem.get_data_component::<DeathProtectionImpl>().unwrap();
        assert_eq!(dp.death_effects.len(), 2);

        let blaze = ItemStack::new(1, &Item::BLAZE_POWDER);
        let fuel = blaze.get_data_component::<BrewingFuelImpl>().unwrap();
        assert_eq!(fuel.get_uses(), 20);
        assert_eq!(fuel.get_speed_multiplier(), 1.0);

        let egg = ItemStack::new(1, &Item::COW_SPAWN_EGG);
        assert_eq!(
            egg.get_data_component::<EntityDataImpl>()
                .and_then(EntityDataImpl::get_entity_type)
                .map(|e| e.id),
            Some(crate::entity_type::EntityType::COW.id)
        );

        let pumpkin = ItemStack::new(1, &Item::CARVED_PUMPKIN);
        let attrs = pumpkin
            .get_data_component::<AttributeModifiersImpl>()
            .unwrap();
        assert_eq!(attrs.attribute_modifiers.len(), 1);
        assert_eq!(
            attrs.attribute_modifiers[0].display,
            ModifierDisplay::Hidden
        );
        assert_round_trip(attrs.clone(), AttributeModifiersImpl::read_data);

        let stew = ItemStack::new(1, &Item::MUSHROOM_STEW);
        assert_eq!(
            stew.get_data_component::<UseRemainderImpl>()
                .and_then(|u| u.remainder.as_ref())
                .map(|s| s.item),
            Some(&Item::BOWL)
        );

        let sword = ItemStack::new(1, &Item::NETHERITE_SWORD);
        assert!(!sword.can_be_hurt_by(crate::damage::DamageType::LAVA));
        assert!(sword.can_be_hurt_by(crate::damage::DamageType::CACTUS));
    }

    #[test]
    fn lock_predicate_and_written_book_roundtrips() {
        use crate::item::Item;
        use crate::item_stack::ItemStack;
        use pumpkin_util::text::TextComponent;

        let mut comp_pred = NbtCompound::new();
        comp_pred.put_string("minecraft:custom_name", "Secret Key".to_string());
        let mut count_bounds = NbtCompound::new();
        count_bounds.put_int("min", 2);
        count_bounds.put_int("max", 4);
        let mut pred = NbtCompound::new();
        pred.put_string("items", "minecraft:tripwire_hook".to_string());
        pred.put_compound("count", count_bounds);
        pred.put_compound("components", comp_pred);
        let lock = LockImpl { predicate: pred };

        assert!(!lock.unlocks_with(&ItemStack::EMPTY));

        let mut key = ItemStack::new(2, &Item::TRIPWIRE_HOOK);
        assert!(!lock.unlocks_with(&key));
        key.set_data_component(CustomNameImpl {
            name: TextComponent::text("Secret Key"),
        });
        assert!(lock.unlocks_with(&key));

        let mut wrong_count = key.clone();
        wrong_count.item_count = 1;
        assert!(!lock.unlocks_with(&wrong_count));

        let wrong_item = ItemStack::new_with_component(
            2,
            &Item::STICK,
            vec![(
                DataComponent::CustomName,
                Some(Box::new(CustomNameImpl {
                    name: TextComponent::text("Secret Key"),
                })),
            )],
        );
        assert!(!lock.unlocks_with(&wrong_item));

        let legacy = LockImpl::read_data(&NbtTag::String("Vault".into())).unwrap();
        let mut legacy_key = ItemStack::new(1, &Item::STICK);
        assert!(!legacy.unlocks_with(&legacy_key));
        legacy_key.set_data_component(CustomNameImpl {
            name: TextComponent::text("Vault"),
        });
        assert!(legacy.unlocks_with(&legacy_key));

        assert_round_trip(
            WrittenBookContentImpl {
                title: "Chronicle".to_string(),
                author: "Alex".to_string(),
                generation: 2,
                pages: vec![TextComponent::text("Page 1")],
                resolved: true,
            },
            WrittenBookContentImpl::read_data,
        );
        assert_round_trip(
            JukeboxPlayableImpl {
                song: "minecraft:precipice",
            },
            JukeboxPlayableImpl::read_data,
        );
    }
}
