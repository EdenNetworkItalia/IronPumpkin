use std::sync::Arc;

use pumpkin_data::game_event::GameEvent;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::{Block, BlockStateId, Property};
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::world::BlockFlags;
use rand::{Rng, RngExt};

use super::super::super::BrainTick;
use super::super::super::memory::position_tracker::{BlockPosTracker, PositionTracker};
use super::super::super::memory::walk_target::WalkTarget;
use super::super::super::memory::{MemoryModuleId, MemoryStatus, types};
use super::super::timed::Behavior;
use crate::entity::passive::villager::{VillagerEntity, VillagerProfession};
use crate::world::World;
use crate::world::game_event::GameEventContext;

const HARVEST_DURATION: i32 = 200;
const SPEED_MODIFIER: f32 = 0.5;

const ENTRY_CONDITIONS: [(MemoryModuleId, MemoryStatus); 3] = [
    (types::LOOK_TARGET.id(), MemoryStatus::ValueAbsent),
    (types::WALK_TARGET.id(), MemoryStatus::ValueAbsent),
    (types::SECONDARY_JOB_SITE.id(), MemoryStatus::ValuePresent),
];

pub struct HarvestFarmland {
    above_farmland_pos: Option<BlockPos>,
    next_ok_start_time: i64,
    time_worked_so_far: i32,
    valid_farmland_around_villager: Vec<BlockPos>,
}

impl HarvestFarmland {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            above_farmland_pos: None,
            next_ok_start_time: 0,
            time_worked_so_far: 0,
            valid_farmland_around_villager: Vec::new(),
        }
    }

    fn get_valid_farmland(&self, random: &mut impl Rng) -> Option<BlockPos> {
        if self.valid_farmland_around_villager.is_empty() {
            return None;
        }
        let index = random.random_range(0..self.valid_farmland_around_villager.len());
        Some(self.valid_farmland_around_villager[index])
    }

    fn set_target(tick: &mut BrainTick<'_>, pos: BlockPos) {
        let tracker: Arc<dyn PositionTracker> = Arc::new(BlockPosTracker::new(pos));
        tick.state.set(types::LOOK_TARGET, Arc::clone(&tracker));
        tick.state.set(
            types::WALK_TARGET,
            WalkTarget::new(tracker, SPEED_MODIFIER, 1),
        );
    }
}

impl Behavior for HarvestFarmland {
    fn entry_conditions(&self) -> &[(MemoryModuleId, MemoryStatus)] {
        &ENTRY_CONDITIONS
    }

