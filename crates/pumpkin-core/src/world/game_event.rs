use std::sync::Arc;

use pumpkin_data::{BlockStateId, game_event::GameEvent};
use pumpkin_util::math::{
    boundingbox::BoundingBox,
    position::{BlockPos, chunk_section_from_pos},
    vector3::Vector3,
};
use pumpkin_world::generation::section_coords::section_to_block;

use super::{World, entity_islands};
use crate::entity::EntityBase;

/// The context of a game event, vanilla `GameEvent.Context`: the entity that causes the event
/// and the block state that the event affects. Build it with the constructors, as the vanilla
/// `Context.of` calls.
pub struct GameEventContext<'a> {
    /// The entity that causes the event (`sourceEntity`).
    pub source_entity: Option<&'a dyn EntityBase>,
    /// The block state that the event affects (`affectedState`). A vibration listener ignores a
    /// state in the `dampens_vibrations` tag.
    pub affected_state: Option<BlockStateId>,
    /// The owner of `source_entity`, when the caller holds it. The cause of the event is then this
    /// entity, also after the world has removed it.
    owner: Option<&'a Arc<dyn EntityBase>>,
    /// The facts of the source entity that a listener reads, when the caller captured them. A
    /// queued fire captures them, because it cannot keep the borrowed source entity.
    facts: Option<SourceFacts>,
}

/// What the vanilla listeners read of the entity that causes a game event
/// (`VibrationSystem.Listener.isValidVibration`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceFacts {
    pub spectator: bool,
    pub stepping_carefully: bool,
    pub dampens_vibrations: bool,
}

impl<'a> GameEventContext<'a> {
    /// `Context.of((Entity) null)`: no entity and no state.
    pub const NONE: Self = Self {
        source_entity: None,
        affected_state: None,
        owner: None,
        facts: None,
    };

    /// `Context.of(entity)`.
    #[must_use]
    pub const fn of_entity(entity: Option<&'a dyn EntityBase>) -> Self {
        Self {
            source_entity: entity,
            affected_state: None,
            owner: None,
            facts: None,
        }
    }

    /// `Context.of(state)`.
    #[must_use]
    pub const fn of_state(state: BlockStateId) -> Self {
        Self {
            source_entity: None,
            affected_state: Some(state),
            owner: None,
            facts: None,
        }
    }

    /// `Context.of(entity, state)`.
    #[must_use]
    pub const fn of_entity_state(entity: Option<&'a dyn EntityBase>, state: BlockStateId) -> Self {
        Self {
            source_entity: entity,
            affected_state: Some(state),
            owner: None,
            facts: None,
        }
    }

    /// `Context.of(entity)` for an entity that the caller owns. The cause of the event is `owner`,
    /// also when the world does not list it: an entity that the world removed (the shulker bullet
    /// emits `entity_damage` after its removal) or that it has not yet added (a bee leaves its
    /// hive before it joins the world). Vanilla `Entity.kill` also emits `entity_die` after the
    /// removal, but Pumpkin emits no `entity_die` yet (#220).
    #[must_use]
    pub fn of_owner(owner: &'a Arc<dyn EntityBase>) -> Self {
        Self {
            source_entity: Some(owner.as_ref()),
            affected_state: None,
            owner: Some(owner),
            facts: None,
        }
    }

    /// `Context.of(entity, state)` for an entity that the caller owns, see [`Self::of_owner`].
    #[must_use]
    pub fn of_owner_state(owner: &'a Arc<dyn EntityBase>, state: BlockStateId) -> Self {
        Self {
            affected_state: Some(state),
            ..Self::of_owner(owner)
        }
    }

    /// The same context with `state` as the affected state, `None` for no state.
    #[must_use]
    pub const fn with_state(mut self, state: Option<BlockStateId>) -> Self {
        self.affected_state = state;
        self
    }

    /// The facts of the source entity, from the capture or from the entity itself.
    #[must_use]
    pub fn source_facts(&self) -> Option<SourceFacts> {
        self.facts.or_else(|| {
            self.source_entity.map(|entity| SourceFacts {
                spectator: entity.is_spectator(),
                stepping_carefully: entity.is_stepping_carefully(),
                dampens_vibrations: entity.dampens_vibrations(),
            })
        })
    }

    /// The same context with captured source facts.
    #[must_use]
    pub const fn with_facts(mut self, facts: Option<SourceFacts>) -> Self {
        self.facts = facts;
        self
    }

    /// The owner of the source entity, when the caller gave one.
    #[must_use]
    pub const fn owner(&self) -> Option<&'a Arc<dyn EntityBase>> {
        self.owner
    }
}

pub trait GameEventListener {
    fn listener_position(&self) -> Vector3<f64>;

    fn listener_radius(&self) -> i32;

    fn handle_game_event(
        &self,
        world: &World,
        event: GameEvent,
        context: &GameEventContext<'_>,
        source_position: Vector3<f64>,
    ) -> bool;
}

fn notified_sections(center: BlockPos, radius: i32) -> (Vector3<i32>, Vector3<i32>) {
    (
        chunk_section_from_pos(&center.offset(Vector3::new(-radius, -radius, -radius))),
        chunk_section_from_pos(&center.offset(Vector3::new(radius, radius, radius))),
    )
}

/// The chunk sections whose listeners an event at `position` can notify.
#[must_use]
pub fn notified_area(position: Vector3<f64>, radius: i32) -> BoundingBox {
    let (section_min, section_max) = notified_sections(BlockPos::floored_v(position), radius);
    let block_min = |section: i32| f64::from(section_to_block(section));
    BoundingBox::new(
        Vector3::new(
            block_min(section_min.x),
            block_min(section_min.y),
            block_min(section_min.z),
        ),
        Vector3::new(
            block_min(section_max.x + 1),
            block_min(section_max.y + 1),
            block_min(section_max.z + 1),
        ),
    )
}

impl World {
    pub(super) fn post_game_event(
        &self,
        event: GameEvent,
        position: Vector3<f64>,
        context: &GameEventContext<'_>,
    ) {
        let radius = event.notification_radius();
        let center = BlockPos::floored_v(position);
        let (section_min, section_max) = notified_sections(center, radius);
        let area = notified_area(position, radius);
        for entity in self.entity_index.listeners(self.entities.load_full(), area) {
            let Some(mob) = entity.get_mob() else {
                continue;
            };
            if !self
                .level
                .is_chunk_loaded(&entity.get_entity().chunk_pos.load())
            {
                continue;
            }
            mob.for_each_game_event_listener(&mut |listener| {
                let listener_block = BlockPos::floored_v(listener.listener_position());
                let section = chunk_section_from_pos(&listener_block);
                let listener_radius = listener.listener_radius();
                if section.x < section_min.x
                    || section.y < section_min.y
                    || section.z < section_min.z
                    || section.x > section_max.x
                    || section.y > section_max.y
                    || section.z > section_max.z
                    || listener_block.squared_distance(&center) > listener_radius * listener_radius
                {
                    return;
                }
                entity_islands::check_reached(entity.get_entity().entity_id);
                listener.handle_game_event(self, event, context, position);
            });
        }
    }
}
