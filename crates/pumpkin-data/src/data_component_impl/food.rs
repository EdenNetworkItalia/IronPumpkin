use super::Digest;
use crate::data_component_impl::basic::SoundEvent;
use crate::data_component_impl::{
    DataComponentImpl, IDSet, IdOr, get_f32_hash, get_i32_hash, get_idor, get_idor_hash,
    get_idset_hash, get_str_hash, hash_ops, put_idor,
};
use crate::effect::StatusEffect;
use crate::item::Item;
use crate::item_stack::ItemStack;
use crate::sound::Sound;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use std::borrow::Cow;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

#[derive(Clone, Debug, PartialEq)]
pub struct FoodImpl {
    pub nutrition: i32,
    pub saturation: f32,
    pub can_always_eat: bool,
}
impl FoodImpl {
    pub fn read_data(data: &NbtTag) -> Option<Self> {
        let compound = data.extract_compound()?;
        let nutrition = compound.get_int("nutrition")?;
        let saturation = compound.get_float("saturation")?;
        let can_always_eat = compound.get_bool("can_always_eat").unwrap_or(false);
        Some(Self {
            nutrition,
            saturation,
            can_always_eat,
        })
    }
}
impl DataComponentImpl for FoodImpl {
    fn write_data(&self) -> NbtTag {
        let mut compound = NbtCompound::new();
        compound.put_int("nutrition", self.nutrition);
        compound.put_float("saturation", self.saturation);
        compound.put_bool("can_always_eat", self.can_always_eat);
        NbtTag::Compound(compound)
    }
    fn get_hash(&self) -> i32 {
        let mut fields = hash_ops::MapHasher::new();
        fields
            .put("nutrition", hash_ops::int(self.nutrition))
            .put("saturation", hash_ops::float(self.saturation));
        if self.can_always_eat {
            fields.put("can_always_eat", hash_ops::boolean(true));
        }
        fields.finish() as i32
    }
    default_impl!(Food);
}
impl Hash for FoodImpl {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.nutrition.hash(state);
        unsafe { (*(&raw const self.saturation).cast::<u32>()).hash(state) };
        self.can_always_eat.hash(state);
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct StatusEffectInstance {
    pub effect_id: Cow<'static, str>,
    pub amplifier: i32,
    pub duration: i32,
    pub ambient: bool,
    pub show_particles: bool,
    pub show_icon: bool,
}

impl StatusEffectInstance {
    /// The hash of `MobEffectInstance.CODEC`. `show_icon` is always written.
    #[must_use]
    pub fn codec_hash(&self) -> u32 {
        let mut fields = hash_ops::MapHasher::new();
        fields.put("id", hash_ops::id(&self.effect_id));
        if self.amplifier != 0 {
            // `ExtraCodecs.UNSIGNED_BYTE` encodes a byte.
            fields.put("amplifier", hash_ops::byte(self.amplifier as i8));
        }
        if self.duration != 0 {
            fields.put("duration", hash_ops::int(self.duration));
        }
        if self.ambient {
            fields.put("ambient", hash_ops::boolean(true));
        }
        if !self.show_particles {
            fields.put("show_particles", hash_ops::boolean(false));
        }
        fields.put("show_icon", hash_ops::boolean(self.show_icon));
        fields.finish()
    }

    /// Reads `MobEffectInstance.CODEC`. The amplifier is an `ExtraCodecs.UNSIGNED_BYTE`; older
    /// IronPumpkin saves wrote it as an int.
    pub fn read_data(nbt: &NbtTag) -> Option<Self> {
        let compound = nbt.extract_compound()?;
        let effect_id = Cow::Owned(compound.get_string("id")?.to_string());
        let amplifier = compound
            .get_byte("amplifier")
            .map(|amplifier| i32::from(amplifier as u8))
            .or_else(|| compound.get_int("amplifier"))
            .unwrap_or(0);
        let duration = compound.get_int("duration").unwrap_or(0);
        let ambient = compound.get_bool("ambient").unwrap_or(false);
        let show_particles = compound.get_bool("show_particles").unwrap_or(true);
        let show_icon = compound.get_bool("show_icon").unwrap_or(show_particles);
        Some(Self {
            effect_id,
            amplifier,
            duration,
            ambient,
            show_particles,
            show_icon,
        })
    }

