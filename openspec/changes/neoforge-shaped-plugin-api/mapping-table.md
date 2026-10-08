# Mapping table

This table maps the NeoForge 26.3 API to the native API `ironpumpkin-neo` and to the `pumpkin:plugin@0.2.0` interfaces it reuses. It is the contract source of the API; the future Wasm projection is generated from the same API (design.md, section "Native channel"). It has two parts: every server-side row of the inventory in design.md (#34), and the modpack coverage section, which ranks the gaps of the reference modpack scan (`modpack-usage.md`, #46) with a verdict each and maps the Minecraft, NeoForge and Mojang classes outside the inventory that mods use.

How to read it:

- Counterparts are written in interface notation, kebab-case: `interface.item` (for example `registration.deferred-register.register`) or `interface.variant::case`. The `ironpumpkin-neo` item has the same name in Rust case (`registration::DeferredRegister::register`). `pumpkin:plugin/x` is an interface of the v0.2 package.
- `callbacks.x` names code that NeoForge calls through a Java interface or lambda: a method of the trait object (or a closure) that the mod registers, the handler the mod registered. `callbacks.handle-payload` is the payload handler the mod registered for the channel.
- Status is the inventory status: `supported` works today, `planned` has a counterpart and waits for its backing phase, `not supported` has no counterpart and the reason is in the reason column of its inventory row in design.md.
- "gap" marks a used class or a planned row without a counterpart; the reason follows in the row, and "`x` doc" names the interface of the API that owns the gap. "partial" marks an extension interface whose methods a mod can call through the named counterpart but cannot override yet.
- **Mods** is the number of the 414 mods of the reference modpack (FTB StoneBlock 4) that have a server-side or mixed class that needs the row, from section 11.5 of `modpack-usage.md`. The scan read NeoForge 21.1 jars and matches rows by simple name, so a row whose 26.3 class has no 21.1 counterpart shows 0. Where section 10.3 or 11.3 of the scan lists the 21.1 name of such a class, the row shows the count of the old name and names it, for example 52 (21.1 `BlockEvent$BreakEvent`). Rows are ordered by Mods, highest first; rows with the same count keep the inventory order.
- Client-only rows (210 in the inventory) and dev-only classes (data generation and game tests) never run on a dedicated server and are not listed.
- A `not supported` row with a "contract only" counterpart: the API defines it so a port compiles against the final shape, and the host rejects the registration until a phase backs it.
- M2 to M5 are the milestones: M2 Content registry, M3 NeoForge handshake and registry sync, M4 NeoForge-shaped native API, M5 Modded gameplay parity.
- Task numbers (task 2.x) are the native items of `tasks.md` in this change; task 3.x projects task 2.x to Wasm.
- The M4 implementation tasks check each supported row with a test, as design.md requires.

## Coverage

| Part | Rows | Counterpart | Partial | Contract only | Gap or not supported | Used by 1 or more mods | Used by 50 or more mods |
|:--|--:|--:|--:|--:|--:|--:|--:|
| Inventory, supported | 108 | 108 | 0 | 0 | 0 | 89 | 13 |
| Inventory, planned | 487 | 444 | 43 | 0 | 0 | 386 | 108 |
| Inventory, not supported | 288 | 0 | 0 | 3 | 285 | 132 | 12 |
| Modpack classes outside the inventory, NeoForge | 104 | 94 | 0 | 0 | 10 | 102 | 44 |
| Modpack classes outside the inventory, Minecraft and Mojang | 314 | 251 | 0 | 0 | 63 | 309 | 174 |

## Inventory rows

### Events

| NeoForge item | Mods | Section | Counterpart | Status |
|:--|--:|:--|:--|:--|
| `RegisterPayloadHandlersEvent` | 170 | event class | `event-bus.event::register-payload-handlers-event` (resource `network.register-payload-handlers-event`) | planned |
| `FMLCommonSetupEvent` | 148 | event class | `event-bus.event::fml-common-setup-event` | planned |
| `RegisterCapabilitiesEvent` | 131 | event class | `event-bus.event::register-capabilities-event` (resource `capabilities.register-capabilities-event`) | planned |
| `PlayerEvent` | 96 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `RegisterCommandsEvent` | 93 | event class | `event-bus.event::register-commands-event` | supported |
| `ModConfigEvent` | 86 | event class | `event-bus.event::mod-config-event` | planned |
| `PlayerInteractEvent` | 85 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `BuildCreativeModeTabContentsEvent` | 75 | event class | `event-bus` doc, not supported | not supported |
| `PlayerInteractEvent.RightClickBlock` | 67 | event class | `event-bus.event::player-interact-event-right-click-block` | supported |
| `PlayerEvent.PlayerLoggedInEvent` | 66 | event class | `event-bus.event::player-event-player-logged-in-event` | supported |
| `BlockEvent` | 65 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `BreakBlockEvent` | 52 (21.1 `BlockEvent$BreakEvent`) | event class | `event-bus.event::break-block-event` | supported |
| `ServerTickEvent` | 52 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `EntityJoinLevelEvent` | 49 | event class | `event-bus.event::entity-join-level-event` | supported |
| `ModConfigEvent.Reloading` | 48 | event class | `event-bus.event::mod-config-event-reloading` | planned |
| `LivingDeathEvent` | 46 | event class | `event-bus.event::living-death-event` | supported |
| `RegisterEvent` | 46 | event class | `event-bus.event::register-event` (resource `registration.register-event`) | planned |
| `ServerTickEvent.Post` | 43 | event class | `event-bus.event::server-tick-event-post` | supported |
| `ModConfigEvent.Loading` | 43 | event class | `event-bus.event::mod-config-event-loading` | planned |
| `LevelEvent` | 42 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `AddServerReloadListenersEvent` | 41 (21.1 `AddReloadListenerEvent`) | event class | `event-bus.event::add-server-reload-listeners-event` (resource `lifecycle.add-server-reload-listeners-event`) | planned |
| `ServerStartedEvent` | 40 | event class | `event-bus.event::server-started-event` | supported |
| `LevelTickEvent` | 40 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `LivingDamageEvent` | 38 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `LivingIncomingDamageEvent` | 37 | event class | `event-bus.event::living-incoming-damage-event` | supported |
| `ServerStoppedEvent` | 36 | event class | `event-bus.event::server-stopped-event` | planned |
| `PlayerTickEvent` | 36 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `LevelTickEvent.Post` | 32 | event class | `event-bus.event::level-tick-event-post` | planned |
| `LivingDropsEvent` | 31 | event class | `event-bus.event::living-drops-event` | supported |
| `InterModEnqueueEvent` | 31 | event class | `event-bus.event::inter-mod-enqueue-event` | planned |
| `NewRegistryEvent` | 31 | event class | `event-bus` doc, not supported | not supported |
| `LevelEvent.Unload` | 30 | event class | `event-bus.event::level-event-unload` | supported |
| `OnDatapackSyncEvent` | 29 | event class | `event-bus.event::on-datapack-sync-event` | planned |
| `EntityAttributeCreationEvent` | 29 | event class | `event-bus.event::entity-attribute-creation-event` (resource `registration.entity-attribute-creation-event`) | planned |
| `ItemTooltipEvent` | 28 | event class | `event-bus` doc, not supported | not supported |
| `PlayerEvent.PlayerLoggedOutEvent` | 28 | event class | `event-bus.event::player-event-player-logged-out-event` | supported |
| `LivingDamageEvent.Pre` | 27 | event class | `event-bus.event::living-damage-event-pre` | planned |
| `PlayerInteractEvent.LeftClickBlock` | 27 | event class | `event-bus.event::player-interact-event-left-click-block` | supported |
| `LevelEvent.Load` | 27 | event class | `event-bus.event::level-event-load` | supported |
| `EntityTickEvent` | 27 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `PlayerEvent.PlayerChangedDimensionEvent` | 26 | event class | `event-bus.event::player-event-player-changed-dimension-event` | supported |
| `ServerStartingEvent` | 26 | event class | `event-bus.event::server-starting-event` | planned |
| `LivingDamageEvent.Post` | 25 | event class | `event-bus.event::living-damage-event-post` | planned |
| `PlayerEvent.PlayerRespawnEvent` | 24 | event class | `event-bus.event::player-event-player-respawn-event` | supported |
| `ServerAboutToStartEvent` | 24 | event class | `event-bus.event::server-about-to-start-event` | planned |
| `ServerStoppingEvent` | 24 | event class | `event-bus.event::server-stopping-event` | planned |
| `PlayerTickEvent.Post` | 24 | event class | `event-bus.event::player-tick-event-post` | planned |
| `FMLLoadCompleteEvent` | 24 | event class | `event-bus.event::fml-load-complete-event` | planned |
| `PlayerEvent.Clone` | 23 | event class | `event-bus.event::player-event-clone` | planned |
| `PlayerInteractEvent.RightClickItem` | 23 | event class | `event-bus.event::player-interact-event-right-click-item` | supported |
| `BlockEvent.EntityPlaceEvent` | 23 | event class | `event-bus.event::block-event-entity-place-event` | supported |
| `EntityTeleportEvent` | 22 | event class | `event-bus.event::entity-teleport-event` | supported |
| `LivingEvent` | 22 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `ItemEntityPickupEvent` | 20 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `TagsUpdatedEvent` | 19 | event class | `event-bus.event::tags-updated-event` | planned |
| `LevelTickEvent.Pre` | 19 | event class | `event-bus.event::level-tick-event-pre` | planned |
| `PlayerEvent.BreakSpeed` | 18 | event class | `event-bus.event::player-event-break-speed` | planned |
| `EntityTickEvent.Post` | 18 | event class | `event-bus.event::entity-tick-event-post` | planned |
| `PlayerTickEvent.Pre` | 18 | event class | `event-bus.event::player-tick-event-pre` | planned |
| `RegisterSpawnPlacementsEvent` | 17 | event class | `event-bus.event::register-spawn-placements-event` (resource `registration.register-spawn-placements-event`) | planned |
| `ItemEntityPickupEvent.Pre` | 17 | event class | `event-bus.event::item-entity-pickup-event-pre` | supported |
| `BlockDropsEvent` | 17 | event class | `event-bus.event::block-drops-event` | supported |
| `ChunkEvent` | 17 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `ServerTickEvent.Pre` | 17 | event class | `event-bus.event::server-tick-event-pre` | supported |
| `EntityAttributeModificationEvent` | 16 | event class | `event-bus.event::entity-attribute-modification-event` (resource `registration.entity-attribute-modification-event`) | planned |
| `FinalizeSpawnEvent` | 16 | event class | `event-bus.event::finalize-spawn-event` | supported |
| `LivingEntityUseItemEvent` | 16 | event class | `event-bus.event::living-entity-use-item-event` | planned |
| `PlayerInteractEvent.EntityInteract` | 16 | event class | `event-bus.event::player-interact-event-entity-interact` | supported |
| `RegisterDataMapTypesEvent` | 16 | event class | `event-bus.event::register-data-map-types-event` (resource `data-maps.register-data-map-types-event`) | planned |
| `LivingEvent.LivingJumpEvent` | 15 | event class | `event-bus.event::living-event-living-jump-event` | planned |
| `AttackEntityEvent` | 15 | event class | `event-bus.event::attack-entity-event` | supported |
| `ChunkEvent.Unload` | 15 | event class | `event-bus.event::chunk-event-unload` | supported |
| `LivingChangeTargetEvent` | 14 | event class | `event-bus.event::living-change-target-event` | supported |
| `LivingFallEvent` | 14 | event class | `event-bus.event::living-fall-event` | planned |
| `AddPackFindersEvent` | 13 | event class | `event-bus.event::add-pack-finders-event` (resource `lifecycle.add-pack-finders-event`) | planned |
| `LivingExperienceDropEvent` | 13 | event class | `event-bus.event::living-experience-drop-event` | planned |
| `LivingHealEvent` | 13 | event class | `event-bus.event::living-heal-event` | supported |
| `MobEffectEvent` | 13 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `PlayerEvent.StartTracking` | 13 | event class | `event-bus.event::player-event-start-tracking` | planned |
| `ProjectileImpactEvent` | 12 | event class | `event-bus.event::projectile-impact-event` | planned |
| `ExplosionEvent` | 12 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `EntityTickEvent.Pre` | 12 | event class | `event-bus.event::entity-tick-event-pre` | planned |
| `ItemAttributeModifierEvent` | 11 | event class | `event-bus.event::item-attribute-modifier-event` | planned |
| `MobSpawnEvent` | 11 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `PlayerEvent.ItemCraftedEvent` | 11 | event class | `event-bus.event::player-event-item-crafted-event` | supported |
| `ChunkWatchEvent` | 11 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `EntityEvent` | 10 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `LivingEquipmentChangeEvent` | 10 | event class | `event-bus.event::living-equipment-change-event` | planned |
| `PlayerContainerEvent` | 10 | event class | `event-bus.event::player-container-event` | supported |
| `ChunkEvent.Load` | 10 | event class | `event-bus.event::chunk-event-load` | planned |
| `ExplosionEvent.Detonate` | 10 | event class | `event-bus.event::explosion-event-detonate` | supported |
| `InterModProcessEvent` | 10 | event class | `event-bus.event::inter-mod-process-event` | planned |
| `RegisterTicketControllersEvent` | 10 | event class | `event-bus` doc, not supported | not supported |
| `AnvilUpdateEvent` | 9 | event class | `event-bus.event::anvil-update-event` | supported |
| `EntityInvulnerabilityCheckEvent` | 9 | event class | `event-bus.event::entity-invulnerability-check-event` | planned |
| `EntityLeaveLevelEvent` | 9 | event class | `event-bus.event::entity-leave-level-event` | planned |
| `EntityMountEvent` | 9 | event class | `event-bus.event::entity-mount-event` | supported |
| `LivingEntityUseItemEvent.Finish` | 9 | event class | `event-bus.event::living-entity-use-item-event-finish` | supported |
| `LivingEntityUseItemEvent.Start` | 9 | event class | `event-bus.event::living-entity-use-item-event-start` | planned |
| `LivingKnockBackEvent` | 9 | event class | `event-bus.event::living-knock-back-event` | planned |
| `ItemEntityPickupEvent.Post` | 9 | event class | `event-bus.event::item-entity-pickup-event-post` | planned |
| `PlayerContainerEvent.Open` | 9 | event class | `event-bus.event::player-container-event-open` | supported |
| `BlockEntityTypeAddBlocksEvent` | 8 | event class | `event-bus.event::block-entity-type-add-blocks-event` (resource `registration.block-entity-type-add-blocks-event`) | planned |
| `ItemTossEvent` | 8 | event class | `event-bus.event::item-toss-event` | supported |
| `LivingShieldBlockEvent` | 8 | event class | `event-bus.event::living-shield-block-event` | planned |
| `MobEffectEvent.Added` | 8 | event class | `event-bus.event::mob-effect-event-added` | supported |
| `LootTableLoadEvent` | 7 | event class | `event-bus.event::loot-table-load-event` | planned |
| `MobEffectEvent.Applicable` | 7 | event class | `event-bus.event::mob-effect-event-applicable` | planned |
| `MobSpawnEvent.PositionCheck` | 7 | event class | `event-bus.event::mob-spawn-event-position-check` | planned |
| `AdvancementEvent` | 7 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `PlayerInteractEvent.LeftClickEmpty` | 7 | event class | `event-bus` doc, not supported | not supported |
| `PlayerXpEvent` | 7 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `BlockEvent.BlockToolModificationEvent` | 7 | event class | `event-bus.event::block-event-block-tool-modification-event` | planned |
| `RegisterConfigurationTasksEvent` | 7 | event class | `event-bus.event::register-configuration-tasks-event` (resource `network.register-configuration-tasks-event`) | planned |
| `ModifyDefaultComponentsEvent` | 6 | event class | `event-bus` doc, not supported | not supported |
| `ServerChatEvent` | 6 | event class | `event-bus.event::server-chat-event` | supported |
| `EntityTeleportEvent.TeleportCommand` | 6 | event class | `event-bus.event::entity-teleport-event-teleport-command` | supported |
| `EntityTravelToDimensionEvent` | 6 | event class | `event-bus.event::entity-travel-to-dimension-event` | supported |
| `MobEffectEvent.Remove` | 6 | event class | `event-bus.event::mob-effect-event-remove` | supported |
| `AdvancementEvent.AdvancementEarnEvent` | 6 | event class | `event-bus.event::advancement-event-advancement-earn-event` | supported |
| `PlayerXpEvent.PickupXp` | 6 | event class | `event-bus.event::player-xp-event-pickup-xp` | planned |
| `BlockEvent.NeighborNotifyEvent` | 6 | event class | `event-bus.event::block-event-neighbor-notify-event` | supported |
| `ChunkWatchEvent.Sent` | 6 | event class | `event-bus.event::chunk-watch-event-sent` | supported |
| `ExplosionEvent.Start` | 6 | event class | `event-bus.event::explosion-event-start` | planned |
| `ModConfigEvent.Unloading` | 6 | event class | `event-bus.event::mod-config-event-unloading` | planned |
| `PermissionGatherEvent` | 6 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `PermissionGatherEvent.Nodes` | 6 | event class | `event-bus.event::permission-gather-event-nodes` | supported |
| `CommandEvent` | 5 | event class | `event-bus.event::command-event` | planned |
| `EntityTeleportEvent.EnderEntity` | 5 | event class | `event-bus.event::entity-teleport-event-ender-entity` | supported |
| `EntityTeleportEvent.EnderPearl` | 5 | event class | `event-bus.event::entity-teleport-event-ender-pearl` | supported |
| `LivingBreatheEvent` | 5 | event class | `event-bus.event::living-breathe-event` | supported |
| `LivingConversionEvent` | 5 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `MobEffectEvent.Expired` | 5 | event class | `event-bus.event::mob-effect-event-expired` | supported |
| `CriticalHitEvent` | 5 | event class | `event-bus.event::critical-hit-event` | planned |
| `PlayerContainerEvent.Close` | 5 | event class | `event-bus.event::player-container-event-close` | supported |
| `PlayerEvent.HarvestCheck` | 5 | event class | `event-bus.event::player-event-harvest-check` | planned |
| `ChunkDataEvent` | 5 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `ChunkDataEvent.Load` | 5 | event class | `event-bus.event::chunk-data-event-load` | planned |
| `ChunkWatchEvent.UnWatch` | 5 | event class | `event-bus.event::chunk-watch-event-un-watch` | planned |
| `ChunkWatchEvent.Watch` | 5 | event class | `event-bus.event::chunk-watch-event-watch` | supported |
| `LevelEvent.Save` | 5 | event class | `event-bus.event::level-event-save` | supported |
| `LivingEntityUseItemEvent.Stop` | 4 | event class | `event-bus.event::living-entity-use-item-event-stop` | planned |
| `LivingEntityUseItemEvent.Tick` | 4 | event class | `event-bus.event::living-entity-use-item-event-tick` | planned |
| `LivingEvent.LivingVisibilityEvent` | 4 | event class | `event-bus.event::living-event-living-visibility-event` | planned |
| `MobDespawnEvent` | 4 | event class | `event-bus.event::mob-despawn-event` | planned |
| `MobSpawnEvent.SpawnPlacementCheck` | 4 | event class | `event-bus.event::mob-spawn-event-spawn-placement-check` | planned |
| `BonemealEvent` | 4 | event class | `event-bus.event::bonemeal-event` | supported |
| `PlayerDestroyItemEvent` | 4 | event class | `event-bus.event::player-destroy-item-event` | supported |
| `PlayerEvent.ItemSmeltedEvent` | 4 | event class | `event-bus.event::player-event-item-smelted-event` | supported |
| `UseItemOnBlockEvent` | 4 | event class | `event-bus.event::use-item-on-block-event` | supported |
| `BlockEvent.EntityMultiPlaceEvent` | 4 | event class | `event-bus.event::block-event-entity-multi-place-event` | planned |
| `BlockEvent.FarmlandTrampleEvent` | 4 | event class | `event-bus.event::block-event-farmland-trample-event` | planned |
| `ChunkDataEvent.Save` | 4 | event class | `event-bus.event::chunk-data-event-save` | planned |
| `FMLConstructModEvent` | 4 | event class | `event-bus.event::fml-construct-mod-event` | planned |
| `DataMapsUpdatedEvent` | 4 | event class | `event-bus.event::data-maps-updated-event` | planned |
| `GameShuttingDownEvent` | 3 | event class | `event-bus.event::game-shutting-down-event` | planned |
| `GrindstoneEvent` | 3 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `GrindstoneEvent.OnPlaceItem` | 3 | event class | `event-bus.event::grindstone-event-on-place-item` | supported |
| `GrindstoneEvent.OnTakeItem` | 3 | event class | `event-bus.event::grindstone-event-on-take-item` | planned |
| `EntityEvent.Size` | 3 | event class | `event-bus` doc, not supported | not supported |
| `EntityMobGriefingEvent` | 3 | event class | `event-bus.event::entity-mob-griefing-event` | planned |
| `ArmorHurtEvent` | 3 | event class | `event-bus.event::armor-hurt-event` | supported |
| `BabyEntitySpawnEvent` | 3 | event class | `event-bus.event::baby-entity-spawn-event` | supported |
| `EndermanAngerEvent` | 3 (21.1 `EnderManAngerEvent`) | event class | `event-bus.event::enderman-anger-event` | planned |
| `LivingConversionEvent.Post` | 3 | event class | `event-bus.event::living-conversion-event-post` | planned |
| `LivingConversionEvent.Pre` | 3 | event class | `event-bus.event::living-conversion-event-pre` | supported |
| `LivingDestroyBlockEvent` | 3 | event class | `event-bus.event::living-destroy-block-event` | planned |
| `PlayerRespawnPositionEvent` | 3 | event class | `event-bus.event::player-respawn-position-event` | planned |
| `PlayerSpawnPhantomsEvent` | 3 | event class | `event-bus` doc, not supported | not supported |
| `ChunkTicketLevelUpdatedEvent` | 3 | event class | `event-bus` doc, not supported | not supported |
| `NoteBlockEvent` | 3 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `NoteBlockEvent.Play` | 3 | event class | `event-bus.event::note-block-event-play` | supported |
| `VillageSiegeEvent` | 3 | event class | `event-bus` doc, not supported | not supported |
| `ItemStackedOnOtherEvent` | 2 | event class | `event-bus.event::item-stacked-on-other-event` | planned |
| `PlayLevelSoundEvent` | 2 | event class | `event-bus.event::play-level-sound-event` | planned |
| `PlayLevelSoundEvent.AtEntity` | 2 | event class | `event-bus.event::play-level-sound-event-at-entity` | planned |
| `PlayLevelSoundEvent.AtPosition` | 2 | event class | `event-bus.event::play-level-sound-event-at-position` | planned |
| `GetEnchantmentLevelEvent` | 2 | event class | `event-bus.event::get-enchantment-level-event` | planned |
| `EntityEvent.EntityConstructing` | 2 | event class | `event-bus.event::entity-event-entity-constructing` | planned |
| `EntityStruckByLightningEvent` | 2 | event class | `event-bus.event::entity-struck-by-lightning-event` | planned |
| `EntityTeleportEvent.SpreadPlayersCommand` | 2 | event class | `event-bus.event::entity-teleport-event-spread-players-command` | planned |
| `ItemExpireEvent` | 2 | event class | `event-bus.event::item-expire-event` | supported |
| `AnimalTameEvent` | 2 | event class | `event-bus.event::animal-tame-event` | supported |
| `LivingGetProjectileEvent` | 2 | event class | `event-bus.event::living-get-projectile-event` | planned |
| `AnvilCraftEvent` | 2 (21.1 `AnvilRepairEvent`) | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `AnvilCraftEvent.Post` | 2 (21.1 `AnvilRepairEvent`) | event class | `event-bus.event::anvil-craft-event-post` | planned |
| `ArrowLooseEvent` | 2 | event class | `event-bus.event::arrow-loose-event` | planned |
| `PlayerEvent.NameFormat` | 2 | event class | `event-bus.event::player-event-name-format` | planned |
| `PlayerEvent.SaveToFile` | 2 | event class | `event-bus.event::player-event-save-to-file` | planned |
| `PlayerInteractEvent.RightClickEmpty` | 2 | event class | `event-bus` doc, not supported | not supported |
| `BlockEvent.FluidPlaceBlockEvent` | 2 | event class | `event-bus.event::block-event-fluid-place-block-event` | supported |
| `BlockGrowFeatureEvent` | 2 | event class | `event-bus.event::block-grow-feature-event` | supported |
| `ExplosionKnockbackEvent` | 2 | event class | `event-bus.event::explosion-knockback-event` | planned |
| `LevelEvent.PotentialSpawns` | 2 | event class | `event-bus.event::level-event-potential-spawns` | planned |
| `SleepFinishedTimeEvent` | 2 | event class | `event-bus.event::sleep-finished-time-event` | supported |
| `CropGrowEvent` | 2 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `CropGrowEvent.Post` | 2 | event class | `event-bus.event::crop-grow-event-post` | planned |
| `FMLDedicatedServerSetupEvent` | 2 | event class | `event-bus.event::fml-dedicated-server-setup-event` | planned |
| `ParallelDispatchEvent` | 2 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `ExtendPoiTypesEvent` | 2 | event class | `event-bus.event::extend-poi-types-event` (resource `registration.extend-poi-types-event`) | planned |
| `ModifyRegistriesEvent` | 2 | event class | `event-bus` doc, not supported | not supported |
| `RegisterGameTestsEvent` | 1 | event class | `event-bus` doc, not supported | not supported |
| `StatAwardEvent` | 1 | event class | `event-bus.event::stat-award-event` | supported |
| `EntityEvent.EnteringSection` | 1 | event class | `event-bus.event::entity-event-entering-section` | planned |
| `ItemEvent` | 1 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `LivingUseTotemEvent` | 1 | event class | `event-bus.event::living-use-totem-event` | supported |
| `MobSplitEvent` | 1 | event class | `event-bus.event::mob-split-event` | planned |
| `AdvancementEvent.AdvancementProgressEvent` | 1 | event class | `event-bus.event::advancement-event-advancement-progress-event` | planned |
| `ArrowNockEvent` | 1 | event class | `event-bus.event::arrow-nock-event` | planned |
| `CanContinueSleepingEvent` | 1 | event class | `event-bus.event::can-continue-sleeping-event` | planned |
| `CanPlayerSleepEvent` | 1 | event class | `event-bus.event::can-player-sleep-event` | supported |
| `ItemFishedEvent` | 1 | event class | `event-bus.event::item-fished-event` | supported |
| `PlayerEvent.LoadFromFile` | 1 | event class | `event-bus.event::player-event-load-from-file` | planned |
| `PlayerEvent.StopTracking` | 1 | event class | `event-bus.event::player-event-stop-tracking` | planned |
| `PlayerWakeUpEvent` | 1 | event class | `event-bus.event::player-wake-up-event` | supported |
| `PlayerXpEvent.XpChange` | 1 | event class | `event-bus.event::player-xp-event-xp-change` | supported |
| `TradeWithVillagerEvent` | 1 | event class | `event-bus.event::trade-with-villager-event` | planned |
| `BlockEvent.PortalSpawnEvent` | 1 | event class | `event-bus.event::block-event-portal-spawn-event` | supported |
| `ModifyCustomSpawnersEvent` | 1 | event class | `event-bus` doc, not supported | not supported |
| `PistonEvent` | 1 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `PistonEvent.Pre` | 1 | event class | `event-bus.event::piston-event-pre` | supported |
| `CropGrowEvent.Pre` | 1 | event class | `event-bus.event::crop-grow-event-pre` | supported |
| `RegisterCauldronFluidContentEvent` | 1 | event class | `event-bus` doc, not supported | not supported |
| `AddAttributeTooltipsEvent` | 0 | event class | `event-bus` doc, not supported | not supported |
| `DefaultDataComponentsBoundEvent` | 0 | event class | `event-bus` doc, not supported | not supported |
| `DifficultyChangeEvent` | 0 | event class | `event-bus.event::difficulty-change-event` | planned |
| `GatherSkippedAttributeTooltipsEvent` | 0 | event class | `event-bus` doc, not supported | not supported |
| `ModMismatchEvent` | 0 | event class | `event-bus` doc, not supported | not supported |
| `RegisterCauldronInteractionEvent` | 0 | event class | `event-bus` doc, not supported | not supported |
| `RegisterCauldronInteractionEvent.Dispatcher` | 0 | event class | `event-bus` doc, not supported | not supported |
| `RegisterCauldronInteractionEvent.Interaction` | 0 | event class | `event-bus` doc, not supported | not supported |
| `RegisterGameRuleCategoryEvent` | 0 | event class | `event-bus` doc, not supported | not supported |
| `RegisterRecipePropertiesEvent` | 0 | event class | `event-bus` doc, not supported | not supported |
| `RegisterStructureConversionsEvent` | 0 | event class | `event-bus` doc, not supported | not supported |
| `RegisterTooltipAppendersEvent` | 0 | event class | `event-bus` doc, not supported | not supported |
| `SortedReloadListenerEvent` | 0 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `TagsUpdatedEvent.ServerDataLoad` | 0 | event class | `event-bus.event::tags-updated-event-server-data-load` | planned |
| `VanillaGameEvent` | 0 | event class | `event-bus.event::vanilla-game-event` | supported |
| `PlayerBrewedPotionEvent` | 0 | event class | `event-bus.event::player-brewed-potion-event` | planned |
| `PotionBrewEvent` | 0 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `PotionBrewEvent.Post` | 0 | event class | `event-bus.event::potion-brew-event-post` | planned |
| `PotionBrewEvent.Pre` | 0 | event class | `event-bus.event::potion-brew-event-pre` | supported |
| `EnchantedBlockLootEvent` | 0 | event class | `event-bus.event::enchanted-block-loot-event` | planned |
| `EnchantedEntityLootEvent` | 0 | event class | `event-bus.event::enchanted-entity-loot-event` | planned |
| `EnchantmentLevelSetEvent` | 0 | event class | `event-bus.event::enchantment-level-set-event` | supported |
| `EntityTeleportEvent.ItemConsumption` | 0 | event class | `event-bus.event::entity-teleport-event-item-consumption` | supported |
| `EffectParticleModificationEvent` | 0 | event class | `event-bus.event::effect-particle-modification-event` | planned |
| `LivingDrownEvent` | 0 | event class | `event-bus.event::living-drown-event` | planned |
| `LivingSwapItemsEvent` | 0 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `LivingSwapItemsEvent.Hands` | 0 | event class | `event-bus.event::living-swap-items-event-hands` | supported |
| `SpawnClusterSizeEvent` | 0 | event class | `event-bus.event::spawn-cluster-size-event` | planned |
| `AnvilCraftEvent.Pre` | 0 | event class | `event-bus.event::anvil-craft-event-pre` | planned |
| `ClientInformationUpdatedEvent` | 0 | event class | `event-bus.event::client-information-updated-event` | planned |
| `CustomClickActionEvent` | 0 | event class | `event-bus.event::custom-click-action-event` | supported |
| `FluidTooltipEvent` | 0 | event class | `event-bus` doc, not supported | not supported |
| `PermissionsChangedEvent` | 0 | event class | `event-bus.event::permissions-changed-event` | planned |
| `PlayerEnchantItemEvent` | 0 | event class | `event-bus.event::player-enchant-item-event` | supported |
| `PlayerEvent.PlayerChangeGameModeEvent` | 0 | event class | `event-bus.event::player-event-player-change-game-mode-event` | supported |
| `PlayerEvent.TabListNameFormat` | 0 | event class | `event-bus.event::player-event-tab-list-name-format` | planned |
| `PlayerFlyableFallEvent` | 0 | event class | `event-bus.event::player-flyable-fall-event` | planned |
| `PlayerNegotiationEvent` | 0 | event class | `event-bus.event::player-negotiation-event` | planned |
| `PlayerSetSpawnEvent` | 0 | event class | `event-bus.event::player-set-spawn-event` | supported |
| `PlayerSwitchHotbarSlotEvent` | 0 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `PlayerSwitchHotbarSlotEvent.Post` | 0 | event class | `event-bus.event::player-switch-hotbar-slot-event-post` | planned |
| `PlayerSwitchHotbarSlotEvent.Pre` | 0 | event class | `event-bus.event::player-switch-hotbar-slot-event-pre` | supported |
| `PlayerXpEvent.LevelChange` | 0 | event class | `event-bus.event::player-xp-event-level-change` | supported |
| `SweepAttackEvent` | 0 | event class | `event-bus.event::sweep-attack-event` | planned |
| `AlterGroundEvent` | 0 | event class | `event-bus` doc, not supported | not supported |
| `GameRuleChangedEvent` | 0 | event class | `event-bus.event::game-rule-changed-event` | planned |
| `LevelEvent.CreateSpawnPosition` | 0 | event class | `event-bus.event::level-event-create-spawn-position` | planned |
| `NoteBlockEvent.Change` | 0 | event class | `event-bus.event::note-block-event-change` | planned |
| `PistonEvent.Post` | 0 | event class | `event-bus.event::piston-event-post` | planned |
| `CreateFluidSourceEvent` | 0 | event class | `event-bus.event::create-fluid-source-event` | planned |
| `RegisterRpcSchemaEvent` | 0 | event class | `event-bus.event::register-rpc-schema-event` (resource `lifecycle.register-rpc-schema-event`) | planned |
| `ServerLifecycleEvent` | 0 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `ModLifecycleEvent` | 0 | event class | no case (abstract); fields in each subclass record of `event-bus` | planned |
| `GatherDataEvent` | 0 | event class | `event-bus` doc, not supported | not supported |
| `GatherDataEvent.Client` | 0 | event class | `event-bus` doc, not supported | not supported |
| `GatherDataEvent.Server` | 0 | event class | `event-bus` doc, not supported | not supported |
| `XpOrbTargetingEvent` | 0 | event class | `event-bus.event::xp-orb-targeting-event` | planned |
| `NewDatapackRegistryEvent` | 0 | event class | `event-bus` doc, not supported | not supported |
| `PermissionGatherEvent.Handler` | 0 | event class | `event-bus` doc, not supported | not supported |

### Registries

| NeoForge item | Mods | Section | Counterpart | Status |
|:--|--:|:--|:--|:--|
| `DeferredHolder` | 224 | Registration API | `registration.deferred-holder` | planned |
| `DeferredRegister#register` | 203 | Registration API | `registration.deferred-register.register` | planned |
| `DeferredRegister#create` | 201 | Registration API | `registration.deferred-register.create` | planned |
| `item` | 132 | Static registries a `DeferredRegister` can target | `registration.entry-definition::item` | planned |
| `data_component_type` | 113 | Static registries a `DeferredRegister` can target | `registration.entry-definition::data-component-type`; values through `data-components` (task 2.6) | planned |
| `creative_mode_tab` | 112 | Static registries a `DeferredRegister` can target | `registration.entry-definition::creative-mode-tab` (contract only: the host accepts the entry and ignores it) | not supported |
| `NeoForgeRegistries` | 111 | Registration API | `registration` registry ids (`neoforge:*`) | planned |
| `DeferredItem` | 104 | Registration API | `registration.deferred-item` | planned |
| `block` | 98 | Static registries a `DeferredRegister` can target | `registration.entry-definition::block` | planned |
| `block_entity_type` | 90 | Static registries a `DeferredRegister` can target | `registration.entry-definition::block-entity-type` | planned |
| `DeferredRegister#createItems` | 85 | Registration API | `registration.deferred-register-items.create-items` | planned |
| `DeferredBlock` | 78 | Registration API | `registration.deferred-block` | planned |
| `menu` | 78 | Static registries a `DeferredRegister` can target | `registration.entry-definition::menu` | planned |
| `NeoForgeRegistries.Keys` | 77 | Registration API | `registration` registry ids (`neoforge:*`) | planned |
| `DeferredRegister.Items#register` | 76 | Registration API | `registration.deferred-register-items.register-item` | planned |
| `recipe_serializer` | 74 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `DeferredRegister#createBlocks` | 66 | Registration API | `registration.deferred-register-blocks.create-blocks` | planned |
| `DeferredRegister.Blocks#register` | 60 | Registration API | `registration.deferred-register-blocks.register-block` | planned |
| `sound_event` | 55 | Static registries a `DeferredRegister` can target | `registration.entry-definition::sound-event`; played by id with the v0.2 `play-custom-sound` (task 2.6) | planned |
| `neoforge:attachment_types` | 54 | Registries NeoForge adds | `registration.entry-definition::attachment-type` | planned |
| `recipe_type` | 49 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `RegistryBuilder` | 47 | Registration API | `registration` doc, not supported | not supported |
| `entity_type` | 46 | Static registries a `DeferredRegister` can target | `registration.entry-definition::entity-type` | planned |
| `DeferredRegister#createDataComponents` | 45 | Registration API | `registration.deferred-register-data-components.create-data-components` (task 2.6) | planned |
| `particle_type` | 41 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `DeferredRegister#getEntries` | 36 | Registration API | `registration.deferred-register.get-entries` | planned |
| `IRegistryExtension` | 31 | Registration API | `registration.registry-contains`, `registry-get-id`, `registry-get-key`, `registry-max-id`, `registry-resolve-alias` | planned |
| `DeferredRegister.DataComponents#registerComponentType` | 23 | Registration API | `registration.deferred-register-data-components.register-component-type`; values through `data-components` (task 2.6) | planned |
| `neoforge:global_loot_modifier_serializers` | 22 | Registries NeoForge adds | `loot.register-loot-modifier-type` | planned |
| `command_argument_type` | 21 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `mob_effect` | 19 | Static registries a `DeferredRegister` can target | `registration.entry-definition::mob-effect`; applied through `mob-effects` (task 2.6) | planned |
| `trigger_type` | 19 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `RegisterEvent.RegisterHelper#register` | 18 | Registration API | `registration.register-event.register` | planned |
| `DeferredRegister.Items#registerItem` | 17 | Registration API | `registration.deferred-register-items.register-item` | planned |
| `DeferredRegister.Items#registerSimpleBlockItem` | 16 | Registration API | `registration.deferred-register-items.register-simple-block-item` | planned |
| `neoforge:fluid_type` | 16 | Registries NeoForge adds | `registration.entry-definition::fluid-type` (contract only: fails with `registration-error::unsupported-registry` until a phase backs it) | not supported |
| `neoforge:condition_codecs` | 16 | Registries NeoForge adds | `registration` doc, not supported | not supported |
| `fluid` | 16 | Static registries a `DeferredRegister` can target | `registration.entry-definition::fluid` (contract only: fails with `registration-error::unsupported-registry` until a phase backs it) | not supported |
| `loot_function_type` | 16 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `attribute` | 13 | Static registries a `DeferredRegister` can target | `registration.entry-definition::attribute`; values through `entity-attributes` (task 2.6) | planned |
| `DeferredRegister#makeRegistry` | 12 | Registration API | `registration` doc, not supported | not supported |
| `DeferredRegister#addAlias` | 11 | Registration API | `registration.deferred-register.add-alias` | planned |
| `neoforge:biome_modifier_serializers` | 10 | Registries NeoForge adds | `registration` doc, not supported | not supported |
| `DeferredRegister.Blocks#registerBlock` | 9 | Registration API | `registration.deferred-register-blocks.register-block` | planned |
| `neoforge:entity_data_serializers` | 9 | Registries NeoForge adds | `registration` doc, not supported | not supported |
| `point_of_interest_type` | 9 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `worldgen/structure_type` | 8 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `worldgen/placement_modifier_type` | 8 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `loot_pool_entry_type` | 7 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `loot_condition_type` | 7 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `worldgen/structure_processor` | 7 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `registries.callback` | 7 | Other registry packages | `registration` doc, not supported | not supported |
| `GameData` | 6 | Registration API | `registration` doc, not supported | not supported |
| `villager_profession` | 6 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `worldgen/structure_piece` | 6 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `potion` | 5 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `custom_stat` | 5 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `registries.holdersets` | 5 | Other registry packages | `registration` doc, not supported | not supported |
| `worldgen/chunk_generator` | 4 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `DeferredRegister#getRegistry` | 3 | Registration API | `registration.registry-*` functions | planned |
| `worldgen/structure_placement` | 3 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `enchantment_effect_component_type` | 3 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `DeferredRegister#getNamespace` | 2 | Registration API | `registration.deferred-register.get-namespace` | planned |
| `DeferredRegister.Blocks#registerSimpleBlock` | 2 | Registration API | `registration.deferred-register-blocks.register-simple-block` | planned |
| `memory_module_type` | 2 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `sensor_type` | 2 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `height_provider_type` | 2 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `worldgen/trunk_placer_type` | 2 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `worldgen/structure_pool_element` | 2 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `map_decoration_type` | 2 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `DataPackRegistriesHooks` | 1 | Registration API | `registration` doc, not supported | not supported |
| `DeferredRegister#getRegistryKey` | 1 | Registration API | `registration.deferred-register.get-registry-name` | planned |
| `DeferredRegister.Items#registerSimpleItem` | 1 | Registration API | `registration.deferred-register-items.register-simple-item` | planned |
| `RegistryManager` | 1 | Registration API | `registration` doc, not supported | not supported |
| `RegistrySnapshot` | 1 | Registration API | host-side M3 `frozen_registry` sync; no mod API | planned |
| `neoforge:structure_modifier_serializers` | 1 | Registries NeoForge adds | `registration` doc, not supported | not supported |
| `neoforge:holder_set_type` | 1 | Registries NeoForge adds | `registration` doc, not supported | not supported |
| `neoforge:fluid_ingredient_type` | 1 | Registries NeoForge adds | `registration` doc, not supported | not supported |
| `rule_test` | 1 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `int_provider_type` | 1 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `worldgen/block_state_provider_type` | 1 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `worldgen/foliage_placer_type` | 1 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `worldgen/tree_decorator_type` | 1 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `worldgen/biome_source` | 1 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `worldgen/density_function_type` | 1 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `entity_sub_predicate_type` | 1 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `enchantment_entity_effect_type` | 1 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `BaseMappedRegistry` | 0 | Registration API | `registration` doc, not supported | not supported |
| `DataMapLoader` | 0 | Registration API | `data-maps` (host loader of `data_maps` JSON) | planned |
| `DatapackRegistryBuilder` | 0 | Registration API | `registration` doc, not supported | not supported |
| `DeferredRegister#createEntities` | 0 | Registration API | `registration.deferred-register-entities.create-entities` | planned |
| `DeferredRegister#createTagKey` | 0 | Registration API | `registration.deferred-register.create-tag-key` | planned |
| `DeferredRegister#getRegistryName` | 0 | Registration API | `registration.deferred-register.get-registry-name` | planned |
| `DeferredRegister.Entities#registerEntityType` | 0 | Registration API | `registration.deferred-register-entities.register-entity-type` | planned |
| `NeoForgeRegistriesSetup` | 0 | Registration API | `registration` doc, not supported | not supported |
| `RegistryManager.SnapshotType` | 0 | Registration API | host-side M3 `frozen_registry` sync; no mod API | planned |
| `neoforge:ingredient_serializer` | 0 | Registries NeoForge adds | `registration` doc, not supported | not supported |
| `neoforge:biome_modifier` | 0 | Registries NeoForge adds | `registration` doc, not supported | not supported |
| `neoforge:structure_modifier` | 0 | Registries NeoForge adds | `registration` doc, not supported | not supported |
| `neoforge:synced_attachment_types` | 0 | Registries NeoForge adds | `attachments.attachment-type-definition.sync` | planned |
| `game_event` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `debug_subscription` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `chunk_status` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `rule_block_entity_modifier` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `pos_rule_test` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `position_source_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `stat_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `villager_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `activity` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `context_float_provider_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `context_int_provider_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `loot_nbt_provider_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `loot_score_provider_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `float_provider_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `block_predicate_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `worldgen/carver_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `worldgen/feature_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `worldgen/root_placer_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `worldgen/feature_size_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `worldgen/material_condition_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `worldgen/material_rule_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `worldgen/pool_alias_binding` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `number_format_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `game_rule` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `data_component_predicate_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `enchantment_level_based_value_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `enchantment_location_based_effect_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `enchantment_value_effect_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `enchantment_provider_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `consume_effect_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `recipe_display` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `slot_display` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `recipe_book_category` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `ticket_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `incoming_rpc_methods` | 0 | Static registries a `DeferredRegister` can target | `lifecycle.register-rpc-schema-event` | planned |
| `outgoing_rpc_methods` | 0 | Static registries a `DeferredRegister` can target | `lifecycle.register-rpc-schema-event` | planned |
| `test_environment_definition_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `test_instance_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `spawn_condition_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `dialog_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `dialog_action_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `input_control_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `dialog_body_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `permission_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `permission_check_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `environment_attribute` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `attribute_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `slot_source_type` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `context_key_set` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |
| `test_function` | 0 | Static registries a `DeferredRegister` can target | `registration` doc, not supported | not supported |

### Capabilities

| NeoForge item | Mods | Section | Counterpart | Status |
|:--|--:|:--|:--|:--|
| `BlockCapability` | 153 | Capability API | `capabilities.block-capability` | planned |
| `Capabilities` | 138 | Capability API | `capabilities` doc (built-in capability names) | planned |
| `ICapabilityProvider` | 119 | Capability API | `callbacks.get-entity-capability`, `get-item-capability` | planned |
| `Capabilities.Item` | 112 | Capability API | `capabilities` doc (`neoforge:item_handler`) | planned |
| `Capabilities.Item.BLOCK` | 107 | Built-in capabilities | `capabilities` doc (Capabilities.Item.BLOCK) | planned |
| `RegisterCapabilitiesEvent#registerBlockEntity` | 95 | Capability API | `capabilities.register-capabilities-event.register-block-entity` | planned |
| `ItemCapability` | 88 | Capability API | `capabilities.item-capability` | planned |
| `Capabilities.Fluid` | 78 | Capability API | `capabilities` doc (`neoforge:fluid_handler`) | planned |
| `Capabilities.Energy` | 73 | Capability API | `capabilities` doc (`neoforge:energy_handler`) | planned |
| `Capabilities.Fluid.BLOCK` | 70 | Built-in capabilities | `capabilities` doc (Capabilities.Fluid.BLOCK) | planned |
| `Capabilities.Energy.BLOCK` | 60 | Built-in capabilities | `capabilities` doc (Capabilities.Energy.BLOCK) | planned |
| `RegisterCapabilitiesEvent#registerItem` | 53 | Capability API | `capabilities.register-capabilities-event.register-item` | planned |
| `Capabilities.Energy.ITEM` | 47 | Built-in capabilities | `capabilities` doc (Capabilities.Energy.ITEM) | planned |
| `EntityCapability` | 44 | Capability API | `capabilities.entity-capability` | planned |
| `Capabilities.Fluid.ITEM` | 42 | Built-in capabilities | `capabilities` doc (Capabilities.Fluid.ITEM) | planned |
| `IBlockCapabilityProvider` | 41 | Capability API | `callbacks.get-block-capability` | planned |
| `RegisterCapabilitiesEvent#registerBlock` | 37 | Capability API | `capabilities.register-capabilities-event.register-block` | planned |
| `BlockCapabilityCache` | 35 | Capability API | `capabilities.block-capability-cache` | planned |
| `Capabilities.Item.ITEM` | 22 | Built-in capabilities | `capabilities` doc (Capabilities.Item.ITEM) | planned |
| `Capabilities.Item.ENTITY` | 18 | Built-in capabilities | `capabilities` doc (Capabilities.Item.ENTITY) | planned |
| `RegisterCapabilitiesEvent#registerEntity` | 16 | Capability API | `capabilities.register-capabilities-event.register-entity` | planned |
| `ICapabilityInvalidationListener` | 8 | Capability API | `callbacks.capability-invalidated` | planned |
| `Capabilities.Item.ENTITY_AUTOMATION` | 7 | Built-in capabilities | `capabilities` doc (Capabilities.Item.ENTITY_AUTOMATION) | planned |
| `Capabilities.Energy.ENTITY` | 5 | Built-in capabilities | `capabilities` doc (Capabilities.Energy.ENTITY) | planned |
| `RegisterCapabilitiesEvent#isBlockRegistered` | 5 | Capability API | `capabilities.register-capabilities-event.is-block-registered` | planned |
| `Capabilities.Fluid.ENTITY` | 4 | Built-in capabilities | `capabilities` doc (Capabilities.Fluid.ENTITY) | planned |
| `BaseCapability` | 4 | Capability API | `capabilities.block-capability`, `entity-capability`, `item-capability` | planned |
| `RegisterCapabilitiesEvent#setProxyable` | 3 | Capability API | `capabilities.register-capabilities-event.set-proxyable` | planned |
| `CapabilityRegistry` | 1 | Capability API | `capabilities.*-capability.create` (name registry) | planned |
| `CapabilityRegistry.CapabilityConstructor` | 1 | Capability API | `capabilities.*-capability.create` | planned |
| `RegisterCapabilitiesEvent#setNonProxyable` | 1 | Capability API | `capabilities.register-capabilities-event.set-non-proxyable` | planned |
| `CapabilityHooks` | 0 | Capability API | `capabilities` doc, not supported | not supported |
| `CapabilityListenerHolder` | 0 | Capability API | `capabilities` doc, not supported | not supported |
| `RegisterCapabilitiesEvent#isEntityRegistered` | 0 | Capability API | `capabilities.register-capabilities-event.is-entity-registered` | planned |
| `RegisterCapabilitiesEvent#isItemRegistered` | 0 | Capability API | `capabilities.register-capabilities-event.is-item-registered` | planned |

### Attachments

| NeoForge item | Mods | Section | Counterpart | Status |
|:--|--:|:--|:--|:--|
| `Level` | 310 | Attachment holders | `attachments.attachment-holder` | planned |
| `Entity` | 249 | Attachment holders | `attachments.attachment-holder` | planned |
| `BlockEntity` | 235 | Attachment holders | `attachments.attachment-holder` | planned |
| `LevelChunk` | 70 | Attachment holders | `attachments.attachment-holder` | planned |
| `ChunkAccess` | 65 | Attachment holders | `attachments.attachment-holder` | planned |
| `IAttachmentHolder#getData` | 57 | Attachment API | `attachments.attachment-holder.get-data` | planned |
| `AttachmentType.Builder#build` | 55 | Attachment API | `registration.entry-definition::attachment-type` | planned |
| `AttachmentType#builder` | 46 | Attachment API | `attachments.attachment-type-definition` | planned |
| `AttachmentType.Builder#serialize` | 43 | Attachment API | `attachments.attachment-type-definition.serialize` | planned |
| `AttachmentType.Builder#copyOnDeath` | 25 | Attachment API | `attachments.attachment-type-definition.copy-on-death` | planned |
| `IAttachmentHolder#hasData` | 16 | Attachment API | `attachments.attachment-holder.has-data` | planned |
| `AttachmentType#serializable` | 15 | Attachment API | `attachments.attachment-type-definition.serialize` | planned |
| `AttachmentType.Builder#sync` | 10 | Attachment API | `attachments.attachment-type-definition.sync` | planned |
| `IAttachmentSerializer` | 7 | Attachment API | `attachments.attachment-type-definition.serialize` (host serializer) | planned |
| `AttachmentType.Builder#copyHandler` | 5 | Attachment API | `attachments.attachment-type-definition.copy-handler` | planned |
| `IAttachmentCopyHandler` | 5 | Attachment API | `callbacks.attachment-copy` | planned |
| `IAttachmentHolder#getExistingData` | 5 | Attachment API | `attachments.attachment-holder.get-existing-data` | planned |
| `AttachmentHolder` | 3 | Attachment API | `attachments.attachment-holder` | planned |
| `IAttachmentHolder#getExistingDataOrNull` | 3 | Attachment API | `attachments.attachment-holder.get-existing-data` | planned |
| `IAttachmentHolder#syncData` | 2 | Attachment API | `attachments.attachment-holder.sync-data` | planned |
| `IAttachmentHolder#hasAttachments` | 1 | Attachment API | `attachments.attachment-holder.has-attachments` | planned |
| `AttachmentHolder.AsField` | 0 | Attachment API | `attachments.attachment-holder` | planned |
| `AttachmentInternals` | 0 | Attachment API | `attachments` doc, not supported | not supported |
| `AttachmentSync` | 0 | Attachment API | `attachments` doc, not supported | not supported |
| `AttachmentSyncHandler` | 0 | Attachment API | `attachments.attachment-sync`, `callbacks.attachment-send-to-player` | planned |
| `LevelAttachmentsSavedData` | 0 | Attachment API | `attachments` doc, not supported | not supported |

### Data maps

| NeoForge item | Mods | Section | Counterpart | Status |
|:--|--:|:--|:--|:--|
| `DataMapType` | 27 | Data map API | `data-maps.data-map-type` | planned |
| `DataMapType.Builder` | 18 | Data map API | `data-maps.data-map-type` | planned |
| `WAXABLES` | 5 | Built-in data maps | `data-maps` doc (`neoforge:waxables`), `data-maps.get-data` | planned |
| `registries.datamaps.builtin` | 5 | Built-in data maps | `data-maps` doc (`neoforge:registries.datamaps.builtin`), `data-maps.get-data` | planned |
| `AdvancedDataMapType` | 3 | Data map API | `data-maps.data-map-type` | planned |
| `OXIDIZABLES` | 3 | Built-in data maps | `data-maps` doc (`neoforge:oxidizables`), `data-maps.get-data` | planned |
| `AdvancedDataMapType.Builder` | 2 | Data map API | `data-maps.data-map-type` | planned |
| `DataMapValueRemover` | 2 | Data map API | `data-maps.data-map-type.remover`, `callbacks.remove-data-map-value` | planned |
| `DataMapEntry` | 1 | Data map API | `data-maps.get-data` (JSON entry) | planned |
| `DataMapEntry.Removal` | 1 | Data map API | `callbacks.remove-data-map-value` | planned |
| `DataMapFile` | 1 | Data map API | `data-maps` host loader | planned |
| `DataMapValueMerger` | 1 | Data map API | `data-maps.value-merger`, `callbacks.merge-data-map-value` | planned |
| `DataMapValueRemover.Default` | 1 | Data map API | `data-maps.data-map-type.remover` (none) | planned |
| `IWithData` | 1 | Data map API | `data-maps.get-data` | planned |
| `MONSTER_ROOM_MOBS` | 1 | Built-in data maps | `data-maps` doc (`neoforge:monster_room_mobs`), `data-maps.get-data` | planned |
| `PARROT_IMITATIONS` | 1 | Built-in data maps | `data-maps` doc (`neoforge:parrot_imitations`), `data-maps.get-data` | planned |
| `RAID_HERO_GIFTS` | 1 | Built-in data maps | `data-maps` doc (`neoforge:raid_hero_gifts`), `data-maps.get-data` | planned |
| `VIBRATION_FREQUENCIES` | 1 | Built-in data maps | `data-maps` doc (`neoforge:vibration_frequencies`), `data-maps.get-data` | planned |
| `VILLAGER_TYPES` | 1 | Built-in data maps | `data-maps` doc (`neoforge:villager_types`), `data-maps.get-data` | planned |
| `DataMapsUpdatedEvent.UpdateCause` | 0 | Data map API | `event-bus.update-cause` | planned |
| `ACCEPTABLE_VILLAGER_DISTANCES` | 0 | Built-in data maps | `data-maps` doc (`neoforge:acceptable_villager_distances`), `data-maps.get-data` | planned |
| `BLOCK_TRANSFORM_APPENDERS` | 0 | Built-in data maps | `data-maps` doc (`neoforge:block_transform_appenders`), `data-maps.get-data` | planned |
| `TRANSFORMABLES` | 0 | Built-in data maps | `data-maps` doc (`neoforge:transformables`), `data-maps.get-data` | planned |
| `VILLAGER_COMPOSTABLES` | 0 | Built-in data maps | `data-maps` doc (`neoforge:villager_compostables`), `data-maps.get-data` | planned |

### Config

| NeoForge item | Mods | Section | Counterpart | Status |
|:--|--:|:--|:--|:--|
| `ModConfigSpec` | 182 | Config API | `mod-config.mod-config-spec` | planned |
| `ModConfig` | 165 | Config API | `mod-config.mod-config` | planned |
| `ModConfig.Type` | 165 | Config API | `mod-config.config-type` | planned |
| `IConfigSpec` | 162 | Config API | `mod-config.mod-config-spec` | planned |
| `ModConfigSpec.Builder#comment` | 156 | Config API | `mod-config.value-spec.comment` | planned |
| `ModConfigSpec.Builder#define` | 140 | Config API | `mod-config.value-kind::boolean`, `value-kind::%string` | planned |
| `ModConfigSpec.BooleanValue` | 138 | Config API | `mod-config.value-kind::boolean` | planned |
| `ModConfigSpec.Builder#defineInRange` | 134 | Config API | `mod-config.value-kind::*-in-range` | planned |
| `ModConfigSpec.Builder#push` | 132 | Config API | `mod-config.value-spec.path`, `section-comment` | planned |
| `ModConfigSpec.Builder#pop` | 130 | Config API | `mod-config.value-spec.path` | planned |
| `ModConfigSpec.Builder#build` | 123 | Config API | `mod-config.mod-config-spec` | planned |
| `ModConfigSpec.IntValue` | 119 | Config API | `mod-config.value-kind::int-in-range` | planned |
| `ModConfigSpec.ConfigValue` | 104 | Config API | `mod-config.mod-config.get`, `set`, `save`, `get-default` | planned |
| `ModConfig.Type.LOCAL` | 102 | Config types | `mod-config.config-type::local` | planned |
| `ModConfig.Type.SYNCED` | 75 | Config types | `mod-config.config-type::synced` | planned |
| `ModConfigSpec.DoubleValue` | 68 | Config API | `mod-config.value-kind::double-in-range` | planned |
| `ModConfigSpec.Builder#configure` | 57 | Config API | `mod-config.mod-config-spec` | planned |
| `ModConfigSpec.EnumValue` | 39 | Config API | `mod-config.value-kind::enum-constant` | planned |
| `ModConfigSpec.Builder#translation` | 38 | Config API | `mod-config.value-spec.translation` | planned |
| `ModConfigSpec.Builder#defineEnum` | 37 | Config API | `mod-config.value-kind::enum-constant` | planned |
| `ModConfigSpec.Builder#defineList` | 31 | Config API | `mod-config.value-kind::%list` | planned |
| `ModConfigSpec.Builder#defineListAllowEmpty` | 31 | Config API | `mod-config.list-spec.allow-empty` | planned |
| `ModConfigSpec.LongValue` | 22 | Config API | `mod-config.value-kind::long-in-range` | planned |
| `ModConfig.Type.STARTUP` | 17 | Config types | `mod-config.config-type::startup` | planned |
| `ModConfigSpec.Builder#worldRestart` | 15 | Config API | `mod-config.restart-type::world` | planned |
| `ModConfigSpec.ValueSpec` | 7 | Config API | `mod-config.value-spec` | planned |
| `ModConfigSpec.Range` | 5 | Config API | `mod-config.int-range`, `long-range`, `double-range` | planned |
| `IConfigSpec.ILoadedConfig` | 5 | Config API | `mod-config.mod-config` | planned |
| `ModConfigSpec.Builder#gameRestart` | 4 | Config API | `mod-config.restart-type::game` | planned |
| `ConfigTracker` | 4 | Config API | `mod-config` doc, not supported | not supported |
| `ModConfigSpec.RestartType` | 3 | Config API | `mod-config.restart-type` | planned |
| `ModConfigs` | 3 | Config API | `mod-config.get-mod-configs`, `get-file-names` | planned |
| `ModConfigSpec.Builder#defineInList` | 1 | Config API | `mod-config.string-spec.allowed` | planned |
| `ModConfigSpec.ListValueSpec` | 0 | Config API | `mod-config.list-spec` | planned |
| `NeoForgeLocalConfig` | 0 | Config API | `mod-config` doc, not supported | not supported |
| `NeoForgeSyncedConfig` | 0 | Config API | `mod-config` doc, not supported | not supported |

### Networking

| NeoForge item | Mods | Section | Counterpart | Status |
|:--|--:|:--|:--|:--|
| `IPayloadHandler` | 166 | Configuration tasks and other network types | `callbacks.handle-payload` | planned |
| `IPayloadContext#player` | 154 | Payload registration and sending | `network.payload-context.player` | planned |
| `IPayloadContext#enqueueWork` | 151 | Payload registration and sending | `pumpkin:plugin/scheduler` (handlers on `handler-thread::main` run on the tick thread) | planned |
| `PayloadRegistrar#playToClient` | 125 | Payload registration and sending | `network.payload-registrar.play-to-client` | planned |
| `PayloadRegistrar#playToServer` | 123 | Payload registration and sending | `network.payload-registrar.play-to-server` | planned |
| `PacketDistributor#sendToPlayer` | 121 | Payload registration and sending | `network.send-to-player` | supported |
| `IContainerFactory` | 84 | Configuration tasks and other network types | `registration.menu-type-definition.extra-data`, `menus.menu-definition.extra-data` | planned |
| `PayloadRegistrar#versioned` | 54 | Payload registration and sending | `network.payload-registrar.versioned` | planned |
| `PacketDistributor#sendToPlayersTrackingChunk` | 43 | Payload registration and sending | `network.send-to-players-tracking-chunk` | planned |
| `PayloadRegistrar#optional` | 42 | Payload registration and sending | `network.payload-registrar.optional` | planned |
| `IPayloadContext#flow` | 41 | Payload registration and sending | `network.payload-context.flow` | planned |
| `PacketDistributor#sendToAllPlayers` | 39 | Payload registration and sending | `network.send-to-all-players` | planned |
| `network.codec` | 34 | Other network packages | `network` doc, not supported | not supported |
| `PayloadRegistrar#playBidirectional` | 32 | Payload registration and sending | `network.payload-registrar.play-bidirectional` | planned |
| `PacketDistributor#sendToPlayersTrackingEntity` | 27 | Payload registration and sending | `network.send-to-players-tracking-entity` | planned |
| `PacketDistributor#sendToPlayersNear` | 20 | Payload registration and sending | `network.send-to-players-near` | planned |
| `PacketDistributor#sendToPlayersTrackingEntityAndSelf` | 19 | Payload registration and sending | `network.send-to-players-tracking-entity-and-self` | planned |
| `IPayloadContext#disconnect` | 17 | Payload registration and sending | `network.payload-context.disconnect` | planned |
| `HandlerThread` | 12 | Configuration tasks and other network types | `network.handler-thread` | planned |
| `PacketDistributor#sendToPlayersInDimension` | 11 | Payload registration and sending | `network.send-to-players-in-dimension` | planned |
| `PayloadRegistrar#executesOn` | 10 | Payload registration and sending | `network.payload-registrar.executes-on` | planned |
| `network.connection` | 8 | Other network packages | `network` doc, not supported | not supported |
| `PayloadRegistrar#configurationToClient` | 7 | Payload registration and sending | `network.payload-registrar.configuration-to-client` | planned |
| `NetworkRegistry` | 7 | Configuration tasks and other network types | `network` doc, not supported | not supported |
| `IPayloadContext#finishCurrentTask` | 6 | Payload registration and sending | `network.payload-context.finish-current-task` | planned |
| `IPayloadContext#connection` | 5 | Payload registration and sending | `network` doc, not supported | not supported |
| `IPayloadContext#reply` | 5 | Payload registration and sending | `network.payload-context.reply` | supported |
| `ICustomConfigurationTask` | 5 | Configuration tasks and other network types | `network.configuration-task`, `callbacks.run-configuration-task` | planned |
| `PayloadRegistrar#commonBidirectional` | 3 | Payload registration and sending | `network.payload-registrar.common-bidirectional` | planned |
| `MainThreadPayloadHandler` | 3 | Configuration tasks and other network types | `network.handler-thread::main` | planned |
| `PayloadRegistrar#configurationToServer` | 2 | Payload registration and sending | `network.payload-registrar.configuration-to-server` | planned |
| `PayloadRegistrar#commonToServer` | 2 | Payload registration and sending | `network.payload-registrar.common-to-server` | planned |
| `IPayloadContext#protocol` | 2 | Payload registration and sending | `network.payload-context.protocol` | planned |
| `PayloadRegistrar#commonToClient` | 1 | Payload registration and sending | `network.payload-registrar.common-to-client` | planned |
| `IPayloadContext#listener` | 1 | Payload registration and sending | `network` doc, not supported | not supported |
| `network.bundle` | 1 | Other network packages | `network` doc, not supported | not supported |
| `PayloadRegistrar#configurationBidirectional` | 0 | Payload registration and sending | `network.payload-registrar.configuration-bidirectional` | planned |
| `IPayloadContext#handle` | 0 | Payload registration and sending | `network` doc, not supported | not supported |
| `IPayloadContext#channelHandlerContext` | 0 | Payload registration and sending | `network` doc, not supported | not supported |
| `neoforge:advanced_add_entity` | 0 | Payloads | `registration.entity-type-builder` (host payload, M3) | planned |
| `neoforge:advanced_container_set_data` | 0 | Payloads | `registration.menu-type-definition` (host payload, M5) | planned |
| `neoforge:advanced_open_screen` | 0 | Payloads | `registration.menu-type-definition` (host payload, M5) | planned |
| `neoforge:auxiliary_light_data` | 0 | Payloads | `network` doc, not supported | not supported |
| `c:register` | 0 | Payloads | host-side wire protocol (phase 1 codec, M3); no mod API | planned |
| `c:version` | 0 | Payloads | host-side wire protocol (phase 1 codec, M3); no mod API | planned |
| `neoforge:config_file` | 0 | Payloads | `mod-config.config-type::synced` (host payload) | planned |
| `neoforge:extensible_enum_ack` | 0 | Payloads | host-side wire protocol (phase 1 codec, M3); no mod API | planned |
| `neoforge:extensible_enum_data` | 0 | Payloads | host-side wire protocol (phase 1 codec, M3); no mod API | planned |
| `neoforge:feature_flags_ack` | 0 | Payloads | host-side wire protocol (phase 1 codec, M3); no mod API | planned |
| `neoforge:feature_flags` | 0 | Payloads | host-side wire protocol (phase 1 codec, M3); no mod API | planned |
| `neoforge:frozen_registry` | 0 | Payloads | host-side wire protocol (phase 1 codec, M3); no mod API | planned |
| `neoforge:frozen_registry_sync_completed` | 0 | Payloads | host-side wire protocol (phase 1 codec, M3); no mod API | planned |
| `neoforge:frozen_registry_sync_start` | 0 | Payloads | host-side wire protocol (phase 1 codec, M3); no mod API | planned |
| `neoforge:known_registry_data_maps` | 0 | Payloads | `data-maps.data-map-type.synced` (host payload) | planned |
| `neoforge:known_registry_data_maps_reply` | 0 | Payloads | `data-maps.data-map-type.synced` (host payload) | planned |
| `minecraft:register` | 0 | Payloads | `pumpkin:plugin/event` channel events; `network.has-channel` | supported |
| `minecraft:unregister` | 0 | Payloads | `pumpkin:plugin/event` channel events; `network.has-channel` | supported |
| `neoforge:network` | 0 | Payloads | host-side wire protocol (phase 1 codec, M3); no mod API | planned |
| `neoforge:register` | 0 | Payloads | host-side wire protocol (phase 1 codec, M3); no mod API | supported |
| `neoforge:modded_network_setup_failed` | 0 | Payloads | host-side wire protocol (phase 1 codec, M3); no mod API | planned |
| `neoforge:recipe_content` | 0 | Payloads | `network` doc, not supported | not supported |
| `neoforge:registry_data_map_sync` | 0 | Payloads | `data-maps.data-map-type.synced` (host payload) | planned |
| `neoforge:split` | 0 | Payloads | host-side wire protocol (phase 1 codec, M3); no mod API | planned |
| `neoforge:sync_attachments` | 0 | Payloads | `attachments.attachment-sync` (host payload) | planned |
| `CheckExtensibleEnums` | 0 | Configuration tasks and other network types | host-side M3 configuration task; no mod API | planned |
| `CheckExtensibleEnums.EnumEntry` | 0 | Configuration tasks and other network types | host-side M3; no mod API | planned |
| `CheckExtensibleEnums.ExtensionData` | 0 | Configuration tasks and other network types | host-side M3; no mod API | planned |
| `CheckFeatureFlags` | 0 | Configuration tasks and other network types | host-side M3 configuration task; no mod API | planned |
| `CommonRegisterTask` | 0 | Configuration tasks and other network types | host-side M3 configuration task; no mod API | planned |
| `CommonVersionTask` | 0 | Configuration tasks and other network types | host-side M3 configuration task; no mod API | planned |
| `RegistryDataMapNegotiation` | 0 | Configuration tasks and other network types | host-side M3 configuration task; no mod API | planned |
| `SyncConfig` | 0 | Configuration tasks and other network types | host-side M3 task for `mod-config.config-type::synced` | planned |
| `SyncRegistries` | 0 | Configuration tasks and other network types | host-side M3 task; no mod API | planned |
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
| `file license` | 414 | `neoforge.mods.toml` keys | `lifecycle.mods-toml.license` | planned |
| `file mods` | 414 | `neoforge.mods.toml` keys | `lifecycle.mods-toml.mods` | planned |
| `[[mods]] modId` | 414 | `neoforge.mods.toml` keys | `lifecycle.mod-info.mod-id` / `mod-dependency.mod-id` | supported |
| `[[mods]] version` | 414 | `neoforge.mods.toml` keys | `lifecycle.mod-info.version` | supported |
| `[[mods]] displayName` | 414 | `neoforge.mods.toml` keys | `lifecycle.mod-info.display-name` | planned |
| `[[mods]] description` | 414 | `neoforge.mods.toml` keys | `lifecycle.mod-info.description` | supported |
| `@Mod(value)` | 411 | Entry point annotations | mod registration (`register_mod!`), the mod's `init` and its embedded `neoforge.mods.toml` | planned |
| `[[dependencies.<modid>]] modId` | 404 | `neoforge.mods.toml` keys | `lifecycle.mod-info.mod-id` / `mod-dependency.mod-id` | supported |
| `[[dependencies.<modid>]] versionRange` | 404 | `neoforge.mods.toml` keys | `lifecycle.mod-dependency.version-range` | planned |
| `[[dependencies.<modid>]] side` | 398 | `neoforge.mods.toml` keys | `lifecycle.mod-dependency.side` | planned |
| `[[mods]] authors` | 392 | `neoforge.mods.toml` keys | `lifecycle.mod-info.authors` | supported |
| `[[dependencies.<modid>]] ordering` | 388 | `neoforge.mods.toml` keys | `lifecycle.mod-dependency.ordering` | planned |
| `[[dependencies.<modid>]] type` | 361 | `neoforge.mods.toml` keys | `lifecycle.mod-dependency.type` | planned |
| `[[mixins]] config` | 291 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `file mixins` | 278 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `IEventBus#addListener` | 263 | Event bus and mod container | `event-bus.add-listener` | planned |
| `file issueTrackerURL` | 236 | `neoforge.mods.toml` keys | `lifecycle.mods-toml.issue-tracker-url`, `mod-info.issue-tracker-url` | planned |
| `ModList#get` | 200 | Event bus and mod container | `lifecycle` ModList functions | planned |
| `@EventBusSubscriber(modid)` | 186 | Entry point annotations | `event-bus.add-listener` of the calling mod | planned |
| `ModList#isLoaded` | 175 | Event bus and mod container | `lifecycle.is-loaded` | planned |
| `ModContainer#registerConfig` | 159 | Event bus and mod container | `mod-config.register-config` | planned |
| `@EventBusSubscriber(value)` | 151 | Entry point annotations | `event-bus.add-listener` (client-dist subscribers are not registered) | planned |
| `IEventBus#register` | 112 | Event bus and mod container | `event-bus.add-listener` | planned |
| `EventPriority` | 99 | Event bus and mod container | `pumpkin:plugin/event.event-priority` | supported |
| `IEventBus#post` | 86 | Event bus and mod container | `event-bus.post` | planned |
| `@Mod(dist)` | 77 | Entry point annotations | `types.dist` (always `dedicated-server`) | planned |
| `@SubscribeEvent(priority)` | 76 | Entry point annotations | `event-bus.add-listener` `priority` (`pumpkin:plugin/event.event-priority`) | supported |
| `ModList#getModContainerById` | 69 | Event bus and mod container | `lifecycle.get-mod-container-by-id` | planned |
| `ModContainer#getModInfo` | 61 | Event bus and mod container | `lifecycle.mod-container.get-mod-info` | supported |
| `file accessTransformers` | 55 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `[[accessTransformers]] file` | 55 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `[[mods]] updateJSONURL` | 38 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `ModContainer#getEventBus` | 32 | Event bus and mod container | `event-bus.bus::mod` | planned |
| `ModList#getMods` | 31 | Event bus and mod container | `lifecycle.get-mods` | planned |
| `ModList#getModFileById` | 16 | Event bus and mod container | `lifecycle` doc, not supported | not supported |
| `ModList#getAllScanData` | 16 | Event bus and mod container | `lifecycle` doc, not supported | not supported |
| `IEventBus#start` | 15 | Event bus and mod container | `lifecycle` doc, not supported | not supported |
| `ModContainer#getModId` | 15 | Event bus and mod container | `lifecycle.mod-container.get-mod-id` | supported |
| `[[dependencies.<modid>]] reason` | 14 | `neoforge.mods.toml` keys | `lifecycle.mod-dependency.reason` | planned |
| `[[mods]] enumExtensions` | 13 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `IEventBus#unregister` | 12 | Event bus and mod container | `lifecycle` doc, not supported | not supported |
| `[[mods]] modUrl` | 10 | `neoforge.mods.toml` keys | `lifecycle.mod-info.mod-url` | planned |
| `[[mods]] issueTrackerURL` | 8 | `neoforge.mods.toml` keys | `lifecycle.mods-toml.issue-tracker-url`, `mod-info.issue-tracker-url` | planned |
| `@SubscribeEvent(receiveCanceled)` | 6 | Entry point annotations | `event-bus.add-listener` `receive-canceled` | planned |
| `[[mods]] featureFlags` | 4 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `ModList#getModFiles` | 3 | Event bus and mod container | `lifecycle` doc, not supported | not supported |
| `ModList#forEachModInOrder` | 3 | Event bus and mod container | `lifecycle.get-sorted-mods` | planned |
| `[[dependencies.<modid>]] referralUrl` | 3 | `neoforge.mods.toml` keys | `lifecycle.mod-dependency.referral-url` | planned |
| `ModList#size` | 2 | Event bus and mod container | `lifecycle.size` | planned |
| `ModList#forEachModFile` | 2 | Event bus and mod container | `lifecycle` doc, not supported | not supported |
| `ModList#forEachModContainer` | 2 | Event bus and mod container | `lifecycle.get-mods` | planned |
| `ModList#getSortedMods` | 2 | Event bus and mod container | `lifecycle.get-sorted-mods` | planned |
| `[[mixins]] requiredMods` | 2 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `ModContainer#getNamespace` | 1 | Event bus and mod container | `lifecycle.mod-container.get-namespace` | planned |
| `ModContainer#acceptEvent` | 1 | Event bus and mod container | `lifecycle` doc, not supported | not supported |
| `ModList#applyForEachModContainer` | 1 | Event bus and mod container | `lifecycle.get-mods` | planned |
| `file showAsResourcePack` | 1 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `@Mod(depends)` | 0 | Entry point annotations | `lifecycle.mod-dependency`; `pumpkin:plugin/metadata` dependencies | supported |
| `ModList#of` | 0 | Event bus and mod container | `lifecycle` doc, not supported | not supported |
| `ModList#applyForEachModFile` | 0 | Event bus and mod container | `lifecycle` doc, not supported | not supported |
| `ModList#applyForEachModFileAlphabetical` | 0 | Event bus and mod container | `lifecycle` doc, not supported | not supported |
| `ModList#clear` | 0 | Event bus and mod container | `lifecycle` doc, not supported | not supported |
| `file showAsDataPack` | 0 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `file services` | 0 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `file properties` | 0 | `neoforge.mods.toml` keys | `lifecycle.mods-toml.properties` | planned |
| `[[mixins]] behaviorVersion` | 0 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `[[mods]] namespace` | 0 | `neoforge.mods.toml` keys | `lifecycle.mod-info.namespace` | planned |
| `[[mods]] dependencies` | 0 | `neoforge.mods.toml` keys | `lifecycle.mod-info.dependencies` | supported |
| `[[mods]] features` | 0 | `neoforge.mods.toml` keys | `lifecycle` doc, not supported | not supported |
| `[[mods]] modproperties` | 0 | `neoforge.mods.toml` keys | `lifecycle.mod-info.mod-properties` | planned |

### Loot modifiers, loot conditions and data conditions

| NeoForge item | Mods | Section | Counterpart | Status |
|:--|--:|:--|:--|:--|
| `ICondition` | 42 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | not supported |
| `ICondition.IContext` | 32 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | not supported |
| `IGlobalLootModifier` | 29 | Loot modifiers, loot conditions and data conditions | `callbacks.decode-loot-modifier`, `callbacks.apply-loot-modifier` | planned |
| `LootModifier` | 25 | Loot modifiers, loot conditions and data conditions | `loot.loot-modifier-entry` (`conditions`, `priority` in the JSON) | planned |
| `ConditionalOps` | 12 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | not supported |
| `WithConditions` | 8 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | not supported |
| `ModLoadedCondition` | 7 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | not supported |
| `AddTableLootModifier` | 2 | Loot modifiers, loot conditions and data conditions | `loot` built-in type `neoforge:add_table`; `loot.roll-loot-table` | planned |
| `CanItemPerformAbility` | 2 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | not supported |
| `LootTableIdCondition` | 2 | Loot modifiers, loot conditions and data conditions | `loot` built-in condition `neoforge:loot_table_id` | planned |
| `AndCondition` | 2 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | not supported |
| `LootModifierManager` | 1 | Loot modifiers, loot conditions and data conditions | `loot.get-loot-modifiers` (host loader) | planned |
| `LootTableIdCondition.Builder` | 1 | Loot modifiers, loot conditions and data conditions | `loot` built-in condition `neoforge:loot_table_id` | planned |
| `ConditionContext` | 1 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | not supported |
| `NotCondition` | 1 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | not supported |
| `OrCondition` | 1 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | not supported |
| `TagEmptyCondition` | 1 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | not supported |
| `NeoForgeLootContextParams` | 0 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | not supported |
| `AlwaysCondition` | 0 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | not supported |
| `FeatureFlagsEnabledCondition` | 0 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | not supported |
| `NeoForgeConditions` | 0 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | not supported |
| `NeverCondition` | 0 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | not supported |
| `RegisteredCondition` | 0 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | not supported |
| `WithConditions.Builder` | 0 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | not supported |
| `GlobalLootModifierProvider` | 0 | Loot modifiers, loot conditions and data conditions | `loot` doc, not supported | not supported |

### Extension interfaces NeoForge adds to `net.minecraft`

| NeoForge item | Mods | Section | Counterpart | Status |
|:--|--:|:--|:--|:--|
| `IBlockStateExtension` | 274 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `types.block-state`; `pumpkin:plugin/world`; overriding hooks is a gap (documented in `types`) | planned |
| `ILevelExtension` | 127 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `pumpkin:plugin/world`; overriding hooks is a gap (documented in `types`) | planned |
| `IItemStackExtension` | 122 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `pumpkin:plugin/item-stack`; overriding hooks is a gap (documented in `types`) | planned |
| `ICommandSourceStackExtension` | 104 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `pumpkin:plugin/command` `command-sender`; overriding hooks is a gap (documented in `types`) | planned |
| `ICommonPacketListener` | 97 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `network.has-channel`, `network.payload-context`; overriding hooks is a gap (documented in `types`) | planned |
| `IDataComponentHolderExtension` | 90 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `data-components` and the `pumpkin:plugin/item-stack` components; overriding hooks is a gap (documented in `types`) | planned |
| `IEntityExtension` | 87 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `pumpkin:plugin/world` `entity`; overriding hooks is a gap (documented in `types`) | planned |
| `IBlockEntityExtension` | 85 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `pumpkin:plugin/block-entity`; overriding hooks is a gap (documented in `types`) | planned |
| `IPlayerExtension` | 85 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `pumpkin:plugin/player` and `menus.open-menu` (`openMenu`); overriding hooks is a gap (documented in `types`) | planned |
| `IMenuTypeExtension` | 70 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `registration.menu-type-definition.extra-data` and `menus.menu-definition.extra-data`; overriding hooks is a gap (documented in `types`) | planned |
| `IItemExtension` | 65 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `registration.item-definition.behaviour`; overriding hooks is a gap (documented in `types`) | planned |
| `IHolderExtension` | 60 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `registration.deferred-holder`; overriding hooks is a gap (documented in `types`) | planned |
| `IFluidExtension` | 49 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `registration.fluid-definition`; overriding hooks is a gap (documented in `types`) | planned |
| `IBlockExtension` | 47 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `registration.block-definition.block-type`; overriding hooks is a gap (documented in `types`) | planned |
| `IFluidStateExtension` | 47 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `types.fluid-state`; overriding hooks is a gap (documented in `types`) | planned |
| `ILevelReaderExtension` | 37 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `pumpkin:plugin/world`; overriding hooks is a gap (documented in `types`) | planned |
| `IItemPropertiesExtensions` | 32 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `registration.item-properties`; overriding hooks is a gap (documented in `types`) | planned |
| `IPacketFlowExtension` | 31 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `network.packet-flow`; overriding hooks is a gap (documented in `types`) | planned |
| `IHolderLookupProviderExtension` | 27 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `registration.registry-*` functions; overriding hooks is a gap (documented in `types`) | planned |
| `IDataComponentMapBuilderExtensions` | 24 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `types.component-value`; overriding hooks is a gap (documented in `types`) | planned |
| `IServerCommonPacketListenerExtension` | 19 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `network.payload-context`; overriding hooks is a gap (documented in `types`) | planned |
| `IFriendlyByteBufExtension` | 13 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `network` payload bytes; overriding hooks is a gap (documented in `types`) | planned |
| `IBlockGetterExtension` | 9 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `pumpkin:plugin/world`; overriding hooks is a gap (documented in `types`) | planned |
| `IBucketPickupExtension` | 7 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `registration.item-definition.behaviour`; overriding hooks is a gap (documented in `types`) | planned |
| `IDispensibleContainerItemExtension` | 6 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `registration.item-definition.behaviour`; overriding hooks is a gap (documented in `types`) | planned |
| `IServerConfigurationPacketListenerExtension` | 6 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `network.configuration-sender`; overriding hooks is a gap (documented in `types`) | planned |
| `IAttributeExtension` | 5 | Extension interfaces NeoForge adds to `net.minecraft` | `types` doc (extension interfaces), not supported | not supported |
| `IBaseRailBlockExtension` | 4 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `registration.block-definition.block-type`; overriding hooks is a gap (documented in `types`) | planned |
| `IOwnedSpawner` | 3 | Extension interfaces NeoForge adds to `net.minecraft` | `types` doc (extension interfaces), not supported | not supported |
| `ILivingEntityExtension` | 2 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `pumpkin:plugin/world` `living-entity`, `mob-effects` and `entity-attributes`; overriding hooks is a gap (documented in `types`) | planned |
| `IMobEffectExtension` | 1 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `registration.mob-effect-definition`; overriding hooks is a gap (documented in `types`) | planned |
| `ITagBuilderExtension` | 1 | Extension interfaces NeoForge adds to `net.minecraft` | `types` doc (extension interfaces), not supported | not supported |
| `ITransformationExtension` | 1 | Extension interfaces NeoForge adds to `net.minecraft` | `types` doc (extension interfaces), not supported | not supported |
| `TooltipFlagExtension` | 1 | Extension interfaces NeoForge adds to `net.minecraft` | `types` doc (extension interfaces), not supported | not supported |
| `BootstrapContextAccessExtension` | 0 | Extension interfaces NeoForge adds to `net.minecraft` | `types` doc (extension interfaces), not supported | not supported |
| `BootstrapContextExtension` | 0 | Extension interfaces NeoForge adds to `net.minecraft` | `types` doc (extension interfaces), not supported | not supported |
| `ContainerExtension` | 0 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `pumpkin:plugin/inventory`; overriding hooks is a gap (documented in `types`) | planned |
| `GameTestHelperExtension` | 0 | Extension interfaces NeoForge adds to `net.minecraft` | `types` doc (extension interfaces), not supported | not supported |
| `IAbstractBoatExtension` | 0 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `pumpkin:plugin/world` `entity`; overriding hooks is a gap (documented in `types`) | planned |
| `IAdvancementBuilderExtension` | 0 | Extension interfaces NeoForge adds to `net.minecraft` | `types` doc (extension interfaces), not supported | not supported |
| `ICommandSourceExtension` | 0 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `pumpkin:plugin/command` `command-sender`; overriding hooks is a gap (documented in `types`) | planned |
| `IEnchantmentExtension` | 0 | Extension interfaces NeoForge adds to `net.minecraft` | `types` doc (extension interfaces), not supported | not supported |
| `IFallableExtension` | 0 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `registration.block-definition.block-type`; overriding hooks is a gap (documented in `types`) | planned |
| `IHolderSetExtension` | 0 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `registration.tag-entries`; overriding hooks is a gap (documented in `types`) | planned |
| `IHolderSetExtension.SerializationType` | 0 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `registration.tag-entries`; overriding hooks is a gap (documented in `types`) | planned |
| `IMenuProviderExtension` | 0 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `menus.menu-definition` (`writeClientSideData` is `extra-data`); overriding hooks is a gap (documented in `types`) | planned |
| `IPlayerListExtension` | 0 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `pumpkin:plugin/server`; overriding hooks is a gap (documented in `types`) | planned |
| `IRecipeOutputExtension` | 0 | Extension interfaces NeoForge adds to `net.minecraft` | `types` doc (extension interfaces), not supported | not supported |
| `IServerChunkCacheExtension` | 0 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `pumpkin:plugin/world` `chunk`; overriding hooks is a gap (documented in `types`) | planned |
| `IServerGamePacketListenerExtension` | 0 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `network.payload-context`; overriding hooks is a gap (documented in `types`) | planned |
| `ITagAppenderExtension` | 0 | Extension interfaces NeoForge adds to `net.minecraft` | `types` doc (extension interfaces), not supported | not supported |
| `ItemInstanceExtension` | 0 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `pumpkin:plugin/item-stack`; overriding hooks is a gap (documented in `types`) | planned |
| `LootTableSubProviderContextExtension` | 0 | Extension interfaces NeoForge adds to `net.minecraft` | `types` doc (extension interfaces), not supported | not supported |
| `PackMetadataResourcesExtension` | 0 | Extension interfaces NeoForge adds to `net.minecraft` | `types` doc (extension interfaces), not supported | not supported |
| `PendingTagsExtension` | 0 | Extension interfaces NeoForge adds to `net.minecraft` | `types` doc (extension interfaces), not supported | not supported |
| `TypedInstanceExtension` | 0 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `registration.deferred-holder`; overriding hooks is a gap (documented in `types`) | planned |
| `ValueInputExtension` | 0 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `pumpkin:plugin/common` `nbt-tree`; overriding hooks is a gap (documented in `types`) | planned |
| `ValueOutputExtension` | 0 | Extension interfaces NeoForge adds to `net.minecraft` | partial: calls through `pumpkin:plugin/common` `nbt-tree`; overriding hooks is a gap (documented in `types`) | planned |

### Other packages

| NeoForge item | Mods | Section | Counterpart | Status |
|:--|--:|:--|:--|:--|
| `NeoForge` | 219 | Types in the root `common` package | `event-bus.bus::game` | planned |
| `transfer` | 189 | Packages outside the families above | `capabilities.capability-handle` | planned |
| `ModConfigSpec` | 182 | Types in the root `common` package | `mod-config.mod-config-spec` | planned |
| `transfer.item` | 149 | Packages outside the families above | `capabilities.handler-type::item-resource-handler`, `types.item-resource` | planned |
| `common.util` | 133 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `transfer.fluid` | 130 | Packages outside the families above | `capabilities.handler-type::fluid-resource-handler`, `types.fluid-resource` | planned |
| `fluids` | 122 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `transfer.energy` | 78 | Packages outside the families above | `capabilities.handler-type::energy-handler` | planned |
| `Tags` | 77 | Types in the root `common` package | `registration.tag-entries`, `is-in-tag` (tag ids) | planned |
| `server` | 59 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `Tags.Items` | 51 | Types in the root `common` package | `registration.tag-entries`, `is-in-tag` (tag ids) | planned |
| `ItemAbility` | 45 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `common.crafting` | 43 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `CommonHooks` | 42 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `ItemAbilities` | 41 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `NeoForgeMod` | 30 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `Tags.Blocks` | 30 | Types in the root `common` package | `registration.tag-entries`, `is-in-tag` (tag ids) | planned |
| `fluids.crafting` | 28 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `SoundAction` | 23 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `common.world` | 22 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `entity` | 20 | Packages outside the families above | gap: `IEntityWithComplexSpawn` spawn data needs the M3 `advanced_add_entity` payload; `PartEntity` has no target (documented in `types`) | planned |
| `MutableDataComponentHolder` | 20 | Types in the root `common` package | `pumpkin:plugin/item-stack` component methods; `types.component-value` | planned |
| `SoundActions` | 20 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `Tags.EntityTypes` | 20 | Types in the root `common` package | `registration.tag-entries`, `is-in-tag` (tag ids) | supported |
| `common.damagesource` | 18 | Packages outside the families above | `types.damage-container`, `damage-reduction`, `event-bus.reduction-modifier`, `callbacks.apply-reduction-modifier` | planned |
| `common.world.chunk` | 13 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `Tags.Fluids` | 12 | Types in the root `common` package | `registration.tag-entries`, `is-in-tag` (tag ids) | supported |
| `IShearable` | 11 | Types in the root `common` package | gap: shear behaviour hook; documented in `types` (extension interfaces) | planned |
| `Tags.DamageTypes` | 11 | Types in the root `common` package | `registration.tag-entries`, `is-in-tag` (tag ids) | planned |
| `UsernameCache` | 10 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `server.command` | 9 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `resource` | 8 | Packages outside the families above | `lifecycle.add-server-reload-listeners-event`, `callbacks.on-server-reload` | planned |
| `SpecialPlantable` | 8 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `server.permission` | 7 | Packages outside the families above | `pumpkin:plugin/player.has-permission` (`PermissionAPI.getPermission`) | supported |
| `server.permission.nodes` | 7 | Packages outside the families above | `pumpkin:plugin/permission.permission` via `event-bus` `permission-gather-event-nodes` | planned |
| `PercentageAttribute` | 7 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `Tags.Biomes` | 4 | Types in the root `common` package | `registration.tag-entries`, `is-in-tag` (tag ids) | planned |
| `BooleanAttribute` | 3 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `CreativeModeTabRegistry` | 3 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `TranslatableEnum` | 3 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `common.world.poi` | 2 | Packages outside the families above | `registration.extend-poi-types-event` | planned |
| `FarmlandWaterManager` | 2 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `common.enums` | 1 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `common.ticket` | 1 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `internal` | 1 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `IOUtilities` | 1 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `LenientUnboundedMapCodec` | 1 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `common.advancements.critereon` | 0 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
| `common.command` | 0 | Packages outside the families above | `types` doc (other packages), not supported | not supported |
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
| `transfer.access` | 0 | Packages outside the families above | `capabilities.item-access` | planned |
| `transfer.resource` | 0 | Packages outside the families above | `types.item-resource`, `fluid-resource`, `capabilities.transfer-resource` | planned |
| `transfer.transaction` | 0 | Packages outside the families above | `capabilities.transaction`, `callbacks.transaction-closed` | planned |
| `world.inventory` | 0 | Packages outside the families above | gap: custom menu slots are M5 (`registration.menu-type-definition`) | planned |
| `CommonHooks.BiomeCallbackFunction` | 0 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `DataMapHooks` | 0 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `IMinecartCollisionHandler` | 0 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `IOUtilities.WriteCallback` | 0 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `MonsterRoomHooks` | 0 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `NeoForgeBuildType` | 0 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `NeoForgeEventHandler` | 0 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `OnlyInWarningsHandler` | 0 | Types in the root `common` package | `types` doc (other packages), not supported | not supported |
| `Tags.Enchantments` | 0 | Types in the root `common` package | `registration.tag-entries`, `is-in-tag` (tag ids) | supported |
| `Tags.Potions` | 0 | Types in the root `common` package | `registration.tag-entries`, `is-in-tag` (tag ids) | supported |
| `Tags.Structures` | 0 | Types in the root `common` package | `registration.tag-entries`, `is-in-tag` (tag ids) | supported |
| `Tags.WorldClocks` | 0 | Types in the root `common` package | `registration.tag-entries`, `is-in-tag` (tag ids) | planned |

## Modpack coverage

The reference modpack has 414 mods. 405 of them need at least one gap or contract-only row of the contract as the scan found it (section 11.1 of `modpack-usage.md`). This section gives the verdict for each of the 30 largest gaps, the gaps that are out of scope by nature, and the classes outside the inventory with their counterpart.

### Top 30 gaps and verdicts

Gaps as section 11.2 of the scan groups them, by the number of mods that need at least one row of the group. A verdict is one of: add now (the API addition of this change), plan (the task and phase), or out of scope (the reason).

| # | Gap | Mods | Largest rows (mods) | Verdict |
|--:|:--|--:|:--|:--|
| 1 | Java helpers | 312 | ChatFormatting(248), RandomSource(200), Mth(191), StringRepresentable(127) | out of scope: mod code; Rust std and `ironpumpkin-neo` replace `Mth`, `RandomSource` and `StringRepresentable`, and `ChatFormatting` is the style of the v0.2 `text-component` (`types` doc) |
| 2 | Mixins | 291 | [[mixins]] config(291), file mixins(278), [[mixins]] requiredMods(2), [[mixins]] behaviorVersion(0) | plan, native channel: server mixins map to build-time accessors (task 2.1.3), `#[hook]` hook points (task 2.1.2) and service seams (task 2.1.4), and `@Unique` state to attachments (task 2.14); #51 sizes the catalogue, and it grows per request. Client mixins and mixins that change data structures or data formats: out of scope by nature (below) |
| 3 | DFU codecs | 278 | Codec(244), MapCodec(220), PrimitiveCodec(196), Products(195) | out of scope by nature (below) |
| 4 | Stream codecs and byte buffers | 263 | StreamCodec(252), RegistryFriendlyByteBuf(226), ByteBufCodecs(204) | out of scope by nature (below): a payload is the bytes the mod encodes |
| 5 | Registries the host rejects | 247 | DataComponentType(232), data_component_type(113), creative_mode_tab(112), sound_event(55) | add now: `data_component_type`, `sound_event`, `mob_effect` and `attribute` become planned definitions with the new interfaces `data-components`, `mob-effects` and `entity-attributes` (task 2.6, M4); `creative_mode_tab` stays accepted and ignored (client UI); `fluid` and `neoforge:fluid_type`: plan, M5, no task yet (custom fluids) |
| 6 | Access transformers | 183 | file accessTransformers(55), [[accessTransformers]] file(55) | plan, native channel: build-time accessors (task 2.1.3) give a mod each member that its access transformer widens, one catalogue entry per member |
| 7 | Container menus | 176 | AbstractContainerMenu(172), MenuProvider(121), MenuConstructor(51), SimpleMenuProvider(45) | add now: interface `menus` (resource `menu`, `open-menu`, `get-open-menu`) and the callbacks `menu-clicked`, `menu-quick-move-stack`, `menu-still-valid`, `menu-click-button`, `menu-slots-changed`, `menu-removed`, `menu-slot-may-place`, `menu-slot-on-take` (task 2.9; vanilla menu types in M4, mod menu types in M5) |
| 8 | Block shapes | 170 | VoxelShape(167), CollisionContext(134) | add now: interface `shapes` (`get-block-shape`, `get-state-shape`, `is-face-sturdy`, `no-collision`) and the `block-properties` methods `shape` and `collision-shape` (task 2.10, M4 over the M2 content registry) |
| 9 | Ingredients and trades | 146 | Ingredient(141), ItemCost(17), Merchant(6) | add now as a mapping (`types` doc): `Ingredient` is the v0.2 `recipe.ingredient` of `recipe-manager`. Trades: out of scope as code: 26.3 removed the trade events and keeps trades as `minecraft:villager_trade` datapack entries that a port ships as JSON; `trade-with-villager-event` (task 2.12) is the code hook |
| 10 | Package common.util | 133 | common.util(133), common.util.flag(0), common.util.strategy(0) | out of scope: Java helpers (`Lazy`, `INBTSerializable`, which is `nbt-tree` data); `TriState` and `BlockSnapshot` already map to `types.tri-state` and `types.block-snapshot`; `FakePlayer` below |
| 11 | Packages fluids | 123 | fluids(122), fluids.crafting(28), fluids.crafting.display(0) | plan: `FluidStack` and fluid handlers through the transfer handlers of task 2.7 (`fluid-resource` and an amount); custom fluids and `FluidUtil` interactions in M5, no task yet |
| 12 | Static registries without API | 118 | recipe_serializer(74), recipe_type(49), particle_type(41), command_argument_type(21) | plan, M5, no task yet: `recipe_type` and `recipe_serializer` (mod machine recipes as datapack JSON). Worldgen types: plan with the world generation service seam (task 2.1.4), as for the world generation gap. Out of scope: `particle_type` and `command_argument_type` (client-synced registries without a Pumpkin registry type), loot and trigger types |
| 13 | Creative tab contents | 84 | BuildCreativeModeTabContentsEvent(75), CreativeModeTab.TabVisibility(26) | out of scope: client UI; the host accepts the tab entry and ignores it |
| 14 | RenderShape | 81 | RenderShape(81) | out of scope by nature (client rendering) |
| 15 | World generation | 72 | Heightmap(58), Heightmap.Types(58), Feature(24), GenerationStep(21) | plan, native channel: a native mod adds features, placements and structures in Rust through the world generation service seam (task 2.1.4); `Heightmap` reads map to the v0.2 `get-top-block-y` and `get-motion-blocking-height`; which NeoForge world generation rows the seam backs, biome modifiers among them, waits for the owner decision on ores (status issue) |
| 16 | FakePlayer | 63 | FakePlayer(63) | out of scope: Pumpkin has no fake players; a mod acts through `level-access` with a `cause` entity, and the one event pipeline (tasks 2.4 and 2.18) fires the events a fake player would trigger |
| 17 | Package server | 63 | server(59), server.command(9), server.command.generation(0), server.console(0) | add now as a mapping (`types` doc): `ServerLifecycleHooks#getCurrentServer` is the v0.2 `context.get-server`; the console, timings and command generation packages: out of scope (internal) |
| 18 | Custom game rules | 53 | GameRules(53) | plan, M5, no task yet: needs a `game_rule` registry, and Pumpkin game rules are a generated enum; reading vanilla rules works through the v0.2 `game-rules` |
| 19 | Data conditions | 52 | ICondition(42), ICondition.IContext(32), ConditionalOps(12), WithConditions(8) | plan, M5, no task yet: the datapack loader evaluates the built-in `neoforge:conditions` (mod loaded, item exists, tag empty, and, or, not); custom condition types stay out (`neoforge:condition_codecs`) |
| 20 | Custom registries | 48 | RegistryBuilder(47), NewRegistryEvent(31), DeferredRegister#makeRegistry(12), ModifyRegistriesEvent(2) | out of scope: new registries need mutable registry types, and Pumpkin registries are generated; a mod keeps its own lookup tables in its own code |
| 21 | Item abilities | 45 | ItemAbility(45), ItemAbilities(41) | out of scope: Pumpkin item behaviours hard-code tool actions; the ability name reaches mods on `block-event-block-tool-modification-event` |
| 22 | Package common.crafting | 43 | common.crafting(43) | out of scope: custom ingredient types need the `neoforge:ingredient_serializer` registry; plain ingredients are the v0.2 `recipe.ingredient`, and `SizedIngredient` is an ingredient and a count in mod code |
| 23 | CommonHooks | 42 | CommonHooks(42) | out of scope: helpers that fire events by hand; under the one event pipeline (tasks 2.4 and 2.18) the host fires them for every mutation that a player or an entity causes, and `event-bus.post` covers the rest |
| 24 | Packages common.world | 34 | common.world(22), common.world.chunk(13) | out of scope: biome modifiers (world generation, owner decision) and forced chunk tickets (`RegisterTicketControllersEvent`; pumpkin-world does not expose tickets) |
| 25 | Package network.codec | 34 | network.codec(34) | out of scope by nature (stream codec helpers; payloads are bytes) |
| 26 | NeoForge registries without API | 32 | neoforge:condition_codecs(16), neoforge:biome_modifier_serializers(10), neoforge:entity_data_serializers(9), neoforge:structure_modifier_serializers(1) | out of scope: codec and serializer registries for Java types (`condition_codecs`, `biome_modifier_serializers`, `entity_data_serializers`, `structure_modifier_serializers`, `holder_set_type`, `ingredient_serializer`) |
| 27 | ModList file data | 30 | ModList#getModFileById(16), ModList#getAllScanData(16), ModList#getModFiles(3), ModList#forEachModFile(2) | out of scope: mod files and scan data describe Java jars, and a Rust mod has none; the `lifecycle` mod list gives the mod metadata |
| 28 | NeoForgeMod constants | 30 | NeoForgeMod(30) | plan, task 2.6: the ids are constants of `ironpumpkin-neo`, and the host registers `neoforge:swim_speed` and `neoforge:creative_flight` in its attribute storage; the milk fluid follows custom fluids (M5, no task yet) |
| 29 | Tooltip events | 28 | ItemTooltipEvent(28), AddAttributeTooltipsEvent(0), GatherSkippedAttributeTooltipsEvent(0) | out of scope by nature (client rendering: the client builds tooltips) |
| 30 | ArmorMaterial.Layer | 24 | ArmorMaterial.Layer(24) | out of scope by nature (client rendering: equipment layers are a client asset; armor is the `minecraft:equippable` component) |

Gaps ranked 31 to 60 in section 11.2 of the scan need 3 to 23 mods each. They keep the reason of their row in the tables of this file; the trade events among them (`VillagerTradesEvent`, `WandererTradesEvent`, 15 mods) follow the trade verdict of the ingredients and trades gap.

### Out of scope by nature

- Client mixins: a dedicated server never runs client code.
- Mixins that change data structures or data formats (packet fields, the chunk or save format, registry types, DataFixerUpper schemas): they need a change to Pumpkin itself, decided per case. Server mixins that hook, read or replace behaviour map to the native primitives (the mixins gap above).
- DataFixerUpper codecs, stream codecs and byte buffers: a Java serialization framework. Mods serialize with Rust code, and data crosses the API as JSON, NBT or bytes.
- Client rendering (render shapes, tooltips, creative tab contents, equipment layers, screens): the client draws it, and a dedicated server never runs that code.

### Classes outside the inventory

Minecraft, NeoForge and Mojang classes that ported mods use and that the inventory does not list as a row, by family. `26.3` is `same` when a class with this name exists in 26.3 and `CHANGED` when it does not; the counterpart follows the 26.3 shape.

#### NeoForge registries

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.neoforged.neoforge.registries.DeferredHolder` | 224 | same | `registration.deferred-holder` |
| `net.neoforged.neoforge.registries.DeferredRegister` | 207 | same | `registration.deferred-register` |
| `net.neoforged.neoforge.registries.NeoForgeRegistries` | 111 | same | `registration` registry ids (`neoforge:*`) |
| `net.neoforged.neoforge.registries.DeferredItem` | 104 | same | `registration.deferred-item` |
| `net.neoforged.neoforge.registries.DeferredRegister$Items` | 86 | same | `registration.deferred-register-items` |
| `net.neoforged.neoforge.registries.DeferredBlock` | 78 | same | `registration.deferred-block` |
| `net.neoforged.neoforge.registries.NeoForgeRegistries$Keys` | 77 | same | `registration` registry ids (`neoforge:*`) |
| `net.neoforged.neoforge.registries.DeferredRegister$Blocks` | 67 | same | `registration.deferred-register-blocks` |
| `net.neoforged.neoforge.registries.DeferredRegister$DataComponents` | 47 | same | `registration.deferred-register-data-components` (planned, task 2.6) |

#### NeoForge events and event bus

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.neoforged.bus.api.IEventBus` | 342 | same | `event-bus.add-listener`, `event-bus.post` |
| `net.neoforged.bus.api.SubscribeEvent` | 207 | same | `event-bus.add-listener` |
| `net.neoforged.neoforge.network.event.RegisterPayloadHandlersEvent` | 170 | same | `event-bus.event::register-payload-handlers-event` |
| `net.neoforged.fml.event.lifecycle.FMLCommonSetupEvent` | 148 | same | `event-bus.event::fml-common-setup-event` |
| `net.neoforged.fml.common.EventBusSubscriber` | 126 | same | `event-bus.add-listener` |
| `net.neoforged.neoforge.event.entity.player.PlayerEvent` | 96 | same | fields in the subclass records of `event-bus` (abstract) |
| `net.neoforged.neoforge.event.RegisterCommandsEvent` | 93 | same | `event-bus.event::register-commands-event` |
| `net.neoforged.neoforge.event.entity.player.PlayerInteractEvent` | 85 | same | fields in the subclass records of `event-bus` (abstract) |
| `net.neoforged.neoforge.event.BuildCreativeModeTabContentsEvent` | 75 | same | gap: not supported (client UI); documented in `event-bus` |
| `net.neoforged.fml.common.EventBusSubscriber$Bus` | 69 | same | `event-bus.bus` |
| `net.neoforged.neoforge.event.entity.player.PlayerEvent$PlayerLoggedInEvent` | 66 | same | `event-bus.event::player-event-player-logged-in-event` |
| `net.neoforged.neoforge.event.level.BlockEvent` | 65 | same | fields in the subclass records of `event-bus` (abstract) |
| `net.neoforged.neoforge.event.EventHooks` | 51 | same | `event-bus.post` |
| `net.neoforged.neoforge.event.entity.EntityJoinLevelEvent` | 49 | same | `event-bus.event::entity-join-level-event` |
| `net.neoforged.neoforge.event.server.ServerStartedEvent` | 40 | same | `event-bus.event::server-started-event` |
| `net.neoforged.neoforge.event.entity.living.LivingDamageEvent` | 38 | same | fields in the subclass records of `event-bus` (abstract) |
| `net.neoforged.neoforge.event.entity.living.LivingIncomingDamageEvent` | 37 | same | `event-bus.event::living-incoming-damage-event` |
| `net.neoforged.neoforge.event.tick.PlayerTickEvent` | 36 | same | fields in the subclass records of `event-bus` (abstract) |
| `net.neoforged.neoforge.event.entity.living.LivingDropsEvent` | 31 | same | `event-bus.event::living-drops-event` |
| `net.neoforged.neoforge.event.entity.EntityAttributeCreationEvent` | 29 | same | `event-bus.event::entity-attribute-creation-event` |
| `net.neoforged.neoforge.event.entity.player.PlayerEvent$PlayerLoggedOutEvent` | 28 | same | `event-bus.event::player-event-player-logged-out-event` |
| `net.neoforged.neoforge.event.entity.player.PlayerInteractEvent$LeftClickBlock` | 27 | same | `event-bus.event::player-interact-event-left-click-block` |
| `net.neoforged.neoforge.event.entity.player.PlayerEvent$PlayerChangedDimensionEvent` | 26 | same | `event-bus.event::player-event-player-changed-dimension-event` |
| `net.neoforged.neoforge.event.entity.living.LivingDamageEvent$Post` | 25 | same | `event-bus.event::living-damage-event-post` |
| `net.neoforged.neoforge.event.entity.player.PlayerEvent$PlayerRespawnEvent` | 24 | same | `event-bus.event::player-event-player-respawn-event` |
| `net.neoforged.neoforge.event.tick.PlayerTickEvent$Post` | 24 | same | `event-bus.event::player-tick-event-post` |
| `net.neoforged.neoforge.event.level.BlockEvent$EntityPlaceEvent` | 23 | same | `event-bus.event::block-event-entity-place-event` |
| `net.neoforged.neoforge.event.entity.player.PlayerEvent$Clone` | 23 | same | `event-bus.event::player-event-clone` |
| `net.neoforged.neoforge.event.entity.player.PlayerEvent$BreakSpeed` | 18 | same | `event-bus.event::player-event-break-speed` |
| `net.neoforged.neoforge.event.level.BlockDropsEvent` | 17 | same | `event-bus.event::block-drops-event` |
| `net.neoforged.neoforge.event.entity.EntityAttributeModificationEvent` | 16 | same | `event-bus.event::entity-attribute-modification-event` |
| `net.neoforged.neoforge.event.entity.living.FinalizeSpawnEvent` | 16 | same | `event-bus.event::finalize-spawn-event` |
| `net.neoforged.neoforge.event.entity.living.LivingEntityUseItemEvent` | 16 | same | `event-bus.event::living-entity-use-item-event` |
| `net.neoforged.neoforge.event.entity.player.AttackEntityEvent` | 15 | same | `event-bus.event::attack-entity-event` |
| `net.neoforged.neoforge.event.entity.living.LivingExperienceDropEvent` | 13 | same | `event-bus.event::living-experience-drop-event` |
| `net.neoforged.neoforge.event.AddPackFindersEvent` | 13 | same | `event-bus.event::add-pack-finders-event` |
| `net.neoforged.neoforge.event.entity.living.MobEffectEvent` | 13 | same | fields in the subclass records of `event-bus` (abstract) |
| `net.neoforged.neoforge.event.entity.player.PlayerEvent$StartTracking` | 13 | same | `event-bus.event::player-event-start-tracking` |
| `net.neoforged.neoforge.event.entity.ProjectileImpactEvent` | 12 | same | `event-bus.event::projectile-impact-event` |
| `net.neoforged.neoforge.event.village.VillagerTradesEvent` | 12 | CHANGED | gap: removed in NeoForge 26.1 (villager trades are datapack data); documented in `event-bus` |
| `net.neoforged.neoforge.event.AnvilUpdateEvent` | 9 | same | `event-bus.event::anvil-update-event` |
| `net.neoforged.neoforge.event.entity.living.LivingEntityUseItemEvent$Start` | 9 | same | `event-bus.event::living-entity-use-item-event-start` |
| `net.neoforged.neoforge.event.entity.living.LivingKnockBackEvent` | 9 | same | `event-bus.event::living-knock-back-event` |
| `net.neoforged.neoforge.event.village.WandererTradesEvent` | 9 | CHANGED | gap: removed in NeoForge 26.1 (wandering trader trades are datapack data); documented in `event-bus` |
| `net.neoforged.neoforge.event.entity.living.LivingShieldBlockEvent` | 8 | same | `event-bus.event::living-shield-block-event` |
| `net.neoforged.neoforge.event.entity.living.MobEffectEvent$Applicable` | 7 | same | `event-bus.event::mob-effect-event-applicable` |
| `net.neoforged.neoforge.event.entity.living.MobEffectEvent$Applicable$Result` | 7 | same | `event-bus.mob-effect-applicable-result` |
| `net.neoforged.neoforge.network.event.RegisterConfigurationTasksEvent` | 7 | same | `event-bus.event::register-configuration-tasks-event` |
| `net.neoforged.neoforge.event.entity.EntityTravelToDimensionEvent` | 6 | same | `event-bus.event::entity-travel-to-dimension-event` |
| `net.neoforged.neoforge.event.entity.player.CriticalHitEvent` | 5 | same | `event-bus.event::critical-hit-event` |

#### NeoForge mod loading and config

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.neoforged.fml.common.Mod` | 340 | same | mod registration (`register_mod!`), the mod's `init` and its embedded `neoforge.mods.toml` |
| `net.neoforged.fml.ModContainer` | 246 | same | `lifecycle.mod-container`, `mod-config.register-config` |
| `net.neoforged.api.distmarker.Dist` | 199 | same | `types.dist` |
| `net.neoforged.neoforge.common.ModConfigSpec` | 182 | same | `mod-config.mod-config-spec` |
| `net.neoforged.neoforge.common.ModConfigSpec$Builder` | 178 | same | `mod-config.mod-config-spec`, `value-spec` |
| `net.neoforged.fml.config.ModConfig` | 165 | same | `mod-config.mod-config` |
| `net.neoforged.fml.config.ModConfig$Type` | 165 | same | `mod-config.config-type` (`SERVER` is gone in FML 12: use `synced` or `local`) |
| `net.neoforged.fml.config.IConfigSpec` | 162 | same | `mod-config.mod-config-spec` |
| `net.neoforged.neoforge.common.ModConfigSpec$ConfigValue` | 104 | same | `mod-config.mod-config.get`, `set`, `save` |
| `net.neoforged.fml.loading.FMLLoader` | 64 | same | `lifecycle.is-production`, `lifecycle.get-dist` |

#### NeoForge attachments

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.neoforged.neoforge.attachment.AttachmentType` | 68 | same | `attachments.attachment-type-definition`, `registration.entry-definition::attachment-type` |
| `net.neoforged.neoforge.attachment.AttachmentType$Builder` | 57 | same | `attachments.attachment-type-definition` |

#### NeoForge networking

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.neoforged.neoforge.network.handling.IPayloadContext` | 183 | same | `network.payload-context` |
| `net.neoforged.neoforge.network.registration.PayloadRegistrar` | 167 | same | `network.payload-registrar` |
| `net.neoforged.neoforge.network.handling.IPayloadHandler` | 166 | same | `callbacks.handle-payload` |
| `net.neoforged.neoforge.network.PacketDistributor` | 157 | same | `network.send-to-*` |
| `net.neoforged.neoforge.network.IContainerFactory` | 84 | same | `menus.menu-definition.extra-data` |
| `net.neoforged.neoforge.network.configuration.ICustomConfigurationTask` | 5 | same | `network.configuration-task`, `callbacks.run-configuration-task` |

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
| `net.neoforged.neoforge.common.Tags` | 77 | same | `registration.tag-entries`, `is-in-tag` |
| `net.neoforged.neoforge.common.extensions.IMenuTypeExtension` | 70 | same | `registration.menu-type-definition.extra-data`, `menus.menu-definition.extra-data` |
| `net.neoforged.neoforge.common.util.FakePlayer` | 63 | same | gap: not supported (Pumpkin has no fake players); documented in `types` |
| `net.neoforged.neoforge.common.Tags$Items` | 51 | same | `registration.tag-entries`, `is-in-tag` |
| `net.neoforged.neoforge.common.ItemAbility` | 45 | same | gap: not supported; `item-ability` name on `block-event-block-tool-modification-event` |
| `net.neoforged.neoforge.common.ItemAbilities` | 41 | same | gap: not supported (item abilities); documented in `types` |
| `net.neoforged.neoforge.common.Tags$Blocks` | 30 | same | `registration.tag-entries`, `is-in-tag` |
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
| `net.neoforged.neoforge.common.loot.IGlobalLootModifier` | 29 | same | `callbacks.decode-loot-modifier`, `apply-loot-modifier` |
| `net.neoforged.neoforge.common.loot.LootModifier` | 25 | same | `loot.loot-modifier-entry` |
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
| `net.minecraft.core.registries.Registries` | 265 | same | `registration` lookup functions (`registry-*`, `tag-entries`) |
| `net.minecraft.core.Holder` | 258 | same | `types.identifier`; `registration.deferred-holder` |
| `net.minecraft.core.registries.BuiltInRegistries` | 257 | same | `registration` lookup functions (`registry-*`, `tag-entries`) |
| `net.minecraft.core.Direction` | 249 | same | `types.direction` |
| `net.minecraft.core.HolderLookup` | 245 | same | `registration` lookup functions (`registry-*`, `tag-entries`) |
| `net.minecraft.core.HolderLookup$Provider` | 239 | same | `registration` lookup functions (`registry-*`, `tag-entries`) |
| `net.minecraft.core.DefaultedRegistry` | 239 | same | `registration` lookup functions (`registry-*`, `tag-entries`) |
| `net.minecraft.core.Registry` | 239 | same | `registration` lookup functions (`registry-*`, `tag-entries`) |
| `net.minecraft.tags.TagKey` | 232 | same | `types.tag-key` |
| `net.minecraft.core.RegistryAccess` | 198 | same | `registration` lookup functions (`registry-*`, `tag-entries`) |
| `net.minecraft.core.Holder$Reference` | 177 | same | `registration.deferred-holder` |
| `net.minecraft.core.NonNullList` | 172 | same | a list of `item-stack` (`list<item-stack>`) |
| `net.minecraft.core.particles.ParticleOptions` | 133 | same | `pumpkin:plugin/particles` |
| `net.minecraft.core.particles.SimpleParticleType` | 114 | same | `pumpkin:plugin/particles` |
| `net.minecraft.core.particles.ParticleTypes` | 113 | same | `pumpkin:plugin/particles` |
| `net.minecraft.tags.BlockTags` | 96 | same | `registration.is-in-tag` (tag ids) |
| `net.minecraft.core.HolderSet` | 94 | same | `registration` lookup functions (`registry-*`, `tag-entries`) |
| `net.minecraft.core.HolderLookup$RegistryLookup` | 81 | same | `registration` lookup functions (`registry-*`, `tag-entries`) |
| `net.minecraft.core.Position` | 75 | same | `pumpkin:plugin/common` (`position`) |
| `net.minecraft.tags.ItemTags` | 74 | same | `registration.is-in-tag` (tag ids) |
| `net.minecraft.core.BlockPos$MutableBlockPos` | 67 | same | `pumpkin:plugin/common` (`block-pos`) |
| `net.minecraft.core.HolderSet$Named` | 63 | same | `registration` lookup functions (`registry-*`, `tag-entries`) |
| `net.minecraft.core.HolderGetter` | 46 | same | `registration` lookup functions (`registry-*`, `tag-entries`) |
| `net.minecraft.tags.DamageTypeTags` | 34 | same | `registration.is-in-tag` (tag ids) |
| `net.minecraft.core.HolderSet$Direct` | 32 | same | `registration` lookup functions (`registry-*`, `tag-entries`) |
| `net.minecraft.tags.BiomeTags` | 9 | same | `registration.is-in-tag` (tag ids) |

#### Minecraft data components

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.minecraft.core.component.DataComponentType` | 232 | same | `registration.data-component-type-definition` (planned, task 2.6); values through `data-components` |
| `net.minecraft.core.component.DataComponents` | 153 | same | `types.component-value`; `data-components` by id; `pumpkin:plugin/item-stack` components |
| `net.minecraft.core.component.DataComponentType$Builder` | 148 | same | `registration.data-component-type-definition` (planned, task 2.6) |
| `net.minecraft.core.component.DataComponentMap` | 103 | same | `types.component-value`; `data-components.get-components-patch` |
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
| `net.minecraft.sounds.SoundEvent` | 190 | same | `registration.entry-definition::sound-event` (planned, task 2.6); played by id with the v0.2 `play-custom-sound` |
| `net.minecraft.sounds.SoundSource` | 165 | same | `types.sound-source` |
| `net.minecraft.sounds.SoundEvents` | 162 | same | `types.identifier` (sound ids); `pumpkin:plugin/sounds` |

#### Minecraft items

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.minecraft.world.item.ItemStack` | 327 | same | `pumpkin:plugin/item-stack`; `registration.item-definition` |
| `net.minecraft.world.item.Item` | 312 | same | `pumpkin:plugin/item-stack`; `registration.item-definition` |
| `net.minecraft.world.item.Item$Properties` | 217 | same | `registration.item-properties` |
| `net.minecraft.world.item.CreativeModeTab` | 183 | same | `registration.creative-mode-tab-definition` (ignored on the server) |
| `net.minecraft.world.item.Items` | 180 | same | `pumpkin:plugin/item-stack`; `registration.item-definition` |
| `net.minecraft.world.item.BlockItem` | 179 | same | `registration.item-definition.block` |
| `net.minecraft.world.item.CreativeModeTab$Builder` | 142 | same | `registration.creative-mode-tab-definition` |
| `net.minecraft.world.item.CreativeModeTab$Output` | 131 | same | `registration.creative-mode-tab-definition.items` |
| `net.minecraft.world.item.CreativeModeTab$ItemDisplayParameters` | 127 | same | `registration.creative-mode-tab-definition.items` |
| `net.minecraft.world.item.CreativeModeTab$DisplayItemsGenerator` | 121 | same | `registration.creative-mode-tab-definition.items` |
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
| `net.minecraft.world.item.crafting.Ingredient` | 141 | same | `pumpkin:plugin/recipe` `ingredient`; tests with the item id and `registration.is-in-tag` (documented in `types`) |
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
| `net.minecraft.world.entity.ai.attributes.Attributes` | 79 | same | `pumpkin:plugin/attributes`; `types.attribute-modifier`, `attribute-value` |
| `net.minecraft.world.entity.ai.attributes.AttributeModifier` | 73 | same | `types.attribute-modifier` |
| `net.minecraft.world.entity.ai.attributes.AttributeModifier$Operation` | 65 | same | `types.attribute-operation` |
| `net.minecraft.world.entity.ai.attributes.AttributeInstance` | 62 | same | `entity-attributes`; `pumpkin:plugin/attributes` for vanilla attributes; `types.attribute-modifier` |
| `net.minecraft.world.entity.ai.attributes.Attribute` | 56 | same | `pumpkin:plugin/attributes` (vanilla); `entity-attributes` by id; `registration.entry-definition::attribute` (planned, task 2.6) |
| `net.minecraft.world.entity.ai.attributes.AttributeSupplier` | 38 | same | `registration.entity-attribute-creation-event` |
| `net.minecraft.world.entity.ai.attributes.AttributeSupplier$Builder` | 38 | same | `types.attribute-value` |
| `net.minecraft.world.entity.ai.attributes.AttributeMap` | 31 | same | `pumpkin:plugin/attributes`; `types.attribute-modifier`, `attribute-value` |
| `net.minecraft.world.entity.ai.attributes.RangedAttribute` | 23 | same | `registration.attribute-definition` |

#### Minecraft effects and damage

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.minecraft.world.damagesource.DamageSource` | 113 | same | `types.damage-source` |
| `net.minecraft.world.effect.MobEffectInstance` | 94 | same | `types.mob-effect-instance`; applied through `mob-effects` |
| `net.minecraft.world.effect.MobEffect` | 72 | same | `registration.mob-effect-definition` (planned, task 2.6), `callbacks.mob-effect-apply-tick` |
| `net.minecraft.world.effect.MobEffects` | 67 | same | `pumpkin:plugin/status-effect`; `types.mob-effect-instance`, `damage-source` |
| `net.minecraft.world.damagesource.DamageSources` | 62 | same | `pumpkin:plugin/world` (`living-entity.damage`) |
| `net.minecraft.world.damagesource.DamageType` | 43 | same | `pumpkin:plugin/damage-types`; custom types are datapack JSON |
| `net.minecraft.world.effect.MobEffectCategory` | 39 | same | `registration.mob-effect-category` |
| `net.minecraft.world.damagesource.DamageTypes` | 33 | same | `pumpkin:plugin/damage-types` |
| `net.minecraft.world.damagesource.DamageScaling` | 0 | same | gap: damage type JSON field (datapack) |

#### Minecraft menus and containers

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.minecraft.world.inventory.AbstractContainerMenu` | 172 | same | `menus.menu`, `menus.menu-definition`, `menus.open-menu` and the `callbacks.menu-*` functions (task 2.9) |
| `net.minecraft.world.inventory.MenuType` | 157 | same | `registration.menu-type-definition`; `menus.menu-definition.menu-type` |
| `net.minecraft.world.inventory.Slot` | 139 | same | `menus.slot-definition` (task 2.9); `pumpkin:plugin/inventory` for vanilla screens |
| `net.minecraft.world.Container` | 135 | same | `pumpkin:plugin/inventory`; menu slots through `menus.slot-source` |
| `net.minecraft.world.MenuProvider` | 121 | same | `menus.menu-definition` and `menus.open-menu` (task 2.9) |
| `net.minecraft.world.inventory.InventoryMenu` | 68 | same | `pumpkin:plugin/inventory`, `pumpkin:plugin/gui` (vanilla screens); custom menus `menus` (task 2.9) |
| `net.neoforged.neoforge.transfer.item.ResourceHandlerSlot` | 55 (21.1 `SlotItemHandler`) | new in 26.3 | `menus.slot-source::handler` (task 2.9) |
| `net.minecraft.world.SimpleContainer` | 52 | same | `menus.slot-source::menu-container`; `pumpkin:plugin/inventory` |
| `net.minecraft.world.inventory.ContainerLevelAccess` | 52 | same | `pumpkin:plugin/inventory`, `pumpkin:plugin/gui` (vanilla screens); custom menus `menus` (task 2.9) |
| `net.minecraft.world.inventory.MenuConstructor` | 51 | same | `menus.menu-definition` (task 2.9) |
| `net.minecraft.world.SimpleMenuProvider` | 45 | same | `menus.menu-definition` and `menus.open-menu` (task 2.9) |
| `net.minecraft.world.inventory.EnchantmentMenu` | 2 | same | `pumpkin:plugin/inventory`, `pumpkin:plugin/gui` (vanilla screens); custom menus `menus` (task 2.9) |

#### Minecraft level and world

| Class | Mods | 26.3 | Counterpart or gap |
|:--|--:|:--|:--|
| `net.minecraft.world.level.Level` | 310 | same | `pumpkin:plugin/world` |
| `net.minecraft.world.level.ItemLike` | 262 | same | `types.identifier` (item ids) |
| `net.minecraft.world.phys.Vec3` | 219 | same | `pumpkin:plugin/common` (`position`) |
| `net.minecraft.world.InteractionHand` | 214 | same | `types.interaction-hand` |
| `net.minecraft.world.level.BlockGetter` | 198 | same | `pumpkin:plugin/world` |
| `net.minecraft.world.InteractionResult` | 186 | same | `types.interaction-result` |
| `net.minecraft.world.phys.shapes.VoxelShape` | 167 | same | `shapes.voxel-shape` (task 2.10); mod block shapes through `registration.block-properties.shape` |
| `net.minecraft.world.level.LevelAccessor` | 162 | same | `pumpkin:plugin/world` |
| `net.minecraft.world.phys.AABB` | 161 | same | `shapes.aabb`; `pumpkin:plugin/world` (`bounding-box`) |
| `net.minecraft.world.phys.shapes.CollisionContext` | 134 | same | `shapes.get-block-shape` with `context-entity` (task 2.10) |
| `net.minecraft.world.InteractionResultHolder` | 131 | CHANGED | `types.interaction-result` (merged in 26.3) |
| `net.minecraft.world.phys.HitResult` | 127 | same | `types.hit-result` |
| `net.minecraft.world.phys.shapes.Shapes` | 119 | same | `shapes.voxel-shape`: the shape helpers are geometry in mod code (task 2.10) |
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
| `net.minecraft.server.network.ConfigurationTask` | 7 | same | `network.configuration-task` |
| `net.minecraft.server.network.ConfigurationTask$Type` | 7 | same | `network.configuration-task.type` |

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
