use super::random_sequences::CheckedOutSequence;
use crate::{block::entities::BlockEntity, entity::EntityBase, world::World};
use pumpkin_data::BlockState;
use pumpkin_data::damage::DamageType;
use pumpkin_data::data_component_impl::EnchantmentsImpl;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::recipes::CookingRecipeKind;
use pumpkin_data::tag::Taggable;
pub use pumpkin_util::loot_table::{
    DynamicLootCondition, DynamicLootEntry, DynamicLootEntryKind, DynamicLootFunction,
    DynamicLootPool, DynamicLootTable, LootBonusFormula, LootCondition, LootEntityTarget,
    LootEntry, LootEntryKind, LootFunction, LootFunctionKind, LootNumberProvider, LootPool,
    LootTable,
};
use pumpkin_util::random::{RandomGenerator, RandomImpl, legacy_rand::LegacyRand};

mod generation;
pub mod modifiers;
mod predicates;

/// `LootContextParams.DAMAGE_SOURCE`: the damage of a kill.
///
/// `DamageSource` of vanilla also holds the causing entity and the direct entity: here they are
/// `attacker` and `direct`, the values of `LootContextParams.ATTACKING_ENTITY` and
/// `DIRECT_ATTACKING_ENTITY`.
#[derive(Clone, Copy)]
pub struct LootDamageSource<'a> {
    pub damage_type: DamageType,
    pub attacker: Option<&'a dyn EntityBase>,
    pub direct: Option<&'a dyn EntityBase>,
}

/// The parameters of a loot roll: `LootParams` of vanilla.
///
/// There is one field per `LootContextParams` key (`position` is `ORIGIN`, `killer_entity` is
/// `ATTACKING_ENTITY`, `direct_killer_entity` is `DIRECT_ATTACKING_ENTITY`, `tool` is `TOOL`).
/// A field that a roll does not set is `None`.
#[derive(Default, Clone)]
pub struct LootContextParameters<'a> {
    pub world: Option<&'a World>,
    pub explosion_radius: Option<f32>,
    pub block_state: Option<&'static BlockState>,
    pub luck: f32,
    pub this_entity: Option<&'a dyn EntityBase>,
    pub killer_entity: Option<&'a dyn EntityBase>,
    pub direct_killer_entity: Option<&'a dyn EntityBase>,
    pub position: Option<pumpkin_util::math::vector3::Vector3<f64>>,
    pub world_time: u64,
    pub damage_type: Option<DamageType>,
    pub tool: Option<ItemStack>,
    pub is_raining: Option<bool>,
    pub is_thundering: Option<bool>,
    /// Whether the killed entity was on fire at death time.
    /// Computed from `Entity.fire_ticks > 0`.
    pub is_on_fire: Option<bool>,
    pub direct_attacker_weapon: Option<ItemStack>,
    pub enchantment_level: Option<i32>,
    /// `LAST_DAMAGE_PLAYER`: the player that last damaged the killed entity. The condition
    /// `killed_by_player` tests this field only.
    pub last_damage_player: Option<&'a dyn EntityBase>,
    /// `DAMAGE_SOURCE`.
    pub damage_source: Option<LootDamageSource<'a>>,
    /// `INTERACTING_ENTITY`: the entity that uses a block or an entity.
    pub interacting_entity: Option<&'a dyn EntityBase>,
    /// `TARGET_ENTITY`: the entity that an interaction or a command targets.
    pub target_entity: Option<&'a dyn EntityBase>,
    /// `BLOCK_ENTITY`: the block entity of the block of the roll.
    pub block_entity: Option<&'a dyn BlockEntity>,
}

impl<'a> LootContextParameters<'a> {
    /// `ATTACKING_ENTITY`: `killer_entity`, else the attacker of `damage_source`.
    fn attacker(&self) -> Option<&'a dyn EntityBase> {
        self.killer_entity
            .or_else(|| self.damage_source.and_then(|source| source.attacker))
    }

    /// `DIRECT_ATTACKING_ENTITY`: `direct_killer_entity`, else the direct entity of
    /// `damage_source`.
    fn direct_attacker(&self) -> Option<&'a dyn EntityBase> {
        self.direct_killer_entity
            .or_else(|| self.damage_source.and_then(|source| source.direct))
    }
}

impl LootContextParameters<'_> {
    /// The damage type of the roll: `damage_type`, else the type of `damage_source`.
    fn resolved_damage_type(&self) -> Option<DamageType> {
        self.damage_type
            .or_else(|| self.damage_source.map(|source| source.damage_type))
    }
}

