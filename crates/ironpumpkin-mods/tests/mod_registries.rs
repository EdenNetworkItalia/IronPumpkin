//! Data component types, sound events, mob effects and attributes of a native mod on a real
//! server (`openspec/changes/neoforge-shaped-plugin-api`, spec `neoforge-registration`,
//! requirements "Data components, sound events, mob effects and attributes" and "Access to mod
//! components, effects and attributes"). The server runs `init_mods` as its startup hook and
//! freezes the registries once per process, so this file is its own test binary. It shares the
//! server support of the `pumpkin-core` tests.
#![expect(
    clippy::unwrap_used,
    reason = "a test fails on the first missing value"
)]

#[path = "../../pumpkin-core/tests/support/mod.rs"]
mod support;

use std::{
    io::Cursor,
    sync::{Arc, Mutex, Once, PoisonError},
    time::Duration,
};

use ironpumpkin_mods::{
    ModInit, NativeMod,
    content::{ContentKind, EntityTypeBuilder},
    data_components::{self, ComponentError, DataComponentType, DataComponentTypeBuilder},
    entity::{Entity, EntityBase, living::LivingEntity, r#type::from_type},
    entity_attributes::{self, AttributeBuilder, AttributeModifier, ModifierOperation},
    event::{BoxFuture, EventHandler, EventPriority, registries::RegisterEvent},
    mob_effects::{self, MobEffect, MobEffectBuilder, MobEffectCategory, Operation},
    pumpkin_core::{
        entity::player::Player,
        net::{ClientPlatform, java::neoforge::NegotiatedState},
        plugin::startup::STARTUP_HOOK,
    },
    pumpkin_data::{
        AttributeModifierSlot,
        attributes::Attributes,
        data_component::DataComponent,
        data_component_impl::{
            AttributeModifiersImpl, DataComponentImpl, Modifier, PotionContentsImpl,
            StatusEffectInstance, SuspiciousStewEffect, SuspiciousStewEffectsImpl,
        },
        dynamic::ContentIds,
        effect::StatusEffect,
        entity::EntityType,
        item::Item,
        item_stack::ItemStack,
        packet::CURRENT_MC_VERSION,
        sound::{Sound, SoundCategory},
    },
    register_mod, registry,
    server::Server,
    sound_events::{self, RegisterSoundEvent, SoundEvent, SoundEventBuilder},
};
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use pumpkin_protocol::{
    EncodingKey,
    codec::{data_component, item_stack_seralizer::ItemStackSerializer},
    java::{
        client::play::CSoundEffect, packet_decoder::TCPNetworkDecoder,
        server::play::SSetCreativeSlot,
    },
    packet::MultiVersionJavaPacket,
    ser::NetworkReadExt,
};
use pumpkin_util::{GameMode, math::vector3::Vector3};

const ZAP: SoundEvent = SoundEvent::new("regmod:zap");
const CHARGE: DataComponentType<i32> = DataComponentType::new("regmod:charge");
const SHOCK: &str = "regmod:shock";
const MANA: &str = "regmod:mana";
const GOLEM: &str = "regmod:golem";

/// The entity ids that the shock effect ticked, in order.
static SHOCKED: Mutex<Vec<i32>> = Mutex::new(Vec::new());

struct Shock;

impl MobEffect for Shock {
    fn should_apply_effect_tick(&self, _duration: i32, _amplifier: u8) -> bool {
        true
    }

    fn apply_effect_tick(&self, living: &LivingEntity, _amplifier: u8) {
        SHOCKED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(living.entity_id());
    }
}

fn spawn_living(entity: Entity) -> Arc<dyn EntityBase> {
    Arc::new(LivingEntity::new(entity))
}

/// Registers one entry of each registry: the sound event from a `RegisterEvent` handler, the
/// rest from `init`.
struct RegMod;

impl NativeMod for RegMod {
    fn id(&self) -> &'static str {
        "regmod"
    }

    fn display_name(&self) -> &'static str {
        "Registries mod"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn init(&self, cx: &mut ModInit) {
        cx.register_data_component_type(DataComponentTypeBuilder::<i32>::new(CHARGE.name()))
            .unwrap();
        // The effect names the attribute before it registers: it resolves at the freeze.
        cx.register_mob_effect(
            MobEffectBuilder::new(SHOCK, MobEffectCategory::Harmful, 0x00_FF_FF)
                .attribute_modifier(MANA, "regmod:effect.shock", -2.0, Operation::AddValue)
                .behaviour(Arc::new(Shock)),
        )
        .unwrap();
        cx.register_attribute(AttributeBuilder::ranged(MANA, 20.0, 0.0, 1000.0).syncable(true))
            .unwrap();
        cx.register_entity_type(
            EntityTypeBuilder::new(GOLEM, &EntityType::ZOMBIE),
            spawn_living,
        )
        .unwrap();
        cx.register_event::<RegisterEvent, _>(
            Arc::new(RegisterZap),
            EventPriority::Normal,
            true,
            false,
        );
    }
}

