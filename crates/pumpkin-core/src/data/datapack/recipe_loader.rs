use pumpkin_data::recipes::RecipeCategoryTypes;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_protocol::codec::recipe::{
    DynamicRecipe, OwnedBrewingRecipe, OwnedCookingRecipe, OwnedCookingRecipeType,
    OwnedCraftingRecipe, OwnedRecipeIngredient, OwnedRecipeResult, OwnedSmithingTransformRecipe,
    OwnedSmithingTrimRecipe, OwnedSpecialCraftingRecipe, OwnedStonecutterRecipe,
};
use serde_json::Value;

pub fn parse_recipe(namespace: &str, name: &str, json_str: &str) -> Option<DynamicRecipe> {
    let value: Value = serde_json::from_str(json_str).ok()?;
    let recipe_id = format!("{namespace}:{name}");
    let raw_type = value.get("type")?.as_str()?;
    let recipe_type = raw_type.strip_prefix("minecraft:").unwrap_or(raw_type);

    match recipe_type {
        "crafting_shaped" => parse_shaped(recipe_id, &value).map(DynamicRecipe::Crafting),
        "crafting_shapeless" => parse_shapeless(recipe_id, &value).map(DynamicRecipe::Crafting),
        "crafting_transmute" => parse_transmute(recipe_id, &value).map(DynamicRecipe::Crafting),
        "crafting_dye" => parse_dye(recipe_id, &value).map(DynamicRecipe::Crafting),
        "crafting_decorated_pot" => {
            parse_decorated_pot(recipe_id, &value).map(DynamicRecipe::Crafting)
        }
        "crafting_imbue" => parse_imbue(recipe_id, &value).map(DynamicRecipe::Crafting),
        "stonecutting" => parse_stonecutting(recipe_id, &value).map(DynamicRecipe::Stonecutting),
        "smithing_transform" => {
            parse_smithing_transform(recipe_id, &value).map(DynamicRecipe::SmithingTransform)
        }
        "smithing_trim" => parse_smithing_trim(recipe_id, &value).map(DynamicRecipe::SmithingTrim),
        s if s.starts_with("crafting_special_") => {
            let category = parse_category(value.get("category"));
            Some(DynamicRecipe::SpecialCrafting(OwnedSpecialCraftingRecipe {
                recipe_id,
                category,
            }))
        }
        "smelting" => parse_cooking(recipe_id, &value, 200)
            .map(|c| DynamicRecipe::Cooking(OwnedCookingRecipeType::Smelting(c))),
        "blasting" => parse_cooking(recipe_id, &value, 100)
            .map(|c| DynamicRecipe::Cooking(OwnedCookingRecipeType::Blasting(c))),
        "smoking" => parse_cooking(recipe_id, &value, 100)
            .map(|c| DynamicRecipe::Cooking(OwnedCookingRecipeType::Smoking(c))),
        "campfire_cooking" => parse_cooking(recipe_id, &value, 600)
            .map(|c| DynamicRecipe::Cooking(OwnedCookingRecipeType::CampfireCooking(c))),
        "brewing" => parse_brewing(recipe_id, &value).map(DynamicRecipe::Brewing),
        _ => None,
    }
}

fn normalize_id(id: &str) -> String {
    if id.contains(':') {
        id.to_string()
    } else {
        format!("minecraft:{id}")
    }
}

