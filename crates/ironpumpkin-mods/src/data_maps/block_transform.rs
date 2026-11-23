//! The check of `BlockTransformer.BlockTransformData.CODEC` on a JSON value.
//!
//! A `NeoForge` client decodes each synced value with this codec and disconnects when it fails,
//! so the loader refuses what the codec refuses. The value stays JSON after the check.

use pumpkin_data::dynamic::ContentKind;
use pumpkin_util::identifier::Identifier;
use serde_json::{Map, Value};

type Check = Result<(), String>;

fn object(json: &Value) -> Result<&Map<String, Value>, String> {
    json.as_object().ok_or_else(|| format!("Not a map: {json}"))
}

fn field<'a>(fields: &'a Map<String, Value>, name: &str) -> Result<&'a Value, String> {
    fields
        .get(name)
        .ok_or_else(|| format!("No key {name} in MapLike"))
}

/// Runs `check` on an optional field.
fn optional(fields: &Map<String, Value>, name: &str, check: impl FnOnce(&Value) -> Check) -> Check {
    fields.get(name).map_or(Ok(()), check)
}

fn identifier(json: &Value) -> Result<Identifier, String> {
    let text = json
        .as_str()
        .ok_or_else(|| format!("Not a string: {json}"))?;
    Identifier::parse(text)
        .map_err(|error| format!("Not a valid resource location: {text} {error}"))
}

fn boolean(json: &Value) -> Check {
    json.as_bool()
        .map(drop)
        .ok_or_else(|| format!("Not a boolean: {json}"))
}

/// `StringRepresentable.fromValues`.
fn one_of(json: &Value, names: &[&str]) -> Check {
    match json.as_str() {
        Some(name) if names.contains(&name) => Ok(()),
        _ => Err(format!("Unknown element name: {json}")),
    }
}

fn number_in(json: &Value, min: f64, max: f64) -> Check {
    let value = json
        .as_f64()
        .ok_or_else(|| format!("Not a number: {json}"))?;
    if value < min || value > max {
        return Err(format!("Value {value} outside of range [{min}:{max}]"));
    }
    Ok(())
}

fn whole_in(json: &Value, min: i64, max: i64) -> Check {
    let value = json
        .as_f64()
        .ok_or_else(|| format!("Not a number: {json}"))?;
    #[expect(clippy::cast_possible_truncation, reason = "Number.intValue")]
    let value = value.trunc() as i64;
    if value < min || value > max {
        return Err(format!("Value must be within range [{min};{max}]: {value}"));
    }
    Ok(())
}

fn non_empty_list(json: &Value) -> Result<&Vec<Value>, String> {
    match json.as_array() {
        Some(list) if !list.is_empty() => Ok(list),
        Some(_) => Err("List must have contents".to_owned()),
        None => Err(format!("Not a list: {json}")),
    }
}

/// `Registry.byNameCodec()` of the block registry.
fn block(json: &Value) -> Check {
    let id = identifier(json)?;
    if crate::registry::contains(ContentKind::Block, &id.to_string()) {
        Ok(())
    } else {
        Err(format!(
            "Unknown registry key in ResourceKey[minecraft:root / minecraft:block]: {id}"
        ))
    }
}

/// `BlockState.FULL_CODEC`: `{"id": block, "properties": ..}`. The properties are lenient: a
/// wrong value falls back to the default state.
fn full_block_state(json: &Value) -> Check {
    block(field(object(json)?, "id")?)
}

/// `BlockState.CODEC`: a block id, or the full form.
fn block_state(json: &Value) -> Check {
    if json.is_string() {
        block(json)
    } else {
        full_block_state(json)
    }
}

fn block_states(json: &Value) -> Check {
    non_empty_list(json)?.iter().try_for_each(block_state)
}

/// `RegistryCodecs.holderSet(Registries.BLOCK)`: a tag, one id or a list of ids.
fn block_set(json: &Value) -> Check {
    match json {
        Value::String(text) if text.starts_with('#') => Identifier::parse(&text[1..])
            .map(drop)
            .map_err(|error| format!("Not a valid resource location: {text} {error}")),
        Value::Array(list) => list.iter().try_for_each(block),
        _ => block(json),
    }
}

