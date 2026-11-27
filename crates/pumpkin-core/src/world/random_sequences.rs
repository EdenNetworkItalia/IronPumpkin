use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};

use pumpkin_util::identifier::Identifier;
use pumpkin_util::random::xoroshiro128::Xoroshiro;
use pumpkin_util::random::{RandomGenerator, RandomImpl};
use pumpkin_world::world_info::data_files::RandomSequencesData;

/// A single random sequence wrapper.
///
/// A roll that draws from the sequence checks it out for the whole roll: see
/// [`SequenceHandle::check_out`].
pub struct RandomSequence {
    slot: Arc<SequenceSlot>,
}

/// The state of a sequence. It is `None` while a roll has the sequence checked out.
struct SequenceSlot {
    rng: Mutex<Option<Xoroshiro>>,
    returned: Condvar,
}

impl SequenceSlot {
    /// Waits until no roll has the sequence checked out and locks the state.
    fn wait_ready(&self) -> MutexGuard<'_, Option<Xoroshiro>> {
        let guard = self.rng.lock().unwrap_or_else(PoisonError::into_inner);
        self.returned
            .wait_while(guard, |rng| rng.is_none())
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl RandomSequence {
    /// Vanilla `RandomSequence.createSequence`: the 128-bit upgrade of `seed` without the mix,
    /// XOR the MD5 seed of `key` when there is one, then the Stafford 13 mix of both words.
    #[must_use]
    pub fn new(seed: i64, key: Option<&Identifier>) -> Self {
        let (mut lo, mut hi) = Xoroshiro::upgrade_seed_to_128bit_unmixed(seed as u64);
        if let Some(key) = key {
            let (key_lo, key_hi) = Xoroshiro::seed_from_hash_of(&key.to_string());
            lo ^= key_lo;
            hi ^= key_hi;
        }
        Self::from_state(
            Xoroshiro::mix_stafford_13(lo),
            Xoroshiro::mix_stafford_13(hi),
        )
    }

    /// A sequence at the saved 128-bit state `(lo, hi)`.
    #[must_use]
    pub fn from_state(lo: u64, hi: u64) -> Self {
        Self {
            slot: Arc::new(SequenceSlot {
                rng: Mutex::new(Some(Xoroshiro::from_state(lo, hi))),
                returned: Condvar::new(),
            }),
        }
    }

    /// A handle to check the sequence out without the lock of the sequence manager. A reset of
    /// the sequence replaces it: the handle goes on with the state it had, as vanilla `reset`
    /// leaves a roll that holds the old source alone.
    #[must_use]
    pub fn handle(&self) -> SequenceHandle {
        SequenceHandle(self.slot.clone())
    }
}

/// A handle to a [`RandomSequence`].
#[derive(Clone)]
pub struct SequenceHandle(Arc<SequenceSlot>);

impl SequenceHandle {
    /// Draws a value for `/random`. It waits while a roll has the sequence checked out. Call it
    /// without the lock of the sequence manager.
    #[must_use]
    pub fn random_between_inclusive(&self, min: i32, max: i32) -> i32 {
        if min >= max {
            return min;
        }
        self.0
            .wait_ready()
            .as_mut()
            .map_or(min, |rng| rng.next_inbetween_i32(min, max))
    }

    /// The 128-bit state `(lo, hi)` to save. It waits while a roll has the sequence checked out.
    /// Call it without the lock of the sequence manager.
    #[must_use]
    pub fn state(&self) -> (u64, u64) {
        self.0
            .wait_ready()
            .as_ref()
            .map_or((0, 0), Xoroshiro::state)
    }

