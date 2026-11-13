use super::{
    GrindstoneInputs, GrindstonePlace, GrindstoneScreenHandler, GrindstoneTake, ItemStack,
    Ordering, PoisonError, ScreenHandler,
};

impl GrindstoneScreenHandler {
    /// Computes the vanilla result and experience and marks the grindstone place event as
    /// pending, like the `createResult` of `NeoForge`'s `GrindstoneMenu`. The player fires it with
    /// [`Self::take_update_event`] after it releases the screen-handler lock.
    pub fn update_result(&mut self) {
        self.xp.store(-1, Ordering::Relaxed);
        self.create_result();
        self.update_event_pending = true;
    }

    /// Whether a grindstone place event is pending, and clears it.
    pub const fn take_update_event(&mut self) -> bool {
        std::mem::replace(&mut self.update_event_pending, false)
    }

    /// The current inputs and result for the grindstone place event.
    #[must_use]
    pub fn update_inputs(&self) -> GrindstoneInputs {
        GrindstoneInputs {
            top: self.inventory.get_stack(Self::INPUT_SLOT),
            bottom: self.inventory.get_stack(Self::ADDITIONAL_SLOT),
            output: self.inventory.get_stack(Self::RESULT_SLOT),
        }
    }

    /// Applies what the handlers of the grindstone place event decided for `inputs`. Returns
    /// `false` and changes nothing when the grindstone closed or the inputs or the result changed
    /// while the event fired.
    pub fn apply_update_event(
        &mut self,
        inputs: &GrindstoneInputs,
        place: GrindstonePlace,
    ) -> bool {
        if self.closed
            || !self
                .inventory
                .get_stack(Self::INPUT_SLOT)
                .are_equal(&inputs.top)
            || !self
                .inventory
                .get_stack(Self::ADDITIONAL_SLOT)
                .are_equal(&inputs.bottom)
            || !self
                .inventory
                .get_stack(Self::RESULT_SLOT)
                .are_equal(&inputs.output)
        {
            return false;
        }
        let (output, xp) = match place {
            GrindstonePlace::Cancelled => (ItemStack::EMPTY.clone(), -1),
            GrindstonePlace::Output { output, xp } => (output, xp),
        };
        self.xp.store(xp, Ordering::Relaxed);
        self.inventory.set_stack(Self::RESULT_SLOT, output);
        self.send_content_updates();
        true
    }

    /// The take that waits for the grindstone take event, and clears it.
    pub fn take_take_event(&self) -> Option<GrindstoneTake> {
        self.take
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }

    /// Puts `top` and `bottom` into the empty input slots after the grindstone take event, and
    /// updates the result when `update_result` is set (a cancelled take restores the inputs and
    /// leaves the result empty, as `NeoForge` does). Returns `false` and changes nothing when the
    /// grindstone closed or an input slot is not empty: the caller then gives the stacks to the
    /// player.
    pub fn apply_take_event(
        &mut self,
        top: ItemStack,
        bottom: ItemStack,
        update_result: bool,
    ) -> bool {
        if self.closed
            || !self.inventory.get_stack(Self::INPUT_SLOT).is_empty()
            || !self.inventory.get_stack(Self::ADDITIONAL_SLOT).is_empty()
        {
            return false;
        }
        if top.is_empty() && bottom.is_empty() {
            return true;
        }
        self.inventory.set_stack(Self::INPUT_SLOT, top);
        self.inventory.set_stack(Self::ADDITIONAL_SLOT, bottom);
        if update_result {
            self.update_result();
        }
        self.send_content_updates();
        true
    }
}
