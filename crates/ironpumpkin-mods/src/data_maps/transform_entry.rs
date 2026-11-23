//! Block transforms from data maps: a `BlockTransformData` value as a block transformer entry.
//!
//! The entry has the form of the generated transformers of `pumpkin-data`, so the transform
//! runs through the code of the vanilla axe, hoe and shovel. It reads these parts of the value:
//! a `rule_based` state provider (or one simple or `copy_properties` provider), whose rules have
//! a `matching_blocks`, `matching_block_tag` or `all_of` predicate, a vanilla sound, the
//! particle, the disallowed faces, the loot table, the drop strategy, the transform type, the
//! update from neighbors and the item damage. A block state keeps its block and loses its
//! properties, as the generated transformers do. A part that the entry cannot hold makes the
//! transform unavailable, and the loader logs it.

use pumpkin_data::{
    BlockDirection, BlockId,
    block_transformer::{
        BlockPredicate, BlockTransformerEntry, BlockTransformerRule, BlockTransformerStateProvider,
        DropStrategy, TransformType,
    },
    dynamic::ContentKind,
    sound::Sound,
    world::WorldEvent,
};
use pumpkin_util::identifier::Identifier;
use serde_json::{Map, Value};
use tracing::warn;

use super::builtin::BlockTransformData;

type Compiled<T> = Result<T, String>;

fn leak<T>(values: Vec<T>) -> &'static [T] {
    Box::leak(values.into_boxed_slice())
}

fn field_str<'a>(fields: &'a Map<String, Value>, name: &str) -> Compiled<&'a str> {
    fields
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("no string {name}"))
}

fn block_id(name: &str) -> Compiled<BlockId> {
    let id = Identifier::parse(name).map_err(|error| format!("{name}: {error}"))?;
    crate::registry::id_of(ContentKind::Block, &id.to_string())
        .and_then(BlockId::new)
        .ok_or_else(|| format!("unknown block {id}"))
}

/// A block state: a block id, or an object with an `id`.
fn state_block(json: &Value) -> Compiled<BlockId> {
    match json {
        Value::String(name) => block_id(name),
        Value::Object(fields) => block_id(field_str(fields, "id")?),
        _ => Err(format!("not a block state: {json}")),
    }
}

/// `BlockStateProvider` of the `simple` and `copy_properties` forms.
fn state_provider(json: &Value) -> Compiled<BlockTransformerStateProvider> {
    let Some(fields) = json.as_object() else {
        return Err(format!("a provider that is not an object: {json}"));
    };
    let Some(kind) = fields.get("type").and_then(Value::as_str) else {
        // `BlockState.FULL_CODEC`.
        return block_id(field_str(fields, "id")?).map(BlockTransformerStateProvider::SimpleState);
    };
    match kind.strip_prefix("minecraft:").unwrap_or(kind) {
        "simple" => state_block(fields.get("state").ok_or("no state")?)
            .map(BlockTransformerStateProvider::SimpleState),
        "copy_properties" => {
            let source = fields.get("source").ok_or("no source")?;
            let block = match source.get("type").and_then(Value::as_str) {
                Some(kind) if kind.strip_prefix("minecraft:").unwrap_or(kind) == "simple" => {
                    state_block(source.get("state").ok_or("no state")?)?
                }
                Some(kind) => return Err(format!("the source provider {kind}")),
                None => state_block(source)?,
            };
            Ok(BlockTransformerStateProvider::CopyProperties(block))
        }
        other => Err(format!("the state provider {other}")),
    }
}

fn offset(fields: &Map<String, Value>) -> Compiled<(i8, i8, i8)> {
    let Some(offset) = fields.get("offset") else {
        return Ok((0, 0, 0));
    };
    let part = |index: usize| {
        offset
            .get(index)
            .and_then(Value::as_i64)
            .and_then(|value| i8::try_from(value).ok())
            .ok_or_else(|| format!("not an offset: {offset}"))
    };
    Ok((part(0)?, part(1)?, part(2)?))
}

/// The ids of the blocks that a `HolderSet` names: a tag, one id or a list of ids.
fn block_set(json: &Value) -> Compiled<Vec<BlockId>> {
    match json {
        Value::String(text) if text.starts_with('#') => tag_blocks(&text[1..]),
        Value::String(text) => Ok(vec![block_id(text)?]),
        Value::Array(list) => list
            .iter()
            .map(|entry| block_id(entry.as_str().ok_or("not a block id")?))
            .collect(),
        _ => Err(format!("not a block set: {json}")),
    }
}

fn tag_blocks(tag: &str) -> Compiled<Vec<BlockId>> {
    let id = Identifier::parse(tag).map_err(|error| format!("{tag}: {error}"))?;
    let entries = crate::registry::tag_entries(ContentKind::Block, &id.to_string())
        .ok_or_else(|| format!("unknown block tag {id}"))?;
    Ok(entries
        .iter()
        .filter_map(|entry| crate::registry::id_of(ContentKind::Block, &entry.to_string()))
        .filter_map(BlockId::new)
        .collect())
}

