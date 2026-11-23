use std::sync::LazyLock;

use arc_swap::ArcSwapOption;
use pumpkin_data::{Block, BlockDirection};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::{
    math::{position::BlockPos, vector3::Vector3},
    random::{RandomGenerator, RandomImpl},
};

use crate::generation::proto_chunk::GenerationCache;

/// `MonsterRoomFeature.MOBS`.
pub const DUNGEON_MOBS: [&str; 4] = [
    "minecraft:skeleton",
    "minecraft:zombie",
    "minecraft:zombie",
    "minecraft:spider",
];

/// The weight that `NeoForge` gives each entry of `DUNGEON_MOBS` in its `monster_room_mobs` data
/// map.
pub const DUNGEON_MOB_WEIGHT: i32 = 100;

/// The weighted mobs of the `monster_room_mobs` data map, or `None` for the vanilla pick.
static MONSTER_ROOM_MOBS: LazyLock<ArcSwapOption<Vec<(String, i32)>>> =
    LazyLock::new(ArcSwapOption::empty);

/// The mobs of `DUNGEON_MOBS` with the weights of the `NeoForge` default data map, ordered by name.
#[must_use]
pub fn vanilla_monster_room_mobs() -> Vec<(String, i32)> {
    let mut mobs: Vec<(String, i32)> = Vec::new();
    for mob in DUNGEON_MOBS {
        match mobs.iter_mut().find(|(name, _)| name == mob) {
            Some((_, weight)) => *weight += DUNGEON_MOB_WEIGHT,
            None => mobs.push((mob.to_owned(), DUNGEON_MOB_WEIGHT)),
        }
    }
    mobs.sort();
    mobs
}

/// Sets the mobs of `MonsterRoomHooks.onDataMapsUpdated`.
///
/// A list that is empty, has no weight or equals the vanilla list keeps the vanilla pick, so a
/// world with no override generates the same dungeons as vanilla for a seed.
pub fn set_monster_room_mobs(mut mobs: Vec<(String, i32)>) {
    mobs.sort();
    let total: i64 = mobs.iter().map(|(_, weight)| i64::from(*weight)).sum();
    let custom = total > 0 && total <= i64::from(i32::MAX) && mobs != vanilla_monster_room_mobs();
    MONSTER_ROOM_MOBS.store(custom.then(|| std::sync::Arc::new(mobs)));
}

/// `MonsterRoomHooks.getRandomMonsterRoomMob`: a mob of the data map by weight, as
/// `WeightedList.getRandomOrThrow` picks, or the vanilla pick.
fn random_mob(random: &mut RandomGenerator) -> String {
    if let Some(mobs) = MONSTER_ROOM_MOBS.load_full() {
        let total: i32 = mobs.iter().map(|(_, weight)| *weight).sum();
        let mut selection = random.next_bounded_i32(total);
        for (mob, weight) in mobs.iter() {
            if selection < *weight {
                return mob.clone();
            }
            selection -= weight;
        }
    }
    DUNGEON_MOBS[random.next_bounded_i32(DUNGEON_MOBS.len() as i32) as usize].to_owned()
}

pub struct DungeonFeature;

