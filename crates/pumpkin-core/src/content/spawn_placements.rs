//! Spawn placements that native mods register, the backing of `NeoForge`
//! `RegisterSpawnPlacementsEvent` and its `MergedSpawnPredicate`.
//!
//! A vanilla type with an entry in `SpawnPlacements` (`EntityType::has_spawn_placement`) has a
//! placement: its spawn restriction and the rules of
//! [`check_spawn_rules`](crate::entity::r#type::check_spawn_rules). A custom type and a vanilla
//! type without an entry (78 of the 161 vanilla types, among them `allay`, `bee`, `player` and
//! every boat, minecart, projectile and display) have one after their first registration, which
//! must name the placement type and the heightmap. Mods register by
//! name before the freeze. On the first lookup after it the placements become a table by entity
//! type id.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use pumpkin_data::dynamic::{self, ContentKind};
use pumpkin_data::entity::{EntityType, SpawnLocation, SpawnRestriction};
use pumpkin_util::HeightMap;
use pumpkin_util::math::position::BlockPos;
use rustc_hash::FxHashMap;

use crate::world::World;

/// `SpawnPlacements.SpawnPredicate`: whether an entity of the type may spawn at the position.
pub type SpawnPredicate = Arc<dyn Fn(&'static EntityType, &World, &BlockPos) -> bool + Send + Sync>;

/// `RegisterSpawnPlacementsEvent.Operation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnPlacementOperation {
    /// Checked third: every one of these predicates must pass too.
    And,
    /// Checked second: one of these predicates or the original must pass.
    Or,
    /// Checked first: the last replacement wins over every other predicate.
    Replace,
}

/// Why a registration fails, as the exceptions of `RegisterSpawnPlacementsEvent#register`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnPlacementError {
    /// The entity type has no placement yet, so the registration must name the placement type and
    /// the heightmap.
    MissingPlacement,
    /// A placement type or heightmap with an operation other than `Replace`.
    PlacementWithoutReplace,
    /// The content registry is frozen.
    RegistryFrozen,
}

#[derive(Clone)]
struct MergedSpawnPredicate {
    /// `None` for a vanilla type, whose original predicate is the server's rules.
    original: Option<SpawnPredicate>,
    or: Vec<SpawnPredicate>,
    and: Vec<SpawnPredicate>,
    replacement: Option<SpawnPredicate>,
    location: Option<SpawnLocation>,
    heightmap: Option<HeightMap>,
}

static PENDING: Mutex<BTreeMap<String, MergedSpawnPredicate>> = Mutex::new(BTreeMap::new());
/// Whether a mod registered any placement, so a spawn check on a server without one skips the
/// table.
static ANY: AtomicBool = AtomicBool::new(false);
static TABLE: OnceLock<FxHashMap<u16, Placement>> = OnceLock::new();

/// The merged placement of one entity type.
pub struct Placement {
    predicate: MergedSpawnPredicate,
    restriction: SpawnRestriction,
}

/// Registers a spawn predicate for the entity type `name`.
///
/// `RegisterSpawnPlacementsEvent#register(EntityType, SpawnPlacementType, Heightmap.Types,
/// SpawnPredicate, Operation)`. `location` and `heightmap` may be `None` to keep the current
/// ones; a type without a placement needs both.
///
/// # Errors
///
/// The [`SpawnPlacementError`] of a registration that `NeoForge` rejects.
pub fn register(
    name: &str,
    location: Option<SpawnLocation>,
    heightmap: Option<HeightMap>,
    predicate: SpawnPredicate,
    operation: SpawnPlacementOperation,
) -> Result<(), SpawnPlacementError> {
    if dynamic::tables().is_some() {
        return Err(SpawnPlacementError::RegistryFrozen);
    }
    let mut pending = PENDING.lock().unwrap_or_else(PoisonError::into_inner);
    register_into(
        &mut pending,
        name,
        has_vanilla_placement(name),
        location,
        heightmap,
        predicate,
        operation,
    )?;
    ANY.store(true, Ordering::Relaxed);
    Ok(())
}

/// Whether vanilla `SpawnPlacements` has an entry for the type `name`. A mod gives a type without
/// one its placement, as it does to a custom type.
fn has_vanilla_placement(name: &str) -> bool {
    EntityType::from_name(name)
        .is_some_and(|ty| ty.id < EntityType::COUNT && ty.has_spawn_placement)
}

