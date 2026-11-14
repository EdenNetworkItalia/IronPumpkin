//! The player, container and level tick events of `NeoForge` 26.3 that only native mods saw
//! reach v0.2 plugins (spec `neoforge-event-bus`, requirement "Wasm plugins see the same events",
//! #136): a v0.2 handler receives each event with its `NeoForge` fields, writes back the settable
//! fields and cancels it, and the host keeps the fields that `NeoForge` cannot set. The guest
//! helper checks that each round trip releases its resources.
#![expect(
    clippy::panic,
    clippy::expect_used,
    reason = "a test fails on the first unexpected record"
)]

#[path = "../../pumpkin-core/tests/support/mod.rs"]
mod support;

mod guest;

use std::{borrow::Cow, path::PathBuf, sync::Arc};

use guest::{drop_resource, fire, is_shared_world, plugin, resource};
use pumpkin_core::{
    entity::{
        EntityBase,
        player::{ChatMode, ParticleStatus},
        r#type::from_type,
    },
    net::PlayerConfig,
    plugin::{
        api::events::results::{InteractionResult, TriState},
        enchantment::PlayerEnchantItemEvent,
        inventory::{
            grindstone_take::GrindstoneEventOnTakeItem, potion_brew_post::PotionBrewEventPost,
        },
        player::{
            AnvilCraftEventPost, AnvilCraftEventPre, ArrowLooseEvent, ArrowNockEvent,
            ClientInformationUpdatedEvent, CriticalHitEvent, ItemEntityPickupEventPost,
            ItemEntityPickupEventPre, PlayerEventBreakSpeed, PlayerEventHarvestCheck,
            PlayerEventLoadFromFile, PlayerEventNameFormat, PlayerEventSaveToFile,
            PlayerEventStartTracking, PlayerEventStopTracking, PlayerEventTabListNameFormat,
            PlayerFlyableFallEvent, PlayerSwitchHotbarSlotEventPost, PlayerXpEventPickupXp,
            SweepAttackEvent, TradeWithVillagerEvent,
        },
        world::{LevelTickEventPost, LevelTickEventPre},
    },
};
use pumpkin_data::{Block, Enchantment, entity::EntityType, item::Item, item_stack::ItemStack};
use pumpkin_protocol::{
    codec::item_stack_seralizer::ItemStackSerializer, java::client::play::MerchantOffer,
};
use pumpkin_util::{
    Hand,
    math::{position::BlockPos, vector3::Vector3},
    text::TextComponent,
};
use pumpkin_wasm_host_v0_2::pumpkin::plugin::{
    common::Hand as WitHand,
    enchantments::Enchantment as WitEnchantment,
    event::Event,
    item_stack::ItemStack as WitItemStack,
    neoforge_events::{
        ChatMode as WitChatMode, InteractionResult as WitInteractionResult,
        ParticleStatus as WitParticleStatus, TriState as WitTriState,
    },
};
use wasmtime::component::Resource;

fn mob(entity_type: &'static EntityType) -> Arc<dyn EntityBase> {
    from_type(
        entity_type,
        Vector3::new(0.5, 70.0, 0.5),
        &support::world(),
        uuid::Uuid::new_v4(),
    )
}

fn id(entity: &Arc<dyn EntityBase>) -> i32 {
    entity.get_entity().entity_id
}

fn item_id(
    state: &pumpkin_wasm_host_common::state::PluginHostState,
    res: &Resource<WitItemStack>,
) -> u16 {
    resource(state, res)
        .try_lock()
        .map(|item| item.item.id)
        .expect("unlocked stack")
}

/// Drops the guest handle `res` and puts a new stack of `item` in its place.
fn replace_stack(
    state: &mut pumpkin_wasm_host_common::state::PluginHostState,
    res: &mut Resource<WitItemStack>,
    item: &'static Item,
) {
    drop_resource(state, res);
    *res = state
        .add(Arc::new(tokio::sync::Mutex::new(ItemStack::new(1, item))))
        .expect("new stack");
}

fn stack(item: &'static Item) -> ItemStack {
    ItemStack::new(1, item)
}