    /// Writes `MobEffectInstance.CODEC`: the fields that equal their default are omitted,
    /// `show_icon` is always written.
    pub fn as_nbt(&self) -> NbtTag {
        let mut compound = NbtCompound::new();
        compound.put_string("id", self.effect_id.to_string());
        if self.amplifier != 0 {
            compound.put_byte("amplifier", self.amplifier as i8);
        }
        if self.duration != 0 {
            compound.put_int("duration", self.duration);
        }
        if self.ambient {
            compound.put_bool("ambient", true);
        }
        if !self.show_particles {
            compound.put_bool("show_particles", false);
        }
        compound.put_bool("show_icon", self.show_icon);
        NbtTag::Compound(compound)
    }
}

/// Vanilla `ItemUseAnimation`: the discriminant is the network id, the name is the codec name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConsumeAnimation {
    None = 0,
    Eat = 1,
    Drink = 2,
    Block = 3,
    Bow = 4,
    Trident = 5,
    Crossbow = 6,
    Spyglass = 7,
    TootHorn = 8,
    Brush = 9,
    Bundle = 10,
    Spear = 11,
}

impl ConsumeAnimation {
    const ALL: [Self; 12] = [
        Self::None,
        Self::Eat,
        Self::Drink,
        Self::Block,
        Self::Bow,
        Self::Trident,
        Self::Crossbow,
        Self::Spyglass,
        Self::TootHorn,
        Self::Brush,
        Self::Bundle,
        Self::Spear,
    ];

    #[must_use]
    pub const fn to_str(&self) -> &'static str {
        match self {
            ConsumeAnimation::None => "none",
            ConsumeAnimation::Eat => "eat",
            ConsumeAnimation::Drink => "drink",
            ConsumeAnimation::Block => "block",
            ConsumeAnimation::Bow => "bow",
            ConsumeAnimation::Trident => "trident",
            ConsumeAnimation::Crossbow => "crossbow",
            ConsumeAnimation::Spyglass => "spyglass",
            ConsumeAnimation::TootHorn => "toot_horn",
            ConsumeAnimation::Brush => "brush",
            ConsumeAnimation::Bundle => "bundle",
            ConsumeAnimation::Spear => "spear",
        }
    }
}
impl TryFrom<i32> for ConsumeAnimation {
    type Error = ();
    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Self::ALL
            .into_iter()
            .find(|animation| *animation as i32 == value)
            .ok_or(())
    }
}
impl FromStr for ConsumeAnimation {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|animation| animation.to_str() == s)
            .ok_or(())
    }
}

#[cfg(test)]
mod consume_animation_tests {
    use super::ConsumeAnimation;
    use std::str::FromStr;

    /// The ids and names of vanilla `ItemUseAnimation`.
    #[test]
    fn consume_animation_matches_item_use_animation() {
        let vanilla = [
            "none",
            "eat",
            "drink",
            "block",
            "bow",
            "trident",
            "crossbow",
            "spyglass",
            "toot_horn",
            "brush",
            "bundle",
            "spear",
        ];
        for (id, name) in vanilla.into_iter().enumerate() {
            let animation = ConsumeAnimation::try_from(id as i32).unwrap();
            assert_eq!(animation.to_str(), name);
            assert_eq!(animation as i32, id as i32);
            assert_eq!(ConsumeAnimation::from_str(name), Ok(animation));
        }
        assert!(ConsumeAnimation::try_from(12).is_err());
        assert!(ConsumeAnimation::from_str("horn").is_err());
    }
}

