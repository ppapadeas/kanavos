# ADR-0005: The command bus is the only way to change a document

- Status: accepted
- Date: 2026-10-08

## Context

Undo, scripting, the control protocol, tests, crash reproduction and collaboration later all need a single,
serialisable description of every change.

## Decision

Every change is a `Command` with a stable name and a JSON-schema'd argument type. Tools, menus, the control protocol
and tests all submit commands to `domos-engine`. Documents are immutable values; a command returns a new document.

## Consequences

- Unlimited undo with structural sharing; reproducible bug reports (a command log replays a session).
- Commands are public API and must be versioned carefully.
