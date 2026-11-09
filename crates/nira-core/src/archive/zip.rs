//! CBZ / ZIP support via the pure-Rust `zip` crate.

use std::fs::File;
use std::path::{Path, PathBuf};

use super::{read_capped, sorted_pages, too_large, Archive, ArchiveError, MAX_ENTRY_BYTES};

pub struct ZipArchive {
    path: PathBuf,
    name: String,
    archive: zip::ZipArchive<File>,
    pages: Option<Vec<String>>,
}

impl ZipArchive {
    pub fn open(path: &Path) -> Result<ZipArchive, ArchiveError> {
        let file = File::open(path)?;
        let archive = zip::ZipArchive::new(file)
            .map_err(|e| ArchiveError::Other(format!("not a valid ZIP archive: {e}")))?;
        Ok(ZipArchive {
            path: path.to_path_buf(),
            name: path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string()),
            archive,
            pages: None,
        })
    }
}

impl Archive for ZipArchive {
    fn name(&self) -> &str {
        &self.name
    }

    fn try_clone(&self) -> Option<Box<dyn Archive>> {
        // Its own file handle: two threads seeking one shared handle would
        // read each other's data. Reading the central directory again is cheap.
        let archive = zip::ZipArchive::new(File::open(&self.path).ok()?).ok()?;
        Some(Box::new(ZipArchive {
            path: self.path.clone(),
            name: self.name.clone(),
            archive,
            pages: self.pages.clone(),
        }))
    }

    fn raw_names(&mut self) -> Result<Vec<String>, ArchiveError> {
        Ok(self
            .archive
            .file_names()
            .map(|s| s.to_string())
            .filter(|n| !n.ends_with('/'))
            .collect())
    }

    fn page_names(&mut self) -> Result<Vec<String>, ArchiveError> {
        if self.pages.is_none() {
            let names = self.raw_names()?;
            self.pages = Some(sorted_pages(names));
        }
        Ok(self.pages.clone().unwrap_or_default())
    }

    fn read(&mut self, name: &str) -> Result<Vec<u8>, ArchiveError> {
        let mut file = self.archive.by_name(name).map_err(|e| {
            ArchiveError::Other(format!(
                "cannot read '{name}' from '{}': {e}",
                self.path.display()
            ))
        })?;
        // The declared size is checked first, and the read is capped too,
        // since a crafted entry may declare less than it inflates to.
        let what = format!("'{name}'");
        if file.size() > MAX_ENTRY_BYTES {
            return Err(too_large(&what, MAX_ENTRY_BYTES));
        }
        read_capped(&mut file, MAX_ENTRY_BYTES, &what)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn archive_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("test")
            .join("files")
            .join("archives")
            .join(name)
    }

    #[test]
    fn lists_and_reads_zip() {
        let path = archive_path("01-ZIP-Normal.zip");
        if !path.exists() {
            eprintln!("skipping: test archive not present");
            return;
        }
        let mut ar = ZipArchive::open(&path).expect("open zip");
        let pages = ar.page_names().expect("list pages");
        assert!(!pages.is_empty(), "expected at least one image page");
        let bytes = ar.read(&pages[0]).expect("read first page");
        assert!(image::guess_format(&bytes).is_ok());
    }
}
