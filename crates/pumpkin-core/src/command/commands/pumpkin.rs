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

use crate::command::argument_builder::{ArgumentBuilder, command, literal};
use crate::command::context::command_context::CommandContext;
use crate::command::node::dispatcher::CommandDispatcher;
use crate::command::node::{CommandExecutor, CommandExecutorResult};

#[path = "pumpkin_ironpumpkin.rs"]
mod ironpumpkin;
use ironpumpkin::{
    CONTRIBUTORS_URL, DESCRIPTION, NAME, REPO_URL, WEBSITE_URL, add_credit, add_discord,
    cache_after_fetch,
};

const PERMISSION: &str = "pumpkin:command.pumpkin";

const CACHE_DURATION: Duration = Duration::from_hours(24);
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

struct Executor;
struct ContributorsExecutor;

const CARGO_PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
const GIT_HASH: &str = env!("GIT_HASH");
const GIT_HASH_FULL: &str = env!("GIT_HASH_FULL");

#[derive(Deserialize, Clone)]
struct Contributor {
    login: String,
}

#[derive(Clone)]
struct ContributorCache {
    fetched_at: Instant,
    /// How long the entry is valid after `fetched_at`.
    ttl: Duration,
    data: Vec<Contributor>,
}

static CONTRIBUTORS_CACHE: LazyLock<Mutex<Option<ContributorCache>>> =
    LazyLock::new(|| Mutex::new(None));

#[cfg(test)]
fn fetch_all_contributors_cached() -> Vec<Contributor> {
    if let Ok(guard) = CONTRIBUTORS_CACHE.lock()
        && let Some(cache) = guard.as_ref()
        && cache.fetched_at.elapsed() < cache.ttl
    {
        return cache.data.clone();
    }

    Vec::new()
}

async fn fetch_all_contributors() -> Vec<Contributor> {
    // No fallback to a default client: it has no timeout.
    let Ok(client) = pumpkin_auth::client_builder()
        .user_agent(NAME)
        .timeout(HTTP_TIMEOUT)
        .build()
    else {
        return Vec::new();
    };

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
        let locale = context.source.output.get_locale();
        let profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };
        let version_string = format!("{CARGO_PKG_VERSION} (Commit: {GIT_HASH}/{profile})");
        let mut msg = TextComponent::text("");

        let version_translation = get_translation_text(
            "pumpkin:commands.pumpkin.version",
            locale,
            vec![TextComponent::text(version_string).0],
        );
        msg = msg.add_child(
            TextComponent::text(version_translation.clone())
                .hover_event(HoverEvent::show_text(TextComponent::text(format!(
                    "Commit: {GIT_HASH_FULL}\n\nClick to copy version\nRun /pumpkin contributors to see all contributors"
                ))))
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

        msg = add_credit(msg, locale);

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

        msg = add_discord(msg, locale);

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

        msg = msg.add_child(TextComponent::text("  "));

        msg = msg.add_child(
            TextComponent::text("[Contributors]")
                .click_event(ClickEvent::RunCommand {
                    command: Cow::from("/pumpkin contributors"),
                })
                .hover_event(HoverEvent::show_text(TextComponent::text(
                    "Click to run /pumpkin contributors",
                )))
                .color_named(NamedColor::Yellow)
                .bold()
                .underlined(),
        );

        context.source.send_message(msg);

        Ok(1)
    }
}

fn format_contributors_message(contributors: &[Contributor]) -> TextComponent {
    if contributors.is_empty() {
        return TextComponent::text("Unable to load contributors.").color_named(NamedColor::Red);
    }

    let count = contributors.len();
    let contributor_names = contributors
        .iter()
        .map(|c| c.login.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    TextComponent::text(format!("{NAME} Contributors ({count}):\n"))
        .color_named(NamedColor::Green)
        .bold()
        .add_child(
            TextComponent::text(contributor_names)
                .gradient_named(&[NamedColor::DarkGreen, NamedColor::Green]),
        )
}

impl CommandExecutor for ContributorsExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let cached = CONTRIBUTORS_CACHE.lock().ok().and_then(|g| g.clone());

        if let Some(cache) = &cached
            && cache.fetched_at.elapsed() < cache.ttl
        {
            context
                .source
                .send_message(format_contributors_message(&cache.data));
            return Ok(cache.data.len() as i32);
        }

        if let Some(cache) = &cached {
            context
                .source
                .send_message(format_contributors_message(&cache.data));
        } else {
            context.source.send_message(
                TextComponent::text(format!("Fetching {NAME} contributors..."))
                    .color_named(NamedColor::Gray),
            );
        }

        let rt_handle = context
            .source
            .server
            .as_ref()
            .map(|s| s.runtime.clone())
            .or_else(|| tokio::runtime::Handle::try_current().ok());

        if let Some(handle) = rt_handle {
            let source = context.source.clone();
            let has_stale = cached.is_some();
            handle.spawn(async move {
                let fetched = fetch_all_contributors().await;
                let contributors = match CONTRIBUTORS_CACHE.lock() {
                    Ok(mut guard) => {
                        let cache = cache_after_fetch(guard.take(), fetched);
                        let data = cache.data.clone();
                        *guard = Some(cache);
                        data
                    }
                    Err(_) => fetched,
                };

                if !has_stale {
                    source.send_message(format_contributors_message(&contributors));
                }
            });
        }

        Ok(cached.map_or(0, |c| c.data.len() as i32))
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
            .then(literal("contributors").executes(ContributorsExecutor))
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
            ttl: CACHE_DURATION,
            data: vec![Contributor {
                login: "test_user".to_string(),
            }],
        });
        drop(guard);

        let contributors = fetch_all_contributors_cached();
        assert_eq!(contributors.len(), 1);
        assert_eq!(contributors[0].login, "test_user");
    }
}

#[cfg(test)]
#[path = "pumpkin_ironpumpkin_tests.rs"]
mod ironpumpkin_tests;
