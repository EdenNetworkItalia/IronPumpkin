use super::{Arc, ClientPlatform, Player, Relaxed, TrackedEntity};

impl TrackedEntity {
    /// Adds `player` to the viewers without pairing it. Returns whether it was not a viewer.
    pub fn mark_seen_by(&self, player: &Arc<Player>) -> bool {
        let inserted = self
            .seen_by
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(player.gameprofile.id, player.clone())
            .is_none();
        if inserted && matches!(player.client.as_ref(), ClientPlatform::Bedrock(_)) {
            self.bedrock_watchers.fetch_add(1, Relaxed);
        }
        inserted
    }
}
