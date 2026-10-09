//! Example native mod. It registers `/hello`, which answers with the mod id, and a block and an
//! item whose behaviours log what the server runs.

use std::{any::Any, sync::Arc};

use ironpumpkin_mods::{
    ModInit, NativeMod,
    command::{
        argument_builder::{ArgumentBuilder, command},
        context::command_context::CommandContext,
        node::{CommandExecutor, CommandExecutorResult},
    },
    content::{BlockBehaviour, BlockBuilder, ItemBehaviour, ItemBuilder, RegistryError},
    entity::player::Player,
    math::{position::BlockPos, vector3::Vector3},
    permission::{Permission, PermissionDefault},
    pumpkin_core::block::{
        BrokenArgs, NormalUseArgs, OnStateReplacedArgs, PlacedArgs, registry::BlockActionResult,
    },
    pumpkin_data::{
        Block, BlockDirection, item::Item, item_stack::ItemStack,
        translation::java::CHAT_SQUARE_BRACKETS,
    },
    register_mod,
    server::Server,
    text::{TextComponent, translate_cross},
    tracing::info,
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
        // `ModInit` keeps the first content error: the server logs it with the mod id and stops
        // after `init` returns. Stop at the first error and do not panic.
        let _ = register_content(cx);
    }
}

/// A lamp that vanilla clients see as a redstone lamp, and the item that places it. Content
/// registers at once and returns the error of the registry.
fn register_content(cx: &mut ModInit) -> Result<(), RegistryError> {
    cx.register_block(
        BlockBuilder::new(LAMP, Block::REDSTONE_LAMP.default_state.id)
            .bool_property("lit", false)
            .behaviour(Arc::new(GreeterLamp)),
    )?;
    cx.register_item(
        ItemBuilder::new(LAMP, &Item::REDSTONE_LAMP)
            .places(LAMP)
            .behaviour(Arc::new(GreeterLampItem)),
    )
}

/// Logs the block hooks that the server runs for the lamp.
struct GreeterLamp;

impl BlockBehaviour for GreeterLamp {
    fn placed(&self, args: PlacedArgs<'_>) {
        info!("[{ID}] {} placed at {}", args.block.name, args.position);
    }

    fn broken(&self, args: BrokenArgs<'_>) {
        info!(
            "[{ID}] {} broken by {} at {}",
            args.block.name, args.player.gameprofile.name, args.position
        );
    }

    fn on_state_replaced(&self, args: OnStateReplacedArgs<'_>) {
        info!("[{ID}] {} replaced at {}", args.block.name, args.position);
    }

    fn normal_use(&self, args: NormalUseArgs<'_>) -> BlockActionResult {
        info!(
            "[{ID}] {} used by {} at {}",
            args.block.name, args.player.gameprofile.name, args.position
        );
        BlockActionResult::Pass
    }
}

/// Logs the uses of the lamp item. It keeps placing the lamp: `use_on_block` returns `Pass`.
struct GreeterLampItem;

impl ItemBehaviour for GreeterLampItem {
    fn normal_use(&self, item: &Item, player: &Player) {
        info!(
            "[{ID}] {} used in the air by {}",
            item.registry_key, player.gameprofile.name
        );
    }

    fn use_on_block(
        &self,
        item: &mut ItemStack,
        player: &Player,
        location: BlockPos,
        _face: BlockDirection,
        _cursor_pos: Vector3<f32>,
        block: &Block,
        _server: &Server,
    ) -> BlockActionResult {
        info!(
            "[{ID}] {} used on {} at {location} by {}",
            item.item.registry_key, block.name, player.gameprofile.name
        );
        BlockActionResult::Pass
    }

    fn as_any(&self) -> &dyn Any {
        self
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
