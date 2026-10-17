//! Data component types of mods and their values on item stacks.
//!
//! `NeoForge`: `DataComponentType<T>`, registered with
//! `DeferredRegister.DataComponents#registerComponentType` and read with `ItemStack#get`,
//! `ItemStack#set` and `ItemStack#remove`. Here the codec of a type is its value type `T`, which
//! implements the `Encode` and `Decode` traits of `pumpkin_codecs`, the Rust form of a `Codec`.
//!
//! A mod registers a type with a [`DataComponentTypeBuilder`] and reads and writes values through
//! a [`DataComponentType`] handle, which only names the type. An item stack holds a mod value in
//! the NBT form of its codec. A save keeps the value when the type is persistent, under the name
//! of the type in the `components` of the stack, as vanilla saves a component. A client that synced
//! the registries (a `NeoForge` client) gets the value as a component in the NBT form of the
//! codec, as `ByteBufCodecs.fromCodec` writes it. A vanilla client never gets it.
//!
//! [`get_json`], [`set_json`] and [`remove`] read and write the value of a type by name, in the
//! JSON form of its codec.
//!
//! ```
//! use ironpumpkin_mods::{
//!     ModInit, NativeMod,
//!     data_components::{DataComponentType, DataComponentTypeBuilder},
//!     pumpkin_data::{item::Item, item_stack::ItemStack},
//! };
//!
//! static CHARGE: DataComponentType<i32> = DataComponentType::new("examplemod:charge");
//!
//! struct ChargeMod;
//!
//! impl NativeMod for ChargeMod {
//!     fn id(&self) -> &'static str {
//!         "examplemod"
//!     }
//!     fn display_name(&self) -> &'static str {
//!         "Charge"
//!     }
//!     fn version(&self) -> &'static str {
//!         "1.0.0"
//!     }
//!     fn init(&self, cx: &mut ModInit) {
//!         let _ = cx.register_data_component_type(DataComponentTypeBuilder::<i32>::new(
//!             CHARGE.name(),
//!         ));
//!     }
//! }
//!
//! // After the freeze:
//! fn charge(stack: &mut ItemStack) {
//!     let charge = CHARGE.get(stack).ok().flatten().unwrap_or(0);
//!     let _ = CHARGE.set(stack, &(charge + 1));
//! }
//! ```

use std::{
    collections::BTreeMap,
    fmt,
    marker::PhantomData,
    sync::{Mutex, PoisonError},
};

use pumpkin_codecs::{DataResult, Decode, Encode, json_ops::JsonOps};
use pumpkin_core::plugin::api::events::registries::RegisterEvent;
use pumpkin_data::{
    dynamic::{ContentKind, RegistryError},
    dynamic_registries::{self, DataComponentTypeDefinition},
    item_stack::ItemStack,
};
use pumpkin_nbt::{nbt_ops::NbtOps, tag::NbtTag};

use crate::ModInit;

/// A data component type of a mod, with value type `T`.
///
/// `NeoForge`: `DataComponentType.builder()`.
pub struct DataComponentTypeBuilder<T> {
    definition: DataComponentTypeDefinition,
    _value: PhantomData<fn() -> T>,
}

impl<T: Encode + Decode + 'static> DataComponentTypeBuilder<T> {
    /// A persistent type named `name`, `namespace:path` outside `minecraft`.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            definition: DataComponentTypeDefinition {
                name: name.into(),
                persistent: true,
            },
            _value: PhantomData,
        }
    }

    /// Whether item saves keep the value. `NeoForge`: a type built without `persistent(codec)`
    /// is not saved. The default is `true`.
    #[must_use]
    pub const fn persistent(mut self, persistent: bool) -> Self {
        self.definition.persistent = persistent;
        self
    }

    fn register(self) -> Result<(), RegistryError> {
        let name = self.definition.name.clone();
        // The lock is held across both steps, so a codec exists for every registered type.
        let mut codecs = CODECS.lock().unwrap_or_else(PoisonError::into_inner);
        dynamic_registries::register_data_component_type(self.definition)?;
        codecs.insert(
            name,
            Codec {
                from_json: from_json::<T>,
                to_json: to_json::<T>,
            },
        );
        Ok(())
    }
}

