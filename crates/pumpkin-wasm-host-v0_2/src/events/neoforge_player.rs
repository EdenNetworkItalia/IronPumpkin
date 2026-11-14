//! The v0.2 conversions of the `NeoForge` 26.3 player events (`event.entity.player`) of
//! `pumpkin_core::plugin::player`, and of `PlayerEnchantItemEvent` and
//! `GrindstoneEventOnTakeItem`.
//!
//! A blocking handler writes back only the settable fields; the other fields keep the native
//! values, as `NeoForge` has no setter for them.

use std::sync::Arc;

use pumpkin_core::{
    entity::player::{ChatMode, ParticleStatus},
    net::PlayerConfig,
    plugin::{
        Context, EventPriority,
        enchantment::PlayerEnchantItemEvent,
        inventory::grindstone_take::GrindstoneEventOnTakeItem,
        player::{
            AnvilCraftEventPost, AnvilCraftEventPre, ArrowLooseEvent, ArrowNockEvent,
            ClientInformationUpdatedEvent, CriticalHitEvent, ItemEntityPickupEventPost,
            ItemEntityPickupEventPre, PlayerEventBreakSpeed, PlayerEventHarvestCheck,
            PlayerEventLoadFromFile, PlayerEventNameFormat, PlayerEventSaveToFile,
            PlayerEventStartTracking, PlayerEventStopTracking, PlayerEventTabListNameFormat,
            PlayerFlyableFallEvent, PlayerSwitchHotbarSlotEventPost, PlayerXpEventPickupXp,
            SweepAttackEvent, TradeWithVillagerEvent,
        },
    },
};
use pumpkin_protocol::java::client::play::MerchantOffer;
use pumpkin_wasm_host_common::state::PluginHostState;
use wasmtime::component::Resource;

use crate::{
    events::{
        ToFromWasmEvent, WasmPluginEventHandler,
        neoforge::{
            add, add_item_stack, discard, from_wasm_interaction_result, from_wasm_tri_state,
            take_item_stack, to_wasm_interaction_result, to_wasm_tri_state,
        },
        to_wasm_block_position, to_wasm_hand,
    },
    item_stack::to_wit_enchantment,
    pumpkin::plugin::{
        event::{Event, EventType},
        item_stack::EnchantmentValue,
        neoforge_events::{
            AnvilCraftEventPostData, AnvilCraftEventPreData, ArrowLooseEventData,
            ArrowNockEventData, ChatMode as WitChatMode, ClientInformation,
            ClientInformationUpdatedEventData, CriticalHitEventData, GrindstoneEventOnTakeItemData,
            ItemEntityPickupEventPostData, ItemEntityPickupEventPreData,
            MerchantOffer as WitMerchantOffer, ParticleStatus as WitParticleStatus,
            PlayerEnchantItemEventData, PlayerEventBreakSpeedData, PlayerEventHarvestCheckData,
            PlayerEventLoadFromFileData, PlayerEventNameFormatData, PlayerEventSaveToFileData,
            PlayerEventStartTrackingData, PlayerEventStopTrackingData,
            PlayerEventTabListNameFormatData, PlayerFlyableFallEventData,
            PlayerSwitchHotbarSlotEventPostData, PlayerXpEventPickupXpData, SweepAttackEventData,
            TradeWithVillagerEventData,
        },
        text::TextComponent as WitTextComponent,
    },
};

fn take_text(
    state: &mut PluginHostState,
    res: Resource<WitTextComponent>,
) -> Option<pumpkin_util::text::TextComponent> {
    state.take(res).ok()
}

fn to_wasm_offer(state: &mut PluginHostState, offer: &MerchantOffer) -> WitMerchantOffer {
    WitMerchantOffer {
        base_cost_a: add_item_stack(state, &offer.base_cost_a.0),
        cost_b: offer
            .cost_b
            .as_ref()
            .map(|cost| add_item_stack(state, &cost.0)),
        output: add_item_stack(state, &offer.output.0),
        reward_exp: offer.reward_exp,
        uses: offer.uses,
        max_uses: offer.max_uses,
        xp: offer.xp,
        special_price: offer.special_price,
        price_multiplier: offer.price_multiplier,
        demand: offer.demand,
    }
}

