# Fire sites and their locks

Every fire site of `PlayLevelSoundEvent` (a `World::play_*` call or `World::play_seeded_sound*`) and of `VanillaGameEvent` (`World::emit_game_event*`) in `pumpkin-core` and `pumpkin-inventory`, by file, with the locks that the site holds at the fire. A mod handler runs inside the fire, so a site that holds a lock can hang the server when the handler takes the same lock.

Method: a script lists every fire call, finds the enclosing function, and flags a call when a guard of `lock`, `read`, `write`, `lock_record_transition` or a helper that returns a guard is bound in an enclosing block before the call, or when the call sits in a statement or closure that holds a lock. The flags were read by hand against the code (most flags were a copied value, not a guard). The guards that a caller holds were searched for the helpers that return a guard and for the screen handler lock.

A site that held a lock now fires after the lock: it reads or takes the state under the lock, drops the guard, fires, and takes the state again when it needs it. A scope that runs many fire sites under one lock starts a `Deferral` (the screen handler click) or a `FireDeferral` (the brain tick of a mob), and the sounds and the game events of the scope fire when it ends. A sound or a game event that a `FireDeferral` queues has no cause that the world no longer lists, and `World::play_seeded_sound` returns 0 for it. Known difference: the take-result and equip sounds of a screen click go out after its content sync packets, because they wait for the `Deferral` of the click, where vanilla plays them inside `clicked()`. Tests: `crates/pumpkin-core/tests/fire_site_locks.rs`.

