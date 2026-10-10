//! Blocks, items and entity types that are not in the generated tables.
//!
//! Content is registered before the first world loads. [`freeze`] then allocates ids after the
//! generated counts, in name order so that plugin load order does not change them, and installs
//! immutable tables. The generated lookups fall through to these tables for ids at or above the
//! generated counts and for names the generated maps do not know. After the freeze the tables
//! never change, so readers take no lock.
//!
//! Each custom entry copies the data of its display entry, the vanilla entry that vanilla clients
//! see. The states of a custom block use the generated layout: properties in name order, the last
//! property varies fastest.
//!
//! Tag membership lives in two tables. The generated tag lists hold generated ids only, so
//! `BlockId::has_tag` and the generated [`Taggable`] methods stay branch-free on the worldgen hot
//! path. The custom tag table holds the custom ids of each generated tag: an entry joins the tags
//! of its display entry and the tags its definition lists. [`DynamicTaggable`] and [`tag_ids`]
//! read both tables.
//!
//! A mod can define a tag of its own with [`register_tag`]. A mod tag is a key of the custom tag
//! table too, and its generated members live in a third map. It holds the members its definition
//! lists and the entries that list it, never the entries of a display entry. Clients and the
//! datapack tag loader do not know mod tags.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap, btree_map::Entry};
use std::fmt;
use std::sync::{Mutex, OnceLock, PoisonError};

use pumpkin_util::identifier::{Identifier, VANILLA_NAMESPACE};
#[cfg(feature = "item")]
use pumpkin_util::text::hover::HoverEvent;

use crate::block_properties::{BlockProperties, BlockProperty};
#[cfg(feature = "entity_type")]
use crate::entity_type::EntityType;
#[cfg(feature = "item")]
use crate::item::Item;
use crate::tag::{RegistryKey, Taggable, get_latest_map};
use crate::{Block, BlockId, BlockState, BlockStateId};

/// The highest id the registry allocates. `u16::MAX` is the "none" value of u16 ids.
const MAX_ID: u16 = u16::MAX - 1;

/// A custom block. It has one state per combination of its property values.
#[derive(Debug, Clone)]
pub struct BlockDefinition {
    /// Namespaced name, for example `mymod:copper_lamp`.
    pub name: String,
    /// Vanilla state that vanilla clients see. Every state of the block copies its physical data.
    pub display: BlockStateId,
    /// Block state properties, in any order. The registry sorts them by name.
    pub properties: Vec<BlockPropertyDefinition>,
    /// Block tags the block joins besides the tags of its display block, for example
    /// `minecraft:mineable/pickaxe`. Each must name a generated block tag or a block tag that a mod
    /// registers with [`register_tag`].
    pub tags: Vec<String>,
}

/// A tag that a mod defines, in the block, item or entity type registry.
#[derive(Debug, Clone)]
pub struct TagDefinition {
    /// The registry of the tag and of its members.
    pub kind: ContentKind,
    /// Namespaced name, for example `mymod:ores`.
    pub name: String,
    /// Entry names: a generated entry such as `minecraft:iron_ore` or `iron_ore`, or a custom
    /// entry of any mod such as `mymod:tin_ore`. Custom entries are resolved at the freeze.
    pub values: Vec<String>,
}

/// A block state property of a custom block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockPropertyDefinition {
    /// Property name, for example `lit`.
    pub name: String,
    /// Values in state layout order.
    pub values: Vec<String>,
    /// The value of the block's default state.
    pub default: String,
}

impl BlockPropertyDefinition {
    /// A boolean property. Its values are `true` then `false`, as in the generated layout.
    #[must_use]
    pub fn bool(name: impl Into<String>, default: bool) -> Self {
        Self {
            name: name.into(),
            values: vec!["true".to_string(), "false".to_string()],
            default: default.to_string(),
        }
    }

    /// An integer property with the values `min..=max`, ascending.
    #[must_use]
    pub fn int(name: impl Into<String>, min: u8, max: u8, default: u8) -> Self {
        Self {
            name: name.into(),
            values: (min..=max).map(|value| value.to_string()).collect(),
            default: default.to_string(),
        }
    }

    /// An enum property. Its values keep the declaration order.
    #[must_use]
    pub fn enumeration(name: impl Into<String>, values: &[&str], default: &str) -> Self {
        Self {
            name: name.into(),
            values: values.iter().map(|value| (*value).to_string()).collect(),
            default: default.to_string(),
        }
    }
}

/// A custom item.
#[cfg(feature = "item")]
#[derive(Debug, Clone)]
pub struct ItemDefinition {
    /// Namespaced name, for example `mymod:copper_lamp`.
    pub name: String,
    /// Vanilla item that vanilla clients see. The item copies its components.
    pub display: &'static Item,
    /// Namespaced name of the custom block this item places.
    pub block: Option<String>,
    /// Item tags the item joins besides the tags of its display item. Each must name a
    /// generated item tag or an item tag that a mod registers with [`register_tag`].
    pub tags: Vec<String>,
}

/// A custom entity type.
///
/// A vanilla client knows only the vanilla entity types, so it sees the `display` type:
/// spawn packets carry the display type's id ([`EntityType::to_java_network_id`]) and the
/// client renders the display type's model, hitbox and animations. The entity's metadata must
/// therefore fit the display type's tracked data, or the client fails to decode it. The server
/// uses `dimensions` for collision, so a hitbox that differs from the display type's shows a
/// mismatch on the client. Statistics of custom types are not sent to the client. A vanilla
/// client shows the entity's name as the raw key `entity.<namespace>.<path>`, because a
/// translated text component carries no fallback.
///
/// The spawn factory is registered with the type in `pumpkin-core`
/// (`pumpkin_core::entity::custom::register_entity_type`), which cannot be named here.
#[cfg(feature = "entity_type")]
#[derive(Debug, Clone)]
pub struct EntityTypeDefinition {
    /// Namespaced name, for example `mymod:golem`.
    pub name: String,
    /// Vanilla entity type that vanilla clients see. The type copies its data.
    pub display: &'static EntityType,
    /// Width and height in blocks. `None` keeps the display type's.
    pub dimensions: Option<[f32; 2]>,
    /// Eye height in blocks. `None` keeps the display type's eye height when `dimensions` is
    /// `None`, and else uses 0.85 of the height, the default of vanilla's
    /// `EntityType.Builder.sized`.
    pub eye_height: Option<f32>,
    /// Entity type tags the type joins besides the tags of its display type. Each must name a
    /// generated entity type tag or an entity type tag that a mod registers with
    /// [`register_tag`].
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContentKind {
    Block,
    Item,
    EntityType,
}

impl ContentKind {
    const fn tag_key(self) -> RegistryKey {
        match self {
            Self::Block => RegistryKey::Block,
            Self::Item => RegistryKey::Item,
            Self::EntityType => RegistryKey::EntityType,
        }
    }
}

impl fmt::Display for ContentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Block => "block",
            Self::Item => "item",
            Self::EntityType => "entity type",
        })
    }
}

/// Which ids a client gets for custom content in play packets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ContentIds {
    /// A custom id maps to the id of its display entry, which a vanilla client knows.
    #[default]
    Display,
    /// Ids as allocated. Only a client that synced the registries of the server knows them. A
    /// placeholder is not in the synced registries, so it keeps its display id.
    Real,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    /// The registry is frozen and accepts no new content.
    RegistryFrozen,
    /// The name is not a valid `namespace:path` resource location.
    InvalidName { kind: ContentKind, name: String },
    /// The name has no namespace or uses the `minecraft` namespace.
    ReservedNamespace { kind: ContentKind, name: String },
    /// The name is already registered for this kind.
    Duplicate { kind: ContentKind, name: String },
    /// The display entry is not a generated (vanilla) entry.
    DisplayNotVanilla { kind: ContentKind, name: String },
    /// The item places a block that is not registered.
    UnknownBlock { item: String, block: String },
    /// Another item already places this block.
    BlockAlreadyHasItem { item: String, block: String },
    /// The content does not fit in the free id range.
    RangeExceeded {
        registry: &'static str,
        count: usize,
    },
    /// A block state property has an invalid name, values or default.
    InvalidProperty {
        block: String,
        property: String,
        reason: &'static str,
    },
    /// The entity type's width, height or eye height is not a positive finite number.
    InvalidDimensions { name: String },
    /// The definition lists a tag that its registry does not have: no generated tag, and no mod
    /// tag at the freeze.
    UnknownTag {
        kind: ContentKind,
        name: String,
        tag: String,
    },
    /// The mod tag name is not a `namespace:path` outside the `minecraft` namespace.
    InvalidTagName { kind: ContentKind, tag: String },
    /// The mod tag is already registered in this registry.
    DuplicateTag { kind: ContentKind, tag: String },
    /// The mod tag lists an entry that its registry does not have.
    UnknownTagMember {
        kind: ContentKind,
        tag: String,
        member: String,
    },
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RegistryFrozen => f.write_str("the content registry is frozen"),
            Self::InvalidName { kind, name } => write!(
                f,
                "invalid {kind} name \"{name}\": expected namespace:path with a-z 0-9 _ - . and / in the path"
            ),
            Self::ReservedNamespace { kind, name } => {
                write!(f, "{kind} \"{name}\" must use its own namespace")
            }
            Self::Duplicate { kind, name } => write!(f, "{kind} \"{name}\" is already registered"),
            Self::DisplayNotVanilla { kind, name } => {
                write!(f, "{kind} \"{name}\" needs a vanilla display entry")
            }
            Self::UnknownBlock { item, block } => {
                write!(f, "item \"{item}\" places unknown block \"{block}\"")
            }
            Self::BlockAlreadyHasItem { item, block } => {
                write!(
                    f,
                    "item \"{item}\" places block \"{block}\", which already has an item"
                )
            }
            Self::RangeExceeded { registry, count } => {
                write!(
                    f,
                    "{count} custom {registry} entries do not fit in the id range"
                )
            }
            Self::InvalidProperty {
                block,
                property,
                reason,
            } => write!(f, "property \"{property}\" of block \"{block}\": {reason}"),
            Self::InvalidDimensions { name } => write!(
                f,
                "entity type \"{name}\" needs a positive finite width, height and eye height"
            ),
            Self::UnknownTag { kind, name, tag } => {
                write!(f, "{kind} \"{name}\" lists unknown {kind} tag \"{tag}\"")
            }
            Self::InvalidTagName { kind, tag } => write!(
                f,
                "invalid {kind} tag name \"{tag}\": expected namespace:path outside the minecraft namespace"
            ),
            Self::DuplicateTag { kind, tag } => {
                write!(f, "{kind} tag \"{tag}\" is already registered")
            }
            Self::UnknownTagMember { kind, tag, member } => {
                write!(f, "{kind} tag \"{tag}\" lists unknown {kind} \"{member}\"")
            }
        }
    }
}

impl std::error::Error for RegistryError {}

/// The frozen custom content. Ids are the generated count plus the index in each table.
#[derive(Debug)]
pub struct ContentTables {
    blocks: Vec<Block>,
    block_names: HashMap<&'static str, usize>,
    block_properties: Vec<&'static [BlockProperty]>,
    states: &'static [BlockState],
    state_blocks: Vec<BlockId>,
    state_displays: Vec<BlockStateId>,
    block_displays: Vec<u16>,
    /// The state ids a `Real` client gets: the client-side id, which skips the states of
    /// placeholders, or the display id of a placeholder.
    real_states: Vec<u16>,
    /// The ids a `Real` client gets: the id itself, or the display id of a placeholder.
    real_blocks: Vec<u16>,
    #[cfg(feature = "item")]
    real_items: Vec<u16>,
    #[cfg(feature = "entity_type")]
    real_entity_types: Vec<u16>,
    #[cfg(feature = "item")]
    items: Vec<Item>,
    #[cfg(feature = "item")]
    item_blocks: Vec<Option<BlockId>>,
    #[cfg(feature = "item")]
    item_names: HashMap<&'static str, usize>,
    #[cfg(feature = "item")]
    item_displays: Vec<u16>,
    #[cfg(feature = "entity_type")]
    entity_types: Vec<EntityType>,
    #[cfg(feature = "entity_type")]
    entity_type_names: HashMap<&'static str, usize>,
    #[cfg(feature = "entity_type")]
    entity_type_displays: Vec<&'static EntityType>,
    placeholders: BTreeMap<ContentKind, BTreeSet<String>>,
    block_tags: TagTables,
    #[cfg(feature = "item")]
    item_tags: TagTables,
    #[cfg(feature = "entity_type")]
    entity_type_tags: TagTables,
    explicit_tags: NamedSets,
    mod_tags: NamedSets,
}

/// The tag tables of one registry.
#[derive(Debug, Default)]
struct TagTables {
    /// The custom ids of each tag that has custom members, sorted, keyed by the generated or mod
    /// tag name.
    custom: HashMap<&'static str, Box<[u16]>>,
    /// The generated members of each mod tag, sorted. Every mod tag of the registry has a key.
    mod_tags: HashMap<&'static str, Box<[u16]>>,
}

/// Name sets per registry and per name: the explicit tags of each entry, or the declared values
/// of each mod tag.
type NamedSets = BTreeMap<ContentKind, BTreeMap<String, BTreeSet<String>>>;

static NO_NAMES: BTreeSet<String> = BTreeSet::new();
static NO_SETS: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

