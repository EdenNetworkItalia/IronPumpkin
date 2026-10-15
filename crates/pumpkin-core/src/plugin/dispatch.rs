//! The `IronPumpkin` part of the event dispatch: the order of the handlers, the cancelled events,
//! the parent events and the helpers that build an event only when a handler listens.

use std::{borrow::Cow, sync::Arc};

use arc_swap::ArcSwap;

use super::{BoxFuture, DynEventHandler, EventPriority, HandlerMap, Payload, PluginManager};
use crate::server::Server;

/// Handles a concrete parent event and each event that names it as a parent.
///
/// A subclass names its parents in [`Payload::get_parent_names_static`], as a `NeoForge` listener
/// on a parent class receives the subclasses. The handler gets the subclass struct and downcasts it with
/// `<dyn Payload>::downcast_mut`.
pub trait ParentEventHandler: Send + Sync {
    /// Handles the event without changing it, after the blocking handlers.
    fn handle<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        _event: &'a (dyn Payload + Send + Sync),
    ) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }

    /// Handles the event and can change it, as `EventHandler::handle_blocking` does.
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        _event: &'a mut (dyn Payload + Send + Sync),
    ) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }
}

/// A [`ParentEventHandler`] with its registration flags.
pub struct ParentHandler<H: ParentEventHandler> {
    pub handler: Arc<H>,
    pub priority: EventPriority,
    pub blocking: bool,
    pub receive_canceled: bool,
    pub source: Option<String>,
}

impl<H: ParentEventHandler> DynEventHandler for ParentHandler<H> {
    fn handle_dyn<'a>(
        &'a self,
        server: &'a Arc<Server>,
        event: &'a (dyn Payload + Send + Sync),
    ) -> BoxFuture<'a, ()> {
        self.handler.handle(server, event)
    }

    fn handle_blocking_dyn<'a>(
        &'a self,
        server: &'a Arc<Server>,
        event: &'a mut (dyn Payload + Send + Sync),
    ) -> BoxFuture<'a, ()> {
        self.handler.handle_blocking(server, event)
    }

    fn is_blocking(&self) -> bool {
        self.blocking
    }

    fn get_priority(&self) -> &EventPriority {
        &self.priority
    }

    fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    fn receive_canceled(&self) -> bool {
        self.receive_canceled
    }
}

/// The dispatch order: blocking handlers before non-blocking ones, then `Highest` to `Lowest`.
fn dispatch_rank(handler: &dyn DynEventHandler) -> (bool, &EventPriority) {
    (!handler.is_blocking(), handler.get_priority())
}

/// Adds `handler` to the handlers of `event_name` after every handler of the same or an earlier
/// rank, so each list stays in dispatch order and a fire never sorts.
pub(crate) fn insert_handler(
    handlers: &ArcSwap<HandlerMap>,
    event_name: &'static str,
    handler: &Arc<dyn DynEventHandler>,
) {
    handlers.rcu(|handlers| {
        let mut new_handlers = (**handlers).clone();
        let list = new_handlers.entry(event_name).or_default();
        let rank = dispatch_rank(handler.as_ref());
        let index = list.partition_point(|other| dispatch_rank(other.as_ref()) <= rank);
        list.insert(index, handler.clone());
        Arc::new(new_handlers)
    });
}

pub(crate) fn has_handlers_in(map: &HandlerMap, event_name: &str, parent_names: &[&str]) -> bool {
    std::iter::once(event_name)
        .chain(parent_names.iter().copied())
        .any(|name| map.get(name).is_some_and(|handlers| !handlers.is_empty()))
}

/// The handlers of an event and of its parents in dispatch order. Inside one rank the handlers
/// of the event come first, then those of each parent, nearest first. Only an event whose
/// parents have handlers allocates.
fn dispatch_list<'a>(
    map: &'a HandlerMap,
    event_name: &str,
    parent_names: &[&str],
) -> Cow<'a, [Arc<dyn DynEventHandler>]> {
    let own = map.get(event_name).map_or(&[][..], Vec::as_slice);
    let mut inherited = parent_names
        .iter()
        .filter_map(|name| map.get(name))
        .filter(|handlers| !handlers.is_empty());
    let Some(first) = inherited.next() else {
        return Cow::Borrowed(own);
    };
    let mut merged: Vec<_> = own.iter().chain(first).cloned().collect();
    for handlers in inherited {
        merged.extend(handlers.iter().cloned());
    }
    merged.sort_by(|a, b| dispatch_rank(a.as_ref()).cmp(&dispatch_rank(b.as_ref())));
    Cow::Owned(merged)
}

/// Calls the handlers in dispatch order. A handler without receive-canceled does not run once a
/// handler before it cancelled the event.
pub(crate) async fn dispatch(
    map: &HandlerMap,
    server: &Arc<Server>,
    event_name: &str,
    parent_names: &[&str],
    event: &mut (dyn Payload + Send + Sync),
) {
    for handler in dispatch_list(map, event_name, parent_names).iter() {
        if !handler.receive_canceled() && event.is_cancelled() {
            continue;
        }
        if handler.is_blocking() {
            handler.handle_blocking_dyn(server, event).await;
        } else {
            handler.handle_dyn(server, event).await;
        }
    }
}

impl PluginManager {
    /// Builds the event with `make` only when a handler listens, fires it and returns it with the
    /// changes of the handlers. Returns `None` without calling `make` when nobody listens, so a
    /// fire site with no handler allocates nothing.
    pub async fn fire_with<E, F>(&self, server: &Arc<Server>, make: F) -> Option<E>
    where
        E: Payload + Send + Sync + 'static,
        F: FnOnce() -> E,
    {
        if !self.has_handlers::<E>() {
            return None;
        }
        let mut event = make();
        self.fire(server, &mut event).await;
        Some(event)
    }

    /// The synchronous form of [`PluginManager::fire_with`], for fire sites outside async code.
    pub fn fire_blocking_with<E, F>(&self, server: &Arc<Server>, make: F) -> Option<E>
    where
        E: Payload + Send + Sync + 'static,
        F: FnOnce() -> E,
    {
        if !self.has_handlers::<E>() {
            return None;
        }
        let mut event = make();
        self.fire_blocking(server, &mut event);
        Some(event)
    }
}
