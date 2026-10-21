//! The v0.2 conversions of the `NeoForge` 26.3 events at fire sites where Pumpkin has no event.
//!
//! These are the events of `pumpkin_core::plugin::level` and `UseItemOnBlockEvent`, and the
//! `NeoForge` result types that the player interact event shares with them.
//!
//! A blocking handler writes back only the settable fields; the other fields keep the native
//! values, as `NeoForge` has no setter for them.

use std::sync::{
    Arc, Mutex as StdMutex, PoisonError,
    atomic::{AtomicBool, Ordering},
};

use pumpkin_core::plugin::{
    Context, EventPriority,
    api::events::results::{InteractionResult, TriState},
    level::{
        BlockEventBlockToolModificationEvent, BlockEventEntityPlaceEvent, ChunkWatchEventUnWatch,
        CreateFluidSourceEvent, CropGrowEventPost, EntityTickEventPost, EntityTickEventPre,
        ExplosionEventDetonate, ExplosionKnockbackEvent, GameRuleChangedEvent,
        LevelEventPotentialSpawns, NoteBlockEventChange, PistonEventPost, PistonMoveType,
        PlayerTickEventPost, PlayerTickEventPre,
    },
    player::{
        player_interact_event::LeftClickBlockAction,
        use_item_on_block::{UseItemOnBlockEvent, UsePhase},
    },
};
use pumpkin_data::{
    BlockState, BlockStateId, biome::Spawner, entity::EntityType, entity::MobCategory,
};
use pumpkin_wasm_host_common::state::{FromResource, PluginHostState};
use tokio::sync::Mutex;
use wasmtime::component::Resource;

use crate::{
    events::{
        ToFromWasmEvent, WasmPluginEventHandler, from_wasm_position, to_wasm_block_position,
        to_wasm_hand, to_wasm_position,
    },
    mob::to_wit_block_direction,
    pumpkin::plugin::{
        event::{Event, EventType},
        neoforge_events::{
            BlockEventBlockToolModificationEventData, BlockEventEntityPlaceEventData,
            ChunkWatchEventUnWatchData, CreateFluidSourceEventData, CropGrowEventPostData,
            EntityTickEventPostData, EntityTickEventPreData, ExplosionEventDetonateData,
            ExplosionKnockbackEventData, GameRuleChangedEventData,
            InteractionResult as WitInteractionResult,
            LeftClickBlockAction as WitLeftClickBlockAction, LevelEventPotentialSpawnsData,
            MobCategory as WitMobCategory, NoteBlockEventChangeData, PistonEventPostData,
            PistonMoveType as WitPistonMoveType, PlayerTickEventPostData, PlayerTickEventPreData,
            SpawnerData, TriState as WitTriState, UseItemOnBlockEventData, UsePhase as WitUsePhase,
        },
    },
    world::{to_wit_game_rule, to_wit_game_rule_value},
};

pub(super) fn state_from_wasm(id: u16) -> &'static BlockState {
    BlockState::from_id(BlockStateId::new_or_air(id))
}

pub(super) const fn to_wasm_tri_state(value: TriState) -> WitTriState {
    match value {
        TriState::True => WitTriState::True,
        TriState::Default => WitTriState::Default,
        TriState::False => WitTriState::False,
    }
}

pub(super) const fn from_wasm_tri_state(value: WitTriState) -> TriState {
    match value {
        WitTriState::True => TriState::True,
        WitTriState::Default => TriState::Default,
        WitTriState::False => TriState::False,
    }
}

pub(super) const fn to_wasm_interaction_result(value: InteractionResult) -> WitInteractionResult {
    match value {
        InteractionResult::Success => WitInteractionResult::Success,
        InteractionResult::SuccessServer => WitInteractionResult::SuccessServer,
        InteractionResult::Consume => WitInteractionResult::Consume,
        InteractionResult::Fail => WitInteractionResult::Fail,
        InteractionResult::Pass => WitInteractionResult::Pass,
        InteractionResult::TryWithEmptyHand => WitInteractionResult::TryWithEmptyHand,
    }
}

