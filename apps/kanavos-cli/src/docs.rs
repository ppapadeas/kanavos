//! The command reference (docs/commands.md), generated from the command schemas, labels, help and examples.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use kanavos_engine::AnyCommand;
use kanavos_engine::commands::{command_help, command_label, examples};
use kanavos_units::Locale;
use serde_json::Value;

fn def_name(r: &str) -> &str {
    r.rsplit('/').next().unwrap_or(r)
}

/// A short type description of a schema, e.g. `number`, `Point2`, `array of Id`, `string or null`.
fn type_of(s: &Value) -> String {
    if let Some(r) = s.get("$ref").and_then(Value::as_str) {
        return format!("`{}`", def_name(r));
    }
    if let Some(c) = s.get("const") {
        return format!("`{c}`");
    }
    if let Some(e) = s.get("enum").and_then(Value::as_array) {
        let values: Vec<String> = e.iter().map(|v| format!("`{v}`")).collect();
        return values.join(" \\| ");
    }
    for key in ["anyOf", "oneOf"] {
        if let Some(variants) = s.get(key).and_then(Value::as_array) {
            let types: Vec<String> = variants
                .iter()
                .filter(|v| v.get("type").and_then(Value::as_str) != Some("null"))
                .map(type_of)
                .collect();
            let nullable = types.len() < variants.len();
            let joined = types.join(" or ");
            return if nullable { format!("{joined} or null") } else { joined };
        }
    }
    match s.get("type") {
        Some(Value::String(t)) if t == "array" => match s.get("items") {
            Some(items) => format!("array of {}", type_of(items)),
            None => match s.get("prefixItems").and_then(Value::as_array) {
                Some(p) => format!("[{}]", p.iter().map(type_of).collect::<Vec<_>>().join(", ")),
                None => "array".into(),
            },
        },
        Some(Value::String(t)) => t.clone(),
        Some(Value::Array(ts)) => {
            ts.iter().filter_map(Value::as_str).filter(|t| *t != "null").collect::<Vec<_>>().join(" or ")
                + if ts.iter().any(|t| t == "null") { " or null" } else { "" }
        }
        _ => "any".into(),
    }
}

fn cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

fn arguments_table(out: &mut String, schema: &Value) {
    let props = schema.get("properties").and_then(Value::as_object).cloned().unwrap_or_default();
    if props.is_empty() {
        out.push_str("No arguments.\n\n");
        return;
    }
    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    out.push_str("| Argument | Type | Required | Description |\n| --- | --- | --- | --- |\n");
    for (name, p) in &props {
        let req = if required.contains(&name.as_str()) { "yes" } else { "" };
        let mut desc = p.get("description").and_then(Value::as_str).unwrap_or_default().to_owned();
        if let Some(d) = p.get("default")
            && !d.is_null()
        {
            let _ = write!(desc, " Default: `{d}`.");
        }
        let _ = writeln!(out, "| `{name}` | {} | {req} | {} |", cell(&type_of(p)), cell(desc.trim()));
    }
    out.push('\n');
}

/// One line describing a shared type: its variants or fields.
fn describe_def(def: &Value) -> String {
    let desc = def.get("description").and_then(Value::as_str).unwrap_or_default().replace('\n', " ");
    let detail = if let Some(e) = def.get("enum").and_then(Value::as_array) {
        let v: Vec<String> = e.iter().map(|x| format!("`{x}`")).collect();
        format!("One of {}.", v.join(", "))
    } else if let Some(variants) = def.get("oneOf").or_else(|| def.get("anyOf")).and_then(Value::as_array) {
        // Tagged variants carry a `type` property; plain enums are constants.
        let tagged: Vec<String> = variants
            .iter()
            .filter_map(|v| {
                let t = v.get("properties")?.get("type")?;
                let c = t.get("const").or_else(|| t.get("enum").and_then(|e| e.get(0)))?;
                Some(format!("`{}`", c.as_str().unwrap_or_default()))
            })
            .collect();
        let consts: Vec<String> = variants
            .iter()
            .filter_map(|v| v.get("const").or_else(|| v.get("enum").and_then(|e| e.get(0))))
            .map(|c| format!("`{c}`"))
            .collect();
        if !tagged.is_empty() {
            format!("Variants (`type`): {}.", tagged.join(", "))
        } else if !consts.is_empty() {
            format!("One of {}.", consts.join(", "))
        } else {
            String::new()
        }
    } else if let Some(p) = def.get("properties").and_then(Value::as_object) {
        let fields: Vec<String> = p.iter().map(|(k, v)| format!("`{k}` ({})", type_of(v))).collect();
        format!("Fields: {}.", fields.join(", "))
    } else {
        type_of(def)
    };
    cell(format!("{desc} {detail}").trim())
}

/// A command's documentation without its leading "`name` — ", as one paragraph.
fn details(schema: &Value) -> String {
    let d = schema.get("description").and_then(Value::as_str).unwrap_or_default();
    let d = d.split_once(" — ").map(|(_, rest)| rest).unwrap_or(d);
    let mut text = d.replace('\n', " ");
    if let Some(first) = text.get(..1) {
        text = first.to_uppercase() + text.get(1..).unwrap_or_default();
    }
    text
}

