//! The data map types of `NeoForge` and their value types.
//!
//! `NeoForge`: `net.neoforged.neoforge.registries.datamaps.builtin`. The server registers these
//! types, so data packs and mods can give their values. `Decode` reads both forms of a value that
//! `NeoForge` reads (`Codec.withAlternative`); `Encode` writes the form of the network codec of
//! the map. A block transform is kept as its JSON object.

use std::{collections::BTreeMap, sync::LazyLock};

use pumpkin_codecs::{DataResult, Decode, DynamicOps, Encode, json_ops::JsonOps};
use pumpkin_data::dynamic::ContentKind;
use pumpkin_util::identifier::Identifier;
use serde_json::{Map, Value, json};

use super::{
    AdvancedDataMapType, DataMapSource, DataMapType, DataMapValueRemover, ListMerger, MapMerger,
    RegisterDataMapTypesEvent, WrappedMerger,
};

/// A value in its JSON form.
trait JsonForm: Sized {
    fn from_json(json: &Value) -> Result<Self, String>;
    fn to_json(&self) -> Value;
}

macro_rules! json_codec {
    ($($ty:ty),* $(,)?) => {$(
        impl Encode for $ty {
            fn encode<O: DynamicOps>(&self, ops: &'static O, _prefix: O::Value) -> DataResult<O::Value> {
                DataResult::new_success(JsonOps.convert_to(ops, self.to_json()))
            }
        }

        impl Decode for $ty {
            fn decode<O: DynamicOps>(input: O::Value, ops: &'static O) -> DataResult<(Self, O::Value)> {
                match Self::from_json(&ops.convert_to(&JsonOps, input)) {
                    Ok(value) => DataResult::new_success((value, ops.empty())),
                    Err(message) => DataResult::new_error(message),
                }
            }
        }
    )*};
}

json_codec!(
    AcceptableVillagerDistance,
    BiomeVillagerType,
    BlockTransformAppender,
    BlockTransformData,
    MonsterRoomMob,
    Oxidizable,
    ParrotImitation,
    RaidHeroGift,
    Transformable,
    TransformableRemover,
    VibrationFrequency,
    VillagerCompostable,
    Waxable,
);

/// `Codec.withAlternative(record with one field, field codec)`: the value of `field` in an
/// object, or the value itself. The record ignores the keys it does not read.
fn one_field<'a>(json: &'a Value, field: &str) -> &'a Value {
    match json {
        Value::Object(object) => object.get(field).unwrap_or(json),
        _ => json,
    }
}

fn identifier(json: &Value) -> Result<Identifier, String> {
    let text = json
        .as_str()
        .ok_or_else(|| format!("Not a string: {json}"))?;
    Identifier::parse(text)
        .map_err(|error| format!("Not a valid resource location: {text} {error}"))
}

/// `Registry.byNameCodec()`: the id of an entry of a built-in registry.
fn registry_entry(json: &Value, kind: ContentKind, registry: &str) -> Result<Identifier, String> {
    let id = identifier(json)?;
    if crate::registry::contains(kind, &id.to_string()) {
        Ok(id)
    } else {
        Err(format!(
            "Unknown registry key in ResourceKey[minecraft:root / minecraft:{registry}]: {id}"
        ))
    }
}

/// `Codec.INT`: any JSON number, through `Number.intValue`, so a fraction is cut off and a
/// number past the `int` range keeps its low 32 bits.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "Number.intValue narrows as Java does"
)]
fn int(json: &Value) -> Result<i32, String> {
    let Value::Number(number) = json else {
        return Err(format!("Not a number: {json}"));
    };
    let wide = number
        .as_i64()
        .or_else(|| number.as_u64().map(|value| value as i64))
        .unwrap_or_else(|| number.as_f64().map_or(0, |value| value.trunc() as i64));
    Ok(wide as i32)
}

/// `Codec.intRange(min, max)`.
fn int_in(json: &Value, min: i32, max: i32) -> Result<i32, String> {
    let value = int(json)?;
    if value < min || value > max {
        return Err(format!("Value must be within range [{min};{max}]: {value}"));
    }
    Ok(value)
}

