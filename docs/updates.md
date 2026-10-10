# Updates

Kanavos can update itself from its GitHub releases: it looks for a newer version, downloads the package for your
operating system, checks that the package is the one the Kanavos maintainers signed, replaces its own files and
starts again with your project open. The code is in `crates/update` (`kanavos-update`); the behaviour is specified
in [`specs/updates/over-the-air.md`](../specs/updates/over-the-air.md).

## When Kanavos looks for a new version

- **At start**, when *Help › Check for updates at start* is on (the default). The check runs in the background; if
  there is nothing new, or the computer is offline, nothing is shown.
- **When you ask**: *Help › Check for updates…* (or the button in *Help › About Kanavos*) always shows the result:
  the new version, "You have the latest version", or what went wrong.
- `kanavos-cli update` (below).

Nothing else contacts the network. The visual-verification hook (`KANAVOS_SCREENSHOT`) never checks.

## What is sent

One HTTPS request to `https://api.github.com/repos/ppapadeas/kanavos/releases?per_page=20`, with these headers:

| Header | Value |
| --- | --- |
| `User-Agent` | `Kanavos/<version> (<os>; <arch>)`, for example `Kanavos/0.2.0 (windows; x86_64)` |
| `Accept` | `application/vnd.github+json` |
| `X-GitHub-Api-Version` | `2022-11-28` |

