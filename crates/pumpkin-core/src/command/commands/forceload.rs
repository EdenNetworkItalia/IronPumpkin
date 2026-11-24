use crate::command::argument_builder::{ArgumentBuilder, argument, command, literal};
use crate::command::argument_types::coordinates::Coordinates;
use crate::command::argument_types::coordinates::column_pos::ColumnPosArgumentType;
use crate::command::context::command_context::CommandContext;
use crate::command::errors::command_syntax_error::CommandSyntaxError;
use crate::command::errors::error_types::CommandErrorType;
use crate::command::node::dispatcher::CommandDispatcher;
use crate::command::node::{CommandExecutor, CommandExecutorResult};
use pumpkin_data::translation;
use pumpkin_util::PermissionLvl;
use pumpkin_util::math::vector2::Vector2;
use pumpkin_util::permission::{Permission, PermissionDefault, PermissionRegistry};
use pumpkin_util::text::TextComponent;

const DESCRIPTION: &str = "Constantly load chunks in the world.";
const PERMISSION: &str = "minecraft:command.forceload";

static ERROR_FAILED_ADD: CommandErrorType<0> = CommandErrorType::new(
    translation::java::COMMANDS_FORCELOAD_ADDED_FAILURE,
    "No world in source",
);
static ERROR_FAILED_REMOVE: CommandErrorType<0> = CommandErrorType::new(
    translation::java::COMMANDS_FORCELOAD_REMOVED_FAILURE,
    "No world in source",
);
static ERROR_FAILED_QUERY: CommandErrorType<2> = CommandErrorType::new(
    translation::java::COMMANDS_FORCELOAD_QUERY_FAILURE,
    "Chunk is not marked for force loading",
);
static ERROR_NO_WORLD: CommandErrorType<0> = CommandErrorType::new(
    translation::java::COMMANDS_FORCELOAD_QUERY_FAILURE,
    "No world in source",
);
static ERROR_TOO_MANY: CommandErrorType<2> = CommandErrorType::new(
    translation::java::COMMANDS_FORCELOAD_TOOBIG,
    "Too many chunks",
);

/// Vanilla `ForceLoadCommand.changeForceLoad`: block coordinates in `[-30000000, 30000000)`.
const WORLD_LIMIT: i32 = 30_000_000;
const MAX_CHUNKS: i64 = 256;

static ERROR_OUT_OF_WORLD: CommandErrorType<0> = CommandErrorType::new(
    translation::java::ARGUMENT_POS_OUTOFWORLD,
    translation::java::ARGUMENT_POS_OUTOFWORLD,
);

/// The chunk area between two column positions, as its minimum and maximum chunk.
fn chunk_area(
    from: Vector2<i32>,
    to: Vector2<i32>,
) -> Result<(Vector2<i32>, Vector2<i32>), CommandSyntaxError> {
    let (min_x, max_x) = (from.x.min(to.x), from.x.max(to.x));
    let (min_z, max_z) = (from.y.min(to.y), from.y.max(to.y));
    if min_x < -WORLD_LIMIT || min_z < -WORLD_LIMIT || max_x >= WORLD_LIMIT || max_z >= WORLD_LIMIT
    {
        return Err(ERROR_OUT_OF_WORLD.create_without_context());
    }
    let min = Vector2::new(min_x >> 4, min_z >> 4);
    let max = Vector2::new(max_x >> 4, max_z >> 4);
    let total = i64::from(max.x - min.x + 1) * i64::from(max.y - min.y + 1);
    if total > MAX_CHUNKS {
        return Err(ERROR_TOO_MANY.create_without_context(
            TextComponent::text(MAX_CHUNKS.to_string()),
            TextComponent::text(total.to_string()),
        ));
    }
    Ok((min, max))
}

fn chunk_text(pos: Vector2<i32>) -> TextComponent {
    TextComponent::text(chunk_string(pos))
}

/// Vanilla `ChunkPos.toString`.
fn chunk_string(pos: Vector2<i32>) -> String {
    format!("[{}, {}]", pos.x, pos.y)
}

/// The dimension as vanilla `translationArg(dimension.identifier())` names it.
fn dimension_identifier(dimension: &'static str) -> TextComponent {
    TextComponent::text(dimension)
}

/// `NeoForge` `CommonHooks.getDimensionDescription` and `Level.getDescription`: the
/// `dimension.<namespace>.<path>` translation, with the identifier as the fallback.
fn dimension_description(dimension: &'static str) -> TextComponent {
    let (namespace, path) = dimension
        .split_once(':')
        .unwrap_or(("minecraft", dimension));
    TextComponent::translate_with_fallback(format!("dimension.{namespace}.{path}"), dimension, [])
}