/// The hash of `ConsumeEffect.CODEC`: the fields of the effect type and its `type` id.
#[must_use]
pub fn consume_effect_hash(effect: &ConsumeEffect) -> u32 {
    let mut fields = hash_ops::MapHasher::new();
    fields.put(
        "type",
        hash_ops::string(&format!("minecraft:{}", effect.to_str())),
    );
    match effect {
        ConsumeEffect::ApplyEffects((effects, probability)) => {
            fields.put(
                "effects",
                hash_ops::list(effects.iter().map(StatusEffectInstance::codec_hash)),
            );
            if !hash_ops::same_float(*probability, 1.0) {
                fields.put("probability", hash_ops::float(*probability));
            }
        }
        ConsumeEffect::RemoveEffects(effects) => {
            fields.put("effects", hash_ops::holder_set(effects));
        }
        ConsumeEffect::ClearAllEffects => {}
        ConsumeEffect::TeleportRandomly(diameter) => {
            if !hash_ops::same_float(*diameter, 16.0) {
                fields.put("diameter", hash_ops::float(*diameter));
            }
        }
        ConsumeEffect::PlaySound(sound) => {
            fields.put("sound", hash_ops::sound(sound));
        }
    }
    fields.finish()
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConsumeEffect {
    ApplyEffects((Cow<'static, [StatusEffectInstance]>, f32)),
    RemoveEffects(IDSet<StatusEffect>),
    ClearAllEffects,
    TeleportRandomly(f32),
    PlaySound(IdOr<SoundEvent>),
}
impl Hash for ConsumeEffect {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.to_str().hash(state);
        match self {
            ConsumeEffect::ApplyEffects(tuple) => {
                tuple.0.hash(state);
                unsafe { (*(&raw const tuple.1).cast::<u32>()).hash(state) };
            }
            ConsumeEffect::RemoveEffects(status_effect_instances) => {
                status_effect_instances.hash(state)
            }
            ConsumeEffect::ClearAllEffects => (),
            ConsumeEffect::TeleportRandomly(dst) => unsafe {
                (*(&raw const dst).cast::<u32>()).hash(state)
            },
            ConsumeEffect::PlaySound(id_or) => id_or.hash(state),
        }
    }
}
impl ConsumeEffect {
    pub fn to_str(&self) -> &str {
        match self {
            ConsumeEffect::ApplyEffects(_) => "apply_effects",
            ConsumeEffect::RemoveEffects(_) => "remove_effects",
            ConsumeEffect::ClearAllEffects => "clear_all_effects",
            ConsumeEffect::TeleportRandomly(_) => "teleport_randomly",
            ConsumeEffect::PlaySound(_) => "play_sound",
        }
    }
    pub fn registry_id(&self) -> u8 {
        match self {
            ConsumeEffect::ApplyEffects(_) => 0,
            ConsumeEffect::RemoveEffects(_) => 1,
            ConsumeEffect::ClearAllEffects => 2,
            ConsumeEffect::TeleportRandomly(_) => 3,
            ConsumeEffect::PlaySound(_) => 4,
        }
    }
    /// Reads `ConsumeEffect.CODEC`. Vanilla saves the type as a namespaced id and omits the
    /// fields that equal their default.
    pub fn read_data(nbt: &NbtTag) -> Option<Self> {
        let compound = nbt.extract_compound()?;
        let r#type = compound.get_string("type")?;
        match r#type.strip_prefix("minecraft:").unwrap_or(r#type) {
            "remove_effects" => {
                let idset = IDSet::read(compound.get("effects")?)?;
                Some(Self::RemoveEffects(idset))
            }
            "clear_all_effects" => Some(Self::ClearAllEffects),
            "teleport_randomly" => {
                let dst = compound.get_float("diameter").unwrap_or(16.0);
                Some(Self::TeleportRandomly(dst))
            }
            "play_sound" => {
                let sound = get_idor(compound, "sound", Sound::EntityGenericEat);
                Some(Self::PlaySound(sound))
            }
            "apply_effects" => {
                let probability = compound.get_float("probability").unwrap_or(1.0);
                let effects_vec: Vec<StatusEffectInstance> = compound
                    .get_list("effects")?
                    .iter()
                    .filter_map(StatusEffectInstance::read_data)
                    .collect();
                let effects: Cow<'static, [StatusEffectInstance]> = Cow::Owned(effects_vec);
                Some(Self::ApplyEffects((effects, probability)))
            }
            _ => None,
        }
    }
    pub fn as_nbt(&self) -> NbtTag {
        let mut compound = NbtCompound::new();
        compound.put_string("type", self.to_str().to_string());
        match self {
            ConsumeEffect::ApplyEffects(data) => {
                let nbt_arr = data.0.iter().map(|x| x.as_nbt()).collect();
                compound.put_list("effects", nbt_arr);
                compound.put_float("probability", data.1);
            }
            ConsumeEffect::RemoveEffects(idset) => idset.write(&mut compound, "effects"),
            ConsumeEffect::ClearAllEffects => (),
            ConsumeEffect::TeleportRandomly(dst) => compound.put_float("diameter", *dst),
            ConsumeEffect::PlaySound(id_or) => {
                put_idor(&mut compound, "sound", id_or);
            }
        }
        NbtTag::Compound(compound)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConsumableImpl {
    pub consume_seconds: f32,
    pub animation: ConsumeAnimation,
    pub sound_event: IdOr<SoundEvent>,
    pub consume_particles: bool,
    pub effects: Cow<'static, [ConsumeEffect]>,
}
impl ConsumableImpl {
    pub const fn new(
        consume_seconds: f32,
        animation: ConsumeAnimation,
        sound_event: IdOr<SoundEvent>,
        consume_particles: bool,
        effects: Cow<'static, [ConsumeEffect]>,
    ) -> Self {
        Self {
            consume_seconds,
            animation,
            sound_event,
            consume_particles,
            effects,
        }
    }
    #[must_use]
    pub fn consume_ticks(&self) -> i32 {
        (self.consume_seconds * 20.0) as i32
    }

    #[must_use]
    pub fn should_emit_particles_and_sounds(&self, use_item_remaining_ticks: i32) -> bool {
        let item_used_for_ticks = self.consume_ticks() - use_item_remaining_ticks;
        let wait_ticks_before_use_effects = (self.consume_ticks() as f32 * 0.218_75) as i32;
        item_used_for_ticks > wait_ticks_before_use_effects && use_item_remaining_ticks % 4 == 0
    }
    pub fn read_data(data: &NbtTag) -> Option<Self> {
        let compound = data.extract_compound()?;
        let consume_seconds = compound.get_float("consume_seconds")?;
        let animation = compound
            .get_string("animation")?
            .parse::<ConsumeAnimation>()
            .ok()?;
        let sound_event = get_idor(compound, "sound", Sound::EntityGenericEat);
        let consume_particles = compound.get_bool("has_consume_particles").unwrap_or(false);
        let opt_list = compound.get_list("on_consume_effects");
        let effects: Cow<'static, [ConsumeEffect]> = if let Some(effect_list) = opt_list {
            effect_list
                .iter()
                .filter_map(ConsumeEffect::read_data)
                .collect()
        } else {
            Cow::Borrowed(&[])
        };
        Some(Self {
            consume_seconds,
            animation,
            sound_event,
            consume_particles,
            effects,
        })
    }
}
impl DataComponentImpl for ConsumableImpl {
    fn write_data(&self) -> NbtTag {
        let mut compound = NbtCompound::new();
        compound.put_float("consume_seconds", self.consume_seconds);
        compound.put_string("animation", self.animation.to_str().to_string());
        put_idor(&mut compound, "sound", &self.sound_event);
        compound.put_bool("has_consume_particles", self.consume_particles);
        let nbt_vec = self.effects.iter().map(|x| x.as_nbt()).collect();
        compound.put_list("on_consume_effects", nbt_vec);
        NbtTag::Compound(compound)
    }
    fn get_hash(&self) -> i32 {
        let mut fields = hash_ops::MapHasher::new();
        if !hash_ops::same_float(self.consume_seconds, 1.6) {
            fields.put("consume_seconds", hash_ops::float(self.consume_seconds));
        }
        if self.animation != ConsumeAnimation::Eat {
            fields.put("animation", hash_ops::string(self.animation.to_str()));
        }
        if !hash_ops::is_registry_sound(&self.sound_event, "entity.generic.eat") {
            fields.put("sound", hash_ops::sound(&self.sound_event));
        }
        if !self.consume_particles {
            fields.put("has_consume_particles", hash_ops::boolean(false));
        }
        if !self.effects.is_empty() {
            fields.put(
                "on_consume_effects",
                hash_ops::list(self.effects.iter().map(consume_effect_hash)),
            );
        }
        fields.finish() as i32
    }
    default_impl!(Consumable);
}
impl Hash for ConsumableImpl {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        unsafe { (*(&raw const self.consume_seconds).cast::<u32>()).hash(state) };
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UseEffectsImpl {
    pub can_sprint: bool,
    pub interact_vibrations: bool,
    pub speed_multiplier: f32,
}
impl Default for UseEffectsImpl {
    fn default() -> Self {
        Self::DEFAULT
    }
}
impl Eq for UseEffectsImpl {}
impl Hash for UseEffectsImpl {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.can_sprint.hash(state);
        self.interact_vibrations.hash(state);
        self.speed_multiplier.to_bits().hash(state);
    }
}
impl UseEffectsImpl {
    pub const DEFAULT: Self = Self {
        can_sprint: false,
        interact_vibrations: true,
        speed_multiplier: 0.2,
    };

    pub fn read_data(data: &NbtTag) -> Option<Self> {
        let Some(compound) = data.extract_compound() else {
            return Some(Self::DEFAULT);
        };
        Some(Self {
            can_sprint: compound.get_bool("can_sprint").unwrap_or(false),
            interact_vibrations: compound.get_bool("interact_vibrations").unwrap_or(true),
            speed_multiplier: compound.get_float("speed_multiplier").unwrap_or(0.2),
        })
    }
}
impl DataComponentImpl for UseEffectsImpl {
    fn write_data(&self) -> NbtTag {
        let mut compound = NbtCompound::new();
        compound.put_bool("can_sprint", self.can_sprint);
        compound.put_bool("interact_vibrations", self.interact_vibrations);
        compound.put_float("speed_multiplier", self.speed_multiplier);
        NbtTag::Compound(compound)
    }
    /// The defaults are `UseEffects.DEFAULT`.
    fn get_hash(&self) -> i32 {
        let mut fields = hash_ops::MapHasher::new();
        if self.can_sprint {
            fields.put("can_sprint", hash_ops::boolean(true));
        }
        if !self.interact_vibrations {
            fields.put("interact_vibrations", hash_ops::boolean(false));
        }
        if !hash_ops::same_float(self.speed_multiplier, 0.2) {
            fields.put("speed_multiplier", hash_ops::float(self.speed_multiplier));
        }
        fields.finish() as i32
    }
    default_impl!(UseEffects);
}

#[derive(Clone)]
pub struct UseRemainderImpl {
    pub remainder: Option<ItemStack>,
}
impl std::fmt::Debug for UseRemainderImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UseRemainderImpl")
            .field(
                "remainder",
                &self.remainder.as_ref().map(|s| s.item.registry_key),
            )
            .finish()
    }
}
impl PartialEq for UseRemainderImpl {
    fn eq(&self, other: &Self) -> bool {
        match (&self.remainder, &other.remainder) {
            (None, None) => true,
            (Some(a), Some(b)) => {
                a.item_count == b.item_count && a.are_items_and_components_equal(b)
            }
            _ => false,
        }
    }
}
impl Eq for UseRemainderImpl {}
impl UseRemainderImpl {
    pub fn read_data(data: &NbtTag) -> Option<Self> {
        Some(Self {
            remainder: Some(ItemStack::read_item_stack_template(data)?),
        })
    }

