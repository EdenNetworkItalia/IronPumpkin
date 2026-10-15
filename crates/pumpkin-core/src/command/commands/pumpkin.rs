use pumpkin_data::packet::CURRENT_MC_VERSION;
use pumpkin_util::text::click::ClickEvent;
use pumpkin_util::text::hover::HoverEvent;
use pumpkin_util::text::{TextComponent, color::NamedColor};
use pumpkin_util::translation::get_translation_text;
use serde::Deserialize;
use std::borrow::Cow;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use pumpkin_util::PermissionLvl;
use pumpkin_util::permission::{Permission, PermissionDefault, PermissionRegistry};

use crate::command::argument_builder::{ArgumentBuilder, command};
use crate::command::context::command_context::CommandContext;
use crate::command::node::dispatcher::CommandDispatcher;
use crate::command::node::{CommandExecutor, CommandExecutorResult};

const DESCRIPTION: &str = "Display information about IronPumpkin.";
const PERMISSION: &str = "pumpkin:command.pumpkin";

// IronPumpkin branding. `main.rs` keeps its own copy of the links.
const REPO_URL: &str = "https://github.com/EdenNetworkItalia/IronPumpkin";
const CONTRIBUTORS_URL: &str =
    "https://api.github.com/repos/EdenNetworkItalia/IronPumpkin/contributors?per_page=100";
const UPSTREAM_URL: &str = "https://github.com/Pumpkin-MC/Pumpkin";
const WEBSITE_URL: &str = "https://eden.network";
const DISCORD_URL: &str = "https://discord.eden.network";

const CACHE_DURATION: Duration = Duration::from_hours(24);

struct Executor;

const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
const GIT_HASH: &str = env!("GIT_HASH");
const GIT_HASH_FULL: &str = env!("GIT_HASH_FULL");

#[derive(Deserialize, Clone)]
struct Contributor {
    login: String,
}

struct ContributorCache {
    fetched_at: Instant,
    data: Vec<Contributor>,
}

static CONTRIBUTORS_CACHE: LazyLock<Mutex<Option<ContributorCache>>> =
    LazyLock::new(|| Mutex::new(None));

fn fetch_all_contributors_cached() -> Vec<Contributor> {
    if let Ok(guard) = CONTRIBUTORS_CACHE.lock()
        && let Some(cache) = guard.as_ref()
        && cache.fetched_at.elapsed() < CACHE_DURATION
    {
        return cache.data.clone();
    }

    let contributors = tokio::runtime::Handle::try_current().map_or_else(
        |_| {
            tokio::runtime::Runtime::new()
                .map_or_else(|_| Vec::new(), |rt| rt.block_on(fetch_all_contributors()))
        },
        |handle| tokio::task::block_in_place(|| handle.block_on(fetch_all_contributors())),
    );

    if !contributors.is_empty() {
        if let Ok(mut guard) = CONTRIBUTORS_CACHE.lock() {
            *guard = Some(ContributorCache {
                fetched_at: Instant::now(),
                data: contributors.clone(),
            });
        }
    } else if let Ok(guard) = CONTRIBUTORS_CACHE.lock()
        && let Some(cache) = guard.as_ref()
    {
        return cache.data.clone();
    }

    contributors
}

async fn fetch_all_contributors() -> Vec<Contributor> {
    let client = pumpkin_auth::client_builder()
        .user_agent("IronPumpkin")
        .build()
        .unwrap_or_default();

    let mut all_contributors = Vec::new();
    let mut next_url = Some(CONTRIBUTORS_URL.to_string());

    while let Some(url) = next_url {
        let response = client.get(&url).send().await;

        match response {
            Ok(res) => {
                let link_header = res
                    .headers()
                    .get("link")
                    .and_then(|s| s.to_str().ok())
                    .map(String::from);

                if let Ok(contributors) = res.json::<Vec<Contributor>>().await {
                    all_contributors.extend(contributors);
                } else {
                    break;
                }

                next_url = link_header.as_deref().and_then(extract_next_url);
            }
            Err(_) => break,
        }
    }

    if all_contributors.is_empty() {
        return vec![];
    }

    all_contributors
}

fn extract_next_url(header: &str) -> Option<String> {
    header
        .split(',')
        .find(|part| part.contains("rel=\"next\""))
        .and_then(|part| {
            let start = part.find('<')? + 1;
            let end = part.find('>')?;
            Some(part[start..end].to_string())
        })
}

