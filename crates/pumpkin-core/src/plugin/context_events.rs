//! The `IronPumpkin` registrations of a plugin context: the receive-canceled flag and the
//! handlers on a parent event.

use std::sync::Arc;

use super::{
    Context, DynEventHandler, EventHandler, EventPriority, ParentEventHandler, ParentHandler,
    Payload, TypedEventHandler, dispatch::insert_handler,
};

impl Context {
    /// Registers an event handler like [`Context::register_event`], with the receive-canceled flag
    /// of a `NeoForge` listener: without it, the handler does not run once a handler before it
    /// cancelled the event. [`Context::register_event`] receives cancelled events.
    pub fn register_event_with<E: Payload + 'static, H>(
        &self,
        handler: Arc<H>,
        priority: EventPriority,
        blocking: bool,
        receive_canceled: bool,
    ) where
        H: EventHandler<E> + 'static,
    {
        let typed_handler: Arc<dyn DynEventHandler> = Arc::new(TypedEventHandler {
            handler,
            priority,
            blocking,
            receive_canceled,
            source: Some(self.get_metadata().name.clone()),
            _phantom: std::marker::PhantomData,
        });
        insert_handler(&self.handlers, E::get_name_static(), &typed_handler);
    }

    /// Registers a handler for the concrete parent event `P` and for every event that names `P`
    /// as a parent, as a `NeoForge` listener on a parent class receives the subclasses.
    pub fn register_parent_event<P: Payload + 'static, H>(
        &self,
        handler: Arc<H>,
        priority: EventPriority,
        blocking: bool,
        receive_canceled: bool,
    ) where
        H: ParentEventHandler + 'static,
    {
        let parent_handler: Arc<dyn DynEventHandler> = Arc::new(ParentHandler {
            handler,
            priority,
            blocking,
            receive_canceled,
            source: Some(self.get_metadata().name.clone()),
        });
        insert_handler(&self.handlers, P::get_name_static(), &parent_handler);
    }
}
