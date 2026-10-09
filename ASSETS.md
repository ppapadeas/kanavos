# Bundled assets

Every font, icon, image, object, material, template and sample file shipped with Kanavos is listed here with its
author, source and licence. Nothing may be bundled without a row. Vendor library objects, templates and default
attribute sets are never bundled (see [clean-room protocol](docs/legal/clean-room.md) §4.7).

| Asset | Path | Author | Source | Licence |
| --- | --- | --- | --- | --- |
| Single-stroke lettering font (glyph definitions) | `crates/fonts/src/stroke.rs` | The Kanavos contributors | Original design, see `specs/drawing/stroke-font.md` | MIT OR Apache‑2.0 |

System fonts (for example on Windows, Linux or macOS) are found and read at run time from the user's machine and
are never copied into the repository or the distribution.