#[expect(clippy::too_many_lines)]
impl CommandExecutor for Executor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let contributors = fetch_all_contributors_cached();
        let contributor_names = contributors
            .iter()
            .map(|c| c.login.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let locale = context.source.output.get_locale();
        let profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };
        let version_string = format!(
            "{} (Commit: {}/{}) - {} Contributors",
            CARGO_PKG_VERSION,
            GIT_HASH,
            profile,
            contributors.len()
        );
        let mut msg = TextComponent::text("");

        let version_translation = get_translation_text(
            "pumpkin:commands.pumpkin.version",
            locale,
            vec![TextComponent::text(version_string).0],
        );
        msg = msg.add_child(
            TextComponent::text(version_translation.clone())
                .hover_event(HoverEvent::show_text(
                    TextComponent::text(format!("Commit: {GIT_HASH_FULL}\n\nContributors:\n"))
                        .add_child(
                            TextComponent::text(contributor_names)
                                .gradient_named(&[NamedColor::DarkGreen, NamedColor::Green])
                                .new_line(),
                        ),
                ))
                .click_event(ClickEvent::CopyToClipboard {
                    value: Cow::from(version_translation.replace('\n', "")),
                })
                .color_named(NamedColor::Green),
        );

        let desc_translation =
            get_translation_text("pumpkin:commands.pumpkin.description", locale, vec![]);
        let desc_hover_translation =
            get_translation_text("pumpkin:commands.pumpkin.description.hover", locale, vec![]);
        msg = msg.add_child(
            TextComponent::text(desc_translation.clone())
                .click_event(ClickEvent::CopyToClipboard {
                    value: Cow::from(desc_translation.replace('\n', "")),
                })
                .hover_event(HoverEvent::show_text(TextComponent::text(
                    desc_hover_translation,
                )))
                .color_named(NamedColor::White),
        );

        let mc_version_translation = get_translation_text(
            "pumpkin:commands.pumpkin.minecraft_version",
            locale,
            vec![
                TextComponent::text(CURRENT_MC_VERSION.to_string()).0,
                TextComponent::text(CURRENT_MC_VERSION.protocol_version().to_string()).0,
            ],
        );
        let mc_version_hover_translation = get_translation_text(
            "pumpkin:commands.pumpkin.minecraft_version.hover",
            locale,
            vec![],
        );
        msg = msg.add_child(
            TextComponent::text(mc_version_translation.clone())
                .click_event(ClickEvent::CopyToClipboard {
                    value: Cow::from(mc_version_translation.replace('\n', "")),
                })
                .hover_event(HoverEvent::show_text(TextComponent::text(
                    mc_version_hover_translation,
                )))
                .color_named(NamedColor::Gold),
        );

        let credit_translation =
            get_translation_text("pumpkin:commands.pumpkin.credit", locale, vec![]);
        let credit_hover_translation =
            get_translation_text("pumpkin:commands.pumpkin.credit.hover", locale, vec![]);
        msg = msg.add_child(
            TextComponent::text(credit_translation)
                .click_event(ClickEvent::OpenUrl {
                    url: Cow::from(UPSTREAM_URL),
                })
                .hover_event(HoverEvent::show_text(TextComponent::text(
                    credit_hover_translation,
                )))
                .color_named(NamedColor::Yellow),
        );

        let github_translation =
            get_translation_text("pumpkin:commands.pumpkin.github", locale, vec![]);
        let github_hover_translation =
            get_translation_text("pumpkin:commands.pumpkin.github.hover", locale, vec![]);
        msg = msg.add_child(
            TextComponent::text(github_translation)
                .click_event(ClickEvent::OpenUrl {
                    url: Cow::from(REPO_URL),
                })
                .hover_event(HoverEvent::show_text(TextComponent::text(
                    github_hover_translation,
                )))
                .color_named(NamedColor::Blue)
                .bold()
                .underlined(),
        );

        msg = msg.add_child(TextComponent::text("  "));

        let discord_translation =
            get_translation_text("pumpkin:commands.pumpkin.discord", locale, vec![]);
        let discord_hover_translation =
            get_translation_text("pumpkin:commands.pumpkin.discord.hover", locale, vec![]);
        msg = msg.add_child(
            TextComponent::text(discord_translation)
                .click_event(ClickEvent::OpenUrl {
                    url: Cow::from(DISCORD_URL),
                })
                .hover_event(HoverEvent::show_text(TextComponent::text(
                    discord_hover_translation,
                )))
                .color_named(NamedColor::Blue)
                .bold()
                .underlined(),
        );

        msg = msg.add_child(TextComponent::text("  "));

        let website_translation =
            get_translation_text("pumpkin:commands.pumpkin.website", locale, vec![]);
        let website_hover_translation =
            get_translation_text("pumpkin:commands.pumpkin.website.hover", locale, vec![]);
        msg = msg.add_child(
            TextComponent::text(website_translation)
                .click_event(ClickEvent::OpenUrl {
                    url: Cow::from(WEBSITE_URL),
                })
                .hover_event(HoverEvent::show_text(TextComponent::text(
                    website_hover_translation,
                )))
                .color_named(NamedColor::Blue)
                .bold()
                .underlined(),
        );

        context.source.send_message(msg);

        // It makes total sense to return the number of
        // contributors as the i32 result for this command.
        Ok(contributors.len() as i32)
    }
}

pub fn register(dispatcher: &mut CommandDispatcher, registry: &PermissionRegistry) {
    registry.register_permission_or_panic(Permission::new(
        PERMISSION,
        DESCRIPTION,
        PermissionDefault::Op(PermissionLvl::Zero),
    ));

    dispatcher.register_with_aliases(
        command("pumpkin", DESCRIPTION)
            .requires(PERMISSION)
            .executes(Executor),
        &["version"],
    );
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn cache_duration_is_24_hours() {
        assert_eq!(CACHE_DURATION, Duration::from_hours(24));
    }

    #[test]
    fn contributor_cache_updates_and_retrieves() {
        let mut guard = CONTRIBUTORS_CACHE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = Some(ContributorCache {
            fetched_at: Instant::now(),
            data: vec![Contributor {
                login: "test_user".to_string(),
            }],
        });
        drop(guard);

        let contributors = fetch_all_contributors_cached();
        assert_eq!(contributors.len(), 1);
        assert_eq!(contributors[0].login, "test_user");
    }

    #[test]
    fn version_translation_names_ironpumpkin_in_every_language() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/translations");
        let mut checked = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let json = std::fs::read_to_string(&path).unwrap();
            let map: std::collections::HashMap<String, String> =
                serde_json::from_str(&json).unwrap();
            if let Some(version) = map.get("commands.pumpkin.version") {
                assert!(
                    version.starts_with("IronPumpkin "),
                    "{} has {version:?}",
                    path.display()
                );
                checked += 1;
            }
        }
        assert!(checked > 0);
    }
}