#[test]
fn a_plugin_cancels_an_anvil_take_and_reads_the_post() {
    let player = support::player("WasmAnvil");
    let pre = plugin::<AnvilCraftEventPre, _>(|event, state| {
        let Event::AnvilCraftEventPre(data) = event else {
            panic!("expected anvil craft pre");
        };
        assert_eq!(item_id(state, &data.left), Item::IRON_SWORD.id);
        assert_eq!(item_id(state, &data.right), Item::IRON_INGOT.id);
        assert_eq!(item_id(state, &data.output), Item::IRON_SWORD.id);
        assert!(!data.cancelled);
        data.cancelled = true;
    });
    let event = fire(
        &pre,
        AnvilCraftEventPre::new(
            player.clone(),
            stack(&Item::IRON_SWORD),
            stack(&Item::IRON_INGOT),
            stack(&Item::IRON_SWORD),
        ),
    );
    assert!(event.cancelled);

    let post = plugin::<AnvilCraftEventPost, _>(|event, state| {
        let Event::AnvilCraftEventPost(data) = event else {
            panic!("expected anvil craft post");
        };
        assert_eq!(item_id(state, &data.output), Item::IRON_SWORD.id);
    });
    fire(
        &post,
        AnvilCraftEventPost::new(
            player,
            stack(&Item::IRON_SWORD),
            stack(&Item::IRON_INGOT),
            stack(&Item::IRON_SWORD),
        ),
    );
}

#[test]
fn a_plugin_sets_the_action_of_a_nock_and_the_charge_of_a_loose() {
    let player = support::player("WasmArrow");
    let nock = plugin::<ArrowNockEvent, _>(|event, state| {
        let Event::ArrowNockEvent(data) = event else {
            panic!("expected arrow nock");
        };
        assert_eq!(item_id(state, &data.bow), Item::BOW.id);
        assert_eq!(data.hand, WitHand::Left);
        assert!(is_shared_world(state, &data.target_world));
        assert!(data.has_ammo);
        assert!(data.action.is_none());
        data.has_ammo = false;
        data.action = Some(WitInteractionResult::Fail);
        data.cancelled = true;
    });
    let event = fire(
        &nock,
        ArrowNockEvent::new(
            player.clone(),
            stack(&Item::BOW),
            Hand::Left,
            support::world(),
            true,
        ),
    );
    assert_eq!(event.action, Some(InteractionResult::Fail));
    assert!(event.has_ammo);
    assert!(event.cancelled);

    let loose = plugin::<ArrowLooseEvent, _>(|event, _state| {
        let Event::ArrowLooseEvent(data) = event else {
            panic!("expected arrow loose");
        };
        assert_eq!(data.charge, 20);
        assert!(!data.has_ammo);
        data.charge = 3;
        data.cancelled = true;
    });
    let event = fire(
        &loose,
        ArrowLooseEvent::new(player, stack(&Item::BOW), support::world(), 20, false),
    );
    assert_eq!(event.charge, 3);
    assert!(event.cancelled);
}

#[test]
fn a_plugin_changes_a_critical_hit_and_stops_a_sweep() {
    let player = support::player("WasmAttack");
    let zombie = mob(&EntityType::ZOMBIE);
    let zombie_id = id(&zombie);
    let critical = plugin::<CriticalHitEvent, _>(move |event, state| {
        let Event::CriticalHitEvent(data) = event else {
            panic!("expected critical hit");
        };
        assert_eq!(
            resource(state, &data.target).get_entity().entity_id,
            zombie_id
        );
        assert_eq!(
            (data.vanilla_multiplier, data.damage_multiplier),
            (1.5, 1.5)
        );
        assert!(data.vanilla_critical && data.critical_hit && data.disable_sweep);
        data.vanilla_multiplier = 9.0;
        data.vanilla_critical = false;
        data.damage_multiplier = 2.5;
        data.critical_hit = false;
        data.disable_sweep = false;
    });
    let event = fire(
        &critical,
        CriticalHitEvent::new(player.clone(), zombie.clone(), 1.5, true),
    );
    assert_eq!(
        (event.vanilla_multiplier, event.damage_multiplier),
        (1.5, 2.5)
    );
    assert!(event.vanilla_critical);
    assert!(!event.critical_hit && !event.disable_sweep);

    let sweep = plugin::<SweepAttackEvent, _>(|event, _state| {
        let Event::SweepAttackEvent(data) = event else {
            panic!("expected sweep attack");
        };
        assert!(data.vanilla_sweep && data.sweeping);
        data.vanilla_sweep = false;
        data.sweeping = false;
        data.cancelled = true;
    });
    let event = fire(&sweep, SweepAttackEvent::new(player, zombie, true));
    assert!(event.vanilla_sweep);
    assert!(!event.sweeping && event.cancelled);
}

