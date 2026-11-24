//! Default attributes that native mods set for entity types, the backing of `NeoForge`
//! `EntityAttributeCreationEvent` and `EntityAttributeModificationEvent`.
//!
//! The mods set them by name before the freeze, when a custom type has no id yet. A mod
//! attribute has no id yet either, so a registration names it ([`AttributeRef::Named`]). On the
//! first lookup after the freeze they become a table by entity type id and the names bind. A type
//! without an entry keeps the attributes of its generated data, which a custom type copies from
//! its display type.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use pumpkin_data::attributes::Attributes;
use pumpkin_data::dynamic::{self, RegistryError};
use pumpkin_data::entity::EntityType;
use rustc_hash::FxHashMap;

type AttributeList = &'static [(Attributes, f64)];

/// An attribute in a registration: a generated or `NeoForge` attribute, or a mod attribute by
/// its namespaced name, which binds at the freeze.
#[derive(Debug, Clone)]
pub enum AttributeRef {
    Known(Attributes),
    Named(String),
}

impl AttributeRef {
    /// The attribute of a registration by name, with the `minecraft` namespace when it has none.
    /// A generated or `NeoForge` attribute is [`Known`](Self::Known), any other name is
    /// [`Named`](Self::Named): the caller checks that a mod registered it.
    #[must_use]
    pub fn named(name: &str) -> Self {
        let name = dynamic::namespaced_name(name);
        Attributes::from_name(&name).map_or_else(
            || Self::Named(name.into_owned()),
            |attribute| Self::Known(attribute.clone()),
        )
    }

    /// The name of the attribute, with its namespace.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Known(attribute) => attribute.name,
            Self::Named(name) => name,
        }
    }

    /// The attribute, once the freeze bound its name.
    fn resolve(&self) -> Option<Attributes> {
        match self {
            Self::Known(attribute) => Some(attribute.clone()),
            Self::Named(name) => Attributes::from_name(name).cloned(),
        }
    }
}

impl PartialEq for AttributeRef {
    fn eq(&self, other: &Self) -> bool {
        self.name() == other.name()
    }
}

impl From<Attributes> for AttributeRef {
    fn from(attribute: Attributes) -> Self {
        Self::Known(attribute)
    }
}

impl From<&Attributes> for AttributeRef {
    fn from(attribute: &Attributes) -> Self {
        Self::Known(attribute.clone())
    }
}

impl From<&str> for AttributeRef {
    fn from(name: &str) -> Self {
        Self::named(name)
    }
}

/// Sets the value of the attribute in the list, or appends it.
pub fn set_value(list: &mut Vec<(AttributeRef, f64)>, attribute: AttributeRef, value: f64) {
    match list.iter_mut().find(|(a, _)| *a == attribute) {
        Some(entry) => entry.1 = value,
        None => list.push((attribute, value)),
    }
}

#[derive(Default)]
struct Pending {
    /// The full attribute set of a type, from the creation event.
    created: BTreeMap<String, Vec<(AttributeRef, f64)>>,
    /// Attributes added or replaced by the modification event, in call order.
    added: BTreeMap<String, Vec<(AttributeRef, f64)>>,
}

static PENDING: Mutex<Option<Pending>> = Mutex::new(None);
/// Whether a mod set any attribute, so a spawn on a server without one skips the table.
static ANY: AtomicBool = AtomicBool::new(false);
static TABLE: OnceLock<FxHashMap<u16, AttributeList>> = OnceLock::new();

fn pending<T>(f: impl FnOnce(&mut Pending) -> T) -> Result<T, RegistryError> {
    if dynamic::tables().is_some() {
        return Err(RegistryError::RegistryFrozen);
    }
    let mut pending = PENDING.lock().unwrap_or_else(PoisonError::into_inner);
    ANY.store(true, Ordering::Relaxed);
    Ok(f(pending.get_or_insert_default()))
}