impl ContentTables {
    /// The tags that the definition of a custom entry lists, in name order: generated tag names
    /// with their namespace, and mod tag names. The tags of the display entry are not included.
    #[must_use]
    pub fn explicit_tags(&self, kind: ContentKind, name: &str) -> &BTreeSet<String> {
        self.explicit_tags
            .get(&kind)
            .and_then(|entries| entries.get(name))
            .unwrap_or(&NO_NAMES)
    }

    /// The mod tags of a registry in name order, each with the entry names its definition lists,
    /// in name order. Generated entry names carry the `minecraft` namespace.
    #[must_use]
    pub fn mod_tags(&self, kind: ContentKind) -> &BTreeMap<String, BTreeSet<String>> {
        self.mod_tags.get(&kind).unwrap_or(&NO_SETS)
    }

    /// Custom blocks in id order.
    #[must_use]
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// The properties of each custom block in [`Self::blocks`] order, sorted by name, with their
    /// values in state layout order.
    #[must_use]
    pub fn block_properties(&self) -> &[&'static [BlockProperty]] {
        &self.block_properties
    }

    /// Custom items in id order.
    #[cfg(feature = "item")]
    #[must_use]
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// Custom entity types in id order.
    #[cfg(feature = "entity_type")]
    #[must_use]
    pub fn entity_types(&self) -> &[EntityType] {
        &self.entity_types
    }

    /// Every block id with its namespaced name: the generated blocks, then the custom blocks in id
    /// order. This is the `minecraft:block` snapshot of the `NeoForge` registry sync.
    ///
    /// Placeholders are left out, so their ids are gaps. A `NeoForge` client disconnects on a key
    /// it does not know, and it would never join a world that still holds content of a removed
    /// mod. `NeoForge` accepts sparse ids.
    #[must_use]
    pub fn block_snapshot(&self) -> Vec<(i32, Identifier)> {
        let entry = |block: &Block| (block.id.as_u16(), block.name);
        self.snapshot(
            ContentKind::Block,
            (0..BlockId::COUNT)
                .map(|id| entry(Block::from_id(BlockId::new_unchecked(id))))
                .chain(self.blocks.iter().map(entry)),
        )
    }

    /// Every item id with its namespaced name. See [`Self::block_snapshot`].
    #[cfg(feature = "item")]
    #[must_use]
    pub fn item_snapshot(&self) -> Vec<(i32, Identifier)> {
        let entry = |item: &Item| (item.id, item.registry_key);
        self.snapshot(
            ContentKind::Item,
            (0..Item::COUNT)
                .filter_map(Item::from_id)
                .map(entry)
                .chain(self.items.iter().map(entry)),
        )
    }

    /// Every entity type id with its namespaced name. See [`Self::block_snapshot`].
    #[cfg(feature = "entity_type")]
    #[must_use]
    pub fn entity_type_snapshot(&self) -> Vec<(i32, Identifier)> {
        let entry = |ty: &EntityType| (ty.id, ty.resource_name);
        self.snapshot(
            ContentKind::EntityType,
            (0..EntityType::COUNT)
                .filter_map(EntityType::from_raw)
                .map(entry)
                .chain(self.entity_types.iter().map(entry)),
        )
    }