    /// Takes the state of the sequence for a roll. A second check out waits until the first one
    /// drops, so two rolls never draw the same values. Call it without the lock of the sequence
    /// manager.
    #[must_use]
    pub fn check_out(&self) -> CheckedOutSequence {
        // `wait_ready` returns with the state present, so the fallback never runs.
        let rng = self
            .0
            .wait_ready()
            .take()
            .unwrap_or_else(|| Xoroshiro::from_seed(0));
        CheckedOutSequence {
            slot: self.0.clone(),
            rng: RandomGenerator::Xoroshiro(rng),
        }
    }
}

/// The state of a random sequence during a roll. It goes back to the sequence when it drops.
pub struct CheckedOutSequence {
    slot: Arc<SequenceSlot>,
    /// Always the `Xoroshiro` variant: a sequence is a `XoroshiroRandomSource`.
    rng: RandomGenerator,
}

impl CheckedOutSequence {
    /// The random source of the sequence.
    pub const fn rng(&mut self) -> &mut RandomGenerator {
        &mut self.rng
    }
}

impl Drop for CheckedOutSequence {
    fn drop(&mut self) {
        let RandomGenerator::Xoroshiro(rng) = &self.rng else {
            return;
        };
        *self.slot.rng.lock().unwrap_or_else(PoisonError::into_inner) = Some(rng.clone());
        // Every waiter checks the state again: a waiter that only reads the state (`/random`)
        // must not take the wake up of a roll.
        self.slot.returned.notify_all();
    }
}

/// Persistent/runtime manager for server random sequences.
pub struct RandomSequences {
    salt: i32,
    include_world_seed: bool,
    include_sequence_id: bool,
    sequences: HashMap<String, RandomSequence>,
}

impl Default for RandomSequences {
    fn default() -> Self {
        Self::new()
    }
}

/// What a save of [`RandomSequences`] needs. It leaves the lock of the manager before it waits
/// for the rolls that hold a sequence.
pub struct RandomSequencesSnapshot {
    salt: i32,
    include_world_seed: bool,
    include_sequence_id: bool,
    sequences: Vec<(String, SequenceHandle)>,
}

impl RandomSequencesSnapshot {
    /// The content of `random_sequences.dat`. It waits while a roll has a sequence checked out.
    #[must_use]
    pub fn into_data(self, data_version: i32) -> RandomSequencesData {
        RandomSequencesData {
            salt: self.salt,
            include_world_seed: self.include_world_seed,
            include_sequence_id: self.include_sequence_id,
            sequences: self
                .sequences
                .into_iter()
                .map(|(id, handle)| {
                    let (lo, hi) = handle.state();
                    (id, (lo as i64, hi as i64))
                })
                .collect(),
            data_version,
        }
    }
}

impl RandomSequences {
    #[must_use]
    pub fn new() -> Self {
        Self {
            salt: 0,
            include_world_seed: true,
            include_sequence_id: true,
            sequences: HashMap::new(),
        }
    }

    /// The manager of a saved `random_sequences.dat`.
    #[must_use]
    pub fn from_data(data: &RandomSequencesData) -> Self {
        Self {
            salt: data.salt,
            include_world_seed: data.include_world_seed,
            include_sequence_id: data.include_sequence_id,
            sequences: data
                .sequences
                .iter()
                .map(|(id, (lo, hi))| {
                    (
                        id.clone(),
                        RandomSequence::from_state(*lo as u64, *hi as u64),
                    )
                })
                .collect(),
        }
    }

    /// The handles of all sequences, to save them after the lock of the manager is free.
    #[must_use]
    pub fn snapshot(&self) -> RandomSequencesSnapshot {
        RandomSequencesSnapshot {
            salt: self.salt,
            include_world_seed: self.include_world_seed,
            include_sequence_id: self.include_sequence_id,
            sequences: self
                .sequences
                .iter()
                .map(|(id, sequence)| (id.clone(), sequence.handle()))
                .collect(),
        }
    }

    /// Vanilla `RandomSequences.createSequence`.
    fn create_sequence(
        sequence: &Identifier,
        world_seed: i64,
        salt: i32,
        include_world_seed: bool,
        include_sequence_id: bool,
    ) -> RandomSequence {
        let seed = (if include_world_seed { world_seed } else { 0 }) ^ i64::from(salt);
        RandomSequence::new(seed, include_sequence_id.then_some(sequence))
    }

    pub fn get_or_create(&mut self, sequence: &Identifier, world_seed: i64) -> &mut RandomSequence {
        let key = sequence.to_string();
        let salt = self.salt;
        let include_world_seed = self.include_world_seed;
        let include_sequence_id = self.include_sequence_id;
        self.sequences.entry(key).or_insert_with(|| {
            Self::create_sequence(
                sequence,
                world_seed,
                salt,
                include_world_seed,
                include_sequence_id,
            )
        })
    }