fn parse_ingredient(value: &Value) -> Option<OwnedRecipeIngredient> {
    match value {
        // `HolderSetCodec` reads a string that starts with `#` as a tag.
        Value::String(s) => Some(s.strip_prefix('#').map_or_else(
            || OwnedRecipeIngredient::Simple(normalize_id(s)),
            |tag| OwnedRecipeIngredient::Tagged(normalize_id(tag)),
        )),
        Value::Object(map) => map
            .get("tag")
            .and_then(Value::as_str)
            .map(|tag| OwnedRecipeIngredient::Tagged(normalize_id(tag)))
            .or_else(|| {
                map.get("item")
                    .and_then(Value::as_str)
                    .map(|item| OwnedRecipeIngredient::Simple(normalize_id(item)))
            }),
        Value::Array(arr) => {
            let mut ids = Vec::new();
            for item in arr {
                // `HolderSetCodec` reads a list of item ids only: a tag, an object (the legacy
                // `{"item": ..}` form too) or any other value in it fails the recipe.
                let id = item.as_str().filter(|id| !id.starts_with('#'))?;
                ids.push(normalize_id(id));
            }
            if ids.is_empty() {
                None
            } else {
                Some(OwnedRecipeIngredient::OneOf(ids))
            }
        }
        _ => None,
    }
}

fn parse_result(value: &Value) -> Option<OwnedRecipeResult> {
    match value {
        Value::String(s) => Some(OwnedRecipeResult {
            item_id: normalize_id(s),
            count: 1,
            components: None,
        }),
        Value::Object(map) => {
            let id = map
                .get("id")
                .or_else(|| map.get("item"))
                .and_then(Value::as_str)?;
            // Vanilla `ItemStackTemplate.MAP_CODEC` takes a count from 1 to 99
            // (`ItemStackTemplate.java:24`) and fails the recipe on any other value.
            let count = match map.get("count") {
                None => 1,
                Some(count) => {
                    let count = count.as_f64()? as i64;
                    u8::try_from(count)
                        .ok()
                        .filter(|count| (1..=99).contains(count))?
                }
            };
            let components = map.get("components").and_then(|components| {
                match super::dynamic_registry_loader::json_to_nbt_tag(components) {
                    NbtTag::Compound(compound) => Some(compound),
                    _ => None,
                }
            });
            Some(OwnedRecipeResult {
                item_id: normalize_id(id),
                count,
                components,
            })
        }
        _ => None,
    }
}

fn parse_category(value: Option<&Value>) -> RecipeCategoryTypes {
    let Some(val) = value.and_then(Value::as_str) else {
        return RecipeCategoryTypes::Misc;
    };
    match val.to_ascii_lowercase().as_str() {
        "equipment" => RecipeCategoryTypes::Equipment,
        "building" => RecipeCategoryTypes::Building,
        "redstone" | "restone" => RecipeCategoryTypes::Restone,
        "food" => RecipeCategoryTypes::Food,
        "blocks" => RecipeCategoryTypes::Blocks,
        _ => RecipeCategoryTypes::Misc,
    }
}

/// Vanilla `ShapedRecipePattern.MAX_SIZE`.
const MAX_SIZE: usize = 3;

/// Vanilla `ShapedRecipePattern.shrink`. Returns `None` for patterns vanilla rejects: more than
/// three rows or columns, rows of different widths, or no ingredients.
fn shrink(pattern: &[&str]) -> Option<Vec<String>> {
    if pattern.len() > MAX_SIZE {
        return None;
    }
    let rows: Vec<Vec<char>> = pattern.iter().map(|row| row.chars().collect()).collect();
    let width = rows.first()?.len();
    if width > MAX_SIZE || rows.iter().any(|row| row.len() != width) {
        return None;
    }

    let is_filled = |c: &char| *c != ' ';
    let top = rows.iter().position(|row| row.iter().any(is_filled))?;
    let bottom = rows.iter().rposition(|row| row.iter().any(is_filled))?;
    let left = rows
        .iter()
        .filter_map(|row| row.iter().position(is_filled))
        .min()?;
    let right = rows
        .iter()
        .filter_map(|row| row.iter().rposition(is_filled))
        .max()?;
    Some(
        rows[top..=bottom]
            .iter()
            .map(|row| row[left..=right].iter().collect())
            .collect(),
    )
}