    /// Generated names are bare and parse into the `minecraft` namespace. Custom names passed
    /// [`validate_name`] at registration, so the parsed name is a valid identifier.
    fn snapshot(
        &self,
        kind: ContentKind,
        entries: impl Iterator<Item = (u16, &'static str)>,
    ) -> Vec<(i32, Identifier)> {
        let placeholders = self.placeholders.get(&kind);
        entries
            .filter(|(_, name)| placeholders.is_none_or(|names| !names.contains(*name)))
            .map(|(id, name)| (i32::from(id), Identifier::parse_static(name)))
            .collect()
    }
}

/// A registered block with its state layout.
#[derive(Debug)]
struct PendingBlock {
    display: BlockStateId,
    /// Sorted by name. The first property is the most significant digit of the state index.
    properties: Vec<BlockPropertyDefinition>,
    state_count: usize,
    default_index: usize,
}

impl PendingBlock {
    fn new(
        name: &str,
        display: BlockStateId,
        mut properties: Vec<BlockPropertyDefinition>,
    ) -> Result<Self, RegistryError> {
        let invalid = |property: &BlockPropertyDefinition, reason| RegistryError::InvalidProperty {
            block: name.to_string(),
            property: property.name.clone(),
            reason,
        };
        properties.sort_by(|a, b| a.name.cmp(&b.name));
        if let Some(pair) = properties
            .windows(2)
            .find(|pair| pair[0].name == pair[1].name)
        {
            return Err(invalid(&pair[0], "the name is used twice"));
        }
        let mut state_count: usize = 1;
        let mut default_index: usize = 0;
        for property in &properties {
            if !is_valid_property_word(&property.name) {
                return Err(invalid(property, "the name must use a-z 0-9 and _"));
            }
            // Vanilla `StateDefinition.Builder.validateProperty` rejects these too.
            if property.values.len() < 2 {
                return Err(invalid(property, "it needs at least two values"));
            }
            if !property
                .values
                .iter()
                .all(|value| is_valid_property_word(value))
            {
                return Err(invalid(property, "the values must use a-z 0-9 and _"));
            }
            let values = property.values.len();
            if (1..values).any(|i| property.values[..i].contains(&property.values[i])) {
                return Err(invalid(property, "a value is listed twice"));
            }
            let Some(default) = property.values.iter().position(|v| *v == property.default) else {
                return Err(invalid(property, "the default is not one of the values"));
            };
            // Saturated counts fail the range check in `build`.
            state_count = state_count.saturating_mul(values);
            default_index = default_index.saturating_mul(values).saturating_add(default);
        }
        Ok(Self {
            display,
            properties,
            state_count,
            default_index,
        })
    }
}

fn is_valid_property_word(word: &str) -> bool {
    !word.is_empty()
        && word
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

#[derive(Debug)]
struct ContentBuilder {
    blocks: BTreeMap<String, PendingBlock>,
    #[cfg(feature = "item")]
    items: BTreeMap<String, (&'static Item, Option<String>)>,
    #[cfg(feature = "entity_type")]
    entity_types: BTreeMap<String, (&'static EntityType, Option<[f32; 2]>, Option<f32>)>,
    placeholders: BTreeMap<ContentKind, BTreeSet<String>>,
    /// The explicit tags of each entry: generated tag names as the generated map spells them, and
    /// mod tag names, which resolve at the freeze.
    tags: NamedSets,
    /// The mod tags of each registry, with their values as canonical entry names.
    mod_tags: NamedSets,
}

impl ContentBuilder {
    const fn new() -> Self {
        Self {
            blocks: BTreeMap::new(),
            #[cfg(feature = "item")]
            items: BTreeMap::new(),
            #[cfg(feature = "entity_type")]
            entity_types: BTreeMap::new(),
            placeholders: BTreeMap::new(),
            tags: BTreeMap::new(),
            mod_tags: BTreeMap::new(),
        }
    }

    fn contains(&self, kind: ContentKind, name: &str) -> bool {
        match kind {
            ContentKind::Block => self.blocks.contains_key(name),
            #[cfg(feature = "item")]
            ContentKind::Item => self.items.contains_key(name),
            #[cfg(not(feature = "item"))]
            ContentKind::Item => false,
            #[cfg(feature = "entity_type")]
            ContentKind::EntityType => self.entity_types.contains_key(name),
            #[cfg(not(feature = "entity_type"))]
            ContentKind::EntityType => false,
        }
    }

    fn mark_placeholder(&mut self, kind: ContentKind, name: String) {
        self.placeholders.entry(kind).or_default().insert(name);
    }

    fn set_tags(&mut self, kind: ContentKind, name: String, tags: BTreeSet<String>) {
        if !tags.is_empty() {
            self.tags.entry(kind).or_default().insert(name, tags);
        }
    }

    fn register_tag(&mut self, definition: TagDefinition) -> Result<(), RegistryError> {
        let TagDefinition { kind, name, values } = definition;
        if !is_custom_name(&name) {
            return Err(RegistryError::InvalidTagName { kind, tag: name });
        }
        // A generated tag outside `minecraft`, such as `c:ores`, would shadow the mod tag.
        if resolve_tag(kind.tag_key(), &name).is_some() {
            return Err(RegistryError::DuplicateTag { kind, tag: name });
        }
        let values = values
            .into_iter()
            .map(|value| {
                canonical_member(kind, &value).ok_or_else(|| RegistryError::UnknownTagMember {
                    kind,
                    tag: name.clone(),
                    member: value,
                })
            })
            .collect::<Result<BTreeSet<String>, _>>()?;
        match self.mod_tags.entry(kind).or_default().entry(name) {
            Entry::Occupied(entry) => Err(RegistryError::DuplicateTag {
                kind,
                tag: entry.key().clone(),
            }),
            Entry::Vacant(entry) => {
                entry.insert(values);
                Ok(())
            }
        }
    }

    /// Checks the tag references that resolve at the freeze: the mod tags that entries list and
    /// the custom members of each mod tag.
    fn check_tags(&self) -> Result<(), RegistryError> {
        for (&kind, entries) in &self.tags {
            let mod_tags = self.mod_tags.get(&kind);
            for (name, tags) in entries {
                let unknown = tags.iter().find(|tag| {
                    resolve_tag(kind.tag_key(), tag).is_none()
                        && !mod_tags.is_some_and(|mod_tags| mod_tags.contains_key(*tag))
                });
                if let Some(tag) = unknown {
                    return Err(RegistryError::UnknownTag {
                        kind,
                        name: name.clone(),
                        tag: tag.clone(),
                    });
                }
            }
        }
        for (&kind, mod_tags) in &self.mod_tags {
            for (tag, values) in mod_tags {
                let unknown = values
                    .iter()
                    .find(|value| is_custom_name(value) && !self.contains(kind, value));
                if let Some(member) = unknown {
                    return Err(RegistryError::UnknownTagMember {
                        kind,
                        tag: tag.clone(),
                        member: member.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    fn register_block(&mut self, definition: BlockDefinition) -> Result<(), RegistryError> {
        let kind = ContentKind::Block;
        validate_name(kind, &definition.name)?;
        // `to_be_network_id` relies on this: display states are vanilla, and registration only
        // runs before the freeze, when no custom state id can exist yet.
        if definition.display.as_u16() >= BlockStateId::STATE_COUNT {
            return Err(RegistryError::DisplayNotVanilla {
                kind,
                name: definition.name,
            });
        }
        let block = PendingBlock::new(&definition.name, definition.display, definition.properties)?;
        let tags = resolve_tags(kind, &definition.name, &definition.tags)?;
        let name = definition.name.clone();
        insert_new(&mut self.blocks, kind, definition.name, block)?;
        self.set_tags(kind, name, tags);
        Ok(())
    }

    #[cfg(feature = "item")]
    fn register_item(&mut self, definition: ItemDefinition) -> Result<(), RegistryError> {
        let kind = ContentKind::Item;
        validate_name(kind, &definition.name)?;
        // Copy from the generated table: the passed value has pub fields and can be forged.
        let display = definition.display.id;
        let Some(display) = Item::from_id(display).filter(|_| display < Item::COUNT) else {
            return Err(RegistryError::DisplayNotVanilla {
                kind,
                name: definition.name,
            });
        };
        let tags = resolve_tags(kind, &definition.name, &definition.tags)?;
        let name = definition.name.clone();
        insert_new(
            &mut self.items,
            kind,
            definition.name,
            (display, definition.block),
        )?;
        self.set_tags(kind, name, tags);
        Ok(())
    }

    #[cfg(feature = "entity_type")]
    fn register_entity_type(
        &mut self,
        definition: EntityTypeDefinition,
    ) -> Result<(), RegistryError> {
        let kind = ContentKind::EntityType;
        validate_name(kind, &definition.name)?;
        // Copy from the generated table: the passed value has pub fields and can be forged.
        let display = definition.display.id;
        let Some(display) = EntityType::from_raw(display).filter(|_| display < EntityType::COUNT)
        else {
            return Err(RegistryError::DisplayNotVanilla {
                kind,
                name: definition.name,
            });
        };
        let sizes = definition
            .dimensions
            .into_iter()
            .flatten()
            .chain(definition.eye_height);
        if !sizes.into_iter().all(|size| size.is_finite() && size > 0.0) {
            return Err(RegistryError::InvalidDimensions {
                name: definition.name,
            });
        }
        let tags = resolve_tags(kind, &definition.name, &definition.tags)?;
        let name = definition.name.clone();
        insert_new(
            &mut self.entity_types,
            kind,
            definition.name,
            (display, definition.dimensions, definition.eye_height),
        )?;
        self.set_tags(kind, name, tags);
        Ok(())
    }

    /// Allocates ids in name order and builds the tables. The maps iterate in name order.
    fn build(self) -> Result<ContentTables, RegistryError> {
        check_range("block", BlockId::COUNT, self.blocks.len())?;
        let state_count = self
            .blocks
            .values()
            .fold(0, |sum: usize, block| sum.saturating_add(block.state_count));
        check_range("block state", BlockStateId::STATE_COUNT, state_count)?;
        #[cfg(feature = "item")]
        check_range("item", Item::COUNT, self.items.len())?;
        #[cfg(feature = "entity_type")]
        check_range("entity type", EntityType::COUNT, self.entity_types.len())?;
        self.check_tags()?;
        let block_placeholders = self.placeholder_flags(ContentKind::Block, self.blocks.keys());
        #[cfg(feature = "item")]
        let item_placeholders = self.placeholder_flags(ContentKind::Item, self.items.keys());
        #[cfg(feature = "entity_type")]
        let entity_type_placeholders =
            self.placeholder_flags(ContentKind::EntityType, self.entity_types.keys());

        #[cfg(feature = "item")]
        let (block_items, item_blocks) = self.link_items()?;
        // Item id 0 is air, which generated blocks without an item use too.
        #[cfg(not(feature = "item"))]
        let block_items = vec![0; self.blocks.len()];

        let blocks: Vec<(&'static str, PendingBlock)> = self
            .blocks
            .into_iter()
            .map(|(name, block)| (leak(name), block))
            .collect();
        let block_names: HashMap<&'static str, usize> = blocks
            .iter()
            .enumerate()
            .map(|(index, (name, _))| (*name, index))
            .collect();

        #[cfg(feature = "item")]
        let item_displays: Vec<u16> = self.items.values().map(|(display, _)| display.id).collect();
        #[cfg(feature = "item")]
        let real_items = real_ids(Item::COUNT, &item_displays, &item_placeholders);
        #[cfg(feature = "item")]
        let items: Vec<Item> = self
            .items
            .into_iter()
            .enumerate()
            .map(|(index, (name, (display, _)))| Item {
                id: Item::COUNT + index as u16,
                registry_key: leak(name),
                ..display.clone()
            })
            .collect();

        let block_ids: Vec<BlockId> = (0..blocks.len())
            .map(|index| BlockId::new_unchecked(BlockId::COUNT + index as u16))
            .collect();

        let mut states = Vec::with_capacity(state_count);
        let mut state_blocks = Vec::with_capacity(state_count);
        let mut state_displays = Vec::with_capacity(state_count);
        let mut real_states = Vec::with_capacity(state_count);
        // A NeoForge client numbers block states itself, appending each synced block's states in
        // block id order. Placeholders are not synced, so live blocks after one shift down.
        let mut next_real_state = BlockStateId::STATE_COUNT;
        for (((_, block), &block_id), &placeholder) in
            blocks.iter().zip(&block_ids).zip(&block_placeholders)
        {
            let display = block.display.to_state();
            for _ in 0..block.state_count {
                let id = BlockStateId::STATE_COUNT + states.len() as u16;
                real_states.push(if placeholder {
                    block.display.as_u16()
                } else {
                    let real = next_real_state;
                    next_real_state += 1;
                    real
                });
                states.push(BlockState {
                    id: BlockStateId::new_unchecked(id),
                    state_flags: display.state_flags,
                    side_flags: display.side_flags,
                    instrument: display.instrument,
                    luminance: display.luminance,
                    piston_behavior: display.piston_behavior.clone(),
                    hardness: display.hardness,
                    collision_shapes: display.collision_shapes,
                    outline_shapes: display.outline_shapes,
                    opacity: display.opacity,
                    // Custom blocks have no block entity type.
                    block_entity_type: u16::MAX,
                });
                state_blocks.push(block_id);
                state_displays.push(block.display);
            }
        }
        let states: &'static [BlockState] = Box::leak(states.into_boxed_slice());

        let mut first_state = 0;
        let custom_blocks = blocks
            .iter()
            .zip(&block_ids)
            .zip(&block_items)
            .map(|(((name, block), &id), &item_id)| {
                let block_states = &states[first_state..first_state + block.state_count];
                first_state += block.state_count;
                Block {
                    id,
                    name,
                    item_id,
                    default_state: &block_states[block.default_index],
                    states: block_states,
                    ..block.display.to_block().clone()
                }
            })
            .collect();
        let block_displays: Vec<u16> = blocks
            .iter()
            .map(|(_, block)| block.display.to_block().id.as_u16())
            .collect();
        let real_blocks = real_ids(BlockId::COUNT, &block_displays, &block_placeholders);
        let block_tags = tag_tables(
            ContentKind::Block,
            BlockId::COUNT,
            blocks
                .iter()
                .zip(&block_displays)
                .map(|((name, _), &display)| (*name, display)),
            &self.tags,
            &self.mod_tags,
        );
        let block_properties = blocks
            .into_iter()
            .map(|(_, block)| leak_properties(block.properties))
            .collect();

        #[cfg(feature = "entity_type")]
        let entity_type_displays: Vec<&'static EntityType> = self
            .entity_types
            .values()
            .map(|(display, ..)| *display)
            .collect();
        #[cfg(feature = "entity_type")]
        let real_entity_types = real_ids(
            EntityType::COUNT,
            &entity_type_displays
                .iter()
                .map(|display| display.id)
                .collect::<Vec<_>>(),
            &entity_type_placeholders,
        );
        #[cfg(feature = "entity_type")]
        let entity_types: Vec<EntityType> = self
            .entity_types
            .into_iter()
            .enumerate()
            .map(|(index, (name, (display, dimensions, eye_height)))| {
                let [width, height] = dimensions.unwrap_or(display.dimension);
                let eye_height = eye_height.unwrap_or_else(|| {
                    if dimensions.is_some() {
                        height * 0.85
                    } else {
                        display.eye_height
                    }
                });
                // A placeholder stands in for content whose mod is missing: it must never spawn.
                let summonable = display.summonable
                    && !self
                        .placeholders
                        .get(&ContentKind::EntityType)
                        .is_some_and(|names| names.contains(&name));
                EntityType {
                    id: EntityType::COUNT + index as u16,
                    summonable,
                    resource_name: leak(name),
                    dimension: [width, height],
                    eye_height,
                    ..display.clone()
                }
            })
            .collect();

        #[cfg(feature = "item")]
        let item_tags = tag_tables(
            ContentKind::Item,
            Item::COUNT,
            items
                .iter()
                .zip(&item_displays)
                .map(|(item, &display)| (item.registry_key, display)),
            &self.tags,
            &self.mod_tags,
        );
        #[cfg(feature = "entity_type")]
        let entity_type_tags = tag_tables(
            ContentKind::EntityType,
            EntityType::COUNT,
            entity_types
                .iter()
                .zip(&entity_type_displays)
                .map(|(ty, display)| (ty.resource_name, display.id)),
            &self.tags,
            &self.mod_tags,
        );

        Ok(ContentTables {
            blocks: custom_blocks,
            block_names,
            block_properties,
            states,
            state_displays,
            block_displays,
            state_blocks,
            real_states,
            real_blocks,
            #[cfg(feature = "item")]
            real_items,
            #[cfg(feature = "entity_type")]
            real_entity_types,
            #[cfg(feature = "item")]
            item_names: name_index(items.iter().map(|item| item.registry_key)),
            #[cfg(feature = "item")]
            items,
            #[cfg(feature = "item")]
            item_blocks,
            #[cfg(feature = "item")]
            item_displays,
            #[cfg(feature = "entity_type")]
            entity_type_names: name_index(entity_types.iter().map(|ty| ty.resource_name)),
            #[cfg(feature = "entity_type")]
            entity_types,
            #[cfg(feature = "entity_type")]
            entity_type_displays,
            placeholders: self.placeholders,
            block_tags,
            #[cfg(feature = "item")]
            item_tags,
            #[cfg(feature = "entity_type")]
            entity_type_tags,
            explicit_tags: self.tags,
            mod_tags: self.mod_tags,
        })
    }

    /// Whether each name, in allocation order, is a placeholder.
    fn placeholder_flags<'a>(
        &self,
        kind: ContentKind,
        names: impl Iterator<Item = &'a String>,
    ) -> Vec<bool> {
        let placeholders = self.placeholders.get(&kind);
        names
            .map(|name| placeholders.is_some_and(|set| set.contains(name)))
            .collect()
    }

    /// Resolves the item -> block links before anything is leaked, so a failed build leaks
    /// nothing. Returns the item id of each custom block and the block of each custom item. The
    /// BTreeMaps iterate in name order, which is the allocation order.
    #[cfg(feature = "item")]
    fn link_items(&self) -> Result<(Vec<u16>, Vec<Option<BlockId>>), RegistryError> {
        let block_index: HashMap<&str, usize> = self
            .blocks
            .keys()
            .enumerate()
            .map(|(index, name)| (name.as_str(), index))
            .collect();
        let mut block_items = vec![Item::AIR.id; self.blocks.len()];
        let mut item_blocks = Vec::with_capacity(self.items.len());
        for (index, (name, (_, block))) in self.items.iter().enumerate() {
            let placed = match block {
                None => None,
                Some(block) => {
                    let Some(&block_index) = block_index.get(block.as_str()) else {
                        return Err(RegistryError::UnknownBlock {
                            item: name.clone(),
                            block: block.clone(),
                        });
                    };
                    if block_items[block_index] != Item::AIR.id {
                        return Err(RegistryError::BlockAlreadyHasItem {
                            item: name.clone(),
                            block: block.clone(),
                        });
                    }
                    block_items[block_index] = Item::COUNT + index as u16;
                    Some(BlockId::new_unchecked(BlockId::COUNT + block_index as u16))
                }
            };
            item_blocks.push(placed);
        }
        Ok((block_items, item_blocks))
    }
}

/// The id a `Real` client gets for each custom entry: its own id, or the display id of a
/// placeholder.
fn real_ids(generated: u16, displays: &[u16], placeholders: &[bool]) -> Vec<u16> {
    displays
        .iter()
        .zip(placeholders)
        .enumerate()
        .map(|(index, (&display, &placeholder))| {
            if placeholder {
                display
            } else {
                generated + index as u16
            }
        })
        .collect()
}

fn leak(name: String) -> &'static str {
    Box::leak(name.into_boxed_str())
}

fn leak_properties(properties: Vec<BlockPropertyDefinition>) -> &'static [BlockProperty] {
    let properties: Vec<BlockProperty> = properties
        .into_iter()
        .map(|property| BlockProperty {
            name: leak(property.name),
            values: Box::leak(property.values.into_iter().map(leak).collect()),
        })
        .collect();
    Box::leak(properties.into_boxed_slice())
}

fn name_index(names: impl Iterator<Item = &'static str>) -> HashMap<&'static str, usize> {
    names
        .enumerate()
        .map(|(index, name)| (name, index))
        .collect()
}

fn insert_new<V>(
    map: &mut BTreeMap<String, V>,
    kind: ContentKind,
    name: String,
    value: V,
) -> Result<(), RegistryError> {
    match map.entry(name) {
        Entry::Occupied(entry) => Err(RegistryError::Duplicate {
            kind,
            name: entry.key().clone(),
        }),
        Entry::Vacant(entry) => {
            entry.insert(value);
            Ok(())
        }
    }
}

/// Accepts `namespace:path` with the characters vanilla allows in a resource location.
fn validate_name(kind: ContentKind, name: &str) -> Result<(), RegistryError> {
    let reserved = || RegistryError::ReservedNamespace {
        kind,
        name: name.to_string(),
    };
    // A name without a namespace, or with an empty one, resolves to `minecraft`.
    let Some((namespace, path)) = name.split_once(':') else {
        return Err(reserved());
    };
    if namespace.is_empty() || namespace == VANILLA_NAMESPACE {
        return Err(reserved());
    }
    if Identifier::is_valid_namespace(namespace)
        && !path.is_empty()
        && Identifier::is_valid_path(path)
    {
        Ok(())
    } else {
        Err(RegistryError::InvalidName {
            kind,
            name: name.to_string(),
        })
    }
}

/// Whether `name` is a valid name for custom content or a mod tag.
fn is_custom_name(name: &str) -> bool {
    validate_name(ContentKind::Block, name).is_ok()
}

/// The id and the bare name of a generated entry. A leading `minecraft:` is optional.
fn generated_entry(kind: ContentKind, name: &str) -> Option<(u16, &'static str)> {
    match kind {
        ContentKind::Block => Block::from_name(name)
            .filter(|block| block.id.as_u16() < BlockId::COUNT)
            .map(|block| (block.id.as_u16(), block.name)),
        #[cfg(feature = "item")]
        ContentKind::Item => Item::from_registry_key(name)
            .filter(|item| item.id < Item::COUNT)
            .map(|item| (item.id, item.registry_key)),
        #[cfg(not(feature = "item"))]
        ContentKind::Item => None,
        #[cfg(feature = "entity_type")]
        ContentKind::EntityType => EntityType::from_name(name)
            .filter(|ty| ty.id < EntityType::COUNT)
            .map(|ty| (ty.id, ty.resource_name)),
        #[cfg(not(feature = "entity_type"))]
        ContentKind::EntityType => None,
    }
}

/// The canonical name of a mod tag value: a custom entry name as it is, checked at the freeze, or
/// the namespaced name of a generated entry. `None` for a generated entry that does not exist.
fn canonical_member(kind: ContentKind, value: &str) -> Option<String> {
    if is_custom_name(value) {
        return Some(value.to_string());
    }
    generated_entry(kind, value).map(|(_, name)| namespaced_name(name).into_owned())
}

fn check_range(registry: &'static str, generated: u16, count: usize) -> Result<(), RegistryError> {
    if count > usize::from(MAX_ID - generated) + 1 {
        return Err(RegistryError::RangeExceeded { registry, count });
    }
    Ok(())
}

/// Finds a generated tag as `Taggable::is_tagged_with` does: a leading `#` is optional and a name
/// without a namespace uses `minecraft`. Returns the generated tag name and its generated ids.
fn resolve_tag(key: RegistryKey, tag: &str) -> Option<(&'static str, &'static [u16])> {
    let map = get_latest_map(key);
    let tag = tag.strip_prefix('#').unwrap_or(tag);
    let (name, members) = map.get_entry(tag).or_else(|| {
        if tag.contains(':') {
            map.get_entry(tag.strip_prefix("minecraft:")?)
        } else {
            map.get_entry(format!("{VANILLA_NAMESPACE}:{tag}").as_str())
        }
    })?;
    Some((name, members.1))
}

/// Resolves the tags of a definition: a generated tag to its generated name, any other
/// `namespace:path` outside `minecraft` to a mod tag name that [`ContentBuilder::check_tags`]
/// checks at the freeze.
fn resolve_tags(
    kind: ContentKind,
    name: &str,
    tags: &[String],
) -> Result<BTreeSet<String>, RegistryError> {
    tags.iter()
        .map(|tag| {
            if let Some((generated, _)) = resolve_tag(kind.tag_key(), tag) {
                return Ok(generated.to_string());
            }
            let mod_tag = tag.strip_prefix('#').unwrap_or(tag);
            if is_custom_name(mod_tag) {
                Ok(mod_tag.to_string())
            } else {
                Err(RegistryError::UnknownTag {
                    kind,
                    name: name.to_string(),
                    tag: tag.clone(),
                })
            }
        })
        .collect()
}

/// Builds the tag tables of one registry. `entries` yields the name and the display id of each
/// custom entry, in id order from `first_id`. Runs after [`ContentBuilder::check_tags`].
fn tag_tables<'a>(
    kind: ContentKind,
    first_id: u16,
    entries: impl Iterator<Item = (&'a str, u16)>,
    explicit: &NamedSets,
    mod_tags: &NamedSets,
) -> TagTables {
    let key = kind.tag_key();
    let explicit = explicit.get(&kind).unwrap_or(&NO_SETS);
    let mod_tags = mod_tags.get(&kind).unwrap_or(&NO_SETS);
    let mod_names: HashMap<&str, (&'static str, &BTreeSet<String>)> = mod_tags
        .iter()
        .map(|(name, values)| (name.as_str(), (leak(name.clone()), values)))
        .collect();
    let tag_name = |tag: &str| {
        resolve_tag(key, tag)
            .map(|(name, _)| name)
            .or_else(|| mod_names.get(tag).map(|&(name, _)| name))
    };

    let mut members: HashMap<&'static str, Vec<u16>> = HashMap::new();
    let mut by_display: HashMap<u16, Vec<u16>> = HashMap::new();
    let mut ids: HashMap<&str, u16> = HashMap::new();
    for (index, (name, display)) in entries.enumerate() {
        let id = first_id + index as u16;
        by_display.entry(display).or_default().push(id);
        ids.insert(name, id);
        for tag in explicit.get(name).into_iter().flatten() {
            if let Some(tag) = tag_name(tag) {
                members.entry(tag).or_default().push(id);
            }
        }
    }
    if !by_display.is_empty() {
        for (name, tag) in get_latest_map(key).entries() {
            for display in tag.1 {
                if let Some(ids) = by_display.get(display) {
                    members.entry(name).or_default().extend(ids);
                }
            }
        }
    }

    let mut generated_members = HashMap::new();
    for &(tag, values) in mod_names.values() {
        let mut generated = Vec::new();
        for value in values {
            if let Some(&id) = ids.get(value.as_str()) {
                members.entry(tag).or_default().push(id);
            } else if let Some((id, _)) = generated_entry(kind, value) {
                generated.push(id);
            }
        }
        generated_members.insert(tag, sorted_ids(generated));
    }
    TagTables {
        custom: members
            .into_iter()
            .map(|(name, ids)| (name, sorted_ids(ids)))
            .collect(),
        mod_tags: generated_members,
    }
}

fn sorted_ids(mut ids: Vec<u16>) -> Box<[u16]> {
    ids.sort_unstable();
    ids.dedup();
    ids.into_boxed_slice()
}

static PENDING: Mutex<Option<ContentBuilder>> = Mutex::new(Some(ContentBuilder::new()));
static TABLES: OnceLock<ContentTables> = OnceLock::new();

fn with_builder(
    register: impl FnOnce(&mut ContentBuilder) -> Result<(), RegistryError>,
) -> Result<(), RegistryError> {
    let mut pending = PENDING.lock().unwrap_or_else(PoisonError::into_inner);
    pending
        .as_mut()
        .map_or(Err(RegistryError::RegistryFrozen), register)
}

/// Registers a custom block. Its id is allocated by [`freeze`].
pub fn register_block(definition: BlockDefinition) -> Result<(), RegistryError> {
    with_builder(|builder| builder.register_block(definition))
}

/// Registers a custom item. Its id is allocated by [`freeze`].
#[cfg(feature = "item")]
pub fn register_item(definition: ItemDefinition) -> Result<(), RegistryError> {
    with_builder(|builder| builder.register_item(definition))
}

/// Registers a custom entity type. Its id is allocated by [`freeze`].
#[cfg(feature = "entity_type")]
pub fn register_entity_type(definition: EntityTypeDefinition) -> Result<(), RegistryError> {
    with_builder(|builder| builder.register_entity_type(definition))
}

/// Registers a mod tag. Its custom members and the entries that list it resolve at [`freeze`],
/// so the order of tag and entry registrations does not matter.
pub fn register_tag(definition: TagDefinition) -> Result<(), RegistryError> {
    with_builder(|builder| builder.register_tag(definition))
}

/// Returns whether `name` is registered as a mod tag of this registry and waits for the freeze.
/// Always `false` after the freeze.
#[must_use]
pub fn is_tag_registered(kind: ContentKind, name: &str) -> bool {
    PENDING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .and_then(|builder| builder.mod_tags.get(&kind))
        .is_some_and(|tags| tags.contains_key(name))
}

/// Registers a placeholder block: content that the world's content manifest lists and no mod
/// registered. It works like [`register_block`], and [`is_placeholder`] reports it after the
/// freeze.
pub fn register_placeholder_block(definition: BlockDefinition) -> Result<(), RegistryError> {
    let name = definition.name.clone();
    with_builder(|builder| {
        builder.register_block(definition)?;
        builder.mark_placeholder(ContentKind::Block, name);
        Ok(())
    })
}

/// Registers a placeholder item. See [`register_placeholder_block`].
#[cfg(feature = "item")]
pub fn register_placeholder_item(definition: ItemDefinition) -> Result<(), RegistryError> {
    let name = definition.name.clone();
    with_builder(|builder| {
        builder.register_item(definition)?;
        builder.mark_placeholder(ContentKind::Item, name);
        Ok(())
    })
}

/// Registers a placeholder entity type. See [`register_placeholder_block`]. The type is not
/// summonable and has no spawn factory, so it never spawns.
#[cfg(feature = "entity_type")]
pub fn register_placeholder_entity_type(
    definition: EntityTypeDefinition,
) -> Result<(), RegistryError> {
    let name = definition.name.clone();
    with_builder(|builder| {
        builder.register_entity_type(definition)?;
        builder.mark_placeholder(ContentKind::EntityType, name);
        Ok(())
    })
}

/// Returns whether `name` is registered as this kind and waits for the freeze. Always `false`
/// after the freeze.
#[must_use]
pub fn is_registered(kind: ContentKind, name: &str) -> bool {
    PENDING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .is_some_and(|builder| builder.contains(kind, name))
}

/// Returns whether the frozen registry holds `name` as a placeholder of this kind.
#[must_use]
pub fn is_placeholder(kind: ContentKind, name: &str) -> bool {
    TABLES.get().is_some_and(|tables| {
        tables
            .placeholders
            .get(&kind)
            .is_some_and(|names| names.contains(name))
    })
}

#[cfg(feature = "entity_type")]
impl EntityType {
    /// Whether this is a placeholder type, which stands in for a missing mod's type and never
    /// spawns.
    #[must_use]
    pub fn is_placeholder(&self) -> bool {
        self.id >= Self::COUNT && is_placeholder(ContentKind::EntityType, self.resource_name)
    }
}

impl BlockStateId {
    /// The state a vanilla client sees: the state itself, or the display state of a custom state.
    #[must_use]
    pub fn display_state(self) -> Self {
        if self.as_u16() < Self::STATE_COUNT {
            self
        } else {
            display_state(self)
        }
    }

    /// The id a client in this mode knows. A generated state keeps its id. A custom state has the
    /// display state's id in `Display` mode and the client-side id in `Real` mode, except a
    /// placeholder state, which keeps the display id. The client-side id differs from the server
    /// id when a placeholder block sorts before the block: the client numbers states itself and
    /// skips unsynced blocks. Packet code writes block state ids only through this function.
    #[inline]
    #[must_use]
    pub fn to_java_network_id(self, ids: ContentIds) -> u16 {
        if self.as_u16() < Self::STATE_COUNT {
            self.as_u16()
        } else {
            match ids {
                ContentIds::Display => display_state(self).as_u16(),
                ContentIds::Real => real_id(self.as_u16(), Self::STATE_COUNT, |tables| {
                    &tables.real_states
                })
                .unwrap_or_else(|| display_state(self).as_u16()),
            }
        }
    }
}

/// Allocates the ids of all registered content and installs the tables. Runs once: later calls,
/// and registrations after it, return [`RegistryError::RegistryFrozen`].
pub fn freeze() -> Result<&'static ContentTables, RegistryError> {
    let builder = PENDING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
        .ok_or(RegistryError::RegistryFrozen)?;
    let tables = builder.build()?;
    Ok(TABLES.get_or_init(|| tables))
}

/// The tables that [`freeze`] installed, or `None` before the freeze and after a failed freeze.
#[must_use]
pub fn tables() -> Option<&'static ContentTables> {
    TABLES.get()
}

/// Returns the namespaced form of a block, item or entity type name. Generated content keeps the
/// bare vanilla name, custom content carries its own namespace.
#[must_use]
pub fn namespaced_name(name: &str) -> Cow<'_, str> {
    if name.contains(':') {
        Cow::Borrowed(name)
    } else {
        Cow::Owned(format!("{VANILLA_NAMESPACE}:{name}"))
    }
}

/// The `Real` mode id of a custom entry from one of the `real_*` tables.
fn real_id(
    raw: u16,
    generated: u16,
    table: impl FnOnce(&'static ContentTables) -> &'static [u16],
) -> Option<u16> {
    TABLES
        .get()
        .and_then(|tables| table(tables).get(custom_index(raw, generated)?))
        .copied()
}

fn custom_index(raw: u16, generated: u16) -> Option<usize> {
    raw.checked_sub(generated).map(usize::from)
}

#[cold]
#[inline(never)]
pub(crate) fn is_custom_state(raw: u16) -> bool {
    TABLES.get().is_some_and(|tables| {
        custom_index(raw, BlockStateId::STATE_COUNT)
            .is_some_and(|index| index < tables.states.len())
    })
}

#[cold]
#[inline(never)]
pub(crate) fn is_custom_block(raw: u16) -> bool {
    TABLES.get().is_some_and(|tables| {
        custom_index(raw, BlockId::COUNT).is_some_and(|index| index < tables.blocks.len())
    })
}

// A `BlockStateId` or `BlockId` above the generated count only exists after its constructor
// found it in the installed tables, and the tables never change after that.

#[cold]
#[inline(never)]
#[expect(
    clippy::unreachable,
    reason = "BlockStateId::new checks the installed tables"
)]
pub(crate) fn state(id: BlockStateId) -> &'static BlockState {
    TABLES
        .get()
        .and_then(|tables| {
            tables
                .states
                .get(custom_index(id.as_u16(), BlockStateId::STATE_COUNT)?)
        })
        .unwrap_or_else(|| unreachable!("{id:?} is not an installed custom state"))
}

#[cold]
#[inline(never)]
#[expect(
    clippy::unreachable,
    reason = "BlockStateId::new checks the installed tables"
)]
pub(crate) fn block_id_of_state(id: BlockStateId) -> BlockId {
    TABLES
        .get()
        .and_then(|tables| {
            tables
                .state_blocks
                .get(custom_index(id.as_u16(), BlockStateId::STATE_COUNT)?)
        })
        .copied()
        .unwrap_or_else(|| unreachable!("{id:?} is not an installed custom state"))
}

#[cold]
#[inline(never)]
#[expect(
    clippy::unreachable,
    reason = "BlockStateId::new checks the installed tables"
)]
pub(crate) fn display_state(id: BlockStateId) -> BlockStateId {
    TABLES
        .get()
        .and_then(|tables| {
            tables
                .state_displays
                .get(custom_index(id.as_u16(), BlockStateId::STATE_COUNT)?)
        })
        .copied()
        .unwrap_or_else(|| unreachable!("{id:?} is not an installed custom state"))
}

#[cold]
#[inline(never)]
#[expect(
    clippy::unreachable,
    reason = "BlockId::new checks the installed tables"
)]
pub(crate) fn block(id: BlockId) -> &'static Block {
    TABLES
        .get()
        .and_then(|tables| {
            tables
                .blocks
                .get(custom_index(id.as_u16(), BlockId::COUNT)?)
        })
        .unwrap_or_else(|| unreachable!("{id:?} is not an installed custom block"))
}

