# Mapping table

This table maps the NeoForge 26.3 API to its counterpart in `ironpumpkin-mods` (the native API for mods) or in Pumpkin. It is the porting guide: a mod author looks up the NeoForge class and finds what to use. IronPumpkin takes the semantics and the catalogue of NeoForge, not its shape (design.md, section "NeoForge semantics, Pumpkin shape"). It has two parts: every server-side row of the inventory in design.md (#34), and the modpack coverage section, which ranks the gaps of the reference modpack scan (`modpack-usage.md`, #46) with a verdict each and maps the Minecraft, NeoForge and Mojang classes outside the inventory that mods use.

How to read it:

- The counterpart column names the `ironpumpkin-mods` item (a Rust path, for example `ModInit::register_block`) or the Pumpkin event (for example `pumpkin_core::plugin::block::BlockBreakEvent`). Until the task that implements a row lands, the counterpart is written in interface notation, kebab-case: `interface.item` (for example `registration.deferred-register.register`) or `interface.variant::case`. The Rust item of the finished API replaces the notation in that task. Every row keeps a test or an explicit "unsupported". `pumpkin:plugin/x` is an interface of the v0.2 package.
- `callbacks.x` names code that NeoForge calls through a Java interface or lambda: a method of the trait object (or a closure) that the mod registers, the handler the mod registered. `callbacks.handle-payload` is the payload handler the mod registered for the channel.
- Status is the inventory status: `supported` works today, `planned` has a counterpart and waits for its backing phase, `not supported` has no counterpart and the reason is in the reason column of its inventory row in design.md.
- "gap" marks a used class or a planned row without a counterpart; the reason follows in the row, and "`x` doc" names the interface of the API that owns the gap. "partial" marks an extension interface whose methods a mod can call through the named counterpart but cannot override yet.
- **Mods** is the number of the 414 mods of the reference modpack (FTB StoneBlock 4) that have a server-side or mixed class that needs the row, from section 11.5 of `modpack-usage.md`. The scan read NeoForge 21.1 jars and matches rows by simple name, so a row whose 26.3 class has no 21.1 counterpart shows 0. Where section 10.3 or 11.3 of the scan lists the 21.1 name of such a class, the row shows the count of the old name and names it, for example 52 (21.1 `BlockEvent$BreakEvent`). Rows are ordered by Mods, highest first; rows with the same count keep the inventory order.
- Client-only rows (235 in the inventory) and dev-only classes (data generation and game tests) never run on a dedicated server and are not listed.
- A `not supported` row with a "contract only" counterpart: the API defines it so a port compiles against the final shape, and the host rejects the registration until a phase backs it.
- M2 to M5 are the milestones: M2 Content registry, M3 NeoForge handshake and registry sync, M4 Native API with NeoForge semantics, M5 Modded gameplay parity.
- Task numbers (task 2.x) are the items of `tasks.md` in this change.
- The status of a row follows design.md. Where the two differ, design.md is right. The command `python3 -I tools/openspec-sync/mapping_status.py` copies the statuses from design.md into the Status column and recomputes the first three rows of the Coverage table. Run it after each change to a status in design.md. `--check` exits 1 when the files differ. Do not edit these cells by hand.
- The M4 implementation tasks check each supported row with a test, as design.md requires.

## Coverage

| Part | Rows | Counterpart | Partial | Contract only | Gap or not supported | Used by 1 or more mods | Used by 50 or more mods |
|:--|--:|--:|--:|--:|--:|--:|--:|
| Inventory, supported | 477 | 477 | 0 | 0 | 0 | 391 | 93 |
| Inventory, planned | 475 | 470 | 3 | 2 | 0 | 293 | 17 |
| Inventory, not supported | 221 | 0 | 0 | 1 | 220 | 95 | 14 |
| Modpack classes outside the inventory, NeoForge | 104 | 94 | 0 | 0 | 10 | 102 | 44 |
| Modpack classes outside the inventory, Minecraft and Mojang | 314 | 251 | 0 | 0 | 63 | 309 | 174 |

## Inventory rows

### Events

| NeoForge item | Mods | Section | Counterpart | Status |
|:--|--:|:--|:--|:--|
| `RegisterPayloadHandlersEvent` | 170 | event class | `ironpumpkin_mods::network::RegisterPayloadHandlersEvent` (a Pumpkin event; `registrar` gives a `PayloadRegistrar`) | supported |
| `FMLCommonSetupEvent` | 148 | event class | `NativeMod::common_setup` (runs after the last `init`, with the Local configs loaded) | supported |
| `RegisterCapabilitiesEvent` | 131 | event class | `NativeMod::register_capabilities` with `ironpumpkin_mods::capabilities::RegisterCapabilitiesEvent` (once per `EventPriority` phase) | supported |
| `PlayerEvent` | 96 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `RegisterCommandsEvent` | 93 | event class | `ModInit::register_command` (API call, no event) | supported |
| `ModConfigEvent` | 86 | event class | `NativeMod::config_loading`, `config_reloading`, `config_unloading` (three hooks, no parent handler) | supported |
| `PlayerInteractEvent` | 85 | event class | no case (abstract); fields in each subclass record of `event-bus` | supported |
| `BuildCreativeModeTabContentsEvent` | 75 | event class | `event-bus` doc, not supported | not supported |
| `PlayerInteractEvent.RightClickBlock` | 67 | event class | `pumpkin_core::plugin::player::PlayerInteractEvent` (action `RightClickBlock`, built with `right_click_block`) | supported |
| `PlayerEvent.PlayerLoggedInEvent` | 66 | event class | `event-bus.event::player-event-player-logged-in-event` | supported |
| `BlockEvent` | 65 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `BreakBlockEvent` | 52 (21.1 `BlockEvent$BreakEvent`) | event class | `pumpkin_core::plugin::block::BlockBreakEvent` (fired by `World::break_block` and `World::destroy_block`; a break without a player and a `Level.destroyBlock` have no NeoForge event) | supported |
| `ServerTickEvent` | 52 | event class | no case (abstract); fields in each subclass record of `event-bus` | supported |
| `EntityJoinLevelEvent` | 49 | event class | `event-bus.event::entity-join-level-event` | supported |
| `ModConfigEvent.Reloading` | 48 | event class | `NativeMod::config_reloading` (after a poll reload and after `ModConfig::save`; a `save` inside the hook does not call it again) | supported |
| `LivingDeathEvent` | 46 | event class | `event-bus.event::living-death-event` | supported |
| `RegisterEvent` | 46 | event class | `pumpkin_core::plugin::registries::RegisterEvent`; a handler registers through `content::RegisterEventExt` | supported |
| `ServerTickEvent.Post` | 43 | event class | `pumpkin_core::plugin::server::ServerTickEndEvent` | supported |
| `ModConfigEvent.Loading` | 43 | event class | `NativeMod::config_loading` (Startup inside `ModInit::register_config`, Local before `NativeMod::common_setup`, Synced after `NativeMod::load_complete`) | supported |
| `LevelEvent` | 42 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `AddServerReloadListenersEvent` | 41 (21.1 `AddReloadListenerEvent`) | event class | `event-bus.event::add-server-reload-listeners-event` (resource `lifecycle.add-server-reload-listeners-event`) | planned |
| `ServerStartedEvent` | 40 | event class | `pumpkin_core::plugin::server::ServerLoadEvent` | supported |
| `LevelTickEvent` | 40 | event class | no case (abstract); fields in each subclass record of `event-bus` | supported |
| `LivingDamageEvent` | 38 | event class | no case (abstract); fields in each subclass record of `event-bus` | supported |
| `LivingIncomingDamageEvent` | 37 | event class | `event-bus.event::living-incoming-damage-event` | supported |
| `ServerStoppedEvent` | 36 | event class | `pumpkin_core::plugin::server::ServerStoppedEvent` | supported |
| `PlayerTickEvent` | 36 | event class | no case (abstract); fields in each subclass record of `event-bus` | supported |
| `LevelTickEvent.Post` | 32 | event class | `pumpkin_core::plugin::world::LevelTickEventPost` (fires while the game is frozen too; the worlds tick in parallel) | supported |
| `LivingDropsEvent` | 31 | event class | `event-bus.event::living-drops-event` | supported |
| `InterModEnqueueEvent` | 31 | event class | `NativeMod::inter_mod_enqueue` (`imc::InterModEnqueue::send`) | supported |
| `NewRegistryEvent` | 31 | event class | `event-bus` doc, not supported | planned |
| `LevelEvent.Unload` | 30 | event class | `event-bus.event::level-event-unload` | supported |
| `OnDatapackSyncEvent` | 29 | event class | `event-bus.event::on-datapack-sync-event` | planned |
| `EntityAttributeCreationEvent` | 29 | event class | `NativeMod::entity_attribute_creation` with `registration::EntityAttributeCreationEvent` | supported |
| `ItemTooltipEvent` | 28 | event class | `event-bus` doc, not supported | not supported |
| `PlayerEvent.PlayerLoggedOutEvent` | 28 | event class | `event-bus.event::player-event-player-logged-out-event` | supported |
| `LivingDamageEvent.Pre` | 27 | event class | `pumpkin_core::plugin::living::LivingDamageEventPre` | supported |
| `PlayerInteractEvent.LeftClickBlock` | 27 | event class | `pumpkin_core::plugin::player::PlayerInteractEvent` (action `LeftClickBlock`, `left_click_action`) | supported |
| `LevelEvent.Load` | 27 | event class | `event-bus.event::level-event-load` | supported |
| `EntityTickEvent` | 27 | event class | no case (abstract); fields in each subclass record of `event-bus` | supported |
| `PlayerEvent.PlayerChangedDimensionEvent` | 26 | event class | `event-bus.event::player-event-player-changed-dimension-event` | supported |
| `ServerStartingEvent` | 26 | event class | `pumpkin_core::plugin::server::ServerStartingEvent` | supported |
| `LivingDamageEvent.Post` | 25 | event class | `pumpkin_core::plugin::living::LivingDamageEventPost` | supported |
| `PlayerEvent.PlayerRespawnEvent` | 24 | event class | `event-bus.event::player-event-player-respawn-event` | supported |
| `ServerAboutToStartEvent` | 24 | event class | `pumpkin_core::plugin::server::ServerAboutToStartEvent` | supported |
| `ServerStoppingEvent` | 24 | event class | `pumpkin_core::plugin::server::ServerStoppingEvent` (fires after the ticker thread exits, with a 30 s bound) | supported |
| `PlayerTickEvent.Post` | 24 | event class | `pumpkin_core::plugin::level::PlayerTickEventPost` | supported |
| `FMLLoadCompleteEvent` | 24 | event class | `NativeMod::load_complete` (the Synced configs load right after it) | supported |
| `PlayerEvent.Clone` | 23 | event class | `event-bus.event::player-event-clone` | planned |
| `PlayerInteractEvent.RightClickItem` | 23 | event class | `pumpkin_core::plugin::player::PlayerInteractEvent` (action `RightClickAir`, built with `right_click_item`) | supported |
| `BlockEvent.EntityPlaceEvent` | 23 | event class | `pumpkin_core::plugin::block::BlockPlaceEvent` (a player), `pumpkin_core::plugin::level::BlockEventEntityPlaceEvent` (another entity) | supported |
| `EntityTeleportEvent` | 22 | event class | `event-bus.event::entity-teleport-event` | supported |
| `LivingEvent` | 22 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `ItemEntityPickupEvent` | 20 | event class | no case (abstract); fields in each subclass record of `event-bus` | supported |
| `TagsUpdatedEvent` | 19 | event class | `event-bus.event::tags-updated-event` | planned |
| `LevelTickEvent.Pre` | 19 | event class | `pumpkin_core::plugin::world::LevelTickEventPre` (fires while the game is frozen too; the worlds tick in parallel) | supported |
| `PlayerEvent.BreakSpeed` | 18 | event class | `pumpkin_core::plugin::player::PlayerEventBreakSpeed` | supported |
| `EntityTickEvent.Post` | 18 | event class | `pumpkin_core::plugin::level::EntityTickEventPost` | supported |
| `PlayerTickEvent.Pre` | 18 | event class | `pumpkin_core::plugin::level::PlayerTickEventPre` | supported |
| `RegisterSpawnPlacementsEvent` | 17 | event class | `NativeMod::register_spawn_placements` with `registration::RegisterSpawnPlacementsEvent` | supported |
| `ItemEntityPickupEvent.Pre` | 17 | event class | `pumpkin_core::plugin::player::ItemEntityPickupEventPre` (`can_pickup: TriState`) | supported |
| `BlockDropsEvent` | 17 | event class | `event-bus.event::block-drops-event` | supported |
| `ChunkEvent` | 17 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `ServerTickEvent.Pre` | 17 | event class | `pumpkin_core::plugin::server::ServerTickStartEvent` | supported |
| `EntityAttributeModificationEvent` | 16 | event class | `NativeMod::entity_attribute_modification` with `registration::EntityAttributeModificationEvent` | supported |
| `FinalizeSpawnEvent` | 16 | event class | `event-bus.event::finalize-spawn-event` | supported |
| `LivingEntityUseItemEvent` | 16 | event class | `pumpkin_core::plugin::living::LivingEntityUseItemEvent` (concrete parent of `LivingEntityUseItemEventStart`, `Tick` and `Stop`) | supported |
| `PlayerInteractEvent.EntityInteract` | 16 | event class | `event-bus.event::player-interact-event-entity-interact` | supported |
| `RegisterDataMapTypesEvent` | 16 | event class | `NativeMod::register_data_map_types` with `ironpumpkin_mods::data_maps::RegisterDataMapTypesEvent` | supported |
| `LivingEvent.LivingJumpEvent` | 15 | event class | `pumpkin_core::plugin::living::LivingEventLivingJumpEvent` | supported |
| `AttackEntityEvent` | 15 | event class | `event-bus.event::attack-entity-event` | supported |
| `ChunkEvent.Unload` | 15 | event class | `pumpkin_core::plugin::world::ChunkUnloadEvent` | supported |
| `LivingChangeTargetEvent` | 14 | event class | `event-bus.event::living-change-target-event` | supported |
| `LivingFallEvent` | 14 | event class | `pumpkin_core::plugin::living::LivingFallEvent` | supported |
| `AddPackFindersEvent` | 13 | event class | `event-bus.event::add-pack-finders-event` (resource `lifecycle.add-pack-finders-event`) | planned |
| `LivingExperienceDropEvent` | 13 | event class | `pumpkin_core::plugin::living::LivingExperienceDropEvent` | supported |
| `LivingHealEvent` | 13 | event class | `event-bus.event::living-heal-event` | supported |
| `MobEffectEvent` | 13 | event class | no case (abstract); fields in each subclass record of `event-bus` | supported |
| `PlayerEvent.StartTracking` | 13 | event class | `pumpkin_core::plugin::player::PlayerEventStartTracking` | supported |
| `ProjectileImpactEvent` | 12 | event class | `pumpkin_core::plugin::level::ProjectileImpactEvent` | supported |
| `ExplosionEvent` | 12 | event class | no case (abstract); fields in each subclass record of `event-bus` | supported |
| `EntityTickEvent.Pre` | 12 | event class | `pumpkin_core::plugin::level::EntityTickEventPre` | supported |
| `ItemAttributeModifierEvent` | 11 | event class | `event-bus.event::item-attribute-modifier-event` | planned |
| `MobSpawnEvent` | 11 | event class | no case (abstract); fields in each subclass record of `event-bus` | supported |
| `PlayerEvent.ItemCraftedEvent` | 11 | event class | `event-bus.event::player-event-item-crafted-event` | supported |
| `ChunkWatchEvent` | 11 | event class | no case (abstract); fields in each subclass record of `event-bus` | supported |
| `EntityEvent` | 10 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `LivingEquipmentChangeEvent` | 10 | event class | `event-bus.event::living-equipment-change-event` | planned |
| `PlayerContainerEvent` | 10 | event class | `pumpkin_core::plugin::player::PlayerContainerEvent` (concrete parent of `InventoryOpenEvent` and `InventoryCloseEvent`) | supported |
| `ChunkEvent.Load` | 10 | event class | `pumpkin_core::plugin::world::ChunkLoad` | supported |
| `ExplosionEvent.Detonate` | 10 | event class | `pumpkin_core::plugin::level::ExplosionEventDetonate` | supported |
| `InterModProcessEvent` | 10 | event class | `NativeMod::inter_mod_process` (`imc::InterModProcess::get_messages`, `imc::ImcMessage`) | supported |
| `RegisterTicketControllersEvent` | 10 | event class | `event-bus` doc, not supported | planned |
| `AnvilUpdateEvent` | 9 | event class | `pumpkin_core::plugin::inventory::PrepareAnvilEvent` (fires outside the screen lock on every result update; applies if the inputs did not change) | supported |
| `EntityInvulnerabilityCheckEvent` | 9 | event class | `pumpkin_core::plugin::level::EntityInvulnerabilityCheckEvent` | supported |
| `EntityLeaveLevelEvent` | 9 | event class | `pumpkin_core::plugin::level::EntityLeaveLevelEvent` | supported |
| `EntityMountEvent` | 9 | event class | `event-bus.event::entity-mount-event` | supported |
| `LivingEntityUseItemEvent.Finish` | 9 | event class | `event-bus.event::living-entity-use-item-event-finish` | supported |
| `LivingEntityUseItemEvent.Start` | 9 | event class | `pumpkin_core::plugin::living::LivingEntityUseItemEventStart` | supported |
| `LivingKnockBackEvent` | 9 | event class | `pumpkin_core::plugin::living::LivingKnockBackEvent` | supported |
| `ItemEntityPickupEvent.Post` | 9 | event class | `pumpkin_core::plugin::player::ItemEntityPickupEventPost` | supported |
| `PlayerContainerEvent.Open` | 9 | event class | `pumpkin_core::plugin::inventory::inventory_open::InventoryOpenEvent` | supported |
| `BlockEntityTypeAddBlocksEvent` | 8 | event class | `NativeMod::block_entity_type_add_blocks` with `registration::BlockEntityTypeAddBlocksEvent` | supported |
| `ItemTossEvent` | 8 | event class | `event-bus.event::item-toss-event` | supported |
| `LivingShieldBlockEvent` | 8 | event class | `pumpkin_core::plugin::living::LivingShieldBlockEvent` | supported |
| `MobEffectEvent.Added` | 8 | event class | `event-bus.event::mob-effect-event-added` | supported |
| `LootTableLoadEvent` | 7 | event class | `event-bus.event::loot-table-load-event` | planned |
| `MobEffectEvent.Applicable` | 7 | event class | `pumpkin_core::plugin::living::MobEffectEventApplicable` | supported |
| `MobSpawnEvent.PositionCheck` | 7 | event class | `pumpkin_core::plugin::level::MobSpawnEventPositionCheck` | supported |
| `AdvancementEvent` | 7 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `PlayerInteractEvent.LeftClickEmpty` | 7 | event class | `event-bus` doc, not supported | not supported |
| `PlayerXpEvent` | 7 | event class | no case (abstract); fields in each subclass record of `event-bus` | supported |
| `BlockEvent.BlockToolModificationEvent` | 7 | event class | `pumpkin_core::plugin::level::BlockEventBlockToolModificationEvent` (fire starters and shears) | supported |
| `RegisterConfigurationTasksEvent` | 7 | event class | `ironpumpkin_mods::network::RegisterConfigurationTasksEvent` (a Pumpkin event; `register` takes a `CustomConfigurationTask`) | supported |
| `ModifyDefaultComponentsEvent` | 6 | event class | `event-bus` doc, not supported | planned |
| `ServerChatEvent` | 6 | event class | `pumpkin_core::plugin::player::PlayerChatEvent` (`raw_text` is filled only when a handler was registered at creation) | supported |
| `EntityTeleportEvent.TeleportCommand` | 6 | event class | `event-bus.event::entity-teleport-event-teleport-command` | supported |
| `EntityTravelToDimensionEvent` | 6 | event class | `event-bus.event::entity-travel-to-dimension-event` | supported |
| `MobEffectEvent.Remove` | 6 | event class | `event-bus.event::mob-effect-event-remove` | supported |
| `AdvancementEvent.AdvancementEarnEvent` | 6 | event class | `event-bus.event::advancement-event-advancement-earn-event` | supported |
| `PlayerXpEvent.PickupXp` | 6 | event class | `pumpkin_core::plugin::player::PlayerXpEventPickupXp` | supported |
| `BlockEvent.NeighborNotifyEvent` | 6 | event class | `event-bus.event::block-event-neighbor-notify-event` | supported |
| `ChunkWatchEvent.Sent` | 6 | event class | `event-bus.event::chunk-watch-event-sent` | supported |
| `ExplosionEvent.Start` | 6 | event class | `pumpkin_core::plugin::entity::EntityExplodeEvent` | supported |
| `ModConfigEvent.Unloading` | 6 | event class | `NativeMod::config_unloading` | supported |
| `PermissionGatherEvent` | 6 | event class | no case (abstract); fields in each subclass record of `event-bus` | supported |
| `PermissionGatherEvent.Nodes` | 6 | event class | `event-bus.event::permission-gather-event-nodes` | supported |
| `CommandEvent` | 5 | event class | `event-bus.event::command-event` | planned |
| `EntityTeleportEvent.EnderEntity` | 5 | event class | `event-bus.event::entity-teleport-event-ender-entity` | supported |
| `EntityTeleportEvent.EnderPearl` | 5 | event class | `event-bus.event::entity-teleport-event-ender-pearl` | supported |
| `LivingBreatheEvent` | 5 | event class | `event-bus.event::living-breathe-event` | supported |
| `LivingConversionEvent` | 5 | event class | no case (abstract); fields in each subclass record of `event-bus` | supported |
| `MobEffectEvent.Expired` | 5 | event class | `event-bus.event::mob-effect-event-expired` | supported |
| `CriticalHitEvent` | 5 | event class | `pumpkin_core::plugin::player::CriticalHitEvent` | supported |
| `PlayerContainerEvent.Close` | 5 | event class | `pumpkin_core::plugin::player::InventoryCloseEvent` | supported |
| `PlayerEvent.HarvestCheck` | 5 | event class | `pumpkin_core::plugin::player::PlayerEventHarvestCheck` | supported |
| `ChunkDataEvent` | 5 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `ChunkDataEvent.Load` | 5 | event class | `event-bus.event::chunk-data-event-load` | planned |
| `ChunkWatchEvent.UnWatch` | 5 | event class | `pumpkin_core::plugin::level::ChunkWatchEventUnWatch` | supported |
| `ChunkWatchEvent.Watch` | 5 | event class | `event-bus.event::chunk-watch-event-watch` | supported |
| `LevelEvent.Save` | 5 | event class | `event-bus.event::level-event-save` | supported |
| `LivingEntityUseItemEvent.Stop` | 4 | event class | `pumpkin_core::plugin::living::LivingEntityUseItemEventStop` | supported |
| `LivingEntityUseItemEvent.Tick` | 4 | event class | `pumpkin_core::plugin::living::LivingEntityUseItemEventTick` | supported |
| `LivingEvent.LivingVisibilityEvent` | 4 | event class | `pumpkin_core::plugin::living::LivingEventLivingVisibilityEvent` | supported |
| `MobDespawnEvent` | 4 | event class | `pumpkin_core::plugin::level::MobDespawnEvent` | supported |
| `MobSpawnEvent.SpawnPlacementCheck` | 4 | event class | `pumpkin_core::plugin::level::MobSpawnEventSpawnPlacementCheck` | supported |
| `BonemealEvent` | 4 | event class | `event-bus.event::bonemeal-event` | supported |
| `PlayerDestroyItemEvent` | 4 | event class | `event-bus.event::player-destroy-item-event` | supported |
| `PlayerEvent.ItemSmeltedEvent` | 4 | event class | `event-bus.event::player-event-item-smelted-event` | supported |
| `UseItemOnBlockEvent` | 4 | event class | `pumpkin_core::plugin::player::UseItemOnBlockEvent` | supported |
| `BlockEvent.EntityMultiPlaceEvent` | 4 | event class | `pumpkin_core::plugin::block::BlockMultiPlaceEvent` | supported |
| `BlockEvent.FarmlandTrampleEvent` | 4 | event class | `event-bus.event::block-event-farmland-trample-event` | planned |
| `ChunkDataEvent.Save` | 4 | event class | `event-bus.event::chunk-data-event-save` | planned |
| `FMLConstructModEvent` | 4 | event class | `NativeMod::init` (`ModInit`) | supported |
| `DataMapsUpdatedEvent` | 4 | event class | `pumpkin_core::plugin::data_maps::DataMapsUpdatedEvent` (re-exported by `ironpumpkin_mods::data_maps`) | supported |
| `GameShuttingDownEvent` | 3 | event class | `event-bus.event::game-shutting-down-event` | planned |
| `GrindstoneEvent` | 3 | event class | no case (abstract); fields in each subclass record of `event-bus` | supported |
| `GrindstoneEvent.OnPlaceItem` | 3 | event class | `pumpkin_core::plugin::inventory::PrepareGrindstoneEvent` (fires outside the screen lock on every result update; applies if the inputs did not change) | supported |
| `GrindstoneEvent.OnTakeItem` | 3 | event class | `pumpkin_core::plugin::inventory::GrindstoneEventOnTakeItem` (fires outside the screen lock on every take; a cancel gives no experience and puts the inputs back) | supported |
| `EntityEvent.Size` | 3 | event class | `event-bus` doc, not supported | planned |
| `EntityMobGriefingEvent` | 3 | event class | `pumpkin_core::plugin::level::EntityMobGriefingEvent` | supported |
| `ArmorHurtEvent` | 3 | event class | `event-bus.event::armor-hurt-event` | supported |
| `BabyEntitySpawnEvent` | 3 | event class | `event-bus.event::baby-entity-spawn-event` | supported |
| `EndermanAngerEvent` | 3 (21.1 `EnderManAngerEvent`) | event class | `pumpkin_core::plugin::level::EndermanAngerEvent` | supported |
| `LivingConversionEvent.Post` | 3 | event class | `pumpkin_core::plugin::living::LivingConversionEventPost` | supported |
| `LivingConversionEvent.Pre` | 3 | event class | `event-bus.event::living-conversion-event-pre` | supported |
| `LivingDestroyBlockEvent` | 3 | event class | `pumpkin_core::plugin::living::LivingDestroyBlockEvent` | supported |
| `PlayerRespawnPositionEvent` | 3 | event class | `pumpkin_core::plugin::player::player_spawn_location::PlayerSpawnLocationEvent` | supported |
| `PlayerSpawnPhantomsEvent` | 3 | event class | `event-bus` doc, not supported | not supported |
| `ChunkTicketLevelUpdatedEvent` | 3 | event class | `event-bus` doc, not supported | planned |
| `NoteBlockEvent` | 3 | event class | no case (abstract); fields in each subclass record of `event-bus` | supported |
| `NoteBlockEvent.Play` | 3 | event class | `event-bus.event::note-block-event-play` | supported |
| `VillageSiegeEvent` | 3 | event class | `event-bus` doc, not supported | not supported |
| `ItemStackedOnOtherEvent` | 2 | event class | `event-bus.event::item-stacked-on-other-event` | planned |
| `PlayLevelSoundEvent` | 2 | event class | `event-bus.event::play-level-sound-event` | planned |
| `PlayLevelSoundEvent.AtEntity` | 2 | event class | `event-bus.event::play-level-sound-event-at-entity` | planned |
| `PlayLevelSoundEvent.AtPosition` | 2 | event class | `event-bus.event::play-level-sound-event-at-position` | planned |
| `GetEnchantmentLevelEvent` | 2 | event class | `event-bus.event::get-enchantment-level-event` | planned |
| `EntityEvent.EntityConstructing` | 2 | event class | `event-bus.event::entity-event-entity-constructing` | planned |
| `EntityStruckByLightningEvent` | 2 | event class | `pumpkin_core::plugin::level::EntityStruckByLightningEvent` | supported |
| `EntityTeleportEvent.SpreadPlayersCommand` | 2 | event class | `pumpkin_core::plugin::level::EntityTeleportEventSpreadPlayersCommand` | supported |
| `ItemExpireEvent` | 2 | event class | `event-bus.event::item-expire-event` | supported |
| `AnimalTameEvent` | 2 | event class | `event-bus.event::animal-tame-event` | supported |
| `LivingGetProjectileEvent` | 2 | event class | `pumpkin_core::plugin::living::LivingGetProjectileEvent` | supported |
| `AnvilCraftEvent` | 2 (21.1 `AnvilRepairEvent`) | event class | no case (abstract); fields in each subclass record of `event-bus` | supported |
| `AnvilCraftEvent.Post` | 2 (21.1 `AnvilRepairEvent`) | event class | `pumpkin_core::plugin::player::AnvilCraftEventPost` (under the screen-handler lock) | supported |
| `ArrowLooseEvent` | 2 | event class | `pumpkin_core::plugin::player::ArrowLooseEvent` | supported |
| `PlayerEvent.NameFormat` | 2 | event class | `pumpkin_core::plugin::player::PlayerEventNameFormat` (not cached) | supported |
| `PlayerEvent.SaveToFile` | 2 | event class | `pumpkin_core::plugin::player::PlayerEventSaveToFile` | supported |
| `PlayerInteractEvent.RightClickEmpty` | 2 | event class | `event-bus` doc, not supported | not supported |
| `BlockEvent.FluidPlaceBlockEvent` | 2 | event class | `event-bus.event::block-event-fluid-place-block-event` | supported |
| `BlockGrowFeatureEvent` | 2 | event class | `event-bus.event::block-grow-feature-event` | supported |
| `ExplosionKnockbackEvent` | 2 | event class | `pumpkin_core::plugin::level::ExplosionKnockbackEvent` | supported |
| `LevelEvent.PotentialSpawns` | 2 | event class | `pumpkin_core::plugin::level::LevelEventPotentialSpawns` | supported |
| `SleepFinishedTimeEvent` | 2 | event class | `event-bus.event::sleep-finished-time-event` | supported |
| `CropGrowEvent` | 2 | event class | no case (abstract); fields in each subclass record of `event-bus` | supported |
| `CropGrowEvent.Post` | 2 | event class | `pumpkin_core::plugin::level::CropGrowEventPost` | supported |
| `FMLDedicatedServerSetupEvent` | 2 | event class | `NativeMod::dedicated_server_setup` | supported |
| `ParallelDispatchEvent` | 2 | event class | no case (abstract); fields in each subclass record of `event-bus` | supported |
| `ExtendPoiTypesEvent` | 2 | event class | `NativeMod::extend_poi_types` with `registration::ExtendPoiTypesEvent` (`addStatesToPoi` with a part of the states: gap, `registration` doc) | supported |
| `ModifyRegistriesEvent` | 2 | event class | `event-bus` doc, not supported | planned |
| `RegisterGameTestsEvent` | 1 | event class | `event-bus` doc, not supported | not supported |
| `StatAwardEvent` | 1 | event class | `pumpkin_core::plugin::player::PlayerStatisticIncrementEvent` (`value` is the new total; fires on `increment_stat` and `set_stat`) | supported |
| `EntityEvent.EnteringSection` | 1 | event class | `event-bus.event::entity-event-entering-section` | planned |
| `ItemEvent` | 1 | event class | no case (abstract); fields in each subclass record of `event-bus` | supported |
| `LivingUseTotemEvent` | 1 | event class | `event-bus.event::living-use-totem-event` | supported |
| `MobSplitEvent` | 1 | event class | `pumpkin_core::plugin::level::MobSplitEvent` | supported |
| `AdvancementEvent.AdvancementProgressEvent` | 1 | event class | `event-bus.event::advancement-event-advancement-progress-event` | planned |
| `ArrowNockEvent` | 1 | event class | `pumpkin_core::plugin::player::ArrowNockEvent` (a set `action` stops the draw) | supported |
| `CanContinueSleepingEvent` | 1 | event class | `event-bus.event::can-continue-sleeping-event` | planned |
| `CanPlayerSleepEvent` | 1 | event class | `event-bus.event::can-player-sleep-event` | supported |
| `ItemFishedEvent` | 1 | event class | `event-bus.event::item-fished-event` | supported |
| `PlayerEvent.LoadFromFile` | 1 | event class | `pumpkin_core::plugin::player::PlayerEventLoadFromFile` | supported |
| `PlayerEvent.StopTracking` | 1 | event class | `pumpkin_core::plugin::player::PlayerEventStopTracking` | supported |
| `PlayerWakeUpEvent` | 1 | event class | `event-bus.event::player-wake-up-event` | supported |
| `PlayerXpEvent.XpChange` | 1 | event class | `event-bus.event::player-xp-event-xp-change` | supported |
| `TradeWithVillagerEvent` | 1 | event class | `pumpkin_core::plugin::player::TradeWithVillagerEvent` (under the screen-handler lock) | supported |
| `BlockEvent.PortalSpawnEvent` | 1 | event class | `event-bus.event::block-event-portal-spawn-event` | supported |
| `ModifyCustomSpawnersEvent` | 1 | event class | `event-bus` doc, not supported | not supported |
| `PistonEvent` | 1 | event class | no case (abstract); fields in each subclass record of `event-bus` | supported |
| `PistonEvent.Pre` | 1 | event class | `event-bus.event::piston-event-pre` | supported |
| `CropGrowEvent.Pre` | 1 | event class | `event-bus.event::crop-grow-event-pre` | supported |
| `RegisterCauldronFluidContentEvent` | 1 | event class | `event-bus` doc, not supported | planned |
| `AddAttributeTooltipsEvent` | 0 | event class | `event-bus` doc, not supported | not supported |
| `DefaultDataComponentsBoundEvent` | 0 | event class | `event-bus` doc, not supported | planned |
| `DifficultyChangeEvent` | 0 | event class | `event-bus.event::difficulty-change-event` | planned |
| `GatherSkippedAttributeTooltipsEvent` | 0 | event class | `event-bus` doc, not supported | not supported |
| `ModMismatchEvent` | 0 | event class | `event-bus` doc, not supported | not supported |
| `RegisterCauldronInteractionEvent` | 0 | event class | `event-bus` doc, not supported | planned |
| `RegisterCauldronInteractionEvent.Dispatcher` | 0 | event class | `event-bus` doc, not supported | planned |
| `RegisterCauldronInteractionEvent.Interaction` | 0 | event class | `event-bus` doc, not supported | planned |
| `RegisterGameRuleCategoryEvent` | 0 | event class | `event-bus` doc, not supported | not supported |
| `RegisterRecipePropertiesEvent` | 0 | event class | `event-bus` doc, not supported | not supported |
| `RegisterStructureConversionsEvent` | 0 | event class | `event-bus` doc, not supported | not supported |
| `RegisterTooltipAppendersEvent` | 0 | event class | `event-bus` doc, not supported | not supported |
| `SortedReloadListenerEvent` | 0 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `TagsUpdatedEvent.ServerDataLoad` | 0 | event class | `event-bus.event::tags-updated-event-server-data-load` | planned |
| `VanillaGameEvent` | 0 | event class | `pumpkin_core::plugin::world::GenericGameEvent` (8 of the 61 vanilla game events from 10 sites, no vibration dispatch: gap) | planned |
| `PlayerBrewedPotionEvent` | 0 | event class | `event-bus.event::player-brewed-potion-event` | planned |
| `PotionBrewEvent` | 0 | event class | no case (abstract); fields in each subclass record of `event-bus` | supported |
| `PotionBrewEvent.Post` | 0 | event class | `pumpkin_core::plugin::inventory::PotionBrewEventPost` (after the brew and after a cancelled Pre; changed stacks apply if the stand did not change) | supported |
| `PotionBrewEvent.Pre` | 0 | event class | `pumpkin_core::plugin::inventory::BrewEvent` (a cancel takes `stacks` only if the stand did not change, then Post fires) | supported |
| `EnchantedBlockLootEvent` | 0 | event class | `event-bus.event::enchanted-block-loot-event` | planned |
| `EnchantedEntityLootEvent` | 0 | event class | `event-bus.event::enchanted-entity-loot-event` | planned |
| `EnchantmentLevelSetEvent` | 0 | event class | `pumpkin_core::plugin::enchantment::PrepareItemEnchantEvent` (has `pos`; `original_level_requirements()` returns the vanilla levels; fires outside the screen lock) | supported |
| `EntityTeleportEvent.ItemConsumption` | 0 | event class | `event-bus.event::entity-teleport-event-item-consumption` | supported |
| `EffectParticleModificationEvent` | 0 | event class | `pumpkin_core::plugin::living::EffectParticleModificationEvent` | supported |
| `LivingDrownEvent` | 0 | event class | `pumpkin_core::plugin::living::LivingDrownEvent` | supported |
| `LivingSwapItemsEvent` | 0 | event class | no case (abstract); fields in each subclass record of `event-bus` | supported |
| `LivingSwapItemsEvent.Hands` | 0 | event class | `event-bus.event::living-swap-items-event-hands` | supported |
| `SpawnClusterSizeEvent` | 0 | event class | `pumpkin_core::plugin::level::SpawnClusterSizeEvent` | supported |
| `AnvilCraftEvent.Pre` | 0 | event class | `pumpkin_core::plugin::player::AnvilCraftEventPre` (under the screen-handler lock; a cancel skips the cost) | supported |
| `ClientInformationUpdatedEvent` | 0 | event class | `pumpkin_core::plugin::player::ClientInformationUpdatedEvent` | supported |
| `CustomClickActionEvent` | 0 | event class | `event-bus.event::custom-click-action-event` | supported |
| `FluidTooltipEvent` | 0 | event class | `event-bus` doc, not supported | not supported |
| `PermissionsChangedEvent` | 0 | event class | `event-bus.event::permissions-changed-event` | planned |
| `PlayerEnchantItemEvent` | 0 | event class | `pumpkin_core::plugin::enchantment::PlayerEnchantItemEvent` (fires before the lapis is consumed; `enchanted_item` is written back; fires outside the screen lock) | supported |
| `PlayerEvent.ItemSmithedEvent` | 0 | event class | `pumpkin_core::plugin::inventory::SmithItemEvent` (the recipe id only; a handler can cancel it) | planned |
| `PlayerEvent.PlayerChangeGameModeEvent` | 0 | event class | `event-bus.event::player-event-player-change-game-mode-event` | supported |
| `PlayerEvent.TabListNameFormat` | 0 | event class | `pumpkin_core::plugin::player::PlayerEventTabListNameFormat` | supported |
| `PlayerFlyableFallEvent` | 0 | event class | `pumpkin_core::plugin::player::PlayerFlyableFallEvent` | supported |
| `PlayerNegotiationEvent` | 0 | event class | `event-bus.event::player-negotiation-event` | planned |
| `PlayerSetSpawnEvent` | 0 | event class | `event-bus.event::player-set-spawn-event` | supported |
| `PlayerSwitchHotbarSlotEvent` | 0 | event class | no case (abstract); fields in each subclass record of `event-bus` | supported |
| `PlayerSwitchHotbarSlotEvent.Post` | 0 | event class | `pumpkin_core::plugin::player::PlayerSwitchHotbarSlotEventPost` | supported |
| `PlayerSwitchHotbarSlotEvent.Pre` | 0 | event class | `event-bus.event::player-switch-hotbar-slot-event-pre` | supported |
| `PlayerXpEvent.LevelChange` | 0 | event class | `event-bus.event::player-xp-event-level-change` | supported |
| `SweepAttackEvent` | 0 | event class | `pumpkin_core::plugin::player::SweepAttackEvent` | supported |
| `AlterGroundEvent` | 0 | event class | `event-bus` doc, not supported | planned |
| `GameRuleChangedEvent` | 0 | event class | `pumpkin_core::plugin::level::GameRuleChangedEvent` | supported |
| `LevelEvent.CreateSpawnPosition` | 0 | event class | `event-bus.event::level-event-create-spawn-position` | planned |
| `NoteBlockEvent.Change` | 0 | event class | `pumpkin_core::plugin::level::NoteBlockEventChange` | supported |
| `PistonEvent.Post` | 0 | event class | `pumpkin_core::plugin::level::PistonEventPost` | supported |
| `CreateFluidSourceEvent` | 0 | event class | `pumpkin_core::plugin::level::CreateFluidSourceEvent` | supported |
| `RegisterRpcSchemaEvent` | 0 | event class | gap: the management server serves a fixed schema; M5 | planned |
| `ServerLifecycleEvent` | 0 | event class | no case (abstract); fields in each subclass record of `event-bus` | supported |
| `ModLifecycleEvent` | 0 | event class | no method (abstract); the `NativeMod` lifecycle methods run in `lifecycle` order | supported |
| `GatherDataEvent` | 0 | event class | `event-bus` doc, not supported | not supported |
| `GatherDataEvent.Client` | 0 | event class | `event-bus` doc, not supported | not supported |
| `GatherDataEvent.Server` | 0 | event class | `event-bus` doc, not supported | not supported |
| `XpOrbTargetingEvent` | 0 | event class | `event-bus.event::xp-orb-targeting-event` | planned |
| `NewDatapackRegistryEvent` | 0 | event class | `event-bus` doc, not supported | planned |
| `PermissionGatherEvent.Handler` | 0 | event class | `event-bus` doc, not supported | not supported |

### Registries

| NeoForge item | Mods | Section | Counterpart | Status |
|:--|--:|:--|:--|:--|
| `DeferredHolder` | 224 | Registration API | no handle type: the name, looked up with `registry::id_of`, `key_of` and `is_in_tag` after the freeze | supported |
| `DeferredRegister#register` | 203 | Registration API | `ModInit::register_block`, `register_item`, `register_entity_type`, `register_sound_event`, `register_mob_effect`, `register_attribute`, `register_data_component_type` | supported |
| `DeferredRegister#create` | 201 | Registration API | `ModInit`, one handle for the block, item, entity type, sound event, mob effect, attribute and data component type registries | supported |
| `item` | 132 | Static registries a `DeferredRegister` can target | `ModInit::register_item`, `content::RegisterEventExt::register_item` | supported |
| `data_component_type` | 113 | Static registries a `DeferredRegister` can target | `ModInit::register_data_component_type`, `data_components::RegisterDataComponentType`; values through `data_components::DataComponentType` | supported |
| `creative_mode_tab` | 112 | Static registries a `DeferredRegister` can target | `ModInit::register_creative_mode_tab` (contract only: the host logs the entry and ignores it) | not supported |
| `NeoForgeRegistries` | 111 | Registration API | partial: `attachment_types` and `global_loot_modifier_serializers`; the other registries are a gap (M5), see the NeoForge registries table | planned |
| `DeferredItem` | 104 | Registration API | no handle type: the name, looked up with `Item::from_registry_key` or `registry::id_of` after the freeze | supported |
| `block` | 98 | Static registries a `DeferredRegister` can target | `ModInit::register_block`, `content::RegisterEventExt::register_block` | supported |
| `block_entity_type` | 90 | Static registries a `DeferredRegister` can target | gap: no registration API for a new block entity type; M5 | planned |
| `DeferredRegister#createItems` | 85 | Registration API | `ModInit::register_item` with `content::ItemBuilder` | supported |
| `DeferredBlock` | 78 | Registration API | no handle type: the name, looked up with `Block::from_name` or `registry::id_of` after the freeze | supported |
| `menu` | 78 | Static registries a `DeferredRegister` can target | gap: no registration API for a mod menu type; M5 | planned |
| `NeoForgeRegistries.Keys` | 77 | Registration API | partial: `attachment_types` and `global_loot_modifier_serializers`; the other registries are a gap (M5), see the NeoForge registries table | planned |
| `DeferredRegister.Items#register` | 76 | Registration API | `ModInit::register_item` with `content::ItemBuilder` | supported |
| `recipe_serializer` | 74 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `DeferredRegister#createBlocks` | 66 | Registration API | `ModInit::register_block` with `content::BlockBuilder` | supported |
| `DeferredRegister.Blocks#register` | 60 | Registration API | `ModInit::register_block` with `content::BlockBuilder` | supported |
| `sound_event` | 55 | Static registries a `DeferredRegister` can target | `ModInit::register_sound_event`, `sound_events::RegisterSoundEvent`; played with `sound_events::play_sound` | supported |
| `neoforge:attachment_types` | 54 | Registries NeoForge adds | `registration.entry-definition::attachment-type` | supported |
| `recipe_type` | 49 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `RegistryBuilder` | 47 | Registration API | `registration` doc, not supported | planned |
| `entity_type` | 46 | Static registries a `DeferredRegister` can target | `ModInit::register_entity_type`, `content::RegisterEventExt::register_entity_type` | supported |
| `DeferredRegister#createDataComponents` | 45 | Registration API | `ModInit::register_data_component_type` with `data_components::DataComponentTypeBuilder` | supported |
| `particle_type` | 41 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `DeferredRegister#getEntries` | 36 | Registration API | `ModInit::entries` inside `init`, `registry::mod_entries` after it | supported |
| `IRegistryExtension` | 31 | Registration API | partial: `registry::contains`, `id_of`, `key_of` (`getKey`, `getKeyOrNull`), `keys`, `data_maps::DataMapType::get_data` (`getDataMap`); the alias, max id, sync and callback methods are a gap (M5) | planned |
| `DeferredRegister.DataComponents#registerComponentType` | 23 | Registration API | `ModInit::register_data_component_type` with `DataComponentTypeBuilder::<T>::new`, the codec is `T`; values through `data_components::DataComponentType` | supported |
| `neoforge:global_loot_modifier_serializers` | 22 | Registries NeoForge adds | `ModInit::register_loot_modifier_type` | supported |
| `command_argument_type` | 21 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `mob_effect` | 19 | Static registries a `DeferredRegister` can target | `ModInit::register_mob_effect`, `mob_effects::RegisterMobEffect`; applied through `mob_effects::add_effect` | supported |
| `trigger_type` | 19 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `RegisterEvent.RegisterHelper#register` | 18 | Registration API | `content::RegisterEventExt::register_block`, `register_item`, `register_entity_type` | supported |
| `DeferredRegister.Items#registerItem` | 17 | Registration API | `ModInit::register_item` with `ItemBuilder::behaviour` | supported |
| `DeferredRegister.Items#registerSimpleBlockItem` | 16 | Registration API | `ModInit::register_item` with `ItemBuilder::places` | supported |
| `neoforge:fluid_type` | 16 | Registries NeoForge adds | `registration.entry-definition::fluid-type` (contract only: fails with `registration-error::unsupported-registry` until a phase backs it) | planned |
| `neoforge:condition_codecs` | 16 | Registries NeoForge adds | `registration` doc, not supported | planned |
| `fluid` | 16 | Static registries a `DeferredRegister` can target | `registration.entry-definition::fluid` (contract only: fails with `registration-error::unsupported-registry` until a phase backs it) | planned |
| `loot_function_type` | 16 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `attribute` | 13 | Static registries a `DeferredRegister` can target | `ModInit::register_attribute`, `entity_attributes::RegisterAttribute`; values through `entity_attributes` | supported |
| `DeferredRegister#makeRegistry` | 12 | Registration API | `registration` doc, not supported | planned |
| `DeferredRegister#addAlias` | 11 | Registration API | gap: the content registry has no alias map; M5 | planned |
| `neoforge:biome_modifier_serializers` | 10 | Registries NeoForge adds | `registration` doc, not supported | planned |
| `DeferredRegister.Blocks#registerBlock` | 9 | Registration API | `ModInit::register_block` with `BlockBuilder::behaviour` | supported |
| `neoforge:entity_data_serializers` | 9 | Registries NeoForge adds | `registration` doc, not supported | planned |
| `point_of_interest_type` | 9 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `worldgen/structure_type` | 8 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `worldgen/placement_modifier_type` | 8 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `loot_pool_entry_type` | 7 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `loot_condition_type` | 7 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `worldgen/structure_processor` | 7 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `registries.callback` | 7 | Other registry packages | `registration` doc, not supported | planned |
| `GameData` | 6 | Registration API | `registration` doc, not supported | not supported |
| `villager_profession` | 6 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `worldgen/structure_piece` | 6 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `potion` | 5 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `custom_stat` | 5 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `registries.holdersets` | 5 | Other registry packages | `registration` doc, not supported | planned |
| `worldgen/chunk_generator` | 4 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `DeferredRegister#getRegistry` | 3 | Registration API | `ModInit::registry`, a `registry::Registry` handle | supported |
| `worldgen/structure_placement` | 3 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `enchantment_effect_component_type` | 3 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `DeferredRegister#getNamespace` | 2 | Registration API | `ModInit::namespace` | supported |
| `DeferredRegister.Blocks#registerSimpleBlock` | 2 | Registration API | `ModInit::register_block` with `BlockBuilder::new` | supported |
| `memory_module_type` | 2 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `sensor_type` | 2 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `height_provider_type` | 2 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `worldgen/trunk_placer_type` | 2 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `worldgen/structure_pool_element` | 2 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `map_decoration_type` | 2 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `DataPackRegistriesHooks` | 1 | Registration API | `registration` doc, not supported | not supported |
| `DeferredRegister#getRegistryKey` | 1 | Registration API | `ModInit::registry_key` | supported |
| `DeferredRegister.Items#registerSimpleItem` | 1 | Registration API | `ModInit::register_item` with `ItemBuilder::new` | supported |
| `RegistryManager` | 1 | Registration API | `registration` doc, not supported | not supported |
| `RegistrySnapshot` | 1 | Registration API | `pumpkin_protocol::java::neoforge::RegistrySnapshot` (host-side `frozen_registry` sync; no mod API) | supported |
| `neoforge:structure_modifier_serializers` | 1 | Registries NeoForge adds | `registration` doc, not supported | planned |
| `neoforge:holder_set_type` | 1 | Registries NeoForge adds | `registration` doc, not supported | planned |
| `neoforge:fluid_ingredient_type` | 1 | Registries NeoForge adds | `registration` doc, not supported | planned |
| `rule_test` | 1 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `int_provider_type` | 1 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `worldgen/block_state_provider_type` | 1 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `worldgen/foliage_placer_type` | 1 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `worldgen/tree_decorator_type` | 1 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `worldgen/biome_source` | 1 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `worldgen/density_function_type` | 1 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `entity_sub_predicate_type` | 1 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `enchantment_entity_effect_type` | 1 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `BaseMappedRegistry` | 0 | Registration API | `registration` doc, not supported | not supported |
| `DataMapLoader` | 0 | Registration API | host loader of `ironpumpkin_mods::data_maps`, after `DatapackManager::load_all` at startup and on `/reload` | supported |
| `DatapackRegistryBuilder` | 0 | Registration API | `registration` doc, not supported | planned |
| `DeferredRegister#createEntities` | 0 | Registration API | `ModInit::register_entity_type` with `content::EntityTypeBuilder` | supported |
| `DeferredRegister#createTagKey` | 0 | Registration API | the tag name: `content::TagBuilder`, `registry::is_in_tag`, `registry::tag_entries` | supported |
| `DeferredRegister#getRegistryName` | 0 | Registration API | `ModInit::registry_name` | supported |
| `DeferredRegister.Entities#registerEntityType` | 0 | Registration API | `ModInit::register_entity_type` with `content::EntityTypeBuilder` | supported |
| `NeoForgeRegistriesSetup` | 0 | Registration API | `registration` doc, not supported | not supported |
| `RegistryManager.SnapshotType` | 0 | Registration API | not supported: member of an internal class; `SYNCED_REGISTRIES` is the sync snapshot | not supported |
| `neoforge:ingredient_serializer` | 0 | Registries NeoForge adds | `registration` doc, not supported | planned |
| `neoforge:biome_modifier` | 0 | Registries NeoForge adds | `registration` doc, not supported | planned |
| `neoforge:structure_modifier` | 0 | Registries NeoForge adds | `registration` doc, not supported | planned |
| `neoforge:synced_attachment_types` | 0 | Registries NeoForge adds | `attachments.attachment-type-definition.sync` | supported |
| `game_event` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `debug_subscription` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `chunk_status` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `rule_block_entity_modifier` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `pos_rule_test` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `position_source_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `stat_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `villager_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `activity` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `context_float_provider_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `context_int_provider_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `loot_nbt_provider_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `loot_score_provider_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `float_provider_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `block_predicate_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `worldgen/carver_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `worldgen/feature_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `worldgen/root_placer_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `worldgen/feature_size_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `worldgen/material_condition_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `worldgen/material_rule_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `worldgen/pool_alias_binding` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `number_format_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `game_rule` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `data_component_predicate_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `enchantment_level_based_value_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `enchantment_location_based_effect_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `enchantment_value_effect_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `enchantment_provider_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `consume_effect_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `recipe_display` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `slot_display` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `recipe_book_category` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `ticket_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `incoming_rpc_methods` | 0 | Static registries a `DeferredRegister` can target | gap: the management server has a fixed method set; M5 | planned |
| `outgoing_rpc_methods` | 0 | Static registries a `DeferredRegister` can target | gap: the management server has a fixed notification set; M5 | planned |
| `test_environment_definition_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `test_instance_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `spawn_condition_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `dialog_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `dialog_action_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `input_control_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `dialog_body_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `permission_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `permission_check_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `environment_attribute` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `attribute_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `slot_source_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `context_key_set` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | planned |
| `test_function` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |

### Capabilities

| NeoForge item | Mods | Section | Counterpart | Status |
|:--|--:|:--|:--|:--|
| `BlockCapability` | 153 | Capability API | `ironpumpkin_mods::capabilities::BlockCapability` | supported |
| `Capabilities` | 138 | Capability API | `ironpumpkin_mods::capabilities::item`, `fluid`, `energy` | supported |
| `ICapabilityProvider` | 119 | Capability API | closures `ironpumpkin_mods::capabilities::BlockEntityProvider`, `EntityProvider`, `ItemProvider` | supported |
| `Capabilities.Item` | 112 | Capability API | `ironpumpkin_mods::capabilities::item` | supported |
| `Capabilities.Item.BLOCK` | 107 | Built-in capabilities | `ironpumpkin_mods::capabilities::item::BLOCK` | supported |
| `RegisterCapabilitiesEvent#registerBlockEntity` | 95 | Capability API | `ironpumpkin_mods::capabilities::RegisterCapabilitiesEvent::register_block_entity` | supported |
| `ItemCapability` | 88 | Capability API | `ironpumpkin_mods::capabilities::ItemCapability` | supported |
| `Capabilities.Fluid` | 78 | Capability API | `ironpumpkin_mods::capabilities::fluid` | supported |
| `Capabilities.Energy` | 73 | Capability API | `ironpumpkin_mods::capabilities::energy` | supported |
| `Capabilities.Fluid.BLOCK` | 70 | Built-in capabilities | `ironpumpkin_mods::capabilities::fluid::BLOCK` | supported |
| `Capabilities.Energy.BLOCK` | 60 | Built-in capabilities | `ironpumpkin_mods::capabilities::energy::BLOCK` | supported |
| `RegisterCapabilitiesEvent#registerItem` | 53 | Capability API | `ironpumpkin_mods::capabilities::RegisterCapabilitiesEvent::register_item` | supported |
| `Capabilities.Energy.ITEM` | 47 | Built-in capabilities | `ironpumpkin_mods::capabilities::energy::ITEM` | supported |
| `EntityCapability` | 44 | Capability API | `ironpumpkin_mods::capabilities::EntityCapability` | supported |
| `Capabilities.Fluid.ITEM` | 42 | Built-in capabilities | `ironpumpkin_mods::capabilities::fluid::ITEM` | supported |
| `IBlockCapabilityProvider` | 41 | Capability API | closure `ironpumpkin_mods::capabilities::BlockProvider` | supported |
| `RegisterCapabilitiesEvent#registerBlock` | 37 | Capability API | `ironpumpkin_mods::capabilities::RegisterCapabilitiesEvent::register_block` | supported |
| `BlockCapabilityCache` | 35 | Capability API | `ironpumpkin_mods::capabilities::BlockCapabilityCache` | supported |
| `Capabilities.Item.ITEM` | 22 | Built-in capabilities | `ironpumpkin_mods::capabilities::item::ITEM` | supported |
| `Capabilities.Item.ENTITY` | 18 | Built-in capabilities | `ironpumpkin_mods::capabilities::item::ENTITY` | supported |
| `RegisterCapabilitiesEvent#registerEntity` | 16 | Capability API | `ironpumpkin_mods::capabilities::RegisterCapabilitiesEvent::register_entity` | supported |
| `ICapabilityInvalidationListener` | 8 | Capability API | `ironpumpkin_mods::capabilities::CapabilityInvalidationListener` | supported |
| `Capabilities.Item.ENTITY_AUTOMATION` | 7 | Built-in capabilities | `ironpumpkin_mods::capabilities::item::ENTITY_AUTOMATION` | supported |
| `Capabilities.Energy.ENTITY` | 5 | Built-in capabilities | `ironpumpkin_mods::capabilities::energy::ENTITY` | supported |
| `RegisterCapabilitiesEvent#isBlockRegistered` | 5 | Capability API | `ironpumpkin_mods::capabilities::RegisterCapabilitiesEvent::is_block_registered` | supported |
| `Capabilities.Fluid.ENTITY` | 4 | Built-in capabilities | `ironpumpkin_mods::capabilities::fluid::ENTITY` | supported |
| `BaseCapability` | 4 | Capability API | `name` of the capability keys | supported |
| `RegisterCapabilitiesEvent#setProxyable` | 3 | Capability API | `ironpumpkin_mods::capabilities::RegisterCapabilitiesEvent::set_proxyable` | supported |
| `CapabilityRegistry` | 1 | Capability API | the `create` functions of the capability keys | supported |
| `CapabilityRegistry.CapabilityConstructor` | 1 | Capability API | the `create` functions of the capability keys | supported |
| `RegisterCapabilitiesEvent#setNonProxyable` | 1 | Capability API | `ironpumpkin_mods::capabilities::RegisterCapabilitiesEvent::set_non_proxyable` | supported |
| `CapabilityHooks` | 0 | Capability API | `capabilities` doc, not supported | not supported |
| `CapabilityListenerHolder` | 0 | Capability API | `capabilities` doc, not supported | not supported |
| `RegisterCapabilitiesEvent#isEntityRegistered` | 0 | Capability API | `ironpumpkin_mods::capabilities::RegisterCapabilitiesEvent::is_entity_registered` | supported |
| `RegisterCapabilitiesEvent#isItemRegistered` | 0 | Capability API | `ironpumpkin_mods::capabilities::RegisterCapabilitiesEvent::is_item_registered` | supported |

### Attachments

| NeoForge item | Mods | Section | Counterpart | Status |
|:--|--:|:--|:--|:--|
| `Level` | 310 | Attachment holders | `attachments.attachment-holder` | supported |
| `Entity` | 249 | Attachment holders | `attachments.attachment-holder` | supported |
| `BlockEntity` | 235 | Attachment holders | `attachments.attachment-holder` | supported |
| `LevelChunk` | 70 | Attachment holders | `attachments.attachment-holder` | supported |
| `ChunkAccess` | 65 | Attachment holders | `attachments.attachment-holder` | supported |
| `IAttachmentHolder#getData` | 57 | Attachment API | `attachments.attachment-holder.get-data` | supported |
| `AttachmentType.Builder#build` | 55 | Attachment API | `registration.entry-definition::attachment-type` | supported |
| `AttachmentType#builder` | 46 | Attachment API | `attachments.attachment-type-definition` | supported |
| `AttachmentType.Builder#serialize` | 43 | Attachment API | `attachments.attachment-type-definition.serialize` | supported |
| `AttachmentType.Builder#copyOnDeath` | 25 | Attachment API | `attachments.attachment-type-definition.copy-on-death` | supported |
| `IAttachmentHolder#hasData` | 16 | Attachment API | `attachments.attachment-holder.has-data` | supported |
| `AttachmentType#serializable` | 15 | Attachment API | `attachments.attachment-type-definition.serialize` | supported |
| `AttachmentType.Builder#sync` | 10 | Attachment API | `attachments.attachment-type-definition.sync` | supported |
| `IAttachmentSerializer` | 7 | Attachment API | `attachments.attachment-type-definition.serialize` (host serializer) | supported |
| `AttachmentType.Builder#copyHandler` | 5 | Attachment API | `attachments.attachment-type-definition.copy-handler` | supported |
| `IAttachmentCopyHandler` | 5 | Attachment API | `callbacks.attachment-copy` | supported |
| `IAttachmentHolder#getExistingData` | 5 | Attachment API | `attachments.attachment-holder.get-existing-data` | supported |
| `AttachmentHolder` | 3 | Attachment API | `attachments.attachment-holder` | supported |
| `IAttachmentHolder#getExistingDataOrNull` | 3 | Attachment API | `attachments.attachment-holder.get-existing-data` | supported |
| `IAttachmentHolder#syncData` | 2 | Attachment API | `attachments.attachment-holder.sync-data` | supported |
| `IAttachmentHolder#hasAttachments` | 1 | Attachment API | `attachments.attachment-holder.has-attachments` | supported |
| `AttachmentHolder.AsField` | 0 | Attachment API | `attachments.attachment-holder` | supported |
| `AttachmentInternals` | 0 | Attachment API | `attachments` doc, not supported | not supported |
| `AttachmentSync` | 0 | Attachment API | `attachments` doc, not supported | not supported |
| `AttachmentSyncHandler` | 0 | Attachment API | `attachments.attachment-sync`, `callbacks.attachment-send-to-player` | supported |
| `LevelAttachmentsSavedData` | 0 | Attachment API | `attachments` doc, not supported | not supported |

### Data maps

| NeoForge item | Mods | Section | Counterpart | Status |
|:--|--:|:--|:--|:--|
| `DataMapType` | 27 | Data map API | `ironpumpkin_mods::data_maps::DataMapType` (`get_data`, `with_data`, `data_map`) | supported |
| `DataMapType.Builder` | 18 | Data map API | `ironpumpkin_mods::data_maps::DataMapTypeBuilder` | supported |
| `WAXABLES` | 5 | Built-in data maps | `ironpumpkin_mods::data_maps::builtin::WAXABLES` | supported |
| `registries.datamaps.builtin` | 5 | Built-in data maps | `ironpumpkin_mods::data_maps::builtin` (value types) | supported |
| `AdvancedDataMapType` | 3 | Data map API | `ironpumpkin_mods::data_maps::AdvancedDataMapType` | supported |
| `OXIDIZABLES` | 3 | Built-in data maps | `ironpumpkin_mods::data_maps::builtin::OXIDIZABLES` | supported |
| `AdvancedDataMapType.Builder` | 2 | Data map API | `ironpumpkin_mods::data_maps::AdvancedDataMapTypeBuilder` | supported |
| `DataMapValueRemover` | 2 | Data map API | trait `ironpumpkin_mods::data_maps::DataMapValueRemover` | supported |
| `DataMapEntry` | 1 | Data map API | host loader: `{"value", "replace"}` or the bare value | supported |
| `DataMapEntry.Removal` | 1 | Data map API | host loader: the `remove` field, with remover objects for an advanced type | supported |
| `DataMapFile` | 1 | Data map API | host loader: `replace`, `values`, `remove` | supported |
| `DataMapValueMerger` | 1 | Data map API | trait `ironpumpkin_mods::data_maps::DataMapValueMerger` (`DefaultMerger`, `ListMerger`, `SetMerger`, `MapMerger`, `WrappedMerger`) | supported |
| `DataMapValueRemover.Default` | 1 | Data map API | `ironpumpkin_mods::data_maps::DefaultRemover` | supported |
| `IWithData` | 1 | Data map API | `ironpumpkin_mods::data_maps::DataMapType::get_data` | supported |
| `MONSTER_ROOM_MOBS` | 1 | Built-in data maps | `ironpumpkin_mods::data_maps::builtin::MONSTER_ROOM_MOBS` | supported |
| `PARROT_IMITATIONS` | 1 | Built-in data maps | `ironpumpkin_mods::data_maps::builtin::PARROT_IMITATIONS` | supported |
| `RAID_HERO_GIFTS` | 1 | Built-in data maps | `ironpumpkin_mods::data_maps::builtin::RAID_HERO_GIFTS` | supported |
| `VIBRATION_FREQUENCIES` | 1 | Built-in data maps | `ironpumpkin_mods::data_maps::builtin::VIBRATION_FREQUENCIES` | supported |
| `VILLAGER_TYPES` | 1 | Built-in data maps | `ironpumpkin_mods::data_maps::builtin::VILLAGER_TYPES` | supported |
| `DataMapsUpdatedEvent.UpdateCause` | 0 | Data map API | `pumpkin_core::plugin::data_maps::UpdateCause` | supported |
| `ACCEPTABLE_VILLAGER_DISTANCES` | 0 | Built-in data maps | `ironpumpkin_mods::data_maps::builtin::ACCEPTABLE_VILLAGER_DISTANCES` | supported |
| `BLOCK_TRANSFORM_APPENDERS` | 0 | Built-in data maps | `ironpumpkin_mods::data_maps::builtin::BLOCK_TRANSFORM_APPENDERS` | supported |
| `TRANSFORMABLES` | 0 | Built-in data maps | `ironpumpkin_mods::data_maps::builtin::TRANSFORMABLES` | supported |
| `VILLAGER_COMPOSTABLES` | 0 | Built-in data maps | `ironpumpkin_mods::data_maps::builtin::VILLAGER_COMPOSTABLES` | supported |

### Config

| NeoForge item | Mods | Section | Counterpart | Status |
|:--|--:|:--|:--|:--|
| `ModConfigSpec` | 182 | Config API | `ironpumpkin_mods::config::ConfigSpec` | supported |
| `ModConfig` | 165 | Config API | `ironpumpkin_mods::config::ModConfig` | supported |
| `ModConfig.Type` | 165 | Config API | `ironpumpkin_mods::config::ConfigType` | supported |
| `IConfigSpec` | 162 | Config API | `ironpumpkin_mods::config::ConfigSpec` | not supported |
| `ModConfigSpec.Builder#comment` | 156 | Config API | `ironpumpkin_mods::config::ConfigSpecBuilder::comment` | supported |
| `ModConfigSpec.Builder#define` | 140 | Config API | `ironpumpkin_mods::config::ConfigSpecBuilder::define`, `define_validated` | supported |
| `ModConfigSpec.BooleanValue` | 138 | Config API | `ironpumpkin_mods::config::ModConfig::get::<bool>` | supported |
| `ModConfigSpec.Builder#defineInRange` | 134 | Config API | `ironpumpkin_mods::config::ConfigSpecBuilder::define_in_range` | supported |
| `ModConfigSpec.Builder#push` | 132 | Config API | `ironpumpkin_mods::config::ConfigSpecBuilder::push` | supported |
| `ModConfigSpec.Builder#pop` | 130 | Config API | `ironpumpkin_mods::config::ConfigSpecBuilder::pop` | supported |
| `ModConfigSpec.Builder#build` | 123 | Config API | `ironpumpkin_mods::config::ConfigSpecBuilder::build` | supported |
| `ModConfigSpec.IntValue` | 119 | Config API | `ironpumpkin_mods::config::ModConfig::get::<i32>` | supported |
| `ModConfigSpec.ConfigValue` | 104 | Config API | `ironpumpkin_mods::config::ModConfig::get`, `set`, `save`, `get_default` | supported |
| `ModConfig.Type.LOCAL` | 102 | Config types | `ironpumpkin_mods::config::ConfigType::Local` | supported |
| `ModConfig.Type.SYNCED` | 75 | Config types | `ironpumpkin_mods::config::ConfigType::Synced` | supported |
| `ModConfigSpec.DoubleValue` | 68 | Config API | `ironpumpkin_mods::config::ModConfig::get::<f64>` | supported |
| `ModConfigSpec.Builder#configure` | 57 | Config API | `ironpumpkin_mods::config::ConfigSpec::builder`, then `ConfigSpecBuilder::build` | supported |
| `ModConfigSpec.EnumValue` | 39 | Config API | `ironpumpkin_mods::config::ConfigEnum`, `ModConfig::get::<E>` | supported |
| `ModConfigSpec.Builder#translation` | 38 | Config API | `ironpumpkin_mods::config::ConfigSpecBuilder::translation` | supported |
| `ModConfigSpec.Builder#defineEnum` | 37 | Config API | `ironpumpkin_mods::config::ConfigSpecBuilder::define_enum` | supported |
| `ModConfigSpec.Builder#defineList` | 31 | Config API | `ironpumpkin_mods::config::ConfigSpecBuilder::define_list` | supported |
| `ModConfigSpec.Builder#defineListAllowEmpty` | 31 | Config API | `ironpumpkin_mods::config::ConfigSpecBuilder::define_list` (an empty list in the file is kept) | supported |
| `ModConfigSpec.LongValue` | 22 | Config API | `ironpumpkin_mods::config::ModConfig::get::<i64>` | supported |
| `ModConfig.Type.STARTUP` | 17 | Config types | `ironpumpkin_mods::config::ConfigType::Startup` | supported |
| `ModConfigSpec.Builder#worldRestart` | 15 | Config API | `ironpumpkin_mods::config::ConfigSpecBuilder::world_restart` | supported |
| `ModConfigSpec.ValueSpec` | 7 | Config API | `ironpumpkin_mods::config::ConfigSpec` (per value: `ModConfig::get_default`, `ConfigSpec::translation`) | supported |
| `ModConfigSpec.Range` | 5 | Config API | `ironpumpkin_mods::config::RangeBound` (`i32`, `i64`, `f64`) | supported |
| `IConfigSpec.ILoadedConfig` | 5 | Config API | `ironpumpkin_mods::config::ModConfig` | not supported |
| `ModConfigSpec.Builder#gameRestart` | 4 | Config API | `ironpumpkin_mods::config::ConfigSpecBuilder::game_restart` | supported |
| `ConfigTracker` | 4 | Config API | `ironpumpkin_mods::config` doc, not supported | not supported |
| `ModConfigSpec.RestartType` | 3 | Config API | `ironpumpkin_mods::config::ConfigSpecBuilder::world_restart`, `game_restart` | supported |
| `ModConfigs` | 3 | Config API | `mod-config.get-mod-configs`, `get-file-names` | supported |
| `ModConfigSpec.Builder#defineInList` | 1 | Config API | `ironpumpkin_mods::config::ConfigSpecBuilder::define_validated` | supported |
| `ModConfigSpec.ListValueSpec` | 0 | Config API | `ironpumpkin_mods::config::ConfigSpecBuilder::define_list` | not supported |
| `NeoForgeLocalConfig` | 0 | Config API | `ironpumpkin_mods::config` doc, not supported | not supported |
| `NeoForgeSyncedConfig` | 0 | Config API | `ironpumpkin_mods::config` doc, not supported | not supported |

### Networking

| NeoForge item | Mods | Section | Counterpart | Status |
|:--|--:|:--|:--|:--|
| `IPayloadHandler` | 166 | Configuration tasks and other network types | `ironpumpkin_mods::network::PayloadHandler` (a closure `Fn(&[u8], &mut PayloadContext)`) | supported |
| `IPayloadContext#player` | 154 | Payload registration and sending | `ironpumpkin_mods::network::PayloadContext::player` | supported |
| `IPayloadContext#enqueueWork` | 151 | Payload registration and sending | `pumpkin:plugin/scheduler`; a handler on `HandlerThread::Main` already runs on the world tick | planned |
| `PayloadRegistrar#playToClient` | 125 | Payload registration and sending | `ironpumpkin_mods::network::PayloadRegistrar::play_to_client` | supported |
| `PayloadRegistrar#playToServer` | 123 | Payload registration and sending | `ironpumpkin_mods::network::PayloadRegistrar::play_to_server` | supported |
| `PacketDistributor#sendToPlayer` | 121 | Payload registration and sending | `ironpumpkin_mods::network::send_to_player` | supported |
| `IContainerFactory` | 84 | Configuration tasks and other network types | gap: the extra data of `menus::open_menu` and mod menu types, both M5 (`neoforge:advanced_open_screen`) | planned |
| `PayloadRegistrar#versioned` | 54 | Payload registration and sending | `ironpumpkin_mods::network::PayloadRegistrar::versioned` | supported |
| `PacketDistributor#sendToPlayersTrackingChunk` | 43 | Payload registration and sending | `network.send-to-players-tracking-chunk` | planned |
| `PayloadRegistrar#optional` | 42 | Payload registration and sending | `ironpumpkin_mods::network::PayloadRegistrar::optional` | supported |
| `IPayloadContext#flow` | 41 | Payload registration and sending | `ironpumpkin_mods::network::PayloadContext::flow` | supported |
| `PacketDistributor#sendToAllPlayers` | 39 | Payload registration and sending | `ironpumpkin_mods::network::send_to_all_players` | supported |
| `network.codec` | 34 | Other network packages | `network` doc, not supported | not supported |
| `PayloadRegistrar#playBidirectional` | 32 | Payload registration and sending | `ironpumpkin_mods::network::PayloadRegistrar::play_bidirectional` | supported |
| `PacketDistributor#sendToPlayersTrackingEntity` | 27 | Payload registration and sending | `network.send-to-players-tracking-entity` | planned |
| `PacketDistributor#sendToPlayersNear` | 20 | Payload registration and sending | `network.send-to-players-near` | planned |
| `PacketDistributor#sendToPlayersTrackingEntityAndSelf` | 19 | Payload registration and sending | `network.send-to-players-tracking-entity-and-self` | planned |
| `IPayloadContext#disconnect` | 17 | Payload registration and sending | `ironpumpkin_mods::network::PayloadContext::disconnect` | supported |
| `HandlerThread` | 12 | Configuration tasks and other network types | `ironpumpkin_mods::network::HandlerThread` | supported |
| `PacketDistributor#sendToPlayersInDimension` | 11 | Payload registration and sending | `ironpumpkin_mods::network::send_to_players_in_dimension` | supported |
| `PayloadRegistrar#executesOn` | 10 | Payload registration and sending | `ironpumpkin_mods::network::PayloadRegistrar::executes_on` | supported |
| `network.connection` | 8 | Other network packages | `network` doc, not supported | not supported |
| `PayloadRegistrar#configurationToClient` | 7 | Payload registration and sending | `ironpumpkin_mods::network::PayloadRegistrar::configuration_to_client` | supported |
| `NetworkRegistry` | 7 | Configuration tasks and other network types | `network` doc, not supported | not supported |
| `IPayloadContext#finishCurrentTask` | 6 | Payload registration and sending | `ironpumpkin_mods::network::PayloadContext::finish_current_task` | supported |
| `IPayloadContext#connection` | 5 | Payload registration and sending | `network` doc, not supported | not supported |
| `IPayloadContext#reply` | 5 | Payload registration and sending | `ironpumpkin_mods::network::PayloadContext::reply` | supported |
| `ICustomConfigurationTask` | 5 | Configuration tasks and other network types | `ironpumpkin_mods::network::CustomConfigurationTask`, `ironpumpkin_mods::network::TaskContext` | supported |
| `PayloadRegistrar#commonBidirectional` | 3 | Payload registration and sending | `ironpumpkin_mods::network::PayloadRegistrar::common_bidirectional` | supported |
| `MainThreadPayloadHandler` | 3 | Configuration tasks and other network types | `ironpumpkin_mods::network::HandlerThread::Main` | supported |
| `PayloadRegistrar#configurationToServer` | 2 | Payload registration and sending | `ironpumpkin_mods::network::PayloadRegistrar::configuration_to_server` | supported |
| `PayloadRegistrar#commonToServer` | 2 | Payload registration and sending | `ironpumpkin_mods::network::PayloadRegistrar::common_to_server` | supported |
| `IPayloadContext#protocol` | 2 | Payload registration and sending | `ironpumpkin_mods::network::PayloadContext::protocol` | supported |
| `PayloadRegistrar#commonToClient` | 1 | Payload registration and sending | `ironpumpkin_mods::network::PayloadRegistrar::common_to_client` | supported |
| `IPayloadContext#listener` | 1 | Payload registration and sending | `network` doc, not supported | not supported |
| `network.bundle` | 1 | Other network packages | `network` doc, not supported | not supported |
| `PayloadRegistrar#configurationBidirectional` | 0 | Payload registration and sending | `ironpumpkin_mods::network::PayloadRegistrar::configuration_bidirectional` | supported |
| `IPayloadContext#handle` | 0 | Payload registration and sending | `network` doc, not supported | not supported |
| `IPayloadContext#channelHandlerContext` | 0 | Payload registration and sending | `network` doc, not supported | not supported |
| `neoforge:advanced_add_entity` | 0 | Payloads | gap: spawn data of a mod entity type; M5 (`neoforge:advanced_add_entity`) | planned |
| `neoforge:advanced_container_set_data` | 0 | Payloads | gap: 32-bit data slots of a mod menu; M5 | planned |
| `neoforge:advanced_open_screen` | 0 | Payloads | gap: extra data of `openMenu` and mod menu types; M5 | planned |
| `neoforge:auxiliary_light_data` | 0 | Payloads | `network` doc, not supported | planned |
| `c:register` | 0 | Payloads | `ConfigurationTask::CommonRegister` (host-side wire protocol; no mod API) | supported |
| `c:version` | 0 | Payloads | `ConfigurationTask::CommonVersion` (host-side wire protocol; no mod API) | supported |
| `neoforge:config_file` | 0 | Payloads | `ConfigurationTask::SyncConfig` (host payload, the file of each synced config of `ironpumpkin_mods::config::ConfigType::Synced`) | supported |
| `neoforge:extensible_enum_ack` | 0 | Payloads | `ConfigurationTask::ExtensibleEnums` (host-side wire protocol; no mod API) | supported |
| `neoforge:extensible_enum_data` | 0 | Payloads | `ConfigurationTask::ExtensibleEnums` (host-side wire protocol; no mod API) | supported |
| `neoforge:feature_flags_ack` | 0 | Payloads | `ConfigurationTask::FeatureFlags` (host-side wire protocol; no mod API) | supported |
| `neoforge:feature_flags` | 0 | Payloads | `ConfigurationTask::FeatureFlags` (host-side wire protocol; no mod API) | supported |
| `neoforge:frozen_registry` | 0 | Payloads | `ConfigurationTask::RegistrySync` (host-side wire protocol; no mod API) | supported |
| `neoforge:frozen_registry_sync_completed` | 0 | Payloads | `ConfigurationTask::RegistrySync` (host-side wire protocol; no mod API) | supported |
| `neoforge:frozen_registry_sync_start` | 0 | Payloads | `ConfigurationTask::RegistrySync` (host-side wire protocol; no mod API) | supported |
| `neoforge:known_registry_data_maps` | 0 | Payloads | `pumpkin_protocol::java::neoforge::KnownRegistryDataMapsPayload` (host payload) | supported |
| `neoforge:known_registry_data_maps_reply` | 0 | Payloads | `pumpkin_protocol::java::neoforge::KnownRegistryDataMapsReplyPayload` (host payload) | supported |
| `minecraft:register` | 0 | Payloads | `pumpkin:plugin/event` channel events; `network.has-channel` | supported |
| `minecraft:unregister` | 0 | Payloads | `pumpkin:plugin/event` channel events; `network.has-channel` | supported |
| `neoforge:network` | 0 | Payloads | channel negotiation (host-side wire protocol; no mod API) | supported |
| `neoforge:register` | 0 | Payloads | host-side wire protocol (phase 1 codec, M3); no mod API | supported |
| `neoforge:modded_network_setup_failed` | 0 | Payloads | channel negotiation (host-side wire protocol; no mod API) | supported |
| `neoforge:recipe_content` | 0 | Payloads | `network` doc, not supported | not supported |
| `neoforge:registry_data_map_sync` | 0 | Payloads | `pumpkin_protocol::java::neoforge::RegistryDataMapSyncPayload` (host payload) | supported |
| `neoforge:split` | 0 | Payloads | split and join of large packets (host-side wire protocol; no mod API) | supported |
| `neoforge:sync_attachments` | 0 | Payloads | `ironpumpkin_mods::attachments` (host payload; `ModInit::register_attachment_type` with a sync handler) | supported |
| `CheckExtensibleEnums` | 0 | Configuration tasks and other network types | `ConfigurationTask::ExtensibleEnums` (host-side configuration task; no mod API) | supported |
| `CheckExtensibleEnums.EnumEntry` | 0 | Configuration tasks and other network types | `pumpkin_protocol::java::neoforge::EnumEntry` (host-side; no mod API) | supported |
| `CheckExtensibleEnums.ExtensionData` | 0 | Configuration tasks and other network types | not supported: Java-only, member of an internal class; the server never sends the data | not supported |
| `CheckFeatureFlags` | 0 | Configuration tasks and other network types | `ConfigurationTask::FeatureFlags` (host-side configuration task; no mod API) | supported |
| `CommonRegisterTask` | 0 | Configuration tasks and other network types | `ConfigurationTask::CommonRegister` (host-side configuration task; no mod API) | supported |
| `CommonVersionTask` | 0 | Configuration tasks and other network types | `ConfigurationTask::CommonVersion` (host-side configuration task; no mod API) | supported |
| `RegistryDataMapNegotiation` | 0 | Configuration tasks and other network types | host configuration task (`pumpkin_core::plugin::data_maps`); no mod API | supported |
| `SyncConfig` | 0 | Configuration tasks and other network types | `ConfigurationTask::SyncConfig` reads `pumpkin_core::plugin::startup::synced_configs` for `ironpumpkin_mods::config::ConfigType::Synced` | supported |
| `SyncRegistries` | 0 | Configuration tasks and other network types | `ConfigurationTask::RegistrySync` with `SYNCED_REGISTRIES` (host-side configuration task; no mod API) | supported |
| `QueuedPacket` | 0 | Configuration tasks and other network types | `network` doc, not supported | not supported |
| `QueuedPacket.CustomPayload` | 0 | Configuration tasks and other network types | `network` doc, not supported | not supported |
| `ServerPayloadContext` | 0 | Configuration tasks and other network types | `network` doc, not supported | not supported |
| `ChannelAttributes` | 0 | Configuration tasks and other network types | `network` doc, not supported | not supported |
| `ModdedConfigurationPayloadRegistration` | 0 | Configuration tasks and other network types | `network` doc, not supported | not supported |
| `ModdedPlayPayloadRegistration` | 0 | Configuration tasks and other network types | `network` doc, not supported | not supported |
| `NetworkChannel` | 0 | Configuration tasks and other network types | `network` doc, not supported | not supported |
| `NetworkPayloadSetup` | 0 | Configuration tasks and other network types | `network` doc, not supported | not supported |
| `PayloadRegistration` | 0 | Configuration tasks and other network types | `network` doc, not supported | not supported |
| `ConfigSync` | 0 | Configuration tasks and other network types | `network` doc, not supported | not supported |
| `ConfigurationInitialization` | 0 | Configuration tasks and other network types | `network` doc, not supported | not supported |
| `DualStackUtils` | 0 | Configuration tasks and other network types | `network` doc, not supported | not supported |
| `network.filters` | 0 | Other network packages | `network` doc, not supported | not supported |
| `network.handlers` | 0 | Other network packages | `network` doc, not supported | not supported |
| `network.negotiation` | 0 | Other network packages | `network` doc, not supported | not supported |

### Lifecycle, entry points and metadata

| NeoForge item | Mods | Section | Counterpart | Status |
|:--|--:|:--|:--|:--|
| `file modLoader` | 414 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `file loaderVersion` | 414 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `file license` | 414 | `neoforge.mods.toml` keys | `mods_toml::parse`: required, a missing or blank license stops the server with "Missing license" | supported |
| `file mods` | 414 | `neoforge.mods.toml` keys | `NativeMod::mods_toml`; the `[[mods]]` entry of the mod (`mods_toml::parse`) | supported |
| `[[mods]] modId` | 414 | `neoforge.mods.toml` keys | `mods_toml::ModInfo::mod_id` (`mods_toml::is_valid_mod_id`) | supported |
| `[[mods]] version` | 414 | `neoforge.mods.toml` keys | `mods_toml::ModInfo::version` (`version::ArtifactVersion`, `${file.jarVersion}` is `NativeMod::version`) | supported |
| `[[mods]] displayName` | 414 | `neoforge.mods.toml` keys | `mods_toml::ModInfo::display_name` | supported |
| `[[mods]] description` | 414 | `neoforge.mods.toml` keys | `mods_toml::ModInfo::description` | supported |
| `@Mod(value)` | 411 | Entry point annotations | `register_mod!` and `NativeMod::init`; the mod id is `NativeMod::id` or the `modId` of `NativeMod::mods_toml` | supported |
| `[[dependencies.<modid>]] modId` | 404 | `neoforge.mods.toml` keys | `mods_toml::ModDependency::mod_id` | supported |
| `[[dependencies.<modid>]] versionRange` | 404 | `neoforge.mods.toml` keys | `mods_toml::ModDependency::version_range` (`version::VersionRange`) | supported |
| `[[dependencies.<modid>]] side` | 398 | `neoforge.mods.toml` keys | `mods_toml::ModDependency::side` (`DependencySide`, `CLIENT` is ignored) | supported |
| `[[mods]] authors` | 392 | `neoforge.mods.toml` keys | `mods_toml::ModInfo::authors` | supported |
| `[[dependencies.<modid>]] ordering` | 388 | `neoforge.mods.toml` keys | `mods_toml::ModDependency::ordering` (`Ordering`, the load order of `init` and the lifecycle methods) | supported |
| `[[dependencies.<modid>]] type` | 361 | `neoforge.mods.toml` keys | `mods_toml::ModDependency::dependency_type` (`DependencyType`) | supported |
| `[[mixins]] config` | 291 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `file mixins` | 278 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `IEventBus#addListener` | 263 | Event bus and mod container | `ModInit::register_event`, `ModInit::register_parent_event`, `Context::register_event` (Wasm) | supported |
| `file issueTrackerURL` | 236 | `neoforge.mods.toml` keys | `lifecycle.mods-toml.issue-tracker-url`, `mod-info.issue-tracker-url` | supported |
| `ModList#get` | 200 | Event bus and mod container | `mod_list::ModList::get` | supported |
| `@EventBusSubscriber(modid)` | 186 | Entry point annotations | `ModInit::register_event` of the calling mod (Rust has no annotation scan) | not supported |
| `ModList#isLoaded` | 175 | Event bus and mod container | `mod_list::ModList::is_loaded` | supported |
| `ModContainer#registerConfig` | 159 | Event bus and mod container | `ModInit::register_config`, `ModInit::register_config_with_file_name` | supported |
| `@EventBusSubscriber(value)` | 151 | Entry point annotations | `ModInit::register_event` (client-dist subscribers are not registered) | not supported |
| `IEventBus#register` | 112 | Event bus and mod container | `ModInit::register_event` for each handler (Rust has no annotation scan) | not supported |
| `EventPriority` | 99 | Event bus and mod container | `pumpkin_core::plugin::EventPriority` (WIT `event-priority`; `Highest` runs first) | supported |
| `IEventBus#post` | 86 | Event bus and mod container | `event-bus.post` | supported |
| `@Mod(dist)` | 77 | Entry point annotations | `mod_list::dist()` (always `Dist::DedicatedServer`) | supported |
| `@SubscribeEvent(priority)` | 76 | Entry point annotations | `ModInit::register_event` `priority` argument (`EventPriority`) | supported |
| `ModList#getModContainerById` | 69 | Event bus and mod container | `mod_list::ModList::get_mod_container_by_id` | supported |
| `ModContainer#getModInfo` | 61 | Event bus and mod container | `mod_list::ModList::get_mod_container_by_id` returns the `mods_toml::ModInfo` | supported |
| `file accessTransformers` | 55 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `[[accessTransformers]] file` | 55 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `[[mods]] updateJSONURL` | 38 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `ModContainer#getEventBus` | 32 | Event bus and mod container | the `NativeMod` lifecycle methods, one set per mod; game events through `ModInit::register_event` | not supported |
| `ModList#getMods` | 31 | Event bus and mod container | `mod_list::ModList::get_mods` | supported |
| `ModList#getModFileById` | 16 | Event bus and mod container | `lifecycle` doc, not supported | not supported |
| `ModList#getAllScanData` | 16 | Event bus and mod container | `lifecycle` doc, not supported | not supported |
| `IEventBus#start` | 15 | Event bus and mod container | `lifecycle` doc, not supported | not supported |
| `ModContainer#getModId` | 15 | Event bus and mod container | `mods_toml::ModInfo::mod_id` | supported |
| `[[dependencies.<modid>]] reason` | 14 | `neoforge.mods.toml` keys | `mods_toml::ModDependency::reason` | supported |
| `[[mods]] enumExtensions` | 13 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `IEventBus#unregister` | 12 | Event bus and mod container | `lifecycle` doc, not supported | not supported |
| `[[mods]] modUrl` | 10 | `neoforge.mods.toml` keys | `lifecycle.mod-info.mod-url` | supported |
| `[[mods]] issueTrackerURL` | 8 | `neoforge.mods.toml` keys | `lifecycle.mods-toml.issue-tracker-url`, `mod-info.issue-tracker-url` | supported |
| `@SubscribeEvent(receiveCanceled)` | 6 | Entry point annotations | `ModInit::register_event` `receive_canceled` argument | supported |
| `[[mods]] featureFlags` | 4 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `ModList#getModFiles` | 3 | Event bus and mod container | `lifecycle` doc, not supported | not supported |
| `ModList#forEachModInOrder` | 3 | Event bus and mod container | `mod_list::ModList::get_sorted_mods` (iterate) | supported |
| `[[dependencies.<modid>]] referralUrl` | 3 | `neoforge.mods.toml` keys | `lifecycle.mod-dependency.referral-url` | supported |
| `ModList#size` | 2 | Event bus and mod container | `mod_list::ModList::size` | supported |
| `ModList#forEachModFile` | 2 | Event bus and mod container | `lifecycle` doc, not supported | not supported |
| `ModList#forEachModContainer` | 2 | Event bus and mod container | `mod_list::ModList::get_mods` (iterate) | supported |
| `ModList#getSortedMods` | 2 | Event bus and mod container | `mod_list::ModList::get_sorted_mods` | supported |
| `[[mixins]] requiredMods` | 2 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `ModContainer#getNamespace` | 1 | Event bus and mod container | `mods_toml::ModInfo::namespace` | supported |
| `ModContainer#acceptEvent` | 1 | Event bus and mod container | `lifecycle` doc, not supported | not supported |
| `ModList#applyForEachModContainer` | 1 | Event bus and mod container | `mod_list::ModList::get_mods` (iterate) | supported |
| `file showAsResourcePack` | 1 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `@Mod(depends)` | 0 | Entry point annotations | `mods_toml::ModInfo::dependencies`; `pumpkin:plugin/metadata` dependencies | supported |
| `ModList#of` | 0 | Event bus and mod container | `lifecycle` doc, not supported | not supported |
| `ModList#applyForEachModFile` | 0 | Event bus and mod container | `lifecycle` doc, not supported | not supported |
| `ModList#applyForEachModFileAlphabetical` | 0 | Event bus and mod container | `lifecycle` doc, not supported | not supported |
| `ModList#clear` | 0 | Event bus and mod container | `lifecycle` doc, not supported | not supported |
| `file showAsDataPack` | 0 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `file services` | 0 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `file properties` | 0 | `neoforge.mods.toml` keys | `lifecycle.mods-toml.properties` | supported |
| `[[mixins]] behaviorVersion` | 0 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `[[mods]] namespace` | 0 | `neoforge.mods.toml` keys | `mods_toml::ModInfo::namespace` (`mods_toml::is_valid_namespace`) | supported |
| `[[mods]] dependencies` | 0 | `neoforge.mods.toml` keys | `mods_toml::ModInfo::dependencies` | supported |
| `[[mods]] features` | 0 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `[[mods]] modproperties` | 0 | `neoforge.mods.toml` keys | `lifecycle.mod-info.mod-properties` | supported |

### Loot modifiers, loot conditions and data conditions

| NeoForge item | Mods | Section | Counterpart | Status |
|:--|--:|:--|:--|:--|
| `ICondition` | 42 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | planned |
| `ICondition.IContext` | 32 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | planned |
| `IGlobalLootModifier` | 29 | Loot modifiers, loot conditions and data conditions | `ironpumpkin_mods::loot::GlobalLootModifier` and the decode callback of `ModInit::register_loot_modifier_type` | supported |
| `LootModifier` | 25 | Loot modifiers, loot conditions and data conditions | the host reads `condition` and `priority` from the modifier file (`ironpumpkin_mods::loot`) | supported |
| `ConditionalOps` | 12 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | planned |
| `WithConditions` | 8 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | planned |
| `ModLoadedCondition` | 7 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | planned |
| `AddTableLootModifier` | 2 | Loot modifiers, loot conditions and data conditions | `ironpumpkin_mods::loot` built-in type `neoforge:add_table`; `LootContext::roll_loot_table_raw` | supported |
| `CanItemPerformAbility` | 2 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | planned |
| `LootTableIdCondition` | 2 | Loot modifiers, loot conditions and data conditions | `ironpumpkin_mods::loot` built-in condition `neoforge:loot_table_id` (modifier conditions only) | supported |
| `AndCondition` | 2 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | planned |
| `LootModifierManager` | 1 | Loot modifiers, loot conditions and data conditions | loader of `data/<namespace>/loot_modifiers/` in `DatapackManager` | supported |
| `LootTableIdCondition.Builder` | 1 | Loot modifiers, loot conditions and data conditions | gap: Java datagen or `LootTableLoadEvent` code; write the JSON condition `neoforge:loot_table_id` | not supported |
| `ConditionContext` | 1 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | planned |
| `NotCondition` | 1 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | planned |
| `OrCondition` | 1 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | planned |
| `TagEmptyCondition` | 1 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | planned |
| `NeoForgeLootContextParams` | 0 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | planned |
| `AlwaysCondition` | 0 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | planned |
| `FeatureFlagsEnabledCondition` | 0 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | planned |
| `NeoForgeConditions` | 0 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | planned |
| `NeverCondition` | 0 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | planned |
| `RegisteredCondition` | 0 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | planned |
| `WithConditions.Builder` | 0 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | planned |
| `GlobalLootModifierProvider` | 0 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | not supported |

### Extension interfaces NeoForge adds to `net.minecraft`

| NeoForge item | Mods | Section | Counterpart | Status |
|:--|--:|:--|:--|:--|
| `ILevelExtension#getCapability` | 75 | `ILevelExtension` (Level) | `ironpumpkin_mods::capabilities::BlockCapability::get_capability` | supported |
| `IItemStackExtension#getCapability` | 68 | `IItemStackExtension` (ItemStack) | `ironpumpkin_mods::capabilities::ItemCapability::get_capability` | supported |
| `IPlayerExtension#openMenu` | 60 | `IPlayerExtension` (Player) | `ironpumpkin_mods::menus::open_menu` | planned |
| `IHolderExtension#getKey` | 49 | `IHolderExtension` (Holder) | `ironpumpkin_mods::registry::key_of` (a holder is a name in IronPumpkin) | supported |
| `IBlockExtension#getCloneItemStack` | 42 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IRecipeOutputExtension#withConditions` | 34 | `IRecipeOutputExtension` (RecipeOutput) | - | not supported |
| `IBlockEntityExtension#invalidateCapabilities` | 32 | `IBlockEntityExtension` (BlockEntity) | `ironpumpkin_mods::capabilities::invalidate_capabilities` | supported |
| `IItemPropertiesExtensions#component` | 32 | `IItemPropertiesExtensions` (Item.Properties) | `ItemBuilder` component calls | not supported |
| `IBlockEntityExtension#onLoad` | 30 | `IBlockEntityExtension` (BlockEntity) | `BlockEntity` trait (pumpkin-core `block/entities/mod.rs`), new hook | planned |
| `IBlockExtension#getLightEmission` | 29 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IItemExtension#onItemUseFirst` | 26 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IDataComponentMapBuilderExtensions#set` | 24 | `IDataComponentMapBuilderExtensions` (DataComponentMap.Builder) | `ItemBuilder` component calls | not supported |
| `IItemExtension#getCreatorModId` | 24 | `IItemExtension` (Item) | - | not supported |
| `IRecipeOutputExtension#accept` | 24 | `IRecipeOutputExtension` (RecipeOutput) | - | not supported |
| `IBlockExtension#getSoundType` | 22 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockStateExtension#getSoundType` | 22 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockEntityExtension#onChunkUnloaded` | 21 | `IBlockEntityExtension` (BlockEntity) | `BlockEntity` trait (pumpkin-core `block/entities/mod.rs`), new hook | planned |
| `IItemExtension#canPerformAction` | 20 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IBlockExtension#onDestroyedByPlayer` | 19 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IItemExtension#getMaxDamage` | 19 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IBlockExtension#canEntityDestroy` | 18 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `ILevelExtension#invalidateCapabilities` | 18 | `ILevelExtension` (Level) | `ironpumpkin_mods::capabilities::invalidate_capabilities` and `invalidate_capabilities_in_chunk` | supported |
| `IServerCommonPacketListenerExtension#send` | 18 | `IServerCommonPacketListenerExtension` (ServerCommonPacketListener) | `ironpumpkin_mods::network` payload send helpers | planned |
| `IBlockExtension#onNeighborChange` | 17 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IHolderLookupProviderExtension#holderOrThrow` | 17 | `IHolderLookupProviderExtension` (HolderLookup.Provider) | `registry::contains`, `registry::id_of` | not supported |
| `IItemExtension#supportsEnchantment` | 17 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IBlockStateExtension#getCloneItemStack` | 16 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `ILevelReaderExtension#isAreaLoaded` | 16 | `ILevelReaderExtension` (LevelReader) | `World` chunk loading state | planned |
| `IPacketFlowExtension#isClientbound` | 16 | `IPacketFlowExtension` (PacketFlow) | `ironpumpkin_mods::network::PacketFlow` match | not supported |
| `IPacketFlowExtension#isServerbound` | 16 | `IPacketFlowExtension` (PacketFlow) | `ironpumpkin_mods::network::PacketFlow` match | not supported |
| `IBlockExtension#getPistonPushReaction` | 15 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `ICommonPacketListener#hasChannel` | 15 | `ICommonPacketListener` | `NegotiatedChannels::has_channel` (pumpkin-core `plugin/network_events.rs`) | supported |
| `IBlockExtension#getExplosionResistance` | 14 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#rotate` | 14 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockStateExtension#getLightEmission` | 14 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IItemExtension#getMaxStackSize` | 14 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IBlockExtension#onBlockExploded` | 13 | `IBlockExtension` (Block) | `BlockBehaviour::explode` | planned |
| `IFluidStateExtension#getFluidType` | 13 | `IFluidStateExtension` (FluidState) | `FluidState` forwarding to `FluidBehaviour`, new hook | planned |
| `IFriendlyByteBufExtension#writeByte` | 13 | `IFriendlyByteBufExtension` (FriendlyByteBuf) | - | not supported |
| `IItemExtension#isDamageable` | 12 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `ILevelReaderExtension#holderOrThrow` | 12 | `ILevelReaderExtension` (LevelReader) | `registry::contains`, `registry::id_of` | not supported |
| `IBlockStateExtension#getToolModifiedState` | 11 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IEntityExtension#isMultipartEntity` | 11 | `IEntityExtension` (Entity) | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | planned |
| `IItemExtension#doesSneakBypassUse` | 11 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IBlockExtension#canSustainPlant` | 10 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#getToolModifiedState` | 10 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockStateExtension#getFriction` | 10 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#canHarvestBlock` | 10 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBucketPickupExtension#getPickupSound` | 10 | `IBucketPickupExtension` (BucketPickup) | bucket `ItemBehaviour` (pumpkin-core `item/items/bucket.rs`), new hook | planned |
| `IItemExtension#getDefaultAttributeModifiers` | 10 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemExtension#getDamage` | 10 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemStackExtension#canEquip` | 10 | `IItemStackExtension` (ItemStack) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `IItemStackExtension#getAttributeModifiers` | 10 | `IItemStackExtension` (ItemStack) | `ItemStack` attribute modifier component | planned |
| `IBlockExtension#getEnchantPowerBonus` | 9 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockStateExtension#onDestroyedByPlayer` | 9 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#rotate` | 9 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IItemExtension#onLeftClickEntity` | 9 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemExtension#onEntityItemUpdate` | 9 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemStackExtension#supportsEnchantment` | 9 | `IItemStackExtension` (ItemStack) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `IPacketFlowExtension#getReceptionSide` | 9 | `IPacketFlowExtension` (PacketFlow) | `ironpumpkin_mods::network::PacketFlow` match | not supported |
| `IBlockGetterExtension#getAuxLightManager` | 8 | `IBlockGetterExtension` (BlockGetter) | `World` light engine | planned |
| `IBlockStateExtension#isEmpty` | 8 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IEntityExtension#canRiderInteract` | 8 | `IEntityExtension` (Entity) | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | planned |
| `IItemExtension#isPrimaryItemFor` | 8 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `TooltipFlagExtension#hasShiftDown` | 8 | `TooltipFlagExtension` (TooltipFlag) | - | not supported |
| `IBlockExtension#addLandingEffects` | 7 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#shouldCheckWeakPower` | 7 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#getBlockPathType` | 7 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#getFlammability` | 7 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockStateExtension#canSustainPlant` | 7 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#canEntityDestroy` | 7 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IEntityExtension#shouldRiderSit` | 7 | `IEntityExtension` (Entity) | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | planned |
| `IEntityExtension#getParts` | 7 | `IEntityExtension` (Entity) | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | planned |
| `IItemExtension#getEquipmentSlot` | 7 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IPlayerExtension#isFakePlayer` | 7 | `IPlayerExtension` (Player) | `Player` (no fake player type) | planned |
| `IServerConfigurationPacketListenerExtension#finishCurrentTask` | 7 | `IServerConfigurationPacketListenerExtension` (ServerConfigurationPacketListener) | `ironpumpkin_mods::network::PayloadContext::finish_current_task` | supported |
| `IBlockExtension#getFriction` | 6 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#addRunningEffects` | 6 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#getBeaconColorMultiplier` | 6 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#getFireSpreadSpeed` | 6 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#isFireSource` | 6 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IHolderExtension#getDelegate` | 6 | `IHolderExtension` (Holder) | `registry::key_of`, `registry::id_of` | not supported |
| `IItemExtension#makesPiglinsNeutral` | 6 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemExtension#hasCustomEntity` | 6 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemExtension#setDamage` | 6 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemExtension#damageItem` | 6 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemStackExtension#getAllEnchantments` | 6 | `IItemStackExtension` (ItemStack) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `ILevelReaderExtension#holder` | 6 | `ILevelReaderExtension` (LevelReader) | `registry::contains`, `registry::id_of` | not supported |
| `ITransformationExtension#isIdentity` | 6 | `ITransformationExtension` (Transformation) | - | not supported |
| `IAttributeExtension#toComponent` | 5 | `IAttributeExtension` (Attribute) | - | not supported |
| `IBlockExtension#isLadder` | 5 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#canHarvestBlock` | 5 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#isFertile` | 5 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#getExpDrop` | 5 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#isFlammable` | 5 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockStateExtension#getExplosionResistance` | 5 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#getEnchantPowerBonus` | 5 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#onNeighborChange` | 5 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#isFlammable` | 5 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IDispensibleContainerItemExtension#emptyContents` | 5 | `IDispensibleContainerItemExtension` (DispensibleContainerItem) | bucket `ItemBehaviour` (pumpkin-core `item/items/bucket.rs`), new hook | planned |
| `IEntityExtension#isPushedByFluid` | 5 | `IEntityExtension` (Entity) | `EntityBase::is_pushed_by_fluids` | planned |
| `IHolderLookupProviderExtension#holder` | 5 | `IHolderLookupProviderExtension` (HolderLookup.Provider) | `registry::contains`, `registry::id_of` | not supported |
| `IItemStackExtension#getXpRepairRatio` | 5 | `IItemStackExtension` (ItemStack) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `ITagAppenderExtension#addTags` | 5 | `ITagAppenderExtension` (TagAppender) | - | not supported |
| `IAttributeExtension#toValueComponent` | 4 | `IAttributeExtension` (Attribute) | - | not supported |
| `IBlockExtension#canDropFromExplosion` | 4 | `IBlockExtension` (Block) | `BlockBehaviour::should_drop_items_on_explosion` | planned |
| `IBlockExtension#collisionExtendsVertically` | 4 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockStateExtension#getExpDrop` | 4 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IItemExtension#createEntity` | 4 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemExtension#canEquip` | 4 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemExtension#getEnchantmentLevel` | 4 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemExtension#getAllEnchantments` | 4 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemExtension#isNotReplaceableByPickAction` | 4 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemStackExtension#getEquipmentSlot` | 4 | `IItemStackExtension` (ItemStack) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `ILivingEntityExtension#canDrownInFluidType` | 4 | `ILivingEntityExtension` (LivingEntity) | `LivingEntity` (pumpkin-core `entity/living.rs`), new hook | planned |
| `ITagAppenderExtension#remove` | 4 | `ITagAppenderExtension` (TagAppender) | - | not supported |
| `IBlockExtension#getAdjacentBlockPathType` | 3 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#onCaughtFire` | 3 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#getMapColor` | 3 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockStateExtension#shouldCheckWeakPower` | 3 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#onCaughtFire` | 3 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#canBeHydrated` | 3 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IDataComponentHolderExtension#addToTooltip` | 3 | `IDataComponentHolderExtension` (DataComponentHolder) | - | not supported |
| `IEntityExtension#getClassification` | 3 | `IEntityExtension` (Entity) | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | planned |
| `IEntityExtension#isInFluidType` | 3 | `IEntityExtension` (Entity) | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | planned |
| `IEntityExtension#isEyeInFluidType` | 3 | `IEntityExtension` (Entity) | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | planned |
| `IItemExtension#onEntitySwing` | 3 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemExtension#canWalkOnPowderedSnow` | 3 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemExtension#canBeHurtBy` | 3 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemStackExtension#isPrimaryItemFor` | 3 | `IItemStackExtension` (ItemStack) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `ILevelExtension#getDescription` | 3 | `ILevelExtension` (Level) | - | not supported |
| `IPlayerExtension#mayFly` | 3 | `IPlayerExtension` (Player) | `Player` abilities | planned |
| `IAdvancementBuilderExtension#save` | 2 | `IAdvancementBuilderExtension` (Advancement.Builder) | - | not supported |
| `IBlockExtension#onTreeGrow` | 2 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#getWeakChanges` | 2 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#isStickyBlock` | 2 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#isScaffolding` | 2 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#canBeHydrated` | 2 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockStateExtension#ignitedByLava` | 2 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#isFertile` | 2 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#getWeakChanges` | 2 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#canStickTo` | 2 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#getFlammability` | 2 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#getFireSpreadSpeed` | 2 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#isFireSource` | 2 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#canDropFromExplosion` | 2 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#onBlockExploded` | 2 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IFluidStateExtension#getExplosionResistance` | 2 | `IFluidStateExtension` (FluidState) | `FluidState` forwarding to `FluidBehaviour`, new hook | planned |
| `IItemExtension#onDroppedByPlayer` | 2 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemExtension#getXpRepairRatio` | 2 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemExtension#onStopUsing` | 2 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemExtension#canContinueUsing` | 2 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemExtension#onDestroyed` | 2 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemExtension#applyEnchantments` | 2 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemExtension#canFitInsideContainerItems` | 2 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemStackExtension#doesSneakBypassUse` | 2 | `IItemStackExtension` (ItemStack) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `IItemStackExtension#isPiglinCurrency` | 2 | `IItemStackExtension` (ItemStack) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `IItemStackExtension#canFitInsideContainerItems` | 2 | `IItemStackExtension` (ItemStack) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `ILivingEntityExtension#onDamageTaken` | 2 | `ILivingEntityExtension` (LivingEntity) | `LivingEntity` (pumpkin-core `entity/living.rs`), new hook | planned |
| `IOwnedSpawner#getOwner` | 2 | `IOwnedSpawner` (BaseSpawner, TrialSpawner) | - | not supported |
| `ITagAppenderExtension#replace` | 2 | `ITagAppenderExtension` (TagAppender) | - | not supported |
| `ITransformationExtension#blockCenterToCorner` | 2 | `ITransformationExtension` (Transformation) | - | not supported |
| `ITransformationExtension#applyOrigin` | 2 | `ITransformationExtension` (Transformation) | - | not supported |
| `IAttributeExtension#getDebugInfo` | 1 | `IAttributeExtension` (Attribute) | - | not supported |
| `IAttributeExtension#getBaseId` | 1 | `IAttributeExtension` (Attribute) | - | not supported |
| `IBaseRailBlockExtension#canMakeSlopes` | 1 | `IBaseRailBlockExtension` (BaseRailBlock) | rail `BlockBehaviour`, new hook | planned |
| `IBaseRailBlockExtension#isValidRailShape` | 1 | `IBaseRailBlockExtension` (BaseRailBlock) | rail `BlockBehaviour`, new hook | planned |
| `IBlockExtension#makesOpenTrapdoorAboveClimbable` | 1 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#getStateAtViewpoint` | 1 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#canStickTo` | 1 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#onBlockStateChange` | 1 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#getBubbleColumnDirection` | 1 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockStateExtension#isLadder` | 1 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#onDestroyedByPushReaction` | 1 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#addLandingEffects` | 1 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#addRunningEffects` | 1 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#getBeaconColorMultiplier` | 1 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#isStickyBlock` | 1 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#getBlockPathType` | 1 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#collisionExtendsVertically` | 1 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `ICommandSourceStackExtension#getUnsidedLevel` | 1 | `ICommandSourceStackExtension` (CommandSourceStack) | `CommandSender` world | planned |
| `ICommonPacketListener#getConnection` | 1 | `ICommonPacketListener` | - | not supported |
| `IEntityExtension#canFluidExtinguish` | 1 | `IEntityExtension` (Entity) | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | planned |
| `IEntityExtension#sendPairingData` | 1 | `IEntityExtension` (Entity) | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | planned |
| `IFluidExtension#getExplosionResistance` | 1 | `IFluidExtension` (Fluid) | `FluidBehaviour` (pumpkin-core `block/fluid/mod.rs`), new hook | planned |
| `IFluidExtension#canConvertToSource` | 1 | `IFluidExtension` (Fluid) | `FluidBehaviour` (pumpkin-core `block/fluid/mod.rs`), new hook | planned |
| `IFluidExtension#canHydrate` | 1 | `IFluidExtension` (Fluid) | `FluidBehaviour` (pumpkin-core `block/fluid/mod.rs`), new hook | planned |
| `IFluidStateExtension#canConvertToSource` | 1 | `IFluidStateExtension` (FluidState) | `FluidState` forwarding to `FluidBehaviour`, new hook | planned |
| `IFluidStateExtension#canHydrate` | 1 | `IFluidStateExtension` (FluidState) | `FluidState` forwarding to `FluidBehaviour`, new hook | planned |
| `IItemExtension#isPiglinCurrency` | 1 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemExtension#isDamaged` | 1 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemExtension#getSweepHitBox` | 1 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemStackExtension#onStopUsing` | 1 | `IItemStackExtension` (ItemStack) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `IItemStackExtension#makesPiglinsNeutral` | 1 | `IItemStackExtension` (ItemStack) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `IItemStackExtension#canWalkOnPowderedSnow` | 1 | `IItemStackExtension` (ItemStack) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `IItemStackExtension#isNotReplaceableByPickAction` | 1 | `IItemStackExtension` (ItemStack) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `IItemStackExtension#canGrindstoneRepair` | 1 | `IItemStackExtension` (ItemStack) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `ILivingEntityExtension#self` | 1 | `ILivingEntityExtension` (LivingEntity) | - | not supported |
| `ITagBuilderExtension#removeElement` | 1 | `ITagBuilderExtension` (TagBuilder) | - | not supported |
| `ITagBuilderExtension#removeTag` | 1 | `ITagBuilderExtension` (TagBuilder) | - | not supported |
| `ITransformationExtension#transformPosition` | 1 | `ITransformationExtension` (Transformation) | - | not supported |
| `ITransformationExtension#transformNormal` | 1 | `ITransformationExtension` (Transformation) | - | not supported |
| `ITransformationExtension#rotateTransform` | 1 | `ITransformationExtension` (Transformation) | - | not supported |
| `ITransformationExtension#blockCornerToCenter` | 1 | `ITransformationExtension` (Transformation) | - | not supported |
| `TooltipFlagExtension#hasAltDown` | 1 | `TooltipFlagExtension` (TooltipFlag) | - | not supported |
| `TooltipFlagExtension#shouldDisplayAllInformation` | 1 | `TooltipFlagExtension` (TooltipFlag) | - | not supported |
| `BootstrapContextAccessExtension#holderLookup` | 0 | `BootstrapContextAccessExtension` (BootstrapContextAccess) | - | not supported |
| `BootstrapContextExtension#register` | 0 | `BootstrapContextExtension` (BootstrapContext) | - | not supported |
| `ContainerExtension#setItem` | 0 | `ContainerExtension` (Container) | `Inventory` trait (pumpkin-inventory), new hook | planned |
| `ContainerExtension#onTransfer` | 0 | `ContainerExtension` (Container) | `Inventory` trait (pumpkin-inventory), new hook | planned |
| `GameTestHelperExtension#fail` | 0 | `GameTestHelperExtension` (GameTestHelper) | - | not supported |
| `GameTestHelperExtension#getCapability` | 0 | `GameTestHelperExtension` (GameTestHelper) | - | not supported |
| `GameTestHelperExtension#requireCapability` | 0 | `GameTestHelperExtension` (GameTestHelper) | - | not supported |
| `IAbstractBoatExtension#canBoatInFluid` | 0 | `IAbstractBoatExtension` (AbstractBoat) | boat entity, new hook | planned |
| `IAttributeExtension#toBaseComponent` | 0 | `IAttributeExtension` (Attribute) | - | not supported |
| `IAttributeExtension#getMergedStyle` | 0 | `IAttributeExtension` (Attribute) | - | not supported |
| `IAttributeExtension#isNullOrAddition` | 0 | `IAttributeExtension` (Attribute) | - | not supported |
| `IBaseRailBlockExtension#isFlexibleRail` | 0 | `IBaseRailBlockExtension` (BaseRailBlock) | rail `BlockBehaviour`, new hook | planned |
| `IBaseRailBlockExtension#getRailDirection` | 0 | `IBaseRailBlockExtension` (BaseRailBlock) | rail `BlockBehaviour`, new hook | planned |
| `IBlockEntityExtension#getPersistentData` | 0 | `IBlockEntityExtension` (BlockEntity) | `attachments::block_entity` | not supported |
| `IBlockEntityExtension#applyStructureRotation` | 0 | `IBlockEntityExtension` (BlockEntity) | `BlockEntity` trait (pumpkin-core `block/entities/mod.rs`), new hook | planned |
| `IBlockExtension#ignitedByLava` | 0 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#isBurning` | 0 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#onDestroyedByPushReaction` | 0 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#getRespawnPosition` | 0 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#playFallSound` | 0 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#playStepSound` | 0 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#isSlimeBlock` | 0 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#isEmpty` | 0 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#getBounceRestitution` | 0 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockExtension#getRelocability` | 0 | `IBlockExtension` (Block) | `BlockBehaviour` (pumpkin-core `block/mod.rs`), new hook | planned |
| `IBlockStateExtension#getRespawnPosition` | 0 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#playFallSound` | 0 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#playStepSound` | 0 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#onTreeGrow` | 0 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#getStateAtViewpoint` | 0 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#isSlimeBlock` | 0 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#isBurning` | 0 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#getAdjacentBlockPathType` | 0 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#isScaffolding` | 0 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#onBlockStateChange` | 0 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#getBubbleColumnDirection` | 0 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#getBounceRestitution` | 0 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `IBlockStateExtension#getRelocability` | 0 | `IBlockStateExtension` (BlockState) | `BlockState` forwarding to `BlockBehaviour`, new hook | planned |
| `ICommandSourceExtension#getGameProfile` | 0 | `ICommandSourceExtension` (CommandSource) | `CommandSender` (pumpkin-core `command/mod.rs`) | planned |
| `ICommandSourceStackExtension#getScoreboard` | 0 | `ICommandSourceStackExtension` (CommandSourceStack) | `CommandSender`, scoreboard of the server | planned |
| `ICommandSourceStackExtension#getAdvancement` | 0 | `ICommandSourceStackExtension` (CommandSourceStack) | `CommandSender`, advancement lookup | planned |
| `ICommandSourceStackExtension#getSourceProfile` | 0 | `ICommandSourceStackExtension` (CommandSourceStack) | `CommandSender` (pumpkin-core `command/mod.rs`) | planned |
| `ICommonPacketListener#send` | 0 | `ICommonPacketListener` | `pumpkin_core` payload send of the connection | planned |
| `ICommonPacketListener#disconnect` | 0 | `ICommonPacketListener` | `JavaClient` kick (pumpkin-core `net/java/mod.rs`) | planned |
| `ICommonPacketListener#getPacketProcessor` | 0 | `ICommonPacketListener` | - | not supported |
| `ICommonPacketListener#getConnectionType` | 0 | `ICommonPacketListener` | `NegotiatedChannels::connection_type` | supported |
| `IEnchantmentExtension` | 0 | `IEnchantmentExtension` | - | not supported |
| `IEntityExtension#captureDrops` | 0 | `IEntityExtension` (Entity) | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | planned |
| `IEntityExtension#getPersistentData` | 0 | `IEntityExtension` (Entity) | `Entity::attachments` | not supported |
| `IEntityExtension#canBeRiddenUnderFluidType` | 0 | `IEntityExtension` (Entity) | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | planned |
| `IEntityExtension#canTrample` | 0 | `IEntityExtension` (Entity) | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | planned |
| `IEntityExtension#isAddedToLevel` | 0 | `IEntityExtension` (Entity) | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | planned |
| `IEntityExtension#onAddedToLevel` | 0 | `IEntityExtension` (Entity) | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | planned |
| `IEntityExtension#onRemovedFromLevel` | 0 | `IEntityExtension` (Entity) | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | planned |
| `IEntityExtension#revive` | 0 | `IEntityExtension` (Entity) | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | planned |
| `IEntityExtension#getFluidTypeHeight` | 0 | `IEntityExtension` (Entity) | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | planned |
| `IEntityExtension#getMaxHeightFluidType` | 0 | `IEntityExtension` (Entity) | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | planned |
| `IEntityExtension#getFirstEyeInFluidType` | 0 | `IEntityExtension` (Entity) | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | planned |
| `IEntityExtension#canStartSwimming` | 0 | `IEntityExtension` (Entity) | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | planned |
| `IEntityExtension#getFluidMotionScale` | 0 | `IEntityExtension` (Entity) | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | planned |
| `IEntityExtension#canSwimInFluidType` | 0 | `IEntityExtension` (Entity) | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | planned |
| `IEntityExtension#getFluidFallDistanceModifier` | 0 | `IEntityExtension` (Entity) | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | planned |
| `IEntityExtension#canHydrateInFluidType` | 0 | `IEntityExtension` (Entity) | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | planned |
| `IEntityExtension#getSoundFromFluidType` | 0 | `IEntityExtension` (Entity) | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | planned |
| `IEntityExtension#copyAttachmentsFrom` | 0 | `IEntityExtension` (Entity) | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | planned |
| `IEntityExtension#getBlockBounciness` | 0 | `IEntityExtension` (Entity) | `EntityBase` trait (pumpkin-core `entity/mod.rs`), new hook | planned |
| `IFallableExtension#fallingTick` | 0 | `IFallableExtension` (Fallable) | falling block `BlockBehaviour`, new hook | planned |
| `IFluidExtension#getFluidType` | 0 | `IFluidExtension` (Fluid) | `FluidBehaviour` (pumpkin-core `block/fluid/mod.rs`), new hook | planned |
| `IFluidExtension#supportsBoating` | 0 | `IFluidExtension` (Fluid) | `FluidBehaviour` (pumpkin-core `block/fluid/mod.rs`), new hook | planned |
| `IFluidExtension#getBlockPathType` | 0 | `IFluidExtension` (Fluid) | `FluidBehaviour` (pumpkin-core `block/fluid/mod.rs`), new hook | planned |
| `IFluidExtension#getAdjacentBlockPathType` | 0 | `IFluidExtension` (Fluid) | `FluidBehaviour` (pumpkin-core `block/fluid/mod.rs`), new hook | planned |
| `IFluidExtension#canExtinguish` | 0 | `IFluidExtension` (Fluid) | `FluidBehaviour` (pumpkin-core `block/fluid/mod.rs`), new hook | planned |
| `IFluidExtension#appendHoverText` | 0 | `IFluidExtension` (Fluid) | - | not supported |
| `IFluidStateExtension#supportsBoating` | 0 | `IFluidStateExtension` (FluidState) | `FluidState` forwarding to `FluidBehaviour`, new hook | planned |
| `IFluidStateExtension#getBlockPathType` | 0 | `IFluidStateExtension` (FluidState) | `FluidState` forwarding to `FluidBehaviour`, new hook | planned |
| `IFluidStateExtension#getAdjacentBlockPathType` | 0 | `IFluidStateExtension` (FluidState) | `FluidState` forwarding to `FluidBehaviour`, new hook | planned |
| `IFluidStateExtension#canExtinguish` | 0 | `IFluidStateExtension` (FluidState) | `FluidState` forwarding to `FluidBehaviour`, new hook | planned |
| `IFriendlyByteBufExtension#readArray` | 0 | `IFriendlyByteBufExtension` (FriendlyByteBuf) | - | not supported |
| `IFriendlyByteBufExtension#writeArray` | 0 | `IFriendlyByteBufExtension` (FriendlyByteBuf) | - | not supported |
| `IFriendlyByteBufExtension#readMap` | 0 | `IFriendlyByteBufExtension` (FriendlyByteBuf) | - | not supported |
| `IFriendlyByteBufExtension#writeMap` | 0 | `IFriendlyByteBufExtension` (FriendlyByteBuf) | - | not supported |
| `IHolderExtension#unwrapLookup` | 0 | `IHolderExtension` (Holder) | `registry::key_of`, `registry::id_of` | not supported |
| `IHolderExtension#key` | 0 | `IHolderExtension` (Holder) | `ironpumpkin_mods::registry::key_of` (a holder is a name in IronPumpkin) | supported |
| `IHolderExtension#keyOrNull` | 0 | `IHolderExtension` (Holder) | `ironpumpkin_mods::registry::key_of` (a holder is a name in IronPumpkin) | supported |
| `IHolderSetExtension#addInvalidationListener` | 0 | `IHolderSetExtension` (HolderSet) | `registry::tag_entries` | not supported |
| `IHolderSetExtension#serializationType` | 0 | `IHolderSetExtension` (HolderSet) | `registry::tag_entries` | not supported |
| `IHolderSetExtension#isImmediatelyResolvable` | 0 | `IHolderSetExtension` (HolderSet) | `registry::tag_entries` | not supported |
| `IHolderSetExtension.SerializationType` | 0 | `IHolderSetExtension.SerializationType` | - | not supported |
| `IItemExtension#isCombineRepairable` | 0 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemExtension#getCraftingRemainder` | 0 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemExtension#getEntityLifespan` | 0 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemExtension#onAnimalArmorTick` | 0 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemExtension#isGazeDisguise` | 0 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemExtension#canGrindstoneRepair` | 0 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemExtension#onGlideDamage` | 0 | `IItemExtension` (Item) | `ItemBehaviour` (pumpkin-core `item/mod.rs`), new hook | planned |
| `IItemStackExtension#onItemUseFirst` | 0 | `IItemStackExtension` (ItemStack) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `IItemStackExtension#onEntitySwing` | 0 | `IItemStackExtension` (ItemStack) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `IItemStackExtension#getEntityLifespan` | 0 | `IItemStackExtension` (ItemStack) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `IItemStackExtension#onEntityItemUpdate` | 0 | `IItemStackExtension` (ItemStack) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `IItemStackExtension#onAnimalArmorTick` | 0 | `IItemStackExtension` (ItemStack) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `IItemStackExtension#onDroppedByPlayer` | 0 | `IItemStackExtension` (ItemStack) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `IItemStackExtension#isCombineRepairable` | 0 | `IItemStackExtension` (ItemStack) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `IItemStackExtension#isGazeDisguise` | 0 | `IItemStackExtension` (ItemStack) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `IItemStackExtension#getSweepHitBox` | 0 | `IItemStackExtension` (ItemStack) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `IItemStackExtension#onDestroyed` | 0 | `IItemStackExtension` (ItemStack) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `IItemStackExtension#onGlideDamage` | 0 | `IItemStackExtension` (ItemStack) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `ILevelExtension#getMaxEntityRadius` | 0 | `ILevelExtension` (Level) | - | not supported |
| `ILevelExtension#increaseMaxEntityRadius` | 0 | `ILevelExtension` (Level) | - | not supported |
| `ILevelExtension#getDescriptionKey` | 0 | `ILevelExtension` (Level) | - | not supported |
| `ILivingEntityExtension#canSwimInFluidType` | 0 | `ILivingEntityExtension` (LivingEntity) | `LivingEntity` (pumpkin-core `entity/living.rs`), new hook | planned |
| `ILivingEntityExtension#jumpInFluid` | 0 | `ILivingEntityExtension` (LivingEntity) | `LivingEntity` (pumpkin-core `entity/living.rs`), new hook | planned |
| `ILivingEntityExtension#sinkInFluid` | 0 | `ILivingEntityExtension` (LivingEntity) | `LivingEntity` (pumpkin-core `entity/living.rs`), new hook | planned |
| `ILivingEntityExtension#moveInFluid` | 0 | `ILivingEntityExtension` (LivingEntity) | `LivingEntity` (pumpkin-core `entity/living.rs`), new hook | planned |
| `IMenuProviderExtension#shouldTriggerClientSideContainerClosingOnOpen` | 0 | `IMenuProviderExtension` (MenuProvider) | `menus::MenuDefinition` | planned |
| `IMenuProviderExtension#writeClientSideData` | 0 | `IMenuProviderExtension` (MenuProvider) | `menus::MenuDefinition` | planned |
| `IMenuTypeExtension#create` | 0 | `IMenuTypeExtension` (MenuType) | `registration.menu-type-definition.extra-data` | planned |
| `IPacketFlowExtension#self` | 0 | `IPacketFlowExtension` (PacketFlow) | - | not supported |
| `IPlayerExtension#isCloseEnough` | 0 | `IPlayerExtension` (Player) | `Player` (pumpkin-core `entity/player.rs`) | planned |
| `IPlayerListExtension#self` | 0 | `IPlayerListExtension` (PlayerList) | - | not supported |
| `IPlayerListExtension#broadcastAll` | 0 | `IPlayerListExtension` (PlayerList) | `ironpumpkin_mods::network` payload send helpers | planned |
| `IPlayerListExtension#broadcast` | 0 | `IPlayerListExtension` (PlayerList) | `ironpumpkin_mods::network` payload send helpers | planned |
| `IServerChunkCacheExtension#self` | 0 | `IServerChunkCacheExtension` (ServerChunkCache) | - | not supported |
| `IServerChunkCacheExtension#sendToTrackingPlayersAndSelf` | 0 | `IServerChunkCacheExtension` (ServerChunkCache) | `ironpumpkin_mods::network` payload send helpers | planned |
| `IServerChunkCacheExtension#sendToTrackingPlayers` | 0 | `IServerChunkCacheExtension` (ServerChunkCache) | `ironpumpkin_mods::network` payload send helpers | planned |
| `IServerGamePacketListenerExtension#sendBundled` | 0 | `IServerGamePacketListenerExtension` (ServerGamePacketListener) | `ironpumpkin_mods::network` payload send helpers | planned |
| `ITagAppenderExtension#addOptionalTags` | 0 | `ITagAppenderExtension` (TagAppender) | - | not supported |
| `ITagAppenderExtension#add` | 0 | `ITagAppenderExtension` (TagAppender) | - | not supported |
| `ItemInstanceExtension#canPerformAction` | 0 | `ItemInstanceExtension` (ItemInstance) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `ItemInstanceExtension#getTagEnchantments` | 0 | `ItemInstanceExtension` (ItemInstance) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `ItemInstanceExtension#getEnchantmentLevel` | 0 | `ItemInstanceExtension` (ItemInstance) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `ItemInstanceExtension#getCraftingRemainder` | 0 | `ItemInstanceExtension` (ItemInstance) | `ItemStack` forwarding to `ItemBehaviour`, new hook | planned |
| `LootTableSubProviderContextExtension#withConditions` | 0 | `LootTableSubProviderContextExtension` (LootTableSubProvider.Context) | - | not supported |
| `PackMetadataResourcesExtension#isHidden` | 0 | `PackMetadataResourcesExtension` (PackMetadataResources) | - | not supported |
| `PendingTagsExtension#contents` | 0 | `PendingTagsExtension` (Registry.PendingTags) | - | not supported |
| `TooltipFlagExtension#hasControlDown` | 0 | `TooltipFlagExtension` (TooltipFlag) | - | not supported |
| `TypedInstanceExtension#getData` | 0 | `TypedInstanceExtension` (TypedInstance) | `ironpumpkin_mods::data_maps::DataMapType::get_data` | supported |
| `ValueInputExtension#keySet` | 0 | `ValueInputExtension` (ValueInput) | `NbtCompound` in `EntityBase::read_custom_nbt` | not supported |
| `ValueInputExtension#readChild` | 0 | `ValueInputExtension` (ValueInput) | `NbtCompound` in `EntityBase::read_custom_nbt` | not supported |
| `ValueInputExtension#rawChildOrEmpty` | 0 | `ValueInputExtension` (ValueInput) | `NbtCompound` in `EntityBase::read_custom_nbt` | not supported |
| `ValueOutputExtension#store` | 0 | `ValueOutputExtension` (ValueOutput) | `NbtCompound` in `EntityBase::write_custom_nbt` | not supported |
| `ValueOutputExtension#putChild` | 0 | `ValueOutputExtension` (ValueOutput) | `NbtCompound` in `EntityBase::write_custom_nbt` | not supported |

### Other packages

| NeoForge item | Mods | Section | Counterpart | Status |
|:--|--:|:--|:--|:--|
| `NeoForge` | 219 | Types in the root `common` package | `ModInit::register_event` (`NeoForge.EVENT_BUS`) | supported |
| `transfer` | 189 | Packages outside the families above | `ironpumpkin_mods::transfer::ResourceHandler`, `CombinedResourceHandler`, `ResourceStacksResourceHandler` | planned |
| `ModConfigSpec` | 182 | Types in the root `common` package | `ironpumpkin_mods::config::ConfigSpec` | supported |
| `transfer.item` | 149 | Packages outside the families above | `ironpumpkin_mods::transfer::ItemResource`, `ItemStacksResourceHandler`, `transfer::inventory` | planned |
| `common.util` | 133 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `transfer.fluid` | 130 | Packages outside the families above | `ironpumpkin_mods::transfer::FluidResource`, `FluidStacksResourceHandler` | planned |
| `fluids` | 122 | Packages outside the families above | `types` doc (other packages), not supported | planned |
| `transfer.energy` | 78 | Packages outside the families above | `ironpumpkin_mods::transfer::EnergyHandler`, `SimpleEnergyHandler` | planned |
| `Tags` | 77 | Types in the root `common` package | `registry::tag_entries`, `registry::is_in_tag` (tag ids); 5 of 10 nested classes complete | planned |
| `server` | 59 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `Tags.Items` | 51 | Types in the root `common` package | `registry::tag_entries`, `registry::is_in_tag` (tag ids) | planned |
| `ItemAbility` | 45 | Types in the root `common` package | `types` doc (other packages), not supported | planned |
| `common.crafting` | 43 | Packages outside the families above | `types` doc (other packages), not supported | planned |
| `CommonHooks` | 42 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `ItemAbilities` | 41 | Types in the root `common` package | `types` doc (other packages), not supported | planned |
| `NeoForgeMod` | 30 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `Tags.Blocks` | 30 | Types in the root `common` package | `registry::tag_entries`, `registry::is_in_tag` (tag ids) | planned |
| `fluids.crafting` | 28 | Packages outside the families above | `types` doc (other packages), not supported | planned |
| `SoundAction` | 23 | Types in the root `common` package | `types` doc (other packages), not supported | planned |
| `common.world` | 22 | Packages outside the families above | `types` doc (other packages), not supported | planned |
| `entity` | 20 | Packages outside the families above | gap: `IEntityWithComplexSpawn` spawn data needs the M3 `advanced_add_entity` payload; `PartEntity` has no target (documented in `types`) | planned |
| `MutableDataComponentHolder` | 20 | Types in the root `common` package | `ItemStack::{get_data_component, set_data_component, remove_data_component, has_data_component}`; `data_components::DataComponentType::{get, set, remove, has}` for mod components | supported |
| `SoundActions` | 20 | Types in the root `common` package | `types` doc (other packages), not supported | planned |
| `Tags.EntityTypes` | 20 | Types in the root `common` package | `registry::tag_entries`, `registry::is_in_tag` (tag ids) | supported |
| `common.damagesource` | 18 | Packages outside the families above | `types.damage-container`, `damage-reduction`, `event-bus.reduction-modifier`, `callbacks.apply-reduction-modifier` | planned |
| `common.world.chunk` | 13 | Packages outside the families above | `types` doc (other packages), not supported | planned |
| `Tags.Fluids` | 12 | Types in the root `common` package | `registry::tag_entries`, `registry::is_in_tag` (tag ids) | supported |
| `IShearable` | 11 | Types in the root `common` package | gap: shear hook for mod blocks and entities; M5 | planned |
| `Tags.DamageTypes` | 11 | Types in the root `common` package | `registry::tag_entries`, `registry::is_in_tag` (tag ids) | planned |
| `UsernameCache` | 10 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `server.command` | 9 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `resource` | 8 | Packages outside the families above | `lifecycle.add-server-reload-listeners-event`, `callbacks.on-server-reload` | planned |
| `SpecialPlantable` | 8 | Types in the root `common` package | `types` doc (other packages), not supported | planned |
| `server.permission` | 7 | Packages outside the families above | `pumpkin:plugin/player.has-permission` (`PermissionAPI.getPermission`) | supported |
| `server.permission.nodes` | 7 | Packages outside the families above | `pumpkin:plugin/permission.permission` via `event-bus` `permission-gather-event-nodes` | planned |
| `PercentageAttribute` | 7 | Types in the root `common` package | `entity_attributes::AttributeBuilder::ranged` | supported |
| `Tags.Biomes` | 4 | Types in the root `common` package | `registry::tag_entries`, `registry::is_in_tag` (tag ids) | planned |
| `BooleanAttribute` | 3 | Types in the root `common` package | `entity_attributes::AttributeBuilder::boolean` | supported |
| `CreativeModeTabRegistry` | 3 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `TranslatableEnum` | 3 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `common.world.poi` | 2 | Packages outside the families above | `registration.extend-poi-types-event` | planned |
| `FarmlandWaterManager` | 2 | Types in the root `common` package | `types` doc (other packages), not supported | planned |
| `common.enums` | 1 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `common.ticket` | 1 | Packages outside the families above | `types` doc (other packages), not supported | planned |
| `internal` | 1 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `IOUtilities` | 1 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `LenientUnboundedMapCodec` | 1 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `common.advancements.critereon` | 0 | Packages outside the families above | `types` doc (other packages), not supported | planned |
| `common.command` | 0 | Packages outside the families above | `types` doc (other packages), not supported | planned |
| `common.data` | 0 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `common.data.fixes` | 0 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `common.data.internal` | 0 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `common.tooltip` | 0 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `common.util.flag` | 0 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `common.util.strategy` | 0 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `data.event` | 0 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `data.loading` | 0 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `fluids.crafting.display` | 0 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `gametest` | 0 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `junit` | 0 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `logging` | 0 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `mixins` | 0 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `model.data` | 0 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `server.command.generation` | 0 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `server.console` | 0 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `server.dedicated` | 0 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `server.jsonrpc` | 0 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `server.loading` | 0 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `server.permission.exceptions` | 0 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `server.permission.handler` | 0 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `server.timings` | 0 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `transfer.access` | 0 | Packages outside the families above | `ironpumpkin_mods::transfer::ItemAccess`, `StackItemAccess` | planned |
| `transfer.resource` | 0 | Packages outside the families above | `ironpumpkin_mods::transfer::Resource`, `ResourceStack` | planned |
| `transfer.transaction` | 0 | Packages outside the families above | `ironpumpkin_mods::transfer::Transaction`, `TransactionContext`, `SnapshotJournal`, `RootCommitJournal` | supported |
| `world.inventory` | 0 | Packages outside the families above | gap: custom menu slots are M5 (`registration.menu-type-definition`) | planned |
| `CommonHooks.BiomeCallbackFunction` | 0 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `DataMapHooks` | 0 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `IMinecartCollisionHandler` | 0 | Types in the root `common` package | `types` doc (other packages), not supported | planned |
| `IOUtilities.WriteCallback` | 0 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `MonsterRoomHooks` | 0 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `NeoForgeBuildType` | 0 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `NeoForgeEventHandler` | 0 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `OnlyInWarningsHandler` | 0 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `Tags.Enchantments` | 0 | Types in the root `common` package | `registry::tag_entries`, `registry::is_in_tag` (tag ids) | supported |
| `Tags.Potions` | 0 | Types in the root `common` package | `registry::tag_entries`, `registry::is_in_tag` (tag ids) | supported |
| `Tags.Structures` | 0 | Types in the root `common` package | `registry::tag_entries`, `registry::is_in_tag` (tag ids) | supported |
| `Tags.WorldClocks` | 0 | Types in the root `common` package | `registry::tag_entries`, `registry::is_in_tag` (tag ids) | planned |

## Modpack coverage

The reference modpack has 414 mods. 405 of them need at least one gap or contract-only row of the contract as the scan found it (section 11.1 of `modpack-usage.md`). This section gives the verdict for each of the 30 largest gaps, the gaps that are out of scope by nature, and the classes outside the inventory with their counterpart.

### Top 30 gaps and verdicts

Gaps as section 11.2 of the scan groups them, by the number of mods that need at least one row of the group. A verdict is one of: add now (the API addition of this change), plan (the task and phase), or out of scope (the reason).

| # | Gap | Mods | Largest rows (mods) | Verdict |
|--:|:--|--:|:--|:--|
| 1 | Java helpers | 312 | ChatFormatting(248), RandomSource(200), Mth(191), StringRepresentable(127) | out of scope: mod code; Rust std and `ironpumpkin-mods` replace `Mth`, `RandomSource` and `StringRepresentable`, and `ChatFormatting` is the style of the v0.2 `text-component` (`types` doc) |
| 2 | Mixins | 291 | [[mixins]] config(291), file mixins(278), [[mixins]] requiredMods(2), [[mixins]] behaviorVersion(0) | plan, native channel: source patch. A server mixin, and a use of reflection such as `ObfuscationReflectionHelper`, becomes a NeoForge event listener where it injects at the start or the end of a target method that fires an inventory event (94 of 2674 server-side mixin members in the scan of #51), and a source patch that the modpack build applies otherwise (2580 members, among them every change inside a method body; task 2.1.2); a patch is accepted only with a justification file next to it that says what it does, why a NeoForge event or API is not enough, and that it is bound to the IronPumpkin commit it names; every accepted patch is a candidate event with NeoForge semantics for a later version. `@Unique` state becomes an attachment (task 2.14). Client mixins and mixins that change generated data: out of scope by nature (below) |
| 3 | DFU codecs | 278 | Codec(244), MapCodec(220), PrimitiveCodec(196), Products(195) | out of scope by nature (below) |
| 4 | Stream codecs and byte buffers | 263 | StreamCodec(252), RegistryFriendlyByteBuf(226), ByteBufCodecs(204) | out of scope by nature (below): a payload is the bytes the mod encodes |
| 5 | Registries the host rejects | 247 | DataComponentType(232), data_component_type(113), creative_mode_tab(112), sound_event(55) | supported (task 2.6, #123): `data_component_type`, `sound_event`, `mob_effect` and `attribute` register through `ModInit` and `RegisterEvent` and are used through the `data_components`, `sound_events`, `mob_effects` and `entity_attributes` modules of `ironpumpkin-mods`; `creative_mode_tab` stays accepted and ignored (client UI); `fluid` and `neoforge:fluid_type`: plan, M5, no task yet (custom fluids) |
| 6 | Access transformers | 183 | file accessTransformers(55), [[accessTransformers]] file(55) | plan, native channel: source patch. A member that an access transformer widens is made visible by a source patch of the mod (task 2.1.2); a patch is accepted only with a justification file next to it that says what it does, why a NeoForge event or API is not enough, and that it is bound to the IronPumpkin commit it names; every accepted patch is a candidate event with NeoForge semantics for a later version |
| 7 | Container menus | 176 | AbstractContainerMenu(172), MenuProvider(121), MenuConstructor(51), SimpleMenuProvider(45) | add now: interface `menus` (resource `menu`, `open-menu`, `get-open-menu`) and the callbacks `menu-clicked`, `menu-quick-move-stack`, `menu-still-valid`, `menu-click-button`, `menu-slots-changed`, `menu-removed`, `menu-slot-may-place`, `menu-slot-on-take` (task 2.9; vanilla menu types in M4, mod menu types in M5) |
| 8 | Block shapes | 170 | VoxelShape(167), CollisionContext(134) | supported (task 2.10, #155): the `shapes` module of `ironpumpkin-mods` (`get_block_shape`, `get_state_shape`, `is_face_sturdy`, `no_collision`, `VoxelShape`) and `BlockBuilder::shape` and `collision_shape`, evaluated per state at the freeze. Shapes per position or entity (`dynamicShape`) are not supported; the interaction, block support, occlusion and visual shapes of vanilla blocks are the vanilla defaults |
| 9 | Ingredients and trades | 146 | Ingredient(141), ItemCost(17), Merchant(6) | add now as a mapping (`types` doc): `Ingredient` is the v0.2 `recipe.ingredient` of `recipe-manager`. Trades: out of scope as code: 26.3 removed the trade events and keeps trades as `minecraft:villager_trade` datapack entries that a port ships as JSON; `trade-with-villager-event` (task 2.12) is the code hook |
| 10 | Package common.util | 133 | common.util(133), common.util.flag(0), common.util.strategy(0) | out of scope: Java helpers (`Lazy`, `INBTSerializable`, which is `nbt-tree` data); `TriState` and `BlockSnapshot` already map to `types.tri-state` and `types.block-snapshot`; `FakePlayer` below |
| 11 | Packages fluids | 123 | fluids(122), fluids.crafting(28), fluids.crafting.display(0) | plan: `FluidStack` and fluid handlers through the transfer handlers of task 2.7 (`fluid-resource` and an amount); custom fluids and `FluidUtil` interactions in M5, no task yet |
| 12 | Static registries without API | 118 | recipe_serializer(74), recipe_type(49), particle_type(41), command_argument_type(21) | plan, M5, no task yet: `recipe_type` and `recipe_serializer` (mod machine recipes as datapack JSON). Worldgen types: plan as an M5 engine extension (a registration API for the world generation registry types), as for the world generation gap. Out of scope: `particle_type` and `command_argument_type` (client-synced registries without a Pumpkin registry type), loot and trigger types |
| 13 | Creative tab contents | 84 | BuildCreativeModeTabContentsEvent(75), CreativeModeTab.TabVisibility(26) | out of scope: client UI; the host accepts the tab entry and ignores it |
| 14 | RenderShape | 81 | RenderShape(81) | out of scope by nature (client rendering) |
| 15 | World generation | 72 | Heightmap(58), Heightmap.Types(58), Feature(24), GenerationStep(21) | plan, M5 engine extension: a native mod registers features, placements and structures through a registration API for the world generation registry types, and a change to the generator itself is a source patch (task 2.1.2); `Heightmap` reads map to the v0.2 `get-top-block-y` and `get-motion-blocking-height`; which NeoForge world generation rows the API backs, biome modifiers among them, waits for the owner decision on ores (status issue) |
| 16 | FakePlayer | 63 | FakePlayer(63) | out of scope: Pumpkin has no fake players; a mod calls the `World` mutators with the entity that acts (`World::destroy_block`, `World::place_block`), and the one event pipeline (tasks 2.4 and 2.18) fires the events a fake player would trigger; the table "Mutators and their events" of `design.md` gives the cause of each event |
| 17 | Package server | 63 | server(59), server.command(9), server.command.generation(0), server.console(0) | add now as a mapping (`types` doc): `ServerLifecycleHooks#getCurrentServer` is the v0.2 `context.get-server`; the console, timings and command generation packages: out of scope (internal) |
| 18 | Custom game rules | 53 | GameRules(53) | plan, M5, no task yet: needs a `game_rule` registry, and Pumpkin game rules are a generated enum; reading vanilla rules works through the v0.2 `game-rules` |
| 19 | Data conditions | 52 | ICondition(42), ICondition.IContext(32), ConditionalOps(12), WithConditions(8) | plan, M5, no task yet: the datapack loader evaluates the built-in `neoforge:conditions` (mod loaded, item exists, tag empty, and, or, not); custom condition types stay out (`neoforge:condition_codecs`) |
| 20 | Custom registries | 48 | RegistryBuilder(47), NewRegistryEvent(31), DeferredRegister#makeRegistry(12), ModifyRegistriesEvent(2) | out of scope: new registries need mutable registry types, and Pumpkin registries are generated; a mod keeps its own lookup tables in its own code |
| 21 | Item abilities | 45 | ItemAbility(45), ItemAbilities(41) | out of scope: Pumpkin item behaviours hard-code tool actions; the ability name reaches mods as `item_ability` on `BlockEventBlockToolModificationEvent` |
| 22 | Package common.crafting | 43 | common.crafting(43) | out of scope: custom ingredient types need the `neoforge:ingredient_serializer` registry; plain ingredients are the v0.2 `recipe.ingredient`, and `SizedIngredient` is an ingredient and a count in mod code |
| 23 | CommonHooks | 42 | CommonHooks(42) | out of scope: helpers that fire events by hand; under the one event pipeline (tasks 2.4 and 2.18) the host fires them for every mutation that a player or an entity causes, and `event-bus.post` covers the rest |
| 24 | Packages common.world | 34 | common.world(22), common.world.chunk(13) | out of scope: biome modifiers (world generation, owner decision) and forced chunk tickets (`RegisterTicketControllersEvent`; pumpkin-world does not expose tickets) |
| 25 | Package network.codec | 34 | network.codec(34) | out of scope by nature (stream codec helpers; payloads are bytes) |
| 26 | NeoForge registries without API | 32 | neoforge:condition_codecs(16), neoforge:biome_modifier_serializers(10), neoforge:entity_data_serializers(9), neoforge:structure_modifier_serializers(1) | out of scope: codec and serializer registries for Java types (`condition_codecs`, `biome_modifier_serializers`, `entity_data_serializers`, `structure_modifier_serializers`, `holder_set_type`, `ingredient_serializer`) |
| 27 | ModList file data | 30 | ModList#getModFileById(16), ModList#getAllScanData(16), ModList#getModFiles(3), ModList#forEachModFile(2) | out of scope: mod files and scan data describe Java jars, and a Rust mod has none; the `lifecycle` mod list gives the mod metadata |
| 28 | NeoForgeMod constants | 30 | NeoForgeMod(30) | partly supported (task 2.6, #123): the host registers `neoforge:swim_speed`, `neoforge:creative_flight` and `neoforge:gliding_flight` with the ids of a `NeoForge` server, `entity_attributes::SWIM_SPEED`, `CREATIVE_FLIGHT` and `GLIDING_FLIGHT` name them, swim speed and creative flight apply, an equipped glider item gives gliding flight; the milk fluid follows custom fluids (M5, no task yet) |
| 29 | Tooltip events | 28 | ItemTooltipEvent(28), AddAttributeTooltipsEvent(0), GatherSkippedAttributeTooltipsEvent(0) | out of scope by nature (client rendering: the client builds tooltips) |
| 30 | ArmorMaterial.Layer | 24 | ArmorMaterial.Layer(24) | out of scope by nature (client rendering: equipment layers are a client asset; armor is the `minecraft:equippable` component) |

Gaps ranked 31 to 60 in section 11.2 of the scan need 3 to 23 mods each. They keep the reason of their row in the tables of this file; the trade events among them (`VillagerTradesEvent`, `WandererTradesEvent`, 15 mods) follow the trade verdict of the ingredients and trades gap.

### Out of scope by nature

- Client mixins: a dedicated server never runs client code.
- Mixins that change generated data (block states, registries and the other tables under `crates/pumpkin-data/src/generated`): a source patch never touches generated files, so the change goes into IronPumpkin itself through pumpkin-codegen, decided per case. Every other server mixin, including one that changes a packet field or the save format, is a NeoForge event listener or a source patch (the mixins gap above).
- DataFixerUpper codecs, stream codecs and byte buffers: a Java serialization framework. Mods serialize with Rust code, and data crosses the API as JSON, NBT or bytes.
- Client rendering (render shapes, tooltips, creative tab contents, equipment layers, screens): the client draws it, and a dedicated server never runs that code.

### Classes outside the inventory

Minecraft, NeoForge and Mojang classes that ported mods use and that the inventory does not list as a row, by family. `26.3` is `same` when a class with this name exists in 26.3 and `CHANGED` when it does not; the counterpart follows the 26.3 shape.

#### NeoForge registries

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.neoforged.neoforge.registries.DeferredHolder` | 224 | same | no handle type: the name and `registry::id_of`, `key_of`, `is_in_tag` |
| `net.neoforged.neoforge.registries.DeferredRegister` | 207 | same | `registration.deferred-register` |
| `net.neoforged.neoforge.registries.NeoForgeRegistries` | 111 | same | `registration` registry ids (`neoforge:*`) |
| `net.neoforged.neoforge.registries.DeferredItem` | 104 | same | no handle type: the name and `Item::from_registry_key`, `registry::id_of` |
| `net.neoforged.neoforge.registries.DeferredRegister$Items` | 86 | same | `registration.deferred-register-items` |
| `net.neoforged.neoforge.registries.DeferredBlock` | 78 | same | no handle type: the name and `Block::from_name`, `registry::id_of` |
| `net.neoforged.neoforge.registries.NeoForgeRegistries$Keys` | 77 | same | `registration` registry ids (`neoforge:*`) |
| `net.neoforged.neoforge.registries.DeferredRegister$Blocks` | 67 | same | `registration.deferred-register-blocks` |
| `net.neoforged.neoforge.registries.DeferredRegister$DataComponents` | 47 | same | `ModInit::register_data_component_type` |

#### NeoForge events and event bus

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.neoforged.bus.api.IEventBus` | 342 | same | `event-bus.add-listener`, `event-bus.post` |
| `net.neoforged.bus.api.SubscribeEvent` | 207 | same | `event-bus.add-listener` |
| `net.neoforged.neoforge.network.event.RegisterPayloadHandlersEvent` | 170 | same | `ironpumpkin_mods::network::RegisterPayloadHandlersEvent` |
| `net.neoforged.fml.event.lifecycle.FMLCommonSetupEvent` | 148 | same | `event-bus.event::fml-common-setup-event` |
| `net.neoforged.fml.common.EventBusSubscriber` | 126 | same | `event-bus.add-listener` |
| `net.neoforged.neoforge.event.entity.player.PlayerEvent` | 96 | same | fields in the subclass records of `event-bus` (abstract) |
| `net.neoforged.neoforge.event.RegisterCommandsEvent` | 93 | same | `ModInit::register_command` (API call, no event) |
| `net.neoforged.neoforge.event.entity.player.PlayerInteractEvent` | 85 | same | fields in the subclass records of `event-bus` (abstract) |
| `net.neoforged.neoforge.event.BuildCreativeModeTabContentsEvent` | 75 | same | gap: not supported (client UI); documented in `event-bus` |
| `net.neoforged.fml.common.EventBusSubscriber$Bus` | 69 | same | `event-bus.bus` |
| `net.neoforged.neoforge.event.entity.player.PlayerEvent$PlayerLoggedInEvent` | 66 | same | `event-bus.event::player-event-player-logged-in-event` |
| `net.neoforged.neoforge.event.level.BlockEvent` | 65 | same | fields in the subclass records of `event-bus` (abstract) |
| `net.neoforged.neoforge.event.EventHooks` | 51 | same | `event-bus.post` |
| `net.neoforged.neoforge.event.entity.EntityJoinLevelEvent` | 49 | same | `event-bus.event::entity-join-level-event` |
| `net.neoforged.neoforge.event.server.ServerStartedEvent` | 40 | same | `pumpkin_core::plugin::server::ServerLoadEvent` |
| `net.neoforged.neoforge.event.entity.living.LivingDamageEvent` | 38 | same | fields in the subclass records of `event-bus` (abstract) |
| `net.neoforged.neoforge.event.entity.living.LivingIncomingDamageEvent` | 37 | same | `event-bus.event::living-incoming-damage-event` |
| `net.neoforged.neoforge.event.tick.PlayerTickEvent` | 36 | same | fields in the subclass records of `event-bus` (abstract) |
| `net.neoforged.neoforge.event.entity.living.LivingDropsEvent` | 31 | same | `event-bus.event::living-drops-event` |
| `net.neoforged.neoforge.event.entity.EntityAttributeCreationEvent` | 29 | same | `NativeMod::entity_attribute_creation` |
| `net.neoforged.neoforge.event.entity.player.PlayerEvent$PlayerLoggedOutEvent` | 28 | same | `event-bus.event::player-event-player-logged-out-event` |
| `net.neoforged.neoforge.event.entity.player.PlayerInteractEvent$LeftClickBlock` | 27 | same | `pumpkin_core::plugin::player::PlayerInteractEvent` (action `LeftClickBlock`, `left_click_action`) |
| `net.neoforged.neoforge.event.entity.player.PlayerEvent$PlayerChangedDimensionEvent` | 26 | same | `event-bus.event::player-event-player-changed-dimension-event` |
| `net.neoforged.neoforge.event.entity.living.LivingDamageEvent$Post` | 25 | same | `event-bus.event::living-damage-event-post` |
| `net.neoforged.neoforge.event.entity.player.PlayerEvent$PlayerRespawnEvent` | 24 | same | `event-bus.event::player-event-player-respawn-event` |
| `net.neoforged.neoforge.event.tick.PlayerTickEvent$Post` | 24 | same | `pumpkin_core::plugin::level::PlayerTickEventPost` |
| `net.neoforged.neoforge.event.level.BlockEvent$EntityPlaceEvent` | 23 | same | `pumpkin_core::plugin::block::BlockPlaceEvent` (a player), `pumpkin_core::plugin::level::BlockEventEntityPlaceEvent` (another entity) |
| `net.neoforged.neoforge.event.entity.player.PlayerEvent$Clone` | 23 | same | `event-bus.event::player-event-clone` |
| `net.neoforged.neoforge.event.entity.player.PlayerEvent$BreakSpeed` | 18 | same | `pumpkin_core::plugin::player::PlayerEventBreakSpeed` |
| `net.neoforged.neoforge.event.level.BlockDropsEvent` | 17 | same | `event-bus.event::block-drops-event` |
| `net.neoforged.neoforge.event.entity.EntityAttributeModificationEvent` | 16 | same | `NativeMod::entity_attribute_modification` |
| `net.neoforged.neoforge.event.entity.living.FinalizeSpawnEvent` | 16 | same | `event-bus.event::finalize-spawn-event` |
| `net.neoforged.neoforge.event.entity.living.LivingEntityUseItemEvent` | 16 | same | `event-bus.event::living-entity-use-item-event` |
| `net.neoforged.neoforge.event.entity.player.AttackEntityEvent` | 15 | same | `event-bus.event::attack-entity-event` |
| `net.neoforged.neoforge.event.entity.living.LivingExperienceDropEvent` | 13 | same | `event-bus.event::living-experience-drop-event` |
| `net.neoforged.neoforge.event.AddPackFindersEvent` | 13 | same | `event-bus.event::add-pack-finders-event` |
| `net.neoforged.neoforge.event.entity.living.MobEffectEvent` | 13 | same | fields in the subclass records of `event-bus` (abstract) |
| `net.neoforged.neoforge.event.entity.player.PlayerEvent$StartTracking` | 13 | same | `pumpkin_core::plugin::player::PlayerEventStartTracking` |
| `net.neoforged.neoforge.event.entity.ProjectileImpactEvent` | 12 | same | `event-bus.event::projectile-impact-event` |
| `net.neoforged.neoforge.event.village.VillagerTradesEvent` | 12 | CHANGED | gap: removed in NeoForge 26.1 (villager trades are datapack data); documented in `event-bus` |
| `net.neoforged.neoforge.event.AnvilUpdateEvent` | 9 | same | `pumpkin_core::plugin::inventory::PrepareAnvilEvent` |
| `net.neoforged.neoforge.event.entity.living.LivingEntityUseItemEvent$Start` | 9 | same | `event-bus.event::living-entity-use-item-event-start` |
| `net.neoforged.neoforge.event.entity.living.LivingKnockBackEvent` | 9 | same | `event-bus.event::living-knock-back-event` |
| `net.neoforged.neoforge.event.village.WandererTradesEvent` | 9 | CHANGED | gap: removed in NeoForge 26.1 (wandering trader trades are datapack data); documented in `event-bus` |
| `net.neoforged.neoforge.event.entity.living.LivingShieldBlockEvent` | 8 | same | `event-bus.event::living-shield-block-event` |
| `net.neoforged.neoforge.event.entity.living.MobEffectEvent$Applicable` | 7 | same | `event-bus.event::mob-effect-event-applicable` |
| `net.neoforged.neoforge.event.entity.living.MobEffectEvent$Applicable$Result` | 7 | same | `event-bus.mob-effect-applicable-result` |
| `net.neoforged.neoforge.network.event.RegisterConfigurationTasksEvent` | 7 | same | `ironpumpkin_mods::network::RegisterConfigurationTasksEvent` |
| `net.neoforged.neoforge.event.entity.EntityTravelToDimensionEvent` | 6 | same | `event-bus.event::entity-travel-to-dimension-event` |
| `net.neoforged.neoforge.event.entity.player.CriticalHitEvent` | 5 | same | `pumpkin_core::plugin::player::CriticalHitEvent` |

#### NeoForge mod loading and config

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.neoforged.fml.common.Mod` | 340 | same | mod registration (`register_mod!`), the mod's `init` and its embedded `neoforge.mods.toml` |
| `net.neoforged.fml.ModContainer` | 246 | same | `lifecycle.mod-container`, `ModInit::register_config` |
| `net.neoforged.api.distmarker.Dist` | 199 | same | `types.dist` |
| `net.neoforged.neoforge.common.ModConfigSpec` | 182 | same | `ironpumpkin_mods::config::ConfigSpec` |
| `net.neoforged.neoforge.common.ModConfigSpec$Builder` | 178 | same | `ironpumpkin_mods::config::ConfigSpecBuilder` |
| `net.neoforged.fml.config.ModConfig` | 165 | same | `ironpumpkin_mods::config::ModConfig` |
| `net.neoforged.fml.config.ModConfig$Type` | 165 | same | `ironpumpkin_mods::config::ConfigType` (`SERVER` is gone in FML 12: use `Synced` or `Local`) |
| `net.neoforged.fml.config.IConfigSpec` | 162 | same | `ironpumpkin_mods::config::ConfigSpec` |
| `net.neoforged.neoforge.common.ModConfigSpec$ConfigValue` | 104 | same | `ironpumpkin_mods::config::ModConfig::get`, `set`, `save` |
| `net.neoforged.fml.loading.FMLLoader` | 64 | same | `lifecycle.is-production`, `lifecycle.get-dist` |

#### NeoForge attachments

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.neoforged.neoforge.attachment.AttachmentType` | 68 | same | `attachments.attachment-type-definition`, `registration.entry-definition::attachment-type` |
| `net.neoforged.neoforge.attachment.AttachmentType$Builder` | 57 | same | `attachments.attachment-type-definition` |

#### NeoForge networking

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.neoforged.neoforge.network.handling.IPayloadContext` | 183 | same | `ironpumpkin_mods::network::PayloadContext` |
| `net.neoforged.neoforge.network.registration.PayloadRegistrar` | 167 | same | `ironpumpkin_mods::network::PayloadRegistrar` |
| `net.neoforged.neoforge.network.handling.IPayloadHandler` | 166 | same | `ironpumpkin_mods::network::PayloadHandler` |
| `net.neoforged.neoforge.network.PacketDistributor` | 157 | same | `network.send-to-*` |
| `net.neoforged.neoforge.network.IContainerFactory` | 84 | same | the extra data of `menus::open_menu`, M5 (`neoforge:advanced_open_screen`) |
| `net.neoforged.neoforge.network.configuration.ICustomConfigurationTask` | 5 | same | `ironpumpkin_mods::network::CustomConfigurationTask` |

#### NeoForge fluids

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.neoforged.neoforge.fluids.FluidType` | 75 | same | `registration.fluid-type-definition` (no phase yet) |
| `net.neoforged.neoforge.fluids.FluidType$Properties` | 32 | same | `registration.fluid-type-definition` (no phase yet) |
| `net.neoforged.neoforge.fluids.BaseFlowingFluid` | 30 | same | `registration.fluid-definition` (no phase yet) |
| `net.neoforged.neoforge.fluids.BaseFlowingFluid$Properties` | 26 | same | `registration.fluid-definition` (no phase yet) |
| `net.neoforged.neoforge.fluids.BaseFlowingFluid$Flowing` | 23 | same | `registration.fluid-definition.is-source` false (no phase yet) |
| `net.neoforged.neoforge.fluids.BaseFlowingFluid$Source` | 21 | same | `registration.fluid-definition.is-source` true (no phase yet) |

#### NeoForge common utilities

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.neoforged.neoforge.common.Tags` | 77 | same | `registry::tag_entries`, `registry::is_in_tag` |
| `net.neoforged.neoforge.common.extensions.IMenuTypeExtension` | 70 | same | `registration.menu-type-definition.extra-data` and the extra data of `menus::open_menu`, both M5 (`neoforge:advanced_open_screen`) |
| `net.neoforged.neoforge.common.util.FakePlayer` | 63 | same | gap: not supported (Pumpkin has no fake players); documented in `types` |
| `net.neoforged.neoforge.common.Tags$Items` | 51 | same | `registry::tag_entries`, `registry::is_in_tag` |
| `net.neoforged.neoforge.common.ItemAbility` | 45 | same | gap: not supported; `item_ability` name on `BlockEventBlockToolModificationEvent` |
| `net.neoforged.neoforge.common.ItemAbilities` | 41 | same | gap: not supported (item abilities); documented in `types` |
| `net.neoforged.neoforge.common.Tags$Blocks` | 30 | same | `registry::tag_entries`, `registry::is_in_tag` |
| `net.neoforged.neoforge.common.SoundAction` | 23 | same | `registration.fluid-sound.action` |
| `net.neoforged.neoforge.common.SoundActions` | 20 | same | `registration.fluid-sound.action` |
| `net.neoforged.neoforge.common.DeferredSpawnEggItem` | 19 | CHANGED | `registration.item-properties.spawn-egg` (class removed in 26.3) |
| `net.neoforged.neoforge.common.damagesource.DamageContainer` | 17 | same | `types.damage-container` |
| `net.neoforged.neoforge.common.SimpleTier` | 5 | CHANGED | `registration.item-properties.component` (`minecraft:tool`; class removed in 26.3) |
| `net.neoforged.neoforge.common.damagesource.DamageContainer$Reduction` | 2 | same | `types.damage-reduction` |
| `net.neoforged.neoforge.common.damagesource.IReductionFunction` | 1 | same | `event-bus.reduction-modifier`, `callbacks.apply-reduction-modifier` |

#### NeoForge loot and world modifiers

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.neoforged.neoforge.common.loot.IGlobalLootModifier` | 29 | same | `ironpumpkin_mods::loot::GlobalLootModifier`, `ModInit::register_loot_modifier_type` |
| `net.neoforged.neoforge.common.loot.LootModifier` | 25 | same | `condition` and `priority` read by the host (`ironpumpkin_mods::loot`) |
| `net.neoforged.neoforge.common.world.BiomeModifier` | 14 | same | gap: not supported (world generation); documented in `loot` |
| `net.neoforged.neoforge.common.world.BiomeModifiers` | 1 | same | gap: not supported; documented in `loot` |
| `net.neoforged.neoforge.common.world.BiomeModifiers$AddFeaturesBiomeModifier` | 0 | same | gap: not supported; documented in `loot` |
| `net.neoforged.neoforge.common.world.BiomeModifiers$RemoveFeaturesBiomeModifier` | 0 | same | gap: not supported; documented in `loot` |

#### Minecraft core, registries and tags

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.minecraft.resources.ResourceLocation` | 367 | CHANGED | `types.identifier` (`Identifier` in 26.3) |
| `net.minecraft.resources.ResourceKey` | 306 | same | `types.resource-key` |
| `net.minecraft.core.BlockPos` | 296 | same | `pumpkin:plugin/common` (`block-pos`) |
| `net.minecraft.core.registries.Registries` | 265 | same | `ironpumpkin_mods::registry` (`contains`, `id_of`, `key_of`, `keys`, `tag_entries`) |
| `net.minecraft.core.Holder` | 258 | same | `types.identifier`; `registration.deferred-holder` |
| `net.minecraft.core.registries.BuiltInRegistries` | 257 | same | `ironpumpkin_mods::registry` (`contains`, `id_of`, `key_of`, `keys`, `tag_entries`) |
| `net.minecraft.core.Direction` | 249 | same | `types.direction` |
| `net.minecraft.core.HolderLookup` | 245 | same | `ironpumpkin_mods::registry` (`contains`, `id_of`, `key_of`, `keys`, `tag_entries`) |
| `net.minecraft.core.HolderLookup$Provider` | 239 | same | `ironpumpkin_mods::registry` (`contains`, `id_of`, `key_of`, `keys`, `tag_entries`) |
| `net.minecraft.core.DefaultedRegistry` | 239 | same | `ironpumpkin_mods::registry` (`contains`, `id_of`, `key_of`, `keys`, `tag_entries`) |
| `net.minecraft.core.Registry` | 239 | same | `ironpumpkin_mods::registry` (`contains`, `id_of`, `key_of`, `keys`, `tag_entries`) |
| `net.minecraft.tags.TagKey` | 232 | same | the tag name, for `registry::is_in_tag` and `registry::tag_entries` |
| `net.minecraft.core.RegistryAccess` | 198 | same | `ironpumpkin_mods::registry` (`contains`, `id_of`, `key_of`, `keys`, `tag_entries`) |
| `net.minecraft.core.Holder$Reference` | 177 | same | no handle type: the name and `registry::id_of`, `key_of`, `is_in_tag` |
| `net.minecraft.core.NonNullList` | 172 | same | a list of `item-stack` (`list<item-stack>`) |
| `net.minecraft.core.particles.ParticleOptions` | 133 | same | `pumpkin:plugin/particles` |
| `net.minecraft.core.particles.SimpleParticleType` | 114 | same | `pumpkin:plugin/particles` |
| `net.minecraft.core.particles.ParticleTypes` | 113 | same | `pumpkin:plugin/particles` |
| `net.minecraft.tags.BlockTags` | 96 | same | `registry::is_in_tag` (tag ids) |
| `net.minecraft.core.HolderSet` | 94 | same | `ironpumpkin_mods::registry` (`contains`, `id_of`, `key_of`, `keys`, `tag_entries`) |
| `net.minecraft.core.HolderLookup$RegistryLookup` | 81 | same | `ironpumpkin_mods::registry` (`contains`, `id_of`, `key_of`, `keys`, `tag_entries`) |
| `net.minecraft.core.Position` | 75 | same | `pumpkin:plugin/common` (`position`) |
| `net.minecraft.tags.ItemTags` | 74 | same | `registry::is_in_tag` (tag ids) |
| `net.minecraft.core.BlockPos$MutableBlockPos` | 67 | same | `pumpkin:plugin/common` (`block-pos`) |
| `net.minecraft.core.HolderSet$Named` | 63 | same | `ironpumpkin_mods::registry` (`contains`, `id_of`, `key_of`, `keys`, `tag_entries`) |
| `net.minecraft.core.HolderGetter` | 46 | same | `ironpumpkin_mods::registry` (`contains`, `id_of`, `key_of`, `keys`, `tag_entries`) |
| `net.minecraft.tags.DamageTypeTags` | 34 | same | `registry::is_in_tag` (tag ids) |
| `net.minecraft.core.HolderSet$Direct` | 32 | same | `ironpumpkin_mods::registry` (`contains`, `id_of`, `key_of`, `keys`, `tag_entries`) |
| `net.minecraft.tags.BiomeTags` | 9 | same | `registry::is_in_tag` (tag ids) |

#### Minecraft data components

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.minecraft.core.component.DataComponentType` | 232 | same | `data_components::DataComponentType<T>`; registered with `DataComponentTypeBuilder` |
| `net.minecraft.core.component.DataComponents` | 153 | same | `pumpkin_data::data_component::DataComponent` and the typed components of `ItemStack`; mod types through `data_components` |
| `net.minecraft.core.component.DataComponentType$Builder` | 148 | same | `data_components::DataComponentTypeBuilder` (`persistent`; the stream codec is the NBT form of the codec) |
| `net.minecraft.core.component.DataComponentMap` | 103 | same | the components of `ItemStack` (`patch` for vanilla types, `custom_components` for mod types) |
| `net.minecraft.core.component.TypedDataComponent` | 27 | same | `types.component-value`; `pumpkin:plugin/item-stack` components |

#### Minecraft utilities

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.minecraft.ChatFormatting` | 248 | same | gap: Java helper; the style of the v0.2 `text-component` covers it (documented in `types`) |
| `net.minecraft.util.RandomSource` | 200 | same | gap: Java helper; Rust std covers it (documented in `types`) |
| `net.minecraft.util.Mth` | 191 | same | gap: Java helper; Rust std covers it (documented in `types`) |
| `net.minecraft.util.StringRepresentable` | 127 | same | gap: Java helper; Rust std covers it (documented in `types`) |
| `net.minecraft.util.StringRepresentable$EnumCodec` | 83 | same | gap: Java helper; Rust std covers it (documented in `types`) |
| `net.minecraft.util.valueproviders.UniformInt` | 16 | same | `registration.block-definition.type-data` (JSON) |
| `net.minecraft.util.valueproviders.IntProvider` | 15 | same | `registration.block-definition.type-data` (JSON) |
| `net.minecraft.util.valueproviders.ConstantInt` | 6 | same | `registration.block-definition.type-data` (JSON) |
| `net.minecraft.util.random.SimpleWeightedRandomList` | 6 | CHANGED | gap: world generation, not supported |
| `net.minecraft.util.random.SimpleWeightedRandomList$Builder` | 3 | CHANGED | gap: world generation, not supported |

#### Minecraft sounds

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.minecraft.sounds.SoundEvent` | 190 | same | `sound_events::SoundEvent`, registered with `SoundEventBuilder`; played with `sound_events::play_sound` |
| `net.minecraft.sounds.SoundSource` | 165 | same | `types.sound-source` |
| `net.minecraft.sounds.SoundEvents` | 162 | same | `types.identifier` (sound ids); `pumpkin:plugin/sounds` |

#### Minecraft items

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.minecraft.world.item.ItemStack` | 327 | same | `pumpkin:plugin/item-stack`; `registration.item-definition` |
| `net.minecraft.world.item.Item` | 312 | same | `pumpkin:plugin/item-stack`; `registration.item-definition` |
| `net.minecraft.world.item.Item$Properties` | 217 | same | `registration.item-properties` |
| `net.minecraft.world.item.CreativeModeTab` | 183 | same | `ModInit::register_creative_mode_tab` (ignored on the server) |
| `net.minecraft.world.item.Items` | 180 | same | `pumpkin:plugin/item-stack`; `registration.item-definition` |
| `net.minecraft.world.item.BlockItem` | 179 | same | `registration.item-definition.block` |
| `net.minecraft.world.item.CreativeModeTab$Builder` | 142 | same | `ModInit::register_creative_mode_tab` (takes the name; the builder is client UI) |
| `net.minecraft.world.item.CreativeModeTab$Output` | 131 | same | `ModInit::register_creative_mode_tab` (the tab contents are client UI) |
| `net.minecraft.world.item.CreativeModeTab$ItemDisplayParameters` | 127 | same | `ModInit::register_creative_mode_tab` (the tab contents are client UI) |
| `net.minecraft.world.item.CreativeModeTab$DisplayItemsGenerator` | 121 | same | `ModInit::register_creative_mode_tab` (the tab contents are client UI) |
| `net.minecraft.world.item.DyeColor` | 82 | same | `pumpkin:plugin/world` (`dye-color`) |
| `net.minecraft.world.item.CreativeModeTabs` | 73 | same | `types.identifier` (tab ids) |
| `net.minecraft.world.item.Rarity` | 70 | same | `registration.rarity` |
| `net.minecraft.world.item.component.ItemAttributeModifiers` | 57 | same | `event-bus.item-attribute-modifier`; `pumpkin:plugin/item-stack` |
| `net.minecraft.world.item.ArmorItem` | 46 | CHANGED | `registration.item-properties.component` (`minecraft:equippable`; class removed in 26.3) |
| `net.minecraft.world.food.FoodProperties` | 45 | same | `registration.item-properties.component` (`minecraft:food`) |
| `net.minecraft.world.item.ItemCooldowns` | 44 | same | `pumpkin:plugin/player` (`start-cooldown`) |
| `net.minecraft.world.item.Tier` | 44 | CHANGED | `registration.item-properties.component` (`ToolMaterial` in 26.3) |
| `net.minecraft.world.item.BucketItem` | 38 | same | `registration.item-definition.behaviour` (`minecraft:bucket`) |
| `net.minecraft.world.item.ArmorMaterial` | 37 | CHANGED | `registration.item-properties.component` (26.3 equipment asset; no registry) |
| `net.minecraft.world.item.alchemy.PotionContents` | 36 | same | `types.component-value` (`minecraft:potion_contents`); `pumpkin:plugin/potions` |
| `net.minecraft.world.item.SwordItem` | 35 | CHANGED | `registration.item-properties.component` (`minecraft:weapon`, `minecraft:tool`) |
| `net.minecraft.world.item.UseAnim` | 34 | CHANGED | `registration.item-properties.component` (`minecraft:consumable`) |
| `net.minecraft.world.item.alchemy.Potion` | 34 | same | `pumpkin:plugin/potions` |
| `net.minecraft.world.item.ArmorItem$Type` | 31 | CHANGED | `registration.item-properties.component` (class removed in 26.3) |
| `net.minecraft.world.item.AxeItem` | 27 | CHANGED | `registration.item-definition.behaviour` and `minecraft:tool` (`item-properties.component`) |
| `net.minecraft.world.item.alchemy.Potions` | 27 | same | `pumpkin:plugin/potions` |
| `net.minecraft.world.item.CreativeModeTab$TabVisibility` | 26 | same | gap: client UI (`BuildCreativeModeTabContentsEvent` not supported) |
| `net.minecraft.world.item.ArmorMaterial$Layer` | 24 | CHANGED | gap: client equipment asset |
| `net.minecraft.world.food.FoodProperties$Builder` | 21 | same | `registration.item-properties.component` (`minecraft:food`) |
| `net.minecraft.world.item.PickaxeItem` | 21 | CHANGED | `registration.item-properties.component` (`minecraft:tool`) |
| `net.minecraft.world.item.ShovelItem` | 21 | CHANGED | `registration.item-definition.behaviour` and `minecraft:tool` (`item-properties.component`) |
| `net.minecraft.world.item.HoeItem` | 17 | CHANGED | `registration.item-definition.behaviour` and `minecraft:tool` (`item-properties.component`) |
| `net.minecraft.world.item.ItemNameBlockItem` | 10 | CHANGED | `registration.item-definition.block` (class removed in 26.3) |
| `net.minecraft.world.item.PotionItem` | 10 | same | `pumpkin:plugin/item-stack`; `registration.item-definition` |
| `net.minecraft.world.item.component.SeededContainerLoot` | 4 | same | `pumpkin:plugin/item-stack`; `registration.item-definition` |

#### Minecraft enchantments

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.minecraft.world.item.enchantment.Enchantment` | 68 | same | `pumpkin:plugin/enchantments`; `types.enchantment-instance` |
| `net.minecraft.world.item.enchantment.EnchantmentHelper` | 65 | same | `pumpkin:plugin/enchantments`; `types.enchantment-instance` |
| `net.minecraft.world.item.enchantment.Enchantments` | 53 | same | `pumpkin:plugin/enchantments`; `types.enchantment-instance` |
| `net.minecraft.world.item.enchantment.ItemEnchantments` | 50 | same | `pumpkin:plugin/enchantments`; `types.enchantment-instance` |
| `net.minecraft.world.item.enchantment.ItemEnchantments$Mutable` | 32 | same | `pumpkin:plugin/enchantments`; `types.enchantment-instance` |
| `net.minecraft.world.item.enchantment.EnchantmentInstance` | 15 | same | `pumpkin:plugin/enchantments`; `types.enchantment-instance` |

#### Minecraft recipes and trading

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.minecraft.world.item.crafting.Ingredient` | 141 | same | `pumpkin:plugin/recipe` `ingredient`; tests with the item id and `registry::is_in_tag` (documented in `types`) |
| `net.minecraft.world.item.trading.MerchantOffer` | 19 | same | `event-bus.merchant-offer` |
| `net.minecraft.world.item.trading.ItemCost` | 17 | same | gap: 26.3 trades are `minecraft:villager_trade` datapack entries; no code API |
| `net.minecraft.world.item.trading.MerchantOffers` | 10 | same | `event-bus.merchant-offer`; trades are datapack data in 26.1 |
| `net.minecraft.world.item.trading.Merchant` | 6 | same | gap: custom `Merchant` implementations; `trade-with-villager-event` is the trade hook |

#### Minecraft blocks and block entities

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.minecraft.world.level.block.state.BlockState` | 280 | same | `types.block-state` |
| `net.minecraft.world.level.block.Block` | 275 | same | `registration.block-definition`; `pumpkin:plugin/world` |
| `net.minecraft.world.level.block.entity.BlockEntity` | 235 | same | `pumpkin:plugin/block-entity` |
| `net.minecraft.world.level.block.state.properties.Property` | 197 | same | `types.block-property` |
| `net.minecraft.world.level.block.entity.BlockEntityType` | 191 | same | `registration.block-entity-type-definition` (M5) |
| `net.minecraft.world.level.block.state.BlockBehaviour` | 186 | same | `registration.block-definition`; `pumpkin:plugin/world` |
| `net.minecraft.world.level.block.Blocks` | 183 | same | `types.identifier` (block ids) |
| `net.minecraft.world.level.block.state.BlockBehaviour$Properties` | 178 | same | `registration.block-properties` |
| `net.minecraft.world.level.block.entity.BlockEntityType$BlockEntitySupplier` | 144 | same | `registration.block-entity-type-definition` |
| `net.minecraft.world.level.block.SoundType` | 127 | same | `registration.sound-type` |
| `net.minecraft.world.level.block.entity.BlockEntityTicker` | 114 | same | `callbacks.tick-block-entity` |
| `net.minecraft.world.level.block.entity.BlockEntityType$Builder` | 112 | CHANGED | `registration.block-entity-type-definition` |
| `net.minecraft.world.level.block.EntityBlock` | 102 | same | `registration.block-entity-type-definition.valid-blocks` |
| `net.minecraft.world.level.block.state.properties.IntegerProperty` | 90 | same | `types.block-property` |
| `net.minecraft.world.level.block.RenderShape` | 81 | same | gap: client rendering |
| `net.minecraft.world.level.block.LiquidBlock` | 49 | same | `registration.block-definition`; `pumpkin:plugin/world` |
| `net.minecraft.world.level.block.SlabBlock` | 29 | same | `registration.block-definition`; `pumpkin:plugin/world` |
| `net.minecraft.world.level.block.StairBlock` | 27 | same | `registration.block-definition`; `pumpkin:plugin/world` |
| `net.minecraft.world.level.block.CropBlock` | 26 | same | `registration.block-definition`; `pumpkin:plugin/world` |
| `net.minecraft.world.level.block.BaseFireBlock` | 24 | same | `registration.block-definition`; `pumpkin:plugin/world` |
| `net.minecraft.world.level.block.RotatedPillarBlock` | 23 | same | `registration.block-definition`; `pumpkin:plugin/world` |
| `net.minecraft.world.level.block.DoorBlock` | 17 | same | `registration.block-definition`; `pumpkin:plugin/world` |
| `net.minecraft.world.level.block.entity.SpawnerBlockEntity` | 14 | same | `pumpkin:plugin/block-entity` (`mob-spawner-block-entity`) |
| `net.minecraft.world.level.block.FireBlock` | 13 | same | gap: flammability registration (`FireBlock#setFlammable`) has no API; documented in `registration` |
| `net.minecraft.world.level.block.entity.RandomizableContainerBlockEntity` | 13 | same | `pumpkin:plugin/block-entity` (`container-block-entity`) |
| `net.minecraft.world.level.block.SnowLayerBlock` | 8 | same | `registration.block-definition`; `pumpkin:plugin/world` |
| `net.minecraft.world.level.block.DropExperienceBlock` | 4 | same | `registration.block-definition`; `pumpkin:plugin/world` |

#### Minecraft entities

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.minecraft.world.entity.player.Player` | 302 | same | `pumpkin:plugin/player` |
| `net.minecraft.world.entity.Entity` | 249 | same | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.LivingEntity` | 219 | same | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.player.Inventory` | 207 | same | `pumpkin:plugin/inventory` (`player-inventory`) |
| `net.minecraft.world.entity.EntityType` | 174 | same | `registration.entity-type-builder`; `pumpkin:plugin/entity-types` |
| `net.minecraft.world.entity.item.ItemEntity` | 160 | same | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.EquipmentSlot` | 113 | same | `pumpkin:plugin/world` (`equipment-slot`) |
| `net.minecraft.world.entity.Mob` | 82 | same | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.player.Abilities` | 81 | same | `pumpkin:plugin/player` (`player-abilities`) |
| `net.minecraft.world.entity.MobCategory` | 80 | same | `types.mob-category` |
| `net.minecraft.world.entity.EntityType$EntityFactory` | 65 | same | `registration.entity-type-builder.of` |
| `net.minecraft.world.entity.EntityType$Builder` | 64 | same | `registration.entity-type-builder` |
| `net.minecraft.world.entity.MobSpawnType` | 46 | CHANGED | `types.entity-spawn-reason` (`EntitySpawnReason` in 26.3) |
| `net.minecraft.world.entity.projectile.Projectile` | 44 | same | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.PathfinderMob` | 37 | same | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.EntityDimensions` | 37 | same | `registration.entity-type-builder.sized` |
| `net.minecraft.world.entity.ai.goal.GoalSelector` | 35 | same | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.ai.goal.Goal` | 34 | same | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.projectile.AbstractArrow` | 27 | CHANGED | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.SpawnGroupData` | 22 | same | gap: Java object; not exposed on `finalize-spawn-event` |
| `net.minecraft.world.entity.ai.goal.target.NearestAttackableTargetGoal` | 19 | same | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.animal.horse.AbstractHorse` | 18 | CHANGED | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.monster.Creeper` | 18 | same | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.ai.goal.LookAtPlayerGoal` | 17 | same | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.animal.Wolf` | 17 | CHANGED | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.ai.goal.target.HurtByTargetGoal` | 16 | same | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.ai.goal.FloatGoal` | 16 | same | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.ai.goal.RandomLookAroundGoal` | 16 | same | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.monster.Zombie` | 14 | CHANGED | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.ai.goal.WrappedGoal` | 13 | same | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.projectile.Arrow` | 11 | CHANGED | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.ai.goal.AvoidEntityGoal` | 9 | same | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.monster.Skeleton` | 9 | CHANGED | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.monster.Spider` | 8 | CHANGED | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.animal.IronGolem` | 8 | CHANGED | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.AreaEffectCloud` | 7 | same | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.monster.Blaze` | 7 | same | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.monster.AbstractSkeleton` | 6 | CHANGED | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.monster.ZombifiedPiglin` | 3 | CHANGED | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.ai.goal.MoveToBlockGoal` | 3 | same | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.animal.Rabbit` | 3 | CHANGED | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.monster.CaveSpider` | 2 | CHANGED | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.ai.goal.FleeSunGoal` | 2 | same | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |
| `net.minecraft.world.entity.ai.goal.RestrictSunGoal` | 2 | same | `pumpkin:plugin/world` (`entity`, `living-entity`, `mob`) |

#### Minecraft attributes

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.minecraft.world.entity.ai.attributes.Attributes` | 79 | same | `entity_attributes::Attributes` (the generated constants), `entity_attributes::attribute` by name |
| `net.minecraft.world.entity.ai.attributes.AttributeModifier` | 73 | same | `entity_attributes::AttributeModifier` |
| `net.minecraft.world.entity.ai.attributes.AttributeModifier$Operation` | 65 | same | `types.attribute-operation` |
| `net.minecraft.world.entity.ai.attributes.AttributeInstance` | 62 | same | `entity_attributes::base_value`, `set_base_value`, `modifiers`, `add_modifier`, `remove_modifier`, `value` |
| `net.minecraft.world.entity.ai.attributes.Attribute` | 56 | same | `entity_attributes::Attributes`, `entity_attributes::attribute` by name; registered with `entity_attributes::AttributeBuilder` |
| `net.minecraft.world.entity.ai.attributes.AttributeSupplier` | 38 | same | `registration.entity-attribute-creation-event` |
| `net.minecraft.world.entity.ai.attributes.AttributeSupplier$Builder` | 38 | same | `types.attribute-value` |
| `net.minecraft.world.entity.ai.attributes.AttributeMap` | 31 | same | `pumpkin:plugin/attributes`; `types.attribute-modifier`, `attribute-value` |
| `net.minecraft.world.entity.ai.attributes.RangedAttribute` | 23 | same | `entity_attributes::AttributeBuilder::ranged` |

#### Minecraft effects and damage

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.minecraft.world.damagesource.DamageSource` | 113 | same | `types.damage-source` |
| `net.minecraft.world.effect.MobEffectInstance` | 94 | same | `mob_effects::Effect` (`mob_effects::instance`); applied with `mob_effects::add_effect` |
| `net.minecraft.world.effect.MobEffect` | 72 | same | `mob_effects::MobEffectBuilder` and the `mob_effects::MobEffect` trait (`apply_effect_tick`) |
| `net.minecraft.world.effect.MobEffects` | 67 | same | `pumpkin:plugin/status-effect`; `types.mob-effect-instance`, `damage-source` |
| `net.minecraft.world.damagesource.DamageSources` | 62 | same | `pumpkin:plugin/world` (`living-entity.damage`) |
| `net.minecraft.world.damagesource.DamageType` | 43 | same | `pumpkin:plugin/damage-types`; custom types are datapack JSON |
| `net.minecraft.world.effect.MobEffectCategory` | 39 | same | `registration.mob-effect-category` |
| `net.minecraft.world.damagesource.DamageTypes` | 33 | same | `pumpkin:plugin/damage-types` |
| `net.minecraft.world.damagesource.DamageScaling` | 0 | same | gap: damage type JSON field (datapack) |

#### Minecraft menus and containers

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.minecraft.world.inventory.AbstractContainerMenu` | 172 | same | `menus::MenuDefinition`, the `menus::MenuLogic` trait (the overridable methods), `menus::MenuAccess` (the protected methods), `menus::Menu`; mod menu types wait for M5 (`neoforge:advanced_open_screen`) |
| `net.minecraft.world.inventory.MenuType` | 157 | same | vanilla types: the `WindowType` of `menus::MenuDefinition::new`, checked against `menus::layout::vanilla_layout`; mod types `registration.menu-type-definition` (M5) |
| `net.minecraft.world.inventory.Slot` | 139 | same | `menus::SlotDefinition` (`with_place_filter`, `with_take_callback`); `pumpkin:plugin/inventory` for vanilla screens |
| `net.minecraft.world.Container` | 135 | same | `pumpkin:plugin/inventory`; menu slots through `menus::SlotSource::Container` |
| `net.minecraft.world.MenuProvider` | 121 | same | `menus::MenuDefinition` (with the title) and `menus::open_menu` |
| `net.minecraft.world.inventory.InventoryMenu` | 68 | same | `pumpkin:plugin/inventory`, `pumpkin:plugin/gui` (vanilla screens); player inventory slots of a mod menu `menus::MenuDefinition::player_inventory` |
| `net.neoforged.neoforge.transfer.item.ResourceHandlerSlot` | 55 (21.1 `SlotItemHandler`) | new in 26.3 | `menus::SlotSource::Handler`: a take extracts and a place inserts in a root transaction, with no `IndexModifier` |
| `net.minecraft.world.SimpleContainer` | 52 | same | `menus::MenuDefinition::with_container` and `menus::SlotSource::MenuContainer` (`menus::MenuContainer`); `pumpkin:plugin/inventory` |
| `net.minecraft.world.inventory.ContainerLevelAccess` | 52 | same | `pumpkin:plugin/inventory`, `pumpkin:plugin/gui` (vanilla screens); `stillValid(access, player, block)` is `menus::still_valid_at` |
| `net.minecraft.world.inventory.MenuConstructor` | 51 | same | `menus::MenuDefinition` and the `menus::MenuLogic` of `menus::open_menu` |
| `net.minecraft.world.SimpleMenuProvider` | 45 | same | `menus::MenuDefinition` and `menus::open_menu` |
| `net.minecraft.world.inventory.EnchantmentMenu` | 2 | same | `pumpkin:plugin/inventory`, `pumpkin:plugin/gui` (vanilla screens); a mod menu of the `enchantment` type with `menus::MenuDefinition` |

#### Minecraft level and world

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.minecraft.world.level.Level` | 310 | same | `pumpkin:plugin/world` |
| `net.minecraft.world.level.ItemLike` | 262 | same | `types.identifier` (item ids) |
| `net.minecraft.world.phys.Vec3` | 219 | same | `pumpkin:plugin/common` (`position`) |
| `net.minecraft.world.InteractionHand` | 214 | same | `types.interaction-hand` |
| `net.minecraft.world.level.BlockGetter` | 198 | same | `pumpkin:plugin/world` |
| `net.minecraft.world.InteractionResult` | 186 | same | `types.interaction-result` |
| `net.minecraft.world.phys.shapes.VoxelShape` | 167 | same | `ironpumpkin_mods::shapes::VoxelShape` (task 2.10, #155); mod block shapes through `BlockBuilder::shape` and `collision_shape` |
| `net.minecraft.world.level.LevelAccessor` | 162 | same | `pumpkin:plugin/world` |
| `net.minecraft.world.phys.AABB` | 161 | same | `shapes.aabb`; `pumpkin:plugin/world` (`bounding-box`) |
| `net.minecraft.world.phys.shapes.CollisionContext` | 134 | same | the `entity` argument of `ironpumpkin_mods::shapes::get_block_shape` (task 2.10, #155); a mod block shape does not read it |
| `net.minecraft.world.InteractionResultHolder` | 131 | CHANGED | `types.interaction-result` (merged in 26.3) |
| `net.minecraft.world.phys.HitResult` | 127 | same | `types.hit-result` |
| `net.minecraft.world.phys.shapes.Shapes` | 119 | same | the methods of `ironpumpkin_mods::shapes::VoxelShape` (`empty`, `block`, `cuboid`, `or`, `join` with `BooleanOp`, `rotate_horizontal`) (task 2.10, #155) |
| `net.minecraft.world.level.ChunkPos` | 111 | same | `types.chunk-pos` |
| `net.minecraft.world.level.chunk.LevelChunk` | 70 | same | `pumpkin:plugin/world` (`chunk`) |
| `net.minecraft.world.phys.HitResult$Type` | 70 | same | `types.hit-result` |
| `net.minecraft.world.level.Explosion` | 64 | same | `event-bus.explosion`; `pumpkin:plugin/world` (`create-explosion`) |
| `net.minecraft.world.phys.EntityHitResult` | 58 | same | `types.entity-hit-result` |
| `net.minecraft.world.level.ServerLevelAccessor` | 54 | same | `pumpkin:plugin/world` |
| `net.minecraft.world.level.GameRules` | 53 | CHANGED | `pumpkin:plugin/game-rules` (read); custom game rules: gap (`game_rule` registry not supported) |
| `net.minecraft.world.level.GameRules$Key` | 52 | CHANGED | `pumpkin:plugin/game-rules`; custom rules: gap |
| `net.minecraft.world.level.WorldGenLevel` | 30 | same | `pumpkin:plugin/world` |
| `net.minecraft.world.DifficultyInstance` | 26 | same | `event-bus.difficulty-instance` |
| `net.minecraft.world.level.Level$ExplosionInteraction` | 24 | same | `pumpkin:plugin/world` (`explosion-interaction`) |
| `net.minecraft.world.level.GameRules$BooleanValue` | 16 | CHANGED | `pumpkin:plugin/game-rules` (`game-rule-value`) |
| `net.minecraft.world.level.BaseSpawner` | 15 | same | `pumpkin:plugin/block-entity` (`mob-spawner-block-entity`) |
| `net.minecraft.world.level.GameRules$Type` | 10 | CHANGED | gap: custom game rules not supported |
| `net.minecraft.world.level.SpawnData` | 8 | same | `pumpkin:plugin/block-entity` (`mob-spawner-block-entity`) |
| `net.minecraft.world.level.GameRules$Category` | 6 | CHANGED | gap: custom game rules not supported |

#### Minecraft fluids and materials

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.minecraft.world.level.material.Fluid` | 155 | same | `registration.fluid-definition` (no phase yet); `types.fluid-state` |
| `net.minecraft.world.level.material.MapColor` | 107 | same | `registration.block-properties.map-color` |
| `net.minecraft.world.level.material.FlowingFluid` | 103 | same | `registration.fluid-definition` (no phase yet) |
| `net.minecraft.world.level.material.PushReaction` | 55 | same | `registration.push-reaction` |

#### Minecraft world generation

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.minecraft.world.level.levelgen.Heightmap` | 58 | same | `pumpkin:plugin/world` `get-top-block-y` and `get-motion-blocking-height` (reads); world generation use: gap |
| `net.minecraft.world.level.levelgen.Heightmap$Types` | 58 | same | `pumpkin:plugin/world` `get-top-block-y` and `get-motion-blocking-height` (reads); world generation use: gap |
| `net.minecraft.world.level.levelgen.feature.Feature` | 24 | same | gap: not supported (Pumpkin world generation is Rust code; `handle-generate-phase`); documented in `loot` |
| `net.minecraft.world.level.levelgen.GenerationStep` | 21 | same | gap: not supported (Pumpkin world generation is Rust code; `handle-generate-phase`); documented in `loot` |
| `net.minecraft.world.level.levelgen.placement.PlacedFeature` | 20 | same | gap: not supported (Pumpkin world generation is Rust code; `handle-generate-phase`); documented in `loot` |
| `net.minecraft.world.level.levelgen.feature.configurations.FeatureConfiguration` | 18 | CHANGED | gap: not supported (Pumpkin world generation is Rust code; `handle-generate-phase`); documented in `loot` |
| `net.minecraft.world.level.levelgen.feature.FeaturePlaceContext` | 18 | CHANGED | gap: not supported (Pumpkin world generation is Rust code; `handle-generate-phase`); documented in `loot` |
| `net.minecraft.world.level.levelgen.GenerationStep$Decoration` | 16 | same | gap: not supported (Pumpkin world generation is Rust code; `handle-generate-phase`); documented in `loot` |
| `net.minecraft.world.level.levelgen.placement.PlacementModifierType` | 14 | CHANGED | gap: not supported (Pumpkin world generation is Rust code; `handle-generate-phase`); documented in `loot` |
| `net.minecraft.world.level.levelgen.feature.ConfiguredFeature` | 13 | CHANGED | gap: not supported (Pumpkin world generation is Rust code; `handle-generate-phase`); documented in `loot` |
| `net.minecraft.world.level.levelgen.structure.templatesystem.RuleTest` | 10 | same | gap: not supported (Pumpkin world generation is Rust code; `handle-generate-phase`); documented in `loot` |
| `net.minecraft.world.level.levelgen.placement.PlacementContext` | 10 | same | gap: not supported (Pumpkin world generation is Rust code; `handle-generate-phase`); documented in `loot` |
| `net.minecraft.world.level.levelgen.feature.configurations.OreConfiguration` | 8 | CHANGED | gap: not supported (Pumpkin world generation is Rust code; `handle-generate-phase`); documented in `loot` |
| `net.minecraft.world.level.levelgen.placement.PlacementFilter` | 8 | same | gap: not supported (Pumpkin world generation is Rust code; `handle-generate-phase`); documented in `loot` |
| `net.minecraft.world.level.levelgen.placement.PlacementModifier` | 7 | same | gap: not supported (Pumpkin world generation is Rust code; `handle-generate-phase`); documented in `loot` |
| `net.minecraft.world.level.levelgen.VerticalAnchor` | 6 | same | gap: not supported (Pumpkin world generation is Rust code; `handle-generate-phase`); documented in `loot` |
| `net.minecraft.world.level.levelgen.feature.configurations.OreConfiguration$TargetBlockState` | 6 | CHANGED | gap: not supported (Pumpkin world generation is Rust code; `handle-generate-phase`); documented in `loot` |
| `net.minecraft.world.level.levelgen.feature.stateproviders.BlockStateProvider` | 5 | same | gap: not supported (Pumpkin world generation is Rust code; `handle-generate-phase`); documented in `loot` |
| `net.minecraft.world.level.levelgen.placement.BiomeFilter` | 3 | same | gap: not supported (Pumpkin world generation is Rust code; `handle-generate-phase`); documented in `loot` |
| `net.minecraft.world.level.levelgen.placement.CountPlacement` | 3 | same | gap: not supported (Pumpkin world generation is Rust code; `handle-generate-phase`); documented in `loot` |
| `net.minecraft.world.level.levelgen.placement.InSquarePlacement` | 3 | same | gap: not supported (Pumpkin world generation is Rust code; `handle-generate-phase`); documented in `loot` |
| `net.minecraft.world.level.levelgen.structure.templatesystem.TagMatchTest` | 3 | same | gap: not supported (Pumpkin world generation is Rust code; `handle-generate-phase`); documented in `loot` |
| `net.minecraft.world.level.levelgen.placement.RarityFilter` | 2 | same | gap: not supported (Pumpkin world generation is Rust code; `handle-generate-phase`); documented in `loot` |
| `net.minecraft.world.level.levelgen.structure.templatesystem.BlockMatchTest` | 2 | same | gap: not supported (Pumpkin world generation is Rust code; `handle-generate-phase`); documented in `loot` |
| `net.minecraft.world.level.levelgen.feature.stateproviders.WeightedStateProvider` | 1 | same | gap: not supported (Pumpkin world generation is Rust code; `handle-generate-phase`); documented in `loot` |
| `net.minecraft.world.level.levelgen.placement.HeightRangePlacement` | 1 | same | gap: not supported (Pumpkin world generation is Rust code; `handle-generate-phase`); documented in `loot` |
| `net.minecraft.world.level.levelgen.feature.configurations.SimpleBlockConfiguration` | 0 | CHANGED | gap: not supported (Pumpkin world generation is Rust code; `handle-generate-phase`); documented in `loot` |
| `net.minecraft.world.level.levelgen.placement.HeightmapPlacement` | 0 | same | gap: not supported (Pumpkin world generation is Rust code; `handle-generate-phase`); documented in `loot` |

#### Minecraft loot

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.minecraft.world.level.storage.loot.LootContext` | 59 | same | `loot.loot-context` |
| `net.minecraft.world.level.storage.loot.LootTable` | 58 | same | `loot.roll-loot-table` |
| `net.minecraft.world.level.storage.loot.predicates.LootItemCondition` | 51 | same | `loot` (`conditions` JSON evaluated by the host) |
| `net.minecraft.world.level.storage.loot.BuiltInLootTables` | 22 | same | `types.identifier` (table ids) |

#### Minecraft server and commands

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.minecraft.server.level.ServerPlayer` | 263 | same | `pumpkin:plugin/player` |
| `net.minecraft.server.level.ServerLevel` | 252 | same | `pumpkin:plugin/world` |
| `net.minecraft.commands.CommandSourceStack` | 127 | same | `pumpkin:plugin/command` |
| `net.minecraft.commands.Commands` | 123 | same | `pumpkin:plugin/command` |
| `net.minecraft.stats.Stats` | 71 | same | `pumpkin:plugin/statistics` |
| `net.minecraft.stats.StatType` | 66 | same | `pumpkin:plugin/statistics` |
| `net.minecraft.stats.Stat` | 63 | same | `pumpkin:plugin/statistics`; `event-bus.stat` |
| `net.minecraft.commands.arguments.EntityArgument` | 51 | same | `pumpkin:plugin/command` |
| `net.minecraft.commands.CommandBuildContext` | 47 | same | `pumpkin:plugin/command` |
| `net.minecraft.server.packs.PackType` | 31 | same | `lifecycle.pack-type` |
| `net.minecraft.server.packs.repository.Pack` | 21 | same | `lifecycle.add-pack-finders-event` |
| `net.minecraft.server.packs.repository.PackSource` | 17 | same | `lifecycle.pack-source` |
| `net.minecraft.server.packs.repository.Pack$Position` | 14 | same | `lifecycle.pack-position` |
| `net.minecraft.stats.ServerStatsCounter` | 12 | same | `pumpkin:plugin/statistics` |
| `net.minecraft.commands.arguments.ResourceArgument` | 9 | same | `pumpkin:plugin/command` |
| `net.minecraft.server.network.ConfigurationTask` | 7 | same | `ironpumpkin_mods::network::CustomConfigurationTask` |
| `net.minecraft.server.network.ConfigurationTask$Type` | 7 | same | `ironpumpkin_mods::network::CustomConfigurationTask::task_type` |

#### Minecraft network and chat

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.minecraft.network.chat.Component` | 335 | same | `pumpkin:plugin/text` |
| `net.minecraft.network.chat.MutableComponent` | 325 | same | `pumpkin:plugin/text` |
| `net.minecraft.network.codec.StreamCodec` | 252 | same | gap: the mod encodes payload bytes itself (documented in `network`) |
| `net.minecraft.network.RegistryFriendlyByteBuf` | 226 | same | gap: the mod encodes payload bytes itself |
| `net.minecraft.network.protocol.common.custom.CustomPacketPayload` | 209 | same | `network` (type id and bytes) |
| `net.minecraft.network.codec.ByteBufCodecs` | 204 | same | gap: the mod encodes payload bytes itself |
| `net.minecraft.network.protocol.common.custom.CustomPacketPayload$Type` | 200 | same | `network` (`type: identifier`) |
| `net.minecraft.network.protocol.configuration.ServerConfigurationPacketListener` | 7 | same | `network.configuration-sender` |

#### Minecraft NBT and codecs

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.minecraft.nbt.CompoundTag` | 257 | same | `pumpkin:plugin/common` (`nbt-tree`) |
| `net.minecraft.nbt.Tag` | 210 | same | `pumpkin:plugin/common` (`nbt-tree`) |

#### Minecraft data generation

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.minecraft.data.worldgen.BootstrapContext` | 0 | same | gap: not supported (world generation data; mods ship generated files) |
| `net.minecraft.data.worldgen.placement.OrePlacements` | 0 | same | gap: not supported (world generation data; mods ship generated files) |

#### Mojang Brigadier

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `com.mojang.brigadier.exceptions.CommandSyntaxException` | 124 | same | `pumpkin:plugin/command` (command tree builder) |
| `com.mojang.brigadier.context.CommandContext` | 122 | same | `pumpkin:plugin/command` (command tree builder) |
| `com.mojang.brigadier.Command` | 121 | same | `pumpkin:plugin/command` (command tree builder) |
| `com.mojang.brigadier.CommandDispatcher` | 120 | same | `pumpkin:plugin/command` (command tree builder) |
| `com.mojang.brigadier.builder.ArgumentBuilder` | 120 | same | `pumpkin:plugin/command` (command tree builder) |
| `com.mojang.brigadier.builder.LiteralArgumentBuilder` | 120 | same | `pumpkin:plugin/command` (command tree builder) |
| `com.mojang.brigadier.tree.LiteralCommandNode` | 118 | same | `pumpkin:plugin/command` (command tree builder) |
| `com.mojang.brigadier.arguments.ArgumentType` | 105 | same | `pumpkin:plugin/command` (command tree builder) |
| `com.mojang.brigadier.builder.RequiredArgumentBuilder` | 104 | same | `pumpkin:plugin/command` (command tree builder) |
| `com.mojang.brigadier.arguments.IntegerArgumentType` | 48 | same | `pumpkin:plugin/command` (command tree builder) |

#### Mojang DataFixerUpper codecs

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `com.mojang.serialization.Codec` | 244 | same | gap: no DataFixerUpper codecs in the API (a Java framework); mods serialize with Rust code, and data crosses as JSON, NBT or bytes (documented in `types`) |
| `com.mojang.serialization.MapCodec` | 220 | same | gap: no DataFixerUpper codecs in the API (a Java framework); mods serialize with Rust code, and data crosses as JSON, NBT or bytes (documented in `types`) |
| `com.mojang.serialization.codecs.PrimitiveCodec` | 196 | same | gap: no DataFixerUpper codecs in the API (a Java framework); mods serialize with Rust code, and data crosses as JSON, NBT or bytes (documented in `types`) |
| `com.mojang.datafixers.Products` | 195 | same | gap: no DataFixerUpper codecs in the API (a Java framework); mods serialize with Rust code, and data crosses as JSON, NBT or bytes (documented in `types`) |
| `com.mojang.datafixers.kinds.App` | 195 | same | gap: no DataFixerUpper codecs in the API (a Java framework); mods serialize with Rust code, and data crosses as JSON, NBT or bytes (documented in `types`) |
| `com.mojang.datafixers.kinds.Applicative` | 195 | same | gap: no DataFixerUpper codecs in the API (a Java framework); mods serialize with Rust code, and data crosses as JSON, NBT or bytes (documented in `types`) |
| `com.mojang.serialization.codecs.RecordCodecBuilder` | 191 | same | gap: no DataFixerUpper codecs in the API (a Java framework); mods serialize with Rust code, and data crosses as JSON, NBT or bytes (documented in `types`) |
| `com.mojang.serialization.codecs.RecordCodecBuilder$Instance` | 191 | same | gap: no DataFixerUpper codecs in the API (a Java framework); mods serialize with Rust code, and data crosses as JSON, NBT or bytes (documented in `types`) |
| `com.mojang.datafixers.util.Function3` | 156 | same | gap: no DataFixerUpper codecs in the API (a Java framework); mods serialize with Rust code, and data crosses as JSON, NBT or bytes (documented in `types`) |
| `com.mojang.datafixers.Products$P2` | 132 | same | gap: no DataFixerUpper codecs in the API (a Java framework); mods serialize with Rust code, and data crosses as JSON, NBT or bytes (documented in `types`) |
| `com.mojang.datafixers.util.Pair` | 129 | same | gap: no DataFixerUpper codecs in the API (a Java framework); mods serialize with Rust code, and data crosses as JSON, NBT or bytes (documented in `types`) |
| `com.mojang.datafixers.Products$P3` | 125 | same | gap: no DataFixerUpper codecs in the API (a Java framework); mods serialize with Rust code, and data crosses as JSON, NBT or bytes (documented in `types`) |
| `com.mojang.datafixers.types.Type` | 125 | same | gap: no DataFixerUpper codecs in the API (a Java framework); mods serialize with Rust code, and data crosses as JSON, NBT or bytes (documented in `types`) |
| `com.mojang.datafixers.Products$P1` | 102 | same | gap: no DataFixerUpper codecs in the API (a Java framework); mods serialize with Rust code, and data crosses as JSON, NBT or bytes (documented in `types`) |

#### Mojang logging and auth

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `com.mojang.logging.LogUtils` | 107 | same | `pumpkin:plugin/logging` |
| `com.mojang.authlib.GameProfile` | 89 | same | `pumpkin:plugin/player` (name and UUID); `network.payload-context.profile` |
