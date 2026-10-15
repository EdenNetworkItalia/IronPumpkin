//! Maven versions and version ranges, as `neoforge.mods.toml` uses them.
//!
//! `FancyModLoader` 12.0.8 reads a mod version with maven-artifact `DefaultArtifactVersion` and a
//! `versionRange` with `VersionRange.createFromVersionSpec` (`ModInfo`, `MavenVersionAdapter`).
//! [`ArtifactVersion`] orders versions like maven-artifact `ComparableVersion`: numbers compare
//! as numbers, the qualifiers `alpha < beta < milestone < rc < snapshot < release < sp`, and other
//! qualifiers after `sp` in text order. It does not implement the combination items of
//! maven-artifact 3.9 (`1-beta1` reads as `1-beta-1`), which no mod version of the reference
//! modpack needs.

use std::{cmp::Ordering, fmt};

/// A mod version. Every text is a version; the comparison follows Maven.
#[derive(Debug, Clone)]
pub struct ArtifactVersion {
    text: String,
    items: Vec<Item>,
}

impl ArtifactVersion {
    #[must_use]
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            items: parse_items(&text.to_lowercase()),
        }
    }
}

impl fmt::Display for ArtifactVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl PartialEq for ArtifactVersion {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for ArtifactVersion {}

impl PartialOrd for ArtifactVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ArtifactVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_lists(&self.items, Some(&other.items))
    }
}

#[derive(Debug, Clone)]
enum Item {
    /// Decimal digits without leading zeros; `0` is the empty string.
    Int(String),
    Str(String),
    List(Vec<Self>),
}

/// The qualifier order of `ComparableVersion.StringItem`; the empty qualifier is a release.
const QUALIFIERS: [&str; 7] = ["alpha", "beta", "milestone", "rc", "snapshot", "", "sp"];
const RELEASE: usize = 5;

fn int_item(digits: &str) -> Item {
    Item::Int(digits.trim_start_matches('0').to_owned())
}

fn string_item(text: &str, followed_by_digit: bool) -> Item {
    let text = match (text, followed_by_digit) {
        ("a", true) => "alpha",
        ("b", true) => "beta",
        ("m", true) => "milestone",
        ("ga" | "final" | "release", _) => "",
        ("cr", _) => "rc",
        (other, _) => other,
    };
    Item::Str(text.to_owned())
}

fn parse_item(digit: bool, text: &str) -> Item {
    if digit {
        int_item(text)
    } else {
        string_item(text, false)
    }
}

/// `ComparableVersion.parseVersion`: `.` separates items, `-` and a switch between digits and
/// letters open a sub-list.
fn parse_items(version: &str) -> Vec<Item> {
    // A new list never gets an item back to its parent, so the lists form a chain: `parents`
    // holds the open lists, and each one ends with the next when the chain folds.
    let mut parents: Vec<Vec<Item>> = Vec::new();
    let mut list = Vec::new();
    let mut is_digit = false;
    let mut start = 0;
    for (i, c) in version.bytes().enumerate() {
        match c {
            b'.' | b'-' => {
                list.push(if i == start {
                    int_item("")
                } else {
                    parse_item(is_digit, &version[start..i])
                });
                start = i + 1;
                if c == b'-' {
                    parents.push(std::mem::take(&mut list));
                }
            }
            b'0'..=b'9' => {
                if !is_digit && i > start {
                    list.push(string_item(&version[start..i], true));
                    start = i;
                    parents.push(std::mem::take(&mut list));
                }
                is_digit = true;
            }
            _ => {
                if is_digit && i > start {
                    list.push(int_item(&version[start..i]));
                    start = i;
                    parents.push(std::mem::take(&mut list));
                }
                is_digit = false;
            }
        }
    }
    if version.len() > start {
        list.push(parse_item(is_digit, &version[start..]));
    }
    normalize(&mut list);
    while let Some(mut parent) = parents.pop() {
        parent.push(Item::List(list));
        normalize(&mut parent);
        list = parent;
    }
    list
}

/// `ListItem.normalize`, after the lists inside it: walking back from the end, it drops each null
/// item (`0`, a release qualifier, an empty list) until it meets an item that is not a list.
fn normalize(list: &mut Vec<Item>) {
    for i in (0..list.len()).rev() {
        if is_null(&list[i]) {
            list.remove(i);
        } else if !matches!(list[i], Item::List(_)) {
            break;
        }
    }
}