pub(super) const fn from_wasm_interaction_result(value: WitInteractionResult) -> InteractionResult {
    match value {
        WitInteractionResult::Success => InteractionResult::Success,
        WitInteractionResult::SuccessServer => InteractionResult::SuccessServer,
        WitInteractionResult::Consume => InteractionResult::Consume,
        WitInteractionResult::Fail => InteractionResult::Fail,
        WitInteractionResult::Pass => InteractionResult::Pass,
        WitInteractionResult::TryWithEmptyHand => InteractionResult::TryWithEmptyHand,
    }
}

pub(super) const fn to_wasm_left_click_action(
    action: LeftClickBlockAction,
) -> WitLeftClickBlockAction {
    match action {
        LeftClickBlockAction::Start => WitLeftClickBlockAction::Start,
        LeftClickBlockAction::Stop => WitLeftClickBlockAction::Stop,
        LeftClickBlockAction::Abort => WitLeftClickBlockAction::Abort,
    }
}

pub(super) const fn from_wasm_left_click_action(
    action: WitLeftClickBlockAction,
) -> LeftClickBlockAction {
    match action {
        WitLeftClickBlockAction::Start => LeftClickBlockAction::Start,
        WitLeftClickBlockAction::Stop => LeftClickBlockAction::Stop,
        WitLeftClickBlockAction::Abort => LeftClickBlockAction::Abort,
    }
}

const fn to_wasm_use_phase(phase: UsePhase) -> WitUsePhase {
    match phase {
        UsePhase::ItemBeforeBlock => WitUsePhase::ItemBeforeBlock,
        UsePhase::Block => WitUsePhase::Block,
        UsePhase::ItemAfterBlock => WitUsePhase::ItemAfterBlock,
    }
}

const fn to_wasm_piston_move_type(move_type: PistonMoveType) -> WitPistonMoveType {
    match move_type {
        PistonMoveType::Extend => WitPistonMoveType::Extend,
        PistonMoveType::Retract => WitPistonMoveType::Retract,
    }
}

const fn to_wasm_mob_category(category: &MobCategory) -> WitMobCategory {
    match category.id {
        id if id == MobCategory::MONSTER.id => WitMobCategory::Monster,
        id if id == MobCategory::CREATURE.id => WitMobCategory::Creature,
        id if id == MobCategory::AMBIENT.id => WitMobCategory::Ambient,
        id if id == MobCategory::AXOLOTLS.id => WitMobCategory::Axolotls,
        id if id == MobCategory::UNDERGROUND_WATER_CREATURE.id => {
            WitMobCategory::UndergroundWaterCreature
        }
        id if id == MobCategory::WATER_CREATURE.id => WitMobCategory::WaterCreature,
        id if id == MobCategory::WATER_AMBIENT.id => WitMobCategory::WaterAmbient,
        _ => WitMobCategory::Misc,
    }
}

fn add<T: FromResource>(state: &mut PluginHostState, item: T::Internal) -> Resource<T> {
    state.add(item).expect("failed to add resource")
}

fn add_item_stack(
    state: &mut PluginHostState,
    item: &pumpkin_data::item_stack::ItemStack,
) -> Resource<crate::pumpkin::plugin::item_stack::ItemStack> {
    add(state, Arc::new(Mutex::new(item.clone())))
}

fn discard<T: FromResource>(state: &mut PluginHostState, res: &Resource<T>) {
    state.discard_to_be_removed(res);
}

/// The spawner entries that a Wasm handler added to a spawn list. A new entry must be a
/// `&'static Spawner`, so each distinct entry is leaked once, up to this many.
const MAX_ADDED_SPAWNERS: usize = 256;

/// The longest spawn list that the host takes back from a Wasm handler.
const MAX_GUEST_SPAWNS: usize = 256;

static ADDED_SPAWNERS: StdMutex<Vec<&'static Spawner>> = StdMutex::new(Vec::new());

static ADDED_SPAWNERS_FULL: AtomicBool = AtomicBool::new(false);

fn spawner_entity_type(name: &str) -> Option<&'static EntityType> {
    EntityType::from_name(name.strip_prefix("minecraft:").unwrap_or(name))
}