fn parse_shaped(recipe_id: String, value: &Value) -> Option<OwnedCraftingRecipe> {
    let pattern = value
        .get("pattern")?
        .as_array()?
        .iter()
        .map(Value::as_str)
        .collect::<Option<Vec<_>>>()?;
    let pattern = shrink(&pattern)?;

    let key_map = value.get("key")?.as_object()?;
    let mut key = Vec::new();
    for (k, v) in key_map {
        let ch = k.chars().next()?;
        let ingredient = parse_ingredient(v)?;
        key.push((ch, ingredient));
    }

    let result = parse_result(value.get("result")?)?;
    let category = parse_category(value.get("category"));
    let group = value
        .get("group")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let show_notification = value
        .get("show_notification")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    Some(OwnedCraftingRecipe::Shaped {
        recipe_id: Some(recipe_id),
        category,
        group,
        show_notification,
        key,
        pattern,
        result,
    })
}

fn parse_shapeless(recipe_id: String, value: &Value) -> Option<OwnedCraftingRecipe> {
    let ingredients_arr = value.get("ingredients")?.as_array()?;
    let mut ingredients = Vec::new();
    for ing_val in ingredients_arr {
        ingredients.push(parse_ingredient(ing_val)?);
    }

    let result = parse_result(value.get("result")?)?;
    let category = parse_category(value.get("category"));
    let group = value
        .get("group")
        .and_then(Value::as_str)
        .map(ToString::to_string);

    Some(OwnedCraftingRecipe::Shapeless {
        recipe_id: Some(recipe_id),
        category,
        group,
        ingredients,
        result,
    })
}

fn parse_cooking(
    recipe_id: String,
    value: &Value,
    default_cooking_time: i32,
) -> Option<OwnedCookingRecipe> {
    let ingredient = parse_ingredient(value.get("ingredient")?)?;
    let result = parse_result(value.get("result")?)?;
    let category = parse_category(value.get("category"));
    let group = value
        .get("group")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let cooking_time = value
        .get("cookingtime")
        .and_then(Value::as_i64)
        .map_or(default_cooking_time, |t| t as i32);
    let experience = value
        .get("experience")
        .and_then(Value::as_f64)
        .map_or(0.0, |e| e as f32);

    Some(OwnedCookingRecipe {
        recipe_id,
        category,
        group,
        ingredient,
        cooking_time,
        experience,
        result,
    })
}

fn parse_brewing(recipe_id: String, value: &Value) -> Option<OwnedBrewingRecipe> {
    let input = value.get("input")?;
    let input_item = parse_ingredient(input.get("item")?)?;
    let input_potion = input
        .get("potion_contents")
        .and_then(|pc| pc.get("potions").or_else(|| pc.get("potion")))
        .and_then(Value::as_str)
        .map(normalize_id);

    let reagent = parse_ingredient(value.get("reagent")?.get("item")?)?;

    let output = value.get("output")?;
    let output_item = normalize_id(output.get("id").or_else(|| output.get("item"))?.as_str()?);
    let output_potion = output
        .get("components")
        .and_then(|c| c.get("minecraft:potion_contents"))
        .and_then(|pc| pc.get("potion"))
        .and_then(Value::as_str)
        .map(normalize_id);

    Some(OwnedBrewingRecipe {
        recipe_id,
        input_item,
        input_potion,
        reagent,
        output_item,
        output_potion,
    })
}

fn parse_transmute(recipe_id: String, value: &Value) -> Option<OwnedCraftingRecipe> {
    let category = parse_category(value.get("category"));
    let group = value
        .get("group")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let input = parse_ingredient(value.get("input")?)?;
    let material = parse_ingredient(value.get("material")?)?;
    let result = parse_result(value.get("result")?).unwrap_or(OwnedRecipeResult {
        item_id: "minecraft:filled_map".to_string(),
        count: 1,
        components: None,
    });
    Some(OwnedCraftingRecipe::Shapeless {
        recipe_id: Some(recipe_id),
        category,
        group,
        ingredients: vec![input, material],
        result,
    })
}