#[test]
fn a_plugin_slows_the_mining_and_refuses_the_harvest_of_a_block() {
    let player = support::player("WasmMining");
    let stone = Block::STONE.default_state;
    let speed = plugin::<PlayerEventBreakSpeed, _>(move |event, _state| {
        let Event::PlayerEventBreakSpeed(data) = event else {
            panic!("expected break speed");
        };
        assert_eq!(data.state_id, stone.id.as_u16());
        assert_eq!((data.original_speed, data.new_speed), (4.0, 4.0));
        let pos = data.block_pos.expect("position");
        assert_eq!((pos.x, pos.y, pos.z), (1, 2, 3));
        data.original_speed = 0.0;
        data.new_speed = 0.5;
        data.cancelled = true;
    });
    let event = fire(
        &speed,
        PlayerEventBreakSpeed::new(player.clone(), stone, 4.0, Some(BlockPos::new(1, 2, 3))),
    );
    assert_eq!((event.original_speed, event.new_speed), (4.0, 0.5));
    assert!(event.cancelled);

    let harvest = plugin::<PlayerEventHarvestCheck, _>(|event, _state| {
        let Event::PlayerEventHarvestCheck(data) = event else {
            panic!("expected harvest check");
        };
        assert!(data.can_harvest);
        data.can_harvest = false;
    });
    let event = fire(
        &harvest,
        PlayerEventHarvestCheck::new(player, stone, BlockPos::new(1, 2, 3), true),
    );
    assert!(!event.can_harvest);
}

#[test]
fn a_plugin_renames_a_player_in_the_chat_and_in_the_tab_list() {
    let player = support::player("WasmNames");
    let name = plugin::<PlayerEventNameFormat, _>(|event, state| {
        let Event::PlayerEventNameFormat(data) = event else {
            panic!("expected name format");
        };
        assert_eq!(resource(state, &data.username).get_text(), "WasmNames");
        assert_eq!(resource(state, &data.display_name).get_text(), "WasmNames");
        drop_resource(state, &data.display_name);
        data.display_name = state
            .add(TextComponent::text("Sir WasmNames"))
            .expect("new text");
    });
    let event = fire(
        &name,
        PlayerEventNameFormat::new(player.clone(), TextComponent::text("WasmNames")),
    );
    assert_eq!(event.display_name.get_text(), "Sir WasmNames");
    assert_eq!(event.username.get_text(), "WasmNames");

    let tab = plugin::<PlayerEventTabListNameFormat, _>(|event, state| {
        let Event::PlayerEventTabListNameFormat(data) = event else {
            panic!("expected tab list name format");
        };
        assert!(data.display_name.is_none());
        data.display_name = Some(
            state
                .add(TextComponent::text("[W] Names"))
                .expect("new text"),
        );
    });
    let event = fire(&tab, PlayerEventTabListNameFormat::new(player.clone()));
    assert_eq!(
        event.display_name.map(TextComponent::get_text).as_deref(),
        Some("[W] Names")
    );

    // A handler that clears the name shows the player name again.
    let clear = plugin::<PlayerEventTabListNameFormat, _>(|event, state| {
        let Event::PlayerEventTabListNameFormat(data) = event else {
            panic!("expected tab list name format");
        };
        let name = data
            .display_name
            .take()
            .expect("name of the native handler");
        drop_resource(state, &name);
    });
    let mut named = PlayerEventTabListNameFormat::new(player);
    named.display_name = Some(TextComponent::text("Native"));
    let event = fire(&clear, named);
    assert!(event.display_name.is_none());
}