    pub fn reset(&mut self, sequence: &Identifier, world_seed: i64) {
        self.reset_with_options(
            sequence,
            world_seed,
            self.salt,
            self.include_world_seed,
            self.include_sequence_id,
        );
    }

    pub fn reset_with_options(
        &mut self,
        sequence: &Identifier,
        world_seed: i64,
        salt: i32,
        include_world_seed: bool,
        include_sequence_id: bool,
    ) {
        self.sequences.insert(
            sequence.to_string(),
            Self::create_sequence(
                sequence,
                world_seed,
                salt,
                include_world_seed,
                include_sequence_id,
            ),
        );
    }

    pub fn clear(&mut self) -> usize {
        let count = self.sequences.len();
        self.sequences.clear();
        count
    }

    pub const fn set_seed_defaults(
        &mut self,
        salt: i32,
        include_world_seed: bool,
        include_sequence_id: bool,
    ) {
        self.salt = salt;
        self.include_world_seed = include_world_seed;
        self.include_sequence_id = include_sequence_id;
    }

    #[must_use]
    pub fn get_sequence_keys(&self) -> Vec<String> {
        self.sequences.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::mpsc, thread, time::Duration};

    use pumpkin_world::world_info::data_files::{read_random_sequences, write_random_sequences};

    use super::*;

    const WORLD_SEED: i64 = -4_172_144_997_902_289_642;

    fn first_values(sequences: &mut RandomSequences, key: &str, count: usize) -> Vec<i64> {
        let id = Identifier::parse(key).unwrap();
        let mut sequence = sequences
            .get_or_create(&id, WORLD_SEED)
            .handle()
            .check_out();
        (0..count).map(|_| sequence.rng().next_i64()).collect()
    }

    /// The expected values come from a Python transcription of the decompiled 26.3 source
    /// (`RandomSequences.createSequence`, `RandomSequence.createSequence`,
    /// `RandomSupport.upgradeSeedTo128bitUnmixed`, `seedFromHashOf`, `mixStafford13` and
    /// `Xoroshiro128PlusPlus.nextLong`), not from this code.
    #[test]
    fn a_sequence_seeds_as_vanilla() {
        const DUNGEON: &str = "minecraft:chests/simple_dungeon";
        let mut sequences = RandomSequences::new();
        assert_eq!(
            first_values(&mut sequences, DUNGEON, 3),
            [
                1_427_891_962_796_759_513,
                309_051_494_640_195_820,
                -9_061_685_028_857_652_198
            ]
        );

        // The salt is XORed into the world seed.
        let mut sequences = RandomSequences::new();
        sequences.set_seed_defaults(7, true, true);
        assert_eq!(
            first_values(&mut sequences, DUNGEON, 3),
            [
                4_336_167_025_769_947_313,
                -7_275_560_245_944_630_756,
                -6_429_347_028_076_903_911
            ]
        );

        // Without the world seed, the seed is the salt alone.
        let mut sequences = RandomSequences::new();
        sequences.set_seed_defaults(0, false, true);
        assert_eq!(
            first_values(&mut sequences, DUNGEON, 3),
            [
                -7_509_090_172_973_561_982,
                8_431_096_442_898_752_500,
                -5_299_148_278_949_906_928
            ]
        );

        // Without the sequence id, the MD5 seed of the key is left out.
        let mut sequences = RandomSequences::new();
        sequences.set_seed_defaults(0, true, false);
        assert_eq!(
            first_values(&mut sequences, DUNGEON, 3),
            [
                -517_336_468_233_864_830,
                3_486_191_825_353_438_332,
                7_544_346_269_521_381_194
            ]
        );

        // `reset` with options uses the options, not the defaults of the manager.
        let mut sequences = RandomSequences::new();
        let id = Identifier::parse(DUNGEON).unwrap();
        sequences.reset_with_options(&id, WORLD_SEED, 7, true, true);
        assert_eq!(
            first_values(&mut sequences, DUNGEON, 1),
            [4_336_167_025_769_947_313]
        );
    }

