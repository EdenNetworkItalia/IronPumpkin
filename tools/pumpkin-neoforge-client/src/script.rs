//! The scripted action list of the play state.

use std::path::Path;
use std::time::Duration;

use pumpkin_data::item::Item;
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};
use serde::Deserialize;

use crate::Error;

/// The wait after each action when the script does not set `wait_ms`.
const DEFAULT_WAIT: Duration = Duration::from_secs(1);

/// The actions, in order, and the time the client keeps reading after each one.
#[derive(Debug, Clone, PartialEq)]
pub struct Script {
    pub wait: Duration,
    pub steps: Vec<Step>,
}

/// One line of the script.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    /// The line as written in the script.
    pub text: String,
    pub action: Action,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Log in, run the configuration phase and enter play. Always the first step.
    Join,
    /// Send an unsigned chat message.
    Chat(String),
    /// Send a command without the leading slash.
    Command(String),
    /// Move the player by this offset in one position packet.
    Move(Vector3<f64>),
    /// Start to destroy the block. With `ticks`, also stop to destroy it after that many ticks,
    /// as a survival client does; without, the start alone breaks the block in creative mode.
    Break { target: Target, ticks: Option<u32> },
    /// Use the item in the main hand on a face of the block: a block item places a block on that
    /// side.
    Place { target: Target, face: Face },
    /// Put one item in the first hotbar slot and select that slot. Creative mode only.
    Hold(&'static Item),
    /// Use the item in the main hand.
    UseItem,
    /// Ask to respawn after a death.
    Respawn,
    /// Only read, for this long instead of the script's wait.
    Wait(Duration),
    /// Close the connection. Always the last step.
    Leave,
}

impl Action {
    /// The first word of the step, as in the script and the expected file.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Join => "join",
            Self::Chat(_) => "chat",
            Self::Command(_) => "command",
            Self::Move(_) => "move",
            Self::Break { .. } => "break",
            Self::Place { .. } => "place",
            Self::Hold(_) => "hold",
            Self::UseItem => "use_item",
            Self::Respawn => "respawn",
            Self::Wait(_) => "wait",
            Self::Leave => "leave",
        }
    }

    /// Reads one step such as `break ~1 ~-1 ~0` or `chat hello`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        let (name, rest) = text.split_once(' ').unwrap_or((text, ""));
        let rest = rest.trim();
        let args: Vec<&str> = rest.split_whitespace().collect();
        let action = match name {
            "join" => Self::Join,
            "leave" => Self::Leave,
            "use_item" => Self::UseItem,
            "respawn" => Self::Respawn,
            "chat" | "command" if rest.is_empty() => {
                return Err(format!("{name} needs a text"));
            }
            "chat" => Self::Chat(rest.to_owned()),
            "command" => Self::Command(rest.trim_start_matches('/').to_owned()),
            "move" => {
                let [dx, dy, dz] = numbers::<f64, 3>(&args)?;
                Self::Move(Vector3::new(dx, dy, dz))
            }
            "break" => match args.as_slice() {
                [x, y, z] => Self::Break {
                    target: Target::parse(x, y, z)?,
                    ticks: None,
                },
                [x, y, z, ticks] => Self::Break {
                    target: Target::parse(x, y, z)?,
                    ticks: Some(ticks.parse().map_err(|e| format!("ticks {ticks:?}: {e}"))?),
                },
                _ => return Err("break needs x y z and optional ticks".to_owned()),
            },
            "place" => match args.as_slice() {
                [x, y, z, face] => Self::Place {
                    target: Target::parse(x, y, z)?,
                    face: Face::parse(face)?,
                },
                _ => return Err("place needs x y z and a face".to_owned()),
            },
            "hold" => match args.as_slice() {
                [item] => Self::Hold(
                    Item::from_registry_key(item).ok_or_else(|| format!("unknown item {item}"))?,
                ),
                _ => return Err("hold needs one item".to_owned()),
            },
            "wait" => {
                let [ms] = numbers::<u64, 1>(&args)?;
                Self::Wait(Duration::from_millis(ms))
            }
            _ => return Err(format!("unknown action {name:?}")),
        };
        let takes_args = matches!(
            action,
            Self::Chat(_)
                | Self::Command(_)
                | Self::Move(_)
                | Self::Break { .. }
                | Self::Place { .. }
                | Self::Hold(_)
                | Self::Wait(_)
        );
        if !takes_args && !rest.is_empty() {
            return Err(format!("{name} takes no arguments"));
        }
        Ok(action)
    }
}

