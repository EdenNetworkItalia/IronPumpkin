use std::sync::Arc;

use pumpkin_data::BlockStateId;
use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::vector3::Vector3;

use crate::{entity::EntityBase, world::World};

/// A vanilla game event is emitted in a world.
///
/// `NeoForge`: `VanillaGameEvent`, fired by `CommonHooks.onVanillaGameEvent` in
/// `ServerLevel.gameEvent`. [`World::emit_game_event_with_context`] is the one emit path, so the
/// event fires for every game event that the server emits. A cancel stops the vanilla
/// listeners, such as the vibration listeners. The event has no other setter, as in `NeoForge`: use
/// the getters for the key and the position.
#[cancellable]
#[derive(Event, Clone)]
pub struct GenericGameEvent {
    /// The world of the game event (`getLevel`).
    pub world: Arc<World>,

    /// The key or type of the game event (`getVanillaEvent`), such as `block_activate`.
    event_key: String,

    /// The position where the game event occurred (`getEventPosition`).
    position: Vector3<f64>,

    /// The entity that caused the game event (`getCause`). It is set for an entity that the world
    /// lists and for an entity that the emitting code owns, also after its removal.
    pub cause: Option<Arc<dyn EntityBase>>,

    /// The block state that the event affects (`getContext().affectedState()`).
    affected_state: Option<BlockStateId>,
}

impl GenericGameEvent {
    /// The key of the game event (`getVanillaEvent`), such as `block_activate`.
    #[must_use]
    pub fn event_key(&self) -> &str {
        &self.event_key
    }

    /// The position of the game event (`getEventPosition`).
    #[must_use]
    pub const fn position(&self) -> Vector3<f64> {
        self.position
    }

    /// The block state that the event affects (`getContext().affectedState()`).
    #[must_use]
    pub const fn affected_state(&self) -> Option<BlockStateId> {
        self.affected_state
    }

    /// Sets the affected block state of a new event.
    #[must_use]
    pub const fn with_affected_state(mut self, state: Option<BlockStateId>) -> Self {
        self.affected_state = state;
        self
    }

    #[must_use]
    pub const fn new(
        world: Arc<World>,
        event_key: String,
        position: Vector3<f64>,
        cause: Option<Arc<dyn EntityBase>>,
    ) -> Self {
        Self {
            world,
            event_key,
            position,
            cause,
            affected_state: None,
            cancelled: false,
        }
    }
}
