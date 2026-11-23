use pumpkin_macros::{Event, cancellable};

use crate::command::CommandSource;

/// A command is about to run.
///
/// `NeoForge`: `net.neoforged.neoforge.event.CommandEvent`, fired by `Commands.performCommand`
/// after the parse and before the execution, for every command that a sender performs: a player,
/// the console, a command block, a sign and the remote consoles. A command of a function does
/// not fire it. The event fires before the parse, and `command` is the input that the parse
/// reads: Pumpkin has no parse result that a handler could keep, so setting `command` replaces
/// the parse result, and the dispatcher parses the new command.
///
/// A cancel stops the command. With `exception` set, a cancel also sends the message to the
/// sender as a failure and logs it.
#[cancellable]
#[derive(Event, Clone)]
pub struct CommandEvent {
    /// The sender of the command.
    pub source: CommandSource,
    /// The command without its leading slash.
    pub command: String,
    /// The failure message of a cancel (`setException`).
    pub exception: Option<String>,
}

impl CommandEvent {
    #[must_use]
    pub const fn new(source: CommandSource, command: String) -> Self {
        Self {
            source,
            command,
            exception: None,
            cancelled: false,
        }
    }
}
