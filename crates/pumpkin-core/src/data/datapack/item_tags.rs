//! The item tags that the loaded data packs define or change, built as vanilla
//! `TagLoader.build` does (`TagLoader.java:91-117`).
//!
//! A generated tag is a flat list of items, so a change to a tag does not reach the generated
//! tags that reference it. This module builds again each tag that a pack changes and each
//! generated tag that reaches a changed tag through `#` references, from the entries of the
//! generated tags (`ITEM_TAG_DEFINITIONS`) and the entries of the packs.

use std::collections::{HashMap, HashSet};

use pumpkin_data::dynamic::{namespaced_name, tag_ids};
use pumpkin_data::item::Item;
use pumpkin_data::tag::{ITEM_TAG_DEFINITIONS, RegistryKey};
use tracing::error;

use super::tag_loader::DynamicTagEntry;

enum Target {
    Item(u16),
    /// An item that a pack names, which may not exist.
    Name(String),
    Tag(String),
}

struct Entry {
    target: Target,
    required: bool,
}

struct Builder<'a> {
    generated: HashMap<&'static str, (&'static [u16], &'static [&'static str])>,
    packs: &'a HashMap<String, DynamicTagEntry>,
    /// The tags that a pack changes or that reach such a tag through references.
    affected: HashSet<String>,
    built: HashMap<String, Option<Vec<u16>>>,
    building: HashSet<String>,
}

impl Builder<'_> {
    /// The entries of the tag `name`: the generated entries unless a pack replaces the tag, then
    /// the entries of the packs.
    fn entries(&self, name: &str) -> Vec<Entry> {
        let pack = self.packs.get(name);
        let mut entries = Vec::new();
        let item = |id: u16| Entry {
            target: Target::Item(id),
            required: true,
        };
        if !pack.is_some_and(|pack| pack.replace) {
            if let Some((ids, references)) = self.generated.get(name) {
                entries.extend(ids.iter().map(|&id| item(id)));
                entries.extend(references.iter().map(|reference| Entry {
                    target: Target::Tag((*reference).to_string()),
                    required: true,
                }));
                // The custom items of the tag, which a mod lists.
                if let Some((_, custom)) = tag_ids(RegistryKey::Item, name) {
                    entries.extend(custom.iter().map(|&id| item(id)));
                }
            } else if let Some((generated, custom)) = tag_ids(RegistryKey::Item, name) {
                entries.extend(generated.iter().chain(custom).map(|&id| item(id)));
            }
        }
        for value in pack.iter().flat_map(|pack| &pack.values) {
            let required = !pack.is_some_and(|pack| pack.optional.contains(value));
            let target = value.strip_prefix('#').map_or_else(
                || Target::Name(namespaced_name(value).into_owned()),
                |tag| Target::Tag(namespaced_name(tag).into_owned()),
            );
            entries.push(Entry { target, required });
        }
        entries
    }

    /// The members of the tag `name`, or `None` when the tag fails or does not exist. A tag that
    /// no pack touches keeps the members of the generated tag.
    fn build(&mut self, name: &str) -> Option<Vec<u16>> {
        if !self.affected.contains(name) {
            return tag_ids(RegistryKey::Item, name)
                .map(|(generated, custom)| generated.iter().chain(custom).copied().collect());
        }
        if let Some(built) = self.built.get(name) {
            return built.clone();
        }
        // A tag that is under construction closes a cycle, so the reference to it is missing.
        if !self.building.insert(name.to_string()) {
            return None;
        }
        let mut members: Vec<u16> = Vec::new();
        let mut missing = Vec::new();
        let add = |members: &mut Vec<u16>, id: u16| {
            if !members.contains(&id) {
                members.push(id);
            }
        };
        for entry in self.entries(name) {
            match entry.target {
                Target::Item(id) => add(&mut members, id),
                Target::Name(item) => match Item::from_registry_key(&item) {
                    Some(item) => add(&mut members, item.id),
                    None if entry.required => missing.push(item),
                    None => {}
                },
                Target::Tag(tag) => match self.build(&tag) {
                    Some(ids) => ids.into_iter().for_each(|id| add(&mut members, id)),
                    None if entry.required => missing.push(format!("#{tag}")),
                    None => {}
                },
            }
        }
        self.building.remove(name);
        let built = if missing.is_empty() {
            Some(members)
        } else {
            error!(
                "Couldn't load tag {name} as it is missing following references: \n\t{}",
                missing.join("\n\t")
            );
            None
        };
        self.built.insert(name.to_string(), built.clone());
        built
    }
}