#[test]
fn a_plugin_receives_the_tracking_and_the_player_file_events() {
    let player = support::player("WasmTracking");
    let zombie = mob(&EntityType::ZOMBIE);
    let zombie_id = id(&zombie);
    let start = plugin::<PlayerEventStartTracking, _>(move |event, state| {
        let Event::PlayerEventStartTracking(data) = event else {
            panic!("expected start tracking");
        };
        assert_eq!(
            resource(state, &data.target).get_entity().entity_id,
            zombie_id
        );
    });
    fire(
        &start,
        PlayerEventStartTracking::new(player.clone(), zombie.clone()),
    );
    let stop = plugin::<PlayerEventStopTracking, _>(move |event, state| {
        let Event::PlayerEventStopTracking(data) = event else {
            panic!("expected stop tracking");
        };
        assert_eq!(
            resource(state, &data.target).get_entity().entity_id,
            zombie_id
        );
    });
    fire(&stop, PlayerEventStopTracking::new(player.clone(), zombie));

    let uuid = player.gameprofile.id.to_string();
    let expected = uuid.clone();
    let load = plugin::<PlayerEventLoadFromFile, _>(move |event, _state| {
        let Event::PlayerEventLoadFromFile(data) = event else {
            panic!("expected load from file");
        };
        assert_eq!(data.player_directory, "world/playerdata");
        assert_eq!(data.player_uuid, expected);
        data.player_uuid = String::from("changed");
    });
    let event = fire(
        &load,
        PlayerEventLoadFromFile::new(
            player.clone(),
            PathBuf::from("world/playerdata"),
            uuid.clone(),
        ),
    );
    assert_eq!(event.player_uuid, uuid);

    let save = plugin::<PlayerEventSaveToFile, _>(|event, _state| {
        let Event::PlayerEventSaveToFile(data) = event else {
            panic!("expected save to file");
        };
        assert_eq!(data.player_directory, "world/playerdata");
    });
    fire(
        &save,
        PlayerEventSaveToFile::new(player, PathBuf::from("world/playerdata"), uuid),
    );
}

#[test]
fn a_plugin_receives_a_flyable_fall_and_a_hotbar_switch() {
    let player = support::player("WasmFall");
    let fall = plugin::<PlayerFlyableFallEvent, _>(|event, _state| {
        let Event::PlayerFlyableFallEvent(data) = event else {
            panic!("expected flyable fall");
        };
        assert_eq!((data.distance, data.multiplier), (12.0, 1.0));
        data.distance = 0.0;
    });
    let event = fire(
        &fall,
        PlayerFlyableFallEvent::new(player.clone(), 12.0, 1.0),
    );
    assert_eq!(event.distance, 12.0);

    let hotbar = plugin::<PlayerSwitchHotbarSlotEventPost, _>(|event, _state| {
        let Event::PlayerSwitchHotbarSlotEventPost(data) = event else {
            panic!("expected hotbar switch");
        };
        assert_eq!((data.old_slot_index, data.new_slot_index), (2, 7));
    });
    fire(&hotbar, PlayerSwitchHotbarSlotEventPost::new(player, 2, 7));
}

#[test]
fn a_plugin_cancels_an_orb_pickup_and_forces_an_item_pickup() {
    let player = support::player("WasmPickup");
    let orb = mob(&EntityType::EXPERIENCE_ORB);
    let orb_id = id(&orb);
    let xp = plugin::<PlayerXpEventPickupXp, _>(move |event, state| {
        let Event::PlayerXpEventPickupXp(data) = event else {
            panic!("expected pickup xp");
        };
        assert_eq!(resource(state, &data.orb).get_entity().entity_id, orb_id);
        assert_eq!(data.amount, 7);
        data.amount = 100;
        data.cancelled = true;
    });
    let event = fire(&xp, PlayerXpEventPickupXp::new(player.clone(), orb, 7));
    assert_eq!(event.amount, 7);
    assert!(event.cancelled);

    let item = mob(&EntityType::ITEM);
    let pre = plugin::<ItemEntityPickupEventPre, _>(|event, _state| {
        let Event::ItemEntityPickupEventPre(data) = event else {
            panic!("expected item pickup pre");
        };
        assert_eq!(data.can_pickup, WitTriState::Default);
        data.can_pickup = WitTriState::True;
    });
    let event = fire(
        &pre,
        ItemEntityPickupEventPre::new(player.clone(), item.clone()),
    );
    assert_eq!(event.can_pickup, TriState::True);

    let post = plugin::<ItemEntityPickupEventPost, _>(|event, state| {
        let Event::ItemEntityPickupEventPost(data) = event else {
            panic!("expected item pickup post");
        };
        assert_eq!(item_id(state, &data.original_stack), Item::APPLE.id);
        assert_eq!(item_id(state, &data.current_stack), Item::AIR.id);
    });
    fire(
        &post,
        ItemEntityPickupEventPost::new(player, item, stack(&Item::APPLE), ItemStack::EMPTY.clone()),
    );
}