impl<'a> LootContextParameters<'a> {
    /// `Block.dropFromBlockInteractLootTable`: the `block_interact` parameter set. `state` is the
    /// state of the block before the interaction changes it, and `position` is its position.
    #[must_use]
    pub fn block_interact(
        world: &'a World,
        position: &pumpkin_util::math::position::BlockPos,
        state: &'static BlockState,
        block_entity: Option<&'a dyn BlockEntity>,
        interacting_entity: Option<&'a dyn EntityBase>,
        tool: Option<ItemStack>,
    ) -> Self {
        Self {
            world: Some(world),
            position: Some(position.to_centered_f64()),
            block_state: Some(state),
            block_entity,
            interacting_entity,
            tool,
            ..Default::default()
        }
    }

    /// `LivingEntity.dropFromEntityInteractLootTable`: the `entity_interact` parameter set.
    #[must_use]
    pub fn entity_interact(
        world: &'a World,
        target: &'a dyn EntityBase,
        interacting_entity: Option<&'a dyn EntityBase>,
        tool: ItemStack,
    ) -> Self {
        Self {
            world: Some(world),
            target_entity: Some(target),
            interacting_entity,
            tool: Some(tool),
            ..Default::default()
        }
    }
}

/// `Player.getLuck`: the luck of the player, `withLuck` of the vanilla loot params.
#[must_use]
pub fn player_luck(player: &crate::entity::player::Player) -> f32 {
    player
        .living_entity
        .get_attribute_value(&pumpkin_data::attributes::Attributes::LUCK) as f32
}

fn enchantment_level_chance(
    base: f32,
    per_level_above_first: f32,
    params: &LootContextParameters<'_>,
) -> f32 {
    let level = params.enchantment_level.unwrap_or(0);
    per_level_above_first.mul_add((level - 1) as f32, base)
}

fn attacker_enchantment_level(params: &LootContextParameters<'_>, name: &str) -> i32 {
    let enchantment = pumpkin_data::Enchantment::from_name(name);
    let Some(enchantment) = enchantment else {
        return 0;
    };
    let entity_level = params
        .killer_entity
        .or(params.direct_killer_entity)
        .map_or(0, |entity| {
            crate::enchantment::helper::EnchantmentHelper::get_enchantment_level(
                enchantment,
                entity,
            )
        });
    let weapon_level = params
        .direct_attacker_weapon
        .as_ref()
        .filter(|weapon| !weapon.is_empty())
        .map_or(0, |weapon| weapon.get_enchantment_level(enchantment));
    entity_level.max(weapon_level)
}

fn tool_enchantment_level(params: &LootContextParameters<'_>, name: &str) -> i32 {
    pumpkin_data::Enchantment::from_name(name)
        .and_then(|enchantment| {
            params
                .tool
                .as_ref()
                .filter(|tool| !tool.is_empty())
                .map(|tool| tool.get_enchantment_level(enchantment))
        })
        .unwrap_or(0)
}

fn direct_attacker_weapon_has_enchantment(params: &LootContextParameters, tag: &str) -> bool {
    params
        .direct_attacker_weapon
        .as_ref()
        .and_then(|weapon| weapon.get_data_component::<EnchantmentsImpl>())
        .is_some_and(|enchantments| {
            enchantments
                .enchantment
                .iter()
                .any(|(enchantment, _)| enchantment.is_tagged_with(tag) == Some(true))
        })
}

/// Vanilla `SmeltItemFunction.run`: the result of the smelting recipe for `count` items of
/// `item`, with the components of the result, at most a full stack. `None` without a recipe.
fn smelt(item: &'static Item, count: i32, use_input_count: bool) -> Option<ItemStack> {
    let result = pumpkin_inventory::recipe_index::current()
        .cooking_recipe(item, &CookingRecipeKind::Smelting)
        .map(|recipe| recipe.result.assemble())
        .filter(|result| !result.is_empty());
    let Some(mut result) = result else {
        tracing::warn!(
            "Couldn't smelt {} because there is no smelting recipe",
            item.registry_key
        );
        return None;
    };
    let inputs = if use_input_count { count } else { 1 };
    let count = inputs
        .saturating_mul(i32::from(result.item_count))
        .min(i32::from(result.get_max_stack_size()));
    result.item_count = u8::try_from(count.max(0)).unwrap_or(u8::MAX);
    Some(result)
}

