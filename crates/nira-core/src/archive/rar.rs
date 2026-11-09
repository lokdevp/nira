//! RAR / CBR support via external tools (`unrar`, or `7z` as fallback).
//!
//! A pure-Rust/FFI unrar backend can be added later behind a cargo feature.

use std::path::{Path, PathBuf};

use super::{find_first, run_capture, sorted_pages, which, Archive, ArchiveError};

#[derive(Clone)]
pub struct RarArchive {
    path: PathBuf,
    name: String,
    /// First available backend: "unrar" or "7z".
    backend: String,
    /// Executable to run for the backend ("unrar", "7z" or "7zz").
    prog: String,
    pages: Option<Vec<String>>,
}

impl RarArchive {
    pub fn open(path: &Path) -> Result<RarArchive, ArchiveError> {
        let (backend, prog) = if which("unrar").is_some() {
            ("unrar".to_string(), "unrar".to_string())
        } else if let Some(prog) = find_first(&["7z", "7zz"]) {
            ("7z".to_string(), prog)
        } else {
            return Err(ArchiveError::Other(
                "CBR/RAR support requires the 'unrar' or '7z' executable to be installed"
                    .to_string(),
            ));
        };
        Ok(RarArchive {
            path: path.to_path_buf(),
            name: path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string()),
            backend,
            prog,
            pages: None,
        })
    }

    fn list_raw(&self) -> Result<Vec<String>, ArchiveError> {
        let path = self.path.to_string_lossy().into_owned();
        let raw = match self.backend.as_str() {
            // NOTE: do NOT pass -inul to `lb`: on recent RARLAB unrar it also
            // suppresses the listing output itself, yielding an empty list.
            "unrar" => run_capture("unrar", &["lb", "-p-", &path])?,
            _ => run_capture(&self.prog, &["l", "-slt", &path])?,
        };
        let text = String::from_utf8_lossy(&raw);
        let mut names = Vec::new();
        if self.backend == "7z" {
            // `7z l -slt`: skip the archive header (own path) and only parse
            // "Path = x" lines after the "----------" separator.
            let mut in_entries = false;
            for line in text.lines() {
                let line = line.trim();
                if line == "----------" {
                    in_entries = true;
                    continue;
                }
                if in_entries {
                    if let Some(rest) = line.strip_prefix("Path = ") {
                        names.push(rest.to_string());
                    }
                }
            }
        } else {
            for line in text.lines() {
                let line = line.trim();
                if !line.is_empty() {
                    names.push(line.to_string());
                }
            }
        }
        Ok(names)
    }
}

impl Archive for RarArchive {
    fn name(&self) -> &str {
        &self.name
    }

    fn try_clone(&self) -> Option<Box<dyn Archive>> {
        // Nothing is unpacked: every read runs from the file itself.
        Some(Box::new(self.clone()))
    }

    fn raw_names(&mut self) -> Result<Vec<String>, ArchiveError> {
        self.list_raw()
    }

    fn page_names(&mut self) -> Result<Vec<String>, ArchiveError> {
        if self.pages.is_none() {
            let names = self.raw_names()?;
            self.pages = Some(sorted_pages(names));
        }
        Ok(self.pages.clone().unwrap_or_default())
    }

    fn read(&mut self, name: &str) -> Result<Vec<u8>, ArchiveError> {
        let path = self.path.to_string_lossy().into_owned();
        match self.backend.as_str() {
            "unrar" => run_capture("unrar", &["p", "-p-", &path, name]),
            _ => run_capture(&self.prog, &["e", "-so", &path, name]),
        }
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
    fn lists_and_reads_rar() {
        if which("unrar").is_none() && find_first(&["7z", "7zz"]).is_none() {
            eprintln!("skipping: neither unrar nor 7z available");
            return;
        }
        let path = archive_path("03-RAR-Normal.rar");
        if !path.exists() {
            eprintln!("skipping: test archive not present");
            return;
        }
        let mut ar = RarArchive::open(&path).expect("open rar");
        let pages = ar.page_names().expect("list pages");
        assert_eq!(pages.len(), 4);
        assert_eq!(pages[0], "images/01-JPG-Indexed.jpg");
        let bytes = ar.read(&pages[0]).expect("read first page");
        assert!(
            bytes.starts_with(&[0xff, 0xd8, 0xff, 0xe0]),
            "expected JPEG magic"
        );
    }
}
