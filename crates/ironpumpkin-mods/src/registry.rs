//! Lookups in the block, item and entity type registries and their tags.
//!
//! A `NeoForge` mod reads a registry through `Registry` (for example `BuiltInRegistries.BLOCK`)
//! and a tag through `TagKey` and `HolderSet`. Here the registry is a [`ContentKind`] and an entry
//! or a tag is its name, `namespace:path`. A name without a namespace uses `minecraft`, as
//! `Identifier.parse` does. A tag name may start with `#`.
//!
//! The lookups answer for vanilla and mod entries alike, and for the generated tags (vanilla and
//! the `c:` and `neoforge:` tags of `NeoForge` `Tags`) and the mod tags. Mod entries and mod tags
//! exist after the freeze of the registry, which runs after the lifecycle steps and before the
//! first world loads. Before the freeze, only the vanilla entries and the generated tags answer.
//!
//! A placeholder, which stands in for the content of a mod that the world has and the server
//! lacks, is not an entry: `NeoForge` drops the entries of a missing mod.
//!
//! ```
//! use ironpumpkin_mods::{content::ContentKind, registry};
//!
//! assert!(registry::contains(ContentKind::Item, "minecraft:iron_ingot"));
//! assert!(registry::is_in_tag(ContentKind::Item, "minecraft:iron_ingot", "c:ingots"));
//! let id = registry::id_of(ContentKind::Item, "minecraft:iron_ingot").unwrap();
//! assert_eq!(
//!     registry::key_of(ContentKind::Item, id).unwrap().to_string(),
//!     "minecraft:iron_ingot"
//! );
//! assert_eq!(registry::id_of(ContentKind::Item, "examplemod:unknown"), None);
//! ```

use pumpkin_data::{
    Block, BlockId,
    dynamic::{self, ContentKind, DynamicTaggable},
    entity::EntityType,
    item::Item,
    tag::RegistryKey,
};
use pumpkin_util::identifier::Identifier;

/// Whether the registry has an entry with this name.
///
/// `NeoForge`: `Registry#containsKey`.
#[must_use]
pub fn contains(kind: ContentKind, key: &str) -> bool {
    entry(kind, key).is_some()
}

/// The numeric id of the entry with this name, or `None` when the registry has no such entry.
/// A mod entry gets its id at the freeze, so a mod reads ids only after it and never stores them.
///
/// `NeoForge`: `Registry#getId`, which returns `-1` for an unknown entry.
#[must_use]
pub fn id_of(kind: ContentKind, key: &str) -> Option<u16> {
    entry(kind, key).map(|(id, _)| id)
}

/// The name of the entry with this numeric id, or `None` when the registry has no such id.
///
/// `NeoForge`: `Registry#getKey` of the value that `Registry#byId` returns.
#[must_use]
pub fn key_of(kind: ContentKind, id: u16) -> Option<Identifier> {
    let name = match kind {
        ContentKind::Block => BlockId::new(id).map(|id| Block::from_id(id).name),
        ContentKind::Item => Item::from_id(id).map(|item| item.registry_key),
        ContentKind::EntityType => EntityType::from_raw(id).map(|ty| ty.resource_name),
    }?;
    is_entry(kind, name).then(|| Identifier::parse_static(name))
}

/// The names of every entry of the registry, in id order: the vanilla entries, then the mod
/// entries.
///
/// `NeoForge`: `Registry#keySet`.
#[must_use]
pub fn keys(kind: ContentKind) -> Vec<Identifier> {
    let tables = dynamic::tables();
    let (vanilla, custom) = match kind {
        ContentKind::Block => (BlockId::COUNT, tables.map_or(0, |t| t.blocks().len())),
        ContentKind::Item => (Item::COUNT, tables.map_or(0, |t| t.items().len())),
        ContentKind::EntityType => (
            EntityType::COUNT,
            tables.map_or(0, |t| t.entity_types().len()),
        ),
    };
    let end = usize::from(vanilla) + custom;
    (0..=u16::MAX)
        .take(end)
        .filter_map(|id| key_of(kind, id))
        .collect()
}

/// Whether the entry with this name is in the tag. `false` when the registry has no such entry
/// or no such tag.
///
/// `NeoForge`: `Holder#is(TagKey)`, and the `is(TagKey)` methods of `BlockState`, `ItemStack` and
/// `EntityType`.
#[must_use]
pub fn is_in_tag(kind: ContentKind, key: &str, tag: &str) -> bool {
    match kind {
        ContentKind::Block => {
            Block::from_name(key).is_some_and(|b| is_entry(kind, b.name) && b.has_tag_dynamic(tag))
        }
        ContentKind::Item => Item::from_registry_key(key)
            .is_some_and(|i| is_entry(kind, i.registry_key) && i.has_tag_dynamic(tag)),
        ContentKind::EntityType => EntityType::from_name(key)
            .is_some_and(|t| is_entry(kind, t.resource_name) && t.has_tag_dynamic(tag)),
    }
}

/// The names of the entries in the tag: the vanilla entries, then the mod entries in id order.
/// `None` when the registry has no such tag.
///
/// `NeoForge`: the `HolderSet.Named` that `Registry#get(TagKey)` returns.
#[must_use]
pub fn tag_entries(kind: ContentKind, tag: &str) -> Option<Vec<Identifier>> {
    let registry = match kind {
        ContentKind::Block => RegistryKey::Block,
        ContentKind::Item => RegistryKey::Item,
        ContentKind::EntityType => RegistryKey::EntityType,
    };
    let (vanilla, custom) = dynamic::tag_ids(registry, tag)?;
    Some(
        vanilla
            .iter()
            .chain(custom)
            .filter_map(|&id| key_of(kind, id))
            .collect(),
    )
}

/// The id and the stored name of the entry with this name.
fn entry(kind: ContentKind, key: &str) -> Option<(u16, &'static str)> {
    let (id, name) = match kind {
        ContentKind::Block => Block::from_name(key).map(|b| (b.id.as_u16(), b.name)),
        ContentKind::Item => Item::from_registry_key(key).map(|i| (i.id, i.registry_key)),
        ContentKind::EntityType => EntityType::from_name(key).map(|t| (t.id, t.resource_name)),
    }?;
    is_entry(kind, name).then_some((id, name))
}

fn is_entry(kind: ContentKind, name: &str) -> bool {
    !dynamic::is_placeholder(kind, name)
}
