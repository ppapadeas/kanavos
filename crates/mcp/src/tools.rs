//! The MCP tools: definitions with input schemas, and their mapping onto control-protocol methods.

use serde_json::{Map, Value, json};

/// One tool: its name, control method, title, description, input schema and whether it only reads.
pub struct Tool {
    /// Tool name.
    pub name: &'static str,
    /// Control-protocol method it calls.
    pub method: &'static str,
    /// Short title.
    pub title: &'static str,
    /// What it does, for the model.
    pub description: &'static str,
    /// Whether it leaves the document unchanged.
    pub read_only: bool,
    /// Whether it may discard data (undo history, unsaved changes).
    pub destructive: bool,
    /// JSON schema of its arguments.
    pub schema: fn() -> Value,
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false })
}

fn empty() -> Value {
    object(json!({}), &[])
}

fn id_or_name(what: &str) -> Value {
    json!({ "description": format!("{what} id (number) or name"), "type": ["integer", "string"] })
}

/// Every tool, in listing order.
pub const TOOLS: &[Tool] = &[
    Tool {
        name: "document_info",
        method: "session.info",
        title: "Session and document information",
        description: "Application and protocol versions, language, available export formats and the open document: title, version, unsaved changes, storeys and counts.",
        read_only: true,
        destructive: false,
        schema: empty,
    },
    Tool {
        name: "new_document",
        method: "document.new",
        title: "New document",
        description: "Starts a new document from a template: 'starter' (materials, build-ups, two storeys and the usual views), 'empty', or 'sample' (a complete two-storey house with its permit drawing set). Fails if the open document has unsaved changes, unless force is true.",
        read_only: false,
        destructive: true,
        schema: || {
            object(
                json!({
                    "template": { "type": "string", "enum": ["starter", "empty", "sample"], "default": "starter" },
                    "force": { "type": "boolean", "description": "Discard unsaved changes of the open document." }
                }),
                &[],
            )
        },
    },
    Tool {
        name: "open_document",
        method: "document.open",
        title: "Open document",
        description: "Opens a .kanavos file.",
        read_only: false,
        destructive: true,
        schema: || {
            object(
                json!({
                    "path": { "type": "string", "description": "File path." },
                    "force": { "type": "boolean", "description": "Discard unsaved changes of the open document." }
                }),
                &["path"],
            )
        },
    },
    Tool {
        name: "save_document",
        method: "document.save",
        title: "Save document",
        description: "Saves the document as a .kanavos file, to path or where it was opened from.",
        read_only: false,
        destructive: false,
        schema: || {
            object(
                json!({ "path": { "type": "string", "description": "File path; required for a new document." } }),
                &[],
            )
        },
    },
    Tool {
        name: "list_commands",
        method: "command.list",
        title: "List commands",
        description: "Lists every command that changes the document, with a label and one-line help. Give a name to get that command's JSON schema of arguments before running it.",
        read_only: true,
        destructive: false,
        schema: || {
            object(
                json!({ "name": { "type": "string", "description": "A command name, e.g. 'opening.place'." } }),
                &[],
            )
        },
    },
    Tool {
        name: "run_command",
        method: "command.run",
        title: "Run command",
        description: "Runs one command (e.g. element.create, opening.place, slab.from_walls, sheet.permit_set) as one undo step. Returns the created, changed and deleted ids and the new document version. Errors name the failing argument path. Use dry_run to check a command without changing the document.",
        read_only: false,
        destructive: false,
        schema: || {
            object(
                json!({
                    "name": { "type": "string", "description": "Command name, see list_commands." },
                    "args": { "type": "object", "description": "Command arguments, as in the command's schema." },
                    "if_version": { "type": "integer", "description": "Run only if the document version still is this." },
                    "dry_run": { "type": "boolean", "description": "Check the command without changing the document." }
                }),
                &["name"],
            )
        },
    },
    Tool {
        name: "run_batch",
        method: "command.batch",
        title: "Run several commands",
        description: "Runs several commands as one undo step; fails as a whole and names the failing command's index.",
        read_only: false,
        destructive: false,
        schema: || {
            object(
                json!({
                    "commands": { "type": "array", "items": { "type": "object", "properties": {
                        "name": { "type": "string" }, "args": { "type": "object" } }, "required": ["name"] } },
                    "label": { "type": "string", "description": "History label for the step." },
                    "if_version": { "type": "integer" },
                    "dry_run": { "type": "boolean" }
                }),
                &["commands"],
            )
        },
    },
    Tool {
        name: "undo",
        method: "history.undo",
        title: "Undo",
        description: "Undoes the last steps.",
        read_only: false,
        destructive: false,
        schema: || object(json!({ "steps": { "type": "integer", "minimum": 1, "default": 1 } }), &[]),
    },
    Tool {
        name: "redo",
        method: "history.redo",
        title: "Redo",
        description: "Redoes undone steps.",
        read_only: false,
        destructive: false,
        schema: || object(json!({ "steps": { "type": "integer", "minimum": 1, "default": 1 } }), &[]),
    },
    Tool {
        name: "history",
        method: "history.list",
        title: "History",
        description: "The undo steps with their labels, oldest first, and the steps that can be redone.",
        read_only: true,
        destructive: false,
        schema: empty,
    },
    Tool {
        name: "query_elements",
        method: "query.elements",
        title: "Find elements",
        description: "Finds elements by category (wall, slab, column, beam, door, window, opening, stair, roof, railing, space, object, dimension, text, level, drafting, grid, underlay), storey, tag, name or user property, returning ids and the requested fields.",
        read_only: true,
        destructive: false,
        schema: || {
            object(
                json!({
                    "category": { "description": "Category key or list of keys.", "type": ["string", "array"], "items": { "type": "string" } },
                    "storey": id_or_name("Storey"),
                    "ids": { "type": "array", "items": { "type": "integer" } },
                    "tag": { "type": "string", "description": "Exact tag, e.g. 'Θ1'." },
                    "name": { "type": "string", "description": "Name contains (spaces, objects, openings, text)." },
                    "properties": { "type": "object", "additionalProperties": { "type": "string" } },
                    "fields": { "type": "array", "items": { "type": "string", "enum": ["kind", "properties", "quantities", "bbox", "name", "storeyName", "phase", "view"] } },
                    "limit": { "type": "integer", "minimum": 0 }
                }),
                &[],
            )
        },
    },
    Tool {
        name: "quantities",
        method: "query.quantities",
        title: "Quantities and ΝΟΚ figures",
        description: "Building-code figures (plot area, coverage and floor-area use against the building terms, height, areas per storey), a schedule (doors, windows, spaces, walls), the area table, or totals per category.",
        read_only: true,
        destructive: false,
        schema: || {
            object(
                json!({
                    "what": { "type": "string", "enum": ["nok", "schedule", "area_table", "totals"] },
                    "schedule": { "type": "string", "enum": ["doors", "windows", "spaces", "walls", "slabs", "materials", "objects"] }
                }),
                &["what"],
            )
        },
    },
    Tool {
        name: "list_views",
        method: "view.list",
        title: "List views and sheets",
        description: "Views (plans, sections, elevations, 3D, schedules) and sheets with their viewports.",
        read_only: true,
        destructive: false,
        schema: empty,
    },
    Tool {
        name: "render_view",
        method: "view.render",
        title: "Render a view or sheet",
        description: "Draws a view or a sheet as a PNG image (default), SVG text or drawing JSON.",
        read_only: true,
        destructive: false,
        schema: || {
            object(
                json!({
                    "view": id_or_name("View"),
                    "sheet": id_or_name("Sheet (or sheet number, e.g. 'Α-02')"),
                    "format": { "type": "string", "enum": ["png", "svg", "json"], "default": "png" },
                    "dpi": { "type": "number", "minimum": 10, "maximum": 600, "default": 100 }
                }),
                &[],
            )
        },
    },
    Tool {
        name: "export",
        method: "export.run",
        title: "Export",
        description: "Writes files: PDF, SVG, DXF, PNG or JSON of a view, a sheet or all sheets; CSV of a schedule view; IFC or glTF of the whole model.",
        read_only: false,
        destructive: false,
        schema: || {
            object(
                json!({
                    "format": { "type": "string", "enum": ["pdf", "svg", "dxf", "png", "ifc", "gltf", "csv", "json"] },
                    "path": { "type": "string", "description": "Output file, or directory for several files." },
                    "view": id_or_name("View"),
                    "sheet": id_or_name("Sheet"),
                    "sheets": { "type": "boolean", "description": "All sheets." }
                }),
                &["format", "path"],
            )
        },
    },
];

