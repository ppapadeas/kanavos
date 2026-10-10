# ADR-0008: Over-the-air updates replace the package in place, trusting only a release signature

- Status: accepted
- Date: 2026-10-10

## Context

Testers run Kanavos from an unpacked release archive. They should get new versions without unpacking archives by
hand, without risk to their projects, and without Kanavos reporting anything about them. Releases are published on
GitHub with a package, a `.sha256` file and an Ed25519 `.sig` file per platform (`docs/updates.md`). The licence
policy (`deny.toml`) rules out `webpki-roots` (CDLA-Permissive-2.0) and `aws-lc-sys` (OpenSSL licence).

## Decision

- **Trust**: a package is installed only when its SHA-256 matches and its Ed25519 signature verifies with the public
  key built into the program. TLS protects the connection but is not what the decision rests on. The top-level
  folder inside the signed archive names the version, which binds a package to its release.
- **Transport**: `ureq` (small, blocking, MIT OR Apache-2.0) with rustls and the *ring* provider; root certificates
  come from the operating system through `rustls-native-certs`, so no certificate bundle ships and no system TLS
  library is linked. `rustls-platform-verifier` was not chosen because it pulls `webpki-root-certs` for some
  targets; `native-tls` was not chosen because it would link OpenSSL on Linux.
- **Crypto**: *ring* also provides SHA-256 and Ed25519, so one audited library covers TLS and package checks.
- **Installation in place**: new files are staged as `<file>.new`, then each current file is renamed to
  `<file>.old` and the staged one renamed into place, with a journal and a lock in the installation folder.
  Renaming works on a running program on Windows, where replacing it does not; renames within one folder are
  atomic. A failed rename rolls back; an interrupted installation is completed at the next start from the staged
  copies. No separate updater program is needed.
- **Scope**: only folders that hold `QUICKSTART.txt` (a release package) and are writable are updated. Installations
  managed elsewhere (system folders, build folders) are refused with an explanation.

## Consequences

- One new layer-8 crate, `kanavos-update`; the desktop interface and the command line both use it.
- A lost or leaked signing key is serious: rotation needs a release signed with the old key that carries the new one.
- `.old` files of a running program remain until a later start.
- Installations under Program Files cannot update themselves; an installer with elevation would be a separate
  decision.
