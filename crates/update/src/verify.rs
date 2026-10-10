//! Checking a downloaded package: SHA-256 against the `.sha256` file and Ed25519 against the release public key.
//!
//! Both checks must pass. The signature covers the raw bytes of the archive, so it also covers the version: the
//! top-level folder inside the archive carries the version number and is checked when installing.
//!
//! Failure modes: a missing checksum or signature file, a file that cannot be read (wrong length, not hex or not
//! base64, a checksum line for another file), a checksum mismatch and a signature that does not verify are all
//! errors; nothing is ever accepted on a partial check.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ring::digest::{Context, SHA256};
use ring::signature::{ED25519, UnparsedPublicKey};

use crate::UpdateError;

/// Lowercase hexadecimal SHA-256 of some bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut ctx = Context::new(&SHA256);
    ctx.update(bytes);
    hex(ctx.finish().as_ref())
}

/// Lowercase hexadecimal text of bytes.
pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        for nibble in [b >> 4, b & 15] {
            s.push(DIGITS.get(usize::from(nibble)).map(|d| char::from(*d)).unwrap_or('0'));
        }
    }
    s
}

/// Reads a checksum file in the `sha256sum` format (`<64 hex digits>  <file name>`) and returns the lowercase
/// digest for `file_name`. A line without a file name is accepted; a line naming another file is not used, and a
/// leading `*` (binary mode) before the name is allowed.
pub fn parse_checksum(text: &str, file_name: &str) -> Result<String, UpdateError> {
    let text = text.trim_start_matches('\u{feff}');
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let Some(digest) = parts.next() else { continue };
        let name = parts.next().map(|n| n.trim_start_matches('*'));
        if parts.next().is_some() {
            return Err(UpdateError::BadChecksumFile);
        }
        if name.is_some_and(|n| n != file_name) {
            continue;
        }
        if digest.len() != 64 || !digest.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(UpdateError::BadChecksumFile);
        }
        return Ok(digest.to_ascii_lowercase());
    }
    Err(UpdateError::BadChecksumFile)
}

/// Reads a signature file: one line of standard base64 holding a 64-byte Ed25519 signature.
pub fn parse_signature(text: &str) -> Result<[u8; 64], UpdateError> {
    let text: String = text.trim_start_matches('\u{feff}').split_whitespace().collect();
    let bytes = STANDARD.decode(text.as_bytes()).map_err(|_| UpdateError::BadSignatureFile)?;
    bytes.try_into().map_err(|_| UpdateError::BadSignatureFile)
}

/// Reads a public key: standard base64 of the 32-byte Ed25519 public key.
pub fn parse_public_key(text: &str) -> Option<[u8; 32]> {
    let bytes = STANDARD.decode(text.trim().as_bytes()).ok()?;
    bytes.try_into().ok()
}

/// Checks a downloaded archive against the text of its checksum file and of its signature file, using `key`.
///
/// `checksum` and `signature` are `None` when the release has no such file. Both must be present and both checks
/// must pass; the archive is otherwise rejected.
pub fn verify(
    archive: &[u8],
    archive_name: &str,
    checksum: Option<&str>,
    signature: Option<&str>,
    key: &[u8; 32],
) -> Result<(), UpdateError> {
    let checksum = checksum.ok_or(UpdateError::ChecksumMissing)?;
    let signature = signature.ok_or(UpdateError::SignatureMissing)?;
    let expected = parse_checksum(checksum, archive_name)?;
    let signature = parse_signature(signature)?;
    if sha256_hex(archive) != expected {
        return Err(UpdateError::ChecksumMismatch);
    }
    UnparsedPublicKey::new(&ED25519, key).verify(archive, &signature).map_err(|_| UpdateError::BadSignature)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_the_standard_test_vectors() {
        assert_eq!(sha256_hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn checksum_files_in_sha256sum_format_are_read() {
        let d = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(parse_checksum(&format!("{d}  a.zip\n"), "a.zip").unwrap(), d);
        assert_eq!(parse_checksum(&format!("{d} *a.zip\r\n"), "a.zip").unwrap(), d);
        assert_eq!(parse_checksum(&d.to_uppercase(), "a.zip").unwrap(), d);
        assert_eq!(parse_checksum(&format!("{d}  b.zip\n{d}  a.zip\n"), "a.zip").unwrap(), d);
        assert!(matches!(
            parse_checksum(&format!("{d}  b.zip\n"), "a.zip"),
            Err(UpdateError::BadChecksumFile)
        ));
        assert!(matches!(parse_checksum("abc  a.zip", "a.zip"), Err(UpdateError::BadChecksumFile)));
        assert!(matches!(parse_checksum("", "a.zip"), Err(UpdateError::BadChecksumFile)));
    }

    #[test]
    fn signatures_and_keys_need_exact_lengths() {
        assert!(parse_signature(&STANDARD.encode([7u8; 64])).is_ok());
        assert!(parse_signature(&format!("{}\n", STANDARD.encode([7u8; 64]))).is_ok());
        assert!(matches!(parse_signature(&STANDARD.encode([7u8; 63])), Err(UpdateError::BadSignatureFile)));
        assert!(matches!(parse_signature("not base64!"), Err(UpdateError::BadSignatureFile)));
        assert!(parse_public_key(&STANDARD.encode([1u8; 32])).is_some());
        assert!(parse_public_key(&STANDARD.encode([1u8; 31])).is_none());
    }
}