/// Finds a tool.
pub fn find(name: &str) -> Option<&'static Tool> {
    TOOLS.iter().find(|t| t.name == name)
}

/// The `tools/list` entry of a tool.
pub fn definition(t: &Tool) -> Value {
    json!({
        "name": t.name,
        "title": t.title,
        "description": t.description,
        "inputSchema": (t.schema)(),
        "annotations": {
            "title": t.title,
            "readOnlyHint": t.read_only,
            "destructiveHint": t.destructive,
            "idempotentHint": t.read_only,
            "openWorldHint": false,
        },
    })
}

/// Control-protocol parameters for a tool's arguments: renames snake_case options to the protocol's camelCase and
/// adapts the few tools whose arguments differ.
pub fn method_params(tool: &Tool, args: Value) -> Value {
    let mut m = match args {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    for (from, to) in [("if_version", "ifVersion"), ("dry_run", "dryRun")] {
        if let Some(v) = m.remove(from) {
            m.insert(to.into(), v);
        }
    }
    match tool.name {
        "list_commands" => match m.remove("name") {
            Some(n) => json!({ "names": [n], "schemas": true }),
            None => json!({ "schemas": false }),
        },
        "render_view" => {
            m.entry("format").or_insert(json!("png"));
            m.entry("dpi").or_insert(json!(100));
            Value::Object(m)
        }
        _ => Value::Object(m),
    }
}
