//! Builders for custom blocks, items and entity types.
//!
//! A builder wraps the definition struct of the content registry and converts into it. A mod
//! never writes a struct literal, so a new field of the registry does not break mod code. Pass a
//! builder to [`ModInit::register_block`](crate::ModInit::register_block),
//! [`ModInit::register_item`](crate::ModInit::register_item) or
//! [`ModInit::register_entity_type`](crate::ModInit::register_entity_type).
//!
//! A builder does not validate. The registry validates when the mod registers the content and
//! returns a [`RegistryError`].
//!
//! Tags: `.tag(..)` takes the name of a generated (vanilla) tag of the same registry. A tag name
//! of the mod is rejected with [`RegistryError::UnknownTag`], because the registry has no way to
//! create a new tag yet. [`DynamicTaggable::has_tag_dynamic`] answers tag membership for generated
//! and custom content.
//!
//! ```
//! use ironpumpkin_mods::{
//!     ModInit, NativeMod,
//!     content::{BlockBuilder, ItemBuilder, RegistryError},
//!     pumpkin_data::{Block, item::Item},
//!     tracing::error,
//! };
//!
//! struct LampMod;
//!
//! impl NativeMod for LampMod {
//!     fn id(&self) -> &'static str {
//!         "lamps"
//!     }
//!     fn display_name(&self) -> &'static str {
//!         "Lamps"
//!     }
//!     fn version(&self) -> &'static str {
//!         "1.0.0"
//!     }
//!     fn init(&self, cx: &mut ModInit) {
//!         if let Err(err) = register_lamp(cx) {
//!             error!("[lamps] cannot register the lamp: {err}");
//!         }
//!     }
//! }
//!
//! fn register_lamp(cx: &mut ModInit) -> Result<(), RegistryError> {
//!     let lamp = BlockBuilder::new("lamps:copper_lamp", Block::REDSTONE_LAMP.default_state.id)
//!         .bool_property("lit", false)
//!         .tag("minecraft:mineable/pickaxe");
//!     cx.register_block(lamp)?;
//!     let item = ItemBuilder::new("lamps:copper_lamp", &Item::REDSTONE_LAMP)
//!         .places("lamps:copper_lamp");
//!     cx.register_item(item)
//! }
//! ```

use pumpkin_data::{
    BlockStateId,
    dynamic::{BlockDefinition, BlockPropertyDefinition, EntityTypeDefinition, ItemDefinition},
    entity::EntityType,
    item::Item,
};

pub use pumpkin_core::entity::custom::EntityFactory;
pub use pumpkin_data::dynamic::{ContentKind, DynamicTaggable, RegistryError};

/// A custom block. It has one state per combination of its property values.
///
/// `name` is `namespace:path` with a namespace other than `minecraft`. `display` is the vanilla
/// state that vanilla clients see: every state of the block copies its physical data.
#[derive(Debug, Clone)]
pub struct BlockBuilder(BlockDefinition);

impl BlockBuilder {
    #[must_use]
    pub fn new(name: impl Into<String>, display: BlockStateId) -> Self {
        Self(BlockDefinition {
            name: name.into(),
            display,
            properties: Vec::new(),
            tags: Vec::new(),
        })
    }

    /// Adds a boolean property. The registry rejects a name used twice.
    #[must_use]
    pub fn bool_property(self, name: impl Into<String>, default: bool) -> Self {
        self.property(BlockPropertyDefinition::bool(name, default))
    }

    /// Adds an integer property with the values `min..=max`. The registry rejects a range with
    /// fewer than two values and a default outside it.
    #[must_use]
    pub fn int_property(self, name: impl Into<String>, min: u8, max: u8, default: u8) -> Self {
        self.property(BlockPropertyDefinition::int(name, min, max, default))
    }

    /// Adds an enum property. The default must be one of the values. Names and values use
    /// `a-z 0-9 _`.
    #[must_use]
    pub fn enum_property(self, name: impl Into<String>, values: &[&str], default: &str) -> Self {
        self.property(BlockPropertyDefinition::enumeration(name, values, default))
    }

    /// Adds the block to a generated block tag, for example `minecraft:mineable/pickaxe`, besides
    /// the tags of the display block. Only generated tag names work: the registry rejects any
    /// other with [`RegistryError::UnknownTag`].
    #[must_use]
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.0.tags.push(tag.into());
        self
    }

    fn property(mut self, property: BlockPropertyDefinition) -> Self {
        self.0.properties.push(property);
        self
    }
}

impl From<BlockBuilder> for BlockDefinition {
    fn from(builder: BlockBuilder) -> Self {
        builder.0
    }
}

/// A custom item.
///
/// `name` is `namespace:path` with a namespace other than `minecraft`. `display` is the vanilla
/// item that vanilla clients see: the item copies its components.
#[derive(Debug, Clone)]
pub struct ItemBuilder(ItemDefinition);

impl ItemBuilder {
    #[must_use]
    pub fn new(name: impl Into<String>, display: &'static Item) -> Self {
        Self(ItemDefinition {
            name: name.into(),
            display,
            block: None,
            tags: Vec::new(),
        })
    }