fn weather_matches(
    params: &LootContextParameters<'_>,
    raining: Option<bool>,
    thundering: Option<bool>,
) -> bool {
    raining.is_none_or(|value| {
        params
            .is_raining
            .or_else(|| params.world.map(World::is_raining))
            .unwrap_or(false)
            == value
    }) && thundering.is_none_or(|value| {
        params
            .is_thundering
            .or_else(|| params.world.map(World::is_thunderstorm))
            .unwrap_or(false)
            == value
    })
}

fn check_dynamic_condition(
    cond: &DynamicLootCondition,
    has_silk_touch: bool,
    has_shears: bool,
    params: &LootContextParameters,
    rng: &mut RandomGenerator,
) -> bool {
    match cond {
        DynamicLootCondition::None => true,
        DynamicLootCondition::Never => false,
        DynamicLootCondition::LocationCheck { offset, predicate } => {
            predicates::matches_location(*offset, predicate, params)
        }
        DynamicLootCondition::MatchBlock(predicate) => predicates::matches_block(predicate, params),
        DynamicLootCondition::MatchTool(predicate) => predicates::matches_tool(predicate, params),
        DynamicLootCondition::WeatherCheck {
            raining,
            thundering,
        } => weather_matches(params, *raining, *thundering),
        DynamicLootCondition::EntityProperties { target, predicate } => {
            predicates::matches_entity(*target, predicate, params)
        }
        DynamicLootCondition::DamageSourceTag { tag, expected } => params
            .resolved_damage_type()
            .is_some_and(|damage| damage.is_tagged_with(tag) == Some(*expected)),
        DynamicLootCondition::SilkTouch => has_silk_touch,
        DynamicLootCondition::NoSilkTouch => !has_silk_touch,
        DynamicLootCondition::Shears => has_shears,
        DynamicLootCondition::SilkTouchOrShears => has_silk_touch || has_shears,
        DynamicLootCondition::NoSilkTouchOrShears => !has_silk_touch && !has_shears,
        DynamicLootCondition::KilledByPlayer => params.last_damage_player.is_some(),
        DynamicLootCondition::SurvivesExplosion => params
            .explosion_radius
            .is_none_or(|radius| radius <= 1.0 || rng.next_f32() <= 1.0 / radius),
        DynamicLootCondition::RandomChance { chance } => rng.next_f32() < *chance,
        DynamicLootCondition::RandomChanceWithEnchantmentLevel {
            base,
            per_level_above_first,
        } => rng.next_f32() < enchantment_level_chance(*base, *per_level_above_first, params),
        DynamicLootCondition::RandomChanceWithEnchantedBonus {
            enchantment,
            unenchanted_chance,
            enchanted_chance_base,
            enchanted_chance_per_level_above_first,
        } => {
            let level = attacker_enchantment_level(params, enchantment);
            let chance = if level > 0 {
                enchanted_chance_base + enchanted_chance_per_level_above_first * (level - 1) as f32
            } else {
                *unenchanted_chance
            };
            rng.next_f32() < chance
        }
        DynamicLootCondition::RandomChanceWithLooting {
            chance,
            looting_multiplier,
        } => {
            let level = attacker_enchantment_level(params, "minecraft:looting");
            let chance = *chance + (level as f32) * (*looting_multiplier);
            rng.next_f32() < chance
        }
        DynamicLootCondition::TableBonus {
            enchantment,
            chances,
        } => {
            let index = (tool_enchantment_level(params, enchantment).max(0) as usize)
                .min(chances.len().saturating_sub(1));
            chances
                .get(index)
                .is_some_and(|chance| rng.next_f32() < *chance)
        }
        DynamicLootCondition::AllOf(conditions) => conditions
            .iter()
            .all(|c| check_dynamic_condition(c, has_silk_touch, has_shears, params, rng)),
        DynamicLootCondition::AnyOf(conditions) => conditions
            .iter()
            .any(|c| check_dynamic_condition(c, has_silk_touch, has_shears, params, rng)),
        DynamicLootCondition::Inverted(cond) => {
            !check_dynamic_condition(cond, has_silk_touch, has_shears, params, rng)
        }
        DynamicLootCondition::EntityOnFire => params.is_on_fire.unwrap_or_else(|| {
            params
                .this_entity
                .is_some_and(|entity| entity.get_entity().is_on_fire())
        }),
        DynamicLootCondition::DirectAttackerWeaponEnchantment(tag) => {
            direct_attacker_weapon_has_enchantment(params, tag)
        }
        DynamicLootCondition::DamageSourceProperties(predicate) => {
            predicates::matches_damage_source(predicate, params)
        }
        DynamicLootCondition::TimeCheck { value, period } => {
            predicates::matches_time(value, *period, params)
        }
        DynamicLootCondition::ValueCheck { value, range } => {
            predicates::matches_value(*value, range, params, rng)
        }
        DynamicLootCondition::Reference(name) => evaluate_predicate(name, params),
    }
}

