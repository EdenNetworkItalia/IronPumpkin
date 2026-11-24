//! The ticking chunks of a world, with the two levels of vanilla `ChunkLevel`: entity ticking
//! (level 31 or lower) and block ticking (level 32 or lower).
//!
//! A ticket level spreads by one per chunk in a Chebyshev square. A player's simulation ticket
//! (`DistanceManager.getPlayerTicketLevel`, level `31 - simulation distance`) makes a square of
//! radius `simulation distance` entity ticking and a square one chunk wider block ticking. A
//! forced ticket (`ChunkMap.FORCED_TICKET_LEVEL`, level 31) makes its own chunk entity ticking
//! and its 8 neighbours block ticking.

use pumpkin_data::chunk_view_lut::{MAX_CHEBYSHEV_RADIUS, get_chebyshev_square};
use pumpkin_util::math::vector2::Vector2;
use rustc_hash::{FxHashMap, FxHashSet};
use uuid::Uuid;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct ActivePlayerArea {
    pub center: Vector2<i32>,
    pub simulation_distance: i32,
}

/// The ticking sets that the tracker writes, and the chunks that became block ticking.
pub(super) struct TickingSets<'a> {
    pub entity: &'a mut FxHashSet<Vector2<i32>>,
    pub block: &'a mut FxHashSet<Vector2<i32>>,
    pub newly_block_ticking: &'a mut Vec<Vector2<i32>>,
}

/// A source of ticking chunks: the chunks within `radius` are entity ticking, the chunks within
/// `radius + 1` block ticking.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Square {
    center: Vector2<i32>,
    radius: i32,
}

impl Square {
    fn new(center: Vector2<i32>, radius: i32) -> Self {
        // The offset table stops at `MAX_CHEBYSHEV_RADIUS`; the block square must fit in it, or
        // the offsets and `contains` disagree and the counts drift.
        Self {
            center,
            radius: radius.clamp(0, i32::from(MAX_CHEBYSHEV_RADIUS) - 1),
        }
    }

    const fn widened(self) -> Self {
        Self {
            center: self.center,
            radius: self.radius + 1,
        }
    }

    const fn contains(self, pos: Vector2<i32>) -> bool {
        (pos.x - self.center.x).abs() <= self.radius && (pos.y - self.center.y).abs() <= self.radius
    }

    fn chunks(self) -> impl Iterator<Item = Vector2<i32>> {
        get_chebyshev_square(self.radius as u8)
            .iter()
            .map(move |&(dx, dz)| self.center.add_raw(i32::from(dx), i32::from(dz)))
    }
}

#[derive(Default)]
pub(super) struct ActiveChunkTracker {
    pub players: FxHashMap<Uuid, ActivePlayerArea>,
    /// Block ticking chunks that are loaded and had their block entities migrated.
    pub loaded_block_ticking_chunks: FxHashSet<Vector2<i32>>,
    entity_counts: FxHashMap<Vector2<i32>, u32>,
    block_counts: FxHashMap<Vector2<i32>, u32>,
    forced_chunks: FxHashSet<Vector2<i32>>,
}

impl ActiveChunkTracker {
    fn add_entity(&mut self, pos: Vector2<i32>, sets: &mut TickingSets<'_>) {
        let count = self.entity_counts.entry(pos).or_default();
        *count += 1;
        if *count == 1 {
            sets.entity.insert(pos);
        }
    }

    fn remove_entity(&mut self, pos: Vector2<i32>, sets: &mut TickingSets<'_>) {
        let Some(count) = self.entity_counts.get_mut(&pos) else {
            return;
        };
        *count -= 1;
        if *count == 0 {
            self.entity_counts.remove(&pos);
            sets.entity.remove(&pos);
        }
    }

    fn add_block(&mut self, pos: Vector2<i32>, sets: &mut TickingSets<'_>) {
        let count = self.block_counts.entry(pos).or_default();
        *count += 1;
        if *count == 1 {
            sets.block.insert(pos);
            sets.newly_block_ticking.push(pos);
        }
    }

    fn remove_block(&mut self, pos: Vector2<i32>, sets: &mut TickingSets<'_>) {
        let Some(count) = self.block_counts.get_mut(&pos) else {
            return;
        };
        *count -= 1;
        if *count == 0 {
            self.block_counts.remove(&pos);
            self.loaded_block_ticking_chunks.remove(&pos);
            sets.block.remove(&pos);
        }
    }

