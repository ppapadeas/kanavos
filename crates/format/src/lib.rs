//! Native `.kanavos` container: read, write, migrate.
//!
//! Layer L6. See `docs/architecture.md` §6 and the file format description in `docs/format.md`.
//!
//! A `.kanavos` file is a ZIP archive:
//!
//! | Entry | Contents |
//! | --- | --- |
//! | `manifest.json` | [`Manifest`]: format name, schema version, application version, timestamps, counts |
//! | `model.json` | The [`Document`], as pretty-printed JSON with deterministic key order |
//! | `thumbnail.png` | Optional preview image, stored uncompressed |
//! | `assets/…`, `cache/…` | Reserved for embedded assets and rebuildable caches; not written yet |
//!
//! Archives are deterministic: entries are written in a fixed order with fixed timestamps, permissions and
//! compression level, so saving the same document with the same [`SaveOptions`] gives the same bytes, and
//! save → load → save is byte-identical.
//!
//! Older schema versions are upgraded on load through [`migrate()`]; files from a newer schema are refused with
//! [`FormatError::NewerVersion`] rather than read wrongly. Information that older schema 1 files kept elsewhere
//! (for example the IFC `GlobalId` of imported elements, once a user property) is moved into its current field on
//! load ([`legacy`]).

#![forbid(unsafe_code)]

mod container;
mod error;
mod files;
mod finite;
pub mod legacy;
mod manifest;
mod migrate;

use std::path::Path;

use kanavos_model::Document;
use serde_json::Value;

pub use container::{
    ASSETS_PREFIX, CACHE_PREFIX, MANIFEST_ENTRY, MAX_ENTRY_BYTES, MODEL_ENTRY, THUMBNAIL_ENTRY,
};
pub use error::{FormatError, LoadWarning};
pub use files::{
    RECOVERY_FILE_NAME, Recovery, autosave, autosave_with, backup_path, discard_recovery, find_recovery,
    read_manifest, write_atomic,
};
pub use manifest::{Counts, FORMAT_NAME, Manifest, is_rfc3339};
pub use migrate::{CURRENT_SCHEMA_VERSION, OLDEST_SCHEMA_VERSION, migrate};

use container::{Archive, Entry, PNG_SIGNATURE};

/// Conventional file extension, without the dot.
pub const FILE_EXTENSION: &str = "kanavos";

/// How a file is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveOptions {
    /// Version of the application writing the file. Defaults to this crate's version.
    pub app_version: String,
    /// When the project was first created (RFC 3339). Keep the value from the loaded manifest when re-saving.
    pub created: Option<String>,
    /// When the project is being saved (RFC 3339). Supplied by the caller so that output is reproducible.
    pub modified: Option<String>,
    /// PNG preview stored as `thumbnail.png`.
    pub thumbnail: Option<Vec<u8>>,
    /// Keep the previous file as `<name>.bak` when [`save`] replaces it. Default `true`.
    pub backup: bool,
    /// For recovery files: the path of the project the copy belongs to.
    pub recovery_of: Option<String>,
}

impl Default for SaveOptions {
    fn default() -> Self {
        SaveOptions {
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            created: None,
            modified: None,
            thumbnail: None,
            backup: true,
            recovery_of: None,
        }
    }
}

impl SaveOptions {
    /// Options for saving a project that was loaded with `manifest` again: keeps its creation time and stamps the
    /// new modification time.
    pub fn resave(manifest: &Manifest, modified: impl Into<String>) -> SaveOptions {
        SaveOptions {
            created: manifest.created.clone(),
            modified: Some(modified.into()),
            ..SaveOptions::default()
        }
    }
}

/// How strictly a file is checked when loading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Validation {
    /// Run [`Document::validate`] and refuse inconsistent documents (default).
    #[default]
    Strict,
    /// Run [`Document::validate`] but load inconsistent documents anyway, reporting the problem as a
    /// [`LoadWarning::Invalid`]. For recovery tools.
    Lenient,
}

/// How a file is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LoadOptions {
    /// Consistency checking.
    pub validation: Validation,
}

/// A successfully loaded project.
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    /// The document, upgraded to the current schema.
    pub document: Document,
    /// The manifest as stored in the file.
    pub manifest: Manifest,
    /// The thumbnail, if the file has a valid one.
    pub thumbnail: Option<Vec<u8>>,
    /// Things worth telling the user.
    pub warnings: Vec<LoadWarning>,
}

