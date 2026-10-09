//! Command logs: one command per line as JSON (`{"name": "...", "args": {...}}`), the format of
//! `kanavos-cli run` scripts and of session logs attached to bug reports.
//!
//! Blank lines and lines starting with `#` or `//` are comments. Each line is read with
//! [`AnyCommand::from_parts`], so errors name the line and the path of the failing value.

use std::fmt;

use serde_json::Value;

use crate::AnyCommand;
use crate::commands::ParseError;

/// Why a command log could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogError {
    /// 1-based line number.
    pub line: usize,
    /// Path of the failing value inside the line, e.g. `args.kind.start`; empty for JSON syntax errors.
    pub path: String,
    /// What is wrong.
    pub message: String,
}

impl fmt::Display for LogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(f, "line {}: {}", self.line, self.message)
        } else {
            write!(f, "line {}: {}: {}", self.line, self.path, self.message)
        }
    }
}

impl std::error::Error for LogError {}

/// Writes commands as JSON lines.
pub fn to_jsonl<'a>(commands: impl IntoIterator<Item = &'a AnyCommand>) -> String {
    let mut out = String::new();
    for c in commands {
        if let Ok(line) = serde_json::to_string(c) {
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

/// Reads one command from its JSON form `{"name": "...", "args": {...}}`.
pub fn command_from_value(value: Value) -> Result<AnyCommand, ParseError> {
    let Value::Object(mut map) = value else {
        return Err(ParseError {
            path: String::new(),
            message: "expected an object with `name` and `args`".into(),
        });
    };
    let name = match map.remove("name") {
        Some(Value::String(s)) => s,
        Some(_) => return Err(ParseError { path: "name".into(), message: "must be a string".into() }),
        None => return Err(ParseError { path: "name".into(), message: "is missing".into() }),
    };
    if let Some(extra) = map.keys().find(|k| k.as_str() != "args") {
        return Err(ParseError {
            path: extra.clone(),
            message: "unknown member; expected `name` and `args`".into(),
        });
    }
    AnyCommand::from_parts(&name, map.remove("args").unwrap_or(Value::Null))
}

/// Reads a JSON-lines command log.
pub fn parse_jsonl(text: &str) -> Result<Vec<AnyCommand>, LogError> {
    Ok(parse_jsonl_numbered(text)?.into_iter().map(|(_, c)| c).collect())
}

/// Reads a JSON-lines command log, keeping the 1-based line number of every command (for error messages when a
/// command later fails to apply).
pub fn parse_jsonl_numbered(text: &str) -> Result<Vec<(usize, AnyCommand)>, LogError> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        let value: Value = serde_json::from_str(line).map_err(|e| LogError {
            line: i + 1,
            path: String::new(),
            message: e.to_string(),
        })?;
        let c = command_from_value(value).map_err(|e| LogError {
            line: i + 1,
            path: e.path,
            message: e.message,
        })?;
        out.push((i + 1, c));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_name_the_line_and_the_path() {
        let text = "# a comment\n\n{\"name\":\"storey.create\",\"args\":{\"name\":\"Δώμα\",\"elevation\":6.4,\"height\":3}}\n\
                    {\"name\":\"storey.create\",\"args\":{\"name\":\"Χ\",\"elevation\":\"high\",\"height\":3}}\n";
        let err = parse_jsonl(text).unwrap_err();
        assert_eq!(err.line, 4);
        assert_eq!(err.path, "args.elevation");
        let err = parse_jsonl("{\"name\":\"wall.fly\"}").unwrap_err();
        assert_eq!(err.path, "name");
        let err = parse_jsonl("{\"name\":\"storey.delete\",\"argz\":{}}").unwrap_err();
        assert_eq!(err.path, "argz");
        let err = parse_jsonl("not json").unwrap_err();
        assert!(err.path.is_empty());
    }

    #[test]
    fn commands_without_arguments_need_none() {
        let cmds = parse_jsonl("{\"name\":\"sheet.permit_set\"}").unwrap();
        assert_eq!(cmds.len(), 1);
        let text = to_jsonl(&cmds);
        assert_eq!(parse_jsonl(&text).unwrap(), cmds);
    }
}
