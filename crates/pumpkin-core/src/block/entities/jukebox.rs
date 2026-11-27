use std::any::Any;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use pumpkin_data::data_component_impl::JukeboxPlayableImpl;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::jukebox_song::JukeboxSong;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::position::BlockPos;
use rand::RngExt;

use crate::block::entities::BlockEntity;
use crate::world::World;
use pumpkin_inventory::{Clearable, Inventory};

/// Matches vanilla's `JukeboxBlockEntity`
pub struct JukeboxBlockEntity {
    position: BlockPos,
    world: OnceLock<Weak<World>>,
    /// The record item stored in the jukebox (`RecordItem` in NBT)
    record_stack: Arc<Mutex<ItemStack>>,
    /// Serializes record snapshots and storage. A change pushes its effect under this lock.
    record_transition: Mutex<()>,
    /// The effects of the record changes, in storage order. One drainer applies them with no lock
    /// held, so a mod handler can touch the jukebox, and two threads cannot apply them out of
    /// order.
    effects: Mutex<VecDeque<RecordEffect>>,
    /// Whether a thread drains `effects`.
    draining: AtomicBool,
    /// Ticks since the current song started playing
    ticks_since_song_started: AtomicU64,
    /// Length of the current song in ticks (0 if not playing)
    song_length_ticks: AtomicU64,
    dirty: AtomicBool,
    comparator_dirty: AtomicBool,
}

const RECORD_ITEM_NBT_KEY: &str = "RecordItem";
const TICKS_SINCE_SONG_STARTED_NBT_KEY: &str = "ticks_since_song_started";

/// Emits a jukebox game event with the block state of the jukebox as its context, as vanilla
/// `Context.of(getBlockState())`.
fn emit_jukebox_event(
    world: &World,
    event: pumpkin_data::game_event::GameEvent,
    position: &pumpkin_util::math::position::BlockPos,
) {
    // A jukebox that the world replaced is no longer at the position: its state is the one of a
    // jukebox without a record, as the state that vanilla caches in the block entity.
    let state = world.get_block_state_id(position);
    let state = if pumpkin_data::Block::from_state_id(state) == &pumpkin_data::Block::JUKEBOX {
        state
    } else {
        pumpkin_data::block_properties::JukeboxLikeProperties { has_record: false }
            .to_state_id(&pumpkin_data::Block::JUKEBOX)
    };
    world.emit_game_event_with_context(
        event.name(),
        position.to_centered_f64(),
        &crate::world::game_event::GameEventContext::of_state(state),
    );
}

/// What a record change does once the record transition lock is released.
enum RecordEffect {
    /// Vanilla `itemChanged`: `has_record` and `block_change`, then the song of the record.
    Changed {
        has_record: bool,
        /// The id and the length in ticks of the song that starts.
        song: Option<(i32, u64)>,
    },
    /// The song stops.
    Stop,
}

