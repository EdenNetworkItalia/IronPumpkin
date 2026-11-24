use pumpkin_data::Enchantment;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::Event;

/// The highest level that an item can hold (`ItemEnchantments.Mutable.set`).
const MAX_LEVEL: i32 = 255;

/// Fires when the server asks for the gameplay enchantment levels of an item stack.
///
/// `NeoForge`: `net.neoforged.neoforge.event.enchanting.GetEnchantmentLevelEvent`, fired by
/// `EventHooks.getEnchantmentLevelSpecific` in `ItemStack.getEnchantmentLevel` for one
/// enchantment and by `EventHooks.getAllEnchantmentLevels` in `ItemStack.getAllEnchantments` for
/// all of them. Pumpkin has no registry lookup, so the event has no `getLookup` and `getHolder`.
///
/// A handler changes `enchantments` for this lookup only: the stack keeps its component. A level
/// of 0 or less removes the enchantment and a level above 255 becomes 255, as in
/// `ItemEnchantments.Mutable.set`. A handler must not read the gameplay levels of `stack`: the
/// lookup of a handler returns the levels before the event.
#[derive(Event, Clone)]
pub struct GetEnchantmentLevelEvent {
    /// The stack that the server asks about.
    pub stack: ItemStack,

    /// The levels that the caller sees. For a lookup of one enchantment (`target_enchant` is
    /// set), it holds that enchantment at its level in the stack, or nothing.
    pub enchantments: Vec<(&'static Enchantment, i32)>,

    /// The enchantment that the lookup asks for, or `None` when it asks for all of them.
    pub target_enchant: Option<&'static Enchantment>,
}

impl GetEnchantmentLevelEvent {
    #[must_use]
    pub const fn new(
        stack: ItemStack,
        enchantments: Vec<(&'static Enchantment, i32)>,
        target_enchant: Option<&'static Enchantment>,
    ) -> Self {
        Self {
            stack,
            enchantments,
            target_enchant,
        }
    }

    /// Whether a change to `enchantment` matters for this lookup: the lookup asks for it or for
    /// all enchantments (`isTargetting`).
    #[must_use]
    pub fn is_targetting(&self, enchantment: &'static Enchantment) -> bool {
        self.target_enchant
            .is_none_or(|target| target == enchantment)
    }

    /// The level of `enchantment` in `enchantments`, 0 if it is missing.
    #[must_use]
    pub fn level(&self, enchantment: &'static Enchantment) -> i32 {
        self.enchantments
            .iter()
            .find(|(enc, _)| *enc == enchantment)
            .map_or(0, |(_, level)| *level)
    }

    /// Sets the level of `enchantment` in `enchantments` (`ItemEnchantments.Mutable.set`).
    pub fn set_level(&mut self, enchantment: &'static Enchantment, level: i32) {
        let position = self
            .enchantments
            .iter()
            .position(|(enc, _)| *enc == enchantment);
        match (position, level > 0) {
            (Some(index), true) => self.enchantments[index].1 = level.min(MAX_LEVEL),
            (Some(index), false) => {
                self.enchantments.remove(index);
            }
            (None, true) => self.enchantments.push((enchantment, level.min(MAX_LEVEL))),
            (None, false) => {}
        }
    }
}
