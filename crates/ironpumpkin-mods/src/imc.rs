//! Inter-mod messages, like FML 12.0.8 `InterModComms`.
//!
//! A mod sends messages in [`NativeMod::inter_mod_enqueue`](crate::NativeMod::inter_mod_enqueue)
//! and receives the messages sent to it in
//! [`NativeMod::inter_mod_process`](crate::NativeMod::inter_mod_process). The queue lives for one
//! startup: a message that its target does not read is dropped after the process step.

use std::{
    any::Any,
    collections::{BTreeMap, VecDeque},
    fmt,
};

use crate::mod_list::ModList;

/// One message (`InterModComms.IMCMessage`).
pub struct ImcMessage {
    sender_mod_id: String,
    mod_id: String,
    method: String,
    payload: Box<dyn Any>,
}

impl ImcMessage {
    /// The mod that sent the message.
    #[must_use]
    pub fn sender_mod_id(&self) -> &str {
        &self.sender_mod_id
    }

    /// The mod the message is for.
    #[must_use]
    pub fn mod_id(&self) -> &str {
        &self.mod_id
    }

    /// The method name that the sender and the receiver agree on.
    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }

    /// The payload, if it has the type `T`. FML passes a `Supplier<?>` that the receiver casts.
    #[must_use]
    pub fn payload<T: Any>(&self) -> Option<&T> {
        self.payload.downcast_ref()
    }
}

impl fmt::Debug for ImcMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImcMessage")
            .field("sender_mod_id", &self.sender_mod_id)
            .field("mod_id", &self.mod_id)
            .field("method", &self.method)
            .finish_non_exhaustive()
    }
}

/// The messages of every target mod, in send order.
#[derive(Default)]
pub(crate) struct ImcQueue {
    queues: BTreeMap<String, VecDeque<ImcMessage>>,
}

/// What [`NativeMod::inter_mod_enqueue`](crate::NativeMod::inter_mod_enqueue) gets: the send side
/// of the queue, in the name of the mod.
pub struct InterModEnqueue<'a> {
    pub(crate) sender_mod_id: &'a str,
    pub(crate) mod_list: &'a ModList,
    pub(crate) queue: &'a mut ImcQueue,
}

impl InterModEnqueue<'_> {
    /// `InterModComms.sendTo`: queues `payload` for the mod `mod_id` under `method`. Returns
    /// false, and drops the message, when `mod_id` is not loaded.
    pub fn send<T: Any>(&mut self, mod_id: &str, method: impl Into<String>, payload: T) -> bool {
        if !self.mod_list.is_loaded(mod_id) {
            return false;
        }
        self.queue
            .queues
            .entry(mod_id.to_owned())
            .or_default()
            .push_back(ImcMessage {
                sender_mod_id: self.sender_mod_id.to_owned(),
                mod_id: mod_id.to_owned(),
                method: method.into(),
                payload: Box::new(payload),
            });
        true
    }
}

/// What [`NativeMod::inter_mod_process`](crate::NativeMod::inter_mod_process) gets: the receive
/// side of the queue of the mod.
pub struct InterModProcess<'a> {
    pub(crate) mod_id: &'a str,
    pub(crate) queue: &'a mut ImcQueue,
}

impl InterModProcess<'_> {
    /// `InterModComms.getMessages(modId)` for the mod's own id: removes and returns the messages
    /// sent to the mod, in send order.
    pub fn get_messages(&mut self) -> Vec<ImcMessage> {
        self.queue
            .queues
            .remove(self.mod_id)
            .map(Vec::from)
            .unwrap_or_default()
    }
}
