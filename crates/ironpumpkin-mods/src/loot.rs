//! Global loot modifiers: data pack files that change the loot of every loot table roll.
//!
//! `NeoForge`: `IGlobalLootModifier` and `LootModifier`, with their codecs registered in
//! `NeoForgeRegistries.GLOBAL_LOOT_MODIFIER_SERIALIZERS` and the files loaded by
//! `LootModifierManager`.
//!
//! A mod registers a modifier type with [`ModInit::register_loot_modifier_type`]: an id and the
//! callback that decodes a file of that type. The files are
//! `data/<namespace>/loot_modifiers/<path>.json` in a data pack, one modifier per file with the id
//! `<namespace>:<path>`. A pack higher in the pack order replaces a file with the same id. Each
//! file loads on startup and on `/reload`; a file with an unknown type, an unknown condition or a
//! failed decode is logged by id and skipped. A `neoforge:conditions` block in a file is ignored:
//! the file loads as if it had none.
//!
//! The host reads the fields of `LootModifier` before the callback runs: `type`, an optional
//! `condition` (an inline loot condition or a predicate id, with the conditions of Pumpkin and
//! `neoforge:loot_table_id`) and `priority` (default [`DEFAULT_PRIORITY`]). The callback gets the
//! whole JSON object and reads its own fields.
//!
//! After each loot table roll, the host runs the modifiers whose condition passes, from the
//! highest priority to the lowest and inside one priority by id, and drops the list that the last
//! one returns. A table that another table nests, or that [`LootContext::roll_loot_table_raw`]
//! rolls, rolls without modifiers, as `getRandomItemsRaw`.
//!
//! The built-in type [`ADD_TABLE`] adds the loot of the table `table`, and the condition
//! [`LOOT_TABLE_ID`] passes for the table `loot_table_id` only:
//!
//! ```json
//! {
//!   "type": "neoforge:add_table",
//!   "condition": { "type": "neoforge:loot_table_id", "loot_table_id": "minecraft:chests/simple_dungeon" },
//!   "table": "examplemod:chests/extra"
//! }
//! ```
//!
//! A mod type that removes an item:
//!
//! ```
//! use ironpumpkin_mods::{
//!     ModInit,
//!     loot::{GlobalLootModifier, ItemStack, LootContext, Value},
//!     pumpkin_data::item::Item,
//! };
//!
//! struct RemoveItem {
//!     item: &'static Item,
//! }
//!
//! impl GlobalLootModifier for RemoveItem {
//!     fn apply(&self, mut loot: Vec<ItemStack>, _context: &mut LootContext<'_, '_>) -> Vec<ItemStack> {
//!         loot.retain(|stack| stack.item != self.item);
//!         loot
//!     }
//! }
//!
//! fn register(cx: &mut ModInit) {
//!     let _ = cx.register_loot_modifier_type("examplemod:remove_item", |json: &Value| {
//!         let item = json
//!             .get("item")
//!             .and_then(Value::as_str)
//!             .and_then(Item::from_registry_key)
//!             .ok_or("no item")?;
//!         Ok(Box::new(RemoveItem { item }) as Box<dyn GlobalLootModifier>)
//!     });
//! }
//! ```

pub use pumpkin_core::world::loot::LootContextParameters;
pub use pumpkin_core::world::loot::modifiers::{
    ADD_TABLE, DEFAULT_PRIORITY, GlobalLootModifier, LOOT_TABLE_ID, LootContext, LootModifierError,
    UNKNOWN_LOOT_TABLE,
};
pub use pumpkin_data::item_stack::ItemStack;
pub use serde_json::Value;

use pumpkin_core::world::loot::modifiers::LOOT_MODIFIER_TYPES;

use crate::ModInit;

impl ModInit {
    /// Registers the global loot modifier type `id`, `namespace:path`, with the callback that
    /// decodes its files. An error from the callback is the reason the file does not load. It
    /// applies at once.
    ///
    /// # Errors
    ///
    /// [`LootModifierError::Duplicate`] for an existing id, [`LootModifierError::InvalidId`] for
    /// an id that is not a resource location, and [`LootModifierError::RegistrationClosed`] after
    /// the registration phase, which ends with the last `RegisterEvent`.
    pub fn register_loot_modifier_type<F>(
        &mut self,
        id: &str,
        decode: F,
    ) -> Result<(), LootModifierError>
    where
        F: Fn(&Value) -> Result<Box<dyn GlobalLootModifier>, String> + Send + Sync + 'static,
    {
        LOOT_MODIFIER_TYPES.register(id, Box::new(decode))
    }
}

/// Ends the registration phase of the modifier types, as the registries freeze after the last
/// `RegisterEvent`.
#[cfg_attr(
    test,
    expect(dead_code, reason = "the lifecycle tests leave the registry open")
)]
pub(crate) fn close_registration() {
    LOOT_MODIFIER_TYPES.close();
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Keep;

    impl GlobalLootModifier for Keep {
        fn apply(
            &self,
            loot: Vec<ItemStack>,
            _context: &mut LootContext<'_, '_>,
        ) -> Vec<ItemStack> {
            loot
        }
    }

    fn keep(json: &Value) -> Result<Box<dyn GlobalLootModifier>, String> {
        json.as_object().ok_or("not an object")?;
        Ok(Box::new(Keep))
    }

    #[test]
    fn a_duplicate_type_fails() {
        let mut cx = ModInit::new("lootmod");
        assert_eq!(cx.register_loot_modifier_type("lootmod:keep", keep), Ok(()));
        assert_eq!(
            cx.register_loot_modifier_type("lootmod:keep", keep),
            Err(LootModifierError::Duplicate("lootmod:keep".to_owned()))
        );
        assert_eq!(
            cx.register_loot_modifier_type(ADD_TABLE, keep),
            Err(LootModifierError::Duplicate(ADD_TABLE.to_owned()))
        );
    }
}