const fn to_wasm_chat_mode(mode: &ChatMode) -> WitChatMode {
    match mode {
        ChatMode::Enabled => WitChatMode::Enabled,
        ChatMode::CommandsOnly => WitChatMode::CommandsOnly,
        ChatMode::Hidden => WitChatMode::Hidden,
    }
}

const fn to_wasm_particle_status(status: ParticleStatus) -> WitParticleStatus {
    match status {
        ParticleStatus::All => WitParticleStatus::All,
        ParticleStatus::Decreased => WitParticleStatus::Decreased,
        ParticleStatus::Minimal => WitParticleStatus::Minimal,
    }
}

fn to_wasm_client_information(config: &PlayerConfig) -> ClientInformation {
    ClientInformation {
        locale: config.locale.clone(),
        view_distance: config.view_distance.get(),
        chat_mode: to_wasm_chat_mode(&config.chat_mode),
        chat_colors: config.chat_colors,
        skin_parts: config.skin_parts,
        main_hand: to_wasm_hand(config.main_hand),
        text_filtering: config.text_filtering,
        server_listing: config.server_listing,
        particle_status: to_wasm_particle_status(config.particle_status),
    }
}

macro_rules! player_event {
    ($($event:tt)*) => {
        neoforge_event!(cleanup_neoforge_player_event; $($event)*);
    };
}

player_event!(
    AnvilCraftEventPre,
    AnvilCraftEventPre,
    |this, state| AnvilCraftEventPreData {
        player: add(state, this.player.clone()),
        left: add_item_stack(state, &this.left),
        right: add_item_stack(state, &this.right),
        output: add_item_stack(state, &this.output),
        cancelled: this.cancelled,
    },
    |target, data| {
        target.cancelled = data.cancelled;
    }
);

player_event!(
    AnvilCraftEventPost,
    AnvilCraftEventPost,
    |this, state| AnvilCraftEventPostData {
        player: add(state, this.player.clone()),
        left: add_item_stack(state, &this.left),
        right: add_item_stack(state, &this.right),
        output: add_item_stack(state, &this.output),
    },
    |target, data| {}
);

player_event!(
    ArrowNockEvent,
    ArrowNockEvent,
    |this, state| ArrowNockEventData {
        player: add(state, this.player.clone()),
        bow: add_item_stack(state, &this.bow),
        hand: to_wasm_hand(this.hand),
        target_world: add(state, this.world.clone()),
        has_ammo: this.has_ammo,
        action: this.action.map(to_wasm_interaction_result),
        cancelled: this.cancelled,
    },
    |target, data| {
        target.action = data.action.map(from_wasm_interaction_result);
        target.cancelled = data.cancelled;
    }
);

player_event!(
    ArrowLooseEvent,
    ArrowLooseEvent,
    |this, state| ArrowLooseEventData {
        player: add(state, this.player.clone()),
        bow: add_item_stack(state, &this.bow),
        target_world: add(state, this.world.clone()),
        has_ammo: this.has_ammo,
        charge: this.charge,
        cancelled: this.cancelled,
    },
    |target, data| {
        target.charge = data.charge;
        target.cancelled = data.cancelled;
    }
);

player_event!(
    CriticalHitEvent,
    CriticalHitEvent,
    |this, state| CriticalHitEventData {
        player: add(state, this.player.clone()),
        target: add(state, this.target.clone()),
        vanilla_multiplier: this.vanilla_multiplier,
        vanilla_critical: this.vanilla_critical,
        damage_multiplier: this.damage_multiplier,
        critical_hit: this.critical_hit,
        disable_sweep: this.disable_sweep,
    },
    |target, data| {
        target.damage_multiplier = data.damage_multiplier;
        target.critical_hit = data.critical_hit;
        target.disable_sweep = data.disable_sweep;
    }
);

