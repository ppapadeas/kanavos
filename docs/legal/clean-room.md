# Clean-room protocol

Domos is an independent, open-source BIM authoring application. Its workflow will feel familiar to people who
use commercial BIM tools, including GRAPHISOFT® Archicad®. It contains no code, text, artwork, library content or
file-format knowledge taken from any of them.

This document is the rule set that keeps that statement provably true. It is binding for every contributor, for
every change, from the first commit. The legal reasoning behind it is in [`legal-brief.md`](legal-brief.md).

> This protocol is an engineering control, not legal advice. Have it reviewed by a lawyer before the first public
> release (see the open questions at the end of the legal brief).

---

## 1. Why a clean room, if functionality is not protected?

Under EU law, the functionality of a program, its programming language and the format of its data files are not
protected by copyright (CJEU, *SAS Institute v World Programming*, C‑406/10, 2 May 2012). Copying *what* a program
does is lawful. Copying *how it is expressed* — source or object code, manual text, icons, screen artwork, library
objects, templates — is not.

A clean room does two jobs:

1. **It prevents expression from leaking in.** Specifications are written in our own words, from observed
   behaviour, so implementers never see protected expression.
2. **It produces evidence.** If anyone ever claims we copied, the specs, observation logs and commit trailers show
   independent creation, step by step, with dates.

## 2. Roles

| Role | May use the reference product? | May read its manuals? | Writes code? |
| --- | --- | --- | --- |
| **Observer** | Yes, under a valid licence, only to observe, study and test how it behaves | Only to understand behaviour; never quotes, paraphrases or restructures them | No |
| **Spec editor** | No | No | No (writes and reviews specs only) |
| **Implementer** | No | No | Yes |

- One person may hold the Observer and Spec editor roles for a given spec, but **a spec must be reviewed by a
  second person** for expression leakage before an Implementer uses it.
- A person who has been an Observer for a feature area must not implement that same feature area for **90 days**
  after their last observation session in it. When the team is too small for that, the spec must be finalised and
  reviewed before implementation starts, and the commit carries `Clean-Room: same-person` (see §6) so the record is
  honest.
- Everyone records their role in `docs/legal/contributors.md` (name, roles, date of first and last observation
  session per area).

## 3. Allowed sources (the "clean" side)

Implementers may use only:

- Specs in [`specs/`](../../specs) that have passed review.
- Open standards and their public documentation: IFC (ISO 16739‑1), STEP (ISO 10303‑21), DXF (published reference),
  PDF (ISO 32000), SVG, glTF, EPSG geodetic definitions.
- Greek and EU regulations and official guidance (ΝΟΚ ν. 4067/2012, ΚΕΝΑΚ, ΕΛΟΤ standards, e‑Άδειες
  requirements), which are public law.
- Textbooks and academic papers on computational geometry, hidden-line removal, solid modelling and BIM.
- Open-source code under a licence compatible with MIT OR Apache‑2.0, recorded in `deny.toml` and `NOTICE`.
- Their own professional knowledge of architecture and construction.

## 4. Forbidden, always, for everyone

1. **Decompiling, disassembling, debugging or memory-inspecting** any reference product binary or plug-in.
2. **Inspecting proprietary project or library files** at byte level (hex editors, `strings`, diffing saved files
   to infer structure), including native project files, archives, templates and compiled library objects.
3. **Capturing network traffic** of the reference product or its collaboration servers.
4. **Using any vendor SDK, developer kit, add-on source code or API header**, or code derived from them.
5. **Copying text** from manuals, help pages, tooltips, error messages, training material or forums — not even
   paraphrased. Write from the behaviour, never from the page.
6. **Copying visual design**: icons, cursors, toolbars, palette artwork, splash screens, colour schemes, typefaces,
   default pen/line/fill sets, or the overall trade dress.
7. **Bundling or reproducing vendor content**: library objects, templates, default attribute sets, sample projects,
   textures, materials.
8. **Using a vendor's trademarks** in the product name, crate names, binary names, domain names, file extensions or
   marketing, beyond the factual references allowed in [`../legal/legal-brief.md`](legal-brief.md) §5.
9. **Pasting third-party code or text without provenance**, including output from automated code-generation tools,
   unless its source and licence are recorded and compatible.