fn is_null(item: &Item) -> bool {
    match item {
        Item::Int(digits) => digits.is_empty(),
        Item::Str(text) => qualifier_rank(text) == RELEASE.to_string(),
        Item::List(items) => items.is_empty(),
    }
}

fn qualifier_rank(text: &str) -> String {
    QUALIFIERS
        .iter()
        .position(|qualifier| *qualifier == text)
        .map_or_else(
            || format!("{}-{text}", QUALIFIERS.len()),
            |index| index.to_string(),
        )
}

fn compare_ints(a: &str, b: &str) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

/// `Item.compareTo`, where `None` is the missing item of the shorter list.
fn compare(item: &Item, other: Option<&Item>) -> Ordering {
    match (item, other) {
        (Item::Int(digits), None) => {
            if digits.is_empty() {
                Ordering::Equal
            } else {
                Ordering::Greater
            }
        }
        (Item::Int(a), Some(Item::Int(b))) => compare_ints(a, b),
        (Item::Int(_), Some(Item::Str(_) | Item::List(_)))
        | (Item::List(_), Some(Item::Str(_))) => Ordering::Greater,
        (Item::Str(text), None) => qualifier_rank(text).cmp(&RELEASE.to_string()),
        (Item::Str(_), Some(Item::Int(_) | Item::List(_)))
        | (Item::List(_), Some(Item::Int(_))) => Ordering::Less,
        (Item::Str(a), Some(Item::Str(b))) => qualifier_rank(a).cmp(&qualifier_rank(b)),
        (Item::List(items), None) => items
            .first()
            .map_or(Ordering::Equal, |first| compare(first, None)),
        (Item::List(a), Some(Item::List(b))) => compare_lists(a, Some(b)),
    }
}

fn compare_lists(items: &[Item], other: Option<&[Item]>) -> Ordering {
    let other = other.unwrap_or(&[]);
    for i in 0..items.len().max(other.len()) {
        let result = match (items.get(i), other.get(i)) {
            (Some(left), right) => compare(left, right),
            (None, Some(right)) => compare(right, None).reverse(),
            (None, None) => Ordering::Equal,
        };
        if result != Ordering::Equal {
            return result;
        }
    }
    Ordering::Equal
}

/// A `versionRange` of `neoforge.mods.toml`.
///
/// It is `[1.0,2.0)`, `[26.3]`, `[1.0,)`, `(,2.0]`, a union such as `[1,2),[3,4)`, or a bare
/// version, which Maven reads as a recommendation that every version satisfies. A blank range
/// accepts every version, like `IModInfo.UNBOUNDED`.
#[derive(Debug, Clone)]
pub struct VersionRange {
    spec: String,
    /// Empty for a bare version or a blank range: every version matches.
    restrictions: Vec<Restriction>,
}

#[derive(Debug, Clone)]
struct Restriction {
    lower: Option<(ArtifactVersion, bool)>,
    upper: Option<(ArtifactVersion, bool)>,
}

impl Restriction {
    fn contains(&self, version: &ArtifactVersion) -> bool {
        let above = self.lower.as_ref().is_none_or(|(lower, inclusive)| {
            let order = version.cmp(lower);
            order == Ordering::Greater || (*inclusive && order == Ordering::Equal)
        });
        let below = self.upper.as_ref().is_none_or(|(upper, inclusive)| {
            let order = version.cmp(upper);
            order == Ordering::Less || (*inclusive && order == Ordering::Equal)
        });
        above && below
    }
}

impl VersionRange {
    /// A range that every version satisfies.
    #[must_use]
    pub const fn unbounded() -> Self {
        Self {
            spec: String::new(),
            restrictions: Vec::new(),
        }
    }

    /// Parses a Maven version range, like `VersionRange.createFromVersionSpec`.
    ///
    /// # Errors
    ///
    /// Returns the reason when a bracket is not closed, an exact range is not `[x]`, or a bound
    /// is missing on both sides.
    pub fn parse(spec: &str) -> Result<Self, String> {
        let mut restrictions = Vec::new();
        let mut rest = spec.trim();
        if !rest.starts_with(['[', '(']) {
            return Ok(Self {
                spec: spec.to_owned(),
                restrictions,
            });
        }
        while !rest.is_empty() {
            let end = rest
                .find([']', ')'])
                .ok_or_else(|| format!("unbounded range: {spec}"))?;
            restrictions.push(parse_restriction(&rest[..=end], spec)?);
            rest = rest[end + 1..].trim_start();
            if let Some(after) = rest.strip_prefix(',') {
                rest = after.trim_start();
            } else if !rest.is_empty() {
                return Err(format!("only a comma may separate ranges: {spec}"));
            }
        }
        Ok(Self {
            spec: spec.to_owned(),
            restrictions,
        })
    }

