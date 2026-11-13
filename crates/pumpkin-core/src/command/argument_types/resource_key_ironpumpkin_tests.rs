use super::*;
use arc_swap::ArcSwap;
use pumpkin_config::world::LevelConfig;
use pumpkin_data::dimension::Dimension;
use pumpkin_util::world_seed::Seed;
use pumpkin_world::dimension::into_level;
use pumpkin_world::world_info::LevelData;
use std::sync::Weak;

fn loaded_worlds(root: &std::path::Path, dimensions: &[Dimension]) -> Vec<Arc<World>> {
    let level_info = Arc::new(ArcSwap::from_pointee(LevelData::default(Seed(0))));
    let block_registry = crate::block::registry::default_registry();
    dimensions
        .iter()
        .map(|dimension| {
            let level = into_level(
                dimension.clone(),
                &LevelConfig::default(),
                root.to_path_buf(),
                0,
            );
            Arc::new(World::load(
                level,
                level_info.clone(),
                dimension.clone(),
                block_registry.clone(),
                Weak::new(),
            ))
        })
        .collect()
}

#[tokio::test]
async fn resolves_loaded_dimensions_only() {
    let root = tempfile::TempDir::new().unwrap();
    let worlds = loaded_worlds(root.path(), &[Dimension::OVERWORLD, Dimension::THE_NETHER]);

    let nether = get_level(&worlds, &Identifier::vanilla_static("the_nether")).unwrap();
    assert!(Arc::ptr_eq(&nether, &worlds[1]));

    let Err(error) = get_level(&worlds, &Identifier::vanilla_static("the_end")) else {
        panic!("an unloaded dimension resolved to a world");
    };
    let message = serde_json::to_value(error.message).unwrap();
    assert_eq!(message["translate"], "argument.dimension.invalid");
    assert_eq!(message["with"][0]["text"], "minecraft:the_end");

    let parsed = ResourceKeyArgument(DIMENSION_REGISTRY)
        .parse(&mut StringReader::new("minecraft:the_nether"))
        .unwrap();
    assert_eq!(parsed.identifier, Identifier::vanilla_static("the_nether"));
}

#[tokio::test]
async fn suggests_loaded_dimensions() {
    let root = tempfile::TempDir::new().unwrap();
    let worlds = loaded_worlds(root.path(), &[Dimension::OVERWORLD, Dimension::THE_NETHER]);

    let texts = |input: &str| -> Vec<String> {
        suggest_levels(&worlds, SuggestionsBuilder::new(input, 0))
            .suggestions
            .iter()
            .map(|suggestion| suggestion.text.cached_text().clone())
            .collect()
    };
    assert_eq!(texts(""), ["minecraft:overworld", "minecraft:the_nether"]);
    assert_eq!(texts("the"), ["minecraft:the_nether"]);
    assert!(texts("minecraft:the_end").is_empty());
}
