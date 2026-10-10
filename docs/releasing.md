# Releasing

How a version of Kanavos reaches the people who use it. Installed copies find new versions on the project's GitHub
releases and update themselves ([updates.md](updates.md)); this page covers the publishing side.

## What a release contains

The release workflow (`.github/workflows/release.yml`) runs when a tag `v<version>` is pushed. It builds on Windows
and Linux, packages with `cargo xtask dist`, signs and publishes these assets:

| Asset | Content |
| --- | --- |
| `kanavos-<version>-windows-x86_64.zip` | Desktop app, `kanavos-cli`, sample project, guides, licences |
| `kanavos-<version>-linux-x86_64.tar.gz` | The same for Linux (glibc 2.35 or newer) |
| `<archive>.sha256` | SHA-256 of the archive, in `sha256sum` format |
| `<archive>.sig` | Ed25519 signature over the archive bytes, base64 |

The release notes are the version's section of [`CHANGELOG.md`](../CHANGELOG.md). A tag with a pre-release suffix
(`v0.2.0-test.1`) is published as a GitHub pre-release: only copies set to the test-build update channel offer it.

## Signing key (once)

Updates are accepted only if their signature matches the public key built into Kanavos (`UPDATE_PUBLIC_KEY` in
`crates/update`). The matching secret key signs every release.

1. Create the key pair, with the secret file outside the repository:

   ```sh
   cargo xtask keygen ~/.kanavos/update-signing-key.txt
   ```

   The command prints the public key. It refuses to overwrite an existing file.
2. Put the printed public key in `UPDATE_PUBLIC_KEY` and commit it.
3. Store the secret as the repository secret `KANAVOS_UPDATE_SIGNING_KEY`:

   ```sh
   gh secret set KANAVOS_UPDATE_SIGNING_KEY < ~/.kanavos/update-signing-key.txt
   ```

4. Keep an offline backup of the secret file (a password manager or an encrypted drive). Without it, installed
   copies can no longer be updated in place.

Never commit the secret, paste it into an issue or print it in a workflow log.

## Each release

1. Write the version's section in `CHANGELOG.md` (`## 0.2.0`): what changed for people using Kanavos, and known
   limits.
2. Set `version` under `[workspace.package]` in `Cargo.toml` and refresh the lock file (`cargo metadata` does it).
3. Run `cargo xtask ci` and `cargo deny check licenses bans sources`; merge to `main`.
4. Tag and push:

   ```sh
   git tag -a v0.2.0 -m "Kanavos 0.2.0"
   git push origin v0.2.0
   ```

   The workflow stops if the tag does not equal the workspace version or the signing secret is missing.
5. When the workflow finishes, check one package against the public key:

   ```sh
   gh release download v0.2.0 --pattern '*windows*'
   cargo xtask verify kanavos-0.2.0-windows-x86_64.zip <public key>
   ```

A broken release is withdrawn by deleting the GitHub release (the tag can stay); installed copies then stop offering
it. Publish a fixed version with a higher number rather than replacing assets of a published one.

## Changing the key

Installed copies trust only the key they were built with. To move to a new key, publish a release signed with the
old key that carries the new public key; releases after it are signed with the new key. If the old secret is lost
or leaked, that path is gone: publish a release with the new key and ask everyone to install it by hand.

## Not yet done

- The Windows executables are not Authenticode-signed, so SmartScreen warns on first launch. A code-signing
  certificate removes the warning.
- No macOS package.