#[test]
fn a_plugin_reads_a_trade_and_the_client_information() {
    let player = support::player("WasmTrade");
    let villager = mob(&EntityType::VILLAGER);
    let villager_id = id(&villager);
    let trade = plugin::<TradeWithVillagerEvent, _>(move |event, state| {
        let Event::TradeWithVillagerEvent(data) = event else {
            panic!("expected trade");
        };
        assert_eq!(
            resource(state, &data.villager).get_entity().entity_id,
            villager_id
        );
        assert_eq!(item_id(state, &data.offer.base_cost_a), Item::EMERALD.id);
        assert_eq!(
            item_id(state, data.offer.cost_b.as_ref().expect("second cost")),
            Item::BOOK.id
        );
        assert_eq!(item_id(state, &data.offer.output), Item::BREAD.id);
        assert_eq!(
            (data.offer.uses, data.offer.max_uses, data.offer.xp),
            (1, 12, 2)
        );
        assert_eq!(data.offer.price_multiplier, 0.05);
        assert!(data.offer.reward_exp);
    });
    let offer = MerchantOffer {
        base_cost_a: ItemStackSerializer(Cow::Owned(stack(&Item::EMERALD))),
        cost_b: Some(ItemStackSerializer(Cow::Owned(stack(&Item::BOOK)))),
        output: ItemStackSerializer(Cow::Owned(stack(&Item::BREAD))),
        reward_exp: true,
        uses: 1,
        max_uses: 12,
        xp: 2,
        special_price: 0,
        price_multiplier: 0.05,
        demand: 0,
    };
    fire(
        &trade,
        TradeWithVillagerEvent::new(player.clone(), offer, villager),
    );

    let client = plugin::<ClientInformationUpdatedEvent, _>(|event, _state| {
        let Event::ClientInformationUpdatedEvent(data) = event else {
            panic!("expected client information");
        };
        assert_eq!(data.old_information.locale, "en_us");
        assert_eq!(data.old_information.chat_mode, WitChatMode::Enabled);
        assert_eq!(data.old_information.main_hand, WitHand::Right);
        assert_eq!(data.old_information.particle_status, WitParticleStatus::All);
        let updated = &data.updated_information;
        assert_eq!(updated.locale, "it_it");
        assert_eq!(updated.view_distance, 12);
        assert_eq!(updated.chat_mode, WitChatMode::CommandsOnly);
        assert_eq!(updated.skin_parts, 0x3F);
        assert_eq!(updated.main_hand, WitHand::Left);
        assert!(updated.text_filtering && updated.server_listing && !updated.chat_colors);
        assert_eq!(updated.particle_status, WitParticleStatus::Minimal);
    });
    let updated = PlayerConfig {
        locale: String::from("it_it"),
        view_distance: std::num::NonZero::new(12).expect("non-zero"),
        chat_mode: ChatMode::CommandsOnly,
        chat_colors: false,
        skin_parts: 0x3F,
        main_hand: Hand::Left,
        text_filtering: true,
        server_listing: true,
        particle_status: ParticleStatus::Minimal,
    };
    fire(
        &client,
        ClientInformationUpdatedEvent::new(
            player,
            Arc::new(PlayerConfig::default()),
            Arc::new(updated),
        ),
    );
}

