//! Builders for custom blocks, items, entity types and tags.
//!
//! A builder wraps the definition struct of the content registry. A mod never writes a struct
//! literal, so a new field of the registry does not break mod code. Pass a builder to
//! [`ModInit::register_block`](crate::ModInit::register_block),
//! [`ModInit::register_item`](crate::ModInit::register_item),
//! [`ModInit::register_entity_type`](crate::ModInit::register_entity_type) or
//! [`ModInit::register_tag`](crate::ModInit::register_tag).
//!
//! A builder does not validate. The registry validates when the mod registers the content and
//! returns a [`RegistryError`].
//!
//! A registration error stops the server. `ModInit` keeps the first one, and the server logs it
//! with the mod id and exits after `init` returns, before the first world loads. A mod only has to
//! stop registering at the first error, for example with `?`. It must not panic or unwrap the
//! `Result`.
//!
//! Tags: `.tag(..)` takes the name of a generated (vanilla) tag of the same registry, or of a mod
//! tag. A [`TagBuilder`] defines a mod tag, such as `mymod:ores`, with vanilla and custom members.
//! Mod tags and the entries that list them resolve when the registry freezes, so the order of the
//! registrations does not matter, and a mod can list the tag of another mod. The freeze fails with
//! [`RegistryError::UnknownTag`] for a mod tag that no mod registers. Custom content joins the
//! generated tags of its display entry, but not its mod tags.
//! [`DynamicTaggable::has_tag_dynamic`] and [`pumpkin_data::dynamic::tag_ids`] answer for
//! generated and mod tags. Clients and data packs do not see mod tags.
//!
//! Behaviour: `.behaviour(..)` gives a block a [`BlockBehaviour`] and an item an [`ItemBehaviour`],
//! the traits of the vanilla blocks and items. The server calls it through the same hooks as for
//! vanilla content: placing, breaking, using, scheduled ticks, neighbour updates and redstone for
//! every state of the block, and every use of the item. Random ticks run only if the display
//! state has random ticks, because a custom state copies the flags of its display state. The
//! argument types are in [`pumpkin_core::block`] and [`pumpkin_core::item`].
//!
//! Attach only a behaviour of the mod, or a server behaviour that reads no block properties. A
//! server behaviour that reads the properties of its own block (for example `AnvilBlock::on_place`
//! through `WallTorchLikeProperties`) panics on a custom block, and the panic stops the server.
//! For the same reason, content does not inherit the behaviour of its display entry: without
//! `.behaviour(..)` the server uses the trait defaults.
//!
//! ```
//! use std::sync::Arc;
//!
//! use ironpumpkin_mods::{
//!     ModInit, NativeMod,
//!     content::{
//!         BlockBehaviour, BlockBuilder, ContentKind, ItemBuilder, RegistryError, TagBuilder,
//!     },
//!     pumpkin_core::block::PlacedArgs,
//!     pumpkin_data::{Block, item::Item},
//!     tracing::info,
//! };
//!
//! struct CopperLamp;
//!
//! impl BlockBehaviour for CopperLamp {
//!     fn placed(&self, args: PlacedArgs<'_>) {
//!         info!("[lamps] copper lamp placed at {:?}", args.position);
//!     }
//! }
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
//!         // On an error the server logs it with the mod id and stops after `init` returns.
//!         let _ = register_lamp(cx);
//!     }
//! }
//!
//! fn register_lamp(cx: &mut ModInit) -> Result<(), RegistryError> {
//!     let lamp = BlockBuilder::new("lamps:copper_lamp", Block::REDSTONE_LAMP.default_state.id)
//!         .bool_property("lit", false)
//!         .tag("minecraft:mineable/pickaxe")
//!         .tag("lamps:lamps")
//!         .behaviour(Arc::new(CopperLamp));
//!     cx.register_block(lamp)?;
//!     // A tag of the mod, with a vanilla member. The copper lamp joins it through `.tag(..)`.
//!     let lamps = TagBuilder::new(ContentKind::Block, "lamps:lamps")
//!         .value("minecraft:redstone_lamp");
//!     cx.register_tag(lamps)?;
//!     let item = ItemBuilder::new("lamps:copper_lamp", &Item::REDSTONE_LAMP)
//!         .places("lamps:copper_lamp");
//!     cx.register_item(item)
//! }
//! ```

use std::{fmt, sync::Arc};

use pumpkin_core::content::behaviour;
use pumpkin_data::{
    BlockStateId,
    dynamic::{
        BlockDefinition, BlockPropertyDefinition, EntityTypeDefinition, ItemDefinition,
        TagDefinition,
    },
    entity::EntityType,
    item::Item,
};

pub use pumpkin_core::block::BlockBehaviour;
pub use pumpkin_core::entity::custom::EntityFactory;
pub use pumpkin_core::item::ItemBehaviour;
pub use pumpkin_data::dynamic::{ContentKind, DynamicTaggable, RegistryError};

/// A custom block. It has one state per combination of its property values.
///
/// `name` is `namespace:path` with a namespace other than `minecraft`. `display` is the vanilla
/// state that vanilla clients see: every state of the block copies its physical data.
#[derive(Clone)]
pub struct BlockBuilder {
    definition: BlockDefinition,
    behaviour: Option<Arc<dyn BlockBehaviour>>,
}

impl BlockBuilder {
    #[must_use]
    pub fn new(name: impl Into<String>, display: BlockStateId) -> Self {
        Self {
            definition: BlockDefinition {
                name: name.into(),
                display,
                properties: Vec::new(),
                tags: Vec::new(),
            },
            behaviour: None,
        }
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

    /// Adds the block to a block tag, besides the tags of the display block: a generated tag such
    /// as `minecraft:mineable/pickaxe`, or a mod tag. See the [module docs](self).
    #[must_use]
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.definition.tags.push(tag.into());
        self
    }