fn numbers<T: std::str::FromStr, const N: usize>(args: &[&str]) -> Result<[T; N], String>
where
    T::Err: std::fmt::Display,
{
    let values = args
        .iter()
        .map(|arg| arg.parse().map_err(|e| format!("{arg:?}: {e}")))
        .collect::<Result<Vec<T>, String>>()?;
    values
        .try_into()
        .map_err(|_| format!("expected {N} numbers, got {}", args.len()))
}

/// One coordinate: absolute, or relative to the block the player stands in after the join, with
/// a `~` prefix as in vanilla commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coordinate {
    Absolute(i32),
    Relative(i32),
}

impl Coordinate {
    fn parse(text: &str) -> Result<Self, String> {
        let parse = |n: &str| n.parse().map_err(|e| format!("coordinate {text:?}: {e}"));
        match text.strip_prefix('~') {
            Some("") => Ok(Self::Relative(0)),
            Some(offset) => parse(offset).map(Self::Relative),
            None => parse(text).map(Self::Absolute),
        }
    }

    const fn resolve(self, origin: i32) -> i32 {
        match self {
            Self::Absolute(value) => value,
            Self::Relative(offset) => origin + offset,
        }
    }
}

/// A block position of a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub x: Coordinate,
    pub y: Coordinate,
    pub z: Coordinate,
}

impl Target {
    fn parse(x: &str, y: &str, z: &str) -> Result<Self, String> {
        Ok(Self {
            x: Coordinate::parse(x)?,
            y: Coordinate::parse(y)?,
            z: Coordinate::parse(z)?,
        })
    }

    /// The block position, with relative coordinates taken from `origin`.
    #[must_use]
    pub const fn resolve(self, origin: BlockPos) -> BlockPos {
        BlockPos::new(
            self.x.resolve(origin.0.x),
            self.y.resolve(origin.0.y),
            self.z.resolve(origin.0.z),
        )
    }
}

/// A block face, numbered like vanilla `Direction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    Down = 0,
    Up = 1,
    North = 2,
    South = 3,
    West = 4,
    East = 5,
}

impl Face {
    fn parse(text: &str) -> Result<Self, String> {
        Ok(match text {
            "down" => Self::Down,
            "up" => Self::Up,
            "north" => Self::North,
            "south" => Self::South,
            "west" => Self::West,
            "east" => Self::East,
            _ => return Err(format!("unknown face {text:?}")),
        })
    }