impl<T> fmt::Debug for DataComponentTypeBuilder<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DataComponentTypeBuilder")
            .field("definition", &self.definition)
            .field("value", &std::any::type_name::<T>())
            .finish()
    }
}

/// The JSON conversions of a registered type, through its value type.
#[derive(Clone, Copy)]
struct Codec {
    from_json: fn(serde_json::Value) -> Result<NbtTag, String>,
    to_json: fn(NbtTag) -> Result<serde_json::Value, String>,
}

static CODECS: Mutex<BTreeMap<String, Codec>> = Mutex::new(BTreeMap::new());

fn data_result<R>(result: DataResult<R>) -> Result<R, String> {
    match result {
        DataResult::Success { result, .. } => Ok(result),
        DataResult::Error { message, .. } => Err(message),
    }
}

fn from_json<T: Encode + Decode>(json: serde_json::Value) -> Result<NbtTag, String> {
    let value = data_result(T::parse(json, &JsonOps))?;
    data_result(value.encode_start(&NbtOps))
}

fn to_json<T: Encode + Decode>(nbt: NbtTag) -> Result<serde_json::Value, String> {
    let value = data_result(T::parse(nbt, &NbtOps))?;
    data_result(value.encode_start(&JsonOps))
}

/// Why a component value could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentError {
    /// No mod registered a data component type with this name, or the registry is not frozen.
    UnknownType(String),
    /// The codec of the type rejects the value.
    Codec { name: String, message: String },
}

impl fmt::Display for ComponentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownType(name) => write!(f, "unknown data component type \"{name}\""),
            Self::Codec { name, message } => {
                write!(f, "data component \"{name}\": {message}")
            }
        }
    }
}

impl std::error::Error for ComponentError {}

/// A data component type of a mod, by name, with value type `T`. It is valid after the freeze.
///
/// `NeoForge`: the `DeferredHolder<DataComponentType<?>, DataComponentType<T>>` of the type.
pub struct DataComponentType<T> {
    name: &'static str,
    _value: PhantomData<fn() -> T>,
}

impl<T> DataComponentType<T> {
    /// The handle of the type named `name`.
    #[must_use]
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            _value: PhantomData,
        }
    }

    /// The name of the type.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Whether the stack has a value of this type. `NeoForge`: `ItemStack#has`.
    #[must_use]
    pub fn has(&self, stack: &ItemStack) -> bool {
        id_of(self.name).is_ok_and(|id| stack.get_custom_component(id).is_some())
    }

    /// Removes the value of this type from the stack and returns whether it had one.
    /// `NeoForge`: `ItemStack#remove`.
    ///
    /// # Errors
    ///
    /// [`ComponentError::UnknownType`] for an unregistered type.
    pub fn remove(&self, stack: &mut ItemStack) -> Result<bool, ComponentError> {
        remove(stack, self.name)
    }
}

impl<T: Encode + Decode> DataComponentType<T> {
    /// The value of this type on the stack, or `None`. `NeoForge`: `ItemStack#get`.
    ///
    /// # Errors
    ///
    /// [`ComponentError::UnknownType`] for an unregistered type, [`ComponentError::Codec`] when the
    /// stored value does not decode as `T`.
    pub fn get(&self, stack: &ItemStack) -> Result<Option<T>, ComponentError> {
        let id = id_of(self.name)?;
        stack
            .get_custom_component(id)
            .map(|nbt| data_result(T::parse(nbt.clone(), &NbtOps)))
            .transpose()
            .map_err(|message| self.codec_error(message))
    }

    /// Sets the value of this type on the stack. `NeoForge`: `ItemStack#set`.
    ///
    /// # Errors
    ///
    /// [`ComponentError::UnknownType`] for an unregistered type, [`ComponentError::Codec`] when the
    /// value does not encode.
    pub fn set(&self, stack: &mut ItemStack, value: &T) -> Result<(), ComponentError> {
        let id = id_of(self.name)?;
        let nbt = data_result(value.encode_start(&NbtOps)).map_err(|m| self.codec_error(m))?;
        stack.set_custom_component(id, nbt);
        Ok(())
    }

