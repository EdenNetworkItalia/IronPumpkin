use pumpkin_macros::Event;

/// An event that fires at the start of every server tick.
///
/// `NeoForge`: `net.neoforged.neoforge.event.tick.ServerTickEvent.Pre`, fired at the head of
/// `MinecraftServer.tickServer`. The server is the `server` argument of the handler.
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

    /// Whether the tick has time left before its deadline (`ServerTickEvent#hasTime`). It reads
    /// the clock at each call, like the `BooleanSupplier` of `NeoForge`.
    #[must_use]
    pub fn has_time(&self) -> bool {
        crate::server::ticker::has_time()
    }
}
