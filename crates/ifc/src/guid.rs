//! Globally unique identifiers: name-based UUIDs and the IFC 22-character compressed form.
//!
//! Kanavos derives every `GlobalId` it writes deterministically, so that exporting the same project twice gives
//! the same identifiers and other tools can match objects between exports:
//!
//! 1. The project has a namespace UUID: [`ExportOptions::project_guid`](crate::ExportOptions::project_guid) if
//!    the caller supplies one, otherwise the document's permanent project id (so renaming the project keeps every
//!    identifier), and only for documents without an id a version-5 UUID of the Kanavos namespace and the
//!    project's descriptive fields (see [`document_namespace`] and [`project_namespace`]).
//! 2. Each object gets the version-5 UUID (RFC 9562 §5.5, SHA-1 based) of the project namespace and a stable
//!    name such as `element:42` or `rel:contains:storey:7`.
//! 3. The 128 bits are written in the IFC compressed form: 22 characters from the 64-character alphabet
//!    `0-9 A-Z a-z _ $`, the first character carrying the top 2 bits and each following one 6 bits, most
//!    significant first (buildingSMART `IfcGloballyUniqueId`).

use std::fmt;

/// The IFC base-64 alphabet, in digit order.
const ALPHABET: &[u8; 64] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";

/// A 128-bit UUID, bytes in network (big-endian) order as in RFC 9562.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Guid(pub [u8; 16]);

/// The RFC 9562 namespace for URLs, `6ba7b811-9dad-11d1-80b4-00c04fd430c8`.
pub const NAMESPACE_URL: Guid =
    Guid([0x6b, 0xa7, 0xb8, 0x11, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30, 0xc8]);

impl Guid {
    /// The name-based (version 5, SHA-1) UUID of `name` in `namespace`.
    pub fn v5(namespace: &Guid, name: &[u8]) -> Guid {
        let mut input = Vec::with_capacity(16 + name.len());
        input.extend_from_slice(&namespace.0);
        input.extend_from_slice(name);
        let hash = sha1(&input);
        let mut bytes = [0u8; 16];
        for (b, h) in bytes.iter_mut().zip(hash.iter()) {
            *b = *h;
        }
        bytes[6] = (bytes[6] & 0x0F) | 0x50;
        bytes[8] = (bytes[8] & 0x3F) | 0x80;
        Guid(bytes)
    }

    /// The UUID as a 128-bit integer.
    pub fn as_u128(&self) -> u128 {
        u128::from_be_bytes(self.0)
    }

    /// The 22-character IFC form.
    pub fn to_ifc(&self) -> String {
        let n = self.as_u128();
        (0..22)
            .map(|i| {
                let shift = 6 * (21 - i);
                let digit = ((n >> shift) & 0x3F) as usize;
                ALPHABET.get(digit).map(|&c| c as char).unwrap_or('0')
            })
            .collect()
    }

    /// Parses the 22-character IFC form. Returns `None` for wrong length, characters outside the alphabet, or a
    /// first character above `3` (which would exceed 128 bits).
    pub fn from_ifc(s: &str) -> Option<Guid> {
        let bytes = s.as_bytes();
        if bytes.len() != 22 {
            return None;
        }
        let mut n: u128 = 0;
        for (i, &c) in bytes.iter().enumerate() {
            let digit = ALPHABET.iter().position(|&a| a == c)? as u128;
            if i == 0 && digit > 3 {
                return None;
            }
            n = (n << 6) | digit;
        }
        Some(Guid(n.to_be_bytes()))
    }
}