    /// Mirrors vanilla's `UseRemainder.convertIntoRemainder`.
    pub fn convert_into_remainder(
        &self,
        used_stack: ItemStack,
        stack_count_before_using: u8,
        has_infinite_materials: bool,
        on_extra_created_remainder: impl FnOnce(ItemStack),
    ) -> ItemStack {
        if has_infinite_materials || used_stack.item_count >= stack_count_before_using {
            return used_stack;
        }
        let Some(remainder_stack) = self.remainder.clone() else {
            return used_stack;
        };
        if used_stack.is_empty() {
            return remainder_stack;
        }
        on_extra_created_remainder(remainder_stack);
        used_stack
    }
}
impl DataComponentImpl for UseRemainderImpl {
    fn write_data(&self) -> NbtTag {
        self.remainder.as_ref().map_or_else(
            || NbtTag::Compound(NbtCompound::new()),
            ItemStack::write_item_stack_template,
        )
    }
    fn get_hash(&self) -> i32 {
        self.remainder.as_ref().map_or(0, |remainder| {
            hash_ops::item_stack_template(remainder) as i32
        })
    }
    default_impl!(UseRemainder);
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct VillagerFoodImpl {
    pub nutrition: i32,
}
impl VillagerFoodImpl {
    pub fn read_data(data: &NbtTag) -> Option<Self> {
        let nutrition = match data {
            NbtTag::Int(n) => *n,
            NbtTag::Compound(c) => c.get_int("nutrition")?,
            _ => return None,
        };
        Some(Self { nutrition })
    }
}
impl DataComponentImpl for VillagerFoodImpl {
    fn write_data(&self) -> NbtTag {
        let mut compound = NbtCompound::new();
        compound.put_int("nutrition", self.nutrition);
        NbtTag::Compound(compound)
    }
    fn get_hash(&self) -> i32 {
        hash_ops::MapHasher::new()
            .put("nutrition", hash_ops::int(self.nutrition))
            .finish() as i32
    }
    default_impl!(VillagerFood);
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UseCooldownImpl {
    pub seconds: f32,
    pub cooldown_group: Option<String>,
}
impl UseCooldownImpl {
    pub fn new(seconds: f32, cooldown_group: Option<String>) -> Self {
        Self {
            seconds,
            cooldown_group,
        }
    }
    pub fn read_data(data: &NbtTag) -> Option<Self> {
        let compound = data.extract_compound()?;
        let seconds = compound.get_float("seconds")?;
        let cooldown_group = compound.get_string("cooldown_group").map(|s| s.to_string());
        Some(Self {
            seconds,
            cooldown_group,
        })
    }
}
impl DataComponentImpl for UseCooldownImpl {
    fn write_data(&self) -> NbtTag {
        let mut compound = NbtCompound::new();
        compound.put_float("seconds", self.seconds);
        if let Some(group) = &self.cooldown_group {
            compound.put_string("cooldown_group", group.clone());
        }
        NbtTag::Compound(compound)
    }
    fn get_hash(&self) -> i32 {
        let mut fields = hash_ops::MapHasher::new();
        fields.put("seconds", hash_ops::float(self.seconds));
        if let Some(group) = &self.cooldown_group {
            fields.put("cooldown_group", hash_ops::id(group));
        }
        fields.finish() as i32
    }
    default_impl!(UseCooldown);
}
impl Eq for UseCooldownImpl {}
impl Hash for UseCooldownImpl {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.seconds.to_bits().hash(state);
        self.cooldown_group.hash(state);
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct PotionContentsImpl {
    pub potion_id: Option<i32>,
    pub custom_color: Option<i32>,
    pub custom_effects: Vec<StatusEffectInstance>,
    pub custom_name: Option<String>,
}
impl PotionContentsImpl {
    /// Reads `PotionContents.CODEC`: the full map or a bare potion id. Older IronPumpkin saves
    /// wrote the potion as its registry index.
    pub fn read_data(tag: &NbtTag) -> Option<Self> {
        let potion_by_name = |name: &str| {
            let name = name.strip_prefix("minecraft:").unwrap_or(name);
            crate::potion::Potion::from_name(name).map(|potion| i32::from(potion.id))
        };
        if let Some(name) = tag.extract_string() {
            return Some(Self {
                potion_id: Some(potion_by_name(name)?),
                custom_color: None,
                custom_effects: Vec::new(),
                custom_name: None,
            });
        }
        let compound = tag.extract_compound()?;
        let potion_id = compound
            .get_int("potion")
            .or_else(|| compound.get_string("potion").and_then(potion_by_name));
        let custom_effects = compound
            .get_list("custom_effects")
            .map(|list| {
                list.iter()
                    .filter_map(StatusEffectInstance::read_data)
                    .collect()
            })
            .unwrap_or_default();
        Some(Self {
            potion_id,
            custom_color: compound.get_int("custom_color"),
            custom_effects,
            custom_name: compound.get_string("custom_name").map(str::to_owned),
        })
    }

