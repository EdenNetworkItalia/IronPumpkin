use crate::command::argument_builder::{ArgumentBuilder, command};
use crate::command::context::command_context::CommandContext;
use crate::command::node::dispatcher::CommandDispatcher;
use crate::command::node::{CommandExecutor, CommandExecutorResult};
use pumpkin_data::translation;
use pumpkin_util::PermissionLvl;
use pumpkin_util::permission::{Permission, PermissionDefault, PermissionRegistry};
use pumpkin_util::text::click::ClickEvent;
use pumpkin_util::text::hover::HoverEvent;
use pumpkin_util::text::{TextComponent, color::NamedColor};
use std::borrow::Cow;

const DESCRIPTION: &str = "Displays the world seed.";
const PERMISSION: &str = "minecraft:command.seed";

struct SeedCommandExecutor;

fn create_copy_on_click_text(content: String) -> TextComponent {
    TextComponent::translate_cross(
        translation::java::COMMANDS_SEED_SUCCESS,
        translation::bedrock::COMMANDS_SEED_SUCCESS,
        [TextComponent::wrap_in_square_brackets(
            TextComponent::text(content.clone())
                .hover_event(HoverEvent::show_text(TextComponent::translate_cross(
                    translation::java::CHAT_COPY_CLICK,
                    translation::java::CHAT_COPY_CLICK,
                    [],
                )))
                .click_event(ClickEvent::CopyToClipboard {
                    value: Cow::from(content.clone()),
                })
                .insertion(content)
                .color_named(NamedColor::Green),
        )],
    )
}

impl CommandExecutor for SeedCommandExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let seed = context.world().level.seed.0;
        let seed_string = seed.to_string();

        context
            .source
            .send_feedback(create_copy_on_click_text(seed_string), false);

        Ok(seed as i32)
    }
}

pub fn register(dispatcher: &mut CommandDispatcher, registry: &PermissionRegistry) {
    registry.register_permission_or_panic(Permission::new(
        PERMISSION,
        DESCRIPTION,
        // For integrated servers, the permission level is 0,
        // but Pumpkin is always a dedicated server. For dedicated servers,
        // /seed is limited to level 2.
        PermissionDefault::Op(PermissionLvl::Two),
    ));

    dispatcher.register(
        command("seed", DESCRIPTION)
            .requires(PERMISSION)
            .executes(SeedCommandExecutor),
    );
}

#[cfg(test)]
mod tests {
    use super::create_copy_on_click_text;
    use pumpkin_util::version::JavaMinecraftVersion;

    /// Vanilla `ComponentUtils.copyOnClickText` inside `commands.seed.success`.
    #[test]
    fn seed_text_matches_vanilla_copy_on_click_text() {
        let component = create_copy_on_click_text("12 64 -3".to_string());
        let json = component.to_json_value_for_version(&JavaMinecraftVersion::V_26_3);
        assert_eq!(json["translate"], "commands.seed.success");
        let brackets = &json["with"][0];
        assert_eq!(brackets["translate"], "chat.square_brackets");
        let inner = &brackets["with"][0];
        assert_eq!(inner["text"], "12 64 -3");
        assert_eq!(inner["color"], "green");
        assert_eq!(inner["insertion"], "12 64 -3");
        assert_eq!(
            inner["click_event"],
            serde_json::json!({"action": "copy_to_clipboard", "value": "12 64 -3"})
        );
        assert_eq!(inner["hover_event"]["action"], "show_text");
        assert_eq!(
            inner["hover_event"]["value"]["translate"],
            "chat.copy.click"
        );
    }
}
