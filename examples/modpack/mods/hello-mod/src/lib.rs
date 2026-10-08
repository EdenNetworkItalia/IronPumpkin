//! Example native mod. It registers `/hello`, which answers with the mod id.

use ironpumpkin_mods::{
    ModInit, NativeMod,
    command::{
        argument_builder::{ArgumentBuilder, command},
        context::command_context::CommandContext,
        node::{CommandExecutor, CommandExecutorResult},
    },
    permission::{Permission, PermissionDefault},
    pumpkin_data::translation::java::CHAT_SQUARE_BRACKETS,
    register_mod,
    text::{TextComponent, translate_cross},
};

const ID: &str = "hello-mod";
const DESCRIPTION: &str = "Answers with the id of the mod that registered it.";

struct HelloMod;

impl NativeMod for HelloMod {
    fn id(&self) -> &'static str {
        ID
    }

    fn display_name(&self) -> &'static str {
        "Hello Mod"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn init(&self, cx: &mut ModInit) {
        cx.register_permission(Permission::new(
            &format!("{ID}:command.hello"),
            DESCRIPTION,
            PermissionDefault::Allow,
        ));
        cx.register_command(
            command("hello", DESCRIPTION).executes(HelloExecutor),
            "command.hello",
        );
    }
}

register_mod!(HelloMod);

struct HelloExecutor;

impl CommandExecutor for HelloExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        context.source.send_feedback(
            translate_cross!(
                CHAT_SQUARE_BRACKETS,
                CHAT_SQUARE_BRACKETS,
                TextComponent::text(format!("Hello from {ID}"))
            ),
            false,
        );
        Ok(1)
    }
}