fn parse_dye(recipe_id: String, value: &Value) -> Option<OwnedCraftingRecipe> {
    let category = parse_category(value.get("category"));
    let group = value
        .get("group")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let target = parse_ingredient(value.get("target")?)?;
    let dye = parse_ingredient(value.get("dye")?)?;
    let result = parse_result(value.get("result")?)?;
    Some(OwnedCraftingRecipe::Shapeless {
        recipe_id: Some(recipe_id),
        category,
        group,
        ingredients: vec![target, dye],
        result,
    })
}

fn parse_decorated_pot(recipe_id: String, value: &Value) -> Option<OwnedCraftingRecipe> {
    let category = parse_category(value.get("category"));
    let group = value
        .get("group")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let back = parse_ingredient(value.get("back")?)?;
    let left = parse_ingredient(value.get("left")?)?;
    let right = parse_ingredient(value.get("right")?)?;
    let front = parse_ingredient(value.get("front")?)?;
    let result = parse_result(value.get("result")?)?;
    Some(OwnedCraftingRecipe::Shaped {
        recipe_id: Some(recipe_id),
        category,
        group,
        show_notification: true,
        key: vec![('B', back), ('L', left), ('R', right), ('F', front)],
        pattern: vec![" B ".to_string(), "L R".to_string(), " F ".to_string()],
        result,
    })
}

fn parse_imbue(recipe_id: String, value: &Value) -> Option<OwnedCraftingRecipe> {
    let category = parse_category(value.get("category"));
    let group = value
        .get("group")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let material = parse_ingredient(value.get("material")?)?;
    let source = parse_ingredient(value.get("source")?)?;
    let result = parse_result(value.get("result")?)?;
    Some(OwnedCraftingRecipe::Shaped {
        recipe_id: Some(recipe_id),
        category,
        group,
        show_notification: true,
        key: vec![('#', material), ('X', source)],
        pattern: vec!["###".to_string(), "#X#".to_string(), "###".to_string()],
        result,
    })
}

fn parse_stonecutting(recipe_id: String, value: &Value) -> Option<OwnedStonecutterRecipe> {
    let group = value
        .get("group")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let ingredient = parse_ingredient(value.get("ingredient")?)?;
    let result = parse_result(value.get("result")?)?;
    Some(OwnedStonecutterRecipe {
        recipe_id,
        group,
        ingredient,
        result,
    })
}

fn parse_smithing_transform(
    recipe_id: String,
    value: &Value,
) -> Option<OwnedSmithingTransformRecipe> {
    // Vanilla `SmithingTransformRecipe.MAP_CODEC`: `template` and `addition` are optional.
    let optional = |field: &str| value.get(field).map(parse_ingredient);
    let template = optional("template").map_or(Some(None), |parsed| parsed.map(Some))?;
    let base = parse_ingredient(value.get("base")?)?;
    let addition = optional("addition").map_or(Some(None), |parsed| parsed.map(Some))?;
    let result = parse_result(value.get("result")?)?;
    Some(OwnedSmithingTransformRecipe {
        recipe_id,
        template,
        base,
        addition,
        result,
    })
}