/// Encodes a document as `.kanavos` bytes.
///
/// Fails if a timestamp is not RFC 3339, the thumbnail is not a PNG, or the document holds a non-finite number
/// (which JSON cannot represent).
pub fn to_bytes(document: &Document, options: &SaveOptions) -> Result<Vec<u8>, FormatError> {
    encode(document, options).map(|(bytes, _)| bytes)
}

fn encode(document: &Document, options: &SaveOptions) -> Result<(Vec<u8>, Manifest), FormatError> {
    manifest::check_timestamp(&options.created)?;
    manifest::check_timestamp(&options.modified)?;
    if let Some(png) = &options.thumbnail
        && !png.starts_with(&PNG_SIGNATURE)
    {
        return Err(FormatError::InvalidThumbnail);
    }
    finite::check(document).map_err(|e| match e.custom {
        Some(message) => FormatError::Serialize(message),
        None => FormatError::NonFinite { path: e.path },
    })?;
    let mut model = serde_json::to_vec_pretty(document).map_err(|e| FormatError::Serialize(e.to_string()))?;
    model.push(b'\n');
    let manifest = Manifest {
        format: FORMAT_NAME.to_owned(),
        schema_version: CURRENT_SCHEMA_VERSION,
        app_version: options.app_version.clone(),
        created: options.created.clone(),
        modified: options.modified.clone(),
        counts: Counts::of(document),
        thumbnail: options.thumbnail.is_some(),
        recovery_of: options.recovery_of.clone(),
    };
    let mut manifest_json =
        serde_json::to_vec_pretty(&manifest).map_err(|e| FormatError::Serialize(e.to_string()))?;
    manifest_json.push(b'\n');
    let mut entries = vec![
        Entry { name: MANIFEST_ENTRY, data: &manifest_json, stored: false },
        Entry { name: MODEL_ENTRY, data: &model, stored: false },
    ];
    if let Some(png) = &options.thumbnail {
        entries.push(Entry { name: THUMBNAIL_ENTRY, data: png, stored: true });
    }
    let bytes = container::write(&entries)?;
    Ok((bytes, manifest))
}

/// Saves a document to `path` atomically, keeping the previous file as `<name>.bak` when
/// [`SaveOptions::backup`] is set. Returns the manifest that was written.
pub fn save(path: &Path, document: &Document, options: &SaveOptions) -> Result<Manifest, FormatError> {
    let (bytes, manifest) = encode(document, options)?;
    write_atomic(path, &bytes, options.backup)?;
    Ok(manifest)
}

/// Loads a project file with default (strict) options.
pub fn load(path: &Path) -> Result<Loaded, FormatError> {
    load_with(path, &LoadOptions::default())
}

/// Loads a project file.
pub fn load_with(path: &Path, options: &LoadOptions) -> Result<Loaded, FormatError> {
    let bytes = std::fs::read(path).map_err(|e| FormatError::io("reading", path, e))?;
    from_bytes_with(&bytes, options)
}

/// Decodes `.kanavos` bytes with default (strict) options.
pub fn from_bytes(bytes: &[u8]) -> Result<Loaded, FormatError> {
    from_bytes_with(bytes, &LoadOptions::default())
}