    /// Makes the item place the custom block `block`.
    ///
    /// The registry resolves the link when it freezes, after the last mod's `init`, so the block
    /// can register before or after the item. The freeze fails, and the server does not start, if
    /// the block is not registered or another item already places it.
    #[must_use]
    pub fn places(mut self, block: impl Into<String>) -> Self {
        self.0.block = Some(block.into());
        self
    }

    /// Adds the item to a generated item tag, for example `minecraft:swords`, besides the tags of
    /// the display item. Only generated tag names work: the registry rejects any other with
    /// [`RegistryError::UnknownTag`].
    #[must_use]
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.0.tags.push(tag.into());
        self
    }
}

impl From<ItemBuilder> for ItemDefinition {
    fn from(builder: ItemBuilder) -> Self {
        builder.0
    }
}

/// A custom entity type.
///
/// `name` is `namespace:path` with a namespace other than `minecraft`. `display` is the vanilla
/// entity type that vanilla clients see: spawn packets carry its id, and the client renders its
/// model, hitbox and animations. The entity's metadata must fit the tracked data of `display`.
/// See [`EntityTypeDefinition`] for the details.
#[derive(Debug, Clone)]
pub struct EntityTypeBuilder(EntityTypeDefinition);

impl EntityTypeBuilder {
    #[must_use]
    pub fn new(name: impl Into<String>, display: &'static EntityType) -> Self {
        Self(EntityTypeDefinition {
            name: name.into(),
            display,
            dimensions: None,
            eye_height: None,
            tags: Vec::new(),
        })
    }

    /// Sets the width and height in blocks. Without it the type keeps the dimensions of `display`.
    /// The registry rejects a value that is not positive and finite.
    #[must_use]
    pub const fn dimensions(mut self, width: f32, height: f32) -> Self {
        self.0.dimensions = Some([width, height]);
        self
    }

    /// Sets the eye height in blocks. Without it the type keeps the eye height of `display`, or
    /// uses 0.85 of the height when the builder sets the dimensions. The registry rejects a value
    /// that is not positive and finite.
    #[must_use]
    pub const fn eye_height(mut self, eye_height: f32) -> Self {
        self.0.eye_height = Some(eye_height);
        self
    }

    /// Adds the type to a generated entity type tag, for example `minecraft:skeletons`, besides
    /// the tags of the display type. Only generated tag names work: the registry rejects any
    /// other with [`RegistryError::UnknownTag`].
    #[must_use]
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.0.tags.push(tag.into());
        self
    }
}

impl From<EntityTypeBuilder> for EntityTypeDefinition {
    fn from(builder: EntityTypeBuilder) -> Self {
        builder.0
    }
}

#[cfg(test)]
mod tests {
    use pumpkin_data::{Block, entity::EntityType, item::Item};

    use super::*;

    #[test]
    fn block_builder_collects_the_properties_in_call_order() {
        let definition: BlockDefinition =
            BlockBuilder::new("test:lamp", Block::REDSTONE_LAMP.default_state.id)
                .bool_property("lit", true)
                .int_property("level", 1, 3, 2)
                .enum_property("mode", &["a", "b"], "b")
                .tag("minecraft:mineable/pickaxe")
                .into();
        assert_eq!(definition.name, "test:lamp");
        assert_eq!(definition.display, Block::REDSTONE_LAMP.default_state.id);
        assert_eq!(
            definition.properties,
            [
                BlockPropertyDefinition::bool("lit", true),
                BlockPropertyDefinition::int("level", 1, 3, 2),
                BlockPropertyDefinition::enumeration("mode", &["a", "b"], "b"),
            ]
        );
        assert_eq!(definition.tags, ["minecraft:mineable/pickaxe"]);
    }

    #[test]
    fn item_builder_links_the_block_only_when_asked() {
        let plain: ItemDefinition = ItemBuilder::new("test:wand", &Item::STICK).into();
        assert_eq!(plain.display.id, Item::STICK.id);
        assert_eq!(plain.block, None);

        assert!(plain.tags.is_empty());

        let placing: ItemDefinition = ItemBuilder::new("test:lamp", &Item::REDSTONE_LAMP)
            .places("test:lamp")
            .tag("minecraft:swords")
            .into();
        assert_eq!(placing.block.as_deref(), Some("test:lamp"));
        assert_eq!(placing.tags, ["minecraft:swords"]);
    }

    #[test]
    fn entity_type_builder_keeps_the_display_sizes_unless_set() {
        let plain: EntityTypeDefinition =
            EntityTypeBuilder::new("test:golem", &EntityType::ZOMBIE).into();
        assert_eq!(plain.dimensions, None);
        assert_eq!(plain.eye_height, None);

        let sized: EntityTypeDefinition = EntityTypeBuilder::new("test:golem", &EntityType::ZOMBIE)
            .dimensions(1.5, 2.5)
            .eye_height(2.0)
            .tag("minecraft:skeletons")
            .into();
        assert_eq!(sized.dimensions, Some([1.5, 2.5]));
        assert_eq!(sized.eye_height, Some(2.0));
        assert_eq!(sized.tags, ["minecraft:skeletons"]);
    }
}