No project data, file names, paths, user names, identifiers or cookies are sent. As with any connection, GitHub sees
the IP address the request comes from; GitHub's own privacy statement covers what it does with it. Downloading an
update fetches three files from the release (`github.com`, which redirects to GitHub's file storage) with the same
`User-Agent`.

This is not telemetry: nothing about how you use Kanavos is reported. Telemetry stays off (see
[`ux.md`](ux.md) §8).

## Turning it off

- *Help › Check for updates at start*, or the same box in *Help › About Kanavos*. A manual check still works.
- The environment variable `KANAVOS_NO_UPDATE_CHECK=1` turns off every check and download, manual ones and
  `kanavos-cli update` included. Use it for managed or offline computers and for tests.
- In the preferences file (`%APPDATA%\Kanavos\prefs.json` on Windows, `~/.config/kanavos/prefs.json` on Linux):
  `"update_check": false`. The same file holds `"update_channel"` and `"skipped_version"`.

## Channels and skipping a version

- **Stable releases** (default): published releases only.
- **Test builds**: also releases marked as pre-releases on GitHub (tags like `v0.3.0-pre.2`).

Choose the channel in *Help › Update channel* or in the About window. Versions are compared by semantic-version
precedence (semver.org §11), so `0.3.0-pre.10` is newer than `0.3.0-pre.2`, and `0.3.0` is newer than both.

When a version is offered you can install it, leave it for later (the notice stays in the status bar), or skip it:
a skipped version is not offered again by the check at start, but a newer one is, and a manual check still shows it.

## Installing

1. *Install and restart* first checks that this copy can update itself: its folder must hold `QUICKSTART.txt`
   (every release package does) and must be writable by your account.
2. The package, its `.sha256` file and its `.sig` file are downloaded to a temporary folder, with a progress bar and
   a Cancel button. The package is accepted only if its SHA-256 matches and its signature verifies (below).
3. If the project has unsaved changes, Kanavos asks: *Save*, *Don't save* or *Cancel*. In every case it then writes
   a recovery copy of the project, so nothing is lost if anything goes wrong; with *Don't save* the new version
   offers that copy when it starts.
4. The files are replaced (below) and Kanavos starts the new version with the open project file, then closes.

### How files are replaced

- The package is unpacked into the temporary folder. Entries with absolute paths, `..` components, drive or
  stream names, links or special files are refused, and so is any entry outside the package's top-level folder
  `kanavos-<version>-<os>-<arch>/`. That folder name must match the version on offer, so an older signed package
  cannot be passed off as a newer release.
- Each file of the package is copied next to its destination as `<file>.new`. Then each current file is renamed
  to `<file>.old` and the new one is renamed into place. Windows allows a running program to be renamed, not
  replaced, which is why `kanavos.exe` becomes `kanavos.exe.old` while it is still running.
- A journal (`.kanavos-update.json`) in the installation folder records each step, and a lock file
  (`.kanavos-update.lock`) keeps two processes from updating the same folder at once.
- If a rename fails, the files already replaced are put back and the previous version keeps working. If the
  computer stops half way, the next start finishes the installation from the `.new` copies.
- The `.old` files are deleted at the next start (a program that is still running keeps its `.old` file until
  it has closed).

Only files that are in the package are replaced. Your own files in the folder are never touched, and no project
file (`*.kanavos`) is replaced except the bundled `sample-house.kanavos`. Only the `.old` and `.new` files listed in
the journal are ever deleted.

### When the folder cannot be written

If Kanavos is unpacked into a folder your account cannot write, such as `C:\Program Files`, it cannot update
itself and says so. Move the Kanavos folder to a place you own (for example your user folder) or download the new
version by hand from the release page. A copy that was not installed from a release package (for example one built
from source and run from `target/`) does not update itself either.

## Command line

```
kanavos-cli update [--check] [--channel stable|test]
```

- `--check` only reports whether a newer version exists and prints its notes.
- Without `--check`, the newer version is downloaded, verified and installed into the folder of `kanavos-cli`, in
  the same way as above (both programs, the guides and the licences). Kanavos programs that are running keep the
  previous version until they start again.
- Exit codes: `0` installed or already the latest version, `1` failure, `2` usage error, `3` updates are not
  configured in this build or are turned off by `KANAVOS_NO_UPDATE_CHECK`.

## Signing scheme

Every release carries, for each package:

| File | Content |
| --- | --- |
| `kanavos-<version>-windows-x86_64.zip`, `kanavos-<version>-linux-x86_64.tar.gz` | The package: one top-level folder `kanavos-<version>-<os>-<arch>/` |
| `<package>.sha256` | Lowercase hex SHA-256 of the package, two spaces, the file name (the `sha256sum` format) |
| `<package>.sig` | One line: standard base64 of the 64-byte Ed25519 signature (RFC 8032) over the raw package bytes |

The Ed25519 public key is built into Kanavos as `UPDATE_PUBLIC_KEY` in `crates/update/src/lib.rs` (standard
base64 of the 32-byte key):

```text
4dyfPC0DrHFPxgVW7w4jSlebT07z72cUGZt1JFGxdVY=
```

The private key never enters the repository. A build whose key is the placeholder refuses to check for or install
updates and reports "Updates are not configured in this build".

The checksum guards against damaged downloads; the signature guards against altered ones, because only the holder
of the private key can produce it. Both must pass. TLS (rustls with the operating system's root certificates)
protects the connection, but trust in the package rests on the signature alone.

### Verifying a download by hand

Linux or macOS, in the folder with the three files:

```sh
sha256sum -c kanavos-0.2.0-linux-x86_64.tar.gz.sha256          # macOS: shasum -a 256 -c …
```

Windows (PowerShell): compare the output with the first field of the `.sha256` file:

```powershell
(Get-FileHash .\kanavos-0.2.0-windows-x86_64.zip -Algorithm SHA256).Hash.ToLower()
```

The signature, with OpenSSL 3 (it ships with Git for Windows). `KEY` is the public key above; the fixed bytes in
the second line turn the raw key into the standard public-key format OpenSSL reads:

```sh
echo "KEY" | base64 -d > key.raw
(printf '\060\052\060\005\006\003\053\145\160\003\041\000'; cat key.raw) > key.der
openssl pkey -pubin -inform DER -in key.der -out key.pem
base64 -d kanavos-0.2.0-linux-x86_64.tar.gz.sig > sig.bin
openssl pkeyutl -verify -pubin -inkey key.pem -rawin -in kanavos-0.2.0-linux-x86_64.tar.gz -sigfile sig.bin
```

The last command prints `Signature Verified Successfully` for a genuine package and `Signature Verification
Failure` otherwise.

### Producing the key and the signatures

`cargo xtask keygen` creates the key pair and `cargo xtask sign` writes the `.sha256` and `.sig` files; the release
workflow signs every package this way. [releasing.md](releasing.md) covers the key, the repository secret and
tagging. `cargo xtask verify <package> <public key>` repeats the checks above.

Changing the key later means installed copies can no longer verify new packages: release one version signed with
the old key that carries the new key, and only then sign with the new one.