fn check_condition(
    cond: LootCondition,
    has_silk_touch: bool,
    has_shears: bool,
    params: &LootContextParameters,
    rng: &mut RandomGenerator,
) -> bool {
    match cond {
        LootCondition::None => true,
        LootCondition::Never => false,
        LootCondition::LocationCheck { offset, predicate } => {
            predicates::matches_location(offset, predicate, params)
        }
        LootCondition::MatchBlock(predicate) => predicates::matches_block(predicate, params),
        LootCondition::MatchTool(predicate) => predicates::matches_tool(predicate, params),
        LootCondition::WeatherCheck {
            raining,
            thundering,
        } => weather_matches(params, raining, thundering),
        LootCondition::EntityProperties { target, predicate } => {
            predicates::matches_entity(target, predicate, params)
        }
        LootCondition::DamageSourceTag { tag, expected } => params
            .resolved_damage_type()
            .is_some_and(|damage| damage.is_tagged_with(tag) == Some(expected)),
        LootCondition::Inverted(condition) => {
            !check_condition(*condition, has_silk_touch, has_shears, params, rng)
        }
        LootCondition::SilkTouch => has_silk_touch,
        LootCondition::NoSilkTouch => !has_silk_touch,
        LootCondition::Shears => has_shears,
        LootCondition::SilkTouchOrShears => has_silk_touch || has_shears,
        LootCondition::NoSilkTouchOrShears => !has_silk_touch && !has_shears,
        LootCondition::KilledByPlayer => params.last_damage_player.is_some(),
        LootCondition::SurvivesExplosion => params
            .explosion_radius
            .is_none_or(|radius| radius <= 1.0 || rng.next_f32() <= 1.0 / radius),
        LootCondition::RandomChance { chance } => rng.next_f32() < chance,
        LootCondition::RandomChanceWithEnchantmentLevel {
            base,
            per_level_above_first,
        } => rng.next_f32() < enchantment_level_chance(base, per_level_above_first, params),
        LootCondition::RandomChanceWithEnchantedBonus {
            enchantment,
            unenchanted_chance,
            enchanted_chance_base,
            enchanted_chance_per_level_above_first,
        } => {
            let level = attacker_enchantment_level(params, enchantment);
            let chance = if level > 0 {
                enchanted_chance_base + enchanted_chance_per_level_above_first * (level - 1) as f32
            } else {
                unenchanted_chance
            };
            rng.next_f32() < chance
        }
        LootCondition::RandomChanceWithLooting {
            chance,
            looting_multiplier,
        } => {
            let level = attacker_enchantment_level(params, "minecraft:looting");
            let chance = chance + (level as f32) * looting_multiplier;
            rng.next_f32() < chance
        }
        LootCondition::TableBonus {
            enchantment,
            chances,
        } => {
            let index = (tool_enchantment_level(params, enchantment).max(0) as usize)
                .min(chances.len().saturating_sub(1));
            chances
                .get(index)
                .is_some_and(|chance| rng.next_f32() < *chance)
        }
        LootCondition::AllOf(conditions) => conditions
            .iter()
            .all(|c| check_condition(*c, has_silk_touch, has_shears, params, rng)),
        LootCondition::AnyOf(conditions) => conditions
            .iter()
            .any(|c| check_condition(*c, has_silk_touch, has_shears, params, rng)),
        LootCondition::EntityOnFire => params.is_on_fire.unwrap_or_else(|| {
            params
                .this_entity
                .is_some_and(|entity| entity.get_entity().is_on_fire())
        }),
        LootCondition::DirectAttackerWeaponEnchantment(tag) => {
            direct_attacker_weapon_has_enchantment(params, tag)
        }
        LootCondition::DamageSourceProperties(predicate) => {
            predicates::matches_damage_source(predicate, params)
        }
        LootCondition::TimeCheck { value, period } => {
            predicates::matches_time(value, period, params)
        }
        LootCondition::ValueCheck { value, range } => {
            predicates::matches_value(value, range, params, rng)
        }
        LootCondition::Reference(name) => evaluate_predicate(name, params),
    }
}