fn spawner_matches(
    spawner: &Spawner,
    entity_type: &'static EntityType,
    data: &SpawnerData,
) -> bool {
    spawner_entity_type(spawner.r#type).is_some_and(|other| std::ptr::eq(other, entity_type))
        && spawner.min_count == data.min_count
        && spawner.max_count == data.max_count
}

/// The native entry of `data`: an entry of the list before the handler, an entry added before,
/// or a new entry. `None` for an unknown entity type, counts that the natural spawner cannot
/// use, or a full table of added entries.
fn from_wasm_spawner(
    original: &[&'static Spawner],
    category: &MobCategory,
    data: &SpawnerData,
) -> Option<&'static Spawner> {
    // A group larger than the mob cap of the category never spawns.
    if data.min_count < 1 || data.max_count < data.min_count || data.max_count > category.max {
        return None;
    }
    let entity_type = spawner_entity_type(&data.entity_type)?;
    if let Some(spawner) = original
        .iter()
        .find(|spawner| spawner_matches(spawner, entity_type, data))
    {
        return Some(spawner);
    }
    let mut added = ADDED_SPAWNERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if let Some(spawner) = added
        .iter()
        .find(|spawner| spawner_matches(spawner, entity_type, data))
    {
        return Some(spawner);
    }
    if added.len() >= MAX_ADDED_SPAWNERS {
        if !ADDED_SPAWNERS_FULL.swap(true, Ordering::Relaxed) {
            tracing::warn!(
                "Wasm plugins added {MAX_ADDED_SPAWNERS} distinct spawn entries; further new entries are dropped"
            );
        }
        return None;
    }
    let spawner: &'static Spawner = Box::leak(Box::new(Spawner {
        r#type: entity_type.resource_name,
        min_count: data.min_count,
        max_count: data.max_count,
    }));
    added.push(spawner);
    Some(spawner)
}

impl ToFromWasmEvent for UseItemOnBlockEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::UseItemOnBlockEvent(UseItemOnBlockEventData {
            player: add(state, self.player.clone()),
            hand: to_wasm_hand(self.hand),
            item: add_item_stack(state, &self.item),
            block_pos: to_wasm_block_position(self.pos),
            face: to_wit_block_direction(self.face),
            hit_vector: to_wasm_position(self.hit_vector),
            phase: to_wasm_use_phase(self.phase),
            cancellation_result: to_wasm_interaction_result(self.cancellation_result),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_event(&event, state);
        if let Event::UseItemOnBlockEvent(data) = event {
            self.cancellation_result = from_wasm_interaction_result(data.cancellation_result);
            self.cancelled = data.cancelled;
        }
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct UseItemOnBlockEvent from WASM")
    }
}

impl ToFromWasmEvent for BlockEventEntityPlaceEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::BlockEventEntityPlaceEvent(BlockEventEntityPlaceEventData {
            entity: add(state, self.entity.clone()),
            target_world: add(state, self.world.clone()),
            block_pos: to_wasm_block_position(self.pos),
            placed_state_id: self.placed_state.id.as_u16(),
            replaced_state_id: self.replaced_state.id.as_u16(),
            placed_against_state_id: self.placed_against.id.as_u16(),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_event(&event, state);
        if let Event::BlockEventEntityPlaceEvent(data) = event {
            self.cancelled = data.cancelled;
        }
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct BlockEventEntityPlaceEvent from WASM")
    }
}

impl ToFromWasmEvent for BlockEventBlockToolModificationEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::BlockEventBlockToolModificationEvent(BlockEventBlockToolModificationEventData {
            target_world: add(state, self.world.clone()),
            block_pos: to_wasm_block_position(self.pos),
            player: self.player.clone().map(|player| add(state, player)),
            held_item: add_item_stack(state, &self.held_item),
            item_ability: self.item_ability.to_string(),
            simulated: self.simulated,
            state_id: self.state.id.as_u16(),
            final_state_id: self.final_state.map(|state| state.id.as_u16()),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_event(&event, state);
        if let Event::BlockEventBlockToolModificationEvent(data) = event {
            self.final_state = data.final_state_id.map(state_from_wasm);
            self.cancelled = data.cancelled;
        }
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct BlockEventBlockToolModificationEvent from WASM")
    }
}

