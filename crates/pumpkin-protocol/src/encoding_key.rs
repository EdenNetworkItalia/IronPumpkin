use std::cmp::Ordering;
use std::ops::Deref;

use pumpkin_data::dynamic::ContentIds;
use pumpkin_util::version::JavaMinecraftVersion;

/// What a clientbound Java packet is encoded for: the protocol version and the content ids of the
/// receiving connection. Connections with the same key get the same bytes.
///
/// Packet writers take it where they took the version. It dereferences to the version and
/// compares with a version, so the version checks of a writer read as before.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct EncodingKey {
    version: JavaMinecraftVersion,
    content_ids: ContentIds,
}

impl EncodingKey {
    #[must_use]
    pub const fn new(version: JavaMinecraftVersion, content_ids: ContentIds) -> Self {
        Self {
            version,
            content_ids,
        }
    }

    #[must_use]
    pub const fn version(&self) -> JavaMinecraftVersion {
        self.version
    }

    #[must_use]
    pub const fn content_ids(&self) -> ContentIds {
        self.content_ids
    }
}

/// The key of a connection that gets display ids, like every vanilla client.
impl From<JavaMinecraftVersion> for EncodingKey {
    fn from(version: JavaMinecraftVersion) -> Self {
        Self::new(version, ContentIds::Display)
    }
}

impl Deref for EncodingKey {
    type Target = JavaMinecraftVersion;

    fn deref(&self) -> &JavaMinecraftVersion {
        &self.version
    }
}

impl PartialEq<JavaMinecraftVersion> for EncodingKey {
    fn eq(&self, other: &JavaMinecraftVersion) -> bool {
        self.version == *other
    }
}

impl PartialOrd<JavaMinecraftVersion> for EncodingKey {
    fn partial_cmp(&self, other: &JavaMinecraftVersion) -> Option<Ordering> {
        self.version.partial_cmp(other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_with_a_version() {
        let key = EncodingKey::new(JavaMinecraftVersion::V_26_3, ContentIds::Real);
        assert!(key >= JavaMinecraftVersion::V_1_21_5);
        assert!(key < JavaMinecraftVersion::Unknown);
        assert_eq!(key, JavaMinecraftVersion::V_26_3);
        assert_eq!(*key, JavaMinecraftVersion::V_26_3);
        assert_eq!(
            EncodingKey::from(JavaMinecraftVersion::V_26_3).content_ids(),
            ContentIds::Display
        );
    }
}
