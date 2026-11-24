use std::sync::Arc;

use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::vector3::Vector3;

use crate::{entity::EntityBase, world::World};

/// A vanilla game event is emitted in a world.
///
/// `NeoForge`: `VanillaGameEvent`, fired by `CommonHooks.onVanillaGameEvent` in
/// `ServerLevel.gameEvent`. [`World::emit_game_event_with_context`] is the one emit path, so the
/// event fires for every game event that the server emits. A cancel stops the vanilla
/// listeners, such as the vibration listeners. `event_key` and `position` are settable, which
/// `NeoForge` does not offer.
#[cancellable]
#[derive(Event, Clone)]
pub struct GenericGameEvent {
    /// The world of the game event (`getLevel`).
    pub world: Arc<World>,

    /// The key or type of the game event (`getVanillaEvent`), such as `block_activate`.
    pub event_key: String,

    /// The position where the game event occurred (`getEventPosition`).
    pub position: Vector3<f64>,

    /// The entity that caused the game event, if it still lives (`getCause`).
    pub cause: Option<Arc<dyn EntityBase>>,
}

impl GenericGameEvent {
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
            cancelled: false,
        }
    }
}
