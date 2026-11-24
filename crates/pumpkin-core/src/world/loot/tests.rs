use super::*;

#[test]
fn alternatives_use_the_first_matching_child() {
    let table = crate::data::datapack::loot_table_loader::parse_loot_table(
            r#"{"pools":[{"rolls":4,"entries":[{"type":"minecraft:alternatives","children":[{"type":"minecraft:item","name":"minecraft:stone"},{"type":"minecraft:item","name":"minecraft:diamond"}]}]}]}"#,
        )
        .unwrap();
    for seed in 0..8 {
        let drops = generate_dynamic_loot(&table, seed);
        assert_eq!(drops.len(), 4);
        assert!(drops.iter().all(|stack| stack.item == &Item::STONE));
    }
}

#[test]
fn entity_predicates_do_not_match_a_missing_entity() {
    for key in [
        "gameplay/chicken_lay",
        "shearing/mooshroom",
        "entities/magma_cube",
    ] {
        let table = get_loot_table(key).unwrap();
        for seed in 0..8 {
            assert!(table.generate_loot(seed).is_empty(), "{key}, seed {seed}");
        }
    }
}

use crate::entity::{
    Entity,
    ai::test_support::world,
    mob::{Mob, magma_cube::MagmaCubeEntity},
    passive::{
        chicken::ChickenEntity, frog::FrogEntity, mooshroom::MooshroomEntity, sheep::SheepEntity,
    },
};
use pumpkin_data::{
    chicken_variant::ChickenVariant, entity::EntityType, entity_variant::MooshroomVariant,
    frog_variant::FrogVariant,
};
use pumpkin_util::loot_table::parser::parse_loot_table;
use pumpkin_util::math::vector3::Vector3;

fn assert_drops(
    key: &str,
    json: &str,
    params: &LootContextParameters<'_>,
    expected: &[(&Item, u8)],
) {
    let dynamic = parse_loot_table(json).unwrap();
    let static_table = get_loot_table(key).unwrap();
    for seed in 0..16 {
        for drops in [
            static_table.generate_loot_with_context(seed, params),
            generate_dynamic_loot_with_context(&dynamic, seed, params),
        ] {
            assert_eq!(drops.len(), expected.len(), "{key}, seed {seed}");
            for (stack, (item, count)) in drops.iter().zip(expected) {
                assert_eq!(stack.item, *item, "{key}, seed {seed}");
                assert_eq!(stack.item_count, *count, "{key}, seed {seed}");
            }
        }
    }
}

#[tokio::test]
async fn component_predicates_select_chicken_and_mooshroom_variants() {
    let folder = tempfile::tempdir().unwrap();
    let world = world(&folder);
    let chicken = ChickenEntity::new(Entity::new(
        world.clone(),
        Vector3::default(),
        &EntityType::CHICKEN,
    ));
    for (variant, item) in [
        (ChickenVariant::Temperate, &Item::EGG),
        (ChickenVariant::Warm, &Item::BROWN_EGG),
        (ChickenVariant::Cold, &Item::BLUE_EGG),
    ] {
        chicken.set_variant(variant);
        let params = LootContextParameters {
            world: Some(&world),
            this_entity: Some(chicken.as_ref()),
            ..Default::default()
        };
        assert_drops(
            "gameplay/chicken_lay",
            include_str!(
                "../../../../../assets/datapack/data/minecraft/loot_table/gameplay/chicken_lay.json"
            ),
            &params,
            &[(item, 1)],
        );
    }
    let mooshroom = MooshroomEntity::new(Entity::new(
        world.clone(),
        Vector3::default(),
        &EntityType::MOOSHROOM,
    ));
    for (variant, item) in [
        (MooshroomVariant::Red, &Item::RED_MUSHROOM),
        (MooshroomVariant::Brown, &Item::BROWN_MUSHROOM),
    ] {
        mooshroom.set_variant(variant);
        let params = LootContextParameters {
            world: Some(&world),
            this_entity: Some(mooshroom.as_ref()),
            ..Default::default()
        };
        assert_drops(
            "shearing/mooshroom",
            include_str!(
                "../../../../../assets/datapack/data/minecraft/loot_table/shearing/mooshroom.json"
            ),
            &params,
            &[(item, 1); 5],
        );
    }
}

#[tokio::test]
async fn chicken_variant_set_and_nbt_roundtrip() {
    let folder = tempfile::tempdir().unwrap();
    let world = world(&folder);
    let chicken = ChickenEntity::new(Entity::new(
        world.clone(),
        Vector3::default(),
        &EntityType::CHICKEN,
    ));
    chicken.mob_set_variant_name("warm");
    assert_eq!(
        chicken.variant.load(std::sync::atomic::Ordering::Relaxed),
        ChickenVariant::Warm.id()
    );

    let mut nbt = pumpkin_nbt::compound::NbtCompound::new();
    chicken.mob_write_nbt(&mut nbt);
    assert_eq!(nbt.get_string("variant"), Some("minecraft:warm"));

    let chicken2 = ChickenEntity::new(Entity::new(world, Vector3::default(), &EntityType::CHICKEN));
    chicken2.mob_read_nbt(&nbt);
    assert_eq!(
        chicken2.variant.load(std::sync::atomic::Ordering::Relaxed),
        ChickenVariant::Warm.id()
    );
}