    /// The point in the middle of the face, in block coordinates, as the cursor of a click.
    #[must_use]
    pub const fn cursor(self) -> Vector3<f32> {
        match self {
            Self::Down => Vector3::new(0.5, 0.0, 0.5),
            Self::Up => Vector3::new(0.5, 1.0, 0.5),
            Self::North => Vector3::new(0.5, 0.5, 0.0),
            Self::South => Vector3::new(0.5, 0.5, 1.0),
            Self::West => Vector3::new(0.0, 0.5, 0.5),
            Self::East => Vector3::new(1.0, 0.5, 0.5),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScriptFile {
    wait_ms: Option<u64>,
    steps: Vec<String>,
}

impl Script {
    /// Reads a script file: JSON when the name ends in `.json`, else TOML.
    pub fn load(path: &Path) -> Result<Self, Error> {
        let text = std::fs::read_to_string(path)?;
        let json = path.extension().is_some_and(|ext| ext == "json");
        Self::parse(&text, json).map_err(|e| format!("{}: {e}", path.display()).into())
    }

    pub fn parse(text: &str, json: bool) -> Result<Self, Error> {
        let file: ScriptFile = if json {
            serde_json::from_str(text)?
        } else {
            toml::from_str(text)?
        };
        let steps = file
            .steps
            .iter()
            .enumerate()
            .map(|(index, text)| {
                Action::parse(text)
                    .map(|action| Step {
                        text: text.trim().to_owned(),
                        action,
                    })
                    .map_err(|e| format!("step {index} ({text:?}): {e}"))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let last = steps.len().saturating_sub(1);
        for (index, step) in steps.iter().enumerate() {
            match step.action {
                Action::Join if index != 0 => return Err("join must be the first step".into()),
                Action::Leave if index != last => return Err("leave must be the last step".into()),
                _ => {}
            }
        }
        if steps.first().map(|s| &s.action) != Some(&Action::Join)
            || steps.last().map(|s| &s.action) != Some(&Action::Leave)
        {
            return Err("a script starts with join and ends with leave".into());
        }
        Ok(Self {
            wait: file.wait_ms.map_or(DEFAULT_WAIT, Duration::from_millis),
            steps,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_action() {
        let script = Script::parse(
            r#"
wait_ms = 250
steps = [
    "join",
    "chat hello  there",
    "command /say hi",
    "move 0 0.5 -1",
    "break ~1 ~-1 ~",
    "break 10 64 -3 12",
    "hold minecraft:stone",
    "place ~1 ~-2 ~0 up",
    "use_item",
    "respawn",
    "wait 500",
    "leave",
]
"#,
            false,
        )
        .unwrap();
        assert_eq!(script.wait, Duration::from_millis(250));
        let actions: Vec<&Action> = script.steps.iter().map(|s| &s.action).collect();
        let relative = Target {
            x: Coordinate::Relative(1),
            y: Coordinate::Relative(-1),
            z: Coordinate::Relative(0),
        };
        assert_eq!(actions[1], &Action::Chat("hello  there".to_owned()));
        assert_eq!(actions[2], &Action::Command("say hi".to_owned()));
        assert_eq!(actions[3], &Action::Move(Vector3::new(0.0, 0.5, -1.0)));
        assert_eq!(
            actions[4],
            &Action::Break {
                target: relative,
                ticks: None
            }
        );
        assert_eq!(
            relative.resolve(BlockPos::new(5, 70, -2)),
            BlockPos::new(6, 69, -2)
        );
        let absolute = Target {
            x: Coordinate::Absolute(10),
            y: Coordinate::Absolute(64),
            z: Coordinate::Absolute(-3),
        };
        assert_eq!(
            actions[5],
            &Action::Break {
                target: absolute,
                ticks: Some(12)
            }
        );
        assert_eq!(
            absolute.resolve(BlockPos::new(5, 70, -2)),
            BlockPos::new(10, 64, -3)
        );
        assert_eq!(actions[6], &Action::Hold(&Item::STONE));
        assert!(matches!(actions[7], Action::Place { face: Face::Up, .. }));
        assert_eq!(actions[10], &Action::Wait(Duration::from_millis(500)));
        assert_eq!(script.steps[1].text, "chat hello  there");
    }

    #[test]
    fn rejects_bad_scripts() {
        for (steps, error) in [
            (r#"["chat hi", "leave"]"#, "starts with join"),
            (r#"["join", "chat hi"]"#, "ends with leave"),
            (r#"["join", "join", "leave"]"#, "first step"),
            (r#"["join", "leave", "leave"]"#, "last step"),
            (r#"["join", "fly", "leave"]"#, "unknown action"),
            (r#"["join", "chat", "leave"]"#, "needs a text"),
            (r#"["join", "move 1 2", "leave"]"#, "expected 3 numbers"),
            (
                r#"["join", "place ~ ~ ~ sideways", "leave"]"#,
                "unknown face",
            ),
            (
                r#"["join", "hold minecraft:nothing", "leave"]"#,
                "unknown item",
            ),
            (r#"["join", "use_item now", "leave"]"#, "no arguments"),
        ] {
            let text = format!("{{\"steps\": {steps}}}");
            let failure = Script::parse(&text, true).unwrap_err().to_string();
            assert!(failure.contains(error), "{steps}: {failure}");
        }
    }
}