/// Sets the full default attribute set of the entity type `name`. Returns `false`, and changes
/// nothing, when the type already got a set this way.
///
/// # Errors
///
/// [`RegistryError::RegistryFrozen`] after the freeze.
pub fn create(name: &str, attributes: Vec<(AttributeRef, f64)>) -> Result<bool, RegistryError> {
    pending(|pending| {
        if pending.created.contains_key(name) {
            return false;
        }
        pending.created.insert(name.to_owned(), attributes);
        true
    })
}

/// Adds the attribute to the default attributes of the entity type `name`, or replaces its
/// value, as `AttributeSupplier.Builder.combine` does.
///
/// # Errors
///
/// [`RegistryError::RegistryFrozen`] after the freeze.
pub fn add(name: &str, attribute: AttributeRef, value: f64) -> Result<(), RegistryError> {
    pending(|pending| {
        pending
            .added
            .entry(name.to_owned())
            .or_default()
            .push((attribute, value));
    })
}

/// Whether the entity type `name` has the attribute by default: in the set of the creation event,
/// in the generated data of a vanilla type, or among the added attributes.
#[must_use]
pub fn has(name: &str, attribute: &AttributeRef) -> bool {
    let pending = PENDING.lock().unwrap_or_else(PoisonError::into_inner);
    let pending = pending.as_ref();
    let has_in = |list: &[(AttributeRef, f64)]| list.iter().any(|(a, _)| a == attribute);
    pending
        .and_then(|pending| pending.created.get(name))
        .map_or_else(
            || {
                EntityType::from_name(name).is_some_and(|ty| {
                    ty.attributes
                        .iter()
                        .any(|(a, _)| a.name == attribute.name())
                })
            },
            |list| has_in(list),
        )
        || pending
            .and_then(|pending| pending.added.get(name))
            .is_some_and(|list| has_in(list))
}

/// The default attributes of an entity of this type: what the mods set, or the generated data.
#[must_use]
pub fn default_attributes(entity_type: &'static EntityType) -> AttributeList {
    if !ANY.load(Ordering::Relaxed) {
        return entity_type.attributes;
    }
    table()
        .and_then(|table| table.get(&entity_type.id).copied())
        .unwrap_or(entity_type.attributes)
}

fn table() -> Option<&'static FxHashMap<u16, AttributeList>> {
    // A custom type gets its id at the freeze, so the table waits for it.
    dynamic::tables()?;
    Some(TABLE.get_or_init(build_table))
}

#[cold]
#[inline(never)]
fn build_table() -> FxHashMap<u16, AttributeList> {
    let pending = PENDING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
        .unwrap_or_default();
    let mut table = FxHashMap::default();
    let names: Vec<&String> = pending.created.keys().chain(pending.added.keys()).collect();
    for name in names {
        let Some(entity_type) = EntityType::from_name(name) else {
            continue;
        };
        if table.contains_key(&entity_type.id) {
            continue;
        }
        let mut attributes: Vec<(AttributeRef, f64)> =
            pending.created.get(name).cloned().unwrap_or_else(|| {
                entity_type
                    .attributes
                    .iter()
                    .map(|(attribute, value)| (AttributeRef::Known(attribute.clone()), *value))
                    .collect()
            });
        for (attribute, value) in pending.added.get(name).into_iter().flatten() {
            set_value(&mut attributes, attribute.clone(), *value);
        }
        // The freeze bound the names of the mod attributes. A mod registered each one before it
        // named it, so a name that does not bind is a mod attribute that left the registry.
        let attributes: Vec<(Attributes, f64)> = attributes
            .into_iter()
            .filter_map(|(attribute, value)| Some((attribute.resolve()?, value)))
            .collect();
        let attributes: AttributeList = Box::leak(attributes.into_boxed_slice());
        table.insert(entity_type.id, attributes);
    }
    table
}

/// The names of the entity types that got a full attribute set from the creation event.
#[must_use]
pub fn created() -> Vec<String> {
    PENDING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .map(|pending| pending.created.keys().cloned().collect())
        .unwrap_or_default()
}