/// `optionalFieldOf(name, true)`.
fn flag(object: &Map<String, Value>, name: &str, default: bool) -> Result<bool, String> {
    object.get(name).map_or(Ok(default), |value| {
        value
            .as_bool()
            .ok_or_else(|| format!("Not a boolean: {value}"))
    })
}

fn object(json: &Value) -> Result<&Map<String, Value>, String> {
    json.as_object().ok_or_else(|| format!("Not a map: {json}"))
}

/// `AcceptableVillagerDistance`: how close a villager must be to the entity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AcceptableVillagerDistance {
    pub distance: f32,
}

impl JsonForm for AcceptableVillagerDistance {
    fn from_json(json: &Value) -> Result<Self, String> {
        let distance = one_field(json, "acceptable_villager_distance")
            .as_f64()
            .ok_or_else(|| format!("Not a number: {json}"))?;
        #[expect(clippy::cast_possible_truncation, reason = "Codec.FLOAT reads a float")]
        Ok(Self {
            distance: distance as f32,
        })
    }

    /// `DISTANCE_CODEC`.
    fn to_json(&self) -> Value {
        json!(self.distance)
    }
}

/// `BiomeVillagerType`: the villager type of a biome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BiomeVillagerType {
    pub villager_type: Identifier,
}

impl JsonForm for BiomeVillagerType {
    fn from_json(json: &Value) -> Result<Self, String> {
        Ok(Self {
            villager_type: identifier(one_field(json, "villager_type"))?,
        })
    }

    /// `TYPE_CODEC`.
    fn to_json(&self) -> Value {
        json!(self.villager_type.to_string())
    }
}

/// `BlockTransformer.BlockTransformData`, kept as its JSON object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockTransformData(pub Map<String, Value>);

impl JsonForm for BlockTransformData {
    fn from_json(json: &Value) -> Result<Self, String> {
        super::block_transform::check(json)?;
        Ok(Self(object(json)?.clone()))
    }

    fn to_json(&self) -> Value {
        Value::Object(self.0.clone())
    }
}

/// `BlockTransformAppender`: transforms that a block transformer gets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockTransformAppender {
    pub entries: Vec<BlockTransformData>,
}

impl JsonForm for BlockTransformAppender {
    fn from_json(json: &Value) -> Result<Self, String> {
        let entries = json
            .as_array()
            .ok_or_else(|| format!("Not a list: {json}"))?
            .iter()
            .map(BlockTransformData::from_json)
            .collect::<Result<_, _>>()?;
        Ok(Self { entries })
    }

    fn to_json(&self) -> Value {
        Value::Array(self.entries.iter().map(JsonForm::to_json).collect())
    }
}

/// `MonsterRoomMob`: the weight of an entity type in a monster room spawner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonsterRoomMob {
    pub weight: i32,
}

impl JsonForm for MonsterRoomMob {
    fn from_json(json: &Value) -> Result<Self, String> {
        Ok(Self {
            weight: int_in(one_field(json, "weight"), 0, i32::MAX)?,
        })
    }

    /// `WEIGHT_CODEC`.
    fn to_json(&self) -> Value {
        json!(self.weight)
    }
}

/// `Oxidizable`: the next oxidation stage of a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Oxidizable {
    pub next_oxidation_stage: Identifier,
    pub generate_block_transform: bool,
}

impl JsonForm for Oxidizable {
    fn from_json(json: &Value) -> Result<Self, String> {
        let fields = object(json)?;
        Ok(Self {
            next_oxidation_stage: registry_entry(
                fields
                    .get("next_oxidation_stage")
                    .ok_or("No key next_oxidation_stage in MapLike")?,
                ContentKind::Block,
                "block",
            )?,
            generate_block_transform: flag(fields, "generate_block_transform", true)?,
        })
    }