    fn check_extra_start_conditions(&mut self, tick: &mut BrainTick<'_>) -> bool {
        if !tick.world.level_info.load().game_rules.mob_griefing {
            return false;
        }
        let Some(villager) = tick.mob.cast_any().downcast_ref::<VillagerEntity>() else {
            return false;
        };
        let profession = villager
            .villager_data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .profession_enum();
        if profession != VillagerProfession::Farmer {
            return false;
        }

        let center = tick.mob.get_entity().block_pos.load();
        self.valid_farmland_around_villager.clear();
        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    let pos = center.add(x, y, z);
                    if valid_pos(pos, tick.world) {
                        self.valid_farmland_around_villager.push(pos);
                    }
                }
            }
        }

        self.above_farmland_pos = self.get_valid_farmland(&mut tick.mob.get_random());
        self.above_farmland_pos.is_some()
    }

    fn start(&mut self, tick: &mut BrainTick<'_>) {
        if tick.time > self.next_ok_start_time
            && let Some(pos) = self.above_farmland_pos
        {
            Self::set_target(tick, pos);
        }
    }

    fn stop(&mut self, tick: &mut BrainTick<'_>, _timed_out: bool) {
        tick.state.erase(types::LOOK_TARGET.id());
        tick.state.erase(types::WALK_TARGET.id());
        self.time_worked_so_far = 0;
        self.next_ok_start_time = tick.time + 40;
    }

    fn tick(&mut self, tick: &mut BrainTick<'_>) {
        let entity_pos = tick.mob.get_entity().pos.load();
        let close = self.above_farmland_pos.is_none_or(|pos| {
            pos.dist_to_center_sqr(entity_pos.x, entity_pos.y, entity_pos.z) < 1.0
        });
        if !close {
            return;
        }

        if let Some(farmland_pos) = self.above_farmland_pos
            && tick.time > self.next_ok_start_time
        {
            let Some(villager) = tick.mob.cast_any().downcast_ref::<VillagerEntity>() else {
                return;
            };
            let (block, state) = tick.world.get_block_and_state(&farmland_pos);
            let block_below = tick.world.get_block(&farmland_pos.down());
            let source: &dyn crate::entity::EntityBase = villager;

            if is_crop_block(block)
                && is_max_age(block, state.id)
                && tick
                    .world
                    .break_block(&farmland_pos, None, BlockFlags::NOTIFY_ALL)
                    .is_some()
            {
                tick.world.emit_game_event_with_context(
                    GameEvent::BlockDestroy.name(),
                    farmland_pos.to_centered_f64(),
                    &GameEventContext::of_entity_state(Some(source), state.id),
                );
            }

            if state.is_air() && is_villager_farmland(block_below) && villager.has_farm_seeds() {
                // The block change, the game event and the sound run mod handlers, so the
                // inventory lock is free while they run. The seed is found first and taken
                // after them, as vanilla, when the slot still holds it.
                let candidate = {
                    let inventory = villager
                        .inventory
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    inventory.iter().enumerate().find_map(|(index, slot)| {
                        if slot.is_empty()
                            || !slot
                                .item
                                .has_tag(&tag::Item::MINECRAFT_VILLAGER_PLANTABLE_SEEDS)
                        {
                            return None;
                        }
                        let place_block = Block::from_item_id(slot.item.id)?;
                        Some((index, slot.item.id, place_block.default_state))
                    })
                };
                if let Some((index, item_id, place)) = candidate {
                    tick.world
                        .set_block_state(&farmland_pos, place.id, BlockFlags::NOTIFY_ALL);
                    tick.world.emit_game_event_with_context(
                        GameEvent::BlockPlace.name(),
                        farmland_pos.to_centered_f64(),
                        &GameEventContext::of_entity_state(Some(source), place.id),
                    );
                    tick.world.play_sound(
                        Sound::ItemCropPlant,
                        SoundCategory::Blocks,
                        &farmland_pos.to_f64(),
                    );
                    let mut inventory = villager
                        .inventory
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if let Some(slot) = inventory.get_mut(index)
                        && !slot.is_empty()
                        && slot.item.id == item_id
                    {
                        slot.decrement(1);
                        if slot.item_count == 0 {
                            slot.clear();
                        }
                    }
                }
            }

            if is_crop_block(block) && !is_max_age(block, state.id) {
                self.valid_farmland_around_villager
                    .retain(|pos| *pos != farmland_pos);
                self.above_farmland_pos = self.get_valid_farmland(&mut tick.mob.get_random());
                if let Some(pos) = self.above_farmland_pos {
                    self.next_ok_start_time = tick.time + 20;
                    Self::set_target(tick, pos);
                }
            }
        }

        self.time_worked_so_far += 1;
    }

    fn can_still_use(&mut self, _tick: &mut BrainTick<'_>) -> bool {
        self.time_worked_so_far < HARVEST_DURATION
    }

    fn debug_name(&self) -> &'static str {
        "HarvestFarmland"
    }
}

impl Default for HarvestFarmland {
    fn default() -> Self {
        Self::new()
    }
}

pub(super) fn is_crop_block(block: &Block) -> bool {
    block.has_tag(&tag::Block::MINECRAFT_CROPS)
        && block != &Block::PITCHER_CROP
        && block != &Block::MELON_STEM
        && block != &Block::PUMPKIN_STEM
}

pub(super) fn is_max_age(block: &Block, state_id: BlockStateId) -> bool {
    if block == &Block::TORCHFLOWER_CROP {
        return false;
    }
    let Some(age) = age_property(block, state_id) else {
        return false;
    };
    max_age(block).is_some_and(|max_age| age >= max_age)
}

fn age_property(block: &Block, state_id: BlockStateId) -> Option<u8> {
    block
        .properties(state_id)?
        .to_props()
        .into_iter()
        .find_map(|property| match property {
            Property::Age(age) => Some(age),
            _ => None,
        })
}

fn max_age(block: &Block) -> Option<u8> {
    block
        .states
        .iter()
        .filter_map(|state| age_property(block, state.id))
        .max()
}

fn is_villager_farmland(block: &Block) -> bool {
    block == &Block::FARMLAND || block.has_tag(&tag::Block::NEOFORGE_VILLAGER_FARMLANDS)
}

fn valid_pos(block_pos: BlockPos, world: &World) -> bool {
    let (block, state) = world.get_block_and_state(&block_pos);
    let block_below = world.get_block(&block_pos.down());
    (is_crop_block(block) && is_max_age(block, state.id))
        || (state.is_air() && is_villager_farmland(block_below))
}
