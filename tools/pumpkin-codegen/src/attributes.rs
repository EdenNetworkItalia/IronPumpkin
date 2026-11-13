use heck::ToShoutySnakeCase;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use syn::LitInt;

/// Raw deserialization shape for a single attribute entry from `attributes.json`.
#[derive(Deserialize)]
struct Attributes {
    /// Numeric registry ID for this attribute.
    id: u8,
    /// Default numeric value applied to entities that do not override this attribute.
    default_value: f64,
    /// Minimum allowed value (`RangedAttribute.getMinValue`).
    min_value: f64,
    /// Maximum allowed value (`RangedAttribute.getMaxValue`).
    max_value: f64,
    /// Whether this attribute is synchronized to clients (`Attribute.isClientSyncable`).
    client_syncable: bool,
}

/// Generates the `TokenStream` for the `Attributes` struct and its associated constants.
pub fn build() -> TokenStream {
    let attributes: BTreeMap<String, Attributes> =
        serde_json::from_str(&fs::read_to_string("../../assets/attributes.json").unwrap())
            .expect("Failed to parse attributes.json");

    let mut sorted_attributes: Vec<(String, Attributes)> = attributes.into_iter().collect();
    sorted_attributes.sort_by_key(|(_, raw)| raw.id);

    let mut constant_defs = Vec::new();
    let mut constant_idents = Vec::new();
    let count = sorted_attributes.len() as u8;

    for (raw_name, raw_value) in sorted_attributes {
        let constant_ident = format_ident!("{}", raw_name.to_shouty_snake_case());
        constant_idents.push(constant_ident.clone());

        let id_lit = LitInt::new(&raw_value.id.to_string(), Span::call_site());
        let default_value_lit = raw_value.default_value;
        let min_value_lit = raw_value.min_value;
        let max_value_lit = raw_value.max_value;
        let client_syncable_lit = raw_value.client_syncable;
        let name_str = format!("minecraft:{raw_name}");

        constant_defs.push(quote!(
            pub const #constant_ident: Self = Self {
                id: #id_lit,
                default_value: #default_value_lit,
                min_value: #min_value_lit,
                max_value: #max_value_lit,
                client_syncable: #client_syncable_lit,
                name: #name_str,
            };
        ));
    }

    quote! {
        use std::hash::Hash;

        #[derive(Clone, Debug)]
        pub struct Attributes {
            pub id: u8,
            pub default_value: f64,
            pub min_value: f64,
            pub max_value: f64,
            pub client_syncable: bool,
            pub name: &'static str,
        }
        impl PartialEq for Attributes {
            fn eq(&self, other: &Self) -> bool {
                self.id == other.id
            }
        }
        impl Eq for Attributes {}
        impl Hash for Attributes {
            fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
                self.id.hash(state);
            }
        }
        impl Attributes {
            #(#constant_defs)*

            pub const ALL: &'static [Self] = &[
                #(Self::#constant_idents),*
            ];

            /// The number of generated attributes. Other attributes take the ids after it.
            pub const COUNT: u8 = #count;

            /// The attribute with this id: a generated attribute, or one of
            /// [`crate::dynamic_registries`].
            #[must_use]
            pub fn from_id(id: u8) -> Option<&'static Self> {
                Self::ALL
                    .get(usize::from(id))
                    .or_else(|| crate::dynamic_registries::attribute(id))
            }

            /// The attribute with this namespaced name. A name without a namespace is in
            /// `minecraft`.
            #[must_use]
            pub fn from_name(name: &str) -> Option<&'static Self> {
                let namespaced = crate::dynamic::namespaced_name(name);
                Self::ALL
                    .iter()
                    .find(|attribute| attribute.name == namespaced)
                    .or_else(|| crate::dynamic_registries::attribute_by_name(&namespaced))
            }

            #[must_use]
            pub const fn sanitize_value(&self, value: f64) -> f64 {
                if value.is_nan() {
                    self.min_value
                } else {
                    value.clamp(self.min_value, self.max_value)
                }
            }
        }
    }
}