player_event!(
    SweepAttackEvent,
    SweepAttackEvent,
    |this, state| SweepAttackEventData {
        player: add(state, this.player.clone()),
        target: add(state, this.target.clone()),
        vanilla_sweep: this.vanilla_sweep,
        sweeping: this.sweeping,
        cancelled: this.cancelled,
    },
    |target, data| {
        target.sweeping = data.sweeping;
        target.cancelled = data.cancelled;
    }
);

player_event!(
    PlayerEventBreakSpeed,
    PlayerEventBreakSpeed,
    |this, state| PlayerEventBreakSpeedData {
        player: add(state, this.player.clone()),
        state_id: this.state.id.as_u16(),
        original_speed: this.original_speed,
        new_speed: this.new_speed,
        block_pos: this.pos.map(to_wasm_block_position),
        cancelled: this.cancelled,
    },
    |target, data| {
        target.new_speed = data.new_speed;
        target.cancelled = data.cancelled;
    }
);

player_event!(
    PlayerEventHarvestCheck,
    PlayerEventHarvestCheck,
    |this, state| PlayerEventHarvestCheckData {
        player: add(state, this.player.clone()),
        state_id: this.state.id.as_u16(),
        block_pos: to_wasm_block_position(this.pos),
        can_harvest: this.can_harvest,
    },
    |target, data| {
        target.can_harvest = data.can_harvest;
    }
);