    fn codec_error(&self, message: String) -> ComponentError {
        ComponentError::Codec {
            name: self.name.to_owned(),
            message,
        }
    }
}

impl<T> fmt::Debug for DataComponentType<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("DataComponentType")
            .field(&self.name)
            .finish()
    }
}

fn id_of(name: &str) -> Result<u16, ComponentError> {
    dynamic_registries::data_component_type_by_name(name)
        .map(|entry| entry.id)
        .ok_or_else(|| ComponentError::UnknownType(name.to_owned()))
}

fn codec_of(name: &str) -> Result<Codec, ComponentError> {
    CODECS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(name)
        .copied()
        .ok_or_else(|| ComponentError::UnknownType(name.to_owned()))
}

/// The value of the mod component type `name` on the stack, in the JSON form of its codec, or
/// `None` when the stack has no value of it.
///
/// # Errors
///
/// [`ComponentError::UnknownType`] for an unregistered type, [`ComponentError::Codec`] when the
/// stored value does not decode.
pub fn get_json(
    stack: &ItemStack,
    name: &str,
) -> Result<Option<serde_json::Value>, ComponentError> {
    let id = id_of(name)?;
    let codec = codec_of(name)?;
    stack
        .get_custom_component(id)
        .map(|nbt| (codec.to_json)(nbt.clone()))
        .transpose()
        .map_err(|message| ComponentError::Codec {
            name: name.to_owned(),
            message,
        })
}

/// Sets the value of the mod component type `name` on the stack from the JSON form of its codec.
///
/// # Errors
///
/// [`ComponentError::UnknownType`] for an unregistered type, [`ComponentError::Codec`] when the
/// codec rejects the JSON.
pub fn set_json(
    stack: &mut ItemStack,
    name: &str,
    value: serde_json::Value,
) -> Result<(), ComponentError> {
    let id = id_of(name)?;
    let nbt = (codec_of(name)?.from_json)(value).map_err(|message| ComponentError::Codec {
        name: name.to_owned(),
        message,
    })?;
    stack.set_custom_component(id, nbt);
    Ok(())
}

/// Removes the value of the mod component type `name` from the stack and returns whether it had
/// one.
///
/// # Errors
///
/// [`ComponentError::UnknownType`] for an unregistered type.
pub fn remove(stack: &mut ItemStack, name: &str) -> Result<bool, ComponentError> {
    Ok(stack.remove_custom_component(id_of(name)?).is_some())
}

impl ModInit {
    /// Registers a data component type. It applies at once, like
    /// [`ModInit::register_block`], and gets its id at the freeze.
    ///
    /// # Errors
    ///
    /// Returns the [`RegistryError`] of the registry: the name is invalid, uses the `minecraft`
    /// namespace or is already registered, or the registry is frozen.
    pub fn register_data_component_type<T: Encode + Decode + 'static>(
        &mut self,
        builder: DataComponentTypeBuilder<T>,
    ) -> Result<(), RegistryError> {
        let name = builder.definition.name.clone();
        self.keep_content_error(ContentKind::DataComponentType, name, builder.register())
    }
}

/// Registration of data component types from a handler of [`RegisterEvent`]. See
/// [`RegisterEventExt`](crate::content::RegisterEventExt) for the pattern.
pub trait RegisterDataComponentType {
    /// Registers a data component type on the fire of `minecraft:data_component_type`.
    ///
    /// # Errors
    ///
    /// The [`RegistryError`] of the registry.
    fn register_data_component_type<T: Encode + Decode + 'static>(
        &self,
        builder: DataComponentTypeBuilder<T>,
    ) -> Result<(), RegistryError>;
}

impl RegisterDataComponentType for RegisterEvent {
    fn register_data_component_type<T: Encode + Decode + 'static>(
        &self,
        builder: DataComponentTypeBuilder<T>,
    ) -> Result<(), RegistryError> {
        let name = builder.definition.name.clone();
        self.register(ContentKind::DataComponentType, &name, || builder.register())
    }
}