register_mod!(RegMod);

struct RegisterZap;

impl EventHandler<RegisterEvent> for RegisterZap {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut RegisterEvent,
    ) -> BoxFuture<'a, ()> {
        event
            .register_sound_event(SoundEventBuilder::new(ZAP.name()))
            .unwrap();
        Box::pin(async {})
    }
}

fn server() -> &'static Arc<Server> {
    static HOOK: Once = Once::new();
    HOOK.call_once(|| {
        STARTUP_HOOK.set(ironpumpkin_mods::init_mods).unwrap();
    });
    support::server()
}

fn shock() -> &'static StatusEffect {
    mob_effects::effect(SHOCK).unwrap()
}

fn mana() -> &'static Attributes {
    entity_attributes::attribute(MANA).unwrap()
}

/// Scenario "Entries bind" for the four registries: the entries take the ids after the vanilla
/// counts, and the lookups answer for them.
#[test]
fn the_entries_bind_after_the_vanilla_ids() {
    server();
    for (kind, name, id) in [
        (
            ContentKind::SoundEvent,
            ZAP.name(),
            Sound::NAMES.len() as u16,
        ),
        (
            ContentKind::MobEffect,
            SHOCK,
            u16::from(StatusEffect::COUNT),
        ),
        // After the three `NeoForge` attributes.
        (
            ContentKind::Attribute,
            MANA,
            u16::from(Attributes::COUNT) + 3,
        ),
        (
            ContentKind::DataComponentType,
            CHARGE.name(),
            DataComponent::COUNT,
        ),
    ] {
        assert_eq!(registry::id_of(kind, name), Some(id), "{kind} {name}");
        assert_eq!(registry::key_of(kind, id).unwrap().to_string(), name);
        assert_eq!(registry::keys(kind).last().unwrap().to_string(), name);
        assert!(!registry::is_in_tag(kind, name, "c:anything"));
    }
    assert_eq!(
        registry::id_of(ContentKind::Attribute, "neoforge:swim_speed"),
        Some(u16::from(Attributes::COUNT))
    );
    assert_eq!(
        registry::key_of(ContentKind::SoundEvent, 0)
            .unwrap()
            .to_string(),
        "minecraft:entity.allay.ambient_with_item"
    );
}

/// Scenario "A played mod sound": a mod registers `regmod:zap` and plays it; the clients near the
/// position get it by name.
#[test]
fn a_played_mod_sound_reaches_the_players_near_the_position() {
    server();
    let (player, peer) = support::player_with_peer("SoundNear", NegotiatedState::default());
    let world = support::world();
    // The other tests of this binary join their players at the spawn and keep them.
    let position = player
        .living_entity
        .entity
        .pos
        .load()
        .add(&Vector3::new(1000.0, 0.0, 0.0));
    player.living_entity.entity.pos.store(position);
    let far = position.add(&Vector3::new(100.0, 0.0, 0.0));
    assert_eq!(
        sound_events::play_sound(&world, &ZAP, SoundCategory::Players, &far, 1.0, 1.0),
        Ok(0)
    );
    assert_eq!(
        sound_events::play_sound(&world, &ZAP, SoundCategory::Players, &position, 1.0, 1.0),
        Ok(1)
    );
    assert!(
        sound_events::play_sound(
            &world,
            &SoundEvent::new("regmod:missing"),
            SoundCategory::Players,
            &position,
            1.0,
            1.0
        )
        .is_err()
    );

    let name = support::RUNTIME.block_on(async {
        let mut decoder = TCPNetworkDecoder::new(tokio::io::BufReader::new(peer));
        loop {
            let packet = tokio::time::timeout(Duration::from_secs(10), decoder.get_raw_packet())
                .await
                .unwrap()
                .unwrap();
            if packet.id == CSoundEffect::to_id(CURRENT_MC_VERSION) {
                let mut body = &packet.payload[..];
                // `Holder.direct`: id 0, then the sound event by name.
                assert_eq!(body.get_var_int().unwrap().0, 0);
                return body.get_str().unwrap().to_string();
            }
        }
    });
    assert_eq!(name, ZAP.name());
}