/// Vanilla `ChunkPos.pack`: the order of `/forceload query` lists.
fn packed(pos: Vector2<i32>) -> i64 {
    (i64::from(pos.x) & 0xFFFF_FFFF) | (i64::from(pos.y) << 32)
}

/// Vanilla `ForceLoadCommand.listForceLoad`: the reply, or `None` for `added.none`.
fn list_feedback(forced: &[Vector2<i32>], dimension: &'static str) -> Option<TextComponent> {
    let mut sorted = forced.to_vec();
    sorted.sort_unstable_by_key(|pos| packed(*pos));
    let list = TextComponent::text(
        sorted
            .into_iter()
            .map(chunk_string)
            .collect::<Vec<_>>()
            .join(", "),
    );
    let description = dimension_description(dimension);
    match forced.len() {
        0 => None,
        1 => Some(TextComponent::translate_cross(
            translation::java::COMMANDS_FORCELOAD_LIST_SINGLE,
            translation::java::COMMANDS_FORCELOAD_LIST_SINGLE,
            [description, list],
        )),
        count => Some(TextComponent::translate_cross(
            translation::java::COMMANDS_FORCELOAD_LIST_MULTIPLE,
            translation::java::COMMANDS_FORCELOAD_LIST_MULTIPLE,
            [TextComponent::text(count.to_string()), description, list],
        )),
    }
}

/// The reply that vanilla sends for the chunks that `add` or `remove` changed. `NeoForge` names
/// the dimension by its description in the `multiple` replies only.
fn change_feedback(
    add: bool,
    changed: &[Vector2<i32>],
    dimension: &'static str,
    min: Vector2<i32>,
    max: Vector2<i32>,
) -> Result<TextComponent, CommandSyntaxError> {
    match changed {
        [] => Err(if add {
            ERROR_FAILED_ADD.create_without_context()
        } else {
            ERROR_FAILED_REMOVE.create_without_context()
        }),
        [only] => {
            let key = if add {
                translation::java::COMMANDS_FORCELOAD_ADDED_SINGLE
            } else {
                translation::java::COMMANDS_FORCELOAD_REMOVED_SINGLE
            };
            Ok(TextComponent::translate_cross(
                key,
                key,
                [chunk_text(*only), dimension_identifier(dimension)],
            ))
        }
        _ => {
            let key = if add {
                translation::java::COMMANDS_FORCELOAD_ADDED_MULTIPLE
            } else {
                translation::java::COMMANDS_FORCELOAD_REMOVED_MULTIPLE
            };
            Ok(TextComponent::translate_cross(
                key,
                key,
                [
                    TextComponent::text(changed.len().to_string()),
                    dimension_description(dimension),
                    chunk_text(min),
                    chunk_text(max),
                ],
            ))
        }
    }
}

fn change_forceload(context: &CommandContext, add: bool) -> CommandExecutorResult {
    let from_pos = ColumnPosArgumentType::get_column_pos(context, "from")?;
    let to_pos = if context.get_argument::<Coordinates>("to").is_ok() {
        ColumnPosArgumentType::get_column_pos(context, "to")?
    } else {
        from_pos
    };
    let world = context.source.world.as_ref().ok_or_else(|| {
        if add {
            ERROR_FAILED_ADD.create_without_context()
        } else {
            ERROR_FAILED_REMOVE.create_without_context()
        }
    })?;

    let (min, max) = chunk_area(from_pos.0, to_pos.0)?;
    let mut changed = Vec::new();
    for x in min.x..=max.x {
        for z in min.y..=max.y {
            let pos = Vector2::new(x, z);
            if world.set_chunk_forced(pos, add) {
                changed.push(pos);
            }
        }
    }
    if !changed.is_empty() {
        world.update_active_chunks();
    }

    let text = change_feedback(add, &changed, world.dimension.minecraft_name, min, max)?;
    context.source.send_feedback(text, true);
    Ok(changed.len() as i32)
}

struct ForceloadAddExecutor;

impl CommandExecutor for ForceloadAddExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        change_forceload(context, true)
    }
}

struct ForceloadRemoveExecutor;

impl CommandExecutor for ForceloadRemoveExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        change_forceload(context, false)
    }
}

struct ForceloadRemoveAllExecutor;

impl CommandExecutor for ForceloadRemoveAllExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let world = context
            .source
            .world
            .as_ref()
            .ok_or_else(|| ERROR_FAILED_REMOVE.create_without_context())?;

        let forced: Vec<_> = world
            .forced_chunks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .copied()
            .collect();
        for pos in forced {
            world.set_chunk_forced(pos, false);
        }

        world.update_active_chunks();

        let text = TextComponent::translate_cross(
            translation::java::COMMANDS_FORCELOAD_REMOVED_ALL,
            translation::java::COMMANDS_FORCELOAD_REMOVED_ALL,
            [dimension_description(world.dimension.minecraft_name)],
        );
        context.source.send_feedback(text, true);

        Ok(0)
    }
}

