use pumpkin_data::packet::CURRENT_MC_VERSION;
use pumpkin_util::text::click::ClickEvent;
use pumpkin_util::text::hover::HoverEvent;
use pumpkin_util::text::{TextComponent, color::NamedColor};
use pumpkin_util::translation::get_translation_text;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use pumpkin_util::PermissionLvl;
use pumpkin_util::permission::{Permission, PermissionDefault, PermissionRegistry};

use crate::command::CommandSource;
use crate::command::argument_builder::{ArgumentBuilder, argument, command, literal};
use crate::command::argument_types::client_parsed::MessageArgumentType;
use crate::command::context::command_context::CommandContext;
use crate::command::node::dispatcher::CommandDispatcher;
use crate::command::node::{CommandExecutor, CommandExecutorResult};
use crate::telemetry::{
    HEADER_PUBLIC_KEY, HEADER_SIGNATURE, HEADER_TIMESTAMP, TelemetryClient,
    build_heartbeat_payload, resolve_identity_key, sign_telemetry_payload,
};

#[path = "pumpkin_ironpumpkin.rs"]
mod ironpumpkin;
use ironpumpkin::{
    CONTRIBUTORS_URL, DESCRIPTION, NAME, REPO_URL, WEBSITE_URL, add_credit, add_discord,
    cache_after_fetch,
};

const PERMISSION: &str = "pumpkin:command.pumpkin";
const ARG_SERVER_NAME: &str = "name";
const DEFAULT_ACCOUNTS_URL: &str = "https://accounts.pumpkinmc.org";

const CACHE_DURATION: Duration = Duration::from_hours(24);
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

struct Executor;
struct ContributorsExecutor;
struct LinkExecutor;
struct LinkNamedExecutor;

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

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
struct ServerLinkInitRequest {
    server_name: String,
    server_type: String,
    server_version: String,
    minecraft_version: String,
    os: String,
    arch: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ServerLinkInitResponse {
    code: String,
    link_url: String,
    expires_in_secs: u64,
}

#[derive(Deserialize)]
struct ApiErrorBody {
    error: String,
}

fn resolve_accounts_base_url() -> String {
    std::env::var("PUMPKIN_ACCOUNTS_URL")
        .ok()
        .map(|val| val.trim().trim_end_matches('/').to_string())
        .filter(|val| !val.is_empty())
        .unwrap_or_else(|| DEFAULT_ACCOUNTS_URL.to_string())
}

fn resolve_link_server_name(
    custom_name: Option<&str>,
    configured_name: Option<&str>,
    motd: Option<&str>,
) -> String {
    custom_name
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| configured_name.map(str::trim).filter(|s| !s.is_empty()))
        .or_else(|| motd.map(str::trim).filter(|s| !s.is_empty()))
        .unwrap_or("Pumpkin Server")
        .to_string()
}

fn detect_host_os() -> String {
    sysinfo::System::long_os_version().unwrap_or_else(|_| {
        let name = sysinfo::System::name().unwrap_or_else(|_| std::env::consts::OS.to_string());
        sysinfo::System::os_version().map_or(name.clone(), |ver| format!("{name} {ver}"))
    })
}

