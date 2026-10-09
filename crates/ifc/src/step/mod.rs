//! ISO 10303-21 ("STEP physical file", Part 21) clear-text encoding: data model, reader and writer.
//!
//! This module knows nothing about IFC. It reads any Part 21 exchange structure into a graph of entity
//! instances whose references are resolved lazily (by id lookup when followed), and writes instances back out.

mod parse;
mod text;
mod write;

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

pub use parse::parse;
pub use text::{decode_string, encode_string};
pub use write::{P, StepWriter, format_real};

/// A parameter value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// `$`: unset optional value.
    Null,
    /// `*`: value derived from other attributes.
    Derived,
    /// An integer.
    Integer(i64),
    /// A real number.
    Real(f64),
    /// A string, with escapes decoded.
    String(String),
    /// A binary value: the hexadecimal digits as written (first digit is the count of unused bits).
    Binary(String),
    /// An enumeration or logical value, without the dots, e.g. `ELEMENT` or `T`.
    Enum(String),
    /// A reference to another instance, `#123`.
    Ref(u64),
    /// A list, set or bag.
    List(Vec<Value>),
    /// A typed parameter such as `IFCLABEL('x')`: the type name (upper case) and the value.
    Typed(Arc<str>, Box<Value>),
}

impl Value {
    /// The referenced instance id.
    pub fn as_ref_id(&self) -> Option<u64> {
        match self {
            Value::Ref(id) => Some(*id),
            _ => None,
        }
    }

    /// The value as a number (integer or real, also inside a typed parameter).
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Real(v) => Some(*v),
            Value::Integer(v) => Some(*v as f64),
            Value::Typed(_, inner) => inner.as_f64(),
            _ => None,
        }
    }

    /// The value as an integer (also inside a typed parameter).
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Integer(v) => Some(*v),
            Value::Typed(_, inner) => inner.as_i64(),
            _ => None,
        }
    }

    /// The value as text (also inside a typed parameter).
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            Value::Typed(_, inner) => inner.as_str(),
            _ => None,
        }
    }

    /// The enumeration value without dots (also inside a typed parameter).
    pub fn as_enum(&self) -> Option<&str> {
        match self {
            Value::Enum(s) => Some(s),
            Value::Typed(_, inner) => inner.as_enum(),
            _ => None,
        }
    }

    /// The value as a boolean (`.T.` / `.F.`; `.U.` and anything else gives `None`).
    pub fn as_bool(&self) -> Option<bool> {
        match self.as_enum() {
            Some("T") => Some(true),
            Some("F") => Some(false),
            _ => None,
        }
    }

    /// The list items (also inside a typed parameter).
    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Value::List(items) => Some(items),
            Value::Typed(_, inner) => inner.as_list(),
            _ => None,
        }
    }

    /// The type name of a typed parameter.
    pub fn type_name(&self) -> Option<&str> {
        match self {
            Value::Typed(name, _) => Some(name),
            _ => None,
        }
    }

    /// Whether the value is `$`.
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// Calls `f` for every instance reference inside this value.
    pub fn for_each_ref(&self, f: &mut impl FnMut(u64)) {
        match self {
            Value::Ref(id) => f(*id),
            Value::List(items) => items.iter().for_each(|v| v.for_each_ref(f)),
            Value::Typed(_, inner) => inner.for_each_ref(f),
            _ => {}
        }
    }
}

/// An entity record: type name (upper case) and parameters in schema order.
#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    /// Upper-case type name, e.g. `IFCWALL`.
    pub name: Arc<str>,
    /// Parameters.
    pub params: Vec<Value>,
}

