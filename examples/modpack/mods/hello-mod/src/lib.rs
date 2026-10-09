//! Example native mod. It registers `/hello`, which answers with the mod id, and a block and an
//! item.

use ironpumpkin_mods::{
    ModInit, NativeMod,
    command::{
        argument_builder::{ArgumentBuilder, command},
        context::command_context::CommandContext,
        node::{CommandExecutor, CommandExecutorResult},
    },
    content::{BlockBuilder, ItemBuilder, RegistryError},
    permission::{Permission, PermissionDefault},
    pumpkin_data::{Block, item::Item, translation::java::CHAT_SQUARE_BRACKETS},
    register_mod,
    text::{TextComponent, translate_cross},
    tracing::error,
};

const ID: &str = "hello-mod";
const DESCRIPTION: &str = "Answers with the id of the mod that registered it.";
const LAMP: &str = "hello-mod:greeter_lamp";

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
        // Log a registration error and go on without the content. Do not panic: `init` has no
        // unwind guard. A clean server exit on a content error is a follow-up.
        if let Err(err) = register_content(cx) {
            error!("[{ID}] cannot register the content: {err}");
        }
    }
}

/// A lamp that vanilla clients see as a redstone lamp, and the item that places it. Content
/// registers at once and returns the error of the registry.
fn register_content(cx: &mut ModInit) -> Result<(), RegistryError> {
    cx.register_block(
        BlockBuilder::new(LAMP, Block::REDSTONE_LAMP.default_state.id).bool_property("lit", false),
    )?;
    cx.register_item(ItemBuilder::new(LAMP, &Item::REDSTONE_LAMP).places(LAMP))
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