    fn to_json(&self) -> Value {
        let mut fields = Map::new();
        fields.insert(
            "next_oxidation_stage".to_owned(),
            json!(self.next_oxidation_stage.to_string()),
        );
        if !self.generate_block_transform {
            fields.insert("generate_block_transform".to_owned(), json!(false));
        }
        Value::Object(fields)
    }
}

/// `ParrotImitation`: the sound a parrot plays to imitate the entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParrotImitation {
    pub sound: Identifier,
}

impl JsonForm for ParrotImitation {
    fn from_json(json: &Value) -> Result<Self, String> {
        Ok(Self {
            sound: registry_entry(
                one_field(json, "sound"),
                ContentKind::SoundEvent,
                "sound_event",
            )?,
        })
    }

    /// `SOUND_CODEC`.
    fn to_json(&self) -> Value {
        json!(self.sound.to_string())
    }
}

/// `RaidHeroGift`: the loot table a villager of a profession gives a hero of the village.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaidHeroGift {
    pub loot_table: Identifier,
}

impl JsonForm for RaidHeroGift {
    fn from_json(json: &Value) -> Result<Self, String> {
        Ok(Self {
            loot_table: identifier(one_field(json, "loot_table"))?,
        })
    }

    /// `LOOT_TABLE_CODEC`.
    fn to_json(&self) -> Value {
        json!(self.loot_table.to_string())
    }
}

/// `Transformable`: the transforms of one block, per block transformer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transformable {
    pub transformers: BTreeMap<Identifier, BlockTransformData>,
}

impl JsonForm for Transformable {
    fn from_json(json: &Value) -> Result<Self, String> {
        let fields = object(json)?;
        if let (Some(transformer), Some(data)) =
            (fields.get("transformer"), fields.get("transform_data"))
            && let (Ok(transformer), Ok(data)) =
                (identifier(transformer), BlockTransformData::from_json(data))
        {
            return Ok(Self {
                transformers: BTreeMap::from([(transformer, data)]),
            });
        }
        let transformers = fields
            .get("transformers")
            .ok_or("No key transformers in MapLike")
            .map_err(str::to_owned)
            .and_then(object)?
            .iter()
            .map(|(key, data)| {
                Ok((
                    Identifier::parse(key)
                        .map_err(|error| format!("Not a valid resource location: {key} {error}"))?,
                    BlockTransformData::from_json(data)?,
                ))
            })
            .collect::<Result<_, String>>()?;
        Ok(Self { transformers })
    }

    /// `Transformable.pack`: one transformer in the short form.
    fn to_json(&self) -> Value {
        if self.transformers.len() == 1
            && let Some((transformer, data)) = self.transformers.iter().next()
        {
            return json!({
                "transformer": transformer.to_string(),
                "transform_data": data.to_json(),
            });
        }
        let map: Map<String, Value> = self
            .transformers
            .iter()
            .map(|(key, data)| (key.to_string(), data.to_json()))
            .collect();
        json!({ "transformers": map })
    }
}

/// `Transformable.Remover`: removes the transform of one block transformer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformableRemover {
    pub to_remove: Identifier,
}

impl JsonForm for TransformableRemover {
    fn from_json(json: &Value) -> Result<Self, String> {
        Ok(Self {
            to_remove: identifier(json)?,
        })
    }

    fn to_json(&self) -> Value {
        json!(self.to_remove.to_string())
    }
}

impl DataMapValueRemover<Transformable> for TransformableRemover {
    fn remove(
        &self,
        value: &Transformable,
        _: &Identifier,
        _: &DataMapSource,
        _: &Identifier,
    ) -> Option<Transformable> {
        if !value.transformers.contains_key(&self.to_remove) {
            return Some(value.clone());
        }
        let mut transformers = value.transformers.clone();
        transformers.remove(&self.to_remove);
        (!transformers.is_empty()).then_some(Transformable { transformers })
    }
}

/// `VibrationFrequency`: the frequency of a game event for sculk sensors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VibrationFrequency {
    pub frequency: i32,
}