#[test]
fn a_plugin_changes_the_stacks_of_a_brew_and_the_item_of_an_enchant() {
    let brew = plugin::<PotionBrewEventPost, _>(|event, state| {
        let Event::PotionBrewEventPost(data) = event else {
            panic!("expected brew post");
        };
        assert_eq!(
            (data.block_pos.x, data.block_pos.y, data.block_pos.z),
            (4, 5, 6)
        );
        assert_eq!(data.stacks.len(), 5);
        assert_eq!(item_id(state, &data.stacks[0]), Item::POTION.id);
        assert_eq!(item_id(state, &data.stacks[4]), Item::BLAZE_POWDER.id);
        replace_stack(state, &mut data.stacks[1], &Item::SPLASH_POTION);
        // A short list keeps the stacks of the host past its end.
        let dropped = data.stacks.split_off(3);
        for res in &dropped {
            drop_resource(state, res);
        }
    });
    let event = fire(
        &brew,
        PotionBrewEventPost::new(
            BlockPos::new(4, 5, 6),
            [
                stack(&Item::POTION),
                stack(&Item::POTION),
                stack(&Item::POTION),
                ItemStack::EMPTY.clone(),
                stack(&Item::BLAZE_POWDER),
            ],
        ),
    );
    let ids: Vec<u16> = event.stacks.iter().map(|stack| stack.item.id).collect();
    assert_eq!(
        ids,
        [
            Item::POTION.id,
            Item::SPLASH_POTION.id,
            Item::POTION.id,
            Item::AIR.id,
            Item::BLAZE_POWDER.id
        ]
    );

    let enchant = plugin::<PlayerEnchantItemEvent, _>(|event, state| {
        let Event::PlayerEnchantItemEvent(data) = event else {
            panic!("expected enchant item");
        };
        assert_eq!(item_id(state, &data.enchanted_item), Item::DIAMOND_SWORD.id);
        assert_eq!(data.enchantments.len(), 1);
        assert_eq!(data.enchantments[0].enchantment, WitEnchantment::Sharpness);
        assert_eq!(data.enchantments[0].level, 3);
        replace_stack(state, &mut data.enchanted_item, &Item::NETHERITE_SWORD);
    });
    let event = fire(
        &enchant,
        PlayerEnchantItemEvent::new(
            support::player("WasmEnchant"),
            stack(&Item::DIAMOND_SWORD),
            vec![(&Enchantment::SHARPNESS, 3)],
        ),
    );
    assert_eq!(event.enchanted_item.item.id, Item::NETHERITE_SWORD.id);
}

#[test]
fn a_plugin_changes_the_inputs_and_the_experience_of_a_grindstone_take() {
    let grindstone = plugin::<GrindstoneEventOnTakeItem, _>(|event, state| {
        let Event::GrindstoneEventOnTakeItem(data) = event else {
            panic!("expected grindstone take");
        };
        assert_eq!(item_id(state, &data.top), Item::DIAMOND_SWORD.id);
        assert_eq!(item_id(state, &data.bottom), Item::AIR.id);
        assert_eq!(item_id(state, &data.new_top), Item::AIR.id);
        assert_eq!(data.xp, 9);
        replace_stack(state, &mut data.top, &Item::STICK);
        replace_stack(state, &mut data.new_bottom, &Item::BOOK);
        data.xp = 1;
        data.cancelled = true;
    });
    let event = fire(
        &grindstone,
        GrindstoneEventOnTakeItem::new(
            support::player("WasmGrindstone"),
            BlockPos::new(0, 64, 0),
            stack(&Item::DIAMOND_SWORD),
            ItemStack::EMPTY.clone(),
            9,
        ),
    );
    assert_eq!(event.top.item.id, Item::DIAMOND_SWORD.id);
    assert_eq!(event.new_top.item.id, Item::AIR.id);
    assert_eq!(event.new_bottom.item.id, Item::BOOK.id);
    assert_eq!(event.xp, 1);
    assert!(event.cancelled);
}

#[test]
fn a_plugin_receives_the_level_tick_events() {
    let pre = plugin::<LevelTickEventPre, _>(|event, state| {
        let Event::LevelTickEventPre(data) = event else {
            panic!("expected level tick pre");
        };
        assert!(is_shared_world(state, &data.target_world));
    });
    let event = fire(&pre, LevelTickEventPre::new(support::world()));
    assert!(Arc::ptr_eq(&event.world, &support::world()));

    let post = plugin::<LevelTickEventPost, _>(|event, state| {
        let Event::LevelTickEventPost(data) = event else {
            panic!("expected level tick post");
        };
        assert!(is_shared_world(state, &data.target_world));
    });
    fire(&post, LevelTickEventPost::new(support::world()));
}
