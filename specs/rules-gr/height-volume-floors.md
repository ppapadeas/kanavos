---
id: SPEC-rules-gr-003
title: Height, volume, floors, basements and pilotis
status: approved
author: Pierros Papadeas
reviewer: Pierros Papadeas
sources:
  - ν. 4067/2012 (ΝΟΚ) άρθρο 2 αρ. 54, 83, 87, 90, 91 [ΚΧΠ ν. 5306/2026 άρθρο 197]
  - ΝΟΚ άρθρο 13 (συντελεστής όγκου) [ΚΧΠ 208]
  - ΝΟΚ άρθρο 15 παρ. 1, 3, 4 (ύψος), as amended by ν. 4759/2020 άρθρο 106 [ΚΧΠ 210]
  - ΝΟΚ άρθρο 19 παρ. 2 (κατασκευές στο δώμα), as amended by ν. 4759/2020 άρθρο 110 [ΚΧΠ 214]
  - ΝΟΚ άρθρο 11 παρ. 6 ιζ' (πυλωτή) [ΚΧΠ 206 §6 ιστ']
---

# Height, volume, floors, basements and pilotis

## Purpose

Check the vertical limits of the terms and the code while designing: height, roof, rooftop structures, volume,
number of floors, and whether basements and pilotis qualify as such.

## Behaviour

### Height

- **Reference ground.** The model holds one ground level at the building (`site.ground_level`). It is taken as
  the finished ground (οριστική στάθμη, ΝΟΚ 2 αρ. 54) from which the law measures, at every point of every
  façade (ΝΟΚ 15 §3). The finished ground may differ from natural ground by at most ±1.50 m (§4); this is the
  designer's input, not checked.
- **Top.** Height runs to the top of the structural slab of the last storey, without insulation and finishes
  (ΝΟΚ 2 αρ. 90 [ΚΧΠ 197 αρ. 89]): the highest top of the core layer of flat roofs and slabs on above-ground
  storeys (layers above the first core layer are left out); for pitched roofs, the lowest bearing point
  (eaves top − roof thickness). Walls and columns set the top only when there is no roof or slab.
- **Allowed** = the terms' maximum height; when the terms give none (zero), the table of ΝΟΚ 15 §1 by Σ.Δ.:
  ≤ 0.4 → 10.75 m; ≤ 0.8 → 14.00; ≤ 1.2 → 17.25; ≤ 1.6 → 19.50; ≤ 2.0 → 22.75; ≤ 2.6 → 26.00; above:
  10 × Σ.Δ., at most 32.00 m.
- **Roof.** A pitched roof may rise at most 2.00 m above the allowed height (ΝΟΚ 2 αρ. 91 [ΚΧΠ 197 αρ. 90]).
  The ridge is the highest point of the lowest roof plane, sampled at the outline's vertices, edge midpoints and
  centroid.
- **Rooftop structures.** Walls or columns rising more than 3.40 m above the top slab (4.20 m when the building
  has a lift shaft) raise a warning (ΝΟΚ 19 §2 [ΚΧΠ 214 §2]). Parapets have no height limit in the code.

### Volume

- Volume above the finished ground (ΝΟΚ 13 [ΚΧΠ 208]): for each above-ground storey, (enclosed gross area +
  semi-open area) × floor-to-floor height; the lowest above-ground storey adds its gross area × the plinth
  between ground and floor; basements count only above the ground; pitched-roof space is added as outline area ×
  rise / 2 (exact for gable and mono-pitch roofs, an overestimate for hips).
- Allowed volume = plot area × Σ.Ο., where Σ.Ο. is the terms' value or, by default, 5 × Σ.Δ. (5.5 × Σ.Δ. where
  the maximum height is at most 8.50 m).

### Floors

- The code sets no number of floors; the limit comes from the local terms (`terms.max_floors`).
- Floors = above-ground storeys with enclosed area, except storeys holding only attic spaces.

### Basements

- A storey marked below ground is a basement only if its ceiling (storey top) is at most **1.20 m** above the
  ground (ΝΟΚ 2 αρ. 87; 0.80 m outside town plans, a parameter); otherwise a warning is raised.

### Pilotis

- A pilotis must cover at least half the coverage and have its floor within ±0.50 m of the ground (ΝΟΚ 11 §6
  ιζ'); otherwise a warning is raised.

## Edge cases

- Several roofs: the highest top and the highest ridge are used.
- No above-ground elements: height 0.

## Acceptance tests

1. Given a flat roof with base 3.20 m on the storey at +3.20 with the starter's inverted-roof build-up and the
   ground at −0.30, then H = 6.40 − 0.195 + 0.30 = 6.505 m.
2. Given a 30° gable over 10 × 8 m with eaves at 6.20 m, then the ridge is at 8.51 m; at 70° the ridge breaks the
   2 m allowance.
3. Given a basement whose ceiling is 1.50 m above the ground, then a warning is raised.
4. Given two 80 m² storeys of 3.20 m and a 0.30 m plinth, then the volume is 536 m³.

## Open questions

- Grid of measurement points along façades on sloping ground (single ground level for now).
- The ideal solid (ιδεατό στερεό, ΝΟΚ 15 §5) is not checked.

---

## Review (completed by the reviewer)

- [x] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [x] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [x] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [x] No screenshots or images of the reference product.
- [x] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [x] Describes behaviour, not implementation details of the reference product.

Reviewed by: Pierros Papadeas, 2026-10-10 — Result: approved

Note: the reviewer is also the author of record. Clean-room protocol §2 asks for a second reviewer; add one when a second contributor joins, before the first public release.