impl JsonForm for VibrationFrequency {
    fn from_json(json: &Value) -> Result<Self, String> {
        Ok(Self {
            frequency: int_in(one_field(json, "frequency"), 1, 15)?,
        })
    }

    /// `FREQUENCY_CODEC`.
    fn to_json(&self) -> Value {
        json!(self.frequency)
    }
}

/// `VillagerCompostable`: whether a farmer villager can compost the item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VillagerCompostable {
    pub can_villager_compost: bool,
}

impl JsonForm for VillagerCompostable {
    fn from_json(json: &Value) -> Result<Self, String> {
        let can_villager_compost = match json {
            Value::Bool(value) => *value,
            Value::Object(fields) => flag(fields, "can_villager_compost", false)?,
            _ => return Err(format!("Not a boolean: {json}")),
        };
        Ok(Self {
            can_villager_compost,
        })
    }

    fn to_json(&self) -> Value {
        if self.can_villager_compost {
            json!({ "can_villager_compost": true })
        } else {
            json!({})
        }
    }
}

/// `Waxable`: the waxed form of a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waxable {
    pub waxed: Identifier,
    pub generate_block_transform: bool,
}

impl JsonForm for Waxable {
    fn from_json(json: &Value) -> Result<Self, String> {
        let fields = object(json)?;
        Ok(Self {
            waxed: registry_entry(
                fields.get("waxed").ok_or("No key waxed in MapLike")?,
                ContentKind::Block,
                "block",
            )?,
            generate_block_transform: flag(fields, "generate_block_transform", true)?,
        })
    }

    fn to_json(&self) -> Value {
        let mut fields = Map::new();
        fields.insert("waxed".to_owned(), json!(self.waxed.to_string()));
        if !self.generate_block_transform {
            fields.insert("generate_block_transform".to_owned(), json!(false));
        }
        Value::Object(fields)
    }
}

const fn neoforge(path: &'static str) -> Identifier {
    Identifier::from_static("neoforge", path)
}

const fn vanilla(path: &'static str) -> Identifier {
    Identifier::vanilla_static(path)
}

const fn appender_entries(appender: &BlockTransformAppender) -> &Vec<BlockTransformData> {
    &appender.entries
}

const fn appender_from_entries(entries: Vec<BlockTransformData>) -> BlockTransformAppender {
    BlockTransformAppender { entries }
}

const fn transformable_transformers(
    transformable: &Transformable,
) -> &BTreeMap<Identifier, BlockTransformData> {
    &transformable.transformers
}

const fn transformable_from_transformers(
    transformers: BTreeMap<Identifier, BlockTransformData>,
) -> Transformable {
    Transformable { transformers }
}

/// The reason of `TRANSFORMABLES` to refuse tag targets.
const SINGLE_BLOCK_REASON: &str = "A Transformable and its BlockTransformData must only target and attach to a single block to prevent duplicate transforms and support datamap removers";

/// `NeoForgeDataMaps.ACCEPTABLE_VILLAGER_DISTANCES`.
pub static ACCEPTABLE_VILLAGER_DISTANCES: LazyLock<DataMapType<AcceptableVillagerDistance>> =
    LazyLock::new(|| {
        DataMapType::builder(
            neoforge("acceptable_villager_distances"),
            vanilla("entity_type"),
        )
        .synced(false)
        .build()
    });

/// `NeoForgeDataMaps.BLOCK_TRANSFORM_APPENDERS`.
pub static BLOCK_TRANSFORM_APPENDERS: LazyLock<AdvancedDataMapType<BlockTransformAppender>> =
    LazyLock::new(|| {
        AdvancedDataMapType::builder(
            neoforge("block_transform_appenders"),
            vanilla("block_transformer"),
        )
        .synced(false)
        .merger(WrappedMerger::new(
            appender_entries,
            appender_from_entries,
            ListMerger,
        ))
        .build()
    });

