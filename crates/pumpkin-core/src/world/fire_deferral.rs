//! Holds back the sounds and game events of a scope until the scope ends.
//!
//! A sound and a game event run mod handlers. A handler can reach the state that the fire site
//! has locked. A scope that holds a lock and runs code that fires, such as the brain tick of a
//! mob, starts a [`FireDeferral`] before it takes the lock. [`World::play_seeded_sound`],
//! [`World::play_seeded_sound_at_entity`] and [`World::emit_game_event_with_context`] then queue
//! their fire. The outermost deferral fires the queue, in order, when it drops, after the locks of
//! its scope.
//!
//! [`World::play_seeded_sound`]: super::World::play_seeded_sound
//! [`World::play_seeded_sound_at_entity`]: super::World::play_seeded_sound_at_entity
//! [`World::emit_game_event_with_context`]: super::World::emit_game_event_with_context

use std::cell::RefCell;

type Fire = Box<dyn FnOnce()>;

thread_local! {
    static QUEUE: RefCell<Option<Vec<Fire>>> = const { RefCell::new(None) };
}

/// Queues the sounds and game events of this thread while it lives.
#[must_use]
pub struct FireDeferral {
    outermost: bool,
}

impl FireDeferral {
    pub fn start() -> Self {
        let outermost = QUEUE.with_borrow_mut(|queue| {
            if queue.is_some() {
                false
            } else {
                *queue = Some(Vec::new());
                true
            }
        });
        Self { outermost }
    }
}

impl Drop for FireDeferral {
    fn drop(&mut self) {
        if !self.outermost {
            return;
        }
        let Some(queue) = QUEUE.with_borrow_mut(Option::take) else {
            return;
        };
        if std::thread::panicking() {
            return;
        }
        for fire in queue {
            fire();
        }
    }
}

/// Whether a [`FireDeferral`] is active on this thread.
pub(super) fn is_active() -> bool {
    QUEUE.with_borrow(Option::is_some)
}

/// Queues `fire` after the active deferral and returns `None`. Gives `fire` back when no
/// deferral is active.
pub(super) fn defer<F: FnOnce() + 'static>(fire: F) -> Option<F> {
    QUEUE.with_borrow_mut(|queue| match queue {
        Some(queue) => {
            queue.push(Box::new(fire));
            None
        }
        None => Some(fire),
    })
}
