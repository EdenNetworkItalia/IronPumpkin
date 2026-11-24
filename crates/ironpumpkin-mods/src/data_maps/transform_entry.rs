//! Block transforms from data maps: a `BlockTransformData` value as a block transformer entry.
//!
//! The entry has the form of the generated transformers of `pumpkin-data`, so the transform
//! runs through the code of the vanilla axe, hoe and shovel. It reads these parts of the value:
//! a `rule_based` state provider (or one simple or `copy_properties` provider), whose rules have
//! a `true`, `matching_blocks`, `matching_block_tag` or `all_of` predicate, a vanilla sound, the
//! particle, the disallowed faces, the loot table, the drop strategy, the transform type, the
//! update from neighbors and the item damage. A block state keeps its block: a state with
//! properties, a provider reference, another provider or predicate, and a sound of a mod make the
//! transform unavailable, and the loader logs it. `consume_on_use` has no effect.
//!
//! A value that compiles to the same entry gives the same `&'static` entry, so a `/reload` with
//! the same data allocates nothing.

use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex, PoisonError},
};

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

use super::{builtin::BlockTransformData, registries::Lookup};

type Compiled<T> = Result<T, String>;

/// A block predicate before it is leaked.
#[derive(Debug)]
enum Predicate {
    Blocks {
        blocks: Vec<BlockId>,
        offset: (i8, i8, i8),
    },
    AllOf(Vec<Self>),
}

/// A state provider before it is leaked.
#[derive(Debug, Clone, Copy)]
enum Provider {
    Simple(BlockId),
    CopyProperties(BlockId),
}

/// An entry before it is leaked: its `Debug` text is the key of the entry.
#[derive(Debug)]
struct Entry {
    rules: Vec<(Predicate, Provider)>,
    disallowed_faces: Vec<BlockDirection>,
    item_damage_per_use: u16,
    sound: Option<Sound>,
    particle: Option<WorldEvent>,
    loot: Option<String>,
    drop_strategy: Option<DropStrategy>,
    transform_type: Option<TransformType>,
    update_from_neighbors: bool,
}

/// What the compiler reads besides the value.
struct Context<'a> {
    lookup: &'a dyn Lookup,
}

fn leak<T>(values: Vec<T>) -> &'static [T] {
    Box::leak(values.into_boxed_slice())
}

fn field_str<'a>(fields: &'a Map<String, Value>, name: &str) -> Compiled<&'a str> {
    fields
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("no string {name}"))
}

fn path(kind: &str) -> &str {
    kind.strip_prefix("minecraft:").unwrap_or(kind)
}

fn block_id(name: &str) -> Compiled<BlockId> {
    let id = Identifier::parse(name).map_err(|error| format!("{name}: {error}"))?;
    crate::registry::id_of(ContentKind::Block, &id.to_string())
        .and_then(BlockId::new)
        .ok_or_else(|| format!("unknown block {id}"))
}

/// A block state: a block id, or an object with an `id`. A state with properties is refused unless
/// `properties_allowed` is set: the entry holds the default state of a block.
fn state_block(json: &Value, properties_allowed: bool) -> Compiled<BlockId> {
    match json {
        Value::String(name) => block_id(name),
        Value::Object(fields) => {
            let has_properties = fields
                .get("properties")
                .and_then(Value::as_object)
                .is_some_and(|properties| !properties.is_empty());
            if has_properties && !properties_allowed {
                return Err(format!("block state properties: {json}"));
            }
            block_id(field_str(fields, "id")?)
        }
        _ => Err(format!("not a block state: {json}")),
    }
}