| File | Fire sites | Lock held at the fire |
|---|---|---|
| `pumpkin-core/src/block/blocks/beehive.rs` | 4 | none |
| `pumpkin-core/src/block/blocks/brushable_block.rs` | 2 | the hit counter of the brushable block entity (fixed: read under the lock, then the sound) |
| `pumpkin-core/src/block/blocks/cake.rs` | 4 | none |
| `pumpkin-core/src/block/blocks/campfire.rs` | 2 | none |
| `pumpkin-core/src/block/blocks/candle_cakes.rs` | 4 | none |
| `pumpkin-core/src/block/blocks/candles.rs` | 4 | none |
| `pumpkin-core/src/block/blocks/cauldron.rs` | 12 | none |
| `pumpkin-core/src/block/blocks/chiseled_bookshelf.rs` | 3 | none |
| `pumpkin-core/src/block/blocks/composter.rs` | 3 | none |
| `pumpkin-core/src/block/blocks/creaking_heart.rs` | 3 | none |
| `pumpkin-core/src/block/blocks/decorated_pot.rs` | 4 | none |
| `pumpkin-core/src/block/blocks/doors.rs` | 6 | none |
| `pumpkin-core/src/block/blocks/dried_ghast.rs` | 3 | none |
| `pumpkin-core/src/block/blocks/dripstone.rs` | 1 | none |
| `pumpkin-core/src/block/blocks/fence_gates.rs` | 4 | none |
| `pumpkin-core/src/block/blocks/flower_pots.rs` | 3 | none |
| `pumpkin-core/src/block/blocks/frogspawn.rs` | 1 | none |
| `pumpkin-core/src/block/blocks/jukebox.rs` | 1 | none |
| `pumpkin-core/src/block/blocks/lectern.rs` | 2 | none |
| `pumpkin-core/src/block/blocks/note.rs` | 3 | none |
| `pumpkin-core/src/block/blocks/piston/piston.rs` | 4 | none |
| `pumpkin-core/src/block/blocks/plant/big_dripleaf.rs` | 1 | none |
| `pumpkin-core/src/block/blocks/plant/cave_vines.rs` | 2 | none |
| `pumpkin-core/src/block/blocks/plant/crop/sweet_berry_bush.rs` | 1 | none |
| `pumpkin-core/src/block/blocks/plant/eyeblossom.rs` | 2 | none |
| `pumpkin-core/src/block/blocks/plant/shelf_mushroom.rs` | 1 | none |
| `pumpkin-core/src/block/blocks/potent_sulfur.rs` | 1 | none |
| `pumpkin-core/src/block/blocks/pumpkin.rs` | 2 | none |
| `pumpkin-core/src/block/blocks/redstone/bell.rs` | 2 | none |
| `pumpkin-core/src/block/blocks/redstone/buttons.rs` | 4 | none |
| `pumpkin-core/src/block/blocks/redstone/comparator.rs` | 1 | none |
| `pumpkin-core/src/block/blocks/redstone/copper_bulb.rs` | 1 | none |
| `pumpkin-core/src/block/blocks/redstone/daylight_detector.rs` | 1 | the abilities of the player (fixed: read under the lock, then the game event) |
| `pumpkin-core/src/block/blocks/redstone/dispenser.rs` | 13 | none |
| `pumpkin-core/src/block/blocks/redstone/lever.rs` | 2 | none |
| `pumpkin-core/src/block/blocks/redstone/pressure_plate/mod.rs` | 4 | none |
| `pumpkin-core/src/block/blocks/redstone/sculk_sensor.rs` | 6 | none |
| `pumpkin-core/src/block/blocks/redstone/tripwire.rs` | 1 | none |
| `pumpkin-core/src/block/blocks/redstone/tripwire_hook.rs` | 10 | none |
| `pumpkin-core/src/block/blocks/respawn_anchor.rs` | 3 | none |
| `pumpkin-core/src/block/blocks/sculk/sculk_shrieker.rs` | 2 | none |
| `pumpkin-core/src/block/blocks/shelf.rs` | 5 | none |
| `pumpkin-core/src/block/blocks/signs.rs` | 1 | none |
| `pumpkin-core/src/block/blocks/sniffer_egg.rs` | 2 | none |
| `pumpkin-core/src/block/blocks/sponge.rs` | 2 | none |
| `pumpkin-core/src/block/blocks/straw_bed.rs` | 1 | none |
| `pumpkin-core/src/block/blocks/tnt.rs` | 2 | none |
| `pumpkin-core/src/block/blocks/trapdoor.rs` | 4 | none |
| `pumpkin-core/src/block/blocks/turtle_egg.rs` | 4 | none |
| `pumpkin-core/src/block/blocks/vault.rs` | 5 | none |
| `pumpkin-core/src/block/blocks/weathering_copper.rs` | 1 | none |
| `pumpkin-core/src/block/entities/barrel.rs` | 3 | none |
| `pumpkin-core/src/block/entities/beehive.rs` | 5 | none |
| `pumpkin-core/src/block/entities/bell.rs` | 1 | none |
| `pumpkin-core/src/block/entities/brewing_stand.rs` | 1 | none |
| `pumpkin-core/src/block/entities/chest_like_block_entity.rs` | 3 | none |
| `pumpkin-core/src/block/entities/chiseled_bookshelf.rs` | 2 | none |
| `pumpkin-core/src/block/entities/conduit.rs` | 4 | none |
| `pumpkin-core/src/block/entities/creaking_heart.rs` | 1 | none |
| `pumpkin-core/src/block/entities/ender_chest.rs` | 3 | none |
| `pumpkin-core/src/block/entities/jukebox.rs` | 1 | the record transition lock; the block state change, the world events, the neighbor updates and the game events of a record change run after it (fixed) |
| `pumpkin-core/src/block/entities/shelf.rs` | 1 | none |
| `pumpkin-core/src/block/entities/shulker_box.rs` | 4 | none |
| `pumpkin-core/src/block/entities/trial_spawner.rs` | 6 | none |
| `pumpkin-core/src/command/commands/playsound.rs` | 7 | none |
| `pumpkin-core/src/command/commands/raid.rs` | 1 | none |
| `pumpkin-core/src/enchantment/effects/play_sound.rs` | 1 | none |
| `pumpkin-core/src/enchantment/effects/replace_block.rs` | 1 | none |
| `pumpkin-core/src/enchantment/effects/replace_disk.rs` | 1 | none |
| `pumpkin-core/src/enchantment/effects/set_block_properties.rs` | 1 | none |
| `pumpkin-core/src/entity/ageable.rs` | 1 | none |
| `pumpkin-core/src/entity/ai/brain/behavior/long_jump.rs` | 2 | the brain lock of the mob, through `FireDeferral`: the sounds and the game events of a behavior fire after the tick (fixed) |
| `pumpkin-core/src/entity/ai/brain/behavior/try_lay_spawn_on_fluid_near_land.rs` | 2 | the brain lock of the mob, through `FireDeferral`: the sounds and the game events of a behavior fire after the tick (fixed) |
| `pumpkin-core/src/entity/ai/brain/behavior/work/harvest_farmland.rs` | 3 | the brain lock of the mob, through `FireDeferral`; the inventory of the villager is free at the fires (fixed) |
| `pumpkin-core/src/entity/ai/goal/bow_attack.rs` | 1 | the execution lock of the goal selector, by design (no API of a mod reaches the selector) |
| `pumpkin-core/src/entity/ai/goal/destroy_egg.rs` | 2 | the execution lock of the goal selector, by design (no API of a mod reaches the selector) |
| `pumpkin-core/src/entity/ai/goal/pick_up_block.rs` | 1 | the execution lock of the goal selector, by design (no API of a mod reaches the selector) |
| `pumpkin-core/src/entity/ai/goal/place_block.rs` | 1 | the execution lock of the goal selector, by design (no API of a mod reaches the selector) |
| `pumpkin-core/src/entity/ai/goal/ranged_crossbow_attack.rs` | 4 | the execution lock of the goal selector, by design (no API of a mod reaches the selector) |
| `pumpkin-core/src/entity/bucketable.rs` | 1 | none |
| `pumpkin-core/src/entity/combat.rs` | 5 | none |
| `pumpkin-core/src/entity/decoration/armor_stand.rs` | 3 | none |
| `pumpkin-core/src/entity/decoration/cushion.rs` | 2 | none |
| `pumpkin-core/src/entity/decoration/item_frame.rs` | 4 | none |
| `pumpkin-core/src/entity/decoration/leash_knot.rs` | 3 | none |
| `pumpkin-core/src/entity/decoration/painting.rs` | 1 | none |
| `pumpkin-core/src/entity/effect/infested.rs` | 1 | none |
| `pumpkin-core/src/entity/item.rs` | 1 | none |
| `pumpkin-core/src/entity/lightning.rs` | 2 | none |
| `pumpkin-core/src/entity/living.rs` | 11 | the equipment of the entity, at the shield break sound (fixed) |
| `pumpkin-core/src/entity/mob/creaking.rs` | 8 | none |
| `pumpkin-core/src/entity/mob/creeper.rs` | 2 | none |
| `pumpkin-core/src/entity/mob/cube_mob.rs` | 2 | none |
| `pumpkin-core/src/entity/mob/enderman.rs` | 2 | none |
| `pumpkin-core/src/entity/mob/equipment.rs` | 3 | none |
| `pumpkin-core/src/entity/mob/evoker.rs` | 6 | none |
| `pumpkin-core/src/entity/mob/ghast.rs` | 2 | none |
| `pumpkin-core/src/entity/mob/hoglin.rs` | 1 | none |
| `pumpkin-core/src/entity/mob/mod.rs` | 6 | none |
| `pumpkin-core/src/entity/mob/piglin.rs` | 2 | none |
| `pumpkin-core/src/entity/mob/piglin_brute.rs` | 1 | none |
| `pumpkin-core/src/entity/mob/raider.rs` | 1 | none |
| `pumpkin-core/src/entity/mob/shulker.rs` | 4 | none |
| `pumpkin-core/src/entity/mob/skeleton/bogged.rs` | 1 | none |
| `pumpkin-core/src/entity/mob/sulfur_cube.rs` | 10 | none |
| `pumpkin-core/src/entity/mob/warden.rs` | 1 | none |
| `pumpkin-core/src/entity/mob/witch.rs` | 2 | none |
| `pumpkin-core/src/entity/mob/zombie/drowned.rs` | 1 | none |
| `pumpkin-core/src/entity/mod.rs` | 7 | none |
| `pumpkin-core/src/entity/ominous_item_spawner.rs` | 2 | none |
| `pumpkin-core/src/entity/passive/abstract_horse.rs` | 3 | none |
| `pumpkin-core/src/entity/passive/allay.rs` | 2 | none |
| `pumpkin-core/src/entity/passive/armadillo.rs` | 6 | none |
| `pumpkin-core/src/entity/passive/camel.rs` | 2 | none |
| `pumpkin-core/src/entity/passive/chicken.rs` | 1 | none |
| `pumpkin-core/src/entity/passive/copper_golem.rs` | 2 | none |
| `pumpkin-core/src/entity/passive/cow.rs` | 1 | none |
| `pumpkin-core/src/entity/passive/fox/goals.rs` | 2 | none |
| `pumpkin-core/src/entity/passive/fox/mod.rs` | 1 | none |
| `pumpkin-core/src/entity/passive/frog/shoot_tongue.rs` | 2 | the brain lock of the mob, through `FireDeferral` (fixed) |
| `pumpkin-core/src/entity/passive/goat.rs` | 1 | none |
| `pumpkin-core/src/entity/passive/mooshroom.rs` | 2 | none |
| `pumpkin-core/src/entity/passive/nautilus.rs` | 3 | none |
| `pumpkin-core/src/entity/passive/parrot.rs` | 1 | none |
| `pumpkin-core/src/entity/passive/rabbit.rs` | 1 | none |
| `pumpkin-core/src/entity/passive/sheep.rs` | 1 | none |
| `pumpkin-core/src/entity/passive/sniffer.rs` | 1 | none |
| `pumpkin-core/src/entity/passive/snow_golem.rs` | 2 | none |
| `pumpkin-core/src/entity/passive/turtle.rs` | 2 | none |
| `pumpkin-core/src/entity/passive/villager/mod.rs` | 4 | none |
| `pumpkin-core/src/entity/player.rs` | 11 | the screen handler lock, at the sound of a take result and of an equipment change (fixed: the sound waits for the `Deferral` of the click, also in the creative slot set) |
| `pumpkin-core/src/entity/player/shoulders.rs` | 1 | none |
| `pumpkin-core/src/entity/projectile/ender_pearl.rs` | 1 | none |
| `pumpkin-core/src/entity/projectile/evoker_fangs.rs` | 1 | none |
| `pumpkin-core/src/entity/projectile/eye_of_ender.rs` | 1 | none |
| `pumpkin-core/src/entity/projectile/firework_rocket.rs` | 1 | none |
| `pumpkin-core/src/entity/projectile/fishing_bobber.rs` | 1 | none |
| `pumpkin-core/src/entity/projectile/mod.rs` | 1 | none |
| `pumpkin-core/src/entity/projectile/owner.rs` | 1 | none |
| `pumpkin-core/src/entity/projectile/shulker_bullet.rs` | 3 | none |
| `pumpkin-core/src/entity/shearable.rs` | 1 | none |
| `pumpkin-core/src/entity/vehicle/boat.rs` | 1 | none |
| `pumpkin-core/src/entity/vehicle/minecart/tnt.rs` | 1 | none |
| `pumpkin-core/src/item/items/armor_stand.rs` | 2 | none |
| `pumpkin-core/src/item/items/axe.rs` | 2 | none |
| `pumpkin-core/src/item/items/boat.rs` | 1 | none |
| `pumpkin-core/src/item/items/bone_meal.rs` | 2 | none |
| `pumpkin-core/src/item/items/bow.rs` | 1 | none |
| `pumpkin-core/src/item/items/brush.rs` | 3 | none |
| `pumpkin-core/src/item/items/bucket.rs` | 17 | none |
| `pumpkin-core/src/item/items/bundle.rs` | 1 | none |
| `pumpkin-core/src/item/items/compass.rs` | 1 | none |
| `pumpkin-core/src/item/items/crossbow.rs` | 5 | none |
| `pumpkin-core/src/item/items/cushion.rs` | 2 | none |
| `pumpkin-core/src/item/items/dye.rs` | 2 | none |
| `pumpkin-core/src/item/items/egg.rs` | 1 | none |
| `pumpkin-core/src/item/items/end_crystal.rs` | 1 | none |
| `pumpkin-core/src/item/items/ender_eye.rs` | 1 | none |
| `pumpkin-core/src/item/items/ender_pearl.rs` | 1 | none |
| `pumpkin-core/src/item/items/experience_bottle.rs` | 1 | none |
| `pumpkin-core/src/item/items/firework_rocket.rs` | 1 | none |
| `pumpkin-core/src/item/items/fishing_rod.rs` | 2 | none |
| `pumpkin-core/src/item/items/glass_bottle.rs` | 6 | none |
| `pumpkin-core/src/item/items/glowing_ink_sac.rs` | 1 | none |
| `pumpkin-core/src/item/items/goat_horn.rs` | 2 | none |
| `pumpkin-core/src/item/items/hanging_entity.rs` | 4 | none |
| `pumpkin-core/src/item/items/hoe.rs` | 2 | none |
| `pumpkin-core/src/item/items/honeycomb.rs` | 5 | none |
| `pumpkin-core/src/item/items/ignite/fire_charge.rs` | 2 | none |
| `pumpkin-core/src/item/items/ignite/flint_and_steel.rs` | 2 | none |
| `pumpkin-core/src/item/items/ink_sac.rs` | 1 | none |
| `pumpkin-core/src/item/items/lead.rs` | 2 | none |
| `pumpkin-core/src/item/items/map.rs` | 1 | none |
| `pumpkin-core/src/item/items/minecart.rs` | 1 | none |
| `pumpkin-core/src/item/items/place_on_water.rs` | 2 | none |
| `pumpkin-core/src/item/items/potions.rs` | 5 | none |
| `pumpkin-core/src/item/items/shears.rs` | 2 | none |
| `pumpkin-core/src/item/items/shovel.rs` | 2 | none |
| `pumpkin-core/src/item/items/snowball.rs` | 1 | none |
| `pumpkin-core/src/item/items/spawn_egg.rs` | 5 | none |
| `pumpkin-core/src/item/items/spear.rs` | 3 | none |
| `pumpkin-core/src/item/items/spyglass.rs` | 2 | none |
| `pumpkin-core/src/item/items/trident.rs` | 2 | none |
| `pumpkin-core/src/item/items/wind_charge.rs` | 1 | none |
| `pumpkin-core/src/net/java/play/attack.rs` | 1 | none |
| `pumpkin-core/src/net/java/play/interact.rs` | 1 | none |
| `pumpkin-core/src/world/explosion.rs` | 1 | none |
| `pumpkin-core/src/world/ironpumpkin.rs` | 1 | none |
| `pumpkin-core/src/world/mod.rs` | 15 | none |
| `pumpkin-core/src/world/raid.rs` | 1 | none |
| `pumpkin-core/src/world/sound.rs` | 6 | none |
| `pumpkin-inventory/src/cartography_table_screen_handler.rs` | 1 | none |
| `pumpkin-inventory/src/enchanting/enchanting_screen_handler/events.rs` | 1 | none |
| `pumpkin-inventory/src/loom_screen_handler.rs` | 1 | none |
| `pumpkin-inventory/src/stonecutter_screen_handler.rs` | 1 | none |