    fn potion(&self) -> Option<&'static crate::potion::Potion> {
        self.potion_id
            .and_then(|id| u8::try_from(id).ok())
            .and_then(crate::potion::Potion::from_id)
    }
}
impl DataComponentImpl for PotionContentsImpl {
    fn write_data(&self) -> NbtTag {
        let mut compound = NbtCompound::new();
        if let Some(potion) = self.potion() {
            compound.put_string("potion", format!("minecraft:{}", potion.name));
        }
        if let Some(color) = self.custom_color {
            compound.put_int("custom_color", color);
        }
        if !self.custom_effects.is_empty() {
            compound.put_list(
                "custom_effects",
                self.custom_effects
                    .iter()
                    .map(StatusEffectInstance::as_nbt)
                    .collect(),
            );
        }
        if let Some(name) = &self.custom_name {
            compound.put_string("custom_name", name.clone());
        }
        NbtTag::Compound(compound)
    }
    /// `PotionContents.CODEC` encodes with its full record codec.
    fn get_hash(&self) -> i32 {
        let mut fields = hash_ops::MapHasher::new();
        if let Some(potion) = self.potion() {
            fields.put("potion", hash_ops::id(potion.name));
        }
        if let Some(color) = self.custom_color {
            fields.put("custom_color", hash_ops::int(color));
        }
        if !self.custom_effects.is_empty() {
            fields.put(
                "custom_effects",
                hash_ops::list(
                    self.custom_effects
                        .iter()
                        .map(StatusEffectInstance::codec_hash),
                ),
            );
        }
        if let Some(name) = &self.custom_name {
            fields.put("custom_name", hash_ops::string(name));
        }
        fields.finish() as i32
    }
    default_impl!(PotionContents);
}

#[derive(Clone, Debug, PartialEq)]
pub struct PotionDurationScaleImpl {
    pub scale: f32,
}
impl PotionDurationScaleImpl {
    pub fn read_data(data: &NbtTag) -> Option<Self> {
        data.extract_float().map(|scale| Self { scale })
    }
}
impl DataComponentImpl for PotionDurationScaleImpl {
    fn write_data(&self) -> NbtTag {
        NbtTag::Float(self.scale)
    }
    fn get_hash(&self) -> i32 {
        get_f32_hash(self.scale) as i32
    }
    default_impl!(PotionDurationScale);
}
impl Hash for PotionDurationScaleImpl {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.scale.to_bits().hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::{DataComponentImpl, PotionDurationScaleImpl};
    use crate::item::Item;

