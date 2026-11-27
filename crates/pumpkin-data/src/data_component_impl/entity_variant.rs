use crate::data_component_impl::DataComponentImpl;
use pumpkin_nbt::tag::NbtTag;
use std::borrow::Cow;

/// A variant saved as a string. A registry holder hashes its namespaced id, a
/// `StringRepresentable` enum hashes its name.
macro_rules! string_variant {
    ($name:ident, $struct_name:ident) => {
        string_variant!($name, $struct_name, |value: &str| {
            crate::data_component_impl::hash_ops::string(value)
        });
    };
    ($name:ident, $struct_name:ident, holder) => {
        string_variant!($name, $struct_name, |value: &str| {
            crate::data_component_impl::hash_ops::id(value)
        });
    };
    ($name:ident, $struct_name:ident, $hash:expr) => {
        #[derive(Clone, Debug, Hash, PartialEq, Eq)]
        pub struct $struct_name {
            pub value: Cow<'static, str>,
        }
        impl $struct_name {
            pub fn read_data(data: &NbtTag) -> Option<Self> {
                data.extract_string().map(|v| Self {
                    value: Cow::Owned(v.to_string()),
                })
            }
        }
        impl DataComponentImpl for $struct_name {
            fn write_data(&self) -> NbtTag {
                NbtTag::String(self.value.clone().into_owned().into())
            }
            fn get_hash(&self) -> i32 {
                ($hash)(self.value.as_ref()) as i32
            }
            default_impl!($name);
        }
    };
}

string_variant!(VillagerVariant, VillagerVariantImpl, holder);
string_variant!(WolfVariant, WolfVariantImpl, holder);
string_variant!(WolfSoundVariant, WolfSoundVariantImpl, holder);
string_variant!(WolfCollar, WolfCollarImpl);
string_variant!(FoxVariant, FoxVariantImpl);
string_variant!(SalmonSize, SalmonSizeImpl);
string_variant!(ParrotVariant, ParrotVariantImpl);
string_variant!(TropicalFishPattern, TropicalFishPatternImpl);
string_variant!(TropicalFishBaseColor, TropicalFishBaseColorImpl);
string_variant!(TropicalFishPatternColor, TropicalFishPatternColorImpl);
string_variant!(MooshroomVariant, MooshroomVariantImpl);
string_variant!(RabbitVariant, RabbitVariantImpl);
string_variant!(PigVariant, PigVariantImpl, holder);
string_variant!(PigSoundVariant, PigSoundVariantImpl, holder);
string_variant!(CowVariant, CowVariantImpl, holder);
string_variant!(CowSoundVariant, CowSoundVariantImpl, holder);
string_variant!(ChickenVariant, ChickenVariantImpl, holder);
string_variant!(ChickenSoundVariant, ChickenSoundVariantImpl, holder);
string_variant!(ZombieNautilusVariant, ZombieNautilusVariantImpl, holder);
string_variant!(FrogVariant, FrogVariantImpl, holder);
string_variant!(HorseVariant, HorseVariantImpl);
string_variant!(PaintingVariant, PaintingVariantImpl, holder);
string_variant!(LlamaVariant, LlamaVariantImpl);
string_variant!(AxolotlVariant, AxolotlVariantImpl);
string_variant!(CatVariant, CatVariantImpl, holder);
string_variant!(CatSoundVariant, CatSoundVariantImpl, holder);
string_variant!(CatCollar, CatCollarImpl);
string_variant!(SheepColor, SheepColorImpl);
string_variant!(ShulkerColor, ShulkerColorImpl);
