//! The ZIP container: deterministic writing and defensive reading.

use std::io::{Cursor, Read, Seek, SeekFrom, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, System, ZipArchive, ZipWriter};

use crate::FormatError;

/// Name of the manifest entry.
pub const MANIFEST_ENTRY: &str = "manifest.json";
/// Name of the model entry.
pub const MODEL_ENTRY: &str = "model.json";
/// Name of the optional thumbnail entry.
pub const THUMBNAIL_ENTRY: &str = "thumbnail.png";
/// Prefix reserved for embedded assets (images, imported drawings). Not written yet.
pub const ASSETS_PREFIX: &str = "assets/";
/// Prefix reserved for rebuildable caches (tessellations, view results). Not written yet.
pub const CACHE_PREFIX: &str = "cache/";

/// Largest entry this build reads, uncompressed (2 GiB). Guards against archive bombs.
pub const MAX_ENTRY_BYTES: u64 = 2 << 30;

/// DEFLATE level for JSON entries. Fixed so that archives are reproducible.
const DEFLATE_LEVEL: i64 = 6;

/// PNG file signature.
pub(crate) const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// One entry to write.
pub(crate) struct Entry<'a> {
    pub name: &'static str,
    pub data: &'a [u8],
    /// Store uncompressed (for data that is already compressed, such as PNG).
    pub stored: bool,
}

fn options(stored: bool) -> SimpleFileOptions {
    // Every field that could vary between runs or platforms is pinned: timestamp (1980-01-01, the ZIP epoch),
    // host system and permissions.
    let base = SimpleFileOptions::default()
        .last_modified_time(DateTime::default())
        .system(System::Unix)
        .unix_permissions(0o644)
        .large_file(false);
    if stored {
        base.compression_method(CompressionMethod::Stored)
    } else {
        base.compression_method(CompressionMethod::Deflated).compression_level(Some(DEFLATE_LEVEL))
    }
}

/// Writes entries in the given order into a new archive.
pub(crate) fn write(entries: &[Entry<'_>]) -> Result<Vec<u8>, FormatError> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for e in entries {
        zip.start_file(e.name, options(e.stored)).map_err(|err| FormatError::Serialize(err.to_string()))?;
        zip.write_all(e.data).map_err(|err| FormatError::Serialize(err.to_string()))?;
    }
    let cursor = zip.finish().map_err(|err| FormatError::Serialize(err.to_string()))?;
    Ok(cursor.into_inner())
}

/// Whether bytes start like a ZIP archive (a local file header, or the end record of an empty archive).
fn looks_like_zip(start: &[u8]) -> bool {
    start.starts_with(b"PK\x03\x04") || start.starts_with(b"PK\x05\x06")
}

/// An archive opened for reading.
pub(crate) struct Archive<R: Read + Seek> {
    zip: ZipArchive<R>,
}

impl<'a> Archive<Cursor<&'a [u8]>> {
    /// Opens an archive in memory, distinguishing "not a ZIP at all" from "a damaged ZIP".
    pub(crate) fn open(bytes: &'a [u8]) -> Result<Self, FormatError> {
        if !looks_like_zip(bytes) {
            return Err(FormatError::NotAnArchive);
        }
        let zip =
            ZipArchive::new(Cursor::new(bytes)).map_err(|e| FormatError::CorruptArchive(e.to_string()))?;
        Ok(Archive { zip })
    }
}

impl<R: Read + Seek> Archive<R> {
    /// Opens an archive from a seekable reader (a file), reading only its directory.
    pub(crate) fn open_reader(mut reader: R) -> Result<Self, FormatError> {
        let mut start = [0u8; 4];
        let n = reader.read(&mut start).map_err(|e| FormatError::CorruptArchive(e.to_string()))?;
        if !looks_like_zip(start.get(..n).unwrap_or_default()) {
            return Err(FormatError::NotAnArchive);
        }
        reader.seek(SeekFrom::Start(0)).map_err(|e| FormatError::CorruptArchive(e.to_string()))?;
        let zip = ZipArchive::new(reader).map_err(|e| FormatError::CorruptArchive(e.to_string()))?;
        Ok(Archive { zip })
    }

    /// Names of all entries, in archive order.
    pub(crate) fn names(&self) -> Vec<String> {
        self.zip.file_names().filter_map(Result::ok).map(|n| n.into_owned()).collect()
    }

    /// Reads an entry, or `None` if the archive has no entry of that name.
    pub(crate) fn read(&mut self, name: &str) -> Result<Option<Vec<u8>>, FormatError> {
        let mut file = match self.zip.by_name(name) {
            Ok(f) => f,
            Err(zip::result::ZipError::FileNotFound) => return Ok(None),
            Err(e) => return Err(FormatError::CorruptArchive(format!("{name}: {e}"))),
        };
        let declared = file.size();
        if declared > MAX_ENTRY_BYTES {
            return Err(FormatError::TooLarge { entry: name.to_owned(), size: declared });
        }
        let capacity = usize::try_from(declared.min(64 << 20)).unwrap_or(0);
        let mut buf = Vec::with_capacity(capacity);
        // Never trust the declared size: read at most one byte past the limit.
        (&mut file)
            .take(MAX_ENTRY_BYTES + 1)
            .read_to_end(&mut buf)
            .map_err(|e| FormatError::CorruptArchive(format!("{name}: {e}")))?;
        if buf.len() as u64 > MAX_ENTRY_BYTES {
            return Err(FormatError::TooLarge { entry: name.to_owned(), size: buf.len() as u64 });
        }
        Ok(Some(buf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archives_are_reproducible_and_ordered() {
        let entries = [
            Entry { name: MANIFEST_ENTRY, data: b"{}", stored: false },
            Entry { name: MODEL_ENTRY, data: b"{\"a\": 1}", stored: false },
        ];
        let a = write(&entries).unwrap();
        let b = write(&entries).unwrap();
        assert_eq!(a, b);
        let mut ar = Archive::open(&a).unwrap();
        assert_eq!(ar.names(), vec![MANIFEST_ENTRY.to_owned(), MODEL_ENTRY.to_owned()]);
        assert_eq!(ar.read(MODEL_ENTRY).unwrap().unwrap(), b"{\"a\": 1}");
        assert!(ar.read("missing").unwrap().is_none());
    }

    #[test]
    fn non_zip_bytes_are_recognised() {
        assert!(matches!(Archive::open(b"hello"), Err(FormatError::NotAnArchive)));
        assert!(matches!(Archive::open(b"PK\x03\x04garbage"), Err(FormatError::CorruptArchive(_))));
    }
}