impl ToFromWasmEvent for PlayerEventNameFormat {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerEventNameFormat(PlayerEventNameFormatData {
            player: add(state, self.player.clone()),
            username: add(state, self.username.clone()),
            display_name: add(state, self.display_name.clone()),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        let Event::PlayerEventNameFormat(data) = event else {
            cleanup_neoforge_player_event(&event, state);
            return;
        };
        discard(state, &data.player);
        discard(state, &data.username);
        if let Some(name) = take_text(state, data.display_name) {
            self.display_name = name;
        }
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct PlayerEventNameFormat from WASM")
    }
}

impl ToFromWasmEvent for PlayerEventTabListNameFormat {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerEventTabListNameFormat(PlayerEventTabListNameFormatData {
            player: add(state, self.player.clone()),
            display_name: self.display_name.clone().map(|name| add(state, name)),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        let Event::PlayerEventTabListNameFormat(data) = event else {
            cleanup_neoforge_player_event(&event, state);
            return;
        };
        discard(state, &data.player);
        // A handle that is no text keeps the name of the handlers before.
        self.display_name = data
            .display_name
            .and_then(|name| take_text(state, name).or_else(|| self.display_name.take()));
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct PlayerEventTabListNameFormat from WASM")
    }
}

player_event!(
    PlayerEventStartTracking,
    PlayerEventStartTracking,
    |this, state| PlayerEventStartTrackingData {
        player: add(state, this.player.clone()),
        target: add(state, this.target.clone()),
    },
    |target, data| {}
);

player_event!(
    PlayerEventStopTracking,
    PlayerEventStopTracking,
    |this, state| PlayerEventStopTrackingData {
        player: add(state, this.player.clone()),
        target: add(state, this.target.clone()),
    },
    |target, data| {}
);

player_event!(
    PlayerEventLoadFromFile,
    PlayerEventLoadFromFile,
    |this, state| PlayerEventLoadFromFileData {
        player: add(state, this.player.clone()),
        player_directory: this.player_directory.to_string_lossy().into_owned(),
        player_uuid: this.player_uuid.clone(),
    },
    |target, data| {}
);

player_event!(
    PlayerEventSaveToFile,
    PlayerEventSaveToFile,
    |this, state| PlayerEventSaveToFileData {
        player: add(state, this.player.clone()),
        player_directory: this.player_directory.to_string_lossy().into_owned(),
        player_uuid: this.player_uuid.clone(),
    },
    |target, data| {}
);

player_event!(
    PlayerFlyableFallEvent,
    PlayerFlyableFallEvent,
    |this, state| PlayerFlyableFallEventData {
        player: add(state, this.player.clone()),
        distance: this.distance,
        multiplier: this.multiplier,
    },
    |target, data| {}
);

player_event!(
    PlayerXpEventPickupXp,
    PlayerXpEventPickupXp,
    |this, state| PlayerXpEventPickupXpData {
        player: add(state, this.player.clone()),
        orb: add(state, this.orb.clone()),
        amount: this.amount,
        cancelled: this.cancelled,
    },
    |target, data| {
        target.cancelled = data.cancelled;
    }
);

player_event!(
    ItemEntityPickupEventPre,
    ItemEntityPickupEventPre,
    |this, state| ItemEntityPickupEventPreData {
        player: add(state, this.player.clone()),
        item_entity: add(state, this.item_entity.clone()),
        can_pickup: to_wasm_tri_state(this.can_pickup),
    },
    |target, data| {
        target.can_pickup = from_wasm_tri_state(data.can_pickup);
    }
);

player_event!(
    ItemEntityPickupEventPost,
    ItemEntityPickupEventPost,
    |this, state| ItemEntityPickupEventPostData {
        player: add(state, this.player.clone()),
        item_entity: add(state, this.item_entity.clone()),
        original_stack: add_item_stack(state, &this.original_stack),
        current_stack: add_item_stack(state, &this.current_stack),
    },
    |target, data| {}
);

player_event!(
    PlayerSwitchHotbarSlotEventPost,
    PlayerSwitchHotbarSlotEventPost,
    |this, state| PlayerSwitchHotbarSlotEventPostData {
        player: add(state, this.player.clone()),
        old_slot_index: this.old_slot_index,
        new_slot_index: this.new_slot_index,
    },
    |target, data| {}
);

player_event!(
    TradeWithVillagerEvent,
    TradeWithVillagerEvent,
    |this, state| TradeWithVillagerEventData {
        player: add(state, this.player.clone()),
        offer: to_wasm_offer(state, &this.offer),
        villager: add(state, this.villager.clone()),
    },
    |target, data| {}
);

player_event!(
    ClientInformationUpdatedEvent,
    ClientInformationUpdatedEvent,
    |this, state| ClientInformationUpdatedEventData {
        player: add(state, this.player.clone()),
        old_information: to_wasm_client_information(&this.old_information),
        updated_information: to_wasm_client_information(&this.updated_information),
    },
    |target, data| {}
);

player_event!(
    PlayerEnchantItemEvent,
    PlayerEnchantItemEvent,
    |this, state| PlayerEnchantItemEventData {
        player: add(state, this.player.clone()),
        enchanted_item: add_item_stack(state, &this.enchanted_item),
        enchantments: this
            .enchantments
            .iter()
            .map(|(enchantment, level)| EnchantmentValue {
                enchantment: to_wit_enchantment(enchantment),
                level: level.cast_unsigned(),
            })
            .collect(),
    },
    |target, data| {}
);

impl ToFromWasmEvent for GrindstoneEventOnTakeItem {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::GrindstoneEventOnTakeItem(GrindstoneEventOnTakeItemData {
            player: add(state, self.player.clone()),
            block_pos: to_wasm_block_position(self.pos),
            top: add_item_stack(state, &self.top),
            bottom: add_item_stack(state, &self.bottom),
            xp: self.xp,
            new_top: add_item_stack(state, &self.new_top),
            new_bottom: add_item_stack(state, &self.new_bottom),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        let Event::GrindstoneEventOnTakeItem(data) = event else {
            cleanup_neoforge_player_event(&event, state);
            return;
        };
        discard(state, &data.player);
        discard(state, &data.top);
        discard(state, &data.bottom);
        if let Some(stack) = take_item_stack(state, data.new_top) {
            self.new_top = stack;
        }
        if let Some(stack) = take_item_stack(state, data.new_bottom) {
            self.new_bottom = stack;
        }
        self.xp = data.xp;
        self.cancelled = data.cancelled;
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct GrindstoneEventOnTakeItem from WASM")
    }
}

/// Discards the resources of a returned event of this module.
#[expect(
    clippy::too_many_lines,
    reason = "one arm for each record of the module"
)]
pub(super) fn cleanup_neoforge_player_event(event: &Event, state: &mut PluginHostState) {
    match event {
        Event::AnvilCraftEventPre(AnvilCraftEventPreData {
            player,
            left,
            right,
            output,
            ..
        })
        | Event::AnvilCraftEventPost(AnvilCraftEventPostData {
            player,
            left,
            right,
            output,
        }) => {
            discard(state, player);
            discard(state, left);
            discard(state, right);
            discard(state, output);
        }
        Event::ArrowNockEvent(ArrowNockEventData {
            player,
            bow,
            target_world,
            ..
        })
        | Event::ArrowLooseEvent(ArrowLooseEventData {
            player,
            bow,
            target_world,
            ..
        }) => {
            discard(state, player);
            discard(state, bow);
            discard(state, target_world);
        }
        Event::CriticalHitEvent(CriticalHitEventData { player, target, .. })
        | Event::SweepAttackEvent(SweepAttackEventData { player, target, .. })
        | Event::PlayerEventStartTracking(PlayerEventStartTrackingData { player, target })
        | Event::PlayerEventStopTracking(PlayerEventStopTrackingData { player, target })
        | Event::PlayerXpEventPickupXp(PlayerXpEventPickupXpData {
            player,
            orb: target,
            ..
        })
        | Event::ItemEntityPickupEventPre(ItemEntityPickupEventPreData {
            player,
            item_entity: target,
            ..
        }) => {
            discard(state, player);
            discard(state, target);
        }
        Event::PlayerEventBreakSpeed(PlayerEventBreakSpeedData { player, .. })
        | Event::PlayerEventHarvestCheck(PlayerEventHarvestCheckData { player, .. })
        | Event::PlayerEventLoadFromFile(PlayerEventLoadFromFileData { player, .. })
        | Event::PlayerEventSaveToFile(PlayerEventSaveToFileData { player, .. })
        | Event::PlayerFlyableFallEvent(PlayerFlyableFallEventData { player, .. })
        | Event::PlayerSwitchHotbarSlotEventPost(PlayerSwitchHotbarSlotEventPostData {
            player,
            ..
        })
        | Event::ClientInformationUpdatedEvent(ClientInformationUpdatedEventData {
            player, ..
        }) => discard(state, player),
        Event::PlayerEventNameFormat(data) => {
            discard(state, &data.player);
            discard(state, &data.username);
            discard(state, &data.display_name);
        }
        Event::PlayerEventTabListNameFormat(data) => {
            discard(state, &data.player);
            if let Some(name) = &data.display_name {
                discard(state, name);
            }
        }
        Event::ItemEntityPickupEventPost(data) => {
            discard(state, &data.player);
            discard(state, &data.item_entity);
            discard(state, &data.original_stack);
            discard(state, &data.current_stack);
        }
        Event::TradeWithVillagerEvent(data) => {
            discard(state, &data.player);
            discard(state, &data.villager);
            discard(state, &data.offer.base_cost_a);
            if let Some(cost) = &data.offer.cost_b {
                discard(state, cost);
            }
            discard(state, &data.offer.output);
        }
        Event::PlayerEnchantItemEvent(data) => {
            discard(state, &data.player);
            discard(state, &data.enchanted_item);
        }
        Event::GrindstoneEventOnTakeItem(data) => {
            discard(state, &data.player);
            discard(state, &data.top);
            discard(state, &data.bottom);
            discard(state, &data.new_top);
            discard(state, &data.new_bottom);
        }
        _ => {}
    }
}

/// Registers a Wasm handler for an event of this module.
pub fn register_neoforge_player_event(
    ctx: &Context,
    handler: &Arc<WasmPluginEventHandler>,
    priority: EventPriority,
    blocking: bool,
    event_type: EventType,
) {
    fn on<E: pumpkin_core::plugin::Payload + ToFromWasmEvent + Clone + 'static>(
        ctx: &Context,
        handler: &Arc<WasmPluginEventHandler>,
        priority: EventPriority,
        blocking: bool,
    ) {
        ctx.register_event::<E, _>(Arc::clone(handler), priority, blocking);
    }

