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
    Success,
    Consume,
    Fail,
    Pass,
}
