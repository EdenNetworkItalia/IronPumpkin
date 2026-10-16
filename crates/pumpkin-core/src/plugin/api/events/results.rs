//! The result types of the `NeoForge` events that have a result setter.

/// A result with a default, as `net.neoforged.neoforge.common.util.TriState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TriState {
    /// Allow the action, also where vanilla would refuse it.
    True,
    /// Let vanilla decide.
    #[default]
    Default,
    /// Refuse the action.
    False,
}

/// The result of an interaction, as `net.minecraft.world.InteractionResult`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionResult {
    /// `SUCCESS`: the action ran, and the other players see the arm swing.
    Success,
    /// `SUCCESS_SERVER`: the action ran, and every player sees the arm swing.
    SuccessServer,
    /// `CONSUME`: the action ran, without a swing.
    Consume,
    Fail,
    Pass,
    /// `TRY_WITH_EMPTY_HAND`: the block use goes on as with an empty hand.
    TryWithEmptyHand,
}
