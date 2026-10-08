# Legal brief: building an independent BIM application in the EU

**Status:** working brief for the project owner and for briefing a lawyer. It is not legal advice. Article numbers
and their Greek transposition must be confirmed by a lawyer specialised in intellectual property (δικηγόρος με
εξειδίκευση στην πνευματική ιδιοκτησία) before the first public release.

**Last reviewed:** 2026-10-08

---

## 1. Summary

| Risk area | What is protected | What we do | Residual risk |
| --- | --- | --- | --- |
| Copyright in software | Code and its expression | Write all code ourselves; no vendor SDK, no decompilation | Low |
| Copyright in manuals and help text | Literary expression, including structure | Never copy or paraphrase; specs written from observed behaviour | Low, if the protocol is followed |
| Copyright in UI artwork, icons, library objects, templates | Artistic expression and content | Own visual design; own object library | Low |
| Functionality, workflow, file formats, languages | **Not protected by copyright** | Free to implement | — |
| Trademarks (Archicad, GDL, BIMcloud, BIMx, GRAPHISOFT) | Names and logos | Own product name; factual references only | Low |
| Unfair competition (αθέμιτος ανταγωνισμός) | Slavish imitation causing confusion | Distinct name, look and branding | Low–medium; lawyer to confirm |
| Licence terms (EULA) | Contractual limits on the licensee | Only observe, study, test; no decompilation | Medium; needs EULA review |
| Patents | Specific technical inventions | Freedom-to-operate search before 1.0 | Unknown until searched |
| Trade secrets | Confidential know-how | No NDA-bound contributors in affected areas | Low |

## 2. Copyright: functionality is free, expression is not

**Key terms**

- *Expression*: the concrete form in which an idea is written down — source code, object code, the wording and
  structure of a manual, the drawing of an icon.
- *Functionality*: what the program does, the ideas and principles behind it.

**Law**

- Directive 2009/24/EC on the legal protection of computer programs, Article 1(2): protection covers the
  expression of a program, not the ideas and principles underlying any of its elements, including its interfaces.
  Transposed in Greece by ν. 2121/1993, Article 2(3) and Articles 40–45.
- CJEU, *SAS Institute Inc. v World Programming Ltd*, C‑406/10, judgment of 2 May 2012 (Grand Chamber):
  - neither the functionality of a computer program nor the programming language and the format of data files used
    to exploit its functions are a form of expression, so they are not protected by copyright;
  - reproducing elements of another program's user manual can infringe the copyright in that manual if those
    elements express its author's own intellectual creation. The UK court later found that World Programming's
    manual infringed in exactly this way.

**What it means for us**

- Building a BIM application that does what Archicad does — walls with junctions, storeys, automatic plans and
  sections, layouts, schedules — is lawful in itself.
- The danger is the manuals. Graphisoft's manuals carry an express notice prohibiting reproduction, paraphrasing or
  translation. That is why the clean-room protocol bans copying or paraphrasing any vendor text and requires specs
  written from observed behaviour.

## 3. Using the reference product to study it

**Law**

- Directive 2009/24/EC, Article 5(3): a person entitled to use a copy of a program may, without authorisation,
  observe, study or test its functioning to determine the ideas and principles underlying it, while loading,
  displaying, running, transmitting or storing it as they are entitled to do. Greek transposition: ν. 2121/1993,
  Article 42.
- Directive 2009/24/EC, Article 8: contractual provisions contrary to Article 6 or to the exceptions in Article
  5(2) and (3) are null and void. A licence agreement therefore cannot validly forbid observing, studying and testing.
- *SAS v WPL* confirmed that a licensee may use Article 5(3) to reproduce functionality, provided they do not access
  the source code.

**What it means for us**

- Observers must hold a valid Archicad licence and only use the program normally (running it, trying inputs, noting
  outputs). That is squarely within Article 5(3).
- The clean-room protocol limits observation to exactly these acts.

## 4. Decompilation and file formats

**Law**

- Directive 2009/24/EC, Article 6: decompilation is allowed only when indispensable to obtain information needed
  for interoperability of an independently created program, by the licensee, when the information has not
  previously been readily available, confined to the parts necessary. The information cannot be used to develop a
  program substantially similar in its expression. Greek transposition: ν. 2121/1993, Article 43.
- Directive (EU) 2016/943 on trade secrets, Article 3(1)(b): reverse engineering of a lawfully acquired product is
  lawful acquisition of a trade secret, **unless contractually restricted**. Greek transposition: ν. 4605/2019.

**What it means for us**

- We do **not** decompile anything and do **not** reverse-engineer the native project format. Archicad exports
  IFC, an ISO standard, so the interoperability information is arguably already available, which would weaken any
  Article 6 argument anyway.
- A licence term restricting reverse engineering is enforceable under the trade secrets directive, unlike a term
  restricting observation. Another reason to stay out of binaries and file internals entirely.

## 5. Trademarks

**Facts**

- Graphisoft states that GRAPHISOFT, Archicad, BIMx, BIMcloud and GDL are its trademarks, and that a mark may not be
  incorporated in any company, product, service or domain name, nor its trade dress, type style or logos imitated.
  Its manuals also list GDL as a trademark.