impl Entity {
    /// Parameter `i`, or `None` if the record is shorter.
    pub fn attr(&self, i: usize) -> Option<&Value> {
        self.params.get(i)
    }
    /// Parameter `i` as an instance reference.
    pub fn ref_at(&self, i: usize) -> Option<u64> {
        self.attr(i).and_then(Value::as_ref_id)
    }
    /// Parameter `i` as a number.
    pub fn f64_at(&self, i: usize) -> Option<f64> {
        self.attr(i).and_then(Value::as_f64)
    }
    /// Parameter `i` as text.
    pub fn str_at(&self, i: usize) -> Option<&str> {
        self.attr(i).and_then(Value::as_str)
    }
    /// Parameter `i` as an enumeration value.
    pub fn enum_at(&self, i: usize) -> Option<&str> {
        self.attr(i).and_then(Value::as_enum)
    }
    /// Parameter `i` as a list.
    pub fn list_at(&self, i: usize) -> Option<&[Value]> {
        self.attr(i).and_then(Value::as_list)
    }
    /// Parameter `i` as a list of references (non-reference items are skipped).
    pub fn refs_at(&self, i: usize) -> Vec<u64> {
        self.list_at(i).map(|l| l.iter().filter_map(Value::as_ref_id).collect()).unwrap_or_default()
    }
}

/// An entity instance: a simple record, or a complex (multi-record) instance in external mapping.
#[derive(Debug, Clone, PartialEq)]
pub enum Record {
    /// `#1=IFCWALL(...);`
    Simple(Entity),
    /// `#1=(A(...)B(...));`
    Complex(Vec<Entity>),
}

impl Record {
    /// The simple record, if this is one.
    pub fn simple(&self) -> Option<&Entity> {
        match self {
            Record::Simple(e) => Some(e),
            Record::Complex(_) => None,
        }
    }
    /// All partial records (one for a simple record).
    pub fn parts(&self) -> &[Entity] {
        match self {
            Record::Simple(e) => std::slice::from_ref(e),
            Record::Complex(parts) => parts,
        }
    }
}

/// A parsed exchange structure.
#[derive(Debug, Clone, Default)]
pub struct StepFile {
    /// Header entities (`FILE_DESCRIPTION`, `FILE_NAME`, `FILE_SCHEMA`, …).
    pub header: Vec<Entity>,
    /// Instances by id, from all data sections.
    pub data: BTreeMap<u64, Record>,
    /// Instance ids per entity type name (simple records only), in id order.
    pub by_type: BTreeMap<Arc<str>, Vec<u64>>,
}

impl StepFile {
    /// A simple record by id.
    pub fn get(&self, id: u64) -> Option<&Entity> {
        self.data.get(&id).and_then(Record::simple)
    }

    /// The type name of a simple record.
    pub fn type_of(&self, id: u64) -> Option<&str> {
        self.get(id).map(|e| e.name.as_ref())
    }

    /// Ids of all simple records of a type (upper case), in id order.
    pub fn of_type(&self, name: &str) -> &[u64] {
        self.by_type.get(name).map(Vec::as_slice).unwrap_or(&[])
    }

    /// A header entity by name.
    pub fn header_entity(&self, name: &str) -> Option<&Entity> {
        self.header.iter().find(|e| e.name.as_ref() == name)
    }

    /// Schema identifiers from `FILE_SCHEMA`, e.g. `["IFC4"]`.
    pub fn schemas(&self) -> Vec<String> {
        self.header_entity("FILE_SCHEMA")
            .and_then(|e| e.list_at(0))
            .map(|l| l.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
            .unwrap_or_default()
    }

    /// Number of instances per type name (complex instances count once per partial record).
    pub fn type_counts(&self) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for record in self.data.values() {
            for part in record.parts() {
                *counts.entry(part.name.to_string()).or_insert(0) += 1;
            }
        }
        counts
    }

    /// References that point at ids not present in the file, as `(from, to)` pairs.
    pub fn dangling_references(&self) -> Vec<(u64, u64)> {
        let mut out = Vec::new();
        for (id, record) in &self.data {
            for part in record.parts() {
                for p in &part.params {
                    p.for_each_ref(&mut |r| {
                        if !self.data.contains_key(&r) {
                            out.push((*id, r));
                        }
                    });
                }
            }
        }
        out
    }
}

/// A syntax error in a Part 21 file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepError {
    /// Line (1-based).
    pub line: usize,
    /// Column (1-based, in bytes).
    pub column: usize,
    /// What was wrong.
    pub message: String,
}

impl fmt::Display for StepError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}, column {}: {}", self.line, self.column, self.message)
    }
}

impl std::error::Error for StepError {}
