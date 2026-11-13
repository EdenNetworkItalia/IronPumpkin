use super::{
    Arc, Block, BlockBreakEvent, BlockFlags, BlockPos, BlockState, BlockStateId, CLevelEvent,
    CSoundEffect, CWorldEvent, Entity, EntityBase, ItemStack, LevelEvent, NbtCompound, Player,
    SoundCategory, StructureKeys, StructureStart, VarInt, Vector3, World, WorldEvent, cause_id,
    load_entity_from_nbt, self_and_passengers,
};

impl World {
    /// Plays a sound event by name to the players within its range of `position`, like
    /// `ServerLevel.playSeededSound` with a direct holder, and returns how many players got it.
    ///
    /// The range is `fixed_range`, or 16 blocks times a volume above 1, like `SoundEvent.getRange`.
    /// `PlayerList.broadcast` sends to the players closer than the range.
    pub fn play_sound_event_near(
        &self,
        sound_name: &str,
        fixed_range: Option<f32>,
        category: SoundCategory,
        position: &Vector3<f64>,
        volume: f32,
        pitch: f32,
    ) -> usize {
        let range =
            f64::from(fixed_range.unwrap_or(if volume > 1.0 { 16.0 * volume } else { 16.0 }));
        let packet = CSoundEffect::new(
            pumpkin_protocol::IdOr::Value(pumpkin_protocol::SoundEvent {
                sound_name: sound_name.into(),
                range: fixed_range,
            }),
            category,
            position,
            volume,
            pitch,
            rand::random::<i64>(),
        );
        let players = self.players.load();
        let recipients: Vec<&Arc<Player>> = players
            .iter()
            .filter(|player| {
                player
                    .living_entity
                    .entity
                    .pos
                    .load()
                    .squared_distance_to_vec(position)
                    < range * range
            })
            .collect();
        let count = recipients.len();
        let recipients_by_version =
            Self::collect_java_recipients_by_version(recipients.into_iter());
        Self::broadcast_java_grouped(&packet, recipients_by_version);
        count
    }

    /// Makes the entities of a record of a loaded chunk live, the root and its passengers, and
    /// returns them. A record without a known type gives none, and an entity whose
    /// [`EntitySpawnEvent`] a handler cancelled stays out.
    ///
    /// [`EntitySpawnEvent`]: crate::plugin::api::events::entity::entity_spawn::EntitySpawnEvent
    pub fn load_chunk_entity(
        self: &Arc<Self>,
        entity_nbt: &NbtCompound,
    ) -> Vec<Arc<dyn EntityBase>> {
        // Pos is zero since it will be read from nbt.
        let Some(root) = load_entity_from_nbt(entity_nbt, Vector3::default(), self) else {
            return Vec::new();
        };
        let mut loaded = Vec::new();
        for entity in self_and_passengers(&root) {
            match entity.get_mob() {
                Some(mob) => mob.mob_init_data_tracker(),
                None => entity.init_data_tracker(),
            }

            // Clear velocity so the client does not replay the drop
            // animation; residual velocity from the original drop is
            // stale data.
            entity.get_entity().velocity.store(Vector3::default());

            // NeoForge fires `EntityJoinLevelEvent` for each loaded entity, passengers too, and a
            // cancel keeps it out of the world, so its chunk no longer saves it.
            if self.spawn_cancelled(entity.as_ref(), true) {
                continue;
            }

            // UUID-dedupes if another watcher already loaded this entity.
            // Tracker owns pairing (spawn packets + vehicle restore).
            self.add_entity_silent(entity.clone());
            loaded.push(entity);
        }
        loaded
    }

    /// Fires [`EntitySpawnEvent`] for `entity` and returns whether a handler cancelled it.
    ///
    /// [`EntitySpawnEvent`]: crate::plugin::api::events::entity::entity_spawn::EntitySpawnEvent
    pub(super) fn spawn_cancelled(
        self: &Arc<Self>,
        entity: &dyn EntityBase,
        loaded_from_disk: bool,
    ) -> bool {
        let Some(server) = self.server.upgrade() else {
            return false;
        };
        server
            .plugin_manager
            .fire_blocking_with(&server, || {
                let base = entity.get_entity();
                let mut event =
                    crate::plugin::api::events::entity::entity_spawn::EntitySpawnEvent::new(
                        base.entity_id,
                        base.entity_type.id.to_string(),
                        base.pos.load(),
                        self.clone(),
                    );
                event.loaded_from_disk = loaded_from_disk;
                event
            })
            .is_some_and(|event| event.cancelled)
    }

    /// The particles and the sound of the break of `state_id` at `position`, for each edition
    /// (world event 2001).
    pub(super) fn destroy_block_effect(
        position: &BlockPos,
        state_id: BlockStateId,
    ) -> (CWorldEvent, CLevelEvent) {
        (
            CWorldEvent::new(
                WorldEvent::ParticlesAndSoundDestroyBlock as i32,
                *position,
                state_id.as_u16().into(),
                false,
            ),
            CLevelEvent {
                event_id: VarInt(LevelEvent::ParticlesDestroyBlock as i32),
                position: position.to_centered_f64().to_f32_lossy(),
                data: VarInt(BlockState::to_be_network_id(state_id) as i32),
            },
        )
    }

