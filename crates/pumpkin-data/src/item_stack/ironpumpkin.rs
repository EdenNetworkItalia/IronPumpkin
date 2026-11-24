use std::sync::OnceLock;

use super::*;
use crate::data_component_impl::AttributeModifiersImpl;

/// The enchantment levels of a stack that a hook may change.
pub type EnchantmentLevels = Vec<(&'static Enchantment, i32)>;

/// Called by [`ItemStack::get_enchantment_level`] and [`ItemStack::get_all_enchantments`] with the
/// stack, the enchantment that the caller asks for (`None` for all) and the levels of the stack.
/// Returns the levels that the caller sees.
pub type EnchantmentLevelHook =
    fn(&ItemStack, Option<&'static Enchantment>, EnchantmentLevels) -> EnchantmentLevels;

/// Called by [`ItemStack::get_attribute_modifiers`] with the stack and its default modifiers.
/// Returns the modifiers that the caller sees, or `None` for the default ones.
pub type AttributeModifiersHook =
    fn(&ItemStack, &AttributeModifiersImpl) -> Option<AttributeModifiersImpl>;

/// A lookup with no hook set costs one atomic load: the hook is set once, when the first handler
/// of `GetEnchantmentLevelEvent` registers.
static ENCHANTMENT_LEVEL_HOOK: OnceLock<EnchantmentLevelHook> = OnceLock::new();

/// Set once, when the first handler of `ItemAttributeModifierEvent` registers.
static ATTRIBUTE_MODIFIERS_HOOK: OnceLock<AttributeModifiersHook> = OnceLock::new();

/// Sets the hook of the enchantment level reads. Only the first call has an effect.
pub fn set_enchantment_level_hook(hook: EnchantmentLevelHook) {
    let _ = ENCHANTMENT_LEVEL_HOOK.set(hook);
}

/// Sets the hook of the attribute modifier reads. Only the first call has an effect.
pub fn set_attribute_modifiers_hook(hook: AttributeModifiersHook) {
    let _ = ATTRIBUTE_MODIFIERS_HOOK.set(hook);
}

impl ItemStack {
    /// The level of `enchantment` in the enchantments component, with no hook. Use
    /// [`Self::get_enchantment_level`] for gameplay.
    #[must_use]
    pub fn get_tag_enchantment_level(&self, enchantment: &'static Enchantment) -> i32 {
        self.get_data_component::<EnchantmentsImpl>()
            .and_then(|data| {
                data.enchantment
                    .iter()
                    .find(|(enc, _)| *enc == enchantment)
                    .map(|(_, level)| *level)
            })
            .unwrap_or(0)
    }

    /// The gameplay level of `enchantment`: the level of the component, changed by the hook of
    /// `GetEnchantmentLevelEvent` (`ItemStack.getEnchantmentLevel` of `NeoForge`).
    #[must_use]
    pub fn get_enchantment_level(&self, enchantment: &'static Enchantment) -> i32 {
        let level = self.get_tag_enchantment_level(enchantment);
        let Some(hook) = ENCHANTMENT_LEVEL_HOOK.get() else {
            return level;
        };
        let levels = if level > 0 {
            vec![(enchantment, level)]
        } else {
            Vec::new()
        };
        hook(self, Some(enchantment), levels)
            .iter()
            .find(|(enc, _)| *enc == enchantment)
            .map_or(0, |(_, level)| (*level).max(0))
    }

    /// The gameplay enchantments: the component, changed by the hook of `GetEnchantmentLevelEvent`
    /// (`ItemStack.getAllEnchantments` of `NeoForge`). Borrows the component when no hook is set.
    #[must_use]
    pub fn get_all_enchantments(&self) -> Option<Cow<'_, EnchantmentsImpl>> {
        let tag = self.get_data_component::<EnchantmentsImpl>();
        let Some(hook) = ENCHANTMENT_LEVEL_HOOK.get() else {
            return tag.map(Cow::Borrowed);
        };
        let levels = tag.map_or_else(Vec::new, |tag| tag.enchantment.to_vec());
        let levels = hook(self, None, levels);
        if levels.is_empty() && tag.is_none() {
            return None;
        }
        Some(Cow::Owned(EnchantmentsImpl {
            enchantment: Cow::Owned(levels),
        }))
    }

    /// The gameplay attribute modifiers: the modifiers of the component, changed by the hook of
    /// `ItemAttributeModifierEvent` (`ItemStack.getAttributeModifiers` of `NeoForge`). Borrows the
    /// component when no hook is set.
    #[must_use]
    pub fn get_attribute_modifiers(&self) -> Option<Cow<'_, AttributeModifiersImpl>> {
        let default = self.get_data_component::<AttributeModifiersImpl>();
        let Some(hook) = ATTRIBUTE_MODIFIERS_HOOK.get() else {
            return default.map(Cow::Borrowed);
        };
        let empty = AttributeModifiersImpl {
            attribute_modifiers: Cow::Borrowed(&[]),
        };
        match hook(self, default.unwrap_or(&empty)) {
            Some(modifiers) => Some(Cow::Owned(modifiers)),
            None => default.map(Cow::Borrowed),
        }
    }

    /// The value of the mod data component type `id`, in NBT form.
    #[must_use]
    pub fn get_custom_component(&self, id: u16) -> Option<&NbtTag> {
        self.custom_components
            .iter()
            .find(|(component, _)| *component == id)
            .map(|(_, value)| value)
    }

    /// Sets the value of the mod data component type `id`, in NBT form.
    pub fn set_custom_component(&mut self, id: u16, value: NbtTag) {
        if let Some((_, old)) = self
            .custom_components
            .iter_mut()
            .find(|(component, _)| *component == id)
        {
            *old = value;
        } else {
            self.custom_components.push((id, value));
        }
    }

    /// Removes the value of the mod data component type `id` and returns it.
    pub fn remove_custom_component(&mut self, id: u16) -> Option<NbtTag> {
        let index = self
            .custom_components
            .iter()
            .position(|(component, _)| *component == id)?;
        Some(self.custom_components.swap_remove(index).1)
    }

    /// Whether a client in mode `ids` sees this stack differently, and `incoming` is the stack it
    /// sees: the shown item with the same count and the components the client gets. A client that
    /// sends a stack back unchanged (creative mode does) sends this, and the server keeps its own
    /// stack. A `Display` client sees every custom item as its display item and no mod
    /// components, a `Real` client only placeholders as their display items.
    #[must_use]
    pub fn is_displayed_as(&self, incoming: &Self, ids: crate::dynamic::ContentIds) -> bool {
        let display = self.item.to_java_network_id(ids);
        let hides_components =
            ids == crate::dynamic::ContentIds::Display && !self.custom_components.is_empty();
        if display == self.item.id && !hides_components {
            return false;
        }
        let Some(item) = Item::from_id(display) else {
            return false;
        };
        let mut shown = self.clone();
        shown.item = item;
        if hides_components {
            shown.custom_components.clear();
        }
        shown.are_equal(incoming)
    }
}
