use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

use pumpkin_util::math::position::BlockPos;
use rustc_hash::FxHashSet;

use crate::tick::{MAX_TICK_DELAY, OrderedTick, ScheduledTick};

pub struct ChunkTickScheduler<T> {
    inner: Mutex<Option<Box<ChunkTickSchedulerInner<T>>>>,
    offset: AtomicUsize,
}

struct ChunkTickSchedulerInner<T> {
    tick_queue: [Vec<OrderedTick<T>>; MAX_TICK_DELAY],
    queued_ticks: FxHashSet<(BlockPos, T)>,
    /// The steps since this queue was created.
    step: u64,
    /// Due ticks that wait for the chunk to block tick, with the step at which each became due.
    overdue: Vec<(u64, OrderedTick<T>)>,
}

impl<T> ChunkTickSchedulerInner<T> {
    fn new() -> Box<Self> {
        Box::new(Self {
            tick_queue: std::array::from_fn(|_| Vec::new()),
            queued_ticks: FxHashSet::default(),
            step: 0,
            overdue: Vec::new(),
        })
    }
}

impl<'a, T: std::hash::Hash + Eq> ChunkTickScheduler<&'a T> {
    /// Advances the queue by one game tick and returns the due ticks when `ticking` is true. When
    /// it is false, the due ticks wait, as vanilla `LevelTicks.sortContainersToTick` keeps a
    /// container until `tickCheck` passes; the delay counts while the chunk is loaded.
    pub fn step_tick(&self, ticking: bool) -> Vec<OrderedTick<&'a T>> {
        // Atomic update for the offset
        let current_offset = self.offset.fetch_add(1, Ordering::SeqCst) % MAX_TICK_DELAY;
        let next_offset = (current_offset + 1) % MAX_TICK_DELAY;
        self.offset.store(next_offset, Ordering::SeqCst);

        let mut inner_guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(inner) = inner_guard.as_mut() else {
            return Vec::new();
        };

        let step = inner.step;
        inner.step += 1;
        let due = std::mem::take(&mut inner.tick_queue[current_offset]);
        if !ticking {
            inner
                .overdue
                .extend(due.into_iter().map(|tick| (step, tick)));
            return Vec::new();
        }
        let mut res: Vec<_> = inner
            .overdue
            .drain(..)
            .map(|(due_step, mut tick)| {
                tick.ticks_overdue = step - due_step;
                tick
            })
            .collect();
        res.extend(due);

        if !res.is_empty() {
            for next_tick in &res {
                inner
                    .queued_ticks
                    .remove(&(next_tick.position, next_tick.value));
            }
            if inner.queued_ticks.is_empty() {
                *inner_guard = None;
            }
        }
        res
    }

    pub fn schedule_tick(&self, tick: &ScheduledTick<&'a T>, sub_tick_order: u64) {
        let offset = self.offset.load(Ordering::SeqCst);
        let mut inner_guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let inner = inner_guard.get_or_insert_with(ChunkTickSchedulerInner::new);

        if inner.queued_ticks.insert((tick.position, tick.value)) {
            let index = (offset + tick.delay as usize) % MAX_TICK_DELAY;

            inner.tick_queue[index].push(OrderedTick {
                ticks_overdue: 0,
                priority: tick.priority,
                sub_tick_order,
                position: tick.position,
                value: tick.value,
            });
        }
    }

    pub fn is_scheduled(&self, pos: BlockPos, value: &T) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .is_some_and(|inner| inner.queued_ticks.contains(&(pos, value)))
    }

    pub fn clear_area(&self, min: &BlockPos, max: &BlockPos) {
        let mut inner_guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(inner) = inner_guard.as_mut() else {
            return;
        };

        let contains = |position: &BlockPos| {
            position.0.x >= min.0.x
                && position.0.x < max.0.x
                && position.0.y >= min.0.y
                && position.0.y < max.0.y
                && position.0.z >= min.0.z
                && position.0.z < max.0.z
        };

        for queue in &mut inner.tick_queue {
            queue.retain(|tick| !contains(&tick.position));
        }
        inner.overdue.retain(|(_, tick)| !contains(&tick.position));
        inner
            .queued_ticks
            .retain(|(position, _)| !contains(position));
        let became_empty = inner.queued_ticks.is_empty();

        if became_empty {
            *inner_guard = None;
        }
    }

    pub fn has_ticks(&self) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .is_some_and(|inner| !inner.queued_ticks.is_empty())
    }

    #[must_use]
    pub fn to_vec(&self) -> Vec<ScheduledTick<&'a T>> {
        let offset = self.offset.load(Ordering::SeqCst);
        let inner_guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(inner) = inner_guard.as_ref() else {
            return Vec::new();
        };

        // Vanilla saves an overdue tick with a negative delay; the saved delay here is unsigned.
        let mut res: Vec<_> = inner
            .overdue
            .iter()
            .map(|(_, x)| ScheduledTick {
                delay: 0,
                priority: x.priority,
                position: x.position,
                value: x.value,
            })
            .collect();

        for i in 0..MAX_TICK_DELAY {
            let index = (offset + i) % MAX_TICK_DELAY;
            res.extend(inner.tick_queue[index].iter().map(|x| ScheduledTick {
                delay: i as u8,
                priority: x.priority,
                position: x.position,
                value: x.value,
            }));
        }
        res
    }
}

