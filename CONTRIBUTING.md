# Contributing to Domos

Thank you for helping. Domos is built under a strict clean-room process, so please read this whole page once.

## Before your first commit

1. Read the [clean-room protocol](docs/legal/clean-room.md). It is short and binding.
2. Add yourself to [`docs/legal/contributors.md`](docs/legal/contributors.md): your roles, and any past employment
   or NDA with a BIM software vendor.
3. Read [`AGENTS.md`](AGENTS.md) for code rules, layering and quality gates.

## Every commit

- Sign off under the [Developer Certificate of Origin 1.1](https://developercertificate.org): `git commit -s`.
- Add a `Clean-Room:` trailer (`yes`, `same-person` or `n/a`) and, for feature work, a `Spec:` trailer:

  ```
  walls: resolve T-junctions by layer priority

  Signed-off-by: Your Name <you@example.org>
  Clean-Room: yes
  Spec: specs/walls/junctions.md
  ```

- `cargo xtask ci` passes locally.

## What we will not accept

- Code, text, icons or content copied or paraphrased from any commercial BIM product, its manuals, SDKs or forums.
- Anything derived from decompiling binaries or inspecting proprietary files.
- Dependencies outside the licence allow-list in `deny.toml`.
- Third-party code or text without a recorded source and compatible licence.

## Licence

By contributing you agree that your contribution is licensed under MIT OR Apache‑2.0, as described in
[`README.md`](README.md#licence).