#[cold]
#[inline(never)]
pub(crate) fn block_by_name(name: &str) -> Option<&'static Block> {
    let tables = TABLES.get()?;
    tables.blocks.get(*tables.block_names.get(name)?)
}

#[cold]
#[inline(never)]
#[expect(
    clippy::unreachable,
    reason = "BlockId::new checks the installed tables"
)]
fn display_block(id: BlockId) -> u16 {
    TABLES
        .get()
        .and_then(|tables| {
            tables
                .block_displays
                .get(custom_index(id.as_u16(), BlockId::COUNT)?)
        })
        .copied()
        .unwrap_or_else(|| unreachable!("{id:?} is not an installed custom block"))
}

impl Block {
    /// The id a client in this mode knows. See [`BlockStateId::to_java_network_id`].
    #[must_use]
    pub fn to_java_network_id(&self, ids: ContentIds) -> u16 {
        if self.id.as_u16() < BlockId::COUNT {
            self.id.as_u16()
        } else {
            match ids {
                ContentIds::Display => display_block(self.id),
                ContentIds::Real => real_id(self.id.as_u16(), BlockId::COUNT, |tables| {
                    &tables.real_blocks
                })
                .unwrap_or_else(|| display_block(self.id)),
            }
        }
    }
}

fn properties_of(block: BlockId) -> &'static [BlockProperty] {
    TABLES
        .get()
        .and_then(|tables| {
            tables
                .block_properties
                .get(custom_index(block.as_u16(), BlockId::COUNT)?)
        })
        .copied()
        .unwrap_or_default()
}

