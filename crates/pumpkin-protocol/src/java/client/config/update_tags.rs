use std::io::Write;

use crate::{ClientPacket, WritingError, ser::NetworkWriteExt};

use crate::EncodingKey;
use crate::codec::var_int::VarInt;
use pumpkin_data::{
    dynamic,
    packet::clientbound::config::UPDATE_TAGS,
    tag::{RegistryKey, get_registry_key_tags},
};
use pumpkin_macros::java_packet;

#[java_packet(UPDATE_TAGS)]
pub struct CUpdateTags<'a> {
    pub tags: &'a [pumpkin_data::tag::RegistryKey],
}

impl<'a> CUpdateTags<'a> {
    #[must_use]
    pub const fn new(tags: &'a [RegistryKey]) -> Self {
        Self { tags }
    }
}

impl ClientPacket for CUpdateTags<'_> {
    fn write_packet_data(
        &self,
        mut write: impl Write,
        version: &EncodingKey,
    ) -> Result<(), WritingError> {
        let valid_keys: Vec<_> = self
            .tags
            .iter()
            .copied()
            .filter(|key| key.is_valid_for_version(version.version()))
            .collect();

        write.write_list(&valid_keys, |p, &registry_key| {
            p.write_string(&format!("minecraft:{}", registry_key.identifier_string()))?;

            let Some(values) = get_registry_key_tags(version.version(), registry_key) else {
                // no tags defined for that registry key in this version
                // write an empty list and continue
                p.write_var_int(&VarInt::from(0))?;
                return Ok(());
            };
            let ids = version.content_ids();
            let mod_tags = dynamic::network_mod_tags(registry_key, ids);
            let count = values.len() + mod_tags.len();
            p.write_var_int(&count.try_into().map_err(|_| {
                WritingError::Message(format!("{count} isn't representable as a VarInt"))
            })?)?;

            for (key, values) in values.entries() {
                // This is technically a `ResourceLocation` but same thing
                p.write_string_bounded(key, u16::MAX as usize)?;
                let members = dynamic::network_tag_ids(registry_key, key, values.1, ids);
                p.write_list(&members, |p, &id| p.write_var_int(&VarInt::from(id)))?;
            }
            for (key, members) in &mod_tags {
                p.write_string_bounded(key, u16::MAX as usize)?;
                p.write_list(members, |p, &id| p.write_var_int(&VarInt::from(id)))?;
            }

            Ok(())
        })
    }
}
