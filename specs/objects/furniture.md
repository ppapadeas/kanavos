---
id: SPEC-objects-005
title: Furniture, kitchen, bathroom, equipment and outdoor objects
status: in-review
author: Pierros Papadeas
reviewer: pending
sources:
  - SPEC-objects-001 (coordinates, layers, line weights)
  - ISO 128-2:2020 (line types)
  - General architectural drafting practice in Greece and Europe for furniture and fixture plan symbols
  - Common product dimensions (mattress sizes, 0.60 m kitchen module, sanitary ware, appliances, cars)
---

# Furniture, kitchen, bathroom, equipment and outdoor objects

## Purpose

Architects furnish the plans of a house so that rooms read at a glance at 1:50 and 1:100 and the 3D model looks
lived in, without modelling furniture themselves.

## Behaviour

All objects follow SPEC-objects-001: front edge on the insertion line and back against the wall for wall-standing
objects, centred for free-standing ones; outlines 0.18 mm, details 0.13 mm (left out below 1:50).

### Bedroom and storage

| Identifier | Greek name | Size (w × d × h) | Plan symbol |
| --- | --- | --- | --- |
| `furniture.bed_single` | Μονό κρεβάτι | 0.90 × 2.00 × 0.50 | outline, headboard line, pillow, duvet edge with a turned-back corner |
| `furniture.bed_double` | Διπλό κρεβάτι | 1.60 × 2.00 × 0.50 | as above with two pillows (from 1.20 m wide) |
| `furniture.cot` | Βρεφική κούνια | 0.70 × 1.30 × 0.90 | double outline (rails) with bar ticks |
| `furniture.nightstand` | Κομοδίνο | 0.45 × 0.40 × 0.50 | outline, front line, lamp circle |
| `furniture.wardrobe` | Ντουλάπα | 1.20 × 0.60 × 2.40 | outline, door fronts, hanging rail with hangers |
| `furniture.chest_of_drawers` | Συρταριέρα | 1.00 × 0.50 × 0.85 | outline, front line, handle |
| `furniture.bookcase` | Βιβλιοθήκη | 1.00 × 0.35 × 2.00 | outline, sides and back, bays; diagonal when taller than 1.5 m |

### Living, dining and work

| Identifier | Greek name | Size | Plan symbol |
| --- | --- | --- | --- |
| `furniture.sofa_2`, `furniture.sofa` | Διθέσιος / Τριθέσιος καναπές | 1.70 / 2.20 × 0.90 × 0.80 | rounded outline, backrest and arm lines, one cushion per seat |
| `furniture.sofa_corner` | Γωνιακός καναπές | 2.60 × 2.00 | L outline with backrests on the back and left sides, arms at both free ends, cushions |
| `furniture.armchair` | Πολυθρόνα | 0.85 × 0.85 | one-seat sofa |
| `furniture.coffee_table` | Τραπεζάκι σαλονιού | 1.10 × 0.60 × 0.42 | rounded outline, inner line |
| `furniture.table` | Τραπέζι φαγητού με καρέκλες | 1.60 × 0.90 × 0.75 | table with `chairs` (6) on the long sides first, then the ends |
| `furniture.table_round` | Στρογγυλό τραπέζι με καρέκλες | Ø 1.10 | circle (ellipse if w ≠ d) with `chairs` (4) evenly around, facing the centre |
| `furniture.chair` | Καρέκλα | 0.45 × 0.50 × 0.90 | seat with backrest band at the back |
| `furniture.desk` | Γραφείο | 1.40 × 0.70 × 0.75 | outline, screen and keyboard |
| `furniture.office_chair` | Καρέκλα γραφείου | 0.62 × 0.62 × 1.00 | five-star base beyond the seat, seat, backrest, arms |
| `furniture.tv_unit` | Έπιπλο τηλεόρασης | 1.80 × 0.45 × 0.50 | outline, front line, screen on its stand |
| `furniture.garden_table` | Τραπέζι κήπου με ομπρέλα | 1.40 × 0.80 × 0.74 | slatted top, `chairs` (4), parasol Ø 2.50 dashed when `parasol` is on |

Chairs stand 4 cm clear of the table edge, sitter facing the table.

### Kitchen

