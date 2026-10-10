//! The permanent identity of a project: a 128-bit id written as a UUID.
//!
//! The id is created once, when a project is started (see [`crate::starter`]), and never changes afterwards:
//! renaming the project, moving it on disk or editing its information keeps it. Exports use it to derive stable
//! identifiers (IFC `GlobalId`s are name-based UUIDs in the project's namespace).
//!
//! New ids use the version 4 (random) layout of RFC 9562 §5.4. The 122 random bits come from the standard
//! library's randomly keyed hasher ([`RandomState`], seeded from the operating system) mixed with the system time,
//! the process id and a per-process counter, so that no extra dependency is needed. The ids are unique for
//! practical purposes; they are not meant to be secret.

use std::collections::hash_map::RandomState;
use std::fmt;
use std::hash::{BuildHasher, Hasher};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A project id: 128 bits, shown and stored in the canonical UUID form `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`
/// (lowercase hexadecimal). The nil id (all zeros) means "not assigned yet".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectId(u128);

/// Why a text is not a project id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidProjectId(pub String);

impl fmt::Display for InvalidProjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "not a UUID in the form xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx: {:?}", self.0)
    }
}

impl std::error::Error for InvalidProjectId {}

/// Counter mixed into every generated id so that two ids made in the same instant differ.
static GENERATED: AtomicU64 = AtomicU64::new(0);

impl ProjectId {
    /// The nil id: no id assigned yet (documents made before ids existed, empty documents).
    pub const NIL: ProjectId = ProjectId(0);

    /// The id with these 128 bits (most significant first, as in the UUID's text form).
    pub const fn from_u128(bits: u128) -> ProjectId {
        ProjectId(bits)
    }

    /// The 128 bits.
    pub const fn as_u128(self) -> u128 {
        self.0
    }

    /// The 16 bytes in network (big-endian) order, as in RFC 9562.
    pub const fn to_bytes(self) -> [u8; 16] {
        self.0.to_be_bytes()
    }

    /// Whether this is the nil id.
    pub fn is_nil(&self) -> bool {
        self.0 == 0
    }

    /// A new random id in the version 4 layout (see the module documentation for where the bits come from).
    pub fn generate() -> ProjectId {
        let count = GENERATED.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let pid = std::process::id();
        let half = |salt: u64| -> u64 {
            let mut h = RandomState::new().build_hasher();
            h.write_u64(salt);
            h.write_u128(nanos);
            h.write_u32(pid);
            h.write_u64(count);
            h.finish()
        };
        let high = half(0x6b61_6e61_766f_7331);
        let low = half(0x6b61_6e61_766f_7332);
        let mut bytes = ((u128::from(high) << 64) | u128::from(low)).to_be_bytes();
        bytes[6] = (bytes[6] & 0x0F) | 0x40;
        bytes[8] = (bytes[8] & 0x3F) | 0x80;
        ProjectId(u128::from_be_bytes(bytes))
    }

    /// Parses the canonical form (hexadecimal digits in either case, hyphens after 8, 12, 16 and 20 digits).
    pub fn parse(text: &str) -> Result<ProjectId, InvalidProjectId> {
        let bad = || InvalidProjectId(text.to_owned());
        if text.len() != 36 {
            return Err(bad());
        }
        let mut bits: u128 = 0;
        for (i, c) in text.chars().enumerate() {
            if matches!(i, 8 | 13 | 18 | 23) {
                if c != '-' {
                    return Err(bad());
                }
                continue;
            }
            let digit = c.to_digit(16).ok_or_else(bad)?;
            bits = (bits << 4) | u128::from(digit);
        }
        Ok(ProjectId(bits))
    }
}

impl fmt::Display for ProjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hex = format!("{:032x}", self.0);
        for (i, c) in hex.chars().enumerate() {
            if matches!(i, 8 | 12 | 16 | 20) {
                f.write_str("-")?;
            }
            write!(f, "{c}")?;
        }
        Ok(())
    }
}

impl FromStr for ProjectId {
    type Err = InvalidProjectId;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ProjectId::parse(s)
    }
}

impl Serialize for ProjectId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ProjectId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        ProjectId::parse(&text).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_form_round_trips() {
        let id = ProjectId::from_u128(0x0123_4567_89ab_4def_8123_4567_89ab_cdef);
        let text = id.to_string();
        assert_eq!(text, "01234567-89ab-4def-8123-456789abcdef");
        assert_eq!(ProjectId::parse(&text), Ok(id));
        assert_eq!(ProjectId::parse(&text.to_uppercase()), Ok(id));
        assert_eq!(ProjectId::NIL.to_string(), "00000000-0000-0000-0000-000000000000");
    }

    #[test]
    fn malformed_text_is_refused() {
        for bad in [
            "",
            "01234567-89ab-4def-8123-456789abcde",
            "01234567-89ab-4def-8123-456789abcdeff",
            "0123456789ab-4def-8123-456789abcdef0",
            "01234567-89ab-4def-8123-456789abcdeg",
            "01234567_89ab_4def_8123_456789abcdef",
        ] {
            assert!(ProjectId::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn generated_ids_are_version_4_and_distinct() {
        let ids: Vec<ProjectId> = (0..64).map(|_| ProjectId::generate()).collect();
        for id in &ids {
            let b = id.to_bytes();
            assert_eq!(b[6] >> 4, 4, "{id}");
            assert_eq!(b[8] >> 6, 0b10, "{id}");
            assert!(!id.is_nil());
            // The text form shows the version digit.
            assert_eq!(id.to_string().chars().nth(14), Some('4'));
        }
        let unique: std::collections::BTreeSet<ProjectId> = ids.iter().copied().collect();
        assert_eq!(unique.len(), ids.len());
    }

    #[test]
    fn json_is_a_string() {
        let id = ProjectId::from_u128(0xfeed_beef_0000_4000_8000_0000_0000_0001);
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"feedbeef-0000-4000-8000-000000000001\"");
        assert_eq!(serde_json::from_str::<ProjectId>(&json).unwrap(), id);
        assert!(serde_json::from_str::<ProjectId>("\"nope\"").is_err());
        assert!(serde_json::from_str::<ProjectId>("42").is_err());
    }
}
