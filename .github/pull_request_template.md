## What and why

<!-- One paragraph. Link the spec in specs/ for feature work. -->

## Clean-room checklist

- [ ] Every commit is signed off (`git commit -s`) and has a `Clean-Room:` trailer.
- [ ] Feature behaviour comes only from an approved spec, open standards, regulations or published research.
- [ ] No text, icons, layouts, objects or templates copied or paraphrased from any commercial BIM product.
- [ ] No vendor names for tools, panels or settings in code, UI strings or docs.
- [ ] New dependencies are on the licence allow-list in `deny.toml` and justified.

## Checks

- [ ] `cargo xtask ci` passes locally.
- [ ] User-facing strings added in both `el` and `en`.
- [ ] Tests added or updated.