    /// `PotionContents.CODEC` saves the potion as its id and each effect as
    /// `MobEffectInstance.CODEC`, which omits the default fields and writes the amplifier as a
    /// byte.
    #[test]
    fn potion_contents_save_the_vanilla_shape_and_load_it_back() {
        use super::{PotionContentsImpl, StatusEffectInstance};
        use pumpkin_nbt::compound::NbtCompound;
        use pumpkin_nbt::tag::NbtTag;
        use std::borrow::Cow;

        let potion = PotionContentsImpl {
            potion_id: Some(i32::from(crate::potion::Potion::SWIFTNESS.id)),
            custom_color: Some(0x00FF_0000),
            custom_effects: vec![StatusEffectInstance {
                effect_id: Cow::Borrowed("minecraft:speed"),
                amplifier: 200,
                duration: 0,
                ambient: false,
                show_particles: true,
                show_icon: true,
            }],
            custom_name: Some("swift".to_owned()),
        };
        let mut effect = NbtCompound::new();
        effect.put_string("id", "minecraft:speed".to_owned());
        effect.put_byte("amplifier", 200u8 as i8);
        effect.put_bool("show_icon", true);
        let mut expected = NbtCompound::new();
        expected.put_string("potion", "minecraft:swiftness".to_owned());
        expected.put_int("custom_color", 0x00FF_0000);
        expected.put_list("custom_effects", vec![NbtTag::Compound(effect)]);
        expected.put_string("custom_name", "swift".to_owned());

        let saved = potion.write_data();
        assert_eq!(saved, NbtTag::Compound(expected));
        assert_eq!(PotionContentsImpl::read_data(&saved), Some(potion));
    }