pub(crate) fn test_loot_condition(
    condition: LootCondition,
    params: &LootContextParameters<'_>,
) -> bool {
    let has_silk_touch = params
        .tool
        .as_ref()
        .is_some_and(|tool| tool.get_enchantment_level(&pumpkin_data::Enchantment::SILK_TOUCH) > 0);
    let has_shears = params
        .tool
        .as_ref()
        .is_some_and(|tool| tool.item.id == pumpkin_data::item::Item::SHEARS.id);
    let mut rng = random_source(rand::random());
    check_condition(condition, has_silk_touch, has_shears, params, &mut rng)
}

fn apply_bonus_formula(
    base_count: i32,
    bonus: LootBonusFormula,
    fortune_level: i32,
    rng: &mut RandomGenerator,
) -> i32 {
    match bonus {
        LootBonusFormula::OreDrops => {
            if fortune_level > 0 {
                let bonus = (rng.next_bounded_i32(fortune_level + 2) - 1).max(0);
                base_count * (bonus + 1)
            } else {
                base_count
            }
        }
        LootBonusFormula::UniformBonusCount(bonus_multiplier) => {
            let bound = fortune_level
                .saturating_mul(bonus_multiplier)
                .saturating_add(1);
            if bound > 0 {
                base_count.saturating_add(rng.next_bounded_i32(bound))
            } else {
                base_count
            }
        }
        LootBonusFormula::BinomialWithBonusCount { extra, probability } => {
            let n = fortune_level + extra;
            let mut bonus_count = 0;
            for _ in 0..n {
                if rng.next_f32() < probability {
                    bonus_count += 1;
                }
            }
            base_count + bonus_count
        }
    }
}

#[must_use]
pub fn generate_loot(table: &LootTable, seed: i64) -> Vec<ItemStack> {
    generate_loot_with_context(table, seed, &LootContextParameters::default())
}
#[must_use]
pub fn generate_loot_with_context(
    table: &LootTable,
    seed: i64,
    params: &LootContextParameters<'_>,
) -> Vec<ItemStack> {
    generation::generate(generation::Table::Static(table), seed, params)
}
pub use generate_loot as generate_chest_loot;
#[must_use]
pub fn generate_dynamic_loot(table: &DynamicLootTable, seed: i64) -> Vec<ItemStack> {
    generate_dynamic_loot_with_context(table, seed, &LootContextParameters::default())
}
#[must_use]
pub fn generate_dynamic_loot_with_context(
    table: &DynamicLootTable,
    seed: i64,
    params: &LootContextParameters<'_>,
) -> Vec<ItemStack> {
    generation::generate(generation::Table::Dynamic(table), seed, params)
}

/// Either a compile-time static loot table or a datapack dynamic loot table.
#[derive(Clone, Debug)]
pub enum LootTableRef {
    Static(&'static LootTable),
    Dynamic(std::sync::Arc<DynamicLootTable>),
}

/// A loot table and the id it was looked up by, which loot modifiers test.
#[derive(Clone, Debug)]
pub struct LootTableHandle {
    pub table: LootTableRef,
    /// `None` for a table that no id names.
    pub id: Option<String>,
}

impl LootTableHandle {
    /// The `random_sequence` of the table: the id of the server random sequence that a roll
    /// without a seed draws from. The tables of the vanilla data name their own id.
    #[must_use]
    pub fn random_sequence(&self) -> Option<&str> {
        match &self.table {
            LootTableRef::Static(_) => self.id.as_deref(),
            LootTableRef::Dynamic(table) => table.random_sequence.as_deref(),
        }
    }

    #[must_use]
    pub const fn new(table: LootTableRef, id: String) -> Self {
        Self {
            table,
            id: Some(id),
        }
    }
}

impl From<&'static LootTable> for LootTableHandle {
    fn from(t: &'static LootTable) -> Self {
        Self {
            table: LootTableRef::Static(t),
            id: None,
        }
    }
}

impl From<std::sync::Arc<DynamicLootTable>> for LootTableHandle {
    fn from(t: std::sync::Arc<DynamicLootTable>) -> Self {
        Self {
            table: LootTableRef::Dynamic(t),
            id: None,
        }
    }
}