/// Decodes `.kanavos` bytes.
pub fn from_bytes_with(bytes: &[u8], options: &LoadOptions) -> Result<Loaded, FormatError> {
    let mut archive = Archive::open(bytes)?;
    let raw_manifest = archive
        .read(MANIFEST_ENTRY)?
        .ok_or_else(|| FormatError::WrongFormat("the archive has no manifest.json".into()))?;
    let manifest = parse_manifest(&raw_manifest)?;
    let raw_model = archive.read(MODEL_ENTRY)?.ok_or(FormatError::MissingEntry(MODEL_ENTRY))?;

    let mut warnings = Vec::new();
    let document = if manifest.schema_version == CURRENT_SCHEMA_VERSION {
        deserialize_model(&raw_model)?
    } else {
        let value: Value = serde_json::from_slice(&raw_model).map_err(|e| json_error(MODEL_ENTRY, "", &e))?;
        let value = migrate(value, manifest.schema_version)?;
        let document = serde_path_to_error::deserialize::<_, Document>(value)
            .map_err(|e| json_error(MODEL_ENTRY, &e.path().to_string(), e.inner()))?;
        warnings.push(LoadWarning::Migrated { from: manifest.schema_version, to: CURRENT_SCHEMA_VERSION });
        document
    };
    let (document, _) = legacy::upgrade(document);

    let thumbnail = match archive.read(THUMBNAIL_ENTRY)? {
        Some(png) if png.starts_with(&PNG_SIGNATURE) => Some(png),
        Some(_) => {
            warnings.push(LoadWarning::BadThumbnail);
            None
        }
        None => None,
    };

    for name in archive.names() {
        let known = name == MANIFEST_ENTRY || name == MODEL_ENTRY || name == THUMBNAIL_ENTRY;
        let reserved = name.starts_with(ASSETS_PREFIX) || name.starts_with(CACHE_PREFIX);
        if !known && !reserved {
            warnings.push(LoadWarning::UnknownEntry(name));
        }
    }

    // A manifest without counts (all zero, e.g. from an older writer) says nothing to compare against.
    let actual = Counts::of(&document);
    let has_counts = manifest.counts != Counts::default();
    for ((what, listed), (_, real)) in manifest.counts.totals().into_iter().zip(actual.totals()) {
        if has_counts && listed != real {
            warnings.push(LoadWarning::CountMismatch {
                what: what.to_owned(),
                manifest: listed,
                model: real,
            });
        }
    }

    if let Err(e) = document.validate() {
        match options.validation {
            Validation::Strict => return Err(FormatError::Invalid(e)),
            Validation::Lenient => warnings.push(LoadWarning::Invalid(e.0)),
        }
    }

    Ok(Loaded { document, manifest, thumbnail, warnings })
}

/// Parses `manifest.json`, checking the format name and schema version before anything else so that files from
/// newer versions are always recognised as such.
pub(crate) fn parse_manifest(raw: &[u8]) -> Result<Manifest, FormatError> {
    let value: Value = serde_json::from_slice(raw).map_err(|e| json_error(MANIFEST_ENTRY, "", &e))?;
    let format = value.get("format").and_then(Value::as_str);
    if format != Some(FORMAT_NAME) {
        return Err(FormatError::WrongFormat(match format {
            Some(other) => format!("the manifest names the format {other:?}"),
            None => "the manifest does not name a format".into(),
        }));
    }
    let version = value
        .get("schema_version")
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| FormatError::WrongFormat("the manifest has no valid schema_version".into()))?;
    migrate::check_readable(version)?;
    serde_path_to_error::deserialize::<_, Manifest>(value)
        .map_err(|e| json_error(MANIFEST_ENTRY, &e.path().to_string(), e.inner()))
}

/// Deserialises the current schema. The fast path uses plain `serde_json`; only when that fails is the input
/// parsed again with path tracking, to name the failing field.
fn deserialize_model(raw: &[u8]) -> Result<Document, FormatError> {
    match serde_json::from_slice::<Document>(raw) {
        Ok(doc) => Ok(doc),
        Err(plain) => {
            let mut de = serde_json::Deserializer::from_slice(raw);
            match serde_path_to_error::deserialize::<_, Document>(&mut de) {
                Err(e) => Err(json_error(MODEL_ENTRY, &e.path().to_string(), e.inner())),
                Ok(_) => Err(json_error(MODEL_ENTRY, "", &plain)),
            }
        }
    }
}

fn json_error(entry: &str, path: &str, e: &serde_json::Error) -> FormatError {
    // serde_path_to_error renders an unknown position as "?"; an empty path reads better.
    let path = if path == "?" || path == "." { "" } else { path };
    FormatError::CorruptJson {
        entry: entry.to_owned(),
        path: path.to_owned(),
        line: e.line(),
        column: e.column(),
        message: strip_position(&e.to_string()),
    }
}

/// `serde_json` appends " at line L column C" to messages; the position is reported separately.
fn strip_position(message: &str) -> String {
    match message.rfind(" at line ") {
        Some(i) => message.get(..i).unwrap_or(message).to_owned(),
        None => message.to_owned(),
    }
}