10. **Using material obtained in breach of an NDA, employment contract or confidentiality duty.** Anyone who has
    worked for, or under NDA with, a BIM vendor declares it in `contributors.md` and does not work on the affected
    areas without a lawyer's sign-off.

## 5. How a feature moves through the clean room

```
Observer                  Spec editor                Reviewer (2nd person)        Implementer
────────                  ───────────                ─────────────────────        ───────────
observe & test     ──▶    write spec in own   ──▶    check: no quotes, no   ──▶   build from spec +
log sessions              words, behaviour only      vendor terms of art,         public standards only
(observations/)           (specs/<area>/*.md)        no screenshots, no           commit with trailers
                                                     structure copied             (§6)
```

### 5.1 Observation sessions

- Run the reference product only under a licence held by the Observer.
- Log every session in `specs/observations/YYYY-MM-DD-<area>-<initials>.md` using the template in
  [`templates/observation-log.md`](templates/observation-log.md): date, product version, licence holder, what was
  tested, inputs used, observed outputs **described in words and numbers**.
- Screenshots of the reference product **never enter the repository**. If needed for memory, keep them privately and
  delete them once the log is written.
- Test inputs are our own: our own sample buildings, our own parameters.

### 5.2 Specs

- One spec per behaviour, using [`templates/spec.md`](templates/spec.md).
- Describe **behaviour and outcomes**: inputs, outputs, rules, edge cases, tolerances. Use our own terminology
  (see the glossary in [`../ux.md`](../ux.md#vocabulary)), not the vendor's names for tools, panels or settings.
- Cite every source: observation log IDs, standards clauses, papers.
- A spec must not mirror the vendor's manual structure (chapter order, setting groupings, dialog layout).

### 5.3 Review

The reviewer ticks the checklist in the spec template. A spec without a completed review block is not usable.

### 5.4 Implementation

- Implementers read only `specs/` (reviewed), public standards and our own code.
- Each commit carries the trailers in §6.

## 6. Commit trailers

Every commit is signed off under the Developer Certificate of Origin (`git commit -s`) and carries a clean-room
trailer:

```
Signed-off-by: Full Name <email>
Clean-Room: yes
Spec: specs/walls/junctions.md
```

Values for `Clean-Room`:

- `yes` — author is an Implementer who has not observed the reference product in this area within 90 days.
- `same-person` — author also observed this area; spec was finalised and reviewed before this commit.
- `n/a` — no feature behaviour involved (tooling, CI, docs, refactors, standards-only code such as IFC parsing).

CI rejects commits without `Signed-off-by` and `Clean-Room` trailers (see `xtask provenance`).

## 7. File formats and interoperability

- **Interchange is through open standards only**: IFC, DXF, PDF, SVG, glTF, CSV.
- The native format of the reference product is **out of scope**. We do not read or write it. Users export IFC from
  their existing tool and open it in Domos.
- Any future proposal to read a proprietary format needs a written legal opinion first, covering Article 6 of
  Directive 2009/24/EC (decompilation for interoperability) and its conditions, which are narrow: the information
  must not already be readily available, and IFC export arguably makes it available.

## 8. Library objects and scripting

- Domos ships only objects authored by contributors from scratch or under a compatible open licence, listed in
  `ASSETS.md`.
- Domos has its own parametric object system. Compatibility with any vendor scripting language for library objects
  is **deferred** to a later phase and needs a legal review before work starts. If approved, it follows this
  protocol with an extra rule: the language spec is reconstructed from observed behaviour of our own test scripts,
  not from the vendor's reference guide, whose notice expressly prohibits reproduction and paraphrasing.
- Users may open their own objects in a future compatibility mode; Domos never redistributes vendor objects.

## 9. Incidents

If anyone finds that protected material may have entered the repository:

1. Open a private issue labelled `provenance` and notify the maintainers.
2. Revert the material and every change derived from it; rewrite history if it was never released.
3. Re-implement from a fresh spec by a different Implementer.
4. Record the incident and remedy in `docs/legal/incidents.md`.

## 10. Record keeping

Keep, for at least 10 years after the last release that contains the work: observation logs, specs with review
blocks, `contributors.md`, licence records of Observers (invoice or subscription confirmation), and the git history.