impl ToFromWasmEvent for CropGrowEventPost {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::CropGrowEventPost(CropGrowEventPostData {
            target_world: add(state, self.world.clone()),
            block_pos: to_wasm_block_position(self.pos),
            original_state_id: self.original_state.id.as_u16(),
            state_id: self.state.id.as_u16(),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_event(&event, state);
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct CropGrowEventPost from WASM")
    }
}

impl ToFromWasmEvent for CreateFluidSourceEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::CreateFluidSourceEvent(CreateFluidSourceEventData {
            target_world: add(state, self.world.clone()),
            block_pos: to_wasm_block_position(self.pos),
            state_id: self.state.id.as_u16(),
            vanilla_result: self.vanilla_result,
            can_convert: self.can_convert,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_event(&event, state);
        if let Event::CreateFluidSourceEvent(data) = event {
            self.can_convert = data.can_convert;
        }
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct CreateFluidSourceEvent from WASM")
    }
}

impl ToFromWasmEvent for NoteBlockEventChange {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::NoteBlockEventChange(NoteBlockEventChangeData {
            target_world: add(state, self.world.clone()),
            block_pos: to_wasm_block_position(self.pos),
            state_id: self.state.id.as_u16(),
            old_note: self.old_note,
            note: self.note,
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_event(&event, state);
        if let Event::NoteBlockEventChange(data) = event {
            self.note = data.note;
            self.cancelled = data.cancelled;
        }
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct NoteBlockEventChange from WASM")
    }
}

impl ToFromWasmEvent for PistonEventPost {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PistonEventPost(PistonEventPostData {
            target_world: add(state, self.world.clone()),
            block_pos: to_wasm_block_position(self.pos),
            state_id: self.state.id.as_u16(),
            direction: to_wit_block_direction(self.direction),
            move_type: to_wasm_piston_move_type(self.move_type),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_event(&event, state);
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct PistonEventPost from WASM")
    }
}

impl ToFromWasmEvent for ExplosionEventDetonate {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        let affected_entities = self
            .affected_entities
            .iter()
            .map(|entity| add(state, entity.clone()))
            .collect();
        Event::ExplosionEventDetonate(ExplosionEventDetonateData {
            target_world: add(state, self.world.clone()),
            center: to_wasm_position(self.center),
            power: self.power,
            affected_blocks: self
                .affected_blocks
                .iter()
                .copied()
                .map(to_wasm_block_position)
                .collect(),
            affected_entities,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        let Event::ExplosionEventDetonate(data) = event else {
            cleanup_neoforge_event(&event, state);
            return;
        };
        discard(state, &data.target_world);
        self.affected_blocks = data
            .affected_blocks
            .into_iter()
            .map(super::from_wasm_block_position)
            .collect();
        // The guest hands the entities back: an entity it dropped leaves the list.
        self.affected_entities = data
            .affected_entities
            .into_iter()
            .filter_map(|entity| state.take(entity).ok())
            .collect();
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct ExplosionEventDetonate from WASM")
    }
}

impl ToFromWasmEvent for ExplosionKnockbackEvent {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::ExplosionKnockbackEvent(ExplosionKnockbackEventData {
            target_world: add(state, self.world.clone()),
            center: to_wasm_position(self.center),
            power: self.power,
            entity: add(state, self.entity.clone()),
            knockback: to_wasm_position(self.knockback),
            affected_blocks: self
                .affected_blocks
                .iter()
                .copied()
                .map(to_wasm_block_position)
                .collect(),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_event(&event, state);
        if let Event::ExplosionKnockbackEvent(data) = event {
            self.knockback = from_wasm_position(data.knockback);
        }
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct ExplosionKnockbackEvent from WASM")
    }
}

impl ToFromWasmEvent for ChunkWatchEventUnWatch {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::ChunkWatchEventUnWatch(ChunkWatchEventUnWatchData {
            player: add(state, self.player.clone()),
            target_world: add(state, self.world.clone()),
            chunk_x: self.pos.x,
            chunk_z: self.pos.y,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_event(&event, state);
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct ChunkWatchEventUnWatch from WASM")
    }
}

impl ToFromWasmEvent for GameRuleChangedEvent {
    fn to_wasm_event(&self, _state: &mut PluginHostState) -> Event {
        Event::GameRuleChangedEvent(GameRuleChangedEventData {
            rule: to_wit_game_rule(&self.rule),
            value: to_wit_game_rule_value(&self.value),
        })
    }