/// The client form of a stack: the count of added components and the bytes.
fn client_stack(stack: &ItemStack, ids: ContentIds) -> (i32, Vec<u8>) {
    let mut bytes = Vec::new();
    ItemStackSerializer::from(stack.clone())
        .write_with_version(&mut bytes, &EncodingKey::new(CURRENT_MC_VERSION, ids))
        .unwrap();
    let mut read = &bytes[..];
    // Count, item id, then the number of added components.
    read.get_var_int().unwrap();
    read.get_var_int().unwrap();
    (read.get_var_int().unwrap().0, bytes)
}

/// Scenario "A mod component on an item": the persistent type `regmod:charge` set to 5 survives a
/// save and load of the stack and reaches a client that synced the registries as a component.
#[test]
fn a_mod_component_on_an_item_survives_a_save_and_reaches_a_synced_client() {
    server();
    let mut stack = ItemStack::new(1, &Item::STICK);
    assert_eq!(CHARGE.get(&stack), Ok(None));
    CHARGE.set(&mut stack, &5).unwrap();

    let mut saved = NbtCompound::new();
    stack.write_item_stack(&mut saved);
    let mut loaded = ItemStack::read_item_stack(&saved).unwrap();
    assert_eq!(CHARGE.get(&loaded), Ok(Some(5)));
    assert_eq!(
        data_components::get_json(&loaded, CHARGE.name()),
        Ok(Some(serde_json::json!(5)))
    );

    // The JSON form goes through the codec of the type.
    data_components::set_json(&mut loaded, CHARGE.name(), serde_json::json!(7)).unwrap();
    assert_eq!(CHARGE.get(&loaded), Ok(Some(7)));
    assert!(matches!(
        data_components::set_json(&mut loaded, CHARGE.name(), serde_json::json!("seven")),
        Err(ComponentError::Codec { .. })
    ));
    assert_eq!(
        data_components::get_json(&loaded, "regmod:missing"),
        Err(ComponentError::UnknownType("regmod:missing".to_owned()))
    );

    // A synced client gets the component id and the NBT int; a vanilla client gets nothing.
    let (added, bytes) = client_stack(&stack, ContentIds::Real);
    assert_eq!(added, 1);
    let mut id_and_value = Vec::new();
    pumpkin_protocol::ser::NetworkWriteExt::write_var_int(
        &mut id_and_value,
        &pumpkin_protocol::VarInt(i32::from(DataComponent::COUNT)),
    )
    .unwrap();
    id_and_value.extend([3, 0, 0, 0, 5]);
    assert!(bytes.ends_with(&id_and_value), "{bytes:?}");
    assert_eq!(client_stack(&stack, ContentIds::Display).0, 0);
    let read = ItemStackSerializer::read(&mut Cursor::new(&bytes[..]))
        .unwrap()
        .to_stack();
    assert_eq!(CHARGE.get(&read), Ok(Some(5)));

    assert!(CHARGE.remove(&mut loaded).unwrap());
    assert!(!CHARGE.has(&loaded));
}

/// Scenario "A mod effect ticks": an effect applied to a player runs its tick callback and its
/// modifier while it lasts, is saved by name, and ends.
#[test]
fn a_mod_effect_applied_to_a_player_ticks_and_expires() {
    let server = server();
    let player = support::player("Shocked");
    let living = &player.living_entity;
    mob_effects::add_effect(living, mob_effects::instance(shock(), 3, 0));
    assert!(mob_effects::has_effect(living, shock()));
    assert_eq!(entity_attributes::value(living, mana()), 18.0);

    let mut saved = NbtCompound::new();
    living.write_living_nbt(&mut saved);
    let effects = saved.get_list("active_effects").unwrap();
    let NbtTag::Compound(effect) = &effects[0] else {
        unreachable_tag();
    };
    assert_eq!(effect.get_string("id"), Some(SHOCK));

    let _guard = support::RUNTIME.enter();
    for _ in 0..3 {
        player.tick(server);
        assert!(mob_effects::has_effect(living, shock()));
    }
    player.tick(server);
    assert!(!mob_effects::has_effect(living, shock()));
    let ticked = SHOCKED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    assert_eq!(ticked, vec![living.entity_id(); 3]);
    assert_eq!(entity_attributes::value(living, mana()), 20.0);
}

