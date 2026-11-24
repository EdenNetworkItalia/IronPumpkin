//! Global loot modifiers (`net.neoforged.neoforge.common.loot`): data pack files that change the
//! loot of every table roll, decoded by the modifier types that mods register.
//!
//! `LootModifierManager` reads one modifier per file, `data/<namespace>/loot_modifiers/<path>.json`
//! with the id `<namespace>:<path>`. A higher pack replaces a file with the same id. Each file
//! names its modifier type in `type`, and the host reads the fields of `LootModifier`: an optional
//! `condition` (an inline condition or a predicate id) and `priority` (default
//! [`DEFAULT_PRIORITY`]). The type's decoder reads the rest of the file.
//!
//! `CommonHooks.modifyLoot` runs after each roll of `LootTable.getRandomItems`: the modifiers
//! whose condition passes run from the highest priority to the lowest, and inside one priority by
//! id, each on the list that the one before returned. Nested tables and
//! [`LootContext::roll_loot_table_raw`] roll without modifiers, as `getRandomItemsRaw` does.

use std::{
    collections::HashMap,
    fmt,
    sync::{
        Arc, LazyLock, PoisonError, RwLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use pumpkin_data::item_stack::ItemStack;
use pumpkin_util::identifier::Identifier;
use pumpkin_util::loot_table::parser::parse_condition_with_predicates;
use pumpkin_util::random::xoroshiro128::Xoroshiro;
use serde_json::Value;
use tracing::error;

use crate::plugin::guarded_call::guarded_call;

use super::{
    DynamicLootCondition, Item, LootContextParameters, LootTableHandle, check_dynamic_condition,
    generation, get_context_loot_table,
};

/// `IGlobalLootModifier.DEFAULT_PRIORITY`.
pub const DEFAULT_PRIORITY: i32 = 1000;
/// `LootTableIdCondition.UNKNOWN_LOOT_TABLE`: the queried id of a table that no id names.
pub const UNKNOWN_LOOT_TABLE: &str = "neoforge:unknown_loot_table";
/// The modifier type of [`AddTableLootModifier`].
pub const ADD_TABLE: &str = "neoforge:add_table";
/// The condition type `LootTableIdCondition`.
pub const LOOT_TABLE_ID: &str = "neoforge:loot_table_id";

const MAX_CONDITION_DEPTH: usize = 128;

/// A decoded global loot modifier (`IGlobalLootModifier`).
///
/// The host tests the `condition` of the file before it calls [`apply`](Self::apply), as
/// `LootModifier.apply` does before `doApply`.
pub trait GlobalLootModifier: Send + Sync {
    /// Returns the loot after the modifier: `generated_loot` itself, changed, or a new list.
    fn apply(
        &self,
        generated_loot: Vec<ItemStack>,
        context: &mut LootContext<'_, '_>,
    ) -> Vec<ItemStack>;
}

/// Decodes the JSON object of a modifier file, `type`, `condition` and `priority` included, into
/// the modifier. An error is the reason the file does not load.
pub type LootModifierDecoder =
    Box<dyn Fn(&Value) -> Result<Box<dyn GlobalLootModifier>, String> + Send + Sync>;

/// Why a modifier type registration failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LootModifierError {
    /// A modifier type with this id exists.
    Duplicate(String),
    /// The registration phase ended: the registries are frozen.
    RegistrationClosed,
    /// The id is not a valid resource location.
    InvalidId(String),
    /// The id has no namespace or the `minecraft` namespace.
    ReservedNamespace(String),
}

impl fmt::Display for LootModifierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duplicate(id) => write!(f, "the loot modifier type \"{id}\" is registered twice"),
            Self::RegistrationClosed => f.write_str("the loot modifier types are frozen"),
            Self::InvalidId(id) => write!(f, "invalid loot modifier type id \"{id}\""),
            Self::ReservedNamespace(id) => {
                write!(
                    f,
                    "the loot modifier type \"{id}\" must use its own namespace"
                )
            }
        }
    }
}

impl std::error::Error for LootModifierError {}

/// The registry `neoforge:global_loot_modifier_serializers`.
pub struct LootModifierTypes {
    decoders: RwLock<HashMap<String, LootModifierDecoder>>,
    closed: AtomicBool,
}

/// The modifier types of the process. It starts with [`ADD_TABLE`].
pub static LOOT_MODIFIER_TYPES: LazyLock<LootModifierTypes> = LazyLock::new(LootModifierTypes::new);