fn predicate(json: &Value) -> Compiled<BlockPredicate> {
    let fields = json
        .as_object()
        .ok_or("a predicate that is not an object")?;
    let kind = field_str(fields, "type")?;
    match kind.strip_prefix("minecraft:").unwrap_or(kind) {
        "matching_blocks" => Ok(BlockPredicate::MatchingBlocks {
            blocks: leak(block_set(fields.get("blocks").ok_or("no blocks")?)?),
            offset: offset(fields)?,
        }),
        "matching_block_tag" => Ok(BlockPredicate::MatchingBlocks {
            blocks: leak(tag_blocks(field_str(fields, "tag")?)?),
            offset: offset(fields)?,
        }),
        "all_of" => Ok(BlockPredicate::AllOf(leak(
            fields
                .get("predicates")
                .and_then(Value::as_array)
                .ok_or("no predicates")?
                .iter()
                .map(predicate)
                .collect::<Compiled<Vec<_>>>()?,
        ))),
        other => Err(format!("the block predicate {other}")),
    }
}

/// A predicate that matches every block: the rule of a provider with no condition.
const ALWAYS: BlockPredicate = BlockPredicate::AllOf(&[]);

fn rules(provider: &Value) -> Compiled<Vec<BlockTransformerRule>> {
    let is_rule_based = provider
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|kind| kind.strip_prefix("minecraft:").unwrap_or(kind) == "rule_based");
    if !is_rule_based {
        return Ok(vec![BlockTransformerRule {
            predicate: ALWAYS,
            provider: state_provider(provider)?,
        }]);
    }
    let mut rules = provider
        .get("rules")
        .and_then(Value::as_array)
        .ok_or("no rules")?
        .iter()
        .map(|rule| {
            Ok(BlockTransformerRule {
                predicate: predicate(rule.get("if_true").ok_or("no if_true")?)?,
                provider: state_provider(rule.get("then").ok_or("no then")?)?,
            })
        })
        .collect::<Compiled<Vec<_>>>()?;
    if let Some(fallback) = provider.get("fallback") {
        rules.push(BlockTransformerRule {
            predicate: ALWAYS,
            provider: state_provider(fallback)?,
        });
    }
    Ok(rules)
}

fn sound(json: Option<&Value>) -> Compiled<Option<Sound>> {
    let Some(json) = json else {
        return Ok(None);
    };
    let name = match json {
        Value::String(name) => name.as_str(),
        Value::Object(fields) => field_str(fields, "sound_id")?,
        _ => return Err(format!("not a sound: {json}")),
    };
    let id = Identifier::parse(name).map_err(|error| format!("{name}: {error}"))?;
    if !id.is_vanilla() {
        return Err(format!("the sound {id} of a mod"));
    }
    Sound::from_name(id.path())
        .map(Some)
        .ok_or_else(|| format!("unknown sound {id}"))
}

fn direction(name: &str) -> Compiled<BlockDirection> {
    Ok(match name {
        "down" => BlockDirection::Down,
        "up" => BlockDirection::Up,
        "north" => BlockDirection::North,
        "south" => BlockDirection::South,
        "west" => BlockDirection::West,
        "east" => BlockDirection::East,
        _ => return Err(format!("unknown direction {name}")),
    })
}

/// The entry of a `BlockTransformData` value, or the reason the entry cannot hold it.
fn entry(data: &BlockTransformData) -> Compiled<BlockTransformerEntry> {
    let fields = &data.0;
    let rules = rules(fields.get("block_state_provider").ok_or("no provider")?)?;
    let particle = match fields.get("particle").and_then(Value::as_str) {
        Some("scrape") => Some(WorldEvent::ParticlesScrape),
        Some("wax_off") => Some(WorldEvent::ParticlesWaxOff),
        Some("wax_on") => Some(WorldEvent::ParticlesWaxOn),
        _ => None,
    };
    let disallowed_faces = fields
        .get("disallowed_faces")
        .and_then(Value::as_array)
        .map(|faces| {
            faces
                .iter()
                .map(|face| direction(face.as_str().unwrap_or_default()))
                .collect::<Compiled<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();
    let loot = fields
        .get("loot")
        .and_then(Value::as_str)
        .map(|loot| {
            Identifier::parse(loot)
                .map(|id| &*Box::leak(id.to_string().into_boxed_str()))
                .map_err(|error| format!("{loot}: {error}"))
        })
        .transpose()?;
    let drop_strategy = match fields.get("drop_strategy").and_then(Value::as_str) {
        Some("clicked_face") => Some(DropStrategy::ClickedFace),
        _ => None,
    };
    let transform_type = match fields.get("transform_type").and_then(Value::as_str) {
        Some("copper_chest") => Some(TransformType::CopperChest),
        _ => None,
    };
    Ok(BlockTransformerEntry {
        rules: leak(rules),
        disallowed_faces: leak(disallowed_faces),
        item_damage_per_use: fields
            .get("item_damage_per_use")
            .and_then(Value::as_u64)
            .map_or(0, |damage| u16::try_from(damage).unwrap_or(u16::MAX)),
        sound: sound(fields.get("sound"))?,
        particle,
        loot,
        drop_strategy,
        transform_type,
        update_from_neighbors: fields
            .get("update_from_neighbors")
            .and_then(Value::as_bool)
            .unwrap_or(true),
    })
}

/// The entry of a value, or `None` with a warning when the entry cannot hold it. `source` names
/// the data map and the key of the value.
pub(super) fn compile(
    data: &BlockTransformData,
    source: &str,
) -> Option<&'static BlockTransformerEntry> {
    match entry(data) {
        Ok(entry) => Some(Box::leak(Box::new(entry))),
        Err(reason) => {
            warn!("Ignored a block transform of {source}: IronPumpkin does not support {reason}");
            None
        }
    }
}