    fn apply_wasm_event(&mut self, _event: Event, _state: &mut PluginHostState) {}

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct GameRuleChangedEvent from WASM")
    }
}

impl ToFromWasmEvent for LevelEventPotentialSpawns {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::LevelEventPotentialSpawns(LevelEventPotentialSpawnsData {
            target_world: add(state, self.world.clone()),
            category: to_wasm_mob_category(self.category),
            block_pos: to_wasm_block_position(self.pos),
            spawns: self
                .spawns
                .iter()
                .map(|spawner| SpawnerData {
                    entity_type: spawner.r#type.to_string(),
                    min_count: spawner.min_count,
                    max_count: spawner.max_count,
                })
                .collect(),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_event(&event, state);
        if let Event::LevelEventPotentialSpawns(data) = event {
            self.spawns = data
                .spawns
                .iter()
                .take(MAX_GUEST_SPAWNS)
                .filter_map(|spawner| from_wasm_spawner(&self.spawns, self.category, spawner))
                .collect();
            self.cancelled = data.cancelled;
        }
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct LevelEventPotentialSpawns from WASM")
    }
}

impl ToFromWasmEvent for EntityTickEventPre {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EntityTickEventPre(EntityTickEventPreData {
            entity: add(state, self.entity.clone()),
            cancelled: self.cancelled,
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_event(&event, state);
        if let Event::EntityTickEventPre(data) = event {
            self.cancelled = data.cancelled;
        }
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct EntityTickEventPre from WASM")
    }
}

impl ToFromWasmEvent for EntityTickEventPost {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::EntityTickEventPost(EntityTickEventPostData {
            entity: add(state, self.entity.clone()),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_event(&event, state);
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct EntityTickEventPost from WASM")
    }
}

impl ToFromWasmEvent for PlayerTickEventPre {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerTickEventPre(PlayerTickEventPreData {
            player: add(state, self.player.clone()),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_event(&event, state);
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct PlayerTickEventPre from WASM")
    }
}

impl ToFromWasmEvent for PlayerTickEventPost {
    fn to_wasm_event(&self, state: &mut PluginHostState) -> Event {
        Event::PlayerTickEventPost(PlayerTickEventPostData {
            player: add(state, self.player.clone()),
        })
    }

    fn apply_wasm_event(&mut self, event: Event, state: &mut PluginHostState) {
        cleanup_neoforge_event(&event, state);
    }