fn format_link_success_message(
    resp: &ServerLinkInitResponse,
    telemetry_enabled: bool,
) -> TextComponent {
    let expires_mins = resp.expires_in_secs.div_ceil(60);
    let mut msg = TextComponent::text("\nPumpkin Account Server Link\n")
        .color_named(NamedColor::Gold)
        .bold();

    msg = msg.add_child(TextComponent::text("  Code: ").color_named(NamedColor::Gray));
    msg = msg.add_child(
        TextComponent::text(resp.code.clone())
            .color_named(NamedColor::Aqua)
            .bold()
            .click_event(ClickEvent::CopyToClipboard {
                value: Cow::from(resp.code.clone()),
            })
            .hover_event(HoverEvent::show_text(TextComponent::text(
                "Click to copy link code",
            ))),
    );
    msg = msg.add_child(
        TextComponent::text(format!(" (expires in {expires_mins}m)\n"))
            .color_named(NamedColor::DarkGray),
    );

    msg = msg.add_child(TextComponent::text("  Claim: ").color_named(NamedColor::Gray));
    msg = msg.add_child(
        TextComponent::text(resp.link_url.clone())
            .color_named(NamedColor::Green)
            .bold()
            .underlined()
            .click_event(ClickEvent::OpenUrl {
                url: Cow::from(resp.link_url.clone()),
            })
            .hover_event(HoverEvent::show_text(TextComponent::text(
                "Click to open Pumpkin Account and claim this server",
            ))),
    );

    if !telemetry_enabled {
        msg = msg.add_child(
            TextComponent::text(
                "\n  Note: Telemetry is disabled in pumpkin.toml ([telemetry] enabled = false). Enable it to see live stats and crash reports.",
            )
            .color_named(NamedColor::Yellow),
        );
    }

    msg
}

async fn request_server_link(server_name: String) -> Result<ServerLinkInitResponse, String> {
    let signing_key = resolve_identity_key();
    let payload = ServerLinkInitRequest {
        server_name,
        server_type: "pumpkin".to_string(),
        server_version: CARGO_PKG_VERSION.to_string(),
        minecraft_version: CURRENT_MC_VERSION.to_string(),
        os: detect_host_os(),
        arch: std::env::consts::ARCH.to_string(),
    };

    let body_bytes =
        serde_json::to_vec(&payload).map_err(|e| format!("Failed to serialize payload: {e}"))?;
    let timestamp_str = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();
    let (pubkey_hex, sig_hex) = sign_telemetry_payload(&signing_key, &timestamp_str, &body_bytes);

    let user_agent = format!("Pumpkin-Server/{CARGO_PKG_VERSION}");
    let client = pumpkin_auth::client_builder()
        .user_agent(&user_agent)
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|e| format!("Failed to initialize HTTP client: {e}"))?;

    let init_url = format!("{}/api/user/servers/link/init", resolve_accounts_base_url());
    let response = client
        .post(&init_url)
        .header("Content-Type", "application/json")
        .header(HEADER_PUBLIC_KEY, pubkey_hex)
        .header(HEADER_SIGNATURE, sig_hex)
        .header(HEADER_TIMESTAMP, timestamp_str)
        .body(body_bytes)
        .send()
        .await
        .map_err(|e| format!("Could not reach Pumpkin Account ({init_url}): {e}"))?;

    let status = response.status();
    if !status.is_success() {
        let detail = response
            .json::<ApiErrorBody>()
            .await
            .map_or_else(|_| format!("HTTP {status}"), |b| b.error);
        return Err(detail);
    }

    response
        .json::<ServerLinkInitResponse>()
        .await
        .map_err(|e| format!("Invalid response from Pumpkin Account: {e}"))
}

const fn is_console_only(source: &CommandSource) -> bool {
    source.output.is_console() && !source.silent
}