fn parse_smithing_trim(recipe_id: String, value: &Value) -> Option<OwnedSmithingTrimRecipe> {
    let template = parse_ingredient(value.get("template")?)?;
    let base = parse_ingredient(value.get("base")?)?;
    let addition = parse_ingredient(value.get("addition")?)?;
    let pattern = value.get("pattern")?.as_str()?.to_string();
    Some(OwnedSmithingTrimRecipe {
        recipe_id,
        template,
        base,
        addition,
        pattern: normalize_id(&pattern),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_data::item::Item;

    #[test]
    fn parse_shaped_recipe() {
        let json = r##"{
            "type": "minecraft:crafting_shaped",
            "group": "sword",
            "pattern": [" X ", " X ", " # "],
            "key": {
                "#": "minecraft:stick",
                "X": "minecraft:diamond"
            },
            "result": {
                "id": "minecraft:diamond_sword",
                "count": 1
            }
        }"##;

        let parsed = parse_recipe("test", "diamond_sword", json);
        assert!(parsed.is_some());
        if let Some(DynamicRecipe::Crafting(OwnedCraftingRecipe::Shaped {
            recipe_id,
            result,
            pattern,
            ..
        })) = parsed
        {
            assert_eq!(recipe_id.as_deref(), Some("test:diamond_sword"));
            assert_eq!(result.item_id, "minecraft:diamond_sword");
            assert_eq!(pattern, ["X", "X", "#"]);
        } else {
            panic!("Expected shaped recipe");
        }
    }

    #[test]
    fn shaped_pattern_is_shrunk_like_vanilla() {
        assert_eq!(
            shrink(&[" # ", " I "]),
            Some(vec!["#".to_string(), "I".to_string()])
        );
        assert_eq!(
            shrink(&["###", "   ", "###"]),
            Some(vec![
                "###".to_string(),
                "   ".to_string(),
                "###".to_string()
            ])
        );
        assert_eq!(shrink(&["   ", "  #"]), Some(vec!["#".to_string()]));
        assert_eq!(shrink(&["##", "#"]), None);
        assert_eq!(shrink(&["   "]), None);
        assert_eq!(shrink(&["  # "]), None);
        assert_eq!(shrink(&["#", "#", "#", "#"]), None);
    }

    #[test]
    fn parse_shapeless_recipe() {
        let json = r#"{
            "type": "minecraft:crafting_shapeless",
            "ingredients": ["minecraft:oak_log"],
            "result": {
                "id": "minecraft:oak_planks",
                "count": 4
            }
        }"#;

        let parsed = parse_recipe("minecraft", "oak_planks_from_oak_log", json);
        assert!(parsed.is_some());
        if let Some(DynamicRecipe::Crafting(OwnedCraftingRecipe::Shapeless {
            recipe_id,
            result,
            ingredients,
            ..
        })) = parsed
        {
            assert_eq!(
                recipe_id.as_deref(),
                Some("minecraft:oak_planks_from_oak_log")
            );
            assert_eq!(result.item_id, "minecraft:oak_planks");
            assert_eq!(result.count, 4);
            assert_eq!(ingredients.len(), 1);
        } else {
            panic!("Expected shapeless recipe");
        }
    }

    #[test]
    fn parse_smelting_recipe() {
        let json = r#"{
            "type": "minecraft:smelting",
            "ingredient": "minecraft:raw_iron",
            "result": {
                "id": "minecraft:iron_ingot"
            },
            "experience": 0.7,
            "cookingtime": 200
        }"#;

        let parsed = parse_recipe("minecraft", "iron_ingot_from_smelting_raw_iron", json);
        assert!(parsed.is_some());
        if let Some(DynamicRecipe::Cooking(OwnedCookingRecipeType::Smelting(cooking))) = parsed {
            assert_eq!(
                cooking.recipe_id,
                "minecraft:iron_ingot_from_smelting_raw_iron"
            );
            assert_eq!(cooking.result.item_id, "minecraft:iron_ingot");
            assert_eq!(cooking.cooking_time, 200);
        } else {
            panic!("Expected smelting recipe");
        }
    }

    #[test]
    fn parse_brewing_recipe() {
        let json = r#"{
            "type": "minecraft:brewing",
            "input": {
                "item": "minecraft:potion",
                "potion_contents": {
                    "potions": "minecraft:water"
                }
            },
            "output": {
                "components": {
                    "minecraft:potion_contents": {
                        "potion": "minecraft:awkward"
                    }
                },
                "id": "minecraft:potion"
            },
            "reagent": {
                "item": "minecraft:nether_wart"
            }
        }"#;

        let parsed = parse_recipe("minecraft", "brewing/potion_water_nether_wart", json);
        assert!(parsed.is_some());
        if let Some(DynamicRecipe::Brewing(brewing)) = parsed {
            assert_eq!(
                brewing.recipe_id,
                "minecraft:brewing/potion_water_nether_wart"
            );
            assert_eq!(
                brewing.input_item,
                OwnedRecipeIngredient::Simple("minecraft:potion".to_string())
            );
            assert_eq!(brewing.input_potion.as_deref(), Some("minecraft:water"));
            assert_eq!(
                brewing.reagent,
                OwnedRecipeIngredient::Simple("minecraft:nether_wart".to_string())
            );
            assert_eq!(brewing.output_item, "minecraft:potion");
            assert_eq!(brewing.output_potion.as_deref(), Some("minecraft:awkward"));
        } else {
            panic!("Expected brewing recipe");
        }
    }

    #[test]
    fn a_tag_inside_an_ingredient_list_fails_the_recipe() {
        let json = r##"{
            "type": "minecraft:smelting",
            "ingredient": ["#c:ingots", "minecraft:stick"],
            "result": { "id": "minecraft:iron_nugget" }
        }"##;
        assert!(parse_recipe("test", "bad_list", json).is_none());
    }

    #[test]
    fn a_tag_object_or_a_number_in_an_ingredient_list_fails_the_recipe() {
        for entry in [r#"{ "tag": "minecraft:planks" }"#, "5"] {
            let json = format!(
                r#"{{
                    "type": "minecraft:smelting",
                    "ingredient": ["minecraft:stick", {entry}],
                    "result": {{ "id": "minecraft:iron_nugget" }}
                }}"#
            );
            assert!(
                parse_recipe("test", "bad_entry", &json).is_none(),
                "{entry}"
            );
        }
    }

    /// The ingredient codec of vanilla 26.3 takes an id, a tag string or a list of ids. The
    /// legacy `{"item": ..}` object is none of them.
    #[test]
    fn a_legacy_item_object_inside_an_ingredient_list_fails_the_recipe() {
        let json = r#"{
            "type": "minecraft:smelting",
            "ingredient": ["minecraft:stick", { "item": "minecraft:dirt" }],
            "result": { "id": "minecraft:iron_nugget" }
        }"#;
        assert!(parse_recipe("test", "legacy_list", json).is_none());
    }

    #[test]
    fn a_result_count_outside_1_to_99_fails_the_recipe() {
        let recipe = |count: &str| {
            let json = format!(
                r#"{{
                    "type": "minecraft:smelting",
                    "ingredient": "minecraft:dirt",
                    "result": {{ "id": "minecraft:diamond", "count": {count} }}
                }}"#
            );
            parse_recipe("test", "count", &json)
        };
        assert!(recipe("0").is_none());
        assert!(recipe("100").is_none());
        assert!(recipe("-1").is_none());
        assert!(recipe("\"a\"").is_none());
        let Some(DynamicRecipe::Cooking(OwnedCookingRecipeType::Smelting(cooking))) = recipe("99")
        else {
            panic!("Expected smelting recipe");
        };
        assert_eq!(cooking.result.count, 99);
    }

    #[test]
    fn a_smithing_transform_can_leave_out_the_template_and_the_addition() {
        let json = r#"{
            "type": "minecraft:smithing_transform",
            "base": "minecraft:iron_sword",
            "result": { "id": "minecraft:diamond_sword" }
        }"#;
        let Some(DynamicRecipe::SmithingTransform(recipe)) = parse_recipe("test", "plain", json)
        else {
            panic!("Expected smithing transform recipe");
        };
        assert_eq!(recipe.template, None);
        assert_eq!(recipe.addition, None);
        assert!(OwnedRecipeIngredient::match_optional(None, &Item::AIR));
        assert!(!OwnedRecipeIngredient::match_optional(None, &Item::STICK));
    }

    #[test]
    fn a_result_keeps_its_components() {
        let json = r#"{
            "type": "minecraft:smelting",
            "ingredient": "minecraft:dirt",
            "result": {
                "id": "minecraft:diamond",
                "components": { "minecraft:max_stack_size": 7 }
            }
        }"#;
        let Some(DynamicRecipe::Cooking(OwnedCookingRecipeType::Smelting(cooking))) =
            parse_recipe("test", "shiny", json)
        else {
            panic!("Expected smelting recipe");
        };
        let components = cooking
            .result
            .components
            .expect("the result has components");
        assert_eq!(components.get_int("minecraft:max_stack_size"), Some(7));
    }

    /// JSON numbers have no NBT type. Vanilla codecs read any number, so a decimal saturation
    /// and a whole food nutrition both load, and one component does not drop the others.
    #[test]
    fn json_numbers_decode_as_the_component_types_expect() {
        let json = r#"{
            "type": "minecraft:smelting",
            "ingredient": "minecraft:dirt",
            "result": {
                "id": "minecraft:diamond",
                "components": {
                    "minecraft:food": { "nutrition": 4, "saturation": 1.5 },
                    "minecraft:max_stack_size": 7.0
                }
            }
        }"#;
        let Some(DynamicRecipe::Cooking(OwnedCookingRecipeType::Smelting(cooking))) =
            parse_recipe("test", "food", json)
        else {
            panic!("Expected smelting recipe");
        };
        let stack =
            pumpkin_inventory::recipe_index::RecipeOutput::owned(&cooking.result).assemble();
        assert_eq!(stack.get_max_stack_size(), 7);
        let food = stack
            .get_data_component::<pumpkin_data::data_component_impl::FoodImpl>()
            .expect("the result has the food component");
        assert_eq!(food.nutrition, 4);
        assert!((food.saturation - 1.5).abs() < f32::EPSILON);
    }

    #[test]
    fn tag_string_ingredient_is_a_tag() {
        let json = r##"{
            "type": "minecraft:smelting",
            "ingredient": "#c:ingots",
            "result": { "id": "minecraft:iron_nugget" }
        }"##;
        let Some(DynamicRecipe::Cooking(OwnedCookingRecipeType::Smelting(cooking))) =
            parse_recipe("test", "ingots", json)
        else {
            panic!("Expected smelting recipe");
        };
        assert_eq!(
            cooking.ingredient,
            OwnedRecipeIngredient::Tagged("c:ingots".to_string())
        );
        assert!(cooking.ingredient.match_item(&Item::IRON_INGOT));
        assert!(!cooking.ingredient.match_item(&Item::STICK));
    }

    #[test]
    fn tag_string_in_ingredient_list_is_a_tag() {
        let json = r##"{
            "type": "minecraft:crafting_shapeless",
            "ingredients": ["#c:ingots", "minecraft:stick"],
            "result": { "id": "minecraft:iron_nugget" }
        }"##;
        let Some(DynamicRecipe::Crafting(OwnedCraftingRecipe::Shapeless { ingredients, .. })) =
            parse_recipe("test", "mix", json)
        else {
            panic!("Expected shapeless recipe");
        };
        assert_eq!(
            ingredients,
            vec![
                OwnedRecipeIngredient::Tagged("c:ingots".to_string()),
                OwnedRecipeIngredient::Simple("minecraft:stick".to_string()),
            ]
        );
    }

    #[test]
    fn brewing_reagent_accepts_a_tag_string() {
        let json = r##"{
            "type": "minecraft:brewing",
            "input": { "item": "minecraft:potion" },
            "output": { "id": "minecraft:potion" },
            "reagent": { "item": "#c:ingots" }
        }"##;
        let Some(DynamicRecipe::Brewing(brewing)) = parse_recipe("test", "ingot_potion", json)
        else {
            panic!("Expected brewing recipe");
        };
        assert!(brewing.reagent.match_item(&Item::GOLD_INGOT));
        assert!(!brewing.reagent.match_item(&Item::NETHER_WART));
    }
}