    fn add_square(&mut self, square: Square, sets: &mut TickingSets<'_>) {
        for pos in square.chunks() {
            self.add_entity(pos, sets);
        }
        for pos in square.widened().chunks() {
            self.add_block(pos, sets);
        }
    }

    fn remove_square(&mut self, square: Square, sets: &mut TickingSets<'_>) {
        for pos in square.chunks() {
            self.remove_entity(pos, sets);
        }
        for pos in square.widened().chunks() {
            self.remove_block(pos, sets);
        }
    }

    /// Changes only the chunks that one square holds and the other does not.
    fn move_square(&mut self, previous: Square, current: Square, sets: &mut TickingSets<'_>) {
        for pos in previous.chunks().filter(|pos| !current.contains(*pos)) {
            self.remove_entity(pos, sets);
        }
        for pos in current.chunks().filter(|pos| !previous.contains(*pos)) {
            self.add_entity(pos, sets);
        }
        let (previous, current) = (previous.widened(), current.widened());
        for pos in previous.chunks().filter(|pos| !current.contains(*pos)) {
            self.remove_block(pos, sets);
        }
        for pos in current.chunks().filter(|pos| !previous.contains(*pos)) {
            self.add_block(pos, sets);
        }
    }

    /// Vanilla `DistanceManager.getPlayerTicketLevel` is `max(0, 31 - simulation distance)`, so
    /// entity ticking stops at a radius of 31.
    fn player_square(area: ActivePlayerArea) -> Square {
        Square::new(area.center, area.simulation_distance.min(31))
    }

    pub fn update_player(&mut self, id: Uuid, area: ActivePlayerArea, sets: &mut TickingSets<'_>) {
        match self.players.insert(id, area) {
            None => self.add_square(Self::player_square(area), sets),
            Some(previous) if previous != area => self.move_square(
                Self::player_square(previous),
                Self::player_square(area),
                sets,
            ),
            Some(_) => {}
        }
    }

    pub fn remove_player(&mut self, id: Uuid, sets: &mut TickingSets<'_>) {
        if let Some(area) = self.players.remove(&id) {
            self.remove_square(Self::player_square(area), sets);
        }
    }

    pub fn forced_chunks_equal(&self, forced_chunks: &FxHashSet<Vector2<i32>>) -> bool {
        self.forced_chunks == *forced_chunks
    }

    pub fn sync_forced_chunks(
        &mut self,
        forced_chunks: &FxHashSet<Vector2<i32>>,
        sets: &mut TickingSets<'_>,
    ) {
        let removed: Vec<_> = self
            .forced_chunks
            .difference(forced_chunks)
            .copied()
            .collect();
        let added: Vec<_> = forced_chunks
            .difference(&self.forced_chunks)
            .copied()
            .collect();
        for pos in removed {
            self.remove_square(Square::new(pos, 0), sets);
        }
        for pos in added {
            self.add_square(Square::new(pos, 0), sets);
        }
        self.forced_chunks.clone_from(forced_chunks);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(x: i32, z: i32, simulation_distance: i32) -> ActivePlayerArea {
        ActivePlayerArea {
            center: Vector2::new(x, z),
            simulation_distance,
        }
    }

    #[derive(Default)]
    struct Sets {
        entity: FxHashSet<Vector2<i32>>,
        block: FxHashSet<Vector2<i32>>,
        newly_block_ticking: Vec<Vector2<i32>>,
    }

    impl Sets {
        fn view(&mut self) -> TickingSets<'_> {
            TickingSets {
                entity: &mut self.entity,
                block: &mut self.block,
                newly_block_ticking: &mut self.newly_block_ticking,
            }
        }
    }

    #[test]
    fn follows_player_boundary_crossing() {
        let id = Uuid::from_u128(1);
        let mut tracker = ActiveChunkTracker::default();
        let mut sets = Sets::default();

        tracker.update_player(id, area(0, 0, 1), &mut sets.view());
        assert_eq!(sets.entity.len(), 9);
        assert_eq!(sets.block.len(), 25);
        sets.newly_block_ticking.clear();

        tracker.update_player(id, area(1, 0, 1), &mut sets.view());

        assert_eq!(sets.entity.len(), 9);
        assert_eq!(sets.block.len(), 25);
        assert_eq!(sets.newly_block_ticking.len(), 5);
        assert!(!sets.entity.contains(&Vector2::new(-1, 0)));
        assert!(sets.entity.contains(&Vector2::new(2, 0)));
        assert!(!sets.block.contains(&Vector2::new(-2, 0)));
        assert!(sets.block.contains(&Vector2::new(3, 0)));
    }