fn execute_link(context: &CommandContext, custom_name: Option<&str>) -> i32 {
    if !is_console_only(&context.source) {
        context.source.send_message(
            TextComponent::text(
                "For security, `pumpkin link` can only be executed from the server console.",
            )
            .color_named(NamedColor::Red),
        );
        return 0;
    }

    let server_ref = context.source.server.as_ref();
    let configured_name = server_ref.and_then(|s| s.telemetry_config.server_name.as_deref());
    let motd = server_ref.map(|s| s.advanced_config.networking.java.motd.as_str());
    let server_name = resolve_link_server_name(custom_name, configured_name, motd);

    let telemetry_enabled = server_ref.is_some_and(|s| s.telemetry_config.enabled);
    let immediate_heartbeat = server_ref.filter(|s| s.telemetry_config.enabled).map(|s| {
        (
            s.telemetry_config.endpoint.clone(),
            build_heartbeat_payload(s, &s.telemetry_config),
        )
    });

    let rt_handle = server_ref
        .map(|s| s.runtime.clone())
        .or_else(|| tokio::runtime::Handle::try_current().ok());

    let Some(handle) = rt_handle else {
        context.source.send_message(
            TextComponent::text("Unable to start async runtime for /pumpkin link.")
                .color_named(NamedColor::Red),
        );
        return 0;
    };

    context.source.send_message(
        TextComponent::text("Generating Pumpkin Account link code...")
            .color_named(NamedColor::Gray),
    );

    let source = context.source.clone();
    handle.spawn(async move {
        if let Some((endpoint, hb_payload)) = immediate_heartbeat
            && let Ok(http_client) = pumpkin_auth::client_builder()
                .user_agent(format!("Pumpkin-Server/{CARGO_PKG_VERSION}"))
                .timeout(HTTP_TIMEOUT)
                .build()
        {
            let client = TelemetryClient::new(resolve_identity_key(), http_client, endpoint);
            let _ = client.send_heartbeat(&hb_payload).await;
        }

        match request_server_link(server_name).await {
            Ok(resp) => {
                source.send_message(format_link_success_message(&resp, telemetry_enabled));
            }
            Err(err) => {
                source.send_message(
                    TextComponent::text(format!("Failed to generate server link code: {err}"))
                        .color_named(NamedColor::Red),
                );
            }
        }
    });

    1
}

impl CommandExecutor for LinkExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        Ok(execute_link(context, None))
    }
}

impl CommandExecutor for LinkNamedExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let custom_name = context.get_argument::<String>(ARG_SERVER_NAME)?;
        Ok(execute_link(context, Some(custom_name.as_str())))
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
            .then(
                literal("link")
                    .requires(is_console_only)
                    .executes(LinkExecutor)
                    .then(
                        argument(ARG_SERVER_NAME, MessageArgumentType).executes(LinkNamedExecutor),
                    ),
            )
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

    #[test]
    fn link_server_name_precedence() {
        assert_eq!(
            resolve_link_server_name(
                Some("  My Custom SMP "),
                Some("Telemetry Name"),
                Some("MOTD")
            ),
            "My Custom SMP"
        );
        assert_eq!(
            resolve_link_server_name(Some("   "), Some("Telemetry Name"), Some("MOTD")),
            "Telemetry Name"
        );
        assert_eq!(
            resolve_link_server_name(None, None, Some("  A blazingly fast server! ")),
            "A blazingly fast server!"
        );
        assert_eq!(
            resolve_link_server_name(None, Some(""), Some("")),
            "Pumpkin Server"
        );
    }

    #[test]
    fn link_payload_signing_verifies_with_telemetry_verifier() {
        let seed = [42u8; 32];
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed);
        let req = ServerLinkInitRequest {
            server_name: "Survival Node 1".to_string(),
            server_type: "pumpkin".to_string(),
            server_version: CARGO_PKG_VERSION.to_string(),
            minecraft_version: CURRENT_MC_VERSION.to_string(),
            os: "Linux".to_string(),
            arch: "x86_64".to_string(),
        };
        let body = serde_json::to_vec(&req).unwrap();
        let ts = "1700000000";
        let (pub_hex, sig_hex) = sign_telemetry_payload(&signing_key, ts, &body);

        assert!(
            crate::telemetry::verify_telemetry_request(
                &pub_hex,
                &sig_hex,
                ts,
                &body,
                1_700_000_010
            )
            .is_ok()
        );
    }

    #[test]
    fn link_command_requires_non_silent_console() {
        let mut src = CommandSource::dummy();
        assert!(!is_console_only(&src));

        src.output = crate::command::CommandSender::Console;
        assert!(is_console_only(&src));

        src.silent = true;
        assert!(!is_console_only(&src));
    }
}

#[cfg(test)]
#[path = "pumpkin_ironpumpkin_tests.rs"]
mod ironpumpkin_tests;
