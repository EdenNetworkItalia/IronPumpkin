use super::{
    AnvilInputs, AnvilScreenHandler, AnvilTake, AnvilUpdate, InventoryPlayer, ItemStack, Ordering,
    PoisonError, ScreenHandler, finish_take,
};

impl AnvilScreenHandler {
    /// Computes the vanilla result with [`Self::create_result`] and marks the anvil update event
    /// as pending, like the `createResult` wrapper of `NeoForge`'s `AnvilMenu`. The player fires
    /// it with [`Self::take_update_event`] after it releases the screen-handler lock.
    ///
    /// While a take waits for the anvil craft events, the inputs are still in place: the result
    /// stays empty, so a quick move takes the result once.
    pub fn update_result(&mut self, player: &dyn InventoryPlayer) {
        if self
            .craft_take
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
        {
            self.inventory.set_stack(2, ItemStack::EMPTY.clone());
            self.set_repair_cost(0);
            return;
        }
        self.create_result(player.has_infinite_materials());
        self.update_event_pending = true;
    }

    /// Whether an anvil update event is pending, and clears it.
    pub const fn take_update_event(&mut self) -> bool {
        std::mem::replace(&mut self.update_event_pending, false)
    }

    /// The take that waits for the anvil craft events, and clears it.
    pub fn take_craft_event(&self) -> Option<AnvilTake> {
        self.craft_take
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }

    /// Ends `take` as vanilla does after an `AnvilCraftEvent.Pre` without a cancel: pays the
    /// levels, uses the inputs and damages the anvil. Returns `false` and changes nothing when an
    /// input changed while the event fired.
    pub fn apply_craft_take(&mut self, take: &AnvilTake, player: &dyn InventoryPlayer) -> bool {
        if !self.inventory.get_stack(0).are_equal(&take.left)
            || !self.inventory.get_stack(1).are_equal(&take.right)
        {
            return false;
        }
        self.repair_cost.store(0, Ordering::Relaxed);
        finish_take(
            self.inventory.as_ref(),
            player,
            take.cost,
            take.repair_item_count_cost,
            take.only_renaming,
        );
        true
    }

    /// The current inputs and result for the anvil update event. `None` when the left input is
    /// empty, as `NeoForge` fires the event only for a left input.
    #[must_use]
    pub fn update_inputs(&self) -> Option<AnvilInputs> {
        let left = self.inventory.get_stack(0);
        if left.is_empty() {
            return None;
        }
        Some(AnvilInputs {
            left,
            right: self.inventory.get_stack(1),
            name: self.item_name.clone(),
            output: self.inventory.get_stack(2),
            xp_cost: self.get_cost(),
            material_cost: self.repair_item_count_cost.load(Ordering::Relaxed),
        })
    }

    /// Applies what the handlers of the anvil update event decided for `inputs`
    /// (`CommonHooks.onAnvilUpdate`). Returns `false` and changes nothing when the inputs or the
    /// result changed while the event fired.
    pub fn apply_update_event(&mut self, inputs: &AnvilInputs, update: AnvilUpdate) -> bool {
        if !self.inventory.get_stack(0).are_equal(&inputs.left)
            || !self.inventory.get_stack(1).are_equal(&inputs.right)
            || !self.inventory.get_stack(2).are_equal(&inputs.output)
            || self.item_name != inputs.name
        {
            return false;
        }
        let (output, xp_cost, material_cost) = match update {
            AnvilUpdate::Cancelled => (ItemStack::EMPTY.clone(), 0, 0),
            AnvilUpdate::Result {
                output,
                xp_cost,
                material_cost,
            } => (output, xp_cost.max(0), material_cost),
        };
        self.inventory.set_stack(2, output);
        self.set_repair_cost(xp_cost);
        self.repair_item_count_cost
            .store(material_cost, Ordering::Relaxed);
        self.send_content_updates();
        true
    }
}
