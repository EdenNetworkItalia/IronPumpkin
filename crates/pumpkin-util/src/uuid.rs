use uuid::Uuid;

/// Java's `UUID.nameUUIDFromBytes`: an MD5 UUID of the bytes alone.
///
/// `Uuid::new_v3` does not match it: that one hashes the namespace bytes too.
#[must_use]
pub fn name_uuid_from_bytes(bytes: &[u8]) -> Uuid {
    uuid::Builder::from_md5_bytes(md5::compute(bytes).0).into_uuid()
}

/// Derives the UUID of a player in offline mode, like vanilla's
/// `UUIDUtil.createOfflinePlayerUUID`.
#[must_use]
pub fn offline_player_uuid(name: &str) -> Uuid {
    name_uuid_from_bytes(format!("OfflinePlayer:{name}").as_bytes())
}

/// Parses a UUID similar to the way that Java does, which makes it ideal for
/// keeping the same behaviors sometimes.
#[must_use]
pub fn parse_uuid_array(uuid: &str) -> Option<[i32; 4]> {
    // We can't directly use the uuid crate to parse UUIDs, as it parses them
    // in a different way from Java.

    if uuid.len() > 36 {
        // UUID string is too large.
        return None;
    }

    // Split by hyphen. (5 segments)
    let mut parts = uuid.split('-');
    let mut parsed_parts: [i64; 5] = [0; 5];

    for part in &mut parsed_parts {
        // If a part is empty, the parsing functions will error anyway - this is what we want.
        *part = i64::from_str_radix(parts.next()?, 16).ok()?;
    }

    if parts.next().is_some() {
        // UUIDs must have exactly 5 parts.
        return None;
    }

    let bits = [
        (parsed_parts[0] & 0xFFFFFFFF) << 32
            | (parsed_parts[1] & 0xFFFF) << 16
            | (parsed_parts[2] & 0xFFFF),
        (parsed_parts[3] & 0xFFFF) << 48 | (parsed_parts[4] & 0xFFFFFFFFFFFF),
    ];

    Some([
        (bits[0] >> 32) as i32,
        bits[0] as i32,
        (bits[1] >> 32) as i32,
        bits[1] as i32,
    ])
}

/// Parses a UUID similar to the way that Java does, which makes it ideal for
/// keeping the same behaviors sometimes.
#[must_use]
pub fn parse_uuid_vec(uuid: &str) -> Option<Vec<i32>> {
    parse_uuid_array(uuid).map(Vec::from)
}

/// Parses a UUID similar to the way that Java does, which makes it ideal for
/// keeping the same behaviors sometimes.
#[must_use]
pub fn parse_uuid(uuid: &str) -> Option<Uuid> {
    let [a, b, c, d] = parse_uuid_array(uuid)?;

    let uuid_int = (a as u32 as u128) << 96
        | (b as u32 as u128) << 64
        | (c as u32 as u128) << 32
        | (d as u32 as u128);

    Some(Uuid::from_u128(uuid_int))
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use uuid::Uuid;

    use crate::uuid::{name_uuid_from_bytes, offline_player_uuid, parse_uuid, parse_uuid_array};

    /// Expected values come from `UUID.nameUUIDFromBytes(("OfflinePlayer:" + name).getBytes(UTF_8))`
    /// run on a JDK, the same call as vanilla's `UUIDUtil.createOfflinePlayerUUID`.
    #[test]
    fn offline_player_uuid_matches_java() {
        for (name, expected) in [
            ("Notch", "b50ad385-829d-3141-a216-7e7d7539ba7f"),
            ("jeb_", "a762f560-4fce-3236-812a-b80efff0b62b"),
            ("Steve", "5627dd98-e6be-3c21-b8a8-e92344183641"),
            ("Player123", "3da1e349-17b0-38ed-a07b-1e78c8ebd956"),
            ("a", "52428a0e-1e30-3cb1-976c-e728b2614047"),
        ] {
            assert_eq!(
                offline_player_uuid(name),
                Uuid::from_str(expected).unwrap(),
                "{name}"
            );
        }
    }

    /// Expected values come from `UUID.nameUUIDFromBytes(url.getBytes(UTF_8))` run on a JDK, the
    /// call vanilla's `DedicatedServerProperties.getServerPackInfo` uses for the resource pack id.
    #[test]
    fn name_uuid_from_bytes_matches_java() {
        for (url, expected) in [
            (
                "https://example.com/pack.zip",
                "50227fe1-89d1-393b-8884-7d487e29fa5a",
            ),
            (
                "https://example.org/packs/server-pack.zip",
                "4f356823-1abb-3bd7-b891-bc79cbeb1197",
            ),
            (
                "http://localhost:8080/resourcepack.zip",
                "cbfc4a96-f3de-3703-9b7e-69d586ee0c5a",
            ),
            ("", "d41d8cd9-8f00-3204-a980-0998ecf8427e"),
        ] {
            let uuid = name_uuid_from_bytes(url.as_bytes());
            assert_eq!(uuid, Uuid::from_str(expected).unwrap(), "{url}");
            // The old derivation hashed the DNS namespace as well and gave another id.
            assert_ne!(
                uuid,
                Uuid::new_v3(&Uuid::NAMESPACE_DNS, url.as_bytes()),
                "{url}"
            );
        }
    }

    #[test]
    fn parse_uuids() {
        assert_eq!(
            parse_uuid_array("3d569d3a-93ef-44a0-9f1c-f69db9d37a56"),
            Some([1029086522, -1813035872, -1625491811, -1177322922])
        );
        assert_eq!(
            parse_uuid_array("3d53a-f-40-c-f69db9d37a56"),
            Some([251194, 983104, 849565, -1177322922])
        );
        assert_eq!(parse_uuid_array("3d53a-f40-c-f69db9d37a56"), None);
        assert_eq!(
            parse_uuid_array("fffffffffffffff-0-0-0-0"),
            Some([-1, 0, 0, 0])
        );
        assert_eq!(parse_uuid_array("ffffffffffffffff-0-0-0-0"), None);
        assert_eq!(
            parse_uuid_array("+1-+2-+3-+4-+5"),
            Some([1, 131075, 262144, 5])
        );
        assert_eq!(
            parse_uuid_array("aaaaaaaaaaaaaaa-bbbbbbbbbbbbbb-c-d-e"),
            Some([-1431655766, -1145372660, 851968, 14])
        );
        assert_eq!(
            parse_uuid_array("aaaaaaaaaaaaaaa-bbbbbbbbbbbbbb-c--e"),
            None
        );

        assert_eq!(
            parse_uuid("3d569d3a-93ef-44a0-9f1c-f69db9d37a56"),
            Some(Uuid::from_str("3d569d3a-93ef-44a0-9f1c-f69db9d37a56").unwrap())
        );
        assert_eq!(
            parse_uuid("3d53a-f-40-c-f69db9d37a56"),
            Some(Uuid::from_str("0003d53a-000f-0040-000c-f69db9d37a56").unwrap())
        );
        assert_eq!(
            parse_uuid("+1-+2-+3-+4-+5"),
            Some(Uuid::from_str("00000001-0002-0003-0004-000000000005").unwrap())
        );
    }
}
