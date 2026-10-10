---
id: SPEC-drawing-003
title: Text fonts and the built-in single-stroke lettering
status: approved
author: Pierros Papadeas
reviewer: Pierros Papadeas
sources:
  - ISO 3098-1 / ISO 3098-2 (lettering for technical drawings: proportions, pen width h/10)
  - OpenType specification (OS/2 sCapHeight, hmtx advance widths, glyf/CFF outlines)
  - Unicode Standard, Greek and Coptic block (U+0370–U+03FF)
---

# Text fonts and the built-in single-stroke lettering

## Purpose

Drawings carry Greek text (room names, titles, notes). Every export must show it correctly on any machine, without
Kanavos distributing font files whose licences it cannot control.

## Behaviour

**Text size.** A text's height is its **cap height** in paper millimetres (2.5 mm is a typical annotation).

**Font choice at run time.** Kanavos looks for an installed sans-serif font that contains Latin, digits and Greek
with tonos (it checks `Aa0ΑΩαωάΆΐ`), in this order:

1. the file named by the environment variable `KANAVOS_FONT` (bold: `KANAVOS_FONT_BOLD`);
2. Windows: `segoeui`, `arial`, `tahoma`, `calibri`, `verdana` in the system and user font folders;
3. Linux: DejaVu Sans, Noto Sans, Liberation Sans, FreeSans in the usual font folders;
4. macOS: Arial, Tahoma, Verdana, Helvetica, Geneva in the system and library font folders.

Each regular file is paired with its bold companion when present. With no bold face, bold text is the regular
outline stroked by 6 % of the cap height. Font files are read, never copied.

**Fallback.** When no font is found, and for any character the chosen font lacks, text is drawn with the built-in
single-stroke font. Output then never fails and never drops characters silently: a character the stroke font also
lacks is drawn as an empty box one cap height tall.

**The stroke font** is an original design on a grid where capitals are 6 units tall, lowercase x-height is 4 and
descenders reach 2 below the baseline. Capitals are about 4 units wide; digits are centred in equal 3.4-unit cells so
columns of figures align; every glyph is followed by a 1.4-unit gap. The pen width is cap height / 10 (bold: / 7),
drawn with round caps and joins. Coverage:

- digits, Latin A–Z and a–z, basic punctuation and symbols (`.,:;!?-+=/\()[]{}'"%&*#_<>|^~$@`);
- Greek capitals Α–Ω and lowercase α–ω including final ς;
- Greek tonos on ά έ ή ί ό ύ ώ and Ά Έ Ή Ί Ό Ύ Ώ (capital tonos sits before the letter), dialytika on ϊ ϋ Ϊ Ϋ,
  dialytika with tonos on ΐ ΰ, and the separate marks ΄ ΅ ¨;
- drawing symbols: ° ² ³ ¹ ± × ÷ · – — … € Ø ø ⌀ ≤ ≥ ≈ ≠ ← ↑ → ↓;
- common Latin accented letters (acute, grave, circumflex, diaeresis, tilde, cedilla).

Greek capitals that share a shape with Latin capitals (Α Β Ε Ζ Η Ι Κ Μ Ν Ο Ρ Τ Υ Χ) use the same strokes.

## Edge cases

- Tabs and non-breaking spaces are drawn as spaces; other control characters are skipped.
- A non-positive or non-finite height gives no visible text.

## Acceptance tests

1. Given no installed font, when "Κάτοψη ισογείου 1:50" is laid out at 2.5 mm, then strokes are produced, the pen is
   0.25 mm, and the layout is identical on every run.
2. Every character in the coverage list has a glyph; every glyph starts at x = 0 and stays between 2 units below the
   baseline and 7.5 units above it.
3. All digits have the same advance.
4. Given an installed font, "H" at 10 mm reaches a height of 10 ± 0.3 mm.

## Open questions

- Should the stroke font gain kerning pairs (e.g. "AV", "Τα")?

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