    #[must_use]
    pub fn contains(&self, version: &ArtifactVersion) -> bool {
        self.restrictions.is_empty()
            || self
                .restrictions
                .iter()
                .any(|restriction| restriction.contains(version))
    }
}

fn parse_restriction(text: &str, spec: &str) -> Result<Restriction, String> {
    let lower_inclusive = text.starts_with('[');
    let upper_inclusive = text.ends_with(']');
    if !text.starts_with(['[', '(']) {
        return Err(format!("a range must start with [ or (: {spec}"));
    }
    let inner = text[1..text.len() - 1].trim();
    let bound = |text: &str| {
        let text = text.trim();
        (!text.is_empty()).then(|| ArtifactVersion::new(text))
    };
    let Some((lower, upper)) = inner.split_once(',') else {
        if !lower_inclusive || !upper_inclusive || inner.is_empty() {
            return Err(format!("a single version must be in [ and ]: {spec}"));
        }
        let version = ArtifactVersion::new(inner);
        return Ok(Restriction {
            lower: Some((version.clone(), true)),
            upper: Some((version, true)),
        });
    };
    let (lower, upper) = (bound(lower), bound(upper));
    if lower.is_none() && upper.is_none() {
        return Err(format!("a range needs at least one bound: {spec}"));
    }
    Ok(Restriction {
        lower: lower.map(|version| (version, lower_inclusive)),
        upper: upper.map(|version| (version, upper_inclusive)),
    })
}

impl fmt::Display for VersionRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.spec.trim().is_empty() {
            f.write_str("any version")
        } else {
            f.write_str(&self.spec)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ArtifactVersion, VersionRange};

    fn v(text: &str) -> ArtifactVersion {
        ArtifactVersion::new(text)
    }

    #[test]
    fn orders_like_maven() {
        let ordered = [
            "1-alpha1",
            "1-beta",
            "1-rc",
            "1-snapshot",
            "1",
            "1.0.1",
            "1.1",
            "1.10",
            "2",
        ];
        for pair in ordered.windows(2) {
            assert!(v(pair[0]) < v(pair[1]), "{} < {}", pair[0], pair[1]);
        }
        assert_eq!(v("26.3"), v("26.3.0"));
        assert_eq!(v("1.0-final"), v("1"));
        assert_eq!(v("1.0-beta"), v("1-beta"));
        assert!(v("26.3.0.64-beta") < v("26.3.0.64"));
        assert!(v("26.3.0.65-beta") > v("26.3.0.64"));
    }

    #[test]
    fn ranges_contain_like_maven() {
        let range = VersionRange::parse("[4.0,5.0)").unwrap();
        assert!(range.contains(&v("4.0")));
        assert!(range.contains(&v("4.9.9")));
        assert!(!range.contains(&v("5.0")));
        assert!(!range.contains(&v("3.9")));

        let exact = VersionRange::parse("[26.3]").unwrap();
        assert!(exact.contains(&v("26.3")));
        assert!(!exact.contains(&v("26.3.1")));

        let open = VersionRange::parse("[26.3.0.64-beta,)").unwrap();
        assert!(open.contains(&v("26.3.0.64-beta")));
        assert!(!open.contains(&v("26.3.0.63-beta")));

        let union = VersionRange::parse("(,1.0],[2.0,)").unwrap();
        assert!(union.contains(&v("0.5")));
        assert!(!union.contains(&v("1.5")));
        assert!(union.contains(&v("2.1")));

        assert!(VersionRange::parse("1.0").unwrap().contains(&v("0.1")));
        assert!(VersionRange::parse(" ").unwrap().contains(&v("0.1")));
        assert!(VersionRange::parse("[1.0").is_err());
        assert!(VersionRange::parse("(1.0)").is_err());
        assert!(VersionRange::parse("[,]").is_err());
    }
}
