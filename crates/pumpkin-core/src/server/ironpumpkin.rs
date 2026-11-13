use super::{
    DisconnectReason, Duration, MAX_TICKS_BEFORE_LOGIN, Server, TextComponent,
    duplicate_login_disconnect_message, translation,
};

impl Server {
    /// Vanilla `PlayerList.disconnectAllPlayersWithProfile` for a new login of the profile: kicks
    /// the online player of the profile and waits until its session is released. Returns the
    /// reason to refuse the new login when the wait reaches `MAX_TICKS_BEFORE_LOGIN`.
    pub async fn disconnect_all_players_with_profile(
        &self,
        id: uuid::Uuid,
    ) -> Option<TextComponent> {
        let max_wait = Duration::from_nanos(
            self.tick_rate_manager
                .nanoseconds_per_tick()
                .unsigned_abs()
                .saturating_mul(MAX_TICKS_BEFORE_LOGIN),
        );
        let message = duplicate_login_disconnect_message();
        let released = self
            .online_profiles
            .replace(id, max_wait, |player| {
                player.kick(DisconnectReason::LoggedInOtherLocation, &message);
            })
            .await;
        (!released).then(|| {
            TextComponent::translate_cross(
                translation::java::MULTIPLAYER_DISCONNECT_SLOW_LOGIN,
                translation::bedrock::DISCONNECTIONSCREEN_TIMEOUT,
                [],
            )
        })
    }
}
