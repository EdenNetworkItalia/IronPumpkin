use pumpkin_macros::Event;

/// `NeoForge` `PlayerContainerEvent`, a concrete parent that the server never fires.
///
/// It is the parent of `PlayerContainerEvent.Open`
/// ([`InventoryOpenEvent`](crate::plugin::api::events::inventory::inventory_open::InventoryOpenEvent))
/// and `PlayerContainerEvent.Close` ([`InventoryCloseEvent`](super::InventoryCloseEvent)).
/// Register a [`ParentEventHandler`](crate::plugin::ParentEventHandler) for it with
/// `ModInit::register_parent_event` to get both subclasses.
#[derive(Event, Clone)]
pub struct PlayerContainerEvent;
