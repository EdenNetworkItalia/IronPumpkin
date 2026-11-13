use super::{
    BlockPos, CBlockUpdate, Hand, JavaClient, Player, PlayerInteractEvent, VarInt, World,
    interact_events,
};

impl JavaClient {
    /// Ends a right click on a block that a handler of `RightClickBlock` cancelled, as
    /// `handleUseItemOn` ends a use with the cancellation result: the client gets the block back,
    /// and a successful result swings the arm.
    pub(super) fn cancel_right_click_block(
        player: &Player,
        hand: Hand,
        world: &World,
        position: BlockPos,
        event: &PlayerInteractEvent,
    ) {
        let state_id = world.get_block_state_id(&position);
        player.try_send_client_packet(&CBlockUpdate::new(
            position,
            VarInt(i32::from(state_id.as_u16())),
        ));
        interact_events::swing_for(player, hand, event.cancellation_result);
    }
}
