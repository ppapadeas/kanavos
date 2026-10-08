# Control protocol

Everything a person can do in Kanavos can be done by a script or an automation client, through the same commands the
interface uses. This makes batch work (export every sheet of 20 projects), testing and integrations possible
without screen scraping.

## Transports

| Transport | Use | Security |
| --- | --- | --- |
| stdio | `kanavos-cli serve --stdio`: one client, spawned as a child process | Inherits the caller's permissions |
| WebSocket | Desktop app, `Preferences › Automation › Allow local control` | Binds to `127.0.0.1` only, random port, one-time token shown in the app; off by default |
| MCP | `kanavos-cli mcp`: MCP server over stdio, mapping tools onto the methods below | Same as stdio |

## Message format

JSON-RPC 2.0. Requests are processed in order per connection.

```json
{"jsonrpc":"2.0","id":1,"method":"command.run",
 "params":{"name":"wall.create","args":{"storey":"0","start":[0,0],"end":[6.4,0],"buildUp":"Δρομική οπτοπλινθοδομή 25"}}}
```

```json
{"jsonrpc":"2.0","id":1,"result":{"created":["01HZX3K8Q2…"],"documentVersion":42}}
```

## Methods (initial set)

| Method | Purpose |
| --- | --- |
| `session.info` | App version, protocol version, open documents |
| `document.open` / `document.new` / `document.save` / `document.close` | File lifecycle |
| `command.list` | All commands with JSON schemas for their arguments |
| `command.run` | Run one command; returns created/changed IDs and the new document version |
| `command.batch` | Run several commands as one undo step |
| `history.undo` / `history.redo` | Undo and redo |
| `query.elements` | Filter elements by type, storey, property expressions; returns IDs and requested properties |
| `query.quantities` | Areas, volumes, lengths, counts; ΝΟΚ figures |
| `view.list` / `view.render` | List views; render one to PNG or SVG for inspection |
| `export.run` | PDF, DXF, IFC, glTF, CSV exports of views, sheets or the model |
| `events.subscribe` | Notifications for document changes, job progress and errors |

## Guarantees

- Commands are validated against their schema before they run; invalid input returns a JSON-RPC error with a
  human-readable message and the failing field path.
- Every successful call that changes the document returns the new `documentVersion`. Clients can pass
  `ifVersion` to make a call conditional, so two clients never overwrite each other silently.
- The protocol is versioned (`session.info.protocol`). Breaking changes bump the major version; old versions stay
  supported for at least one year.