/// Vanilla `ForceLoadCommand.queryForceLoad`.
struct ForceloadQueryExecutor;

impl CommandExecutor for ForceloadQueryExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let world = context
            .source
            .world
            .as_ref()
            .ok_or_else(|| ERROR_NO_WORLD.create_without_context())?;
        let pos = ColumnPosArgumentType::get_column_pos(context, "pos")?;
        let chunk = Vector2::new(pos.0.x >> 4, pos.0.y >> 4);
        let forced = world
            .forced_chunks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(&chunk);
        let dimension = dimension_description(world.dimension.minecraft_name);
        if !forced {
            return Err(ERROR_FAILED_QUERY.create_without_context(chunk_text(chunk), dimension));
        }
        context.source.send_feedback(
            TextComponent::translate_cross(
                translation::java::COMMANDS_FORCELOAD_QUERY_SUCCESS,
                translation::java::COMMANDS_FORCELOAD_QUERY_SUCCESS,
                [chunk_text(chunk), dimension],
            ),
            false,
        );
        Ok(1)
    }
}

/// Vanilla `ForceLoadCommand.listForceLoad`: returns the count of forced chunks.
struct ForceloadListExecutor;

impl CommandExecutor for ForceloadListExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let world = context
            .source
            .world
            .as_ref()
            .ok_or_else(|| ERROR_NO_WORLD.create_without_context())?;
        let forced: Vec<_> = world
            .forced_chunks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .copied()
            .collect();
        let dimension = world.dimension.minecraft_name;
        if let Some(text) = list_feedback(&forced, dimension) {
            context.source.send_feedback(text, false);
        } else {
            context.source.send_error(TextComponent::translate_cross(
                translation::java::COMMANDS_FORCELOAD_ADDED_NONE,
                translation::java::COMMANDS_FORCELOAD_ADDED_NONE,
                [dimension_description(dimension)],
            ));
        }
        Ok(forced.len() as i32)
    }
}