/// `NeoForgeDataMaps.MONSTER_ROOM_MOBS`.
pub static MONSTER_ROOM_MOBS: LazyLock<DataMapType<MonsterRoomMob>> = LazyLock::new(|| {
    DataMapType::builder(neoforge("monster_room_mobs"), vanilla("entity_type"))
        .synced(false)
        .build()
});

/// `NeoForgeDataMaps.OXIDIZABLES`.
pub static OXIDIZABLES: LazyLock<DataMapType<Oxidizable>> = LazyLock::new(|| {
    DataMapType::builder(neoforge("oxidizables"), vanilla("block"))
        .synced(false)
        .build()
});

/// `NeoForgeDataMaps.PARROT_IMITATIONS`.
pub static PARROT_IMITATIONS: LazyLock<DataMapType<ParrotImitation>> = LazyLock::new(|| {
    DataMapType::builder(neoforge("parrot_imitations"), vanilla("entity_type"))
        .synced(false)
        .build()
});

/// `NeoForgeDataMaps.RAID_HERO_GIFTS`.
pub static RAID_HERO_GIFTS: LazyLock<DataMapType<RaidHeroGift>> = LazyLock::new(|| {
    DataMapType::builder(neoforge("raid_hero_gifts"), vanilla("villager_profession"))
        .synced(false)
        .build()
});

/// `NeoForgeDataMaps.TRANSFORMABLES`.
pub static TRANSFORMABLES: LazyLock<AdvancedDataMapType<Transformable, TransformableRemover>> =
    LazyLock::new(|| {
        AdvancedDataMapType::builder(neoforge("transformables"), vanilla("block"))
            .synced(false)
            .merger(WrappedMerger::new(
                transformable_transformers,
                transformable_from_transformers,
                MapMerger,
            ))
            .remover::<TransformableRemover>()
            .disable_tag_support(SINGLE_BLOCK_REASON)
            .build()
    });

/// `NeoForgeDataMaps.VIBRATION_FREQUENCIES`.
pub static VIBRATION_FREQUENCIES: LazyLock<DataMapType<VibrationFrequency>> = LazyLock::new(|| {
    DataMapType::builder(neoforge("vibration_frequencies"), vanilla("game_event"))
        .synced(false)
        .build()
});

/// `NeoForgeDataMaps.VILLAGER_COMPOSTABLES`: not synced.
pub static VILLAGER_COMPOSTABLES: LazyLock<DataMapType<VillagerCompostable>> =
    LazyLock::new(|| {
        DataMapType::builder(neoforge("villager_compostables"), vanilla("item")).build()
    });

/// `NeoForgeDataMaps.VILLAGER_TYPES`.
pub static VILLAGER_TYPES: LazyLock<DataMapType<BiomeVillagerType>> = LazyLock::new(|| {
    DataMapType::builder(neoforge("villager_types"), vanilla("worldgen/biome"))
        .synced(false)
        .build()
});

/// `NeoForgeDataMaps.WAXABLES`.
pub static WAXABLES: LazyLock<DataMapType<Waxable>> = LazyLock::new(|| {
    DataMapType::builder(neoforge("waxables"), vanilla("block"))
        .synced(false)
        .build()
});

/// `NeoForgeDataMaps.register`, in its order.
pub(super) fn register(event: &mut RegisterDataMapTypesEvent) {
    let _ = event.register(&*ACCEPTABLE_VILLAGER_DISTANCES);
    let _ = event.register(&*BLOCK_TRANSFORM_APPENDERS);
    let _ = event.register(&*MONSTER_ROOM_MOBS);
    let _ = event.register(&*OXIDIZABLES);
    let _ = event.register(&*PARROT_IMITATIONS);
    let _ = event.register(&*RAID_HERO_GIFTS);
    let _ = event.register(&*TRANSFORMABLES);
    let _ = event.register(&*VIBRATION_FREQUENCIES);
    let _ = event.register(&*VILLAGER_COMPOSTABLES);
    let _ = event.register(&*VILLAGER_TYPES);
    let _ = event.register(&*WAXABLES);
}