impl LootModifierTypes {
    /// A registry with the built-in type [`ADD_TABLE`].
    #[must_use]
    pub fn new() -> Self {
        let mut decoders: HashMap<String, LootModifierDecoder> = HashMap::new();
        decoders.insert(ADD_TABLE.to_owned(), Box::new(AddTableLootModifier::decode));
        Self {
            decoders: RwLock::new(decoders),
            closed: AtomicBool::new(false),
        }
    }

    /// Registers the modifier type `id`, `namespace:path`, with its decoder.
    ///
    /// # Errors
    ///
    /// [`LootModifierError::InvalidId`] for an id that is not a resource location,
    /// [`LootModifierError::ReservedNamespace`] for an id without a namespace or in `minecraft`,
    /// [`LootModifierError::Duplicate`] for an existing id and
    /// [`LootModifierError::RegistrationClosed`] after [`close`](Self::close).
    pub fn register(
        &self,
        id: &str,
        decoder: LootModifierDecoder,
    ) -> Result<(), LootModifierError> {
        // Before the lock: a decoder that registers during a load would wait for itself.
        if self.closed.load(Ordering::Acquire) {
            return Err(LootModifierError::RegistrationClosed);
        }
        // As the content registries of `pumpkin_data::dynamic` do.
        match id.split_once(':') {
            Some((namespace, _)) if !namespace.is_empty() && namespace != "minecraft" => {}
            _ => return Err(LootModifierError::ReservedNamespace(id.to_owned())),
        }
        let id = normalize_id(id).ok_or_else(|| LootModifierError::InvalidId(id.to_owned()))?;
        let mut decoders = self
            .decoders
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        if self.closed.load(Ordering::Acquire) {
            return Err(LootModifierError::RegistrationClosed);
        }
        if decoders.contains_key(&id) {
            return Err(LootModifierError::Duplicate(id));
        }
        decoders.insert(id, decoder);
        Ok(())
    }

    /// Ends the registration phase. Later registrations fail.
    pub fn close(&self) {
        let _guard = self
            .decoders
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        self.closed.store(true, Ordering::Release);
    }

    /// Decodes the modifier files of one data load, `files` by modifier id, into the modifiers in
    /// application order. A file that fails is logged by id and skipped. `predicates` resolves a
    /// predicate id of a condition.
    pub fn load(
        &self,
        files: HashMap<String, Result<Value, String>>,
        predicates: &dyn Fn(&str) -> Option<Value>,
    ) -> LootModifiers {
        let decoders = self.decoders.read().unwrap_or_else(PoisonError::into_inner);
        let mut sorted: Vec<LoadedLootModifier> = files
            .into_iter()
            .filter_map(|(id, file)| {
                match file.and_then(|json| decode(&decoders, &json, predicates)) {
                    Ok((priority, condition, modifier)) => Some(LoadedLootModifier {
                        id,
                        priority,
                        condition,
                        modifier,
                    }),
                    Err(reason) => {
                        error!("Couldn't parse loot modifier '{id}': {reason}");
                        None
                    }
                }
            })
            .collect();
        sorted.sort_by(|a, b| b.priority.cmp(&a.priority).then_with(|| a.id.cmp(&b.id)));
        if !sorted.is_empty() {
            LIVE_SETS.fetch_add(1, Ordering::Relaxed);
        }
        LootModifiers { sorted }
    }
}

impl Default for LootModifierTypes {
    fn default() -> Self {
        Self::new()
    }
}

fn normalize_id(id: &str) -> Option<String> {
    let id = Identifier::parse(id).ok()?;
    Some(id.to_string())
}

type Decoded = (i32, Option<ModifierCondition>, Box<dyn GlobalLootModifier>);

fn decode(
    decoders: &HashMap<String, LootModifierDecoder>,
    json: &Value,
    predicates: &dyn Fn(&str) -> Option<Value>,
) -> Result<Decoded, String> {
    let kind = json
        .get("type")
        .and_then(Value::as_str)
        .ok_or("no modifier type")?;
    let decoder = normalize_id(kind)
        .and_then(|kind| decoders.get(&kind))
        .ok_or_else(|| format!("unknown modifier type \"{kind}\""))?;
    let priority = match json.get("priority") {
        None => DEFAULT_PRIORITY,
        Some(priority) => int_value(priority).ok_or("the priority is not an int")?,
    };
    let condition = json
        .get("condition")
        .map(|condition| {
            ModifierCondition::parse(condition, predicates, 0).ok_or("unknown or invalid condition")
        })
        .transpose()?;
    let modifier =
        guarded_call(|| decoder(json)).map_err(|_| "the decoder panicked".to_owned())??;
    Ok((priority, condition, modifier))
}