    /// Breaks the block at `position` as vanilla `Level.destroyBlock` does: a break without a
    /// player dig, by a projectile, a mob or a machine. Fires [`BlockBreakEvent`] with the player
    /// behind `breaker`: the breaker itself, or the player that owns it, as the shooter of a
    /// projectile. A block entity passes no breaker. Returns `false` when the block is air or a
    /// handler cancels the break.
    pub fn destroy_block(
        self: &Arc<Self>,
        position: &BlockPos,
        drop: bool,
        breaker: Option<&dyn EntityBase>,
    ) -> bool {
        let (block, state) = self.get_block_and_state(position);
        if state.is_air() {
            return false;
        }

        let event = self.server.upgrade().and_then(|server| {
            server.plugin_manager.fire_blocking_with(&server, || {
                let player = breaker.and_then(|breaker| self.get_player_by_id(cause_id(breaker)));
                let exp = if drop {
                    crate::block::experience_to_drop(block, None)
                } else {
                    0
                };
                BlockBreakEvent::new(player, block, state, *position, exp, drop)
            })
        });
        // Without a handler the experience is `None` and `drop_loot` rolls it.
        let (drop, exp) = match event {
            Some(event) if event.cancelled => return false,
            Some(event) => (event.drop, Some(event.exp)),
            None => (drop, None),
        };

        // Vanilla skips the break effect for a `BaseFireBlock`.
        if block != &Block::FIRE && block != &Block::SOUL_FIRE {
            let (je_packet, be_packet) = Self::destroy_block_effect(position, state.id);
            self.broadcast_to_chunk_editioned(position.chunk_position(), &je_packet, &be_packet);
        }
        if drop {
            let params = crate::world::loot::LootContextParameters {
                block_state: Some(state),
                position: Some(position.to_f64()),
                this_entity: breaker,
                ..Default::default()
            };
            crate::block::drop_loot(self, block, position, exp.is_none(), &params);
            if let Some(exp) = exp {
                crate::block::drop_experience(self, position, exp);
            }
        }
        // `getFluidState(pos).createLegacyBlock()`: kelp, seagrass and bubble columns leave water.
        let (_, fluid_state) = self.get_fluid_and_fluid_state(position);
        self.set_block_state(position, fluid_state.block_state_id, BlockFlags::NOTIFY_ALL);
        self.emit_game_event(
            pumpkin_data::game_event::GameEvent::BlockDestroy.name(),
            position.to_centered_f64(),
        );
        true
    }

    /// Places `state_id` at `position` for `placer`, as `NeoForge` `EventHooks.onBlockPlace` before
    /// `Level.setBlock`. The cause is the placer itself, or the entity that owns it: a player
    /// fires [`BlockPlaceEvent`], another entity [`BlockEventEntityPlaceEvent`]. Returns `false`
    /// when a handler cancels the place.
    ///
    /// [`BlockPlaceEvent`]: crate::plugin::api::events::block::block_place::BlockPlaceEvent
    /// [`BlockEventEntityPlaceEvent`]: crate::plugin::api::events::level::BlockEventEntityPlaceEvent
    pub fn place_block(
        self: &Arc<Self>,
        position: &BlockPos,
        state_id: BlockStateId,
        placer: &dyn EntityBase,
    ) -> bool {
        if let Some(server) = self.server.upgrade() {
            let cause = cause_id(placer);
            let placed = self.get_player_by_id(cause).map_or_else(
                || {
                    !server
                        .plugin_manager
                        .has_handlers::<crate::plugin::api::events::level::BlockEventEntityPlaceEvent>()
                        || self
                            .get_entity_by_id(cause)
                            .or_else(|| self.get_entity_by_id(placer.get_entity().entity_id))
                            .is_none_or(|entity| {
                                crate::plugin::level_events::entity_place(
                                    self, &entity, *position, state_id,
                                )
                            })
                },
                |player| {
                    crate::plugin::level_events::player_place_one(
                        &server,
                        self,
                        &player,
                        Block::from_state_id(state_id),
                        self.get_block(&position.down()),
                        *position,
                        state_id,
                    )
                },
            );
            if !placed {
                return false;
            }
        }
        self.set_block_state(position, state_id, BlockFlags::NOTIFY_ALL);
        true
    }

    /// The structure starts whose bounding box contains `pos`, as vanilla
    /// `StructureManager.startsForStructure` and `BoundingBox.isInside` select them.
    ///
    /// Loads or generates the chunk that holds `pos` and the chunks of the starts it
    /// references, like vanilla. For commands and one-off queries, not for tick or worldgen
    /// paths: each fetch loads or generates up to 121 chunks, see [`Level::structure_starts_at`].
    pub async fn structure_starts_at(&self, pos: &BlockPos) -> Vec<StructureStart> {
        self.level.structure_starts_at(pos).await
    }

    /// Whether a start of `structure` contains `pos`: vanilla
    /// `StructureManager.getStructureAt(pos, structure).isValid()`. Loads chunks like
    /// [`Self::structure_starts_at`]: for commands and one-off queries, not for tick or
    /// worldgen paths.
    pub async fn is_inside_structure(&self, pos: &BlockPos, structure: StructureKeys) -> bool {
        self.structure_starts_at(pos)
            .await
            .iter()
            .any(|start| start.structure == structure)
    }

    /// Fires [`ItemSpawnEvent`] for the item entity `entity` of `stack` and returns whether a
    /// handler cancelled it.
    ///
    /// [`ItemSpawnEvent`]: crate::plugin::api::events::entity::item_spawn::ItemSpawnEvent
    pub(super) fn item_spawn_cancelled(&self, entity: &Entity, stack: &ItemStack) -> bool {
        let Some(server) = self.server.upgrade() else {
            return false;
        };
        server
            .plugin_manager
            .fire_blocking_with(&server, || {
                crate::plugin::api::events::entity::item_spawn::ItemSpawnEvent::new(
                    entity.entity_id,
                    entity.pos.load(),
                    stack.item.registry_key.to_string(),
                )
            })
            .is_some_and(|event| event.cancelled)
    }
}