    #[test]
    fn potion_contents_load_the_id_shorthand_and_old_int_saves() {
        use super::PotionContentsImpl;
        use pumpkin_nbt::compound::NbtCompound;
        use pumpkin_nbt::tag::NbtTag;

        let swiftness = Some(i32::from(crate::potion::Potion::SWIFTNESS.id));
        let shorthand = NbtTag::String("minecraft:swiftness".into());
        let loaded = PotionContentsImpl::read_data(&shorthand).expect("shorthand loads");
        assert_eq!(loaded.potion_id, swiftness);
        assert!(loaded.custom_effects.is_empty());

        // An older IronPumpkin save: the potion index and every effect field as an int.
        let mut effect = NbtCompound::new();
        effect.put_string("id", "minecraft:speed".to_owned());
        effect.put_int("amplifier", 1);
        effect.put_int("duration", 200);
        effect.put_byte("ambient", 0);
        effect.put_byte("show_particles", 1);
        effect.put_byte("show_icon", 1);
        let mut old = NbtCompound::new();
        old.put_int("potion", swiftness.unwrap());
        old.put_list("custom_effects", vec![NbtTag::Compound(effect)]);
        let loaded = PotionContentsImpl::read_data(&NbtTag::Compound(old)).expect("old save loads");
        assert_eq!(loaded.potion_id, swiftness);
        assert_eq!(loaded.custom_effects[0].amplifier, 1);
        assert_eq!(loaded.custom_effects[0].duration, 200);

        // Vanilla omits `show_icon`: it then follows `show_particles`.
        let mut hidden = NbtCompound::new();
        hidden.put_string("id", "minecraft:speed".to_owned());
        hidden.put_bool("show_particles", false);
        let effect = super::StatusEffectInstance::read_data(&NbtTag::Compound(hidden)).unwrap();
        assert_eq!((effect.amplifier, effect.duration), (0, 0));
        assert!(!effect.show_icon);
    }