/// The fallthrough of `Block::properties`. A custom block without properties has none, as a
/// generated one.
#[cold]
#[inline(never)]
pub(crate) fn properties(
    block: &Block,
    state_id: BlockStateId,
) -> Option<Box<dyn BlockProperties>> {
    if properties_of(block.id).is_empty() {
        return None;
    }
    Some(Box::new(DynamicProperties::from_state_id(state_id, block)))
}

/// The fallthrough of `Block::from_properties`: a registered block, or a vanilla block without
/// properties (one state, the default).
#[cold]
#[inline(never)]
pub(crate) fn properties_from_props(
    block: &Block,
    props: &[(&str, &str)],
) -> Box<dyn BlockProperties> {
    Box::new(DynamicProperties::from_props(props, block))
}

/// The property values of one state of a custom block: the state's index in the block's states.
#[derive(Debug, Clone, Copy)]
pub struct DynamicProperties {
    properties: &'static [BlockProperty],
    index: u16,
}

impl DynamicProperties {
    /// The value index of each property, in property order.
    fn value_indices(&self) -> Vec<usize> {
        let mut index = usize::from(self.index);
        let mut values: Vec<usize> = self
            .properties
            .iter()
            .rev()
            .map(|property| {
                let value = index % property.values.len();
                index /= property.values.len();
                value
            })
            .collect();
        values.reverse();
        values
    }
}

impl BlockProperties for DynamicProperties {
    fn to_index(&self) -> u16 {
        self.index
    }

    /// The index alone does not name the block, so the value has no properties: `to_state_id`
    /// works and `to_props` is empty. Use `from_state_id` to read the values.
    fn from_index(index: u16) -> Self {
        Self {
            properties: &[],
            index,
        }
    }

    fn handles_block_id(id: BlockId) -> bool {
        is_custom_block(id.as_u16())
    }

    fn to_state_id(&self, block: &Block) -> BlockStateId {
        block.states[usize::from(self.index)].id
    }

    fn from_state_id(id: BlockStateId, block: &Block) -> Self {
        let index = id.as_u16().wrapping_sub(block.states[0].id.as_u16());
        debug_assert!(
            usize::from(index) < block.states.len(),
            "state id {id} does not exist for {}",
            block.name
        );
        Self {
            properties: properties_of(block.id),
            index: if usize::from(index) < block.states.len() {
                index
            } else {
                0
            },
        }
    }

    fn default(block: &Block) -> Self {
        Self::from_state_id(block.default_state.id, block)
    }

    fn to_props(&self) -> Vec<(&'static str, &'static str)> {
        self.properties
            .iter()
            .zip(self.value_indices())
            .map(|(property, value)| (property.name, property.values[value]))
            .collect()
    }

    /// Starts from the default state. Unknown names and values keep the default value.
    fn from_props(props: &[(&str, &str)], block: &Block) -> Self {
        let default = Self::default(block);
        let mut values = default.value_indices();
        for (name, value) in props {
            let Some(property) = default.properties.iter().position(|p| p.name == *name) else {
                continue;
            };
            if let Some(value) = default.properties[property]
                .values
                .iter()
                .position(|v| v == value)
            {
                values[property] = value;
            }
        }
        let index = default
            .properties
            .iter()
            .zip(values)
            .fold(0, |index, (property, value)| {
                index * property.values.len() + value
            });
        Self {
            properties: default.properties,
            index: index as u16,
        }
    }
}

/// The first custom item id. `Block::from_item_id` answers ids below it inline.
#[cfg(feature = "item")]
pub(crate) const ITEM_COUNT: u16 = Item::COUNT;
/// Without the item feature no custom item exists, so every id stays on the generated path.
#[cfg(not(feature = "item"))]
pub(crate) const ITEM_COUNT: u16 = u16::MAX;

#[cold]
#[inline(never)]
pub(crate) fn block_from_item_id(id: u16) -> Option<&'static Block> {
    #[cfg(feature = "item")]
    {
        let tables = TABLES.get()?;
        let block_id = (*tables.item_blocks.get(custom_index(id, Item::COUNT)?)?)?;
        tables
            .blocks
            .get(custom_index(block_id.as_u16(), BlockId::COUNT)?)
    }
    #[cfg(not(feature = "item"))]
    {
        let _ = id;
        None
    }
}

#[cold]
#[inline(never)]
#[cfg(feature = "item")]
pub(crate) fn item(id: u16) -> Option<&'static Item> {
    TABLES.get()?.items.get(custom_index(id, Item::COUNT)?)
}

#[cold]
#[inline(never)]
#[cfg(feature = "item")]
pub(crate) fn item_by_name(name: &str) -> Option<&'static Item> {
    let tables = TABLES.get()?;
    tables.items.get(*tables.item_names.get(name)?)
}

#[cold]
#[inline(never)]
#[cfg(feature = "item")]
fn display_item(id: u16) -> u16 {
    // `Item` has pub fields, so an id that no table holds can exist: send air, not a bad id.
    TABLES
        .get()
        .and_then(|tables| tables.item_displays.get(custom_index(id, Item::COUNT)?))
        .copied()
        .unwrap_or(Item::AIR.id)
}

#[cfg(feature = "item")]
impl Item {
    /// The id a client in this mode knows. See [`BlockStateId::to_java_network_id`]. Packet code
    /// writes item ids only through this function.
    #[must_use]
    pub fn to_java_network_id(&self, ids: ContentIds) -> u16 {
        if self.id < Self::COUNT {
            self.id
        } else {
            match ids {
                ContentIds::Display => display_item(self.id),
                ContentIds::Real => real_id(self.id, Self::COUNT, |tables| &tables.real_items)
                    .unwrap_or_else(|| display_item(self.id)),
            }
        }
    }

    /// The `show_item` hover event for a stack of this item. The client decodes the id, so a
    /// custom item shows its display item.
    #[must_use]
    pub fn show_item_hover(&self, count: Option<i32>) -> HoverEvent {
        let display =
            Self::from_id(self.to_java_network_id(ContentIds::Display)).unwrap_or(&Self::AIR);
        HoverEvent::ShowItem {
            id: display.registry_key.into(),
            count,
        }
    }
}

#[cold]
#[inline(never)]
#[cfg(feature = "entity_type")]
pub(crate) fn entity_type(id: u16) -> Option<&'static EntityType> {
    TABLES
        .get()?
        .entity_types
        .get(custom_index(id, EntityType::COUNT)?)
}

#[cold]
#[inline(never)]
#[cfg(feature = "entity_type")]
pub(crate) fn entity_type_by_name(name: &str) -> Option<&'static EntityType> {
    let tables = TABLES.get()?;
    tables
        .entity_types
        .get(*tables.entity_type_names.get(name)?)
}

#[cold]
#[inline(never)]
#[cfg(feature = "entity_type")]
fn display_entity_type(id: u16) -> &'static EntityType {
    // `EntityType` has pub fields, so an id that no table holds can exist: send a type every
    // client knows and that renders nothing, not a bad id.
    TABLES
        .get()
        .and_then(|tables| {
            tables
                .entity_type_displays
                .get(custom_index(id, EntityType::COUNT)?)
        })
        .copied()
        .unwrap_or(&EntityType::MARKER)
}

#[cfg(feature = "entity_type")]
impl EntityType {
    /// The type a vanilla client sees: the type itself, or the display type of a custom type.
    #[must_use]
    pub fn display_type(&'static self) -> &'static Self {
        if self.id < Self::COUNT {
            self
        } else {
            display_entity_type(self.id)
        }
    }

    /// The id a client in this mode knows. See [`BlockStateId::to_java_network_id`]. Packet code
    /// writes entity type ids only through this function.
    #[must_use]
    pub fn to_java_network_id(&self, ids: ContentIds) -> u16 {
        if self.id < Self::COUNT {
            self.id
        } else {
            match ids {
                ContentIds::Display => display_entity_type(self.id).id,
                ContentIds::Real => {
                    real_id(self.id, Self::COUNT, |tables| &tables.real_entity_types)
                        .unwrap_or_else(|| display_entity_type(self.id).id)
                }
            }
        }
    }
}

fn tag_tables_of(key: RegistryKey) -> Option<&'static TagTables> {
    let tables = TABLES.get()?;
    match key {
        RegistryKey::Block => Some(&tables.block_tags),
        #[cfg(feature = "item")]
        RegistryKey::Item => Some(&tables.item_tags),
        #[cfg(feature = "entity_type")]
        RegistryKey::EntityType => Some(&tables.entity_type_tags),
        _ => None,
    }
}

/// The custom ids in a generated or mod tag, by tag name.
fn custom_tag_ids(key: RegistryKey, tag: &str) -> &'static [u16] {
    tag_tables_of(key)
        .and_then(|tables| tables.custom.get(tag))
        .map_or(&[], |ids| ids)
}

/// Finds a generated tag as [`resolve_tag`] does, or else a mod tag of the installed tables.
/// Returns the tag name and its generated ids.
fn resolve_any_tag(key: RegistryKey, tag: &str) -> Option<(&'static str, &'static [u16])> {
    resolve_tag(key, tag).or_else(|| {
        let tag = tag.strip_prefix('#').unwrap_or(tag);
        let (name, members) = tag_tables_of(key)?.mod_tags.get_key_value(tag)?;
        Some((*name, &**members))
    })
}

/// The members of a generated or mod tag: its generated ids, then its custom ids sorted by id.
/// `None` when the registry has no such tag. The name follows `Taggable::is_tagged_with`: a
/// leading `#` is optional and a name without a namespace uses `minecraft`.
#[must_use]
pub fn tag_ids(key: RegistryKey, tag: &str) -> Option<(&'static [u16], &'static [u16])> {
    let (name, generated) = resolve_any_tag(key, tag)?;
    Some((generated, custom_tag_ids(key, name)))
}

/// The members of a generated tag as a client in this mode gets them in `update_tags`. A
/// `Display` client gets the generated ids only. A `Real` client also gets the custom ids,
/// without placeholders, which no client knows.
#[must_use]
pub fn network_tag_ids(
    key: RegistryKey,
    tag: &str,
    generated: &'static [u16],
    ids: ContentIds,
) -> Cow<'static, [u16]> {
    if ids == ContentIds::Display {
        return Cow::Borrowed(generated);
    }
    let custom = custom_tag_ids(key, tag);
    if custom.is_empty() {
        return Cow::Borrowed(generated);
    }
    Cow::Owned(
        generated
            .iter()
            .chain(custom)
            .copied()
            .filter(|&id| is_synced_id(key, id))
            .collect(),
    )
}

