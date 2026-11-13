use super::{
    Arc, CBlockUpdate, JavaClient, LeftClickBlockAction, Player, SPlayerAction, TriState, VarInt,
    interact_events,
};

impl JavaClient {
    /// Fires `PlayerInteractEvent.LeftClickBlock` for a dig action, first, as
    /// `ServerPlayerGameMode.handleBlockBreakAction` does in `NeoForge` 26.3 (and Bukkit on the
    /// dig start). Returns `None` when a handler cancelled it: the action does not run and the
    /// client gets the block back. Otherwise returns `use_block`.
    pub(super) fn fire_left_click_block(
        player: &Arc<Player>,
        player_action: &SPlayerAction,
        action: LeftClickBlockAction,
    ) -> Option<TriState> {
        let position = player_action.position;
        let use_block =
            interact_events::left_click_block(player, position, player_action.face, action);
        if use_block.is_none() {
            let state_id = player.world().get_block_state_id(&position);
            player.try_send_client_packet(&CBlockUpdate::new(
                position,
                VarInt(i32::from(state_id.as_u16())),
            ));
        }
        use_block
    }
}
