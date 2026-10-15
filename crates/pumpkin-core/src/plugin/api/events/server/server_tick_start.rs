use pumpkin_macros::Event;

/// An event that fires at the start of every server tick.
#[derive(Event, Clone)]
pub struct ServerTickStartEvent {
    /// 0-indexed number of the tick about to run.
    pub tick: i32,
}

impl ServerTickStartEvent {
    /// Creates a new `ServerTickStartEvent`.
    #[must_use]
    pub const fn new(tick: i32) -> Self {
        Self { tick }
    }

    /// Whether the tick has time left before its deadline. It reads the clock at each call.
    #[must_use]
    pub fn has_time(&self) -> bool {
        crate::server::ticker::has_time()
    }
}