/// `PrimitiveCodec.INT` over `JsonOps` reads `Number.intValue()`: a fraction truncates toward
/// zero. A value outside the `i32` range fails.
fn int_value(value: &Value) -> Option<i32> {
    if let Some(int) = value.as_i64() {
        return i32::try_from(int).ok();
    }
    let float = value.as_f64()?.trunc();
    (float >= f64::from(i32::MIN) && float <= f64::from(i32::MAX)).then_some(float as i32)
}

/// The live [`LootModifiers`] of the process that hold a modifier. While it is 0, a roll skips
/// the modifier lookup.
static LIVE_SETS: AtomicUsize = AtomicUsize::new(0);

struct LoadedLootModifier {
    id: String,
    priority: i32,
    condition: Option<ModifierCondition>,
    modifier: Box<dyn GlobalLootModifier>,
}

/// The modifiers of one data load, in application order.
#[derive(Default)]
pub struct LootModifiers {
    sorted: Vec<LoadedLootModifier>,
}

impl LootModifiers {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.sorted.is_empty()
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.sorted.len()
    }

    /// The modifier ids in application order.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.sorted.iter().map(|modifier| modifier.id.as_str())
    }

    /// `CommonHooks.modifyLoot`. A modifier that panics is logged, and the next one gets the
    /// loot from before it.
    pub fn apply(
        &self,
        mut generated_loot: Vec<ItemStack>,
        context: &mut LootContext<'_, '_>,
    ) -> Vec<ItemStack> {
        for loaded in &self.sorted {
            if loaded
                .condition
                .as_ref()
                .is_none_or(|condition| context.test(condition))
            {
                let before = generated_loot.clone();
                generated_loot = guarded_call(|| loaded.modifier.apply(generated_loot, context))
                    .unwrap_or_else(|_| {
                        error!(
                            "Error applying global loot modifier {} for loot table {}!",
                            loaded.id,
                            context.queried_loot_table_id()
                        );
                        before
                    });
            }
        }
        generated_loot
    }
}

