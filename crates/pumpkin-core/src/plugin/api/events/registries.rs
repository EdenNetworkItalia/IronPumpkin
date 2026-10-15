//! The registration event of `NeoForge`.

use std::{
    fmt,
    sync::{Mutex, PoisonError},
};

use pumpkin_data::dynamic::{ContentKind, RegistryError};
use pumpkin_macros::Event;

/// Fires once for each registry that mods can fill.
///
/// It fires after the `init` of every native mod and before the common setup, while the content
/// registry still accepts entries. A handler
/// registers content into the registry of the event with [`RegisterEvent::register`], and native
/// mods use the builder methods of `ironpumpkin_mods::content::RegisterEventExt`. The registry
/// freezes after the lifecycle steps, before the first world loads.
///
/// `NeoForge`: `net.neoforged.neoforge.registries.RegisterEvent`, fired on the mod bus by
/// `GameData.postRegisterEvents` once per registry, in the order of
/// `GameData.getRegistrationOrder`. `IronPumpkin` backs `minecraft:block`, `minecraft:entity_type`
/// and `minecraft:item`, and fires the event for them in that order. The event has no cancel.
#[derive(Event)]
pub struct RegisterEvent {
    registry: ContentKind,
    error: Mutex<Option<RegisterEventError>>,
}

/// The first registration of a [`RegisterEvent`] handler that failed. The server logs it and
/// stops after the last fire, as `GameData.postRegisterEvents` throws the collected errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterEventError {
    /// The registry of the event.
    pub registry: ContentKind,
    /// The name of the entry.
    pub name: String,
    /// What the registry returned.
    pub error: RegistryError,
}

impl fmt::Display for RegisterEventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "a RegisterEvent handler cannot register the {} \"{}\": {}",
            self.registry, self.name, self.error
        )
    }
}

impl std::error::Error for RegisterEventError {}

impl RegisterEvent {
    #[must_use]
    pub const fn new(registry: ContentKind) -> Self {
        Self {
            registry,
            error: Mutex::new(None),
        }
    }

    /// The registry of this fire.
    ///
    /// `NeoForge`: `RegisterEvent#getRegistryKey`.
    #[must_use]
    pub const fn registry(&self) -> ContentKind {
        self.registry
    }

    /// The key of the registry of this fire: `minecraft:block`, `minecraft:entity_type` or
    /// `minecraft:item`.
    ///
    /// `NeoForge`: `RegisterEvent#getRegistryKey().identifier()`.
    #[must_use]
    pub const fn registry_key(&self) -> &'static str {
        match self.registry {
            ContentKind::Block => "minecraft:block",
            ContentKind::Item => "minecraft:item",
            ContentKind::EntityType => "minecraft:entity_type",
        }
    }

    /// Runs `register` when `registry` is the registry of this fire, and else does nothing and
    /// returns `Ok`. `name` is the name of the entry. The event keeps the first error.
    ///
    /// `NeoForge`: `RegisterEvent#register(ResourceKey, Identifier, Supplier)`.
    ///
    /// # Errors
    ///
    /// Returns the error of `register`.
    pub fn register(
        &self,
        registry: ContentKind,
        name: &str,
        register: impl FnOnce() -> Result<(), RegistryError>,
    ) -> Result<(), RegistryError> {
        if registry != self.registry {
            return Ok(());
        }
        let result = register();
        if let Err(error) = &result {
            self.error
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get_or_insert_with(|| RegisterEventError {
                    registry,
                    name: name.to_owned(),
                    error: error.clone(),
                });
        }
        result
    }

    /// Takes the first registration error of the handlers. The host calls it after the fire.
    pub fn take_error(&mut self) -> Option<RegisterEventError> {
        self.error
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }
}