impl DungeonFeature {
    #[expect(clippy::too_many_lines)]
    pub fn generate<T: GenerationCache>(
        chunk: &mut T,
        _min_y: i8,
        _height: u16,
        _feature: pumpkin_data::placed_feature::PlacedFeature,
        random: &mut RandomGenerator,
        pos: BlockPos,
    ) -> bool {
        let xr = random.next_bounded_i32(2) + 2;
        let zr = random.next_bounded_i32(2) + 2;

        let min_x = -xr - 1;
        let max_x = xr + 1;
        let min_z = -zr - 1;
        let max_z = zr + 1;

        let mut hole_count = 0i32;
        for dx in min_x..=max_x {
            for dy in -1..=4i32 {
                for dz in min_z..=max_z {
                    let check_pos = pos.0.add(&Vector3::new(dx, dy, dz));
                    let solid = GenerationCache::get_block_state(chunk, &check_pos)
                        .to_state()
                        .is_solid();

                    if dy == -1 && !solid {
                        return false;
                    }
                    if dy == 4 && !solid {
                        return false;
                    }

                    let on_wall = dx == min_x || dx == max_x || dz == min_z || dz == max_z;
                    if on_wall
                        && dy == 0
                        && chunk.is_air(&check_pos)
                        && chunk.is_air(&check_pos.add(&Vector3::new(0, 1, 0)))
                    {
                        hole_count += 1;
                    }
                }
            }
        }

        if !(1..=5).contains(&hole_count) {
            return false;
        }

        for dx in min_x..=max_x {
            for dy in (-1..=3i32).rev() {
                for dz in min_z..=max_z {
                    let wall_pos = pos.0.add(&Vector3::new(dx, dy, dz));

                    let on_boundary = dx == min_x
                        || dy == -1
                        || dz == min_z
                        || dx == max_x
                        || dy == 4
                        || dz == max_z;

                    if on_boundary {
                        let below_pos = wall_pos.add(&Vector3::new(0, -1, 0));
                        let below_solid = GenerationCache::get_block_state(chunk, &below_pos)
                            .to_state()
                            .is_solid();
                        let cur_state =
                            GenerationCache::get_block_state(chunk, &wall_pos).to_state();
                        let cur_solid = cur_state.is_solid();
                        let is_chest = cur_state.id.to_block() == &Block::CHEST;

                        let world_min_y = chunk.bottom_y() as i32;
                        if wall_pos.y >= world_min_y && !below_solid {
                            chunk.set_block_state(&wall_pos, Block::CAVE_AIR.default_state);
                        } else if cur_solid && !is_chest {
                            if dy == -1 && random.next_bounded_i32(4) != 0 {
                                chunk.set_block_state(
                                    &wall_pos,
                                    Block::MOSSY_COBBLESTONE.default_state,
                                );
                            } else {
                                chunk.set_block_state(&wall_pos, Block::COBBLESTONE.default_state);
                            }
                        }
                    } else {
                        let state = GenerationCache::get_block_state(chunk, &wall_pos).to_state();
                        let is_chest = state.id.to_block() == &Block::CHEST;
                        let is_spawner = state.id.to_block() == &Block::SPAWNER;
                        if !is_chest && !is_spawner {
                            chunk.set_block_state(&wall_pos, Block::CAVE_AIR.default_state);
                        }
                    }
                }
            }
        }

        for _ in 0..2 {
            for _ in 0..3 {
                let cx = pos.0.x + random.next_bounded_i32(xr * 2 + 1) - xr;
                let cy = pos.0.y;
                let cz = pos.0.z + random.next_bounded_i32(zr * 2 + 1) - zr;
                let chest_pos = Vector3::new(cx, cy, cz);

                if !chunk.is_air(&chest_pos) {
                    continue;
                }

                let wall_count = BlockDirection::horizontal()
                    .iter()
                    .filter(|d| {
                        GenerationCache::get_block_state(chunk, &chest_pos.add(&d.to_offset()))
                            .to_state()
                            .is_solid()
                    })
                    .count();

                if wall_count == 1 {
                    let chest_state =
                        crate::generation::structure::structures::StructurePiece::reorient(
                            &chest_pos,
                            Block::CHEST.default_state,
                            |p| GenerationCache::get_block_state(chunk, p),
                        );

                    chunk.set_block_state(&chest_pos, chest_state);

                    let loot_seed = random.next_i64();
                    let mut chest_nbt = NbtCompound::new();
                    chest_nbt.put_string("id", "minecraft:chest".to_string());
                    chest_nbt.put_int("x", chest_pos.x);
                    chest_nbt.put_int("y", chest_pos.y);
                    chest_nbt.put_int("z", chest_pos.z);
                    chest_nbt
                        .put_string("LootTable", "minecraft:chests/simple_dungeon".to_string());
                    chest_nbt.put_long("LootTableSeed", loot_seed);
                    chunk.add_block_entity(&chest_pos, chest_nbt);

                    break;
                }
            }
        }

        let mob = random_mob(random);
        chunk.set_block_state(&pos.0, Block::SPAWNER.default_state);

        let mut entity_nbt = NbtCompound::new();
        entity_nbt.put_string("id", "minecraft:mob_spawner".to_string());
        entity_nbt.put_int("x", pos.0.x);
        entity_nbt.put_int("y", pos.0.y);
        entity_nbt.put_int("z", pos.0.z);

        let mut spawn_entry = NbtCompound::new();
        let mut entity_nbt_inner = NbtCompound::new();
        entity_nbt_inner.put_string("id", mob);
        spawn_entry.put_compound("entity", entity_nbt_inner);
        entity_nbt.put_compound("SpawnData", spawn_entry);

        chunk.add_block_entity(&pos.0, entity_nbt);

        true
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use pumpkin_util::random::legacy_rand::LegacyRand;

    use super::*;

    /// The mob table is a process-wide static: the tests that set it run one at a time.
    static TABLE: Mutex<()> = Mutex::new(());

    fn picks(seed: i64, count: usize) -> Vec<String> {
        let mut random = RandomGenerator::Legacy(LegacyRand::from_seed(seed as u64));
        (0..count).map(|_| random_mob(&mut random)).collect()
    }

    #[test]
    fn a_data_pack_weight_changes_the_spawner_mob() {
        let _guard = TABLE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        set_monster_room_mobs(Vec::new());
        let vanilla = picks(7, 64);
        assert!(vanilla.iter().any(|mob| mob == "minecraft:skeleton"));
        set_monster_room_mobs(vec![("mymod:crab".to_owned(), 5)]);
        assert!(picks(7, 64).iter().all(|mob| mob == "mymod:crab"));
        set_monster_room_mobs(Vec::new());
        assert_eq!(picks(7, 64), vanilla);
    }

    #[test]
    fn the_default_data_map_keeps_the_vanilla_pick_for_a_seed() {
        let _guard = TABLE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        set_monster_room_mobs(Vec::new());
        let vanilla = picks(11, 64);
        set_monster_room_mobs(vanilla_monster_room_mobs());
        assert_eq!(picks(11, 64), vanilla);
        // A list with no weight cannot pick: the vanilla pick stays.
        set_monster_room_mobs(vec![("mymod:crab".to_owned(), 0)]);
        assert_eq!(picks(11, 64), vanilla);
        set_monster_room_mobs(Vec::new());
    }

    #[test]
    fn the_pick_follows_the_weights() {
        let _guard = TABLE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        set_monster_room_mobs(vec![
            ("mymod:rare".to_owned(), 1),
            ("mymod:common".to_owned(), 9),
        ]);
        let picked = picks(3, 2000);
        let rare = picked.iter().filter(|mob| *mob == "mymod:rare").count();
        assert!((100..300).contains(&rare), "rare picked {rare} of 2000");
        set_monster_room_mobs(Vec::new());
    }
}