/// `BlockStateProvider` of the `simple` and `copy_properties` forms.
fn state_provider(json: &Value) -> Compiled<Provider> {
    if json.is_string() {
        return Err(format!("a block state provider reference: {json}"));
    }
    let Some(fields) = json.as_object() else {
        return Err(format!("a provider that is not an object: {json}"));
    };
    let Some(kind) = fields.get("type").and_then(Value::as_str) else {
        // `BlockState.FULL_CODEC`.
        return state_block(json, false).map(Provider::Simple);
    };
    match path(kind) {
        "simple" => {
            state_block(fields.get("state").ok_or("no state")?, false).map(Provider::Simple)
        }
        "copy_properties" => {
            let source = fields.get("source").ok_or("no source")?;
            if source.is_string() {
                return Err(format!("a block state provider reference: {source}"));
            }
            // The properties of the source are defaults: the block that is there overrides them.
            let block = match source.get("type").and_then(Value::as_str) {
                Some(kind) if path(kind) == "simple" => {
                    state_block(source.get("state").ok_or("no state")?, true)?
                }
                Some(kind) => return Err(format!("the source provider {kind}")),
                None => state_block(source, true)?,
            };
            Ok(Provider::CopyProperties(block))
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
fn block_set(json: &Value, cx: &Context) -> Compiled<Vec<BlockId>> {
    match json {
        Value::String(text) if text.starts_with('#') => tag_blocks(&text[1..], cx),
        Value::String(text) => Ok(vec![block_id(text)?]),
        Value::Array(list) => list
            .iter()
            .map(|entry| block_id(entry.as_str().ok_or("not a block id")?))
            .collect(),
        _ => Err(format!("not a block set: {json}")),
    }
}

/// The blocks of a tag as the server resolves it: the built-in tags, the tags of mods and the
/// tags of the enabled data packs.
fn tag_blocks(tag: &str, cx: &Context) -> Compiled<Vec<BlockId>> {
    let id = Identifier::parse(tag).map_err(|error| format!("{tag}: {error}"))?;
    Ok(cx
        .lookup
        .tag(&Identifier::vanilla_static("block"), &id)
        .iter()
        .filter_map(|entry| crate::registry::id_of(ContentKind::Block, &entry.to_string()))
        .filter_map(BlockId::new)
        .collect())
}

fn predicate(json: &Value, cx: &Context) -> Compiled<Predicate> {
    let fields = json
        .as_object()
        .ok_or("a predicate that is not an object")?;
    let kind = field_str(fields, "type")?;
    match path(kind) {
        "true" => Ok(Predicate::AllOf(Vec::new())),
        "matching_blocks" => Ok(Predicate::Blocks {
            blocks: block_set(fields.get("blocks").ok_or("no blocks")?, cx)?,
            offset: offset(fields)?,
        }),
        "matching_block_tag" => Ok(Predicate::Blocks {
            blocks: tag_blocks(field_str(fields, "tag")?, cx)?,
            offset: offset(fields)?,
        }),
        "all_of" => Ok(Predicate::AllOf(
            fields
                .get("predicates")
                .and_then(Value::as_array)
                .ok_or("no predicates")?
                .iter()
                .map(|json| predicate(json, cx))
                .collect::<Compiled<Vec<_>>>()?,
        )),
        other => Err(format!("the block predicate {other}")),
    }
}

fn rules(provider: &Value, cx: &Context) -> Compiled<Vec<(Predicate, Provider)>> {
    let is_rule_based = provider
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|kind| path(kind) == "rule_based");
    if !is_rule_based {
        return Ok(vec![(
            Predicate::AllOf(Vec::new()),
            state_provider(provider)?,
        )]);
    }
    let mut rules = provider
        .get("rules")
        .and_then(Value::as_array)
        .ok_or("no rules")?
        .iter()
        .map(|rule| {
            Ok((
                predicate(rule.get("if_true").ok_or("no if_true")?, cx)?,
                state_provider(rule.get("then").ok_or("no then")?)?,
            ))
        })
        .collect::<Compiled<Vec<_>>>()?;
    if let Some(fallback) = provider.get("fallback") {
        rules.push((Predicate::AllOf(Vec::new()), state_provider(fallback)?));
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
fn compile_entry(data: &BlockTransformData, cx: &Context) -> Compiled<Entry> {
    let fields = &data.0;
    let rules = rules(fields.get("block_state_provider").ok_or("no provider")?, cx)?;
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
                .map(|id| id.to_string())
                .map_err(|error| format!("{loot}: {error}"))
        })
        .transpose()?;
    Ok(Entry {
        rules,
        disallowed_faces,
        item_damage_per_use: fields
            .get("item_damage_per_use")
            .and_then(Value::as_u64)
            .map_or(0, |damage| u16::try_from(damage).unwrap_or(u16::MAX)),
        sound: sound(fields.get("sound"))?,
        particle,
        loot,
        drop_strategy: match fields.get("drop_strategy").and_then(Value::as_str) {
            Some("clicked_face") => Some(DropStrategy::ClickedFace),
            _ => None,
        },
        transform_type: match fields.get("transform_type").and_then(Value::as_str) {
            Some("copper_chest") => Some(TransformType::CopperChest),
            _ => None,
        },
        update_from_neighbors: fields
            .get("update_from_neighbors")
            .and_then(Value::as_bool)
            .unwrap_or(true),
    })
}

fn leak_predicate(predicate: &Predicate) -> BlockPredicate {
    match predicate {
        Predicate::Blocks { blocks, offset } => BlockPredicate::MatchingBlocks {
            blocks: leak(blocks.clone()),
            offset: *offset,
        },
        Predicate::AllOf(predicates) => {
            BlockPredicate::AllOf(leak(predicates.iter().map(leak_predicate).collect()))
        }
    }
}

fn leak_entry(entry: &Entry) -> &'static BlockTransformerEntry {
    let rules = entry
        .rules
        .iter()
        .map(|(predicate, provider)| BlockTransformerRule {
            predicate: leak_predicate(predicate),
            provider: match provider {
                Provider::Simple(block) => BlockTransformerStateProvider::SimpleState(*block),
                Provider::CopyProperties(block) => {
                    BlockTransformerStateProvider::CopyProperties(*block)
                }
            },
        })
        .collect();
    Box::leak(Box::new(BlockTransformerEntry {
        rules: leak(rules),
        disallowed_faces: leak(entry.disallowed_faces.clone()),
        item_damage_per_use: entry.item_damage_per_use,
        sound: entry.sound,
        particle: entry.particle,
        loot: entry
            .loot
            .as_deref()
            .map(|loot| &*Box::leak(loot.to_owned().into_boxed_str())),
        drop_strategy: entry.drop_strategy,
        transform_type: entry.transform_type,
        update_from_neighbors: entry.update_from_neighbors,
    }))
}

/// The entries that the server holds, by the `Debug` text of the compiled entry.
static ENTRIES: LazyLock<Mutex<HashMap<String, &'static BlockTransformerEntry>>> =
    LazyLock::new(Mutex::default);

/// The entry of a value, or `None` with a warning when the entry cannot hold it. `source` names
/// the data map and the key of the value.
pub(super) fn compile(
    data: &BlockTransformData,
    source: &str,
    lookup: &dyn Lookup,
) -> Option<&'static BlockTransformerEntry> {
    if data.0.get("consume_on_use").and_then(Value::as_bool) == Some(false) {
        warn!("The block transform of {source} has consume_on_use false: IronPumpkin ignores it");
    }
    match compile_entry(data, &Context { lookup }) {
        Ok(entry) => Some(
            *ENTRIES
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(format!("{entry:?}"))
                .or_insert_with(|| leak_entry(&entry)),
        ),
        Err(reason) => {
            warn!("Ignored a block transform of {source}: IronPumpkin does not support {reason}");
            None
        }
    }
}
