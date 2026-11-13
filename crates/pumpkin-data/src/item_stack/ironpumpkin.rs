use super::*;

impl ItemStack {
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
