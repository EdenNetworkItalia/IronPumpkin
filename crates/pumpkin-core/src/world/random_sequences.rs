use std::collections::HashMap;
use std::hash::{DefaultHasher, Hasher};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};

use pumpkin_util::identifier::Identifier;
use pumpkin_util::random::RandomImpl;
use pumpkin_util::random::xoroshiro128::Xoroshiro;

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
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            slot: Arc::new(SequenceSlot {
                rng: Mutex::new(Some(Xoroshiro::from_seed(seed))),
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
            rng,
        }
    }
}

/// The state of a random sequence during a roll. It goes back to the sequence when it drops.
pub struct CheckedOutSequence {
    slot: Arc<SequenceSlot>,
    rng: Xoroshiro,
}

impl CheckedOutSequence {
    /// The random source of the sequence.
    pub const fn rng(&mut self) -> &mut Xoroshiro {
        &mut self.rng
    }
}

impl Drop for CheckedOutSequence {
    fn drop(&mut self) {
        *self.slot.rng.lock().unwrap_or_else(PoisonError::into_inner) = Some(self.rng.clone());
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

    fn calculate_seed(
        sequence: &Identifier,
        world_seed: i64,
        salt: i32,
        include_world_seed: bool,
        include_sequence_id: bool,
    ) -> u64 {
        let base_seed = if include_world_seed { world_seed } else { 0 };
        let mut seed = (base_seed ^ i64::from(salt)) as u64;
        if include_sequence_id {
            let mut hasher = DefaultHasher::new();
            hasher.write(sequence.to_string().as_bytes());
            seed ^= hasher.finish();
        }
        seed
    }

    pub fn get_or_create(&mut self, sequence: &Identifier, world_seed: i64) -> &mut RandomSequence {
        let key = sequence.to_string();
        let salt = self.salt;
        let include_world_seed = self.include_world_seed;
        let include_sequence_id = self.include_sequence_id;
        self.sequences.entry(key).or_insert_with(|| {
            let seed = Self::calculate_seed(
                sequence,
                world_seed,
                salt,
                include_world_seed,
                include_sequence_id,
            );
            RandomSequence::new(seed)
        })
    }

    pub fn reset(&mut self, sequence: &Identifier, world_seed: i64) {
        let key = sequence.to_string();
        let seed = Self::calculate_seed(
            sequence,
            world_seed,
            self.salt,
            self.include_world_seed,
            self.include_sequence_id,
        );
        self.sequences.insert(key, RandomSequence::new(seed));
    }

    pub fn reset_with_options(
        &mut self,
        sequence: &Identifier,
        world_seed: i64,
        salt: i32,
        include_world_seed: bool,
        include_sequence_id: bool,
    ) {
        let key = sequence.to_string();
        let seed = Self::calculate_seed(
            sequence,
            world_seed,
            salt,
            include_world_seed,
            include_sequence_id,
        );
        self.sequences.insert(key, RandomSequence::new(seed));
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

    use super::*;

    /// A `/random` call and a second roll wait for a roll that holds the sequence. The drop of the
    /// roll wakes the `/random` call first, and the second roll must wake too.
    #[test]
    fn a_dropped_roll_wakes_every_waiter() {
        let sequence = RandomSequence::new(7);
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