impl fmt::Display for Guid {
    /// The canonical `8-4-4-4-12` hexadecimal form.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, b) in self.0.iter().enumerate() {
            if matches!(i, 4 | 6 | 8 | 10) {
                f.write_str("-")?;
            }
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

/// Namespace of all Kanavos-derived identifiers: the URL-namespace UUID of `urn:kanavos:ifc`.
pub fn kanavos_namespace() -> Guid {
    Guid::v5(&NAMESPACE_URL, b"urn:kanavos:ifc")
}

/// The namespace of a document's derived identifiers: its project id, or, when the document has none yet (files
/// written before project ids existed), [`project_namespace`] of its descriptive fields.
pub fn document_namespace(doc: &kanavos_model::Document) -> Guid {
    let id = doc.project_id();
    if id.is_nil() {
        let info = doc.info();
        project_namespace(&info.name, &info.location, &info.municipality, &info.owner)
    } else {
        Guid(id.to_bytes())
    }
}

/// Fallback project namespace, from the project's descriptive fields, for documents without a project id.
/// Renaming such a project changes it.
pub fn project_namespace(name: &str, location: &str, municipality: &str, owner: &str) -> Guid {
    let key = format!("project\n{name}\n{location}\n{municipality}\n{owner}");
    Guid::v5(&kanavos_namespace(), key.as_bytes())
}

/// SHA-1 (FIPS 180-4). Used only to derive name-based UUIDs, not for security.
pub fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476, 0xC3D2_E1F0];
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());
    for block in message.chunks_exact(64) {
        let mut w = [0u32; 80];
        for (t, word) in block.chunks_exact(4).enumerate() {
            if let (Some(slot), [a, b, c, d]) = (w.get_mut(t), word) {
                *slot = u32::from_be_bytes([*a, *b, *c, *d]);
            }
        }
        for t in 16..80 {
            let v = w.get(t - 3).copied().unwrap_or(0)
                ^ w.get(t - 8).copied().unwrap_or(0)
                ^ w.get(t - 14).copied().unwrap_or(0)
                ^ w.get(t - 16).copied().unwrap_or(0);
            if let Some(slot) = w.get_mut(t) {
                *slot = v.rotate_left(1);
            }
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (t, wt) in w.iter().enumerate() {
            let (f, k) = match t {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let temp = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(*wt);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        for (hv, v) in h.iter_mut().zip([a, b, c, d, e]) {
            *hv = hv.wrapping_add(v);
        }
    }
    let mut out = [0u8; 20];
    for (chunk, v) in out.chunks_exact_mut(4).zip(h) {
        chunk.copy_from_slice(&v.to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn sha1_matches_fips_examples() {
        assert_eq!(hex(&sha1(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(hex(&sha1(b"abc")), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(
            hex(&sha1(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
        assert_eq!(hex(&sha1(&vec![b'a'; 1_000_000])), "34aa973cd4c4daa4f61eeb2bdbad27316534016f");
    }

    #[test]
    fn uuid_v5_matches_rfc_9562_example() {
        // RFC 9562 appendix A.4: DNS namespace, name "www.example.com".
        let dns = Guid([
            0x6b, 0xa7, 0xb8, 0x10, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30, 0xc8,
        ]);
        assert_eq!(Guid::v5(&dns, b"www.example.com").to_string(), "2ed6657d-e927-568b-95e1-2665a8aea6a2");
    }

    #[test]
    fn ifc_form_round_trips() {
        assert_eq!(Guid([0; 16]).to_ifc(), "0000000000000000000000");
        assert_eq!(Guid([0xFF; 16]).to_ifc(), "3zzzzzzzzzzzzzzzzzzzzz".replace('z', "$"));
        let g = Guid::v5(&kanavos_namespace(), b"element:42");
        let s = g.to_ifc();
        assert_eq!(s.len(), 22);
        assert_eq!(Guid::from_ifc(&s), Some(g));
        assert_eq!(Guid::from_ifc("4000000000000000000000"), None);
        assert_eq!(Guid::from_ifc("short"), None);
        assert_eq!(Guid::from_ifc("000000000000000000000!"), None);
    }

    #[test]
    fn derived_ids_are_stable_and_distinct() {
        let ns = project_namespace("Κατοικία", "Καλαμάτα", "", "");
        let a = Guid::v5(&ns, b"element:1");
        assert_eq!(a, Guid::v5(&ns, b"element:1"));
        assert_ne!(a, Guid::v5(&ns, b"element:2"));
        assert_ne!(ns, project_namespace("Κατοικία 2", "Καλαμάτα", "", ""));
    }

    #[test]
    fn the_project_id_is_the_namespace_and_survives_renames() {
        use kanavos_model::{Document, ProjectId, ProjectInfo};
        let id = ProjectId::from_u128(0x0f1e_2d3c_4b5a_4978_8695_a4b3_c2d1_e0f0);
        let doc = Document::new()
            .with_project_id(id)
            .with_info(ProjectInfo { name: "Α".into(), ..ProjectInfo::default() });
        assert_eq!(document_namespace(&doc).to_string(), id.to_string());
        let renamed = doc.with_info(ProjectInfo { name: "Β".into(), ..ProjectInfo::default() });
        assert_eq!(document_namespace(&renamed), document_namespace(&doc));
        // Without an id the descriptive fields decide.
        let old = Document::new().with_info(ProjectInfo { name: "Α".into(), ..ProjectInfo::default() });
        assert_eq!(document_namespace(&old), project_namespace("Α", "", "", ""));
    }
}