pub fn register(dispatcher: &mut CommandDispatcher, registry: &PermissionRegistry) {
    registry.register_permission_or_panic(Permission::new(
        PERMISSION,
        DESCRIPTION,
        PermissionDefault::Op(PermissionLvl::Two),
    ));

    let builder = command("forceload", DESCRIPTION)
        .requires(PERMISSION)
        .then(
            literal("add").then(
                argument("from", ColumnPosArgumentType)
                    .executes(ForceloadAddExecutor)
                    .then(argument("to", ColumnPosArgumentType).executes(ForceloadAddExecutor)),
            ),
        )
        .then(
            literal("remove")
                .then(literal("all").executes(ForceloadRemoveAllExecutor))
                .then(
                    argument("from", ColumnPosArgumentType)
                        .executes(ForceloadRemoveExecutor)
                        .then(
                            argument("to", ColumnPosArgumentType).executes(ForceloadRemoveExecutor),
                        ),
                ),
        )
        .then(
            literal("query")
                .executes(ForceloadListExecutor)
                .then(argument("pos", ColumnPosArgumentType).executes(ForceloadQueryExecutor)),
        );

    dispatcher.register(builder);
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_util::text::TextContent;

    fn at(x: i32, z: i32) -> Vector2<i32> {
        Vector2::new(x, z)
    }

    fn error_key(error: &CommandSyntaxError) -> String {
        format!("{:?}", error.error_type)
    }

    fn translation_of(text: &TextComponent) -> (&str, Vec<String>) {
        let TextContent::Translate {
            translate, with, ..
        } = &*text.0.content
        else {
            panic!("not a translation");
        };
        let args = with
            .iter()
            .map(|arg| match &*arg.content {
                TextContent::Text { text } => text.to_string(),
                _ => panic!("not a text argument"),
            })
            .collect();
        (translate, args)
    }

    #[test]
    fn an_area_of_more_than_256_chunks_counts_in_64_bits() {
        let error = chunk_area(at(0, 0), at(1_048_575, 1_048_575)).unwrap_err();
        assert!(error_key(&error).contains("commands.forceload.toobig"));
        let (_, args) = translation_of(&error.message);
        assert_eq!(args, ["256", "4294967296"]);
    }

    #[test]
    fn the_largest_area_in_the_world_is_too_big_and_not_out_of_the_world() {
        let error = chunk_area(at(-WORLD_LIMIT, -WORLD_LIMIT), at(WORLD_LIMIT - 1, 0)).unwrap_err();
        assert!(error_key(&error).contains("commands.forceload.toobig"));
    }

    #[test]
    fn coordinates_outside_the_world_fail_with_the_out_of_world_error() {
        for (from, to) in [
            (at(WORLD_LIMIT, 0), at(WORLD_LIMIT, 0)),
            (at(0, WORLD_LIMIT), at(0, 0)),
            (at(-WORLD_LIMIT - 1, 0), at(0, 0)),
            (at(0, 0), at(0, -WORLD_LIMIT - 1)),
        ] {
            let error = chunk_area(from, to).unwrap_err();
            assert!(
                error_key(&error).contains("argument.pos.outofworld"),
                "{from:?} {to:?}"
            );
        }
    }

    #[test]
    fn the_edges_of_the_world_are_inside_it() {
        assert_eq!(
            chunk_area(at(WORLD_LIMIT - 1, 0), at(WORLD_LIMIT - 1, 0)).unwrap(),
            (at(1_874_999, 0), at(1_874_999, 0))
        );
        assert_eq!(
            chunk_area(
                at(-WORLD_LIMIT, -WORLD_LIMIT),
                at(-WORLD_LIMIT, -WORLD_LIMIT)
            )
            .unwrap(),
            (at(-1_875_000, -1_875_000), at(-1_875_000, -1_875_000))
        );
    }

    #[test]
    fn the_area_between_two_corners_is_ordered_in_chunks() {
        assert_eq!(
            chunk_area(at(31, -1), at(-16, 16)).unwrap(),
            (at(-1, -1), at(1, 1))
        );
    }

    #[test]
    fn one_changed_chunk_is_named_in_the_single_reply() {
        let text = change_feedback(
            true,
            &[at(3, -4)],
            "minecraft:overworld",
            at(0, 0),
            at(5, 5),
        )
        .unwrap();
        let (key, args) = translation_of(&text);
        assert_eq!(key, "commands.forceload.added.single");
        assert_eq!(args, ["[3, -4]", "minecraft:overworld"]);
    }

    #[test]
    fn several_changed_chunks_count_only_the_changed_ones() {
        let changed = [at(0, 0), at(1, 0), at(0, 1)];
        let text =
            change_feedback(false, &changed, "minecraft:the_end", at(0, 0), at(1, 1)).unwrap();
        let TextContent::Translate {
            translate, with, ..
        } = &*text.0.content
        else {
            panic!("not a translation");
        };
        assert_eq!(translate, "commands.forceload.removed.multiple");
        assert_eq!(with.len(), 4);
        assert_eq!(
            TextComponent(with[1].clone()),
            dimension_description("minecraft:the_end")
        );
    }

    #[test]
    fn the_dimension_description_is_the_neoforge_translation_with_the_identifier_fallback() {
        let TextContent::Translate {
            translate,
            fallback,
            with,
            ..
        } = &*dimension_description("minecraft:the_nether").0.content
        else {
            panic!("not a translation");
        };
        assert_eq!(translate, "dimension.minecraft.the_nether");
        assert_eq!(fallback.as_deref(), Some("minecraft:the_nether"));
        assert!(with.is_empty());
    }

    fn list_of(text: &TextComponent) -> (&str, usize, String) {
        let TextContent::Translate {
            translate, with, ..
        } = &*text.0.content
        else {
            panic!("not a translation");
        };
        let TextContent::Text { text: list } = &*with.last().unwrap().content else {
            panic!("not a text list");
        };
        (translate, with.len(), list.to_string())
    }

    /// Vanilla sorts the list by `ChunkPos.pack`: by z, then by x as an unsigned 32-bit value.
    #[test]
    fn the_list_is_in_packed_chunk_order() {
        let forced = [at(1, 0), at(-1, 0), at(0, -1), at(5, 2), at(0, 0)];
        let text = list_feedback(&forced, "minecraft:overworld").unwrap();
        let (key, args, list) = list_of(&text);
        assert_eq!(key, "commands.forceload.list.multiple");
        assert_eq!(args, 3);
        assert_eq!(list, "[0, -1], [0, 0], [1, 0], [-1, 0], [5, 2]");
    }

    #[test]
    fn one_forced_chunk_is_the_single_list_and_none_is_no_reply() {
        let text = list_feedback(&[at(-3, 7)], "minecraft:overworld").unwrap();
        let (key, args, list) = list_of(&text);
        assert_eq!(key, "commands.forceload.list.single");
        assert_eq!(args, 2);
        assert_eq!(list, "[-3, 7]");
        assert!(list_feedback(&[], "minecraft:overworld").is_none());
    }

    #[test]
    fn no_changed_chunk_is_a_failure() {
        let add = change_feedback(true, &[], "minecraft:overworld", at(0, 0), at(0, 0));
        assert!(error_key(&add.unwrap_err()).contains("commands.forceload.added.failure"));
        let remove = change_feedback(false, &[], "minecraft:overworld", at(0, 0), at(0, 0));
        assert!(error_key(&remove.unwrap_err()).contains("commands.forceload.removed.failure"));
    }
}