| Identifier | Greek name | Size | Plan symbol |
| --- | --- | --- | --- |
| `kitchen.counter` | Ντουλάπια βάσης με πάγκο εργασίας | length 2.40 × 0.60 × 0.90 | worktop outline, upstand line; 3D doors per `module` (0.60) |
| `kitchen.wall_cabinets` | Ντουλάπια τοίχου | 2.40 × 0.35 × 0.70, at `mount` 1.45 | dashed outline and module divisions |
| `kitchen.sink`, `kitchen.sink_double` | Νεροχύτης (με δύο γούρνες) | 0.80 / 1.20 × 0.60 | worktop, steel plate, `bowls` (the second smaller), drainer ribs, tap with spout |
| `kitchen.hob` | Εστίες μαγειρέματος | 0.60 × 0.60, top at 0.90 | plate with `burners` (4) as circles with inner rings |
| `kitchen.cooker` | Κουζίνα με φούρνο | 0.60 × 0.60 × 0.85 | outline, front line, plate with burners |
| `kitchen.oven` | Φούρνος σε ντουλάπι-στήλη | 0.60 × 0.60 × 2.10 | outline, front line, crossed diagonals (tall unit) |
| `kitchen.fridge` | Ψυγειοκαταψύκτης | 0.60 × 0.65 × 1.85 | outline, door line, one diagonal, handle |
| `kitchen.dishwasher` | Πλυντήριο πιάτων | 0.60 × 0.60 × 0.90 | worktop outline, appliance under it short-dashed with diagonals |
| `kitchen.island` | Νησίδα κουζίνας | 1.80 × 0.90 × 0.90 | worktop, cabinet line under the `overhang` short-dashed, `stools` (3) |

### Bathroom and equipment

| Identifier | Greek name | Size | Plan symbol |
| --- | --- | --- | --- |
| `sanitary.wc` | Λεκάνη WC δαπέδου με καζανάκι | 0.38 × 0.68, cistern top 0.80 | cistern, egg-shaped bowl up to the cistern, seat opening |
| `sanitary.wc_wall` | Κρεμαστή λεκάνη WC | 0.36 × 0.54, seat 0.40 | bowl, mounting block; flush plate on the wall in 3D |
| `sanitary.bidet` | Μπιντές | 0.36 × 0.54 × 0.40 | bowl, basin, tap |
| `sanitary.basin` | Νιπτήρας | 0.60 × 0.45, rim 0.85 | rounded front, bowl ellipse, tap, waste; optional pedestal |
| `sanitary.vanity` | Νιπτήρας με έπιπλο | 0.80 × 0.50 × 0.85 | cabinet top with bowl and tap |
| `sanitary.bath` | Μπανιέρα | 1.70 × 0.75 × 0.55 | outline, rounded inner basin, waste |
| `sanitary.shower` | Ντουζιέρα | 0.90 × 0.90 × 0.05 | outline, inner rim, diagonals, waste |
| `sanitary.shower_screen` | Διαχωριστικό ντουζιέρας | 0.90 long × 1.95 | wall profile, fixed glass, hinged door opening outwards with arc |
| `equipment.washing_machine` | Πλυντήριο ρούχων | 0.60 × 0.60 × 0.85 | outline, front line, drum door circles |
| `equipment.water_heater` | Ηλεκτρικός θερμοσίφωνας | Ø 0.45 × 0.80 at `mount` 1.40 | dashed circle against the wall with its brackets |

### Outdoor

| Identifier | Greek name | Size | Plan symbol |
| --- | --- | --- | --- |
| `vegetation.tree` | Δέντρο | crown 4.00 × 4.00, 6.00 high, `trunk` 2.00 | lobed organic crown outline, inner crown, trunk circle, branches |
| `vegetation.shrub` | Θάμνος | 1.20 × 1.20 × 1.00 | finer lobed outline, inner outline |
| `vehicle.car` | Αυτοκίνητο | 1.80 × 4.50 × 1.50 | rounded body, windscreen, roof and rear window, mirrors |

### 3D

Simple solids that read clearly: legs, carcasses, fronts with handles, cushions, pillows and duvet; worktops with
real cut-outs for sinks; hollow bowls for basins, baths and trays; appliances white with black glass; trees as a
trunk and a cluster of foliage masses; the car as a body, tinted glasshouse, roof and wheels.

## Edge cases

- Chairs that do not fit along a table are dropped (at most the long sides plus one at each end).
- Stools that do not fit along an island are dropped (one per 0.42 m).
- Sizes outside an object's range are limited to it.

## Acceptance tests

1. Given every object, then its symbol is on `object.symbol*`, its parts are closed, and they lie within its plan
   footprint, which contains its nominal size.
2. Given the dining table with 0 chairs, then its symbol has fewer items than with 6.
3. Given wall cabinets, a water heater or a garden table, then the parts above the cut plane are dashed on
   `object.symbol.above`.

## Open questions

- Kitchen corner units and tall pantry units.
- Accessible WC set (grab rails, turning circle).

---

## Review (completed by the reviewer)

- [ ] No sentence is quoted or paraphrased from any vendor manual, help page or UI.
- [ ] No vendor names for tools, settings, panels or dialogs; Kanavos vocabulary only.
- [ ] Structure follows the behaviour, not a vendor manual's chapter or dialog layout.
- [ ] No screenshots or images of the reference product.
- [ ] Every claim traces to a listed source (observation log, standard, paper, regulation).
- [ ] Describes behaviour, not implementation details of the reference product.

Reviewed by: pending
