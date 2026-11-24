use pumpkin_data::attributes::Attributes;
use pumpkin_data::data_component_impl::Modifier;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_macros::Event;

/// Fires when the server reads the attribute modifiers of an item stack.
///
/// `NeoForge`: `net.neoforged.neoforge.event.ItemAttributeModifierEvent`, fired by
/// `CommonHooks.computeModifiedAttributes` in `ItemStack.getAttributeModifiers`. Pumpkin reads the
/// modifiers of a stack for the attributes of the equipment of a living entity, the attack speed
/// of a player attack and the spear. A modifier is identified by its attribute and its id.
///
/// `modifiers` is settable: the reader takes the list as the last handler left it. The event
/// has the methods of `NeoForge` that change it (`add_modifier`, `remove_modifier`,
/// `replace_modifier`, `remove_if`, `remove_all_modifiers_for`, `clear_modifiers`).
#[derive(Event, Clone)]
pub struct ItemAttributeModifierEvent {
    /// The stack whose modifiers the server reads.
    pub item_stack: ItemStack,
    /// The modifiers of the stack before any handler (`getDefaultModifiers`).
    pub default_modifiers: Vec<Modifier>,
    /// The modifiers that the reader sees (`getModifiers`).
    pub modifiers: Vec<Modifier>,
}

impl ItemAttributeModifierEvent {
    #[must_use]
    pub fn new(item_stack: ItemStack, default_modifiers: Vec<Modifier>) -> Self {
        Self {
            item_stack,
            modifiers: default_modifiers.clone(),
            default_modifiers,
        }
    }

    /// Adds a modifier. Returns `false` and changes nothing when a modifier with the same
    /// attribute and id exists.
    pub fn add_modifier(&mut self, modifier: Modifier) -> bool {
        if self.position(modifier.r#type, &modifier.id).is_some() {
            return false;
        }
        self.modifiers.push(modifier);
        true
    }

    /// Removes the modifier with this attribute and id. Returns whether it removed one.
    pub fn remove_modifier(&mut self, attribute: &'static Attributes, id: &str) -> bool {
        match self.position(attribute, id) {
            Some(index) => {
                self.modifiers.remove(index);
                true
            }
            None => false,
        }
    }

    /// Adds a modifier and replaces the one with the same attribute and id, if any, at its
    /// position.
    pub fn replace_modifier(&mut self, modifier: Modifier) {
        match self.position(modifier.r#type, &modifier.id) {
            Some(index) => self.modifiers[index] = modifier,
            None => self.modifiers.push(modifier),
        }
    }

    /// Removes the modifiers that `condition` accepts. Returns whether it removed any.
    pub fn remove_if(&mut self, mut condition: impl FnMut(&Modifier) -> bool) -> bool {
        let before = self.modifiers.len();
        self.modifiers.retain(|modifier| !condition(modifier));
        self.modifiers.len() != before
    }

    /// Removes every modifier of `attribute`. Returns whether it removed any.
    pub fn remove_all_modifiers_for(&mut self, attribute: &'static Attributes) -> bool {
        self.remove_if(|modifier| modifier.r#type == attribute)
    }

    /// Removes every modifier.
    pub fn clear_modifiers(&mut self) {
        self.modifiers.clear();
    }

    fn position(&self, attribute: &'static Attributes, id: &str) -> Option<usize> {
        self.modifiers
            .iter()
            .position(|modifier| modifier.r#type == attribute && modifier.id == id)
    }
}