    /// Sets the behaviour that the server runs for every state of the block. A second call
    /// replaces the first. See the [module docs](self).
    #[must_use]
    pub fn behaviour(mut self, behaviour: Arc<dyn BlockBehaviour>) -> Self {
        self.behaviour = Some(behaviour);
        self
    }

    fn property(mut self, property: BlockPropertyDefinition) -> Self {
        self.definition.properties.push(property);
        self
    }

    pub(crate) fn name(&self) -> &str {
        &self.definition.name
    }

    pub(crate) fn register(self) -> Result<(), RegistryError> {
        behaviour::register_block(self.definition, self.behaviour)
    }
}

impl fmt::Debug for BlockBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlockBuilder")
            .field("definition", &self.definition)
            .field("behaviour", &self.behaviour.is_some())
            .finish()
    }
}

/// A custom item.
///
/// `name` is `namespace:path` with a namespace other than `minecraft`. `display` is the vanilla
/// item that vanilla clients see: the item copies its components.
#[derive(Clone)]
pub struct ItemBuilder {
    definition: ItemDefinition,
    behaviour: Option<Arc<dyn ItemBehaviour>>,
}

impl ItemBuilder {
    #[must_use]
    pub fn new(name: impl Into<String>, display: &'static Item) -> Self {
        Self {
            definition: ItemDefinition {
                name: name.into(),
                display,
                block: None,
                tags: Vec::new(),
            },
            behaviour: None,
        }
    }

    /// Makes the item place the custom block `block`.
    ///
    /// The registry resolves the link when it freezes, after the last mod's `init`, so the block
    /// can register before or after the item. The freeze fails, and the server does not start, if
    /// the block is not registered or another item already places it.
    #[must_use]
    pub fn places(mut self, block: impl Into<String>) -> Self {
        self.definition.block = Some(block.into());
        self
    }

    /// Adds the item to an item tag, besides the tags of the display item: a generated tag such
    /// as `minecraft:swords`, or a mod tag. See the [module docs](self).
    #[must_use]
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.definition.tags.push(tag.into());
        self
    }

    /// Sets the behaviour that the server runs when a player uses the item. A second call
    /// replaces the first. An item that places a block keeps placing it when
    /// [`ItemBehaviour::use_on_block`] returns `BlockActionResult::Pass`, the default. See the
    /// [module docs](self).
    #[must_use]
    pub fn behaviour(mut self, behaviour: Arc<dyn ItemBehaviour>) -> Self {
        self.behaviour = Some(behaviour);
        self
    }

    pub(crate) fn name(&self) -> &str {
        &self.definition.name
    }

    pub(crate) fn register(self) -> Result<(), RegistryError> {
        behaviour::register_item(self.definition, self.behaviour)
    }
}

impl fmt::Debug for ItemBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ItemBuilder")
            .field("definition", &self.definition)
            .field("behaviour", &self.behaviour.is_some())
            .finish()
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

    /// Adds the type to an entity type tag, besides the tags of the display type: a generated tag
    /// such as `minecraft:skeletons`, or a mod tag. See the [module docs](self).
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

/// A tag that the mod defines in the block, item or entity type registry.
///
/// `name` is `namespace:path` with a namespace other than `minecraft`, without the `#` prefix that
/// `.tag(..)` accepts. It must be unique in its registry, where a generated tag such as `c:ores`
/// counts as registered.
#[derive(Debug, Clone)]
pub struct TagBuilder(TagDefinition);

impl TagBuilder {
    #[must_use]
    pub fn new(kind: ContentKind, name: impl Into<String>) -> Self {
        Self(TagDefinition {
            kind,
            name: name.into(),
            values: Vec::new(),
        })
    }

    /// Adds an entry of the tag's registry: a vanilla entry such as `minecraft:iron_ore`, or a
    /// custom entry of any mod such as `mymod:tin_ore`. The registry rejects an unknown vanilla
    /// entry at once, and the freeze fails for an unknown custom entry.
    #[must_use]
    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.0.values.push(value.into());
        self
    }
}

impl From<TagBuilder> for TagDefinition {
    fn from(builder: TagBuilder) -> Self {
        builder.0
    }
}

#[cfg(test)]
mod tests {
    use pumpkin_data::{Block, entity::EntityType, item::Item};

    use super::*;

    #[test]
    fn block_builder_collects_the_properties_in_call_order() {
        let definition = BlockBuilder::new("test:lamp", Block::REDSTONE_LAMP.default_state.id)
            .bool_property("lit", true)
            .int_property("level", 1, 3, 2)
            .enum_property("mode", &["a", "b"], "b")
            .tag("minecraft:mineable/pickaxe")
            .definition;
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
        let plain = ItemBuilder::new("test:wand", &Item::STICK).definition;
        assert_eq!(plain.display.id, Item::STICK.id);
        assert_eq!(plain.block, None);

        assert!(plain.tags.is_empty());

        let placing = ItemBuilder::new("test:lamp", &Item::REDSTONE_LAMP)
            .places("test:lamp")
            .tag("minecraft:swords")
            .definition;
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

    #[test]
    fn tag_builder_collects_the_values_in_call_order() {
        let tag: TagDefinition = TagBuilder::new(ContentKind::Block, "test:ores")
            .value("minecraft:iron_ore")
            .value("test:tin_ore")
            .into();
        assert_eq!(tag.kind, ContentKind::Block);
        assert_eq!(tag.name, "test:ores");
        assert_eq!(tag.values, ["minecraft:iron_ore", "test:tin_ore"]);
    }
}