const DIRECTIONS: &[&str] = &["down", "up", "north", "south", "west", "east"];

/// `IntProviders.CODEC`: an int, or an object with a `type`.
fn int_provider(json: &Value) -> Check {
    if json.is_number() {
        return whole_in(json, i64::from(i32::MIN), i64::from(i32::MAX));
    }
    identifier(field(object(json)?, "type")?).map(drop)
}

/// `BlockPredicate.CODEC`: an object with a `type`.
fn block_predicate(json: &Value) -> Check {
    identifier(field(object(json)?, "type")?).map(drop)
}

/// `NormalNoise.Parameters.CODEC`.
fn noise(json: &Value) -> Check {
    let fields = object(json)?;
    optional(fields, "base_amplitude", |value| {
        number_in(value, 1.0e-5, 1.0e6)
    })?;
    whole_in(field(fields, "base_octave")?, -32, 32)?;
    optional(fields, "octave_count", |value| whole_in(value, 1, 32))?;
    optional(fields, "normalize", |value| match value {
        Value::Bool(_) => Ok(()),
        Value::String(text) if text == "legacy" => Ok(()),
        _ => Err(format!("Invalid normalization type: {value}")),
    })?;
    optional(fields, "amplitude_modifiers", |value| {
        let list = value
            .as_array()
            .ok_or_else(|| format!("Not a list: {value}"))?;
        if list.len() > 32 {
            return Err(format!("List too long: {} > 32", list.len()));
        }
        list.iter()
            .try_for_each(|value| number_in(value, 0.0, 1.0e6))
    })
}

/// The fields of `NoiseBasedStateProvider`.
fn noise_based(fields: &Map<String, Value>) -> Check {
    let seed = field(fields, "seed")?;
    if !seed.is_number() {
        return Err(format!("Not a number: {seed}"));
    }
    noise(field(fields, "noise")?)?;
    positive(field(fields, "scale")?)
}

/// `ExtraCodecs.POSITIVE_FLOAT`.
fn positive(json: &Value) -> Check {
    match json.as_f64() {
        Some(value) if value > 0.0 => Ok(()),
        _ => Err(format!("Value must be positive: {json}")),
    }
}

/// `InclusiveRange.codec(Codec.INT, min, max)`: a point, `[min, max]` or
/// `{"min_inclusive", "max_inclusive"}`.
fn int_range(json: &Value, min: i64, max: i64) -> Check {
    let (low, high) = match json {
        Value::Array(pair) if pair.len() == 2 => (&pair[0], &pair[1]),
        Value::Object(fields) => (
            field(fields, "min_inclusive")?,
            field(fields, "max_inclusive")?,
        ),
        _ => (json, json),
    };
    whole_in(low, min, max)?;
    whole_in(high, min, max)?;
    if low.as_f64() > high.as_f64() {
        return Err(format!("Range: min > max: {json}"));
    }
    Ok(())
}