#[expect(clippy::panic, reason = "a test fails on the first missing value")]
fn unreachable_tag() -> ! {
    panic!("an effect is a compound");
}

/// A mod attribute on a mod entity: the default is its base value, and it takes modifiers within
/// its range.
#[test]
fn a_mod_attribute_on_a_mod_entity_has_its_base_value_and_accepts_modifiers() {
    server();
    let world = support::world();
    let golem_type = EntityType::from_name(GOLEM).unwrap();
    let golem = from_type(
        golem_type,
        Vector3::new(0.0, 200.0, 0.0),
        &world,
        uuid::Uuid::new_v4(),
    );
    let living = golem.get_living_entity().unwrap();
    assert_eq!(entity_attributes::base_value(living, mana()), 20.0);
    assert_eq!(entity_attributes::value(living, mana()), 20.0);

    let modifier = |id: &str, amount| AttributeModifier {
        id: id.to_owned(),
        amount,
        operation: ModifierOperation::Add,
    };
    entity_attributes::add_modifier(living, mana(), modifier("regmod:crystal", 5.0));
    assert_eq!(entity_attributes::value(living, mana()), 25.0);
    entity_attributes::add_modifier(living, mana(), modifier("regmod:overflow", 5000.0));
    // `RangedAttribute#sanitizeValue` clamps to the maximum.
    assert_eq!(entity_attributes::value(living, mana()), 1000.0);
    assert!(entity_attributes::remove_modifier(
        living,
        mana(),
        "regmod:overflow"
    ));
    assert!(!entity_attributes::remove_modifier(
        living,
        mana(),
        "regmod:overflow"
    ));
    assert_eq!(entity_attributes::modifiers(living, mana()).len(), 1);
    entity_attributes::set_base_value(living, mana(), 30.0);
    assert_eq!(entity_attributes::value(living, mana()), 35.0);
    // A vanilla attribute answers the same way.
    assert_eq!(
        entity_attributes::base_value(living, &Attributes::MAX_HEALTH),
        20.0
    );
}

/// Writes `component` for a client with `ids` and returns the bytes.
fn component_bytes(
    id: DataComponent,
    component: &dyn DataComponentImpl,
    ids: ContentIds,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    data_component::serialize(id, component, &mut bytes, ids).unwrap();
    bytes
}

/// The effect ids of a `potion_contents` component without a potion, a color or a name.
fn potion_effect_ids(mut bytes: &[u8]) -> Vec<i32> {
    assert!(!bytes.get_bool().unwrap());
    assert!(!bytes.get_bool().unwrap());
    let count = bytes.get_var_int().unwrap().0;
    let ids = (0..count)
        .map(|_| {
            let id = bytes.get_var_int().unwrap().0;
            // Amplifier, duration, the three flags and no hidden effect.
            bytes.get_var_int().unwrap();
            bytes.get_var_int().unwrap();
            for _ in 0..4 {
                bytes.get_bool().unwrap();
            }
            id
        })
        .collect();
    assert!(!bytes.get_bool().unwrap());
    assert!(bytes.is_empty());
    ids
}

/// The effect ids of a `suspicious_stew_effects` component.
fn stew_effect_ids(mut bytes: &[u8]) -> Vec<i32> {
    let count = bytes.get_var_int().unwrap().0;
    let ids = (0..count)
        .map(|_| {
            let id = bytes.get_var_int().unwrap().0;
            bytes.get_var_int().unwrap();
            id
        })
        .collect();
    assert!(bytes.is_empty());
    ids
}

/// The attribute ids of an `attribute_modifiers` component with the default display.
fn modifier_attribute_ids(mut bytes: &[u8]) -> Vec<i32> {
    let count = bytes.get_var_int().unwrap().0;
    let ids = (0..count)
        .map(|_| {
            let id = bytes.get_var_int().unwrap().0;
            bytes.get_str().unwrap();
            bytes.get_f64().unwrap();
            // Operation, slot and display.
            for _ in 0..3 {
                bytes.get_var_int().unwrap();
            }
            id
        })
        .collect();
    assert!(bytes.is_empty());
    ids
}