**Law**

- Regulation (EU) 2017/1001 on the EU trade mark, Article 14(1)(c), and Directive (EU) 2015/2436, Article 14:
  a mark may be used to identify or refer to the goods of its owner, in particular to indicate compatibility, if the
  use follows honest practices. Greek law: ν. 4679/2020.

**Rules for the project**

1. Product, crates, binaries, file extensions and domains never contain any Graphisoft mark. Product name: **Kanavos** (from κάναβος, the structural grid).
   A trademark clearance search (EUIPO, OBI/ΓΓΕ for Greece, WIPO Global Brand Database) is needed before public launch.
2. Allowed factual statements, in plain text and not in a logo or headline style, with the mark noted the first time:
   - "Opens IFC files exported from GRAPHISOFT® Archicad® and other BIM tools."
   - "Familiar to people who work in Archicad®."
3. Not allowed: "Archicad alternative" as a product tagline, "free Archicad", comparisons implying endorsement, any
   Graphisoft logo, colour or trade dress.
4. Include in `NOTICE` and on the website: "Archicad, BIMcloud, BIMx, GDL and GRAPHISOFT are trademarks of
   GRAPHISOFT SE. Kanavos is an independent project, not affiliated with or endorsed by GRAPHISOFT SE."

## 6. Unfair competition and look-alike risk

**Law:** ν. 146/1914 on unfair competition (αθέμιτος ανταγωνισμός), Articles 1 and 13, can apply to slavish
imitation (δουλική απομίμηση) that causes confusion about origin, even where no copyright or trademark is infringed.

**Rules:** our own name, logo, icon set, colour palette and layout. We keep familiar *workflow concepts* (storeys,
cut planes, automatic drawings), which are functional, but the screen must not look like Archicad.

## 7. Patents

Software-implemented inventions can be patented in Europe when they have a technical character. We have not done a
search. Before 1.0, run a freedom-to-operate search on Espacenet with applicant "Graphisoft" and other BIM vendors,
for the features we ship (collaboration server deltas, element junction resolution, automatic drawing generation).
The cost of a professional FTO opinion should be budgeted.

## 8. Our own licensing and contributions

- Code: MIT OR Apache‑2.0, at the user's option. Apache‑2.0 includes an express patent licence from contributors.
- Contributions under the Developer Certificate of Origin (`Signed-off-by`), plus the `Clean-Room` trailer.
- Third-party dependencies must be MIT, Apache‑2.0, BSD, ISC, Zlib, Unicode or MPL‑2.0 (file-level copyleft,
  acceptable as a dependency). No GPL or LGPL in the shipped binary. Enforced by `cargo deny` (`deny.toml`).
- Copyright holder line: "the Kanavos contributors". If a legal entity should hold the project later (for example a
  non-profit foundation), adopt a contributor licence agreement before external contributions grow.

## 9. Open questions for the lawyer

1. Confirm the Greek transposition articles cited (ν. 2121/1993 Articles 2(3), 40–45; ν. 4605/2019; ν. 4679/2020;
   ν. 146/1914).
2. Review the current Graphisoft licence terms for clauses on reverse engineering, benchmarking, or developing
   competing products, and whether Article 8 of Directive 2009/24/EC voids them as applied to observation.
3. Is observation by the same person who later implements acceptable, given *SAS v WPL*, with the documentation in
   our protocol? Is the 90-day separation useful or unnecessary?
4. Wording of compatibility claims on the website and in the app.
5. Clearance of the final product name and a freedom-to-operate opinion before 1.0.
6. Whether future compatibility with a vendor's library-object scripting language can be implemented from observed
   behaviour, given the manual's express prohibition on paraphrasing.

## References

1. Directive 2009/24/EC of 23 April 2009 on the legal protection of computer programs (codified version), OJ L 111,
   5.5.2009 — <https://eur-lex.europa.eu/eli/dir/2009/24/oj>
2. CJEU, Case C‑406/10, *SAS Institute Inc. v World Programming Ltd*, 2 May 2012 —
   <https://curia.europa.eu/juris/liste.jsf?num=C-406/10>
3. Directive (EU) 2016/943 of 8 June 2016 on the protection of trade secrets —
   <https://eur-lex.europa.eu/eli/dir/2016/943/oj>
4. Regulation (EU) 2017/1001 on the European Union trade mark — <https://eur-lex.europa.eu/eli/reg/2017/1001/oj>
5. Directive (EU) 2015/2436 approximating the laws of the Member States relating to trade marks —
   <https://eur-lex.europa.eu/eli/dir/2015/2436/oj>
6. Ν. 2121/1993 «Πνευματική ιδιοκτησία, συγγενικά δικαιώματα και πολιτιστικά θέματα» (ΦΕΚ Α' 25) — Οργανισμός
   Πνευματικής Ιδιοκτησίας, <https://www.opi.gr>
7. GRAPHISOFT trademark usage guidelines — <https://graphisoft.com/legal/trademark-usage>
8. GRAPHISOFT GDL Reference Guide, copyright and trademark notice — <https://help.graphisoft.com/AC/27/INT/GDL.pdf>
9. Developer Certificate of Origin 1.1 — <https://developercertificate.org>
