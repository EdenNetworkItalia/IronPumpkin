//! Evaluates the context int and float providers that the data packs define.
//!
//! It covers what the cooking fuel needs: constants, references by id, `conditional` on a
//! `match_block` predicate, and `div`. A provider of another type has no value here.

use pumpkin_data::Block;
use pumpkin_data::dynamic::DynamicTaggable;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;

use super::DatapackManager;
use super::context_provider_loader::json_value_to_nbt;

/// A reference chain longer than this is a cycle.
const MAX_DEPTH: u8 = 16;

/// What a provider reads: the providers and predicates of the data packs, and the block of the
/// loot context (the furnace).
pub struct ProviderContext<'a> {
    pub datapacks: &'a DatapackManager,
    pub block: &'a Block,
}

/// The selected branch of a `conditional` provider.
enum Branch<'t> {
    Provider(&'t NbtTag),
    /// A missing `on_false`, which vanilla reads as 0.
    Zero,
}

fn provider_type(compound: &NbtCompound) -> &str {
    let name = compound.get_string("type").unwrap_or_default();
    name.strip_prefix("minecraft:").unwrap_or(name)
}

impl ProviderContext<'_> {
    /// The value of the int provider with the id `id`.
    #[must_use]
    pub fn int_by_id(&self, id: &str) -> Option<i32> {
        self.int_at(&self.datapacks.get_context_int_provider(id)?, 1)
    }

    /// The value of the float provider with the id `id`.
    #[must_use]
    pub fn float_by_id(&self, id: &str) -> Option<f32> {
        self.float_at(&self.datapacks.get_context_float_provider(id)?, 1)
    }

    fn int_at(&self, provider: &NbtTag, depth: u8) -> Option<i32> {
        if depth > MAX_DEPTH {
            return None;
        }
        match provider {
            NbtTag::String(id) => {
                self.int_at(&self.datapacks.get_context_int_provider(id)?, depth + 1)
            }
            NbtTag::Compound(compound) => match provider_type(compound) {
                "constant" => compound.get("value")?.as_numeric_int(),
                "conditional" => match self.branch(compound, depth)? {
                    Branch::Provider(branch) => self.int_at(branch, depth + 1),
                    Branch::Zero => Some(0),
                },
                "div" => {
                    let left = self.int_at(compound.get("left")?, depth + 1)?;
                    let right = self.int_at(compound.get("right")?, depth + 1)?;
                    left.checked_div(right)
                }
                _ => None,
            },
            number => number.as_numeric_int(),
        }
    }

    fn float_at(&self, provider: &NbtTag, depth: u8) -> Option<f32> {
        if depth > MAX_DEPTH {
            return None;
        }
        match provider {
            NbtTag::String(id) => {
                self.float_at(&self.datapacks.get_context_float_provider(id)?, depth + 1)
            }
            NbtTag::Compound(compound) => match provider_type(compound) {
                "constant" => compound.get("value")?.as_numeric_float(),
                "conditional" => match self.branch(compound, depth)? {
                    Branch::Provider(branch) => self.float_at(branch, depth + 1),
                    Branch::Zero => Some(0.0),
                },
                "div" => {
                    let left = self.float_at(compound.get("left")?, depth + 1)?;
                    let right = self.float_at(compound.get("right")?, depth + 1)?;
                    Some(left / right)
                }
                _ => None,
            },
            number => number.as_numeric_float(),
        }
    }

    /// The branch of a `conditional` provider that its condition selects.
    fn branch<'t>(&self, compound: &'t NbtCompound, depth: u8) -> Option<Branch<'t>> {
        let selected = if self.condition(compound.get("condition")?, depth)? {
            "on_true"
        } else {
            "on_false"
        };
        match compound.get(selected) {
            Some(branch) => Some(Branch::Provider(branch)),
            None if selected == "on_false" => Some(Branch::Zero),
            None => None,
        }
    }

    /// A predicate by id or inline. Only `match_block` has a value here.
    fn condition(&self, condition: &NbtTag, depth: u8) -> Option<bool> {
        if depth > MAX_DEPTH {
            return None;
        }
        match condition {
            NbtTag::String(id) => {
                let predicate = json_value_to_nbt(&self.datapacks.get_predicate(id)?);
                self.condition(&predicate, depth + 1)
            }
            NbtTag::Compound(compound) if provider_type(compound) == "match_block" => {
                let matches = |entry: &str| {
                    entry.strip_prefix('#').map_or_else(
                        || {
                            pumpkin_data::dynamic::namespaced_name(entry)
                                == pumpkin_data::dynamic::namespaced_name(self.block.name)
                        },
                        |tag| self.block.has_tag_dynamic(tag),
                    )
                };
                match compound.get("blocks")? {
                    NbtTag::String(entry) => Some(matches(entry)),
                    NbtTag::List(entries) => Some(
                        entries
                            .iter()
                            .filter_map(NbtTag::extract_string)
                            .any(matches),
                    ),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::super::context_provider_loader::load_embedded_context_providers;
    use super::*;

    fn manager() -> DatapackManager {
        let manager = DatapackManager::new();
        let (mut ints, mut floats) = (HashMap::new(), HashMap::new());
        load_embedded_context_providers(&mut ints, &mut floats);
        for (id, provider) in ints {
            manager.insert_context_int_provider(id, provider);
        }
        for (id, provider) in floats {
            manager.insert_context_float_provider(id, provider);
        }
        manager
    }

    fn context<'a>(manager: &'a DatapackManager, block: &'a Block) -> ProviderContext<'a> {
        ProviderContext {
            datapacks: manager,
            block,
        }
    }

    /// `time_coal` divides 1600 by a `fast_cooking` condition on the block of the furnace.
    #[test]
    fn a_provider_reads_the_block_of_the_furnace() {
        let manager = manager();
        let furnace = context(&manager, &Block::FURNACE);
        let smoker = context(&manager, &Block::SMOKER);
        assert_eq!(furnace.int_by_id("minecraft:cooking/time_coal"), Some(1600));
        assert_eq!(smoker.int_by_id("minecraft:cooking/time_coal"), Some(800));
        assert_eq!(
            furnace.float_by_id("minecraft:cooking/speed_default"),
            Some(1.0)
        );
        assert_eq!(
            smoker.float_by_id("minecraft:cooking/speed_default"),
            Some(2.0)
        );
        assert_eq!(furnace.float_by_id("minecraft:cooking/missing"), None);
    }

    /// A data pack provider with the id of a built-in provider replaces it.
    #[test]
    fn a_provider_that_a_pack_defines_replaces_the_built_in_one() {
        let manager = manager();
        manager.insert_context_float_provider(
            "minecraft:cooking/speed_default".to_string(),
            NbtTag::Double(4.0),
        );
        manager.insert_context_int_provider(
            "minecraft:cooking/time_coal".to_string(),
            NbtTag::Int(10),
        );
        let furnace = context(&manager, &Block::FURNACE);
        assert_eq!(
            furnace.float_by_id("minecraft:cooking/speed_default"),
            Some(4.0)
        );
        assert_eq!(furnace.int_by_id("minecraft:cooking/time_coal"), Some(10));
    }

    /// A reference cycle has no value.
    #[test]
    fn a_reference_cycle_has_no_value() {
        let manager = DatapackManager::new();
        manager.insert_context_int_provider("test:a".to_string(), NbtTag::String("test:b".into()));
        manager.insert_context_int_provider("test:b".to_string(), NbtTag::String("test:a".into()));
        assert_eq!(context(&manager, &Block::FURNACE).int_by_id("test:a"), None);
    }
}