    /// A save and a reload give each sequence its state back, so the stream goes on.
    #[test]
    fn a_saved_sequence_goes_on_after_a_reload() {
        const FISHING: &str = "minecraft:gameplay/fishing";
        let folder = tempfile::tempdir().unwrap();
        let mut sequences = RandomSequences::new();
        sequences.set_seed_defaults(3, true, true);
        let mut expected = RandomSequences::new();
        expected.set_seed_defaults(3, true, true);
        let stream = first_values(&mut expected, FISHING, 4);
        assert_eq!(
            stream,
            [
                1_144_178_146_449_311_450,
                -7_117_936_853_665_482_912,
                6_175_020_934_306_257_829,
                2_754_780_967_605_295_328
            ]
        );
        assert_eq!(first_values(&mut sequences, FISHING, 2), stream[..2]);

        // A roll that is open during the save is waited for, not skipped.
        let handle = sequences
            .get_or_create(&Identifier::parse(FISHING).unwrap(), WORLD_SEED)
            .handle();
        let snapshot = sequences.snapshot();
        let roll = handle.check_out();
        let saver = thread::spawn(move || snapshot.into_data(5023));
        thread::sleep(Duration::from_millis(200));
        assert!(!saver.is_finished(), "the save waits for the roll");
        drop(roll);
        let data = saver.join().unwrap();
        // The state after two draws, from the Python transcription.
        assert_eq!(
            data.sequences.get(FISHING),
            Some(&(-2_575_714_336_028_842_434, 4_499_803_302_146_347_433))
        );

        write_random_sequences(folder.path(), &data).unwrap();
        let reloaded = read_random_sequences(folder.path());
        assert_eq!(reloaded, data);
        let mut reloaded = RandomSequences::from_data(&reloaded);
        assert_eq!(first_values(&mut reloaded, FISHING, 2), stream[2..]);
        // The options come back too, so a new sequence seeds with them.
        let mut fresh = RandomSequences::new();
        fresh.set_seed_defaults(3, true, true);
        assert_eq!(
            first_values(&mut reloaded, "minecraft:other", 2),
            first_values(&mut fresh, "minecraft:other", 2)
        );
    }

    /// A `/random` call and a second roll wait for a roll that holds the sequence. The drop of the
    /// roll wakes the `/random` call first, and the second roll must wake too.
    #[test]
    fn a_dropped_roll_wakes_every_waiter() {
        let sequence = RandomSequence::new(7, None);
        let first = sequence.handle().check_out();
        let (sender, receiver) = mpsc::channel();

        let command = sequence.handle();
        let command_sender = sender.clone();
        let command = thread::spawn(move || {
            let _ = command.random_between_inclusive(0, 9);
            command_sender.send("command").unwrap();
        });
        // The `/random` call waits before the second roll does.
        thread::sleep(Duration::from_millis(200));
        let second = sequence.handle();
        let second = thread::spawn(move || {
            drop(second.check_out());
            sender.send("roll").unwrap();
        });
        thread::sleep(Duration::from_millis(200));
        assert!(receiver.try_recv().is_err(), "both wait for the first roll");

        drop(first);
        let mut done = [
            receiver.recv_timeout(Duration::from_secs(3)).unwrap(),
            receiver.recv_timeout(Duration::from_secs(3)).unwrap(),
        ];
        done.sort_unstable();
        assert_eq!(done, ["command", "roll"]);
        command.join().unwrap();
        second.join().unwrap();
    }

    /// Review 171: `/random` waited while it held the lock of the manager, so a roll of any
    /// sequence waited too. The handle leaves the manager before the wait.
    #[test]
    fn a_waiting_command_does_not_hold_the_manager() {
        let sequences = Arc::new(Mutex::new(RandomSequences::new()));
        let id = Identifier::parse("minecraft:busy").unwrap();
        let handle = sequences.lock().unwrap().get_or_create(&id, 1).handle();
        let roll = handle.check_out();

        let waiting = sequences.lock().unwrap().get_or_create(&id, 1).handle();
        let command = thread::spawn(move || waiting.random_between_inclusive(0, 9));
        thread::sleep(Duration::from_millis(200));
        // The manager answers for another sequence while the command waits.
        let (sender, receiver) = mpsc::channel();
        let manager = sequences;
        thread::spawn(move || {
            let other = Identifier::parse("minecraft:other").unwrap();
            manager.lock().unwrap().get_or_create(&other, 1);
            sender.send(()).unwrap();
        });
        receiver.recv_timeout(Duration::from_secs(3)).unwrap();

        drop(roll);
        command.join().unwrap();
    }
}
