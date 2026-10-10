//! Release signing for over-the-air updates (docs/updates.md).
//!
//! - `cargo xtask keygen <secret-file>` creates an Ed25519 key pair. The secret (a 32-byte seed, base64) goes to
//!   the file, which must not exist yet and must never be committed; the public key is printed for the update
//!   crate's `UPDATE_PUBLIC_KEY`.
//! - `cargo xtask sign <archive>…` signs release archives with the seed from the `KANAVOS_UPDATE_SIGNING_KEY`
//!   environment variable, writing `<archive>.sig` (base64 of the 64-byte signature over the raw archive bytes)
//!   and `<archive>.sha256` (`sha256sum` format).
//! - `cargo xtask verify <archive> <public-key>` checks both files, as an installed copy of Kanavos does.

use std::fs;
use std::path::Path;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding.
pub fn b64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk.first().copied().unwrap_or(0),
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let idx = [(n >> 18) & 63, (n >> 12) & 63, (n >> 6) & 63, n & 63];
        for (i, k) in idx.iter().enumerate() {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET.get(*k as usize).copied().unwrap_or(b'A')));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Decodes standard base64 (padding optional, whitespace ignored).
pub fn b64_decode(text: &str) -> Result<Vec<u8>, String> {
    let mut bits: u32 = 0;
    let mut n = 0;
    let mut out = Vec::new();
    for c in text.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=') {
        let v = ALPHABET
            .iter()
            .position(|a| *a == c)
            .ok_or_else(|| format!("invalid base64 character {:?}", char::from(c)))?;
        bits = (bits << 6) | v as u32;
        n += 6;
        if n >= 8 {
            n -= 8;
            out.push(((bits >> n) & 0xff) as u8);
        }
    }
    Ok(out)
}

fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect()
}

fn signing_key_from_b64(text: &str) -> Result<SigningKey, String> {
    let seed: [u8; 32] =
        b64_decode(text.trim())?.try_into().map_err(|_| "the signing key must be 32 bytes".to_owned())?;
    Ok(SigningKey::from_bytes(&seed))
}

fn verifying_key_from_b64(text: &str) -> Result<VerifyingKey, String> {
    let bytes: [u8; 32] =
        b64_decode(text.trim())?.try_into().map_err(|_| "the public key must be 32 bytes".to_owned())?;
    VerifyingKey::from_bytes(&bytes).map_err(|e| format!("invalid public key: {e}"))
}

/// `cargo xtask keygen <secret-file>`.
pub fn keygen(secret_file: Option<&str>) -> Result<(), String> {
    let path = secret_file.ok_or("usage: cargo xtask keygen <secret-file>")?;
    let path = Path::new(path);
    if path.exists() {
        return Err(format!("{} exists; refusing to overwrite a signing key", path.display()));
    }
    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed).map_err(|e| format!("no system randomness: {e}"))?;
    let key = SigningKey::from_bytes(&seed);
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|e| format!("creating {}: {e}", parent.display()))?;
    }
    fs::write(path, format!("{}\n", b64_encode(&seed)))
        .map_err(|e| format!("writing {}: {e}", path.display()))?;
    println!("secret key written to {} (keep it safe; never commit it)", path.display());
    println!("public key: {}", b64_encode(key.verifying_key().as_bytes()));
    Ok(())
}

/// `cargo xtask sign <archive>…`.
pub fn sign(files: &[String]) -> Result<(), String> {
    if files.is_empty() {
        return Err("usage: cargo xtask sign <archive>…".into());
    }
    let secret = std::env::var("KANAVOS_UPDATE_SIGNING_KEY")
        .map_err(|_| "KANAVOS_UPDATE_SIGNING_KEY is not set".to_owned())?;
    let key = signing_key_from_b64(&secret)?;
    for f in files {
        let path = Path::new(f);
        let data = fs::read(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| format!("bad file name {}", path.display()))?;
        let signature = key.sign(&data);
        fs::write(format!("{f}.sig"), format!("{}\n", b64_encode(&signature.to_bytes())))
            .map_err(|e| format!("writing {f}.sig: {e}"))?;
        fs::write(format!("{f}.sha256"), format!("{}  {name}\n", hex(&Sha256::digest(&data))))
            .map_err(|e| format!("writing {f}.sha256: {e}"))?;
        println!("signed {name}");
    }
    println!("public key: {}", b64_encode(key.verifying_key().as_bytes()));
    Ok(())
}

/// `cargo xtask verify <archive> <public-key>`.
pub fn verify(file: Option<&str>, public_key: Option<&str>) -> Result<(), String> {
    let (Some(f), Some(pk)) = (file, public_key) else {
        return Err("usage: cargo xtask verify <archive> <public-key>".into());
    };
    let data = fs::read(f).map_err(|e| format!("reading {f}: {e}"))?;
    let sums = fs::read_to_string(format!("{f}.sha256")).map_err(|e| format!("reading {f}.sha256: {e}"))?;
    let expected = sums.split_whitespace().next().unwrap_or_default();
    if expected != hex(&Sha256::digest(&data)) {
        return Err(format!("{f}: SHA-256 does not match"));
    }
    let sig_text = fs::read_to_string(format!("{f}.sig")).map_err(|e| format!("reading {f}.sig: {e}"))?;
    let sig_bytes: [u8; 64] =
        b64_decode(&sig_text)?.try_into().map_err(|_| "the signature must be 64 bytes".to_owned())?;
    verifying_key_from_b64(pk)?
        .verify(&data, &Signature::from_bytes(&sig_bytes))
        .map_err(|_| format!("{f}: the signature does not match the public key"))?;
    println!("{f}: checksum and signature OK");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips() {
        for data in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"] {
            assert_eq!(b64_decode(&b64_encode(data)).unwrap(), data);
        }
        assert_eq!(b64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(b64_encode(b"fo"), "Zm8=");
    }

    #[test]
    fn signatures_verify_and_tampering_is_detected() {
        let dir = std::env::temp_dir().join(format!("kanavos-xtask-sign-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("pkg.zip");
        fs::write(&file, b"archive bytes").unwrap();
        let seed = [7u8; 32];
        let key = SigningKey::from_bytes(&seed);
        let f = file.to_string_lossy().into_owned();
        let sig = key.sign(b"archive bytes");
        fs::write(format!("{f}.sig"), b64_encode(&sig.to_bytes())).unwrap();
        fs::write(format!("{f}.sha256"), format!("{}  pkg.zip\n", hex(&Sha256::digest(b"archive bytes"))))
            .unwrap();
        let pk = b64_encode(key.verifying_key().as_bytes());
        verify(Some(&f), Some(&pk)).unwrap();
        fs::write(&file, b"archive bytez").unwrap();
        assert!(verify(Some(&f), Some(&pk)).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