impl<'a, T: std::hash::Hash + Eq + 'static> FromIterator<ScheduledTick<&'a T>>
    for ChunkTickScheduler<&'a T>
{
    fn from_iter<I: IntoIterator<Item = ScheduledTick<&'a T>>>(iter: I) -> Self {
        let scheduler = Self::default();
        let iter = iter.into_iter();

        let (lower, _) = iter.size_hint();
        if lower > 0 {
            let mut inner_guard = scheduler
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let inner = inner_guard.get_or_insert_with(ChunkTickSchedulerInner::new);
            inner.queued_ticks.reserve(lower);
        }

        for tick in iter {
            scheduler.schedule_tick(&tick, 0);
        }
        scheduler
    }
}

impl<T> Default for ChunkTickScheduler<T> {
    fn default() -> Self {
        Self {
            inner: Mutex::new(None),
            offset: AtomicUsize::new(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use pumpkin_data::Block;
    use pumpkin_util::math::position::BlockPos;

    use super::ChunkTickScheduler;
    use crate::tick::{ScheduledTick, TickPriority};

    const LAMP: &Block = &Block::REDSTONE_LAMP;
    const TORCH: &Block = &Block::REDSTONE_TORCH;

    fn tick(
        x: i32,
        delay: u8,
        priority: TickPriority,
        value: &'static Block,
    ) -> ScheduledTick<&'static Block> {
        ScheduledTick {
            delay,
            priority,
            position: BlockPos::new(x, 0, 0),
            value,
        }
    }

    /// A due tick in a loaded chunk that does not block tick waits, and runs on the first step
    /// that ticks, as vanilla `LevelTicks` runs a tick whose trigger tick has passed.
    #[test]
    fn a_due_tick_waits_while_the_chunk_does_not_tick() {
        let scheduler = ChunkTickScheduler::default();
        scheduler.schedule_tick(&tick(0, 3, TickPriority::Normal, LAMP), 0);
        scheduler.schedule_tick(&tick(1, 20, TickPriority::Normal, LAMP), 1);

        for _ in 0..10 {
            assert!(scheduler.step_tick(false).is_empty());
        }
        assert!(scheduler.is_scheduled(BlockPos::new(0, 0, 0), LAMP));
        let saved: Vec<_> = scheduler
            .to_vec()
            .into_iter()
            .map(|t| (t.position.0.x, t.delay))
            .collect();
        assert_eq!(saved, [(0, 0), (1, 10)]);

        let due = scheduler.step_tick(true);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].position, BlockPos::new(0, 0, 0));
        assert_eq!(due[0].ticks_overdue, 7);
        assert!(!scheduler.is_scheduled(BlockPos::new(0, 0, 0), LAMP));
        assert!(scheduler.has_ticks());
    }

    /// Vanilla `ScheduledTick.DRAIN_ORDER`: the earlier trigger tick runs first, whatever the
    /// priority.
    #[test]
    fn an_overdue_tick_runs_before_a_tick_of_a_higher_priority() {
        let scheduler = ChunkTickScheduler::default();
        scheduler.schedule_tick(&tick(0, 1, TickPriority::Low, LAMP), 0);
        scheduler.schedule_tick(&tick(1, 3, TickPriority::Normal, LAMP), 1);
        scheduler.schedule_tick(&tick(2, 3, TickPriority::ExtremelyHigh, TORCH), 2);
        for _ in 0..3 {
            scheduler.step_tick(false);
        }

        let mut due = scheduler.step_tick(true);
        due.sort_unstable();
        let order: Vec<_> = due.iter().map(|t| t.position.0.x).collect();
        assert_eq!(order, [0, 2, 1]);
    }

    #[test]
    fn clear_area_removes_an_overdue_tick() {
        let scheduler = ChunkTickScheduler::default();
        scheduler.schedule_tick(&tick(0, 1, TickPriority::Normal, LAMP), 0);
        scheduler.step_tick(false);
        scheduler.step_tick(false);

        scheduler.clear_area(&BlockPos::new(-1, -1, -1), &BlockPos::new(1, 1, 1));

        assert!(!scheduler.has_ticks());
        assert!(scheduler.step_tick(true).is_empty());
    }
}