#[tokio::test]
async fn component_predicates_use_the_current_entity_color() {
    let folder = tempfile::tempdir().unwrap();
    let world = world(&folder);
    let sheep = SheepEntity::new(Entity::new(
        world.clone(),
        Vector3::default(),
        &EntityType::SHEEP,
    ));
    let dynamic = parse_loot_table(include_str!(
        "../../../../../assets/datapack/data/minecraft/loot_table/entities/sheep.json"
    ))
    .unwrap();
    let table = get_loot_table("entities/sheep").unwrap();
    for color in 0..16 {
        sheep.set_color(color);
        let wool_name = format!(
            "{}_wool",
            pumpkin_data::dye_color::DyeColor::from(color).name()
        );
        let wool = Item::from_registry_key(&wool_name).unwrap();
        {
            let params = LootContextParameters {
                world: Some(&world),
                this_entity: Some(sheep.as_ref()),
                ..Default::default()
            };
            for seed in 0..8 {
                for drops in [
                    table.generate_loot_with_context(seed, &params),
                    generate_dynamic_loot_with_context(&dynamic, seed, &params),
                ] {
                    let wool_drops: Vec<_> = drops
                        .iter()
                        .filter(|stack| stack.item != &Item::MUTTON)
                        .collect();
                    assert_eq!(wool_drops.len(), 1);
                    assert!(
                        wool_drops
                            .iter()
                            .all(|stack| stack.item == wool && stack.item_count == 1)
                    );
                    assert!(
                        drops.iter().any(|stack| stack.item == &Item::MUTTON
                            && (1..=2).contains(&stack.item_count))
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn damage_source_predicates_use_the_attackers_components() {
    let folder = tempfile::tempdir().unwrap();
    let world = world(&folder);
    let cube = MagmaCubeEntity::new(Entity::new(
        world.clone(),
        Vector3::default(),
        &EntityType::MAGMA_CUBE,
    ));
    let frog = FrogEntity::new(Entity::new(
        world.clone(),
        Vector3::default(),
        &EntityType::FROG,
    ));
    let json = include_str!(
        "../../../../../assets/datapack/data/minecraft/loot_table/entities/magma_cube.json"
    );
    for size in [1, 2] {
        cube.slime.set_size(size, true);
        for (variant, light) in [
            (FrogVariant::Temperate, &Item::OCHRE_FROGLIGHT),
            (FrogVariant::Warm, &Item::PEARLESCENT_FROGLIGHT),
            (FrogVariant::Cold, &Item::VERDANT_FROGLIGHT),
        ] {
            frog.set_variant(variant);
            let params = LootContextParameters {
                world: Some(&world),
                this_entity: Some(cube.as_ref()),
                killer_entity: Some(frog.as_ref()),
                // `damage_source_properties` fails without a damage source and an origin.
                position: Some(Vector3::default()),
                damage_source: Some(LootDamageSource {
                    damage_type: DamageType::MOB_ATTACK,
                    attacker: Some(frog.as_ref()),
                    direct: Some(frog.as_ref()),
                }),
                ..Default::default()
            };
            assert_drops("entities/magma_cube", json, &params, &[(light, 1)]);
        }
    }
}

#[test]
fn conditions_are_expanded_again_for_each_roll() {
    let table = parse_loot_table(r#"{"pools":[{"rolls":128,"entries":[{"type":"minecraft:alternatives","children":[{"type":"minecraft:item","name":"minecraft:stone","condition":{"type":"minecraft:random_chance","chance":0.5}},{"type":"minecraft:item","name":"minecraft:diamond"}]}]}]}"#).unwrap();
    let drops = generate_dynamic_loot(&table, 0);
    assert_eq!(drops.len(), 128);
    assert!(drops.iter().any(|stack| stack.item == &Item::STONE));
    assert!(drops.iter().any(|stack| stack.item == &Item::DIAMOND));
}

#[test]
fn empty_entries_preserve_their_conditions() {
    for (entry, fallback_weight, expected) in [
        (
            r#"{"type":"minecraft:empty","condition":{"type":"minecraft:random_chance","chance":0}}"#,
            1,
            Some(&Item::DIAMOND),
        ),
        (r#"{"type":"minecraft:empty"}"#, 0, None),
    ] {
        let json = format!(
            r#"{{"pools":[{{"entries":[{{"type":"minecraft:alternatives","children":[{entry},{{"type":"minecraft:item","name":"minecraft:diamond","weight":{fallback_weight}}}]}}]}}]}}"#
        );
        let table = parse_loot_table(&json).unwrap();
        let drops = generate_dynamic_loot(&table, 0);
        assert_eq!(drops.len(), usize::from(expected.is_some()));
        if let Some(item) = expected {
            assert_eq!(drops[0].item, item);
        }
    }
}

#[test]
fn nested_tables_keep_rolls_and_modifier_order() {
    let table = parse_loot_table(r#"{"modifier":{"type":"minecraft:set_count","count":4,"add":true},"pools":[{"modifier":{"type":"minecraft:set_count","count":3,"add":true},"entries":[{"type":"minecraft:alternatives","modifier":{"type":"minecraft:set_count","count":1,"add":true},"children":[{"type":"minecraft:loot_table","value":{"pools":[{"rolls":3,"entries":[{"type":"minecraft:item","name":"minecraft:diamond","modifier":{"type":"minecraft:set_count","count":2}}]}]}}]}]}]}"#).unwrap();
    let drops = generate_dynamic_loot(&table, 0);
    assert_eq!(drops.len(), 3);
    assert!(
        drops
            .iter()
            .all(|stack| stack.item == &Item::DIAMOND && stack.item_count == 10)
    );
}

#[test]
fn immature_wheat_only_drops_one_seed() {
    use pumpkin_data::{Block, block_properties::WheatLikeProperties};
    let dynamic = parse_loot_table(include_str!(
        "../../../../../assets/datapack/data/minecraft/loot_table/blocks/wheat.json"
    ))
    .unwrap();
    let table = get_loot_table("blocks/wheat").unwrap();
    for age in 0..7 {
        let state = WheatLikeProperties { age }
            .to_state_id(&Block::WHEAT)
            .to_state();
        let params = LootContextParameters {
            block_state: Some(state),
            ..Default::default()
        };
        for seed in 0..16 {
            for drops in [
                table.generate_loot_with_context(seed, &params),
                generate_dynamic_loot_with_context(&dynamic, seed, &params),
            ] {
                assert_eq!(drops.len(), 1, "age {age}, seed {seed}");
                assert_eq!(drops[0].item, &Item::WHEAT_SEEDS);
                assert_eq!(drops[0].item_count, 1);
            }
        }
    }
    let state = WheatLikeProperties { age: 7 }
        .to_state_id(&Block::WHEAT)
        .to_state();
    let params = LootContextParameters {
        block_state: Some(state),
        tool: Some(ItemStack::EMPTY.clone()),
        ..Default::default()
    };
    for seed in 0..16 {
        for drops in [
            table.generate_loot_with_context(seed, &params),
            generate_dynamic_loot_with_context(&dynamic, seed, &params),
        ] {
            assert_eq!(drops[0].item, &Item::WHEAT);
            assert_eq!(drops[0].item_count, 1);
            assert_eq!(drops[1].item, &Item::WHEAT_SEEDS);
            assert!((1..=4).contains(&drops[1].item_count));
        }
    }
}

#[test]
fn weather_checks_do_not_become_unconditional_alternatives() {
    let table = parse_loot_table(r#"{"pools":[{"entries":[{"type":"minecraft:alternatives","children":[{"type":"minecraft:item","name":"minecraft:diamond","condition":{"type":"minecraft:weather_check","raining":true}},{"type":"minecraft:item","name":"minecraft:stone"}]}]}]}"#).unwrap();
    for (raining, expected) in [(false, &Item::STONE), (true, &Item::DIAMOND)] {
        let drops = generate_dynamic_loot_with_context(
            &table,
            0,
            &LootContextParameters {
                is_raining: Some(raining),
                ..Default::default()
            },
        );
        assert_eq!(drops[0].item, expected);
    }
}

#[test]
fn unknown_conditions_and_invalid_entries_do_not_change_alternatives() {
    for condition in [
        r#"{"type":"unknown:condition"}"#,
        r#"{"type":"minecraft:inverted","term":{"type":"unknown:condition"}}"#,
    ] {
        let json = format!(
            r#"{{"pools":[{{"entries":[{{"type":"minecraft:alternatives","children":[{{"type":"minecraft:item","name":"minecraft:diamond","condition":{condition}}},{{"type":"minecraft:item","name":"minecraft:stone"}}]}}]}}]}}"#
        );
        let table = parse_loot_table(&json).unwrap();
        assert_eq!(generate_dynamic_loot(&table, 0)[0].item, &Item::STONE);
    }
    assert!(parse_loot_table(r#"{"pools":[{"entries":[{"type":"minecraft:alternatives","children":[{"type":"minecraft:item"},{"type":"minecraft:item","name":"minecraft:stone"}]}]}]}"#).is_none());
}

#[test]
fn group_and_sequence_preserve_partial_expansion() {
    for (first, fallback_weight, expected) in [
        (
            r#"{"type":"minecraft:group","children":[{"type":"minecraft:item","name":"minecraft:stone","condition":{"type":"minecraft:random_chance","chance":0}}]}"#,
            1,
            &Item::DIAMOND,
        ),
        (
            r#"{"type":"minecraft:sequence","children":[{"type":"minecraft:item","name":"minecraft:stone"},{"type":"minecraft:empty","condition":{"type":"minecraft:random_chance","chance":0}},{"type":"minecraft:item","name":"minecraft:emerald"}]}"#,
            0,
            &Item::STONE,
        ),
        (
            r#"{"type":"minecraft:group","children":[{"type":"minecraft:item","name":"minecraft:stone","condition":{"type":"minecraft:random_chance","chance":0}},{"type":"minecraft:item","name":"minecraft:emerald"}]}"#,
            1,
            &Item::EMERALD,
        ),
    ] {
        let json = format!(
            r#"{{"pools":[{{"entries":[{{"type":"minecraft:alternatives","children":[{first},{{"type":"minecraft:item","name":"minecraft:diamond","weight":{fallback_weight}}}]}}]}}]}}"#
        );
        let table = parse_loot_table(&json).unwrap();
        let drops = generate_dynamic_loot(&table, 0);
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].item, expected);
    }
}

#[test]
fn amethyst_tool_tag_selects_the_right_alternative() {
    let table = get_loot_table("blocks/amethyst_cluster").unwrap();
    let dynamic = crate::data::datapack::loot_table_loader::parse_loot_table(include_str!(
        "../../../../../assets/datapack/data/minecraft/loot_table/blocks/amethyst_cluster.json"
    ))
    .unwrap();
    for (tool, expected, count) in [
        (None, &Item::AMETHYST_SHARD, 2),
        (
            Some(ItemStack::new(1, &Item::IRON_PICKAXE)),
            &Item::AMETHYST_SHARD,
            4,
        ),
    ] {
        let params = LootContextParameters {
            tool,
            ..Default::default()
        };
        for drops in [
            table.generate_loot_with_context(0, &params),
            generate_dynamic_loot_with_context(&dynamic, 0, &params),
        ] {
            assert_eq!(drops.len(), 1);
            assert_eq!(drops[0].item, expected);
            assert_eq!(drops[0].item_count, count);
        }
    }
    let mut silk = ItemStack::new(1, &Item::IRON_PICKAXE);
    silk.add_enchantment(&pumpkin_data::Enchantment::SILK_TOUCH, 1);
    let params = LootContextParameters {
        tool: Some(silk),
        ..Default::default()
    };
    let drops = table.generate_loot_with_context(0, &params);
    assert_eq!(drops[0].item, &Item::AMETHYST_CLUSTER);
}

#[test]
fn luck_changes_quality_weights_and_bonus_rolls() {
    let table = parse_loot_table(r#"{"pools":[{"rolls":1,"bonus_rolls":0.75,"entries":[{"type":"minecraft:item","name":"minecraft:stone","quality":2},{"type":"minecraft:item","name":"minecraft:diamond","quality":-2}]}]}"#).unwrap();
    for seed in 0..16 {
        let drops = generate_dynamic_loot_with_context(
            &table,
            seed,
            &LootContextParameters {
                luck: 4.0,
                ..Default::default()
            },
        );
        assert_eq!(drops.len(), 4);
        assert!(drops.iter().all(|stack| stack.item == &Item::STONE));
    }
    let table = parse_loot_table(r#"{"pools":[{"entries":[{"type":"minecraft:item","name":"minecraft:stone","quality":2},{"type":"minecraft:item","name":"minecraft:diamond","quality":-2}]}]}"#).unwrap();
    let drops = generate_dynamic_loot_with_context(
        &table,
        0,
        &LootContextParameters {
            luck: -1.0,
            ..Default::default()
        },
    );
    assert_eq!(drops[0].item, &Item::DIAMOND);
}

#[test]
fn binomial_stem_count_is_not_a_uniform_range() {
    use pumpkin_data::{Block, block_properties::MelonStemProperties};
    let table = get_loot_table("blocks/melon_stem").unwrap();
    let dynamic = parse_loot_table(include_str!(
        "../../../../../assets/datapack/data/minecraft/loot_table/blocks/melon_stem.json"
    ))
    .unwrap();
    let state = MelonStemProperties { age: 0 }
        .to_state_id(&Block::MELON_STEM)
        .to_state();
    let params = LootContextParameters {
        block_state: Some(state),
        ..Default::default()
    };
    let mut total = 0u32;
    for seed in 0..2048 {
        let drops = table.generate_loot_with_context(seed, &params);
        let loaded = generate_dynamic_loot_with_context(&dynamic, seed, &params);
        let count: u32 = drops.iter().map(|stack| u32::from(stack.item_count)).sum();
        assert_eq!(
            count,
            loaded
                .iter()
                .map(|stack| u32::from(stack.item_count))
                .sum::<u32>()
        );
        total += count;
    }
    assert!(
        (250..600).contains(&total),
        "{total} seeds from 2048 young stems"
    );
}

#[test]
fn count_limit_precedes_explosion_decay() {
    let table = parse_loot_table(r#"{"pools":[{"entries":[{"type":"minecraft:item","name":"minecraft:glowstone_dust","modifier":[{"type":"minecraft:set_count","count":32},{"type":"minecraft:limit_count","limit":{"max":4}},{"type":"minecraft:explosion_decay"}]}]}]}"#).unwrap();
    let intact = generate_dynamic_loot(&table, 0);
    assert_eq!(intact[0].item_count, 4);
    let mut surviving = 0u32;
    for seed in 0..128 {
        let drops = generate_dynamic_loot_with_context(
            &table,
            seed,
            &LootContextParameters {
                explosion_radius: Some(2.0),
                ..Default::default()
            },
        );
        surviving += drops
            .iter()
            .map(|stack| u32::from(stack.item_count))
            .sum::<u32>();
    }
    assert!(
        (180..330).contains(&surviving),
        "{surviving} surviving drops"
    );
}

#[tokio::test]
async fn enchanted_count_uses_the_attackers_allowed_equipment_and_float_rounding() {
    use pumpkin_data::{Enchantment, data_component_impl::EquipmentSlot};
    let folder = tempfile::tempdir().unwrap();
    let world = world(&folder);
    let attacker = SheepEntity::new(Entity::new(world, Vector3::default(), &EntityType::SHEEP));
    let mut sword = ItemStack::new(1, &Item::DIAMOND_SWORD);
    sword.add_enchantment(&Enchantment::LOOTING, 3);
    attacker
        .mob_entity
        .living_entity
        .entity_equipment
        .lock()
        .unwrap()
        .put(&EquipmentSlot::MAIN_HAND, sword);
    let mut helmet = ItemStack::new(1, &Item::DIAMOND_HELMET);
    helmet.add_enchantment(&Enchantment::LOOTING, 10);
    attacker
        .mob_entity
        .living_entity
        .entity_equipment
        .lock()
        .unwrap()
        .put(&EquipmentSlot::HEAD, helmet);
    let mut tool = ItemStack::new(1, &Item::DIAMOND_PICKAXE);
    tool.add_enchantment(&Enchantment::FORTUNE, 10);
    let params = LootContextParameters {
        killer_entity: Some(attacker.as_ref()),
        tool: Some(tool),
        ..Default::default()
    };
    let table = parse_loot_table(r#"{"pools":[{"entries":[{"type":"minecraft:item","name":"minecraft:diamond","modifier":[{"type":"minecraft:set_count","count":0},{"type":"minecraft:enchanted_count_increase","enchantment":"minecraft:looting","count":{"type":"minecraft:uniform","min":0.0,"max":1.0}}]}]}]}"#).unwrap();
    let mut bins = [0; 4];
    for seed in 0..4096 {
        let drops = generate_dynamic_loot_with_context(&table, seed, &params);
        let count = drops
            .first()
            .map_or(0, |stack| usize::from(stack.item_count));
        bins[count] += 1;
    }
    assert!(
        (500..900).contains(&bins[0]) && (500..900).contains(&bins[3]),
        "{bins:?}"
    );
    assert!(
        (1150..1600).contains(&bins[1]) && (1150..1600).contains(&bins[2]),
        "{bins:?}"
    );
    for (base, count, limit, expected) in [(0, 0.5, 0, 2), (0, 0.5, 1, 1), (-2, 1.0, 0, 3)] {
        let json = format!(
            r#"{{"pools":[{{"entries":[{{"type":"minecraft:item","name":"minecraft:diamond","modifier":[{{"type":"minecraft:set_count","count":{base}}},{{"type":"minecraft:enchanted_count_increase","enchantment":"minecraft:looting","count":{count},"limit":{limit}}}]}}]}}]}}"#
        );
        let table = parse_loot_table(&json).unwrap();
        let drops = generate_dynamic_loot_with_context(&table, 0, &params);
        assert_eq!(drops[0].item_count, expected);
        assert!(
            generate_dynamic_loot_with_context(
                &table,
                0,
                &LootContextParameters {
                    tool: params.tool.clone(),
                    ..Default::default()
                }
            )
            .iter()
            .all(|stack| stack.item_count == base.max(0) as u8)
        );
    }
    attacker
        .mob_entity
        .living_entity
        .entity_equipment
        .lock()
        .unwrap()
        .equipment
        .get_mut(&EquipmentSlot::MAIN_HAND)
        .unwrap()
        .item_count = 0;
    assert!(generate_dynamic_loot_with_context(&table, 0, &params).is_empty());
}

#[test]
fn match_tool_distinguishes_missing_and_present_empty_tools() {
    let table = parse_loot_table(r#"{"pools":[{"entries":[{"type":"minecraft:alternatives","children":[{"type":"minecraft:item","name":"minecraft:diamond","condition":{"type":"minecraft:match_tool"}},{"type":"minecraft:item","name":"minecraft:stone"}]}]}]}"#).unwrap();
    for (tool, expected) in [
        (None, &Item::STONE),
        (Some(ItemStack::EMPTY.clone()), &Item::DIAMOND),
    ] {
        let drops = generate_dynamic_loot_with_context(
            &table,
            0,
            &LootContextParameters {
                tool,
                ..Default::default()
            },
        );
        assert_eq!(drops[0].item, expected);
    }
}

#[tokio::test]
async fn component_predicates_do_not_discard_the_existing_fire_condition() {
    let folder = tempfile::tempdir().unwrap();
    let world = world(&folder);
    let chicken = ChickenEntity::new(Entity::new(world, Vector3::default(), &EntityType::CHICKEN));
    let table = parse_loot_table(r#"{"pools":[{"entries":[{"type":"minecraft:alternatives","children":[{"type":"minecraft:item","name":"minecraft:diamond","condition":{"type":"minecraft:entity_properties","entity":"this","predicate":{"minecraft:flags":{"is_on_fire":true},"minecraft:components":{"minecraft:chicken/variant":"minecraft:warm"}}}},{"type":"minecraft:item","name":"minecraft:stone"}]}]}]}"#).unwrap();
    for (variant, on_fire, expected) in [
        (ChickenVariant::Warm, false, &Item::STONE),
        (ChickenVariant::Warm, true, &Item::DIAMOND),
        (ChickenVariant::Cold, true, &Item::STONE),
    ] {
        chicken.set_variant(variant);
        let params = LootContextParameters {
            this_entity: Some(chicken.as_ref()),
            is_on_fire: Some(on_fire),
            ..Default::default()
        };
        assert_eq!(
            generate_dynamic_loot_with_context(&table, 0, &params)[0].item,
            expected
        );
    }
}

#[tokio::test]
async fn double_plants_keep_their_adjacent_half_location_checks() {
    use pumpkin_data::{
        Block,
        block_properties::{DoubleBlockHalf, LargeFernProperties},
    };
    use pumpkin_util::math::{position::BlockPos, vector2::Vector2};
    use pumpkin_world::{chunk::ChunkData, world::BlockFlags};
    let folder = tempfile::tempdir().unwrap();
    let world = world(&folder);
    world
        .level
        .loaded_chunks
        .insert(Vector2::new(-1, -1), ChunkData::empty_sync(-1, -1));
    let flags = BlockFlags::SKIP_BLOCK_ADDED_CALLBACK | BlockFlags::UPDATE_KNOWN_SHAPE;
    let lower_pos = BlockPos::new(-8, 64, -8);
    let upper_pos = BlockPos::new(-8, 65, -8);
    for (block, key, expected, json) in [
        (
            &Block::LARGE_FERN,
            "blocks/large_fern",
            &Item::FERN,
            include_str!(
                "../../../../../assets/datapack/data/minecraft/loot_table/blocks/large_fern.json"
            ),
        ),
        (
            &Block::TALL_GRASS,
            "blocks/tall_grass",
            &Item::SHORT_GRASS,
            include_str!(
                "../../../../../assets/datapack/data/minecraft/loot_table/blocks/tall_grass.json"
            ),
        ),
    ] {
        let lower = LargeFernProperties {
            half: DoubleBlockHalf::Lower,
        }
        .to_state_id(block);
        let upper = LargeFernProperties {
            half: DoubleBlockHalf::Upper,
        }
        .to_state_id(block);
        world.set_block_state(&lower_pos, lower, flags);
        world.set_block_state(&upper_pos, upper, flags);
        let table = get_loot_table(key).unwrap();
        let loaded = crate::data::datapack::loot_table_loader::parse_loot_table(json).unwrap();
        for (position, state) in [(lower_pos, lower), (upper_pos, upper)] {
            let params = LootContextParameters {
                world: Some(&world),
                position: Some(position.to_f64().add_raw(0.25, 0.125, 0.25)),
                block_state: Some(state.to_state()),
                tool: Some(ItemStack::new(1, &Item::SHEARS)),
                ..Default::default()
            };
            for drops in [
                table.generate_loot_with_context(0, &params),
                generate_dynamic_loot_with_context(&loaded, 0, &params),
            ] {
                assert_eq!(drops.len(), 1);
                assert_eq!(drops[0].item, expected);
                assert_eq!(drops[0].item_count, 2);
            }
        }
        world.set_block_state(&upper_pos, Block::AIR.default_state.id, flags);
        let params = LootContextParameters {
            world: Some(&world),
            position: Some(lower_pos.to_f64()),
            block_state: Some(lower.to_state()),
            tool: Some(ItemStack::new(1, &Item::SHEARS)),
            ..Default::default()
        };
        assert!(table.generate_loot_with_context(0, &params).is_empty());
    }
    world.level.shutdown().await;
}

#[test]
fn negative_uniform_bonus_multiplier_keeps_the_base_count() {
    for seed in 0..32 {
        let mut rng = Xoroshiro::from_seed(seed);
        assert_eq!(
            apply_bonus_formula(2, LootBonusFormula::UniformBonusCount(-3), 3, &mut rng),
            2
        );
    }
}

#[test]
fn count_modifiers_preserve_integer_and_clamp_boundaries() {
    for (modifiers, expected) in [
        (
            r#"[{"type":"minecraft:set_count","count":16777217},{"type":"minecraft:set_count","count":-16777216,"add":true}]"#,
            1,
        ),
        (
            r#"[{"type":"minecraft:set_count","count":2},{"type":"minecraft:set_count","count":-1.5,"add":true}]"#,
            1,
        ),
        (
            r#"[{"type":"minecraft:set_count","count":0},{"type":"minecraft:limit_count","limit":{"min":4,"max":2}}]"#,
            4,
        ),
    ] {
        let json = format!(
            r#"{{"pools":[{{"entries":[{{"type":"minecraft:item","name":"minecraft:diamond","modifier":{modifiers}}}]}}]}}"#
        );
        let table = parse_loot_table(&json).unwrap();
        let drops = generate_dynamic_loot(&table, 0);
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].item_count, expected);
    }
}

#[test]
fn empty_location_and_state_predicates_preserve_their_context_requirements() {
    for condition in [
        r#"{"type":"minecraft:location_check"}"#,
        r#"{"type":"minecraft:location_check","predicate":{}}"#,
    ] {
        let json = format!(
            r#"{{"pools":[{{"entries":[{{"type":"minecraft:alternatives","children":[{{"type":"minecraft:item","name":"minecraft:diamond","condition":{condition}}},{{"type":"minecraft:item","name":"minecraft:stone"}}]}}]}}]}}"#
        );
        let table = parse_loot_table(&json).unwrap();
        assert_eq!(generate_dynamic_loot(&table, 0)[0].item, &Item::STONE);
        let params = LootContextParameters {
            position: Some(Vector3::default()),
            ..Default::default()
        };
        assert_eq!(
            generate_dynamic_loot_with_context(&table, 0, &params)[0].item,
            &Item::DIAMOND
        );
    }
    let table = parse_loot_table(r#"{"pools":[{"entries":[{"type":"minecraft:item","name":"minecraft:diamond","condition":{"type":"minecraft:match_block","blocks":"minecraft:stone","state":{}}}]}]}"#).unwrap();
    assert_eq!(
        generate_dynamic_loot_with_context(
            &table,
            0,
            &LootContextParameters {
                block_state: Some(pumpkin_data::Block::STONE.default_state),
                ..Default::default()
            }
        )[0]
        .item,
        &Item::DIAMOND
    );
}

#[tokio::test]
async fn random_chance_with_looting_respects_level_and_multiplier() {
    use pumpkin_data::{Enchantment, data_component_impl::EquipmentSlot};

    let folder = tempfile::tempdir().unwrap();
    let world = world(&folder);
    let attacker = SheepEntity::new(Entity::new(world, Vector3::default(), &EntityType::SHEEP));
    let mut sword = ItemStack::new(1, &Item::DIAMOND_SWORD);
    sword.add_enchantment(&Enchantment::LOOTING, 2);
    attacker
        .mob_entity
        .living_entity
        .entity_equipment
        .lock()
        .unwrap()
        .put(&EquipmentSlot::MAIN_HAND, sword);

    let json = r#"{"pools":[{"entries":[{"type":"minecraft:item","name":"minecraft:diamond","conditions":[{"condition":"minecraft:random_chance_with_looting","chance":0.0,"looting_multiplier":0.5}]}]}]}"#;
    let table = parse_loot_table(json).unwrap();

    let no_looting_params = LootContextParameters::default();
    let drops = generate_dynamic_loot_with_context(&table, 0, &no_looting_params);
    assert!(drops.is_empty());

    let looting_params = LootContextParameters {
        killer_entity: Some(attacker.as_ref()),
        ..Default::default()
    };
    let drops = generate_dynamic_loot_with_context(&table, 0, &looting_params);
    assert_eq!(drops.len(), 1);
    assert_eq!(drops[0].item, &Item::DIAMOND);
}

#[test]
fn survives_explosion_and_decay_at_or_below_one_radius_preserves_drops() {
    let json_condition = r#"{"pools":[{"entries":[{"type":"minecraft:item","name":"minecraft:diamond","conditions":[{"condition":"minecraft:survives_explosion"}]}]}]}"#;
    let table_condition = parse_loot_table(json_condition).unwrap();

    let json_decay = r#"{"pools":[{"entries":[{"type":"minecraft:item","name":"minecraft:diamond","functions":[{"function":"minecraft:set_count","count":10},{"function":"minecraft:explosion_decay"}]}]}]}"#;
    let table_decay = parse_loot_table(json_decay).unwrap();

    for radius in [None, Some(0.5), Some(1.0)] {
        let params = LootContextParameters {
            explosion_radius: radius,
            ..Default::default()
        };
        for seed in 0..10 {
            let drops_cond = generate_dynamic_loot_with_context(&table_condition, seed, &params);
            assert_eq!(drops_cond.len(), 1, "radius {radius:?}, seed {seed}");
            assert_eq!(drops_cond[0].item, &Item::DIAMOND);

            let drops_decay = generate_dynamic_loot_with_context(&table_decay, seed, &params);
            assert_eq!(drops_decay.len(), 1, "radius {radius:?}, seed {seed}");
            assert_eq!(drops_decay[0].item_count, 10);
        }
    }
}

#[tokio::test]
async fn entity_properties_evaluates_flags_distance_and_type_specific() {
    use std::sync::atomic::Ordering;

    let folder = tempfile::tempdir().unwrap();
    let world = world(&folder);
    let sheep = SheepEntity::new(Entity::new(
        world.clone(),
        Vector3::default(),
        &EntityType::SHEEP,
    ));
    let cube = MagmaCubeEntity::new(Entity::new(
        world,
        Vector3::new(10.0, 0.0, 0.0),
        &EntityType::MAGMA_CUBE,
    ));
    cube.slime.set_size(4, false);

    // Test flags: is_sneaking
    sheep
        .mob_entity
        .living_entity
        .entity
        .sneaking
        .store(true, Ordering::Relaxed);
    let sneaking_json = r#"{"pools":[{"entries":[{"type":"minecraft:item","name":"minecraft:diamond","conditions":[{"condition":"minecraft:entity_properties","entity":"this","predicate":{"flags":{"is_sneaking":true}}}]}]}]}"#;
    let table_sneaking = parse_loot_table(sneaking_json).unwrap();
    let params_sheep = LootContextParameters {
        this_entity: Some(sheep.as_ref()),
        ..Default::default()
    };
    assert_eq!(
        generate_dynamic_loot_with_context(&table_sneaking, 0, &params_sheep).len(),
        1
    );

    sheep
        .mob_entity
        .living_entity
        .entity
        .sneaking
        .store(false, Ordering::Relaxed);
    assert!(generate_dynamic_loot_with_context(&table_sneaking, 0, &params_sheep).is_empty());

    // Test distance: horizontal
    let dist_json = r#"{"pools":[{"entries":[{"type":"minecraft:item","name":"minecraft:diamond","conditions":[{"condition":"minecraft:entity_properties","entity":"this","predicate":{"distance":{"horizontal":{"min":9.0,"max":11.0}}}}]}]}]}"#;
    let table_dist = parse_loot_table(dist_json).unwrap();
    let params_cube = LootContextParameters {
        this_entity: Some(cube.as_ref()),
        position: Some(Vector3::default()),
        ..Default::default()
    };
    assert_eq!(
        generate_dynamic_loot_with_context(&table_dist, 0, &params_cube).len(),
        1
    );

    // Test type_specific: cube_mob size
    let size_json = r#"{"pools":[{"entries":[{"type":"minecraft:item","name":"minecraft:diamond","conditions":[{"condition":"minecraft:entity_properties","entity":"this","predicate":{"type_specific":{"type":"minecraft:slime","size":4}}}]}]}]}"#;
    let table_size = parse_loot_table(size_json).unwrap();
    assert_eq!(
        generate_dynamic_loot_with_context(&table_size, 0, &params_cube).len(),
        1
    );

    cube.slime.set_size(1, false);
    assert!(generate_dynamic_loot_with_context(&table_size, 0, &params_cube).is_empty());
}

#[test]
fn match_tool_evaluates_damage_durability_and_enchantments() {
    use pumpkin_data::Enchantment;

    let mut pickaxe = ItemStack::new(1, &Item::DIAMOND_PICKAXE);
    pickaxe.add_enchantment(&Enchantment::SILK_TOUCH, 1);
    pickaxe.set_damage(100);

    let json = r#"{"pools":[{"entries":[{"type":"minecraft:item","name":"minecraft:diamond","conditions":[{"condition":"minecraft:match_tool","predicate":{"items":"minecraft:diamond_pickaxe","predicates":{"minecraft:enchantments":[{"enchantments":"minecraft:silk_touch","levels":{"min":1}}],"minecraft:damage":{"damage":{"min":50,"max":150}}}}}]}]}]}"#;
    let table = parse_loot_table(json).unwrap();

    let params_matching = LootContextParameters {
        tool: Some(pickaxe),
        ..Default::default()
    };
    assert_eq!(
        generate_dynamic_loot_with_context(&table, 0, &params_matching).len(),
        1
    );

    let mut no_silk = ItemStack::new(1, &Item::DIAMOND_PICKAXE);
    no_silk.set_damage(100);
    let params_no_silk = LootContextParameters {
        tool: Some(no_silk),
        ..Default::default()
    };
    assert!(generate_dynamic_loot_with_context(&table, 0, &params_no_silk).is_empty());
}

#[test]
fn time_check_and_predicate_reference_evaluation() {
    let time_json = r#"{"pools":[{"entries":[{"type":"minecraft:item","name":"minecraft:diamond","conditions":[{"condition":"minecraft:time_check","period":24000,"value":{"min":6000,"max":18000}}]}]}]}"#;
    let table_time = parse_loot_table(time_json).unwrap();

    let day_params = LootContextParameters {
        world_time: 12000,
        ..Default::default()
    };
    assert_eq!(
        generate_dynamic_loot_with_context(&table_time, 0, &day_params).len(),
        1
    );

    let night_params = LootContextParameters {
        world_time: 20000,
        ..Default::default()
    };
    assert!(generate_dynamic_loot_with_context(&table_time, 0, &night_params).is_empty());

    let shears = ItemStack::new(1, &Item::SHEARS);
    let params_shears = LootContextParameters {
        tool: Some(shears),
        ..Default::default()
    };
    assert!(evaluate_predicate(
        "minecraft:tool/can_shear",
        &params_shears
    ));

    let stick = ItemStack::new(1, &Item::STICK);
    let params_stick = LootContextParameters {
        tool: Some(stick),
        ..Default::default()
    };
    assert!(!evaluate_predicate(
        "minecraft:tool/can_shear",
        &params_stick
    ));
}

#[test]
fn loot_fill_skips_occupied_slots() {
    let mut slots = vec![
        ItemStack::new(8, &Item::DIAMOND),
        ItemStack::EMPTY.clone(),
        ItemStack::EMPTY.clone(),
    ];
    place_items_in_slots(
        &mut slots,
        vec![ItemStack::new(1, &Item::STONE)],
        &mut Xoroshiro::from_seed(1),
    );
    assert_eq!(slots[0].item.id, Item::DIAMOND.id);
    assert_eq!(slots[0].item_count, 8);
    let placed = slots
        .iter()
        .filter(|stack| stack.item.id == Item::STONE.id)
        .count();
    assert_eq!(placed, 1);
}

#[test]
fn unpack_loot_into_slots_preserves_concurrent_inserts() {
    use std::sync::{Mutex, RwLock};

    let loot_table = Mutex::new(Some("minecraft:chests/simple_dungeon".to_string()));
    let items = RwLock::new([
        ItemStack::new(8, &Item::DIAMOND),
        ItemStack::EMPTY.clone(),
        ItemStack::EMPTY.clone(),
    ]);
    let key = unpack_loot_into_slots(&loot_table, 1, &items, |key, seed| {
        assert_eq!(key, "minecraft:chests/simple_dungeon");
        (
            vec![ItemStack::new(1, &Item::STONE)],
            RollRandom::seeded(seed),
        )
    });
    assert_eq!(key.as_deref(), Some("minecraft:chests/simple_dungeon"));
    assert!(loot_table.lock().unwrap().is_none());
    let slots = items.read().unwrap();
    assert_eq!(slots[0].item.id, Item::DIAMOND.id);
    assert_eq!(
        slots
            .iter()
            .filter(|stack| stack.item.id == Item::STONE.id)
            .count(),
        1
    );
    assert!(
        unpack_loot_into_slots(&loot_table, 1, &items, |_, seed| {
            (
                vec![ItemStack::new(1, &Item::GOLD_INGOT)],
                RollRandom::seeded(seed),
            )
        })
        .is_none()
    );
    assert!(
        items
            .read()
            .unwrap()
            .iter()
            .all(|stack| stack.item.id != Item::GOLD_INGOT.id)
    );
}

/// The slot that receives one stone in 27 empty slots, placed with `rng`.
fn stone_slot(rng: &mut Xoroshiro) -> usize {
    let mut slots = vec![ItemStack::EMPTY.clone(); 27];
    place_items_in_slots(&mut slots, vec![ItemStack::new(1, &Item::STONE)], rng);
    slots.iter().position(|stack| !stack.is_empty()).unwrap()
}

#[test]
fn fill_places_with_the_random_source_of_the_roll() {
    use std::sync::{Mutex, RwLock};

    let mut moved = false;
    for seed in 1..17 {
        let loot_table = Mutex::new(Some("minecraft:chests/simple_dungeon".to_string()));
        let items = RwLock::new(std::array::from_fn::<_, 27, _>(|_| {
            ItemStack::EMPTY.clone()
        }));
        // The roll draws once, as `LootTable.fill` rolls before it shuffles.
        unpack_loot_into_slots(&loot_table, seed, &items, |_, seed| {
            let mut random = RollRandom::seeded(seed);
            random.rng().next_i64();
            (vec![ItemStack::new(1, &Item::STONE)], random)
        });
        let placed = items
            .read()
            .unwrap()
            .iter()
            .position(|stack| !stack.is_empty())
            .unwrap();

        let mut after_roll = Xoroshiro::from_seed(seed as u64);
        after_roll.next_i64();
        assert_eq!(placed, stone_slot(&mut after_roll), "seed {seed}");
        moved |= placed != stone_slot(&mut Xoroshiro::from_seed(seed as u64));
    }
    assert!(moved, "no seed tells the two random sources apart");
}

/// Rolls every vanilla table with 15 seeds in 9 contexts and writes one line per roll to the file
/// that `LOOT_PARITY_DUMP` names. Diff the files of two builds to prove that a change to the loot
/// context leaves the loot of the vanilla tables alone:
/// `LOOT_PARITY_DUMP=target/scratch/parity.txt tools/cargo-slot.sh nextest run -p pumpkin-core --lib --run-ignored only vanilla_tables_dump`.
#[tokio::test]
#[ignore = "writes the dump that LOOT_PARITY_DUMP names"]
#[allow(clippy::too_many_lines)]
async fn vanilla_tables_dump() {
    use std::fmt::Write as _;

    let path = std::env::var("LOOT_PARITY_DUMP").expect("LOOT_PARITY_DUMP names the dump file");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/datapack/data/minecraft/loot_table");
    let mut keys = Vec::new();
    let mut dirs = vec![root.clone()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else if let Some(name) = path.strip_prefix(&root).unwrap().to_str()
                && let Some(name) = name.strip_suffix(".json")
            {
                keys.push(format!("minecraft:{name}"));
            }
        }
    }
    keys.sort();
    assert!(keys.len() > 1400, "{} tables", keys.len());

    let folder = tempfile::tempdir().unwrap();
    let world = world(&folder);
    let player = chicken(&world);
    let pot = crate::block::entities::decorated_pot::DecoratedPotBlockEntity::new(
        pumpkin_util::math::position::BlockPos::new(0, 0, 0),
    );
    let tool = |item: &'static Item| ItemStack::new(1, item);
    let contexts: Vec<(&str, LootContextParameters<'_>)> = vec![
        ("default", LootContextParameters::default()),
        (
            "pickaxe",
            LootContextParameters {
                block_state: Some(pumpkin_data::Block::STONE.default_state),
                tool: Some(tool(&Item::DIAMOND_PICKAXE)),
                ..Default::default()
            },
        ),
        (
            "shears",
            LootContextParameters {
                block_state: Some(pumpkin_data::Block::OAK_LEAVES.default_state),
                tool: Some(tool(&Item::SHEARS)),
                ..Default::default()
            },
        ),
        (
            "empty_tool",
            LootContextParameters {
                tool: Some(ItemStack::EMPTY.clone()),
                ..Default::default()
            },
        ),
        (
            "luck",
            LootContextParameters {
                luck: 3.0,
                ..Default::default()
            },
        ),
        (
            "player_kill",
            LootContextParameters {
                last_damage_player: Some(player.as_ref()),
                damage_type: Some(DamageType::PLAYER_ATTACK),
                position: Some(Vector3::default()),
                damage_source: Some(LootDamageSource {
                    damage_type: DamageType::PLAYER_ATTACK,
                    attacker: None,
                    direct: None,
                }),
                direct_attacker_weapon: Some(tool(&Item::DIAMOND_SWORD)),
                ..Default::default()
            },
        ),
        (
            "explosion",
            LootContextParameters {
                explosion_radius: Some(4.0),
                block_state: Some(pumpkin_data::Block::STONE.default_state),
                ..Default::default()
            },
        ),
        (
            "fire_kill",
            LootContextParameters {
                is_on_fire: Some(true),
                damage_type: Some(DamageType::ON_FIRE),
                position: Some(Vector3::default()),
                damage_source: Some(LootDamageSource {
                    damage_type: DamageType::ON_FIRE,
                    attacker: None,
                    direct: None,
                }),
                ..Default::default()
            },
        ),
        (
            "rain",
            LootContextParameters {
                is_raining: Some(true),
                is_thundering: Some(true),
                world_time: 6000,
                ..Default::default()
            },
        ),
        // The contexts below hold the parameters that issue 171 added. The dump of a build
        // without them has no line for these names.
        (
            "last_player",
            LootContextParameters {
                last_damage_player: Some(player.as_ref()),
                luck: 1.0,
                ..Default::default()
            },
        ),
        (
            "attacker_source",
            LootContextParameters {
                position: Some(Vector3::default()),
                damage_source: Some(LootDamageSource {
                    damage_type: DamageType::PLAYER_ATTACK,
                    attacker: Some(player.as_ref()),
                    direct: Some(player.as_ref()),
                }),
                last_damage_player: Some(player.as_ref()),
                ..Default::default()
            },
        ),
        (
            "fire_source",
            LootContextParameters {
                position: Some(Vector3::default()),
                damage_source: Some(LootDamageSource {
                    damage_type: DamageType::IN_FIRE,
                    attacker: None,
                    direct: None,
                }),
                ..Default::default()
            },
        ),
        (
            "block_entity",
            LootContextParameters {
                block_state: Some(pumpkin_data::Block::DECORATED_POT.default_state),
                block_entity: Some(&pot),
                tool: Some(tool(&Item::DIAMOND_PICKAXE)),
                ..Default::default()
            },
        ),
    ];

    let mut dump = String::new();
    for key in &keys {
        let handle = get_loot_table(key).unwrap_or_else(|| panic!("no table {key}"));
        for (name, params) in &contexts {
            for seed in 0..15 {
                write!(dump, "{key} {name} {seed}:").unwrap();
                for stack in generate_loot_from_handle(&handle, seed, params) {
                    write!(dump, " {}*{}", stack.item.registry_key, stack.item_count).unwrap();
                }
                dump.push('\n');
            }
        }
    }
    std::fs::write(path, dump).unwrap();
}

/// A table of one stone behind `condition`.
fn stone_if(condition: &str) -> DynamicLootTable {
    parse_loot_table(&format!(
        r#"{{"pools":[{{"rolls":1,"entries":[{{"type":"minecraft:item","name":"minecraft:stone"}}],"conditions":[{condition}]}}]}}"#
    ))
    .unwrap()
}

fn passes(condition: &str, params: &LootContextParameters<'_>) -> bool {
    !generate_dynamic_loot_with_context(&stone_if(condition), 1, params).is_empty()
}

fn chicken(world: &std::sync::Arc<World>) -> std::sync::Arc<ChickenEntity> {
    ChickenEntity::new(Entity::new(
        world.clone(),
        Vector3::default(),
        &EntityType::CHICKEN,
    ))
}

#[tokio::test]
async fn killed_by_player_passes_for_a_last_damage_player() {
    let folder = tempfile::tempdir().unwrap();
    let world = world(&folder);
    let player = chicken(&world);
    let condition = r#"{"condition":"minecraft:killed_by_player"}"#;
    assert!(!passes(condition, &LootContextParameters::default()));
    assert!(passes(
        condition,
        &LootContextParameters {
            last_damage_player: Some(player.as_ref()),
            ..Default::default()
        }
    ));
}

#[tokio::test]
async fn entity_targets_read_the_interaction_parameters() {
    let folder = tempfile::tempdir().unwrap();
    let world = world(&folder);
    let chicken = chicken(&world);
    let entity: &dyn EntityBase = chicken.as_ref();
    for target in ["attacking_player", "target_entity", "interacting_entity"] {
        let condition = format!(
            r#"{{"condition":"minecraft:entity_properties","entity":"{target}","predicate":{{"type":"minecraft:chicken"}}}}"#
        );
        let with = |params: LootContextParameters<'_>| passes(&condition, &params);
        assert!(!with(LootContextParameters::default()), "{target}");
        let set = match target {
            "attacking_player" => LootContextParameters {
                last_damage_player: Some(entity),
                ..Default::default()
            },
            "target_entity" => LootContextParameters {
                target_entity: Some(entity),
                ..Default::default()
            },
            _ => LootContextParameters {
                interacting_entity: Some(entity),
                ..Default::default()
            },
        };
        assert!(with(set), "{target}");
        // Another parameter does not stand in for the target.
        let other = LootContextParameters {
            this_entity: Some(entity),
            ..Default::default()
        };
        assert!(!with(other), "{target}");
    }
}

#[tokio::test]
async fn the_damage_source_feeds_the_damage_conditions() {
    let folder = tempfile::tempdir().unwrap();
    let world = world(&folder);
    let chicken = chicken(&world);
    let fire = r#"{"condition":"minecraft:damage_source_properties","predicate":{"tags":[{"id":"minecraft:is_fire","expected":true}]}}"#;
    let attacker = r#"{"condition":"minecraft:damage_source_properties","predicate":{"source_entity":{"type":"minecraft:chicken"}}}"#;
    let source = |damage_type, attacker| LootContextParameters {
        position: Some(Vector3::default()),
        damage_source: Some(LootDamageSource {
            damage_type,
            attacker,
            direct: None,
        }),
        ..Default::default()
    };
    assert!(!passes(fire, &LootContextParameters::default()));
    assert!(passes(fire, &source(DamageType::ON_FIRE, None)));
    assert!(!passes(fire, &source(DamageType::PLAYER_ATTACK, None)));
    assert!(!passes(attacker, &source(DamageType::PLAYER_ATTACK, None)));
    assert!(passes(
        attacker,
        &source(DamageType::PLAYER_ATTACK, Some(chicken.as_ref()))
    ));
}

/// Review 171: `DamageSourceCondition.test` fails without a damage source or an origin.
#[test]
fn damage_source_properties_need_the_damage_source_and_the_origin() {
    // An empty predicate passes in vanilla when both exist.
    let any = r#"{"condition":"minecraft:damage_source_properties","predicate":{}}"#;
    let source = Some(LootDamageSource {
        damage_type: DamageType::ON_FIRE,
        attacker: None,
        direct: None,
    });
    let origin = Some(Vector3::default());
    assert!(passes(
        any,
        &LootContextParameters {
            position: origin,
            damage_source: source,
            ..Default::default()
        }
    ));
    for (position, damage_source) in [(None, source), (origin, None), (None, None)] {
        // The damage type alone is not a damage source.
        let params = LootContextParameters {
            position,
            damage_source,
            damage_type: Some(DamageType::ON_FIRE),
            ..Default::default()
        };
        assert!(
            !passes(any, &params),
            "{position:?} {}",
            damage_source.is_some()
        );
    }
}

/// Review 171: `DamageSource.isDirect` is true when the causing entity is the direct entity, and
/// reads the damage source for both, as the other keys do.
#[tokio::test]
async fn is_direct_compares_the_causing_and_the_direct_entity() {
    fn source<'a>(
        attacker: Option<&'a dyn EntityBase>,
        direct: Option<&'a dyn EntityBase>,
    ) -> LootContextParameters<'a> {
        LootContextParameters {
            position: Some(Vector3::default()),
            damage_source: Some(LootDamageSource {
                damage_type: DamageType::PLAYER_ATTACK,
                attacker,
                direct,
            }),
            ..Default::default()
        }
    }

    let folder = tempfile::tempdir().unwrap();
    let world = world(&folder);
    let first = chicken(&world);
    let second = chicken(&world);
    let direct =
        r#"{"condition":"minecraft:damage_source_properties","predicate":{"is_direct":true}}"#;
    let (a, b) = (
        first.as_ref() as &dyn EntityBase,
        second.as_ref() as &dyn EntityBase,
    );
    assert!(passes(direct, &source(None, None)));
    assert!(passes(direct, &source(Some(a), Some(a))));
    assert!(!passes(direct, &source(Some(a), Some(b))));
    // The attacker without a direct entity, and a direct entity without an attacker.
    assert!(!passes(direct, &source(Some(a), None)));
    assert!(!passes(direct, &source(None, Some(a))));
}