fn register_into(
    pending: &mut BTreeMap<String, MergedSpawnPredicate>,
    name: &str,
    vanilla_placement: bool,
    location: Option<SpawnLocation>,
    heightmap: Option<HeightMap>,
    predicate: SpawnPredicate,
    operation: SpawnPlacementOperation,
) -> Result<(), SpawnPlacementError> {
    if !(vanilla_placement || pending.contains_key(name)) {
        let (Some(location), Some(heightmap)) = (location, heightmap) else {
            return Err(SpawnPlacementError::MissingPlacement);
        };
        pending.insert(
            name.to_owned(),
            MergedSpawnPredicate {
                original: Some(predicate),
                or: Vec::new(),
                and: Vec::new(),
                replacement: None,
                location: Some(location),
                heightmap: Some(heightmap),
            },
        );
        return Ok(());
    }
    if operation != SpawnPlacementOperation::Replace && (location.is_some() || heightmap.is_some())
    {
        return Err(SpawnPlacementError::PlacementWithoutReplace);
    }
    let merged = pending
        .entry(name.to_owned())
        .or_insert_with(|| MergedSpawnPredicate {
            original: None,
            or: Vec::new(),
            and: Vec::new(),
            replacement: None,
            location: None,
            heightmap: None,
        });
    match operation {
        SpawnPlacementOperation::And => merged.and.push(predicate),
        SpawnPlacementOperation::Or => merged.or.push(predicate),
        SpawnPlacementOperation::Replace => {
            merged.replacement = Some(predicate);
            if location.is_some() {
                merged.location = location;
            }
            if heightmap.is_some() {
                merged.heightmap = heightmap;
            }
        }
    }
    Ok(())
}