/// The mod tags of a registry as a client in this mode gets them in `update_tags`, in name order,
/// each with its generated and custom members. Only a `Real` client knows mod tags.
#[must_use]
pub fn network_mod_tags(key: RegistryKey, ids: ContentIds) -> Vec<(&'static str, Vec<u16>)> {
    let Some(tables) = tag_tables_of(key).filter(|_| ids == ContentIds::Real) else {
        return Vec::new();
    };
    let mut tags: Vec<(&'static str, Vec<u16>)> = tables
        .mod_tags
        .iter()
        .map(|(&name, generated)| {
            let members = generated
                .iter()
                .chain(custom_tag_ids(key, name))
                .copied()
                .filter(|&id| is_synced_id(key, id))
                .collect();
            (name, members)
        })
        .collect();
    tags.sort_unstable_by_key(|(name, _)| *name);
    tags
}

/// Whether a `Real` client knows the id: a generated id or a custom id that is not a placeholder.
fn is_synced_id(key: RegistryKey, id: u16) -> bool {
    let Some(tables) = TABLES.get() else {
        return true;
    };
    let (generated, real): (u16, &[u16]) = match key {
        RegistryKey::Block => (BlockId::COUNT, &tables.real_blocks),
        #[cfg(feature = "item")]
        RegistryKey::Item => (Item::COUNT, &tables.real_items),
        #[cfg(feature = "entity_type")]
        RegistryKey::EntityType => (EntityType::COUNT, &tables.real_entity_types),
        _ => return true,
    };
    custom_index(id, generated).is_none_or(|index| real.get(index) == Some(&id))
}

/// Tag membership that includes custom content. It reads both tag tables, so it is for paths off
/// the worldgen hot path; `BlockId::has_tag` and the generated [`Taggable`] methods answer for
/// generated ids only.
pub trait DynamicTaggable: Taggable {
    /// The first custom id of the registry.
    const FIRST_CUSTOM_ID: u16;

    /// Whether the entry is in the generated or mod tag: from the generated members for a
    /// generated entry, from the custom tag table for a custom one. `false` for a tag the registry
    /// does not have. The name follows [`Taggable::is_tagged_with`].
    #[must_use]
    fn has_tag_dynamic(&self, tag: &str) -> bool {
        let Some((name, generated)) = resolve_any_tag(Self::tag_key(), tag) else {
            return false;
        };
        let id = self.registry_id();
        if id < Self::FIRST_CUSTOM_ID {
            generated.contains(&id)
        } else {
            custom_tag_ids(Self::tag_key(), name)
                .binary_search(&id)
                .is_ok()
        }
    }
}

impl DynamicTaggable for Block {
    const FIRST_CUSTOM_ID: u16 = BlockId::COUNT;
}

#[cfg(feature = "item")]
impl DynamicTaggable for Item {
    const FIRST_CUSTOM_ID: u16 = Item::COUNT;
}

#[cfg(feature = "entity_type")]
impl DynamicTaggable for EntityType {
    const FIRST_CUSTOM_ID: u16 = EntityType::COUNT;
}

#[cfg(all(test, feature = "item", feature = "entity_type"))]
mod tests {
    use super::*;
    use crate::tag::Taggable;

    fn block(name: &str, display: &Block) -> BlockDefinition {
        BlockDefinition {
            name: name.to_string(),
            display: display.default_state.id,
            properties: Vec::new(),
            tags: Vec::new(),
        }
    }

    fn item(name: &str, block: Option<&str>) -> ItemDefinition {
        ItemDefinition {
            name: name.to_string(),
            display: &Item::DIAMOND_SWORD,
            block: block.map(str::to_string),
            tags: Vec::new(),
        }
    }

    fn entity_type(name: &str) -> EntityTypeDefinition {
        EntityTypeDefinition {
            name: name.to_string(),
            display: &EntityType::ZOMBIE,
            dimensions: None,
            eye_height: None,
            tags: Vec::new(),
        }
    }

    fn register_all(builder: &mut ContentBuilder, names: &[&str]) {
        for name in names {
            builder.register_block(block(name, &Block::STONE)).unwrap();
            builder.register_item(item(name, Some(name))).unwrap();
            builder.register_entity_type(entity_type(name)).unwrap();
        }
    }

    fn allocation(tables: &ContentTables) -> Vec<(&'static str, u16, u16, u16, u16)> {
        tables
            .blocks()
            .iter()
            .zip(tables.items())
            .zip(tables.entity_types())
            .map(|((block, item), entity_type)| {
                (
                    block.name,
                    block.id.as_u16(),
                    block.default_state.id.as_u16(),
                    item.id,
                    entity_type.id,
                )
            })
            .collect()
    }

    #[test]
    fn allocation_does_not_depend_on_registration_order() {
        let mut forward = ContentBuilder::new();
        register_all(&mut forward, &["b:middle", "a:first", "c:last"]);
        let mut backward = ContentBuilder::new();
        register_all(&mut backward, &["c:last", "a:first", "b:middle"]);

        let forward = forward.build().unwrap();
        let backward = backward.build().unwrap();

        let block = BlockId::COUNT;
        let state = BlockStateId::STATE_COUNT;
        let item = Item::COUNT;
        let entity = EntityType::COUNT;
        assert_eq!(
            allocation(&forward),
            vec![
                ("a:first", block, state, item, entity),
                ("b:middle", block + 1, state + 1, item + 1, entity + 1),
                ("c:last", block + 2, state + 2, item + 2, entity + 2),
            ]
        );
        assert_eq!(allocation(&forward), allocation(&backward));
        for (block, item) in forward.blocks().iter().zip(forward.items()) {
            assert_eq!(block.item_id, item.id);
        }
    }

    #[test]
    fn rejects_duplicates_and_reserved_names() {
        let mut builder = ContentBuilder::new();
        builder
            .register_block(block("mymod:lamp", &Block::STONE))
            .unwrap();
        assert_eq!(
            builder.register_block(block("mymod:lamp", &Block::DIRT)),
            Err(RegistryError::Duplicate {
                kind: ContentKind::Block,
                name: "mymod:lamp".to_string()
            })
        );
        // The same name in another registry is a different entry, as in vanilla.
        builder.register_item(item("mymod:lamp", None)).unwrap();

        for name in ["minecraft:lamp", "lamp", ":lamp"] {
            assert_eq!(
                builder.register_block(block(name, &Block::STONE)),
                Err(RegistryError::ReservedNamespace {
                    kind: ContentKind::Block,
                    name: name.to_string()
                })
            );
        }
        for name in ["MyMod:lamp", "mymod:", "mymod:lamp:2", "my mod:lamp"] {
            assert!(matches!(
                builder.register_entity_type(entity_type(name)),
                Err(RegistryError::InvalidName { .. })
            ));
        }
    }

    #[test]
    fn entity_type_dimensions_override_the_display_type() {
        let mut builder = ContentBuilder::new();
        builder
            .register_entity_type(EntityTypeDefinition {
                dimensions: Some([2.0, 3.0]),
                ..entity_type("mymod:big")
            })
            .unwrap();
        builder
            .register_entity_type(entity_type("mymod:same"))
            .unwrap();
        builder
            .register_entity_type(EntityTypeDefinition {
                dimensions: Some([2.0, 3.0]),
                eye_height: Some(2.0),
                ..entity_type("mymod:eyes")
            })
            .unwrap();
        builder
            .register_entity_type(EntityTypeDefinition {
                eye_height: Some(1.25),
                ..entity_type("mymod:eyes_only")
            })
            .unwrap();
        for dimensions in [
            [0.0, 1.0],
            [1.0, -1.0],
            [f32::NAN, 1.0],
            [1.0, f32::INFINITY],
        ] {
            assert_eq!(
                builder.register_entity_type(EntityTypeDefinition {
                    dimensions: Some(dimensions),
                    ..entity_type("mymod:bad")
                }),
                Err(RegistryError::InvalidDimensions {
                    name: "mymod:bad".to_string()
                })
            );
        }
        for eye_height in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(
                builder.register_entity_type(EntityTypeDefinition {
                    eye_height: Some(eye_height),
                    ..entity_type("mymod:bad")
                }),
                Err(RegistryError::InvalidDimensions {
                    name: "mymod:bad".to_string()
                })
            );
        }

        let tables = builder.build().unwrap();
        let [big, eyes, eyes_only, same] = tables.entity_types() else {
            panic!("expected four entity types");
        };
        assert_eq!(big.dimension, [2.0, 3.0]);
        assert!((big.eye_height - 2.55).abs() < 1e-6);
        assert_eq!(eyes.dimension, [2.0, 3.0]);
        assert_eq!(eyes.eye_height, 2.0);
        assert_eq!(eyes_only.dimension, EntityType::ZOMBIE.dimension);
        assert_eq!(eyes_only.eye_height, 1.25);
        assert_eq!(same.dimension, EntityType::ZOMBIE.dimension);
        assert_eq!(same.eye_height, EntityType::ZOMBIE.eye_height);
        assert_eq!(tables.entity_type_displays, [&EntityType::ZOMBIE; 4]);
    }

    #[test]
    fn rejects_items_for_unknown_or_taken_blocks() {
        let mut builder = ContentBuilder::new();
        builder
            .register_item(item("mymod:a", Some("mymod:missing")))
            .unwrap();
        assert_eq!(
            builder.build().unwrap_err(),
            RegistryError::UnknownBlock {
                item: "mymod:a".to_string(),
                block: "mymod:missing".to_string()
            }
        );

        let mut builder = ContentBuilder::new();
        builder
            .register_block(block("mymod:lamp", &Block::STONE))
            .unwrap();
        builder
            .register_item(item("mymod:a", Some("mymod:lamp")))
            .unwrap();
        builder
            .register_item(item("mymod:b", Some("mymod:lamp")))
            .unwrap();
        assert_eq!(
            builder.build().unwrap_err(),
            RegistryError::BlockAlreadyHasItem {
                item: "mymod:b".to_string(),
                block: "mymod:lamp".to_string()
            }
        );
    }

    #[test]
    fn failed_item_link_fails_before_allocation() {
        let mut failing = ContentBuilder::new();
        register_all(&mut failing, &["mymod:a", "mymod:b"]);
        failing
            .register_item(item("mymod:c", Some("mymod:missing")))
            .unwrap();
        assert_eq!(
            failing.build().unwrap_err(),
            RegistryError::UnknownBlock {
                item: "mymod:c".to_string(),
                block: "mymod:missing".to_string()
            }
        );

        let mut fixed = ContentBuilder::new();
        register_all(&mut fixed, &["mymod:a", "mymod:b"]);
        fixed.register_item(item("mymod:c", None)).unwrap();
        let tables = fixed.build().unwrap();
        assert_eq!(tables.blocks().len(), 2);
        assert_eq!(tables.items().len(), 3);
        assert_eq!(tables.items()[2].id, Item::COUNT + 2);
        assert_eq!(tables.blocks()[1].item_id, Item::COUNT + 1);
    }

    #[test]
    fn rejects_more_blocks_than_free_state_ids() {
        let free = usize::from(MAX_ID - BlockStateId::STATE_COUNT) + 1;
        assert_eq!(free, 29_812);
        let mut builder = ContentBuilder::new();
        for index in 0..=free {
            builder
                .register_block(block(&format!("mymod:b{index}"), &Block::STONE))
                .unwrap();
        }
        assert_eq!(
            builder.build().unwrap_err(),
            RegistryError::RangeExceeded {
                registry: "block state",
                count: free + 1
            }
        );
    }

    #[test]
    fn rejects_content_beyond_the_id_range() {
        let free = usize::from(MAX_ID - BlockStateId::STATE_COUNT) + 1;
        assert!(check_range("block state", BlockStateId::STATE_COUNT, free).is_ok());
        assert_eq!(
            check_range("block state", BlockStateId::STATE_COUNT, free + 1),
            Err(RegistryError::RangeExceeded {
                registry: "block state",
                count: free + 1
            })
        );
    }

    fn block_with(name: &str, properties: Vec<BlockPropertyDefinition>) -> BlockDefinition {
        BlockDefinition {
            properties,
            ..block(name, &Block::STONE)
        }
    }

    fn state_ids(block: &Block) -> Vec<u16> {
        block.states.iter().map(|state| state.id.as_u16()).collect()
    }

    fn props_at(
        tables: &ContentTables,
        block: usize,
        index: u16,
    ) -> Vec<(&'static str, &'static str)> {
        DynamicProperties {
            properties: tables.block_properties()[block],
            index,
        }
        .to_props()
    }

    #[test]
    fn block_without_properties_has_one_state() {
        let mut builder = ContentBuilder::new();
        builder
            .register_block(block("mymod:plain", &Block::STONE))
            .unwrap();
        let tables = builder.build().unwrap();

        let plain = &tables.blocks()[0];
        assert_eq!(state_ids(plain), vec![BlockStateId::STATE_COUNT]);
        assert_eq!(plain.default_state.id.as_u16(), BlockStateId::STATE_COUNT);
        assert!(tables.block_properties()[0].is_empty());
        assert!(props_at(&tables, 0, 0).is_empty());
    }

    #[test]
    fn boolean_property_lists_true_before_false() {
        let mut builder = ContentBuilder::new();
        builder
            .register_block(block_with(
                "mymod:lamp",
                vec![BlockPropertyDefinition::bool("lit", false)],
            ))
            .unwrap();
        let tables = builder.build().unwrap();

        let base = BlockStateId::STATE_COUNT;
        let lamp = &tables.blocks()[0];
        assert_eq!(state_ids(lamp), vec![base, base + 1]);
        assert_eq!(props_at(&tables, 0, 0), vec![("lit", "true")]);
        assert_eq!(props_at(&tables, 0, 1), vec![("lit", "false")]);
        assert_eq!(lamp.default_state.id.as_u16(), base + 1);
    }

    /// `age` sorts before `powered`, so `powered` varies fastest:
    /// id = first + age_index * 2 + powered_index, with `true` at index 0.
    #[test]
    fn mixed_properties_follow_the_generated_layout() {
        let mut builder = ContentBuilder::new();
        builder
            .register_block(block_with(
                "mymod:b_crop",
                vec![
                    BlockPropertyDefinition::bool("powered", false),
                    BlockPropertyDefinition::int("age", 0, 3, 1),
                ],
            ))
            .unwrap();
        builder
            .register_block(block("mymod:a_plain", &Block::DIRT))
            .unwrap();
        builder
            .register_block(block_with(
                "mymod:c_lamp",
                vec![BlockPropertyDefinition::bool("lit", true)],
            ))
            .unwrap();
        let tables = builder.build().unwrap();

        let base = BlockStateId::STATE_COUNT;
        let [plain, crop, lamp] = tables.blocks() else {
            panic!("expected three blocks");
        };
        assert_eq!(state_ids(plain), vec![base]);
        let first = base + 1;
        assert_eq!(state_ids(crop), (first..first + 8).collect::<Vec<_>>());
        assert_eq!(state_ids(lamp), vec![first + 8, first + 9]);
        assert_eq!(tables.states.len(), 11);

        let names: Vec<&str> = tables.block_properties()[1]
            .iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, vec!["age", "powered"]);
        for age in 0..4u16 {
            for (powered_index, powered) in ["true", "false"].into_iter().enumerate() {
                let index = age * 2 + powered_index as u16;
                let age = age.to_string();
                assert_eq!(
                    props_at(&tables, 1, index),
                    vec![("age", age.as_str()), ("powered", powered)]
                );
                assert_eq!(tables.state_blocks[usize::from(1 + index)], crop.id);
            }
        }
        // age=1, powered=false
        assert_eq!(crop.default_state.id.as_u16(), first + 3);
        assert_eq!(lamp.default_state.id.as_u16(), first + 8);
    }

    /// The custom layout must give each index the same values as the generated block.
    fn assert_layout_matches(vanilla: &Block, properties: Vec<BlockPropertyDefinition>) {
        let mut builder = ContentBuilder::new();
        builder
            .register_block(block_with("mymod:copy", properties))
            .unwrap();
        let tables = builder.build().unwrap();

        assert_eq!(tables.blocks()[0].states.len(), vanilla.states.len());
        for (index, state) in vanilla.states.iter().enumerate() {
            let expected = vanilla.properties(state.id).unwrap().to_props();
            assert_eq!(props_at(&tables, 0, index as u16), expected);
        }
        let default = vanilla
            .states
            .iter()
            .position(|state| state.id == vanilla.default_state.id)
            .unwrap();
        assert_eq!(
            tables.blocks()[0].default_state.id.as_u16(),
            BlockStateId::STATE_COUNT + default as u16
        );
    }

    /// The daylight detector has `inverted` (boolean) and `power` (0..=15).
    #[test]
    fn layout_matches_a_generated_block() {
        assert_layout_matches(
            &Block::DAYLIGHT_DETECTOR,
            vec![
                BlockPropertyDefinition::int("power", 0, 15, 0),
                BlockPropertyDefinition::bool("inverted", false),
            ],
        );
    }

    /// The lever has two enums whose declaration order is not alphabetical, so sorting enum
    /// values would fail this test.
    #[test]
    fn enum_layout_matches_a_generated_block() {
        assert_layout_matches(
            &Block::LEVER,
            vec![
                BlockPropertyDefinition::bool("powered", false),
                BlockPropertyDefinition::enumeration(
                    "facing",
                    &["north", "south", "west", "east"],
                    "north",
                ),
                BlockPropertyDefinition::enumeration("face", &["floor", "wall", "ceiling"], "wall"),
            ],
        );
    }

    #[test]
    fn rejects_invalid_properties() {
        let cases = [
            (
                vec![
                    BlockPropertyDefinition::bool("lit", true),
                    BlockPropertyDefinition::int("lit", 0, 1, 0),
                ],
                "the name is used twice",
            ),
            (
                vec![BlockPropertyDefinition::bool("Lit", true)],
                "the name must use a-z 0-9 and _",
            ),
            (
                vec![BlockPropertyDefinition::int("age", 3, 1, 1)],
                "it needs at least two values",
            ),
            (
                vec![BlockPropertyDefinition::int("age", 2, 2, 2)],
                "it needs at least two values",
            ),
            (
                vec![BlockPropertyDefinition::enumeration(
                    "mode",
                    &["a", "a"],
                    "a",
                )],
                "a value is listed twice",
            ),
            (
                vec![BlockPropertyDefinition::enumeration(
                    "mode",
                    &["a", "B"],
                    "a",
                )],
                "the values must use a-z 0-9 and _",
            ),
            (
                vec![BlockPropertyDefinition::int("age", 0, 3, 7)],
                "the default is not one of the values",
            ),
        ];
        let mut builder = ContentBuilder::new();
        for (properties, reason) in cases {
            let property = properties[0].name.clone();
            assert_eq!(
                builder.register_block(block_with("mymod:bad", properties)),
                Err(RegistryError::InvalidProperty {
                    block: "mymod:bad".to_string(),
                    property,
                    reason
                })
            );
        }
        assert!(builder.blocks.is_empty());
    }

    #[test]
    fn rejects_more_states_than_free_state_ids() {
        let mut builder = ContentBuilder::new();
        // 16^4 = 65,536 states, above the free ids.
        let properties = ["a", "b", "c", "d"]
            .into_iter()
            .map(|name| BlockPropertyDefinition::int(name, 0, 15, 0))
            .collect();
        builder
            .register_block(block_with("mymod:huge", properties))
            .unwrap();
        assert_eq!(
            builder.build().unwrap_err(),
            RegistryError::RangeExceeded {
                registry: "block state",
                count: 65_536
            }
        );
    }

    fn tags(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn custom_tag_table_joins_display_and_explicit_tags() {
        let mut builder = ContentBuilder::new();
        builder
            .register_block(BlockDefinition {
                tags: tags(&["mineable/pickaxe"]),
                ..block("test:mud_lamp", &Block::DIRT)
            })
            .unwrap();
        builder
            .register_block(block("test:stone_lamp", &Block::STONE))
            .unwrap();
        builder
            .register_item(ItemDefinition {
                tags: tags(&["#minecraft:piglin_loved"]),
                ..item("test:blade", None)
            })
            .unwrap();
        builder
            .register_entity_type(EntityTypeDefinition {
                tags: tags(&["minecraft:skeletons"]),
                ..entity_type("test:golem")
            })
            .unwrap();
        let tables = builder.build().unwrap();

        let (mud, stone) = (BlockId::COUNT, BlockId::COUNT + 1);
        let block_tag = |tag: &str| tables.block_tags.custom.get(tag).map(|ids| ids.to_vec());
        assert_eq!(block_tag("minecraft:dirt"), Some(vec![mud]));
        assert_eq!(
            block_tag("minecraft:base_stone_overworld"),
            Some(vec![stone])
        );
        assert_eq!(
            block_tag("minecraft:mineable/pickaxe"),
            Some(vec![mud, stone])
        );
        assert_eq!(block_tag("minecraft:logs"), None);

        let item_tag = |tag: &str| tables.item_tags.custom.get(tag).map(|ids| ids.to_vec());
        assert_eq!(item_tag("minecraft:swords"), Some(vec![Item::COUNT]));
        assert_eq!(item_tag("minecraft:piglin_loved"), Some(vec![Item::COUNT]));

        let entity_tag = |tag: &str| {
            tables
                .entity_type_tags
                .custom
                .get(tag)
                .map(|ids| ids.to_vec())
        };
        assert_eq!(
            entity_tag("minecraft:zombies"),
            Some(vec![EntityType::COUNT])
        );
        assert_eq!(
            entity_tag("minecraft:skeletons"),
            Some(vec![EntityType::COUNT])
        );
        assert_eq!(entity_tag("minecraft:raiders"), None);
    }

    #[test]
    fn rejects_unknown_tags() {
        let mut builder = ContentBuilder::new();
        // A block tag is not an item tag.
        assert_eq!(
            builder.register_item(ItemDefinition {
                tags: tags(&["minecraft:base_stone_overworld"]),
                ..item("test:blade", None)
            }),
            Err(RegistryError::UnknownTag {
                kind: ContentKind::Item,
                name: "test:blade".to_string(),
                tag: "minecraft:base_stone_overworld".to_string(),
            })
        );
        assert!(!builder.contains(ContentKind::Item, "test:blade"));
        assert!(matches!(
            builder.register_block(BlockDefinition {
                tags: tags(&["mineable/pickaxe", "minecraft:missing"]),
                ..block("test:lamp", &Block::STONE)
            }),
            Err(RegistryError::UnknownTag { .. })
        ));
        assert!(!builder.contains(ContentKind::Block, "test:lamp"));
        assert!(matches!(
            builder.register_entity_type(EntityTypeDefinition {
                tags: tags(&["minecraft:swords"]),
                ..entity_type("test:golem")
            }),
            Err(RegistryError::UnknownTag { .. })
        ));
        assert!(builder.tags.is_empty());
        builder
            .register_item(item("test:blade", None))
            .expect("the name stays free after a failed registration");
    }

    fn tag(kind: ContentKind, name: &str, values: &[&str]) -> TagDefinition {
        TagDefinition {
            kind,
            name: name.to_string(),
            values: tags(values),
        }
    }

    fn ids(ids: Option<&Box<[u16]>>) -> Option<Vec<u16>> {
        ids.map(|ids| ids.to_vec())
    }

    #[test]
    fn mod_tags_hold_their_members_and_the_entries_that_list_them() {
        let mut builder = ContentBuilder::new();
        // The block lists the tag before the tag is registered.
        builder
            .register_block(BlockDefinition {
                tags: tags(&["#test:lamps", "mineable/pickaxe"]),
                ..block("test:lamp_a", &Block::REDSTONE_LAMP)
            })
            .unwrap();
        builder
            .register_tag(tag(
                ContentKind::Block,
                "test:lamps",
                &["redstone_lamp", "minecraft:stone", "test:lamp_b"],
            ))
            .unwrap();
        builder
            .register_block(block("test:lamp_b", &Block::STONE))
            .unwrap();
        // A display entry in a mod tag does not put the custom entry in it.
        builder
            .register_block(block("test:lamp_c", &Block::REDSTONE_LAMP))
            .unwrap();
        builder
            .register_tag(tag(ContentKind::Item, "test:lamps", &["test:wand"]))
            .unwrap();
        builder.register_item(item("test:wand", None)).unwrap();
        builder
            .register_tag(tag(ContentKind::EntityType, "test:empty", &[]))
            .unwrap();
        let tables = builder.build().unwrap();

        let (a, b) = (BlockId::COUNT, BlockId::COUNT + 1);
        assert_eq!(
            ids(tables.block_tags.custom.get("test:lamps")),
            Some(vec![a, b])
        );
        let mut generated = vec![Block::REDSTONE_LAMP.id.as_u16(), Block::STONE.id.as_u16()];
        generated.sort_unstable();
        assert_eq!(
            ids(tables.block_tags.mod_tags.get("test:lamps")),
            Some(generated)
        );
        assert_eq!(
            ids(tables.block_tags.custom.get("minecraft:mineable/pickaxe")),
            Some(vec![a, b])
        );
        assert_eq!(
            ids(tables.item_tags.custom.get("test:lamps")),
            Some(vec![Item::COUNT])
        );
        assert_eq!(
            ids(tables.item_tags.mod_tags.get("test:lamps")),
            Some(vec![])
        );
        assert_eq!(
            ids(tables.entity_type_tags.mod_tags.get("test:empty")),
            Some(vec![])
        );
        assert!(!tables.entity_type_tags.custom.contains_key("test:empty"));

        fn names(set: &BTreeSet<String>) -> Vec<&str> {
            set.iter().map(String::as_str).collect()
        }
        assert_eq!(
            names(tables.explicit_tags(ContentKind::Block, "test:lamp_a")),
            ["minecraft:mineable/pickaxe", "test:lamps"]
        );
        assert!(
            tables
                .explicit_tags(ContentKind::Block, "test:lamp_b")
                .is_empty()
        );
        assert_eq!(
            tables
                .mod_tags(ContentKind::Block)
                .get("test:lamps")
                .map(names),
            Some(vec![
                "minecraft:redstone_lamp",
                "minecraft:stone",
                "test:lamp_b"
            ])
        );
        assert!(
            tables
                .mod_tags(ContentKind::EntityType)
                .contains_key("test:empty")
        );
    }

    #[test]
    fn rejects_invalid_duplicate_and_unknown_mod_tags() {
        let mut builder = ContentBuilder::new();
        for name in ["minecraft:ores", "ores", ":ores", "Test:Ores", "test:"] {
            assert_eq!(
                builder.register_tag(tag(ContentKind::Block, name, &[])),
                Err(RegistryError::InvalidTagName {
                    kind: ContentKind::Block,
                    tag: name.to_string(),
                })
            );
        }
        assert_eq!(
            builder.register_tag(tag(
                ContentKind::Block,
                "test:ores",
                &["minecraft:no_such_block"]
            )),
            Err(RegistryError::UnknownTagMember {
                kind: ContentKind::Block,
                tag: "test:ores".to_string(),
                member: "minecraft:no_such_block".to_string(),
            })
        );
        // A value names an entry, not a tag, and an item is not a block.
        for value in ["#minecraft:logs", "minecraft:diamond_sword"] {
            assert!(matches!(
                builder.register_tag(tag(ContentKind::Block, "test:ores", &[value])),
                Err(RegistryError::UnknownTagMember { .. })
            ));
        }
        builder
            .register_tag(tag(ContentKind::Block, "test:ores", &["iron_ore"]))
            .expect("the name stays free after a failed registration");
        assert_eq!(
            builder.register_tag(tag(ContentKind::Block, "test:ores", &[])),
            Err(RegistryError::DuplicateTag {
                kind: ContentKind::Block,
                tag: "test:ores".to_string(),
            })
        );
        // A generated tag counts as registered.
        assert_eq!(
            builder.register_tag(tag(ContentKind::Block, "c:ores", &["stone"])),
            Err(RegistryError::DuplicateTag {
                kind: ContentKind::Block,
                tag: "c:ores".to_string(),
            })
        );
        builder
            .register_tag(tag(ContentKind::Item, "test:ores", &["iron_ingot"]))
            .expect("each registry has its own tag names");
    }

    #[test]
    fn unknown_mod_tags_and_members_fail_the_freeze() {
        let mut builder = ContentBuilder::new();
        builder
            .register_block(BlockDefinition {
                tags: tags(&["test:missing"]),
                ..block("test:lamp", &Block::STONE)
            })
            .unwrap();
        assert_eq!(
            builder.build().unwrap_err(),
            RegistryError::UnknownTag {
                kind: ContentKind::Block,
                name: "test:lamp".to_string(),
                tag: "test:missing".to_string(),
            }
        );

        let mut builder = ContentBuilder::new();
        builder
            .register_tag(tag(ContentKind::Item, "test:tools", &["test:gone"]))
            .unwrap();
        // A block of that name is not an item.
        builder
            .register_block(block("test:gone", &Block::STONE))
            .unwrap();
        assert_eq!(
            builder.build().unwrap_err(),
            RegistryError::UnknownTagMember {
                kind: ContentKind::Item,
                tag: "test:tools".to_string(),
                member: "test:gone".to_string(),
            }
        );
    }

    fn snapshot_names(snapshot: &[(i32, Identifier)]) -> Vec<String> {
        for (index, (id, _)) in snapshot.iter().enumerate() {
            assert_eq!(*id, index as i32, "the snapshot has a gap");
        }
        snapshot.iter().map(|(_, name)| name.to_string()).collect()
    }

    #[test]
    fn snapshots_without_custom_content_hold_the_generated_entries() {
        let tables = ContentBuilder::new().build().unwrap();
        // The lengths and the first and last names match the registry snapshots of a NeoForge
        // 26.3.0.64-beta server: tools/pumpkin-neoforge-client/captures/neoforge-26.3.0.64-beta/.
        let cases = [
            (
                tables.block_snapshot(),
                1286,
                BlockId::COUNT,
                "minecraft:air",
                "minecraft:firefly_bush",
            ),
            (
                tables.item_snapshot(),
                1658,
                Item::COUNT,
                "minecraft:air",
                "minecraft:ominous_bottle",
            ),
            (
                tables.entity_type_snapshot(),
                161,
                EntityType::COUNT,
                "minecraft:acacia_boat",
                "minecraft:fishing_bobber",
            ),
        ];
        for (snapshot, len, count, first, last) in cases {
            let names = snapshot_names(&snapshot);
            assert_eq!(names.len(), len);
            assert_eq!(names.len(), usize::from(count));
            assert_eq!(names.first().map(String::as_str), Some(first));
            assert_eq!(names.last().map(String::as_str), Some(last));
            assert!(snapshot.iter().all(|(_, name)| name.is_vanilla()));
        }
    }

    #[test]
    fn block_snapshot_appends_custom_blocks_in_name_order() {
        let mut builder = ContentBuilder::new();
        builder
            .register_block(block("test:beta", &Block::STONE))
            .unwrap();
        builder
            .register_block(block("test:alpha", &Block::DIRT))
            .unwrap();
        let names = snapshot_names(&builder.build().unwrap().block_snapshot());

        let count = usize::from(BlockId::COUNT);
        assert_eq!(count, 1286);
        assert_eq!(names.len(), count + 2);
        assert_eq!(names[0], "minecraft:air");
        assert_eq!(names[count - 1], "minecraft:firefly_bush");
        assert_eq!(names[count], "test:alpha");
        assert_eq!(names[count + 1], "test:beta");
    }

    #[test]
    fn item_snapshot_skips_placeholders() {
        let mut builder = ContentBuilder::new();
        builder.register_item(item("test:gem", None)).unwrap();
        builder.register_item(item("gone:widget", None)).unwrap();
        builder.mark_placeholder(ContentKind::Item, "gone:widget".to_string());
        let tables = builder.build().unwrap();
        let snapshot = tables.item_snapshot();

        // The placeholder sorts first and keeps its id, which is a gap in the snapshot.
        let count = usize::from(Item::COUNT);
        assert_eq!(snapshot.len(), count + 1);
        assert_eq!(snapshot[count - 1].0, i32::from(Item::COUNT) - 1);
        assert_eq!(
            snapshot[count],
            (
                i32::from(Item::COUNT) + 1,
                Identifier::from_static("test", "gem")
            )
        );
        assert!(snapshot.iter().all(|(id, _)| *id != i32::from(Item::COUNT)));
        assert_eq!(
            (tables.items()[0].id, tables.items()[0].registry_key),
            (Item::COUNT, "gone:widget")
        );
    }

    /// The only test that installs the global tables: they are process-wide and install once.
    #[test]
    fn frozen_registry_falls_through_and_rejects_registration() {
        register_block(BlockDefinition {
            tags: tags(&["minecraft:mineable/pickaxe", "test:lamps"]),
            ..block("test:zeta", &Block::DIRT)
        })
        .unwrap();
        register_tag(tag(
            ContentKind::Block,
            "test:lamps",
            &["minecraft:redstone_lamp", "test:alpha"],
        ))
        .unwrap();
        assert!(is_tag_registered(ContentKind::Block, "test:lamps"));
        assert!(!is_tag_registered(ContentKind::Item, "test:lamps"));
        register_block(block("test:alpha", &Block::STONE)).unwrap();
        register_item(ItemDefinition {
            tags: tags(&["minecraft:piglin_loved"]),
            ..item("test:alpha", Some("test:alpha"))
        })
        .unwrap();
        register_entity_type(EntityTypeDefinition {
            tags: tags(&["minecraft:skeletons"]),
            ..entity_type("test:golem")
        })
        .unwrap();

        assert!(tables().is_none());
        let frozen = freeze().unwrap();
        assert!(std::ptr::eq(tables().unwrap(), frozen));
        assert!(std::ptr::eq(tables().unwrap(), frozen));
        let tables = frozen;
        assert_eq!(tables.blocks().len(), 2);

        // Blocks and states.
        let alpha = Block::from_registry_key("test:alpha").unwrap();
        assert_eq!(alpha.id.as_u16(), BlockId::COUNT);
        assert_eq!(Block::from_name("test:alpha"), Some(alpha));
        assert_eq!(
            Block::from_name("test:zeta").unwrap().id.as_u16(),
            BlockId::COUNT + 1
        );
        assert_eq!(Block::from_name("minecraft:stone"), Some(&Block::STONE));
        assert_eq!(Block::from_registry_key("test:missing"), None);

        let block_id = BlockId::new(BlockId::COUNT).unwrap();
        assert_eq!(Block::from_id(block_id), alpha);
        assert_eq!(block_id.to_block(), alpha);
        assert_eq!(BlockId::new(BlockId::COUNT + 2), None);
        assert_eq!(BlockId::new_or_air(BlockId::COUNT + 2), BlockId::AIR);

        let state_id = BlockStateId::new(BlockStateId::COUNT).unwrap();
        assert_eq!(state_id, alpha.default_state.id);
        assert_eq!(BlockStateId::new(BlockStateId::COUNT + 2), None);
        assert_eq!(
            BlockStateId::new_or_air(BlockStateId::COUNT + 2),
            BlockStateId::AIR
        );
        assert_eq!(BlockState::from_id(state_id).id, state_id);
        assert_eq!(
            state_id.to_state().hardness,
            Block::STONE.default_state.hardness
        );
        assert_eq!(Block::from_state_id(state_id), alpha);
        assert_eq!(BlockId::from_state_id(state_id), block_id);
        assert_eq!(BlockState::from_id_with_block(state_id).0, alpha);
        assert_eq!(
            BlockState::to_be_network_id(state_id),
            BlockState::to_be_network_id(Block::STONE.default_state.id)
        );
        assert!(alpha.properties(state_id).is_none());
        assert_eq!(
            alpha.to_java_network_id(ContentIds::Display),
            Block::STONE.id.as_u16()
        );
        assert_eq!(alpha.to_java_network_id(ContentIds::Real), BlockId::COUNT);
        assert_eq!(
            Block::from_name("test:zeta")
                .unwrap()
                .to_java_network_id(ContentIds::Display),
            Block::DIRT.id.as_u16()
        );
        for ids in [ContentIds::Display, ContentIds::Real] {
            assert_eq!(
                Block::STONE.to_java_network_id(ids),
                Block::STONE.id.as_u16()
            );
        }
        assert_eq!(namespaced_name("stone"), "minecraft:stone");
        assert_eq!(namespaced_name("test:alpha"), "test:alpha");

        // Items and the block they place.
        let alpha_item = Item::from_registry_key("test:alpha").unwrap();
        assert_eq!(alpha_item.id, Item::COUNT);
        assert_eq!(Item::from_id(Item::COUNT), Some(alpha_item));
        assert_eq!(Item::from_id(Item::COUNT + 1), None);
        assert_eq!(
            Item::from_registry_key("minecraft:stone"),
            Some(&Item::STONE)
        );
        assert_eq!(alpha.item_id, alpha_item.id);
        assert_eq!(Block::from_item_id(alpha_item.id), Some(alpha));
        assert_eq!(Block::from_item_id(Item::DIAMOND.id), None);

        // Entity types.
        let golem = EntityType::from_name("test:golem").unwrap();
        assert_eq!(golem.id, EntityType::COUNT);
        assert_eq!(EntityType::from_raw(EntityType::COUNT), Some(golem));
        assert_eq!(EntityType::from_raw(EntityType::COUNT + 1), None);
        assert_eq!(
            EntityType::from_name("minecraft:zombie"),
            Some(&EntityType::ZOMBIE)
        );
        assert_eq!(golem.display_type(), &EntityType::ZOMBIE);
        assert_eq!(
            golem.to_java_network_id(ContentIds::Display),
            EntityType::ZOMBIE.id
        );
        assert_eq!(
            golem.to_java_network_id(ContentIds::Real),
            EntityType::COUNT
        );
        for ids in [ContentIds::Display, ContentIds::Real] {
            assert_eq!(EntityType::PIG.to_java_network_id(ids), EntityType::PIG.id);
        }

        // Generated tag lists hold generated ids only.
        assert!(!alpha.has_tag(&crate::tag::Block::MINECRAFT_BASE_STONE_OVERWORLD));
        assert_eq!(
            alpha.is_tagged_with("minecraft:base_stone_overworld"),
            Some(false)
        );
        // The custom tag table answers for custom ids: tags of the display entry plus explicit tags.
        assert!(alpha.has_tag_dynamic("minecraft:base_stone_overworld"));
        let zeta = Block::from_name("test:zeta").unwrap();
        assert!(zeta.has_tag_dynamic("#mineable/pickaxe"));
        assert!(zeta.has_tag_dynamic("dirt"));
        assert!(!zeta.has_tag_dynamic("minecraft:base_stone_overworld"));
        assert!(!zeta.has_tag_dynamic("test:missing"));
        assert!(Block::STONE.has_tag_dynamic("base_stone_overworld"));
        assert!(!Block::DIRT.has_tag_dynamic("base_stone_overworld"));
        let (generated, custom) =
            tag_ids(RegistryKey::Block, "minecraft:mineable/pickaxe").unwrap();
        assert!(generated.contains(&Block::STONE.id.as_u16()));
        assert_eq!(custom, [alpha.id.as_u16(), zeta.id.as_u16()]);
        assert_eq!(tag_ids(RegistryKey::Block, "test:missing"), None);
        assert!(alpha_item.has_tag_dynamic("minecraft:swords"));
        assert!(alpha_item.has_tag_dynamic("piglin_loved"));
        assert!(!Item::DIAMOND.has_tag_dynamic("minecraft:swords"));
        assert_eq!(alpha_item.is_tagged_with("minecraft:swords"), Some(false));
        assert!(golem.has_tag_dynamic("minecraft:zombies"));
        assert!(golem.has_tag_dynamic("minecraft:skeletons"));
        assert!(!golem.has_tag_dynamic("minecraft:raiders"));

        // A mod tag answers for its generated and custom members, off the generated lists.
        assert!(alpha.has_tag_dynamic("test:lamps"));
        assert!(zeta.has_tag_dynamic("#test:lamps"));
        assert!(Block::REDSTONE_LAMP.has_tag_dynamic("test:lamps"));
        assert!(!Block::STONE.has_tag_dynamic("test:lamps"));
        assert_eq!(Block::REDSTONE_LAMP.is_tagged_with("test:lamps"), None);
        assert_eq!(
            tag_ids(RegistryKey::Block, "#test:lamps"),
            Some((
                &[Block::REDSTONE_LAMP.id.as_u16()][..],
                &[alpha.id.as_u16(), zeta.id.as_u16()][..]
            ))
        );
        assert!(!alpha_item.has_tag_dynamic("test:lamps"));
        assert_eq!(tag_ids(RegistryKey::Item, "test:lamps"), None);

        // Frozen.
        assert_eq!(
            register_block(block("test:late", &Block::STONE)),
            Err(RegistryError::RegistryFrozen)
        );
        assert_eq!(
            register_item(item("test:late", None)),
            Err(RegistryError::RegistryFrozen)
        );
        assert_eq!(
            register_entity_type(entity_type("test:late")),
            Err(RegistryError::RegistryFrozen)
        );
        assert_eq!(
            register_tag(tag(ContentKind::Block, "test:late", &[])),
            Err(RegistryError::RegistryFrozen)
        );
        assert!(!is_tag_registered(ContentKind::Block, "test:lamps"));
        assert_eq!(freeze().unwrap_err(), RegistryError::RegistryFrozen);
        assert_eq!(Block::from_name("test:late"), None);
    }
}