/// `TYPED_CODEC`: the providers of `BlockStateProviderTypes`.
fn typed_provider(fields: &Map<String, Value>) -> Check {
    let kind = identifier(field(fields, "type")?)?;
    if !kind.is_vanilla() {
        return Err(format!(
            "Unknown registry key in ResourceKey[minecraft:root / minecraft:block_state_provider_type]: {kind}"
        ));
    }
    match kind.path() {
        "simple" => block_state(field(fields, "state")?),
        "weighted" => non_empty_list(field(fields, "entries")?)?
            .iter()
            .try_for_each(|entry| {
                let entry = object(entry)?;
                block_state(field(entry, "data")?)?;
                whole_in(field(entry, "weight")?, 0, i64::from(i32::MAX))
            }),
        "rotated" => {
            provider(field(fields, "state")?)?;
            optional(fields, "direction", |value| one_of(value, DIRECTIONS))
        }
        "copy_properties" => provider(field(fields, "source")?),
        "random_block" => block_set(field(fields, "blocks")?),
        "randomized_int" => {
            provider(field(fields, "source")?)?;
            field(fields, "property")?
                .as_str()
                .ok_or("Not a string: property")?;
            int_provider(field(fields, "values")?)
        }
        "noise" => {
            noise_based(fields)?;
            block_states(field(fields, "states")?)
        }
        "noise_threshold" => {
            noise_based(fields)?;
            number_in(field(fields, "threshold")?, -1.0, 1.0)?;
            number_in(field(fields, "high_chance")?, 0.0, 1.0)?;
            block_state(field(fields, "default_state")?)?;
            block_states(field(fields, "low_states")?)?;
            block_states(field(fields, "high_states")?)
        }
        "dual_noise" => {
            noise_based(fields)?;
            block_states(field(fields, "states")?)?;
            int_range(field(fields, "variety")?, 1, 64)?;
            noise(field(fields, "slow_noise")?)?;
            positive(field(fields, "slow_scale")?)
        }
        "rule_based" => {
            optional(fields, "fallback", provider)?;
            field(fields, "rules")?
                .as_array()
                .ok_or("Not a list: rules")?
                .iter()
                .try_for_each(|rule| {
                    let rule = object(rule)?;
                    block_predicate(field(rule, "if_true")?)?;
                    provider(field(rule, "then")?)
                })
        }
        _ => Err(format!(
            "Unknown registry key in ResourceKey[minecraft:root / minecraft:block_state_provider_type]: {kind}"
        )),
    }
}

/// `BlockStateProvider.CODEC`: a reference to `worldgen/block_state_provider`, or
/// `Codec.xor(BlockState.FULL_CODEC, TYPED_CODEC)`.
fn provider(json: &Value) -> Check {
    if json.is_string() {
        return identifier(json).map(drop);
    }
    let fields = object(json)?;
    match (full_block_state(json), typed_provider(fields)) {
        (Ok(()), Ok(())) => Err(format!(
            "Both alternatives read successfully, can not pick the correct one: {json}"
        )),
        (Ok(()), Err(_)) | (Err(_), Ok(())) => Ok(()),
        (Err(state), Err(typed)) => Err(format!(
            "Failed to parse either. First: {state}; Second: {typed}"
        )),
    }
}

/// `SoundEvent.CODEC`: a sound id or `{"sound_id", "range"}`.
fn sound(json: &Value) -> Check {
    if json.is_string() {
        let id = identifier(json)?;
        return if crate::registry::contains(ContentKind::SoundEvent, &id.to_string()) {
            Ok(())
        } else {
            Err(format!(
                "Unknown registry key in ResourceKey[minecraft:root / minecraft:sound_event]: {id}"
            ))
        };
    }
    identifier(field(object(json)?, "sound_id")?).map(drop)
}

/// `BlockTransformData.CODEC`.
pub(super) fn check(json: &Value) -> Check {
    let fields = object(json)?;
    provider(field(fields, "block_state_provider")?)?;
    optional(fields, "sound", sound)?;
    optional(fields, "particle", |value| {
        one_of(value, &["none", "scrape", "wax_on", "wax_off"])
    })?;
    optional(fields, "disallowed_faces", |value| {
        value
            .as_array()
            .ok_or_else(|| format!("Not a list: {value}"))?
            .iter()
            .try_for_each(|face| one_of(face, DIRECTIONS))
    })?;
    optional(fields, "loot", |value| identifier(value).map(drop))?;
    optional(fields, "drop_strategy", |value| {
        one_of(value, &["clicked_face", "from_middle"])
    })?;
    optional(fields, "update_from_neighbors", boolean)?;
    optional(fields, "transform_type", |value| {
        one_of(value, &["single_block", "copper_chest"])
    })?;
    optional(fields, "consume_on_use", boolean)?;
    optional(fields, "item_damage_per_use", |value| {
        whole_in(value, 0, i64::from(i32::MAX))
    })
}
