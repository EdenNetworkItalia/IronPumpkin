use super::{Attributes, ItemStack, Player};

impl Player {
    /// Whether the cooldown group of `stack` is on cooldown (`ItemCooldowns.isOnCooldown`): the
    /// group of its `use_cooldown` component, else its item.
    pub fn is_item_on_cooldown(&self, stack: &ItemStack) -> bool {
        let group = stack
            .get_use_cooldown()
            .and_then(|cooldown| cooldown.cooldown_group.clone())
            .unwrap_or_else(|| stack.item.registry_key.to_string());
        self.is_on_cooldown(&group)
    }

    /// Vanilla `LivingEntity.releaseUsingItem`: the client released the item in use. The item
    /// reacts, such as a bow that shoots, unless a handler cancels `LivingEntityUseItemEventStop`,
    /// and the use stops.
    pub fn release_using_item(&self) {
        let item_in_use = self
            .living_entity
            .item_in_use
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(stack) = item_in_use
            && let Some(server) = self.world().server.upgrade()
            && crate::plugin::living_events::use_item_stop(self, &stack)
        {
            server.item_registry.on_stopped_using(&stack, self);
        }
        self.living_entity.clear_active_hand();
    }

    /// Whether the player may fly: the ability, or the `neoforge:creative_flight` attribute.
    ///
    /// `NeoForge`: `IPlayerExtension#mayFly`.
    pub fn may_fly(&self) -> bool {
        self.abilities
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .allow_flying
            || self.has_creative_flight()
    }

    /// A client asks to start or stop flying. The player flies only if it [may fly](Self::may_fly).
    pub fn request_flying(&self, flying: bool) {
        let may_fly = self.may_fly();
        self.abilities
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .flying = flying && may_fly;
    }

    /// `NeoForge` `ServerPlayer.doTick`: a player who may no longer fly stops flying.
    pub(super) fn stop_flying_unless_may_fly(&self) {
        let creative_flight = self.has_creative_flight();
        let stopped = {
            let mut abilities = self
                .abilities
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let stop = abilities.flying && !abilities.allow_flying && !creative_flight;
            if stop {
                abilities.flying = false;
            }
            stop
        };
        if stopped {
            self.send_abilities_update();
        }
    }

    pub(super) fn has_creative_flight(&self) -> bool {
        self.living_entity
            .get_attribute_value(&Attributes::NEOFORGE_CREATIVE_FLIGHT)
            > 0.0
    }

    /// What the client negotiated in the configuration phase: the `NeoForge` connection type,
    /// channels and content ids. The default for any other Java client and every Bedrock client.
    #[must_use]
    pub fn negotiated_state(&self) -> &crate::net::java::neoforge::NegotiatedState {
        self.client.negotiated_state()
    }
}
