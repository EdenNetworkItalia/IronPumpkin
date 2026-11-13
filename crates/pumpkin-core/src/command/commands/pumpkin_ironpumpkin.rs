//! The `IronPumpkin` parts of the `/pumpkin` command: branding, the credit and Discord buttons, and
//! the contributor cache that keeps an empty result for a short time.

use super::{
    CACHE_DURATION, ClickEvent, Contributor, ContributorCache, Cow, Duration, HoverEvent, Instant,
    NamedColor, TextComponent, get_translation_text,
};
use pumpkin_util::translation::Locale;

pub(super) use pumpkin_util::branding::{CONTRIBUTORS_URL, NAME, REPO_URL, WEBSITE_URL};
use pumpkin_util::branding::{DISCORD_URL, UPSTREAM_URL};

pub(super) const DESCRIPTION: &str = "Display information about IronPumpkin.";

/// How long an empty result stays cached, so a server without network does not retry at each call.
pub(super) const EMPTY_CACHE_DURATION: Duration = Duration::from_mins(5);

/// The cache entry after a fetch. An empty result means that the fetch failed or found nobody:
/// the entry keeps the previous list and expires after `EMPTY_CACHE_DURATION`.
pub(super) fn cache_after_fetch(
    previous: Option<ContributorCache>,
    fetched: Vec<Contributor>,
) -> ContributorCache {
    let (data, ttl) = if fetched.is_empty() {
        (
            previous.map_or_else(Vec::new, |cache| cache.data),
            EMPTY_CACHE_DURATION,
        )
    } else {
        (fetched, CACHE_DURATION)
    };
    ContributorCache {
        fetched_at: Instant::now(),
        ttl,
        data,
    }
}

/// Adds the line that credits Pumpkin.
pub(super) fn add_credit(msg: TextComponent, locale: Locale) -> TextComponent {
    let credit_translation =
        get_translation_text("pumpkin:commands.pumpkin.credit", locale, vec![]);
    let credit_hover_translation =
        get_translation_text("pumpkin:commands.pumpkin.credit.hover", locale, vec![]);
    msg.add_child(
        TextComponent::text(credit_translation)
            .click_event(ClickEvent::OpenUrl {
                url: Cow::from(UPSTREAM_URL),
            })
            .hover_event(HoverEvent::show_text(TextComponent::text(
                credit_hover_translation,
            )))
            .color_named(NamedColor::Yellow),
    )
}

/// Adds the Discord button and the spacing after it.
pub(super) fn add_discord(msg: TextComponent, locale: Locale) -> TextComponent {
    let discord_translation =
        get_translation_text("pumpkin:commands.pumpkin.discord", locale, vec![]);
    let discord_hover_translation =
        get_translation_text("pumpkin:commands.pumpkin.discord.hover", locale, vec![]);
    msg.add_child(
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
    )
    .add_child(TextComponent::text("  "))
}
