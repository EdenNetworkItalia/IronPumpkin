use pumpkin_macros::{Event, cancellable};
use pumpkin_util::math::vector3::Vector3;

/// An event that occurs when a generic game event is triggered in a world.
///
/// `NeoForge`: `net.neoforged.neoforge.event.VanillaGameEvent`, fired by
/// `CommonHooks.onVanillaGameEvent` for every vanilla game event. Pumpkin has no game event
/// dispatch: `World::emit_game_event` fires this event from a few call sites, and a cancel stops
/// no listener.
#[cancellable]
#[derive(Event, Clone)]
pub struct GenericGameEvent {
    /// The key or type of the game event.
    pub event_key: String,

    /// The position where the game event occurred.
    pub position: Vector3<f64>,
}

impl GenericGameEvent {
    #[must_use]
    pub const fn new(event_key: String, position: Vector3<f64>) -> Self {
        Self {
            event_key,
            position,
            cancelled: false,
        }
    }
}