    match event_type {
        EventType::AnvilCraftEventPre => on::<AnvilCraftEventPre>(ctx, handler, priority, blocking),
        EventType::AnvilCraftEventPost => {
            on::<AnvilCraftEventPost>(ctx, handler, priority, blocking);
        }
        EventType::ArrowNockEvent => on::<ArrowNockEvent>(ctx, handler, priority, blocking),
        EventType::ArrowLooseEvent => on::<ArrowLooseEvent>(ctx, handler, priority, blocking),
        EventType::CriticalHitEvent => on::<CriticalHitEvent>(ctx, handler, priority, blocking),
        EventType::SweepAttackEvent => on::<SweepAttackEvent>(ctx, handler, priority, blocking),
        EventType::PlayerEventBreakSpeed => {
            on::<PlayerEventBreakSpeed>(ctx, handler, priority, blocking);
        }
        EventType::PlayerEventHarvestCheck => {
            on::<PlayerEventHarvestCheck>(ctx, handler, priority, blocking);
        }
        EventType::PlayerEventNameFormat => {
            on::<PlayerEventNameFormat>(ctx, handler, priority, blocking);
        }
        EventType::PlayerEventTabListNameFormat => {
            on::<PlayerEventTabListNameFormat>(ctx, handler, priority, blocking);
        }
        EventType::PlayerEventStartTracking => {
            on::<PlayerEventStartTracking>(ctx, handler, priority, blocking);
        }
        EventType::PlayerEventStopTracking => {
            on::<PlayerEventStopTracking>(ctx, handler, priority, blocking);
        }
        EventType::PlayerEventLoadFromFile => {
            on::<PlayerEventLoadFromFile>(ctx, handler, priority, blocking);
        }
        EventType::PlayerEventSaveToFile => {
            on::<PlayerEventSaveToFile>(ctx, handler, priority, blocking);
        }
        EventType::PlayerFlyableFallEvent => {
            on::<PlayerFlyableFallEvent>(ctx, handler, priority, blocking);
        }
        EventType::PlayerXpEventPickupXp => {
            on::<PlayerXpEventPickupXp>(ctx, handler, priority, blocking);
        }
        EventType::ItemEntityPickupEventPre => {
            on::<ItemEntityPickupEventPre>(ctx, handler, priority, blocking);
        }
        EventType::ItemEntityPickupEventPost => {
            on::<ItemEntityPickupEventPost>(ctx, handler, priority, blocking);
        }
        EventType::PlayerSwitchHotbarSlotEventPost => {
            on::<PlayerSwitchHotbarSlotEventPost>(ctx, handler, priority, blocking);
        }
        EventType::TradeWithVillagerEvent => {
            on::<TradeWithVillagerEvent>(ctx, handler, priority, blocking);
        }
        EventType::ClientInformationUpdatedEvent => {
            on::<ClientInformationUpdatedEvent>(ctx, handler, priority, blocking);
        }
        EventType::PlayerEnchantItemEvent => {
            on::<PlayerEnchantItemEvent>(ctx, handler, priority, blocking);
        }
        EventType::GrindstoneEventOnTakeItem => {
            on::<GrindstoneEventOnTakeItem>(ctx, handler, priority, blocking);
        }
        _ => tracing::error!(
            "non-player NeoForge event should not be routed to register_neoforge_player_event"
        ),
    }
}
