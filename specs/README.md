# Specs

Behaviour specifications written under the [clean-room protocol](../docs/legal/clean-room.md). Implementers build
only from **approved** specs here, open standards, and published research.

```
specs/
  observations/          observation logs (OBS-…), one file per session
  <area>/<behaviour>.md  specs (SPEC-…), from docs/legal/templates/spec.md
```

A spec is usable only when its front matter says `status: approved` and its review block is complete.
