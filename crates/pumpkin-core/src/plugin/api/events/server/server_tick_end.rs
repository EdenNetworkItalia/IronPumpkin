use pumpkin_macros::Event;

/// An event that fires at the end of every server tick.
///
/// `NeoForge`: `net.neoforged.neoforge.event.tick.ServerTickEvent.Post`, fired at the tail of
/// `MinecraftServer.tickServer`. The server is the `server` argument of the handler.
#[derive(Event, Clone)]
pub struct ServerTickEndEvent {
    /// 0-indexed number of the tick that just finished.
    pub tick: i32,

    /// Duration (in nanoseconds) of the tick that just finished.
    pub duration_nanos: i64,
}

impl ServerTickEndEvent {
    /// Creates a new `ServerTickEndEvent`.
    #[must_use]
    pub const fn new(tick: i32, duration_nanos: i64) -> Self {
        Self {
            tick,
            duration_nanos,
        }
    }

    /// Whether the tick has time left before its deadline (`ServerTickEvent#hasTime`). It reads
    /// the clock at each call, like the `BooleanSupplier` of `NeoForge`.
    #[must_use]
    pub fn has_time(&self) -> bool {
        crate::server::ticker::has_time()
    }
}
