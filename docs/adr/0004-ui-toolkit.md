# ADR-0004: egui on wgpu for the first desktop interface

- Status: accepted (revisit after M2)
- Date: 2026-10-08

## Context

The interface must be fast, themeable, accessible and portable to the web. Candidates: egui (immediate mode,
mature, AccessKit support, MIT/Apache), iced (retained, MIT), Slint (licensing terms that need checking against our
licence policy), and newer retained-mode stacks that are not yet mature.

## Decision

Use egui/eframe with wgpu for the first interface, with a custom design system (tokens, icons, widgets) in
`kanavos-ui`. The canvas is rendered by `kanavos-render`, not by egui widgets.

## Consequences

- Fast iteration and a working web target.
- Immediate mode makes some polished interactions (complex text editing, large tables) more work; those widgets get
  dedicated implementations. Revisit after milestone M2 with usability-test evidence.
