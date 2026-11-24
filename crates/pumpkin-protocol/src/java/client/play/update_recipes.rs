use std::io::Write;

use crate::{
    ClientPacket, EncodingKey, ServerPacket, VarInt,
    codec::recipe::{OwnedRecipeIngredient, PropertySetSync, StonecutterSync},
    ser::{NetworkReadSliceExt, NetworkWriteExt, ReadingError, WritingError},
};
use pumpkin_data::item::Item;
use pumpkin_data::packet::clientbound::play::UPDATE_RECIPES;
use pumpkin_macros::java_packet;
use pumpkin_util::version::JavaMinecraftVersion;

use super::recipe_book_add::{
    synced_item_ids, write_dynamic_ingredient_holderset, write_empty_slot_display,
    write_item_stack_slot_display,
};

#[java_packet(UPDATE_RECIPES)]
pub struct CUpdateRecipes<'a> {
    pub raw_data: &'a [u8],
}

impl<'a> CUpdateRecipes<'a> {
    #[must_use]
    pub const fn new(raw_data: &'a [u8]) -> Self {
        Self { raw_data }
    }

    /// Encodes the packet body as `ClientboundUpdateRecipesPacket.STREAM_CODEC`: a map from the
    /// property set key to the list of its item ids, then the stonecutter recipes, each one an
    /// ingredient `HolderSet` and the `SlotDisplay` of its result.
    pub fn encode(
        property_sets: &[PropertySetSync<'_>],
        stonecutter: &[StonecutterSync<'_>],
        version: EncodingKey,
    ) -> Result<Vec<u8>, WritingError> {
        let mut write = Vec::new();
        write.write_var_int(&VarInt(property_sets.len() as i32))?;
        for set in property_sets {
            write.write_string(set.key)?;
            let ids = property_set_item_ids(set.ingredients, version);
            write.write_var_int(&VarInt(ids.len() as i32))?;
            for id in ids {
                write.write_var_int(&VarInt(id))?;
            }
        }
        write.write_var_int(&VarInt(stonecutter.len() as i32))?;
        for entry in stonecutter {
            write_dynamic_ingredient_holderset(&mut write, entry.ingredient, version)?;
            match Item::from_registry_key(entry.result_item) {
                Some(item) => {
                    write_item_stack_slot_display(&mut write, item, entry.result_count, version)?;
                }
                None => write_empty_slot_display(&mut write, version)?,
            }
        }
        Ok(write)
    }
}

/// The network ids of the items of `ingredients` that the client matches, without duplicates. Vanilla holds them in a
/// `Set`, so the order is free.
fn property_set_item_ids(ingredients: &[OwnedRecipeIngredient], version: EncodingKey) -> Vec<i32> {
    let items: Vec<&'static Item> = ingredients
        .iter()
        .flat_map(OwnedRecipeIngredient::items)
        .collect();
    synced_item_ids(&items, version)
}

impl ClientPacket for CUpdateRecipes<'_> {
    fn write_packet_data(
        &self,
        mut write: impl Write,
        _version: &EncodingKey,
    ) -> Result<(), WritingError> {
        write.write_slice(self.raw_data)?;
        Ok(())
    }
}

impl<'a> ServerPacket<'a> for CUpdateRecipes<'a> {
    fn read(bytebuf: &mut &'a [u8], _version: &JavaMinecraftVersion) -> Result<Self, ReadingError> {
        let raw_data = bytebuf.read_remaining_slice_borrowed(usize::MAX)?;
        Ok(Self { raw_data })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var_int(value: i32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.write_var_int(&VarInt(value)).unwrap();
        bytes
    }

    fn network_id(item: &Item, version: EncodingKey) -> i32 {
        i32::from(item.to_java_network_id(version.content_ids()))
    }

    /// The bytes that the decompiled stream codecs give for one property set and one stonecutter
    /// recipe: `ByteBufCodecs.map(ResourceKey.streamCodec, RecipePropertySet.STREAM_CODEC)`, then
    /// `ByteBufCodecs.list(SingleInputEntry.noRecipeCodec())` with `ByteBufCodecs.holderSet` for the
    /// ingredient and `SlotDisplay.STREAM_CODEC` for the result.
    #[test]
    fn encodes_as_the_vanilla_stream_codec() {
        let version = EncodingKey::from(JavaMinecraftVersion::V_26_3);
        let furnace = [
            OwnedRecipeIngredient::Simple("minecraft:raw_iron".to_string()),
            OwnedRecipeIngredient::OneOf(vec![
                "minecraft:raw_iron".to_string(),
                "minecraft:raw_gold".to_string(),
            ]),
        ];
        let stone = OwnedRecipeIngredient::Simple("minecraft:stone".to_string());
        let body = CUpdateRecipes::encode(
            &[PropertySetSync {
                key: "minecraft:furnace_input",
                ingredients: &furnace,
            }],
            &[StonecutterSync {
                ingredient: &stone,
                result_item: "minecraft:stone_slab",
                result_count: 2,
            }],
            version,
        )
        .unwrap();

        let mut expected = var_int(1);
        expected.write_string("minecraft:furnace_input").unwrap();
        expected.extend(var_int(2));
        expected.extend(var_int(network_id(&Item::RAW_IRON, version)));
        expected.extend(var_int(network_id(&Item::RAW_GOLD, version)));
        expected.extend(var_int(1));
        // A direct holder set of one item: size + 1, then the item id.
        expected.extend(var_int(2));
        expected.extend(var_int(network_id(&Item::STONE, version)));
        // `minecraft:item_stack` is id 5 of `minecraft:slot_display_type`, followed by the
        // `ItemStackTemplate`: item, count, then the empty component patch.
        expected.extend(var_int(5));
        expected.extend(var_int(network_id(&Item::STONE_SLAB, version)));
        expected.extend(var_int(2));
        expected.extend(var_int(0));
        expected.extend(var_int(0));
        assert_eq!(body, expected);
    }

    #[test]
    fn tag_ingredient_lists_every_tag_member_once() {
        let version = EncodingKey::from(JavaMinecraftVersion::V_26_3);
        let ingredients = [
            OwnedRecipeIngredient::Tagged("minecraft:planks".to_string()),
            OwnedRecipeIngredient::Simple("minecraft:oak_planks".to_string()),
        ];
        let ids = property_set_item_ids(&ingredients, version);
        let oak = network_id(&Item::OAK_PLANKS, version);
        assert_eq!(ids.iter().filter(|id| **id == oak).count(), 1);
        assert!(ids.len() > 5);
    }

    #[test]
    fn a_tag_ingredient_is_a_direct_list() {
        let version = EncodingKey::from(JavaMinecraftVersion::V_26_3);
        let logs = OwnedRecipeIngredient::Tagged("minecraft:planks".to_string());
        let mut body = Vec::new();
        write_dynamic_ingredient_holderset(&mut body, &logs, version).unwrap();
        let members = logs.items().len() as i32;
        assert!(members > 5);
        assert_eq!(body[..1], var_int(members + 1)[..]);
    }

    #[test]
    fn an_ingredient_without_items_is_an_error() {
        let version = EncodingKey::from(JavaMinecraftVersion::V_26_3);
        for ingredient in [
            OwnedRecipeIngredient::Simple("test:missing".to_string()),
            OwnedRecipeIngredient::Tagged("test:missing".to_string()),
            OwnedRecipeIngredient::OneOf(vec!["test:missing".to_string()]),
        ] {
            let mut body = Vec::new();
            assert!(write_dynamic_ingredient_holderset(&mut body, &ingredient, version).is_err());
        }
    }
}