    /// The ring at the simulation distance plus one is block ticking only, like level 32 of
    /// vanilla `ChunkLevel`.
    #[test]
    fn the_ring_past_the_simulation_distance_is_block_ticking_only() {
        let mut tracker = ActiveChunkTracker::default();
        let mut sets = Sets::default();

        tracker.update_player(Uuid::from_u128(1), area(0, 0, 3), &mut sets.view());

        for (x, z) in [(3, 3), (-3, 0), (0, 3)] {
            let pos = Vector2::new(x, z);
            assert!(sets.entity.contains(&pos) && sets.block.contains(&pos));
        }
        for (x, z) in [(4, 4), (-4, 0), (0, 4), (4, -2)] {
            let pos = Vector2::new(x, z);
            assert!(!sets.entity.contains(&pos) && sets.block.contains(&pos));
        }
        assert!(!sets.block.contains(&Vector2::new(5, 0)));
    }

    #[test]
    fn a_forced_chunk_makes_its_neighbours_block_ticking() {
        let mut tracker = ActiveChunkTracker::default();
        let mut sets = Sets::default();
        let forced = FxHashSet::from_iter([Vector2::new(10, 10)]);

        tracker.sync_forced_chunks(&forced, &mut sets.view());

        assert_eq!(sets.entity, forced);
        assert_eq!(sets.block.len(), 9);
        assert!(sets.block.contains(&Vector2::new(9, 11)));
        assert_eq!(sets.newly_block_ticking.len(), 9);

        tracker.sync_forced_chunks(&FxHashSet::default(), &mut sets.view());
        assert!(sets.entity.is_empty() && sets.block.is_empty());
        assert!(tracker.entity_counts.is_empty() && tracker.block_counts.is_empty());
    }

    #[test]
    fn removes_chunks_when_player_leaves() {
        let id = Uuid::from_u128(1);
        let mut tracker = ActiveChunkTracker::default();
        let mut sets = Sets::default();

        tracker.update_player(id, area(0, 0, 1), &mut sets.view());
        tracker.remove_player(id, &mut sets.view());

        assert!(sets.entity.is_empty() && sets.block.is_empty());
        assert!(tracker.entity_counts.is_empty() && tracker.block_counts.is_empty());
    }

    #[test]
    fn keeps_overlaps_active_until_both_players_leave() {
        let first = Uuid::from_u128(1);
        let second = Uuid::from_u128(2);
        let mut tracker = ActiveChunkTracker::default();
        let mut sets = Sets::default();

        tracker.update_player(first, area(0, 0, 1), &mut sets.view());
        tracker.update_player(second, area(0, 0, 1), &mut sets.view());
        tracker.remove_player(first, &mut sets.view());

        assert_eq!(sets.entity.len(), 9);
        assert_eq!(sets.block.len(), 25);
        assert!(tracker.entity_counts.values().all(|count| *count == 1));
        assert!(tracker.block_counts.values().all(|count| *count == 1));

        tracker.remove_player(second, &mut sets.view());
        assert!(sets.entity.is_empty() && sets.block.is_empty());
    }

    /// At a simulation distance above 31, vanilla entity ticks a radius of 31 and block ticks a
    /// radius of 32.
    #[test]
    fn a_simulation_distance_above_31_ticks_as_31() {
        let mut tracker = ActiveChunkTracker::default();
        let mut sets = Sets::default();

        tracker.update_player(Uuid::from_u128(1), area(0, 0, 40), &mut sets.view());

        assert_eq!(sets.entity.len(), 63 * 63);
        assert_eq!(sets.block.len(), 65 * 65);
        assert!(sets.entity.contains(&Vector2::new(31, -31)));
        assert!(!sets.entity.contains(&Vector2::new(32, 0)));
        assert!(sets.block.contains(&Vector2::new(-32, 32)));
        assert!(!sets.block.contains(&Vector2::new(33, 0)));
    }

    /// A simulation distance past the offset table keeps the counts in agreement with the sets.
    #[test]
    fn a_simulation_distance_past_the_offset_table_keeps_the_counts() {
        let id = Uuid::from_u128(1);
        let mut tracker = ActiveChunkTracker::default();
        let mut sets = Sets::default();

        tracker.update_player(id, area(0, 0, 200), &mut sets.view());
        tracker.update_player(id, area(3, -2, 200), &mut sets.view());
        tracker.remove_player(id, &mut sets.view());

        assert!(sets.entity.is_empty() && sets.block.is_empty());
        assert!(tracker.entity_counts.is_empty() && tracker.block_counts.is_empty());
    }
}