impl LootTableHandle {
    #[must_use]
    pub fn generate_loot(&self, seed: i64) -> Vec<ItemStack> {
        self.generate_loot_with_context(seed, &LootContextParameters::default())
    }

    #[must_use]
    pub fn generate_loot_with_context(
        &self,
        seed: i64,
        params: &LootContextParameters,
    ) -> Vec<ItemStack> {
        generate_loot_from_handle(self, seed, params)
    }
}

thread_local! {
    static ACTIVE_PREDICATES: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

struct PredicateGuard(String);

impl Drop for PredicateGuard {
    fn drop(&mut self) {
        ACTIVE_PREDICATES.with(|set| {
            set.borrow_mut().remove(&self.0);
        });
    }
}

#[must_use]
pub fn evaluate_predicate(predicate_name: &str, params: &LootContextParameters<'_>) -> bool {
    let full_key = if predicate_name.contains(':') {
        predicate_name.to_string()
    } else {
        format!("minecraft:{predicate_name}")
    };
    let inserted = ACTIVE_PREDICATES.with(|set| set.borrow_mut().insert(full_key.clone()));
    if !inserted {
        tracing::warn!("Recursive loot predicate evaluation detected for '{full_key}'");
        return false;
    }
    let _guard = PredicateGuard(full_key.clone());

    if let Some(world) = params.world
        && let Some(server) = world.server.upgrade()
        && let Some(val) = server
            .datapack_manager
            .get_predicate(&full_key)
            .or_else(|| server.datapack_manager.get_predicate(predicate_name))
    {
        return evaluate_predicate_value(&val, params);
    }
    pumpkin_data::loot_table::get_loot_predicate(&full_key)
        .or_else(|| pumpkin_data::loot_table::get_loot_predicate(predicate_name))
        .is_some_and(|val| evaluate_predicate_value(val, params))
}

#[must_use]
pub fn evaluate_predicate_value(
    value: &serde_json::Value,
    params: &LootContextParameters<'_>,
) -> bool {
    pumpkin_util::loot_table::parser::parse_condition_with_predicates(value, &|name| {
        pumpkin_data::loot_table::get_loot_predicate(name).cloned()
    })
    .map_or_else(
        || {
            if value.is_object() && params.this_entity.is_some() {
                predicates::matches_entity(LootEntityTarget::This, value, params)
            } else {
                false
            }
        },
        |cond| {
            let has_silk_touch = params.tool.as_ref().is_some_and(|tool| {
                tool.get_enchantment_level(&pumpkin_data::Enchantment::SILK_TOUCH) > 0
            });
            let has_shears = params
                .tool
                .as_ref()
                .is_some_and(|tool| tool.item.id == pumpkin_data::item::Item::SHEARS.id);
            let mut rng = random_source(rand::random());
            check_dynamic_condition(&cond, has_silk_touch, has_shears, params, &mut rng)
        },
    )
}

/// The table `key` of the world of `params`, so data pack tables apply, or the vanilla table
/// when `params` has no world.
#[must_use]
pub fn get_context_loot_table(
    params: &LootContextParameters<'_>,
    key: &str,
) -> Option<LootTableHandle> {
    params
        .world
        .map_or_else(|| get_loot_table(key), |world| world.get_loot_table(key))
}

#[must_use]
pub fn get_loot_table(key: &str) -> Option<LootTableHandle> {
    let full_key = if key.contains(':') {
        key.to_string()
    } else {
        format!("minecraft:{key}")
    };
    let table = pumpkin_data::loot_table::get_loot_table(key)
        .or_else(|| pumpkin_data::loot_table::get_loot_table(&full_key))?;
    Some(LootTableHandle::new(LootTableRef::Static(table), full_key))
}

#[must_use]
pub fn generate_loot_from_handle(
    handle: &LootTableHandle,
    seed: i64,
    params: &LootContextParameters,
) -> Vec<ItemStack> {
    generate_loot_from_handle_with_rng(handle, &mut random_source(seed), params)
}

/// [`generate_loot_from_handle`] on the random source `rng`, which the caller goes on with.
#[must_use]
pub fn generate_loot_from_handle_with_rng(
    handle: &LootTableHandle,
    rng: &mut RandomGenerator,
    params: &LootContextParameters,
) -> Vec<ItemStack> {
    if let Some(modifiers) = modifiers::active(params) {
        return modifiers::generate_with_rng(handle, rng, params, &modifiers);
    }
    generation::generate_with_rng(handle.into(), rng, params)
}

/// Vanilla `RandomSource.create(seed)`: the legacy random source of an explicit seed. The loot
/// code draws from it where vanilla does, so a roll with a seed gives the vanilla items.
#[must_use]
pub const fn random_source(seed: i64) -> RandomGenerator {
    RandomGenerator::Legacy(LegacyRand::from_seed(seed as u64))
}

/// Vanilla `LootTable.getRandomItems(params, optionalLootTableSeed)`.
///
/// The roll uses the random source of [`RollRandom::new`]. A seed other than 0 gives a source of
/// its own. The seed 0 gives the random sequence of the table, which goes on where the last roll
/// of the sequence ended. `getRandomItems(params)` is the seed 0.
#[must_use]
pub fn roll_loot(
    handle: &LootTableHandle,
    optional_seed: i64,
    params: &LootContextParameters<'_>,
) -> Vec<ItemStack> {
    let mut random = RollRandom::new(params, handle, optional_seed);
    generate_loot_from_handle_with_rng(handle, random.rng(), params)
}

/// The random source of a container roll.
///
/// `LootTable.fill` and `getRandomItems(params, seed)` create it with
/// `LootContext.Builder.withOptionalRandomSeed(seed)` and `create(randomSequence)`: a seed other
/// than 0 gives a source of its own, and the seed 0 gives the random sequence of the table, or a
/// fresh source (the level random) when the table has none. The roll and the placement of the
/// loot draw from the same source.
///
/// A source from a sequence has the sequence checked out until it drops: another roll of the
/// same sequence waits, so two rolls never draw the same values, and the sequence goes on where
/// the roll ended. A `/random reset` during the roll replaces the sequence, and the roll ends on
/// the old state.
pub struct RollRandom {
    source: RollSource,
}

enum RollSource {
    Own(RandomGenerator),
    Sequence(CheckedOutSequence),
}

impl RollRandom {
    /// A source of its own for `seed`: the roll of a container with no table to look up.
    #[must_use]
    pub const fn seeded(seed: i64) -> Self {
        Self {
            source: RollSource::Own(random_source(seed)),
        }
    }

    /// The source of a roll of `table` with `seed`, in the world of `params`.
    #[must_use]
    pub fn new(params: &LootContextParameters<'_>, table: &LootTableHandle, seed: i64) -> Self {
        if seed != 0 {
            return Self::seeded(seed);
        }
        let link = table
            .random_sequence()
            .and_then(|id| pumpkin_util::identifier::Identifier::parse(id).ok())
            .zip(params.world.and_then(|world| world.server.upgrade()));
        let Some((id, server)) = link else {
            return Self::seeded(rand::random());
        };
        let world_seed = server.level_info.load().world_gen_settings.seed;
        // The handle leaves the manager lock before the check out waits for another roll.
        let handle = server
            .random_sequences
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get_or_create(&id, world_seed)
            .handle();
        Self {
            source: RollSource::Sequence(handle.check_out()),
        }
    }

    /// The random source of the roll.
    pub const fn rng(&mut self) -> &mut RandomGenerator {
        match &mut self.source {
            RollSource::Own(rng) => rng,
            RollSource::Sequence(sequence) => sequence.rng(),
        }
    }
}

/// Places loot into empty slots of `slots` with the random source of its roll. Occupied slots
/// stay.
pub fn place_items_in_slots(
    slots: &mut [ItemStack],
    items_to_place: Vec<ItemStack>,
    rng: &mut RandomGenerator,
) {
    let empty_slots = slots
        .iter()
        .enumerate()
        .filter(|(_, stack)| stack.is_empty())
        .map(|(slot, _)| slot)
        .collect();
    place_items(empty_slots, items_to_place, rng, |slot, item| {
        slots[slot] = item;
    });
}

/// Vanilla `RandomizableContainer.unpackLootTable` lock protocol for concurrent containers.
///
/// Takes the pending loot-table key so another unpack cannot run, runs `generate` with neither
/// chest lock held, then places into still-empty slots so concurrent inserts stay. `generate`
/// gets the key and the seed of the container and returns the loot with the random source of its
/// roll: as `LootTable.fill`, the placement goes on with that source.
pub fn unpack_loot_into_slots<const N: usize>(
    loot_table: &std::sync::Mutex<Option<String>>,
    seed: i64,
    items: &std::sync::RwLock<[ItemStack; N]>,
    generate: impl FnOnce(String, i64) -> (Vec<ItemStack>, RollRandom),
) -> Option<String> {
    let key = {
        let mut loot = loot_table
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loot.take()?
    };
    let (generated, mut random) = generate(key.clone(), seed);
    let mut slots = items
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    place_items_in_slots(&mut *slots, generated, random.rng());
    Some(key)
}

fn place_items_in_chest(
    inventory: &std::sync::Arc<dyn pumpkin_inventory::Inventory>,
    items_to_place: Vec<ItemStack>,
    rng: &mut RandomGenerator,
) {
    let empty_slots = (0..inventory.size())
        .filter(|&slot| inventory.get_stack(slot).is_empty())
        .collect();
    place_items(empty_slots, items_to_place, rng, |slot, item| {
        inventory.set_stack(slot, item);
    });
}

/// Vanilla `LootTable.fill` placement: shuffled empty slots, split stacks, last slot first.
fn place_items(
    mut empty_slots: Vec<usize>,
    mut items_to_place: Vec<ItemStack>,
    rng: &mut RandomGenerator,
    mut set: impl FnMut(usize, ItemStack),
) {
    if items_to_place.is_empty() {
        return;
    }

    for i in (1..empty_slots.len()).rev() {
        let j = rng.next_bounded_i32((i + 1) as i32) as usize;
        empty_slots.swap(i, j);
    }

    shuffle_and_split_items(&mut items_to_place, empty_slots.len(), rng);

    for item in items_to_place {
        let Some(slot) = empty_slots.pop() else {
            tracing::warn!("Tried to over-fill a container");
            return;
        };
        set(slot, item);
    }
}

pub fn fill_chest_inventory_handle(
    inventory: &std::sync::Arc<dyn pumpkin_inventory::Inventory>,
    handle: &LootTableHandle,
    seed: i64,
    params: &LootContextParameters<'_>,
) {
    let mut random = RollRandom::new(params, handle, seed);
    let items_to_place = generate_loot_from_handle_with_rng(handle, random.rng(), params);
    place_items_in_chest(inventory, items_to_place, random.rng());
}

pub fn fill_chest_inventory(
    inventory: &std::sync::Arc<dyn pumpkin_inventory::Inventory>,
    table: &LootTable,
    seed: i64,
) {
    let mut rng = random_source(seed);
    let items_to_place = generation::generate_with_rng(
        generation::Table::Static(table),
        &mut rng,
        &LootContextParameters::default(),
    );
    place_items_in_chest(inventory, items_to_place, &mut rng);
}

fn shuffle_and_split_items(
    result: &mut Vec<ItemStack>,
    available_slots: usize,
    rng: &mut RandomGenerator,
) {
    let mut splittable: Vec<ItemStack> = Vec::new();
    let mut i = 0;
    while i < result.len() {
        if result[i].is_empty() {
            result.swap_remove(i);
        } else if result[i].item_count > 1 {
            splittable.push(result.swap_remove(i));
        } else {
            i += 1;
        }
    }

    while available_slots > result.len() + splittable.len() && !splittable.is_empty() {
        let idx = rng.next_bounded_i32(splittable.len() as i32) as usize;
        let mut stack = splittable.swap_remove(idx);

        let count = stack.item_count as i32;
        let split_off = 1 + rng.next_bounded_i32(count / 2);
        stack.item_count = (count - split_off) as u8;
        let mut copy = stack.clone();
        copy.item_count = split_off as u8;

        if stack.item_count > 1 && rng.next_bool() {
            splittable.push(stack);
        } else {
            result.push(stack);
        }
        if copy.item_count > 1 && rng.next_bool() {
            splittable.push(copy);
        } else {
            result.push(copy);
        }
    }

    result.extend(splittable);

    let n = result.len();
    for i in (1..n).rev() {
        let j = rng.next_bounded_i32((i + 1) as i32) as usize;
        result.swap(i, j);
    }
}

pub(crate) fn add_stew_effect(
    stack: &mut ItemStack,
    effect: &pumpkin_data::effect::StatusEffect,
    duration: i32,
) {
    use pumpkin_data::data_component_impl::{SuspiciousStewEffect, SuspiciousStewEffectsImpl};
    let duration = if effect.instantaneous {
        duration
    } else {
        duration * 20
    };
    if let Some(effects) = stack.get_data_component_mut::<SuspiciousStewEffectsImpl>() {
        effects.effects.to_mut().push(SuspiciousStewEffect {
            effect: effect.minecraft_name.into(),
            duration,
        });
    }
}

#[cfg(test)]
mod tests;