/// The placement that mods registered for this type, or `None`.
#[must_use]
pub fn placement(entity_type: &'static EntityType) -> Option<&'static Placement> {
    if !ANY.load(Ordering::Relaxed) {
        return None;
    }
    table()?.get(&entity_type.id)
}

/// The spawn restriction of this type: the placement type and heightmap that mods registered, or
/// the generated ones.
#[must_use]
pub fn spawn_restriction(entity_type: &'static EntityType) -> &'static SpawnRestriction {
    placement(entity_type).map_or(&entity_type.spawn_restriction, |placement| {
        &placement.restriction
    })
}

impl Placement {
    /// `MergedSpawnPredicate#build` applied to one check. `rules` runs the server's rules, the
    /// original predicate of a vanilla type.
    pub fn test(
        &self,
        entity_type: &'static EntityType,
        world: &World,
        pos: &BlockPos,
        rules: impl FnOnce() -> bool,
    ) -> bool {
        let merged = &self.predicate;
        if let Some(replacement) = &merged.replacement {
            return replacement(entity_type, world, pos);
        }
        let original = merged
            .original
            .as_ref()
            .map_or_else(rules, |original| original(entity_type, world, pos));
        (original || merged.or.iter().any(|p| p(entity_type, world, pos)))
            && merged.and.iter().all(|p| p(entity_type, world, pos))
    }
}

fn table() -> Option<&'static FxHashMap<u16, Placement>> {
    // A custom type gets its id at the freeze, so the table waits for it.
    dynamic::tables()?;
    Some(TABLE.get_or_init(build_table))
}

#[cold]
#[inline(never)]
fn build_table() -> FxHashMap<u16, Placement> {
    let pending = std::mem::take(&mut *PENDING.lock().unwrap_or_else(PoisonError::into_inner));
    pending
        .into_iter()
        .filter(|(name, _)| !dynamic::is_placeholder(ContentKind::EntityType, name))
        .filter_map(|(name, predicate)| {
            let entity_type = EntityType::from_name(&name)?;
            let restriction = SpawnRestriction {
                location: predicate
                    .location
                    .clone()
                    .unwrap_or_else(|| entity_type.spawn_restriction.location.clone()),
                heightmap: predicate
                    .heightmap
                    .unwrap_or(entity_type.spawn_restriction.heightmap),
            };
            Some((
                entity_type.id,
                Placement {
                    predicate,
                    restriction,
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn always() -> SpawnPredicate {
        Arc::new(|_, _, _| true)
    }

    #[test]
    fn a_type_without_a_placement_takes_the_first_registration_as_its_placement() {
        let mut pending = BTreeMap::new();
        let first = register_into(
            &mut pending,
            "minecraft:allay",
            has_vanilla_placement("minecraft:allay"),
            Some(SpawnLocation::Unrestricted),
            Some(HeightMap::MotionBlocking),
            always(),
            SpawnPlacementOperation::Replace,
        );
        assert_eq!(first, Ok(()));
        let merged = &pending["minecraft:allay"];
        assert!(merged.original.is_some());
        assert!(merged.location.is_some() && merged.heightmap.is_some());
        // The next registration changes the placement as for any type that has one.
        let next = register_into(
            &mut pending,
            "minecraft:allay",
            has_vanilla_placement("minecraft:allay"),
            Some(SpawnLocation::OnGround),
            None,
            always(),
            SpawnPlacementOperation::Or,
        );
        assert_eq!(next, Err(SpawnPlacementError::PlacementWithoutReplace));
    }

    #[test]
    fn a_type_without_a_placement_needs_its_placement_type_and_heightmap() {
        let mut pending = BTreeMap::new();
        let result = register_into(
            &mut pending,
            "minecraft:allay",
            has_vanilla_placement("minecraft:allay"),
            None,
            None,
            always(),
            SpawnPlacementOperation::Or,
        );
        assert_eq!(result, Err(SpawnPlacementError::MissingPlacement));
        assert!(pending.is_empty());
    }

    #[test]
    fn a_type_with_a_placement_rejects_a_placement_type_without_replace() {
        let mut pending = BTreeMap::new();
        let result = register_into(
            &mut pending,
            "minecraft:zombie",
            has_vanilla_placement("minecraft:zombie"),
            Some(SpawnLocation::OnGround),
            Some(HeightMap::MotionBlocking),
            always(),
            SpawnPlacementOperation::And,
        );
        assert_eq!(result, Err(SpawnPlacementError::PlacementWithoutReplace));
    }

    /// The vanilla 26.3 types that `SpawnPlacements` does not register.
    const WITHOUT_PLACEMENT: &[&str] = &[
        "acacia_boat",
        "acacia_chest_boat",
        "allay",
        "area_effect_cloud",
        "armor_stand",
        "arrow",
        "bamboo_chest_raft",
        "bamboo_raft",
        "bee",
        "birch_boat",
        "birch_chest_boat",
        "block_display",
        "breeze_wind_charge",
        "cherry_boat",
        "cherry_chest_boat",
        "chest_minecart",
        "command_block_minecart",
        "copper_golem",
        "cushion",
        "dark_oak_boat",
        "dark_oak_chest_boat",
        "dragon_fireball",
        "egg",
        "end_crystal",
        "ender_pearl",
        "evoker_fangs",
        "experience_bottle",
        "experience_orb",
        "eye_of_ender",
        "falling_block",
        "fireball",
        "firework_rocket",
        "fishing_bobber",
        "furnace_minecart",
        "glow_item_frame",
        "hopper_minecart",
        "interaction",
        "item",
        "item_display",
        "item_frame",
        "jungle_boat",
        "jungle_chest_boat",
        "leash_knot",
        "lightning_bolt",
        "lingering_potion",
        "llama_spit",
        "mangrove_boat",
        "mangrove_chest_boat",
        "mannequin",
        "marker",
        "minecart",
        "oak_boat",
        "oak_chest_boat",
        "ominous_item_spawner",
        "painting",
        "pale_oak_boat",
        "pale_oak_chest_boat",
        "piglin_brute",
        "player",
        "poplar_boat",
        "poplar_chest_boat",
        "shulker_bullet",
        "small_fireball",
        "sniffer",
        "snowball",
        "spawner_minecart",
        "spectral_arrow",
        "splash_potion",
        "spruce_boat",
        "spruce_chest_boat",
        "tadpole",
        "text_display",
        "tnt",
        "tnt_minecart",
        "trident",
        "wind_charge",
        "wither_skull",
        "zombie_nautilus",
    ];

    #[test]
    fn the_types_without_a_vanilla_placement_need_one_on_their_first_registration() {
        assert_eq!(WITHOUT_PLACEMENT.len(), 78);
        for entity_type in EntityType::ALL {
            let name = format!("minecraft:{}", entity_type.resource_name);
            let expected = !WITHOUT_PLACEMENT.contains(&entity_type.resource_name);
            assert_eq!(has_vanilla_placement(&name), expected, "{name}");
            let result = register_into(
                &mut BTreeMap::new(),
                &name,
                has_vanilla_placement(&name),
                None,
                None,
                always(),
                SpawnPlacementOperation::Or,
            );
            let expected = if expected {
                Ok(())
            } else {
                Err(SpawnPlacementError::MissingPlacement)
            };
            assert_eq!(result, expected, "{name}");
        }
    }
}