/// The item tags that the loaded packs define or change, by namespaced name, as item ids. A tag
/// that fails to build has no members.
pub(super) fn data_pack_item_tags(
    tags: &HashMap<String, HashMap<String, DynamicTagEntry>>,
) -> HashMap<String, Vec<u16>> {
    let Some(packs) = tags.get("item") else {
        return HashMap::new();
    };
    let generated: HashMap<_, _> = ITEM_TAG_DEFINITIONS
        .iter()
        .map(|&(name, ids, references)| (name, (ids, references)))
        .collect();
    let mut affected: HashSet<String> = packs.keys().cloned().collect();
    loop {
        let before = affected.len();
        for (name, (_, references)) in &generated {
            if !affected.contains(*name)
                && references
                    .iter()
                    .any(|reference| affected.contains(*reference))
            {
                affected.insert((*name).to_string());
            }
        }
        if affected.len() == before {
            break;
        }
    }
    let mut builder = Builder {
        generated,
        packs,
        affected,
        built: HashMap::new(),
        building: HashSet::new(),
    };
    let mut names: Vec<String> = builder.affected.iter().cloned().collect();
    names.sort_unstable();
    names
        .into_iter()
        .map(|name| {
            let members = builder.build(&name).unwrap_or_default();
            (name, members)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(values: &[&str], replace: bool, optional: &[&str]) -> DynamicTagEntry {
        DynamicTagEntry {
            id: String::new(),
            values: values.iter().map(ToString::to_string).collect(),
            replace,
            optional: optional.iter().map(ToString::to_string).collect(),
        }
    }

    fn build(items: &[(&str, DynamicTagEntry)]) -> HashMap<String, Vec<u16>> {
        let items = items
            .iter()
            .map(|(name, entry)| ((*name).to_string(), entry.clone()))
            .collect();
        data_pack_item_tags(&HashMap::from([("item".to_string(), items)]))
    }

    fn has(tags: &HashMap<String, Vec<u16>>, name: &str, item: &Item) -> bool {
        tags.get(name)
            .unwrap_or_else(|| panic!("{name} is built"))
            .contains(&item.id)
    }

    /// `logs_that_burn` lists `#minecraft:oak_logs`, so a change to `oak_logs` reaches it.
    #[test]
    fn a_tag_that_a_pack_extends_reaches_the_tags_that_reference_it() {
        let tags = build(&[("minecraft:oak_logs", tag(&["minecraft:dirt"], false, &[]))]);
        for name in ["minecraft:oak_logs", "minecraft:logs_that_burn"] {
            assert!(has(&tags, name, &Item::DIRT), "{name}");
            assert!(has(&tags, name, &Item::OAK_LOG), "{name}");
        }
        assert!(has(&tags, "minecraft:logs_that_burn", &Item::BIRCH_LOG));
    }

    #[test]
    fn a_tag_that_a_pack_replaces_reaches_the_tags_that_reference_it() {
        let tags = build(&[("minecraft:oak_logs", tag(&["minecraft:dirt"], true, &[]))]);
        assert!(has(&tags, "minecraft:logs_that_burn", &Item::DIRT));
        assert!(!has(&tags, "minecraft:logs_that_burn", &Item::OAK_LOG));
        assert!(has(&tags, "minecraft:logs_that_burn", &Item::BIRCH_LOG));
    }

    /// Vanilla `TagLoader.tryBuildTag` fails a tag with a missing required entry, and a tag that
    /// requires a failed tag. An optional entry may be missing.
    #[test]
    fn a_tag_with_a_missing_required_entry_fails() {
        let tags = build(&[
            (
                "test:required",
                tag(&["minecraft:stone", "test:missing"], false, &[]),
            ),
            (
                "test:optional",
                tag(
                    &["minecraft:stone", "test:missing"],
                    false,
                    &["test:missing"],
                ),
            ),
            ("test:user", tag(&["#test:required"], false, &[])),
            ("test:reference", tag(&["#test:nothing"], false, &[])),
            (
                "test:optional_reference",
                tag(
                    &["minecraft:stone", "#test:nothing"],
                    false,
                    &["#test:nothing"],
                ),
            ),
        ]);
        assert!(tags["test:required"].is_empty());
        assert_eq!(tags["test:optional"], [Item::STONE.id]);
        assert!(tags["test:user"].is_empty());
        assert!(tags["test:reference"].is_empty());
        assert_eq!(tags["test:optional_reference"], [Item::STONE.id]);
    }

    #[test]
    fn a_tag_in_a_reference_cycle_fails() {
        let tags = build(&[
            ("test:a", tag(&["minecraft:stone", "#test:b"], false, &[])),
            ("test:b", tag(&["minecraft:dirt", "#test:a"], false, &[])),
            ("test:c", tag(&["#test:a"], false, &[])),
            ("test:fine", tag(&["minecraft:stone"], false, &[])),
        ]);
        for name in ["test:a", "test:b", "test:c"] {
            assert!(tags[name].is_empty(), "{name}");
        }
        assert_eq!(tags["test:fine"], [Item::STONE.id]);
    }
}