impl Drop for LootModifiers {
    fn drop(&mut self) {
        if !self.sorted.is_empty() {
            LIVE_SETS.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

/// The modifiers of the server of `params`, when it has any.
pub(super) fn active(params: &LootContextParameters<'_>) -> Option<Arc<LootModifiers>> {
    if LIVE_SETS.load(Ordering::Relaxed) == 0 {
        return None;
    }
    let modifiers = params
        .world?
        .server
        .upgrade()?
        .datapack_manager
        .get_loot_modifiers();
    (!modifiers.is_empty()).then_some(modifiers)
}

/// `LootTable.getRandomItems` with the modifiers: the roll and the modifiers share the random
/// source of one loot context.
#[cfg(test)]
fn generate(
    handle: &LootTableHandle,
    seed: i64,
    params: &LootContextParameters<'_>,
    modifiers: &LootModifiers,
) -> Vec<ItemStack> {
    generate_with_rng(
        handle,
        &mut Xoroshiro::from_seed(seed as u64),
        params,
        modifiers,
    )
}

/// [`generate`] on the random source of the caller, which `LootTable.fill` goes on with.
pub(super) fn generate_with_rng(
    handle: &LootTableHandle,
    random: &mut Xoroshiro,
    params: &LootContextParameters<'_>,
    modifiers: &LootModifiers,
) -> Vec<ItemStack> {
    let loot = generation::generate_with_rng(handle.into(), random, params);
    let mut context = LootContext {
        params,
        queried_loot_table_id: handle.id.as_deref().unwrap_or(UNKNOWN_LOOT_TABLE),
        random,
    };
    modifiers.apply(loot, &mut context)
}

/// The loot context of a roll, as a modifier sees it.
pub struct LootContext<'a, 'b> {
    params: &'a LootContextParameters<'b>,
    queried_loot_table_id: &'a str,
    random: &'a mut Xoroshiro,
}

impl<'a, 'b> LootContext<'a, 'b> {
    /// A context for a modifier test outside a roll.
    pub const fn new(
        params: &'a LootContextParameters<'b>,
        queried_loot_table_id: &'a str,
        random: &'a mut Xoroshiro,
    ) -> Self {
        Self {
            params,
            queried_loot_table_id,
            random,
        }
    }

    /// The parameters of the roll: world, position, entities, tool.
    #[must_use]
    pub const fn params(&self) -> &LootContextParameters<'b> {
        self.params
    }

    /// `LootContext.getQueriedLootTableId`: the id of the rolled table, or
    /// [`UNKNOWN_LOOT_TABLE`].
    #[must_use]
    pub const fn queried_loot_table_id(&self) -> &str {
        self.queried_loot_table_id
    }

    /// `LootContext.getRandom`: the random source of the roll.
    pub const fn random(&mut self) -> &mut Xoroshiro {
        self.random
    }

    /// `LootTable.getRandomItemsRaw` of the table `id` with this context: no modifiers run on the
    /// result. An unknown table yields nothing.
    pub fn roll_loot_table_raw(&mut self, id: &str) -> Vec<ItemStack> {
        get_context_loot_table(self.params, id).map_or_else(Vec::new, |handle| {
            generation::generate_with_rng((&handle).into(), self.random, self.params)
        })
    }

    fn test(&mut self, condition: &ModifierCondition) -> bool {
        match condition {
            ModifierCondition::LootTableId(id) => self.queried_loot_table_id == id,
            ModifierCondition::Loot(condition) => {
                let params = self.params;
                let has_silk_touch = params.tool.as_ref().is_some_and(|tool| {
                    tool.get_enchantment_level(&pumpkin_data::Enchantment::SILK_TOUCH) > 0
                });
                let has_shears = params
                    .tool
                    .as_ref()
                    .is_some_and(|tool| tool.item == &Item::SHEARS);
                check_dynamic_condition(condition, has_silk_touch, has_shears, params, self.random)
            }
            ModifierCondition::AllOf(terms) => terms.iter().all(|term| self.test(term)),
            ModifierCondition::AnyOf(terms) => terms.iter().any(|term| self.test(term)),
            ModifierCondition::Inverted(term) => !self.test(term),
        }
    }
}

/// A modifier condition: the loot conditions of Pumpkin and `neoforge:loot_table_id`, which
/// the composite conditions can hold.
enum ModifierCondition {
    LootTableId(String),
    Loot(DynamicLootCondition),
    AllOf(Vec<Self>),
    AnyOf(Vec<Self>),
    Inverted(Box<Self>),
}

impl ModifierCondition {
    fn parse(
        value: &Value,
        predicates: &dyn Fn(&str) -> Option<Value>,
        depth: usize,
    ) -> Option<Self> {
        if depth >= MAX_CONDITION_DEPTH {
            return None;
        }
        let reference =
            |id: &str| Self::parse(&predicates(&normalize_id(id)?)?, predicates, depth + 1);
        if let Some(id) = value.as_str() {
            return reference(id);
        }
        let terms = |key| -> Option<Vec<Self>> {
            value
                .get(key)?
                .as_array()?
                .iter()
                .map(|term| Self::parse(term, predicates, depth + 1))
                .collect()
        };
        match value
            .get("type")
            .and_then(Value::as_str)
            .and_then(normalize_id)
        {
            Some(kind) if kind == LOOT_TABLE_ID => Some(Self::LootTableId(normalize_id(
                value.get("loot_table_id")?.as_str()?,
            )?)),
            Some(kind) if kind == "minecraft:all_of" => Some(Self::AllOf(terms("terms")?)),
            Some(kind) if kind == "minecraft:any_of" => Some(Self::AnyOf(terms("terms")?)),
            Some(kind) if kind == "minecraft:inverted" => Some(Self::Inverted(Box::new(
                Self::parse(value.get("term")?, predicates, depth + 1)?,
            ))),
            Some(kind) if kind == "minecraft:reference" => reference(value.get("name")?.as_str()?),
            _ => parse_condition_with_predicates(value, &|name| {
                normalize_id(name).and_then(|name| predicates(&name))
            })
            .map(Self::Loot),
        }
    }
}

/// `AddTableLootModifier` (`neoforge:add_table`): adds the loot of the table `table`, rolled raw
/// with the context of the roll.
pub struct AddTableLootModifier {
    table: String,
}

impl AddTableLootModifier {
    fn decode(json: &Value) -> Result<Box<dyn GlobalLootModifier>, String> {
        let table = json
            .get("table")
            .and_then(Value::as_str)
            .and_then(normalize_id)
            .ok_or("no table")?;
        Ok(Box::new(Self { table }))
    }
}

impl GlobalLootModifier for AddTableLootModifier {
    fn apply(
        &self,
        mut generated_loot: Vec<ItemStack>,
        context: &mut LootContext<'_, '_>,
    ) -> Vec<ItemStack> {
        // Raw, as `AddTableLootModifier.doApply`: the modifiers of the target table see the added
        // loot, so modifying the subtable too would modify it twice.
        generated_loot.extend(context.roll_loot_table_raw(&self.table));
        generated_loot
    }
}

#[cfg(test)]
mod tests;
