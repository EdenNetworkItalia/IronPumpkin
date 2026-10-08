use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::entity::player::Player;

/// The profiles that are online, like vanilla `PlayerList.playersByUUID`.
///
/// A profile is online from the end of its configuration until its disconnect has saved the
/// player, so a new login of the same profile never loads data that is still being saved.
pub struct OnlineProfiles<P = Player> {
    profiles: Mutex<HashMap<Uuid, OnlineState<P>>>,
}

struct OnlineState<P> {
    released: CancellationToken,
    replaced: bool,
    player: Weak<P>,
}

impl<P> Default for OnlineProfiles<P> {
    fn default() -> Self {
        Self {
            profiles: Mutex::default(),
        }
    }
}

impl<P> OnlineProfiles<P> {
    /// Marks the profile as online. `None` when it already is.
    pub fn claim(self: &Arc<Self>, id: Uuid) -> Option<OnlineProfile<P>> {
        let mut profiles = self.profiles.lock().unwrap_or_else(PoisonError::into_inner);
        if profiles.contains_key(&id) {
            return None;
        }
        let released = CancellationToken::new();
        profiles.insert(
            id,
            OnlineState {
                released: released.clone(),
                replaced: false,
                player: Weak::new(),
            },
        );
        Some(OnlineProfile {
            profiles: self.clone(),
            id,
            released,
        })
    }

    /// Vanilla `disconnectAllPlayersWithProfile` and the login state `WAITING_FOR_DUPE_DISCONNECT`:
    /// marks the online session of the profile as replaced, passes its player to `kick` and waits
    /// until the session is released. `false` when that takes longer than `max_wait`.
    pub async fn replace(&self, id: Uuid, max_wait: Duration, kick: impl FnOnce(Arc<P>)) -> bool {
        let (released, player) = {
            let mut profiles = self.profiles.lock().unwrap_or_else(PoisonError::into_inner);
            let Some(state) = profiles.get_mut(&id) else {
                return true;
            };
            state.replaced = true;
            (state.released.clone(), state.player.upgrade())
        };
        // A session without a player yet kicks its player itself in `OnlineProfile::attach`.
        if let Some(player) = player {
            kick(player);
        }
        tokio::time::timeout(max_wait, released.cancelled())
            .await
            .is_ok()
    }
}

/// An online profile. Dropping it releases the profile.
pub struct OnlineProfile<P = Player> {
    profiles: Arc<OnlineProfiles<P>>,
    id: Uuid,
    released: CancellationToken,
}

impl<P> OnlineProfile<P> {
    /// Records the player of this session. `false` when a new login already replaced the profile:
    /// that login found no player to kick, so the caller kicks this one.
    pub fn attach(&self, player: &Arc<P>) -> bool {
        let mut profiles = self
            .profiles
            .profiles
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let Some(state) = profiles.get_mut(&self.id) else {
            return false;
        };
        state.player = Arc::downgrade(player);
        !state.replaced
    }
}

impl<P> Drop for OnlineProfile<P> {
    fn drop(&mut self) {
        self.profiles
            .profiles
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.id);
        self.released.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX_WAIT: Duration = Duration::from_secs(30);

    #[test]
    fn profile_is_online_until_released() {
        let profiles = Arc::new(OnlineProfiles::<()>::default());
        let id = Uuid::new_v4();

        let first = profiles.claim(id);
        assert!(first.is_some());
        assert!(profiles.claim(id).is_none());

        drop(first);
        assert!(profiles.claim(id).is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn replace_kicks_the_player_and_waits_for_the_release() {
        let profiles = Arc::new(OnlineProfiles::<()>::default());
        let id = Uuid::new_v4();
        let mut kicked_offline = false;
        assert!(
            profiles
                .replace(id, MAX_WAIT, |_| kicked_offline = true)
                .await
        );
        assert!(!kicked_offline);

        let player = Arc::new(());
        let online = profiles.claim(id);
        assert!(online.as_ref().is_some_and(|online| online.attach(&player)));
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            drop(online);
        });
        let mut kicked = None;
        assert!(profiles.replace(id, MAX_WAIT, |p| kicked = Some(p)).await);
        assert!(kicked.is_some_and(|p| Arc::ptr_eq(&p, &player)));
        assert!(profiles.claim(id).is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn replace_before_attach_lets_the_session_kick_itself() {
        let profiles = Arc::new(OnlineProfiles::<()>::default());
        let id = Uuid::new_v4();
        let online = profiles.claim(id);

        let mut kicked = false;
        assert!(!profiles.replace(id, MAX_WAIT, |_| kicked = true).await);
        assert!(!kicked);
        assert!(online.is_some_and(|online| !online.attach(&Arc::new(()))));
    }
}