/// A vanilla client knows the generated effects and attributes only: the vanilla components that
/// hold a mod effect or a `NeoForge` attribute leave them out for it, and keep them for a client
/// that synced the registries.
#[test]
fn vanilla_components_leave_mod_effects_and_attributes_out_for_a_vanilla_client() {
    server();
    let effect = |name: &'static str| StatusEffectInstance {
        effect_id: name.into(),
        amplifier: 0,
        duration: 200,
        ambient: false,
        show_particles: true,
        show_icon: true,
    };
    let potion = PotionContentsImpl {
        potion_id: None,
        custom_color: None,
        custom_effects: vec![effect("minecraft:speed"), effect(SHOCK)],
        custom_name: None,
    };
    let stew = SuspiciousStewEffectsImpl {
        effects: vec![
            SuspiciousStewEffect {
                effect: "minecraft:speed".into(),
                duration: 200,
            },
            SuspiciousStewEffect {
                effect: SHOCK.into(),
                duration: 200,
            },
        ]
        .into(),
    };
    let modifier = |attribute, id| Modifier {
        r#type: attribute,
        id,
        amount: 1.0,
        operation: Operation::AddValue,
        slot: AttributeModifierSlot::Any,
    };
    let modifiers = AttributeModifiersImpl {
        attribute_modifiers: vec![
            modifier(&Attributes::ARMOR, "test:armor"),
            modifier(&Attributes::NEOFORGE_SWIM_SPEED, "test:fins"),
        ]
        .into(),
    };

    let speed = i32::from(StatusEffect::SPEED.id);
    let shock = i32::from(shock().id);
    let armor = i32::from(Attributes::ARMOR.id);
    let swim_speed = i32::from(Attributes::NEOFORGE_SWIM_SPEED.id);
    for (ids, effects, attributes) in [
        (ContentIds::Display, vec![speed], vec![armor]),
        (
            ContentIds::Real,
            vec![speed, shock],
            vec![armor, swim_speed],
        ),
    ] {
        assert_eq!(
            potion_effect_ids(&component_bytes(
                DataComponent::PotionContents,
                &potion,
                ids
            )),
            effects,
            "{ids:?}"
        );
        assert_eq!(
            stew_effect_ids(&component_bytes(
                DataComponent::SuspiciousStewEffects,
                &stew,
                ids
            )),
            effects,
            "{ids:?}"
        );
        assert_eq!(
            modifier_attribute_ids(&component_bytes(
                DataComponent::AttributeModifiers,
                &modifiers,
                ids
            )),
            attributes,
            "{ids:?}"
        );
    }
}

/// The first hotbar slot of the player screen handler.
const HOTBAR_SLOT: i16 = 36;

/// A creative client of the player puts a stick with `charge` as the `regmod:charge` value in the
/// first hotbar slot. Returns the stack that the slot holds after.
fn creative_charge(player: &Arc<Player>, charge: NbtTag) -> ItemStack {
    let mut stack = ItemStack::new(1, &Item::STICK);
    stack.set_custom_component(
        registry::id_of(ContentKind::DataComponentType, CHARGE.name()).unwrap(),
        charge,
    );
    let ClientPlatform::Java(client) = player.client.as_ref() else {
        unreachable_client();
    };
    client
        .handle_set_creative_slot(
            player,
            SSetCreativeSlot::new(HOTBAR_SLOT, ItemStackSerializer::from(stack)),
        )
        .unwrap();
    // The first hotbar slot of the screen handler is the first slot of the inventory.
    player.inventory().get_slot(0)
}

#[expect(clippy::panic, reason = "a test fails on the first missing value")]
fn unreachable_client() -> ! {
    panic!("the support players are Java players");
}

/// A creative client sends a stack with a mod component: the server keeps a value that decodes
/// through the codec of the type from a client that synced the registries, keeps its own stack
/// when the value does not decode, and drops the value from a vanilla client.
#[test]
fn a_creative_client_sets_only_mod_components_that_it_knows_and_that_decode() {
    server();
    let synced = NegotiatedState {
        content_ids: ContentIds::Real,
        ..NegotiatedState::default()
    };
    let (player, _peer) = support::player_with_peer("CreativeSynced", synced);
    player.gamemode.store(GameMode::Creative);
    let stack = creative_charge(&player, NbtTag::Int(5));
    assert_eq!(CHARGE.get(&stack), Ok(Some(5)));
    let stack = creative_charge(&player, NbtTag::String("five".into()));
    assert_eq!(CHARGE.get(&stack), Ok(Some(5)));

    let (player, _peer) = support::player_with_peer("CreativeVanilla", NegotiatedState::default());
    player.gamemode.store(GameMode::Creative);
    let stack = creative_charge(&player, NbtTag::Int(5));
    assert_eq!(stack.item.id, Item::STICK.id);
    assert!(stack.custom_components.is_empty());
}