    /// `ApplyStatusEffectsConsumeEffect` omits `probability` at 1.0 and
    /// `TeleportRandomlyConsumeEffect` omits `diameter` at 16.0.
    #[test]
    fn consume_effects_load_the_vanilla_shape_without_default_fields() {
        use super::ConsumeEffect;
        use pumpkin_nbt::compound::NbtCompound;
        use pumpkin_nbt::tag::NbtTag;

        let mut effect = NbtCompound::new();
        effect.put_string("id", "minecraft:speed".to_owned());
        effect.put_bool("show_icon", true);
        let mut apply = NbtCompound::new();
        apply.put_string("type", "minecraft:apply_effects".to_owned());
        apply.put_list("effects", vec![NbtTag::Compound(effect)]);
        let Some(ConsumeEffect::ApplyEffects((effects, probability))) =
            ConsumeEffect::read_data(&NbtTag::Compound(apply))
        else {
            panic!("apply_effects loads");
        };
        assert_eq!(probability, 1.0);
        assert_eq!(effects[0].effect_id, "minecraft:speed");

        let mut teleport = NbtCompound::new();
        teleport.put_string("type", "minecraft:teleport_randomly".to_owned());
        assert_eq!(
            ConsumeEffect::read_data(&NbtTag::Compound(teleport)),
            Some(ConsumeEffect::TeleportRandomly(16.0))
        );
    }

    #[test]
    fn potion_duration_scale_round_trips_as_a_float() {
        let scale = PotionDurationScaleImpl { scale: 0.125 };
        let encoded = scale.write_data();
        let decoded = PotionDurationScaleImpl::read_data(&encoded).expect("scale should decode");

        assert_eq!(decoded, scale);
    }

    #[test]
    fn generated_arrow_duration_scale_is_data_driven() {
        let scale = Item::TIPPED_ARROW
            .components
            .iter()
            .find_map(|(id, component)| {
                (*id == crate::data_component::DataComponent::PotionDurationScale)
                    .then(|| component.as_any().downcast_ref::<PotionDurationScaleImpl>())
                    .flatten()
            })
            .expect("tipped arrows should have a duration scale");

        assert_eq!(scale.scale, 0.125);
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct SuspiciousStewEffect {
    pub effect: Cow<'static, str>,
    pub duration: i32,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct SuspiciousStewEffectsImpl {
    pub effects: Cow<'static, [SuspiciousStewEffect]>,
}
impl SuspiciousStewEffectsImpl {
    pub const EMPTY: Self = Self {
        effects: Cow::Borrowed(&[]),
    };

    pub fn read_data(data: &NbtTag) -> Option<Self> {
        Some(Self {
            effects: Cow::Owned(
                data.extract_list()?
                    .iter()
                    .filter_map(|tag| {
                        let effect = tag.extract_compound()?;
                        Some(SuspiciousStewEffect {
                            effect: Cow::Owned(effect.get_string("id")?.to_owned()),
                            duration: effect.get_int("duration").unwrap_or(160),
                        })
                    })
                    .collect(),
            ),
        })
    }
}
impl DataComponentImpl for SuspiciousStewEffectsImpl {
    fn write_data(&self) -> NbtTag {
        NbtTag::List(
            self.effects
                .iter()
                .map(|effect| {
                    let mut nbt = NbtCompound::new();
                    nbt.put_string("id", effect.effect.to_string());
                    nbt.put_int("duration", effect.duration);
                    NbtTag::Compound(nbt)
                })
                .collect(),
        )
    }
    fn get_hash(&self) -> i32 {
        hash_ops::list(self.effects.iter().map(|effect| {
            let mut fields = hash_ops::MapHasher::new();
            fields.put("id", hash_ops::id(&effect.effect));
            if effect.duration != 160 {
                fields.put("duration", hash_ops::int(effect.duration));
            }
            fields.finish()
        })) as i32
    }
    default_impl!(SuspiciousStewEffects);
}