/// Whether a shared type is itself a command (reached through `batch`).
fn is_command_def(name: &str, def: &Value) -> bool {
    name == "AnyCommand"
        || def
            .get("description")
            .and_then(Value::as_str)
            .and_then(|d| d.strip_prefix('`'))
            .and_then(|d| d.split_once('`'))
            .is_some_and(|(n, _)| AnyCommand::NAMES.contains(&n))
}

/// The whole command reference as Markdown.
pub fn markdown() -> String {
    let mut out = String::new();
    out.push_str(
        "# Commands\n\n\
         <!-- Generated by `kanavos-cli commands --markdown`; do not edit by hand. -->\n\n\
         Every change to a Kanavos document is a command (ADR-0005). The same commands are used by the interface, \
         the [control protocol](control-protocol.md) (`command.run`, `command.batch`), the MCP server \
         (`run_command`) and command scripts (`kanavos-cli run script.jsonl project.kanavos`, one command per \
         line). A command's JSON form is `{\"name\": \"<command>\", \"args\": {…}}`.\n\n\
         Units are SI: lengths in metres, areas in m², angles in radians unless an argument says degrees; plan \
         points are `[x, y]` (x east, y north); ids are integers. Arguments with a default may be left out. \
         `command.list` returns the full JSON schema of every command, including the shared types listed at the \
         end.\n\n",
    );
    // Group by area (the part before the first dot).
    let mut groups: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for name in AnyCommand::NAMES {
        let area = name.split('.').next().unwrap_or(name);
        groups.entry(area).or_default().push(name);
    }
    out.push_str("| Command | Ελληνικά | English | What it does |\n| --- | --- | --- | --- |\n");
    for name in AnyCommand::NAMES {
        let _ = writeln!(
            out,
            "| [`{name}`](#{}) | {} | {} | {} |",
            name.replace('.', ""),
            command_label(name, Locale::El),
            command_label(name, Locale::En),
            cell(&command_help(name, Locale::En))
        );
    }
    out.push('\n');
    let mut defs: BTreeMap<String, Value> = BTreeMap::new();
    for (area, names) in &groups {
        let _ = writeln!(out, "## {area}\n");
        for name in names {
            let schema = AnyCommand::args_schema(name).unwrap_or(Value::Null);
            let _ = writeln!(out, "### `{name}`\n");
            let _ =
                writeln!(out, "{} · {}\n", command_label(name, Locale::El), command_label(name, Locale::En));
            // The schema's description usually extends the one-line help; print the help only when it does not.
            let help = command_help(name, Locale::En);
            let detail = details(&schema);
            let same_start = |a: &str, b: &str| {
                let k = |s: &str| s.chars().take(24).collect::<String>().to_lowercase();
                k(a) == k(b)
            };
            if detail.is_empty() || !same_start(&help, &detail) {
                let _ = writeln!(out, "{help}\n");
            }
            if !detail.is_empty() {
                let _ = writeln!(out, "{detail}\n");
            }
            arguments_table(&mut out, &schema);
            if let Some(ex) = examples::example(name) {
                let args = serde_json::to_string(&ex).unwrap_or_default();
                let _ = writeln!(out, "Example:\n\n```json\n{{\"name\":\"{name}\",\"args\":{args}}}\n```\n");
            }
            if let Some(Value::Object(d)) = schema.get("$defs") {
                for (k, v) in d {
                    defs.entry(k.clone()).or_insert_with(|| v.clone());
                }
            }
        }
    }
    out.push_str("## Shared types\n\n| Type | Description |\n| --- | --- |\n");
    for (name, def) in &defs {
        if !is_command_def(name, def) {
            let _ = writeln!(out, "| `{name}` | {} |", describe_def(def));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_covers_every_command_with_an_example() {
        let md = markdown();
        for name in AnyCommand::NAMES {
            assert!(md.contains(&format!("### `{name}`")), "{name}");
        }
        assert_eq!(md.matches("Example:").count(), AnyCommand::NAMES.len());
        assert!(md.contains("| `ElementKind` |"));
    }

    #[test]
    fn committed_reference_is_up_to_date() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/commands.md");
        let committed = std::fs::read_to_string(path).unwrap().replace("\r\n", "\n");
        assert!(
            committed == markdown(),
            "docs/commands.md is out of date: run `kanavos-cli commands --markdown > docs/commands.md`"
        );
    }

    #[test]
    fn types_are_described_compactly() {
        assert_eq!(type_of(&serde_json::json!({"type": ["string", "null"]})), "string or null");
        assert_eq!(type_of(&serde_json::json!({"$ref": "#/$defs/Id"})), "`Id`");
        assert_eq!(
            type_of(&serde_json::json!({"type": "array", "items": {"$ref": "#/$defs/Id"}})),
            "array of `Id`"
        );
    }
}