    fn from_wasm_event(_event: Event, _state: &mut PluginHostState) -> Self {
        panic!("Cannot construct PlayerTickEventPost from WASM")
    }
}

/// Discards the resources of a returned event of this module.
pub(super) fn cleanup_neoforge_event(event: &Event, state: &mut PluginHostState) {
    match event {
        Event::UseItemOnBlockEvent(data) => {
            discard(state, &data.player);
            discard(state, &data.item);
        }
        Event::BlockEventEntityPlaceEvent(data) => {
            discard(state, &data.entity);
            discard(state, &data.target_world);
        }
        Event::BlockEventBlockToolModificationEvent(data) => {
            discard(state, &data.target_world);
            if let Some(player) = &data.player {
                discard(state, player);
            }
            discard(state, &data.held_item);
        }
        Event::CropGrowEventPost(CropGrowEventPostData { target_world, .. })
        | Event::CreateFluidSourceEvent(CreateFluidSourceEventData { target_world, .. })
        | Event::NoteBlockEventChange(NoteBlockEventChangeData { target_world, .. })
        | Event::PistonEventPost(PistonEventPostData { target_world, .. })
        | Event::LevelEventPotentialSpawns(LevelEventPotentialSpawnsData {
            target_world, ..
        }) => discard(state, target_world),
        Event::ExplosionEventDetonate(data) => {
            discard(state, &data.target_world);
            for entity in &data.affected_entities {
                discard(state, entity);
            }
        }
        Event::ExplosionKnockbackEvent(data) => {
            discard(state, &data.target_world);
            discard(state, &data.entity);
        }
        Event::ChunkWatchEventUnWatch(data) => {
            discard(state, &data.player);
            discard(state, &data.target_world);
        }
        Event::EntityTickEventPre(EntityTickEventPreData { entity, .. })
        | Event::EntityTickEventPost(EntityTickEventPostData { entity }) => discard(state, entity),
        Event::PlayerTickEventPre(PlayerTickEventPreData { player })
        | Event::PlayerTickEventPost(PlayerTickEventPostData { player }) => discard(state, player),
        _ => {}
    }
}

/// Registers a Wasm handler for an event of this module.
pub(crate) fn register_neoforge_event(
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
        EventType::UseItemOnBlockEvent => {
            on::<UseItemOnBlockEvent>(ctx, handler, priority, blocking);
        }
        EventType::BlockEventEntityPlaceEvent => {
            on::<BlockEventEntityPlaceEvent>(ctx, handler, priority, blocking);
        }
        EventType::BlockEventBlockToolModificationEvent => {
            on::<BlockEventBlockToolModificationEvent>(ctx, handler, priority, blocking);
        }
        EventType::CropGrowEventPost => on::<CropGrowEventPost>(ctx, handler, priority, blocking),
        EventType::CreateFluidSourceEvent => {
            on::<CreateFluidSourceEvent>(ctx, handler, priority, blocking);
        }
        EventType::NoteBlockEventChange => {
            on::<NoteBlockEventChange>(ctx, handler, priority, blocking);
        }
        EventType::PistonEventPost => on::<PistonEventPost>(ctx, handler, priority, blocking),
        EventType::ExplosionEventDetonate => {
            on::<ExplosionEventDetonate>(ctx, handler, priority, blocking);
        }
        EventType::ExplosionKnockbackEvent => {
            on::<ExplosionKnockbackEvent>(ctx, handler, priority, blocking);
        }
        EventType::ChunkWatchEventUnWatch => {
            on::<ChunkWatchEventUnWatch>(ctx, handler, priority, blocking);
        }
        EventType::GameRuleChangedEvent => {
            on::<GameRuleChangedEvent>(ctx, handler, priority, blocking);
        }
        EventType::LevelEventPotentialSpawns => {
            on::<LevelEventPotentialSpawns>(ctx, handler, priority, blocking);
        }
        EventType::EntityTickEventPre => {
            on::<EntityTickEventPre>(ctx, handler, priority, blocking);
        }
        EventType::EntityTickEventPost => {
            on::<EntityTickEventPost>(ctx, handler, priority, blocking);
        }
        EventType::PlayerTickEventPre => {
            on::<PlayerTickEventPre>(ctx, handler, priority, blocking);
        }
        EventType::PlayerTickEventPost => {
            on::<PlayerTickEventPost>(ctx, handler, priority, blocking);
        }
        _ => tracing::error!("non-NeoForge event should not be routed to register_neoforge_event"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wit_mob_categories_match_internal_mob_categories() {
        let cases = [
            (&MobCategory::MONSTER, WitMobCategory::Monster),
            (&MobCategory::CREATURE, WitMobCategory::Creature),
            (&MobCategory::AMBIENT, WitMobCategory::Ambient),
            (&MobCategory::AXOLOTLS, WitMobCategory::Axolotls),
            (
                &MobCategory::UNDERGROUND_WATER_CREATURE,
                WitMobCategory::UndergroundWaterCreature,
            ),
            (&MobCategory::WATER_CREATURE, WitMobCategory::WaterCreature),
            (&MobCategory::WATER_AMBIENT, WitMobCategory::WaterAmbient),
            (&MobCategory::MISC, WitMobCategory::Misc),
        ];
        for category in MobCategory::SPAWNING_CATEGORIES {
            assert!(cases.iter().any(|(case, _)| case.id == category.id));
        }
        for (category, wit) in cases {
            assert_eq!(to_wasm_mob_category(category), wit, "{}", category.id);
        }
    }
}