impl BlockEntity for JukeboxBlockEntity {
    fn resource_location(&self) -> &'static str {
        Self::ID
    }

    fn get_position(&self) -> BlockPos {
        self.position
    }

    fn from_nbt(nbt: &NbtCompound, position: BlockPos) -> Self
    where
        Self: Sized,
    {
        let record_stack = nbt
            .get_compound(RECORD_ITEM_NBT_KEY)
            .and_then(ItemStack::read_item_stack)
            .unwrap_or_else(|| ItemStack::EMPTY.clone());

        let (ticks_since_song_started, song_length_ticks) = if let Some(raw_ticks) =
            nbt.get_long(TICKS_SINCE_SONG_STARTED_NBT_KEY)
            && raw_ticks >= 0
            && let Some(song) = record_stack
                .get_data_component::<JukeboxPlayableImpl>()
                .and_then(|p| p.song.rsplit(':').next())
                .and_then(JukeboxSong::from_name)
        {
            let ticks = raw_ticks as u64;
            let end_ticks = song.length_in_ticks() + Self::SONG_END_PADDING_TICKS;
            if ticks < end_ticks {
                (ticks, end_ticks)
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        };

        Self {
            position,
            world: OnceLock::new(),
            record_stack: Arc::new(Mutex::new(record_stack)),
            record_transition: Mutex::new(()),
            effects: Mutex::new(VecDeque::new()),
            draining: AtomicBool::new(false),
            ticks_since_song_started: AtomicU64::new(ticks_since_song_started),
            song_length_ticks: AtomicU64::new(song_length_ticks),
            dirty: AtomicBool::new(false),
            comparator_dirty: AtomicBool::new(false),
        }
    }

    fn write_nbt(&self, nbt: &mut NbtCompound) {
        let record = self
            .record_stack
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(record_nbt) = record.to_nbt() {
            nbt.put(RECORD_ITEM_NBT_KEY, record_nbt);
        }

        if self.song_length_ticks.load(Ordering::Relaxed) > 0 {
            let ticks = self.ticks_since_song_started.load(Ordering::Relaxed);
            nbt.put_long(TICKS_SINCE_SONG_STARTED_NBT_KEY, ticks as i64);
        }
    }

    fn tick(&self, world: &Arc<World>) {
        self.world.get_or_init(|| Arc::downgrade(world));
        let song_length = self.song_length_ticks.load(Ordering::Relaxed);
        if song_length > 0 {
            let ticks = self.ticks_since_song_started.load(Ordering::Relaxed);
            if ticks >= song_length {
                self.stop_playing();
            } else {
                if ticks.is_multiple_of(20) {
                    emit_jukebox_event(
                        world,
                        pumpkin_data::game_event::GameEvent::JukeboxPlay,
                        &self.position,
                    );
                    let random_color = rand::rng().random_range(0..4) as f32 / 24.0;
                    world.spawn_particle(
                        pumpkin_util::math::vector3::Vector3::new(
                            f64::from(self.position.0.x) + 0.5,
                            f64::from(self.position.0.y) + 1.2,
                            f64::from(self.position.0.z) + 0.5,
                        ),
                        pumpkin_util::math::vector3::Vector3::new(random_color, 0.0, 0.0),
                        1.0,
                        0,
                        pumpkin_data::particle::Particle::Note,
                    );
                }
                self.ticks_since_song_started
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    fn is_comparator_dirty(&self) -> bool {
        self.comparator_dirty.load(Ordering::Relaxed)
    }

    fn clear_comparator_dirty(&self) {
        self.comparator_dirty.store(false, Ordering::Relaxed);
    }

    fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Relaxed)
    }

    fn bedrock_block_actor_data(
        &self,
        _state_id: pumpkin_data::BlockStateId,
    ) -> Option<NbtCompound> {
        let record_guard = self.record_stack.lock().ok();
        let record = record_guard.as_deref();
        Some(build_jukebox_actor(
            self.position,
            record,
            self.ticks_since_song_started.load(Ordering::Relaxed),
        ))
    }

    fn set_world(&self, world: Weak<World>) {
        let _ = self.world.set(world);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn get_inventory(self: Arc<Self>) -> Option<Arc<dyn Inventory>> {
        Some(self)
    }

    fn on_block_replaced(self: Arc<Self>, world: &Arc<World>, position: &BlockPos) {
        self.world.get_or_init(|| Arc::downgrade(world));
        let record = self.clear_record();
        if !record.is_empty() {
            let spawn_pos = pumpkin_util::math::vector3::Vector3::new(
                f64::from(position.0.x) + 0.5 + rand::rng().random_range(-0.35..0.35),
                f64::from(position.0.y) + 1.01,
                f64::from(position.0.z) + 0.5 + rand::rng().random_range(-0.35..0.35),
            );
            let entity = crate::entity::Entity::new(
                world.clone(),
                spawn_pos,
                &pumpkin_data::entity::EntityType::ITEM,
            );
            let item_entity = Arc::new(crate::entity::item::ItemEntity::new(entity, record));
            world.spawn_entity(item_entity);
        }
        emit_jukebox_event(
            world,
            pumpkin_data::game_event::GameEvent::JukeboxStopPlay,
            position,
        );
        world.sync_world_event(
            pumpkin_data::world::WorldEvent::SoundStopJukeboxSong,
            *position,
            0,
        );
    }
}

impl JukeboxBlockEntity {
    pub const ID: &'static str = "minecraft:jukebox";
    pub const SONG_END_PADDING_TICKS: u64 = 20;

    #[must_use]
    pub fn new(position: BlockPos) -> Self {
        Self {
            position,
            world: OnceLock::new(),
            record_stack: Arc::new(Mutex::new(ItemStack::EMPTY.clone())),
            record_transition: Mutex::new(()),
            effects: Mutex::new(VecDeque::new()),
            draining: AtomicBool::new(false),
            ticks_since_song_started: AtomicU64::new(0),
            song_length_ticks: AtomicU64::new(0),
            dirty: AtomicBool::new(false),
            comparator_dirty: AtomicBool::new(false),
        }
    }

    /// Get the current record stack
    pub fn get_record(&self) -> ItemStack {
        self.record_stack
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Set the record stack - matches vanilla's `setStack()`
    /// Note: The caller is responsible for updating block state and playing music
    // TODO vanilla `JukeboxPlayable.tryInsertIntoJukebox` uses `setTheItem` (item, HAS_RECORD, play).
    // This only writes the stack; the block applies HAS_RECORD and playback afterwards.
    pub fn set_record(&self, stack: ItemStack) {
        let _transition = self.lock_record_transition();
        *self
            .record_stack
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = stack;
        self.mark_dirty();
    }

    /// Clear the stack and return what was there - used for dropping. As vanilla `setTheItem`
    /// with an empty stack: `has_record` and `block_change`, then the stop of the song.
    pub fn clear_record(&self) -> ItemStack {
        let transition = self.lock_record_transition();
        let taken = std::mem::replace(
            &mut *self
                .record_stack
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            ItemStack::EMPTY.clone(),
        );
        self.push_effect(RecordEffect::Changed {
            has_record: false,
            song: None,
        });
        self.mark_dirty();
        drop(transition);
        self.drain_effects();
        taken
    }

    fn lock_record_transition(&self) -> std::sync::MutexGuard<'_, ()> {
        self.record_transition
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Start playing a song with the given length in ticks
    pub fn start_playing(&self, length_in_ticks: u64) {
        self.ticks_since_song_started.store(0, Ordering::Relaxed);
        self.song_length_ticks
            .store(length_in_ticks, Ordering::Relaxed);
        self.mark_dirty();
    }

    /// The song of `record`, as vanilla `JukeboxSong.fromStack`.
    fn song_of(record: &ItemStack) -> Option<JukeboxSong> {
        record
            .get_data_component::<JukeboxPlayableImpl>()
            .and_then(|playable| playable.song.rsplit(':').next())
            .and_then(JukeboxSong::from_name)
    }

    /// The effect of the record now in the slot, as vanilla `itemChanged`: a record with a song
    /// starts it, anything else stops the playing song.
    fn changed_effect(record: &ItemStack) -> RecordEffect {
        RecordEffect::Changed {
            has_record: !record.is_empty(),
            song: if record.is_empty() {
                None
            } else {
                Self::song_of(record).map(|song| {
                    (
                        song.get_id() as i32,
                        song.length_in_ticks() + Self::SONG_END_PADDING_TICKS,
                    )
                })
            },
        }
    }

    /// Queues `effect` after the effects of the earlier changes. The caller holds the record
    /// transition lock, so the queue order is the storage order.
    fn push_effect(&self, effect: RecordEffect) {
        self.effects
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push_back(effect);
    }

    /// Applies the queued effects in order, with no lock held. When another thread drains, it
    /// applies the effects that this call queued, and a handler that changes the record while the
    /// drainer applies an effect only queues its own effect.
    fn drain_effects(&self) {
        loop {
            if self
                .draining
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_err()
            {
                return;
            }
            loop {
                // The guard of the queue ends with this statement: a handler queues an effect
                // while the drainer applies one.
                let next = self
                    .effects
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .pop_front();
                let Some(effect) = next else { break };
                self.apply_effect(&effect);
            }
            self.draining.store(false, Ordering::Release);
            // An effect that was queued after the last pop needs a drainer.
            if self
                .effects
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_empty()
            {
                return;
            }
        }
    }

    /// Resets the song state and tells whether a song was playing.
    fn stop_song(&self) -> bool {
        self.ticks_since_song_started.store(0, Ordering::Relaxed);
        let playing = self.song_length_ticks.swap(0, Ordering::Relaxed) != 0;
        self.mark_dirty();
        playing
    }

    /// Runs an effect in the order of vanilla `itemChanged`: `has_record` and `block_change`
    /// first, then the start or the stop of the song. The block state change, the world events,
    /// the neighbor updates and the game events run mod handlers, so no lock is held.
    fn apply_effect(&self, effect: &RecordEffect) {
        let world = self.world.get().and_then(Weak::upgrade);
        let song = match effect {
            RecordEffect::Changed { has_record, song } => {
                if let Some(world) = &world {
                    let (block, _) = world.get_block_and_state(&self.position);
                    if block == &pumpkin_data::Block::JUKEBOX {
                        world.set_block_state(
                            &self.position,
                            pumpkin_data::block_properties::JukeboxLikeProperties {
                                has_record: *has_record,
                            }
                            .to_state_id(block),
                            pumpkin_world::world::BlockFlags::NOTIFY_LISTENERS,
                        );
                        emit_jukebox_event(
                            world,
                            pumpkin_data::game_event::GameEvent::BlockChange,
                            &self.position,
                        );
                    }
                }
                *song
            }
            RecordEffect::Stop => None,
        };
        if let Some((song_id, length_in_ticks)) = song {
            self.start_playing(length_in_ticks);
            if let Some(world) = &world {
                world.sync_world_event(
                    pumpkin_data::world::WorldEvent::SoundPlayJukeboxSong,
                    self.position,
                    song_id,
                );
                world.update_neighbors(&self.position, None);
            }
        } else if self.stop_song()
            && let Some(world) = &world
        {
            emit_jukebox_event(
                world,
                pumpkin_data::game_event::GameEvent::JukeboxStopPlay,
                &self.position,
            );
            world.sync_world_event(
                pumpkin_data::world::WorldEvent::SoundStopJukeboxSong,
                self.position,
                0,
            );
            world.update_neighbors(&self.position, None);
        }
    }

    /// Stop playing the current song
    pub fn stop_playing(&self) {
        let transition = self.lock_record_transition();
        self.push_effect(RecordEffect::Stop);
        drop(transition);
        self.drain_effects();
    }

    /// Check if a song is currently playing
    pub fn is_playing(&self) -> bool {
        let song_length = self.song_length_ticks.load(Ordering::Relaxed);
        if song_length == 0 {
            return false;
        }
        let ticks = self.ticks_since_song_started.load(Ordering::Relaxed);
        ticks < song_length
    }

    fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Relaxed);
        self.comparator_dirty.store(true, Ordering::Relaxed);
    }
}

/// Implements single-slot inventory for jukebox (matches vanilla's `SingleStackInventory`)
// TODO vanilla `splitTheItem` is `setTheItem(EMPTY)`: `HAS_RECORD` then song stop. `remove_stack` stops first.
// TODO vanilla `setTheItem` always notifies and play/stop. `update_stack` only syncs when emptiness flips.
impl Inventory for JukeboxBlockEntity {
    fn size(&self) -> usize {
        1
    }

    fn is_empty(&self) -> bool {
        self.record_stack
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_empty()
    }

    fn get_stack(&self, _slot: usize) -> ItemStack {
        self.record_stack
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn remove_stack(&self, _slot: usize) -> ItemStack {
        let transition = self.lock_record_transition();
        let taken = std::mem::replace(
            &mut *self
                .record_stack
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            ItemStack::EMPTY.clone(),
        );
        self.push_effect(if taken.is_empty() {
            RecordEffect::Stop
        } else {
            RecordEffect::Changed {
                has_record: false,
                song: None,
            }
        });
        self.mark_dirty();
        drop(transition);
        self.drain_effects();
        taken
    }

    fn remove_stack_specific(&self, _slot: usize, _amount: u8) -> ItemStack {
        // Jukebox only holds one item, so remove the whole stack
        self.remove_stack(0)
    }

    fn set_stack(&self, _slot: usize, stack: ItemStack) {
        let transition = self.lock_record_transition();
        let effect = Self::changed_effect(&stack);
        *self
            .record_stack
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = stack;
        self.push_effect(effect);
        self.mark_dirty();
        drop(transition);
        self.drain_effects();
    }

    fn update_stack(&self, slot: usize, update: &mut dyn FnMut(&mut ItemStack) -> bool) -> bool {
        if slot >= self.size() {
            return false;
        }
        let transition = self.lock_record_transition();
        let (updated, inserted_or_removed, record) = {
            let mut record = self
                .record_stack
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let had_record = !record.is_empty();
            let updated = update(&mut record);
            let changed = updated && had_record == record.is_empty();
            (updated, changed, record.clone())
        };
        if inserted_or_removed {
            self.push_effect(Self::changed_effect(&record));
        }
        drop(transition);
        self.drain_effects();
        updated
    }

    fn get_max_count_per_stack(&self) -> u8 {
        1
    }

    fn is_valid_slot_for(&self, slot: usize, stack: &ItemStack) -> bool {
        slot == 0
            && stack
                .get_data_component::<pumpkin_data::data_component_impl::JukeboxPlayableImpl>()
                .is_some()
            && self.is_empty()
    }

    /// Vanilla `canTakeItem`: only into a container with an empty slot, so a pulled record always fits.
    fn can_transfer_to(
        &self,
        hopper_inventory: &dyn Inventory,
        _slot: usize,
        _stack: &ItemStack,
    ) -> bool {
        (0..hopper_inventory.size()).any(|slot| hopper_inventory.get_stack(slot).is_empty())
    }

    fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Relaxed);
        self.comparator_dirty.store(true, Ordering::Relaxed);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl Clearable for JukeboxBlockEntity {
    fn clear(&self) {
        let transition = self.lock_record_transition();
        *self
            .record_stack
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = ItemStack::EMPTY.clone();
        self.push_effect(RecordEffect::Stop);
        self.mark_dirty();
        drop(transition);
        self.drain_effects();
    }
}

/// Builds the Bedrock block-actor NBT for jukeboxes.
#[must_use]
pub fn build_jukebox_actor(
    position: BlockPos,
    record: Option<&ItemStack>,
    ticks_since_song_started: u64,
) -> NbtCompound {
    let mut builder = super::bedrock::BedrockBlockActorBuilder::new("Jukebox", position)
        .put_long("TicksSinceSongStarted", ticks_since_song_started as i64);

    if let Some(record_stack) = record
        && !record_stack.is_empty()
    {
        let mut rec_nbt = NbtCompound::new();
        rec_nbt.put_string(
            "Name",
            super::bedrock::bedrock_item_name(record_stack.item.registry_key),
        );
        rec_nbt.put_byte("Count", 1);
        builder = builder.put_compound("RecordItem", rec_nbt);
    }

    builder.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{Entity, ai::test_support::world, passive::allay::AllayEntity};
    use pumpkin_data::{entity::EntityType, item::Item};
    use pumpkin_util::math::{vector2::Vector2, vector3::Vector3};

    #[test]
    fn jukebox_actor_builder() {
        let pos = BlockPos::new(8, 64, 8);
        let record = ItemStack::new(1, &Item::MUSIC_DISC_CAT);
        let nbt = build_jukebox_actor(pos, Some(&record), 100);
        assert_eq!(nbt.get_string("id"), Some("Jukebox"));
        assert_eq!(nbt.get_long("TicksSinceSongStarted"), Some(100));
        let rec = nbt.get_compound("RecordItem").expect("RecordItem missing");
        assert_eq!(rec.get_string("Name"), Some("minecraft:music_disc_cat"));
    }

    #[tokio::test]
    async fn removing_the_record_through_inventory_stops_allay_dancing()
    -> Result<(), Box<dyn std::error::Error>> {
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        world.level.loaded_chunks.insert(
            Vector2::new(0, 0),
            pumpkin_world::chunk::ChunkData::empty_sync(0, 0),
        );
        let allay = AllayEntity::new(Entity::new(
            world.clone(),
            Vector3::new(1.5, 64.0, 0.5),
            &EntityType::ALLAY,
        ));
        world.add_entity_silent(allay.clone());
        let jukebox = JukeboxBlockEntity::new(BlockPos::new(0, 64, 0));
        jukebox.start_playing(100);
        jukebox.tick(&world);
        assert!(allay.is_dancing());
        jukebox.remove_stack(0);
        assert!(!allay.is_dancing());

        // A hopper takes the record through `update_stack`.
        jukebox.set_stack(0, ItemStack::new(1, &Item::MUSIC_DISC_CAT));
        jukebox.start_playing(100);
        jukebox.tick(&world);
        assert!(allay.is_dancing());
        let record = ItemStack::new(1, &Item::MUSIC_DISC_CAT);
        assert!(
            crate::block::entities::hopper::HopperBlockEntity::take_one(&jukebox, 0, &record)
                .is_some()
        );
        assert!(!allay.is_dancing());
        world.level.shutdown().await;
        Ok(())
    }
}
