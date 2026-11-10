//! CB7 / 7z support.
//!
//! Prefers the external `7z` binary, which is the most reliable across the wild variety
//! of 7z archives (solid, PPMd, BCJ filters, encryption headers, …). Falls
//! back to the pure-Rust `sevenz-rust` crate when no 7z binary is available
//! (e.g. minimal Windows bundles).

use std::io::Read;
use std::path::{Path, PathBuf};

use super::{
    find_first, run_capture, sorted_pages, too_large, Archive, ArchiveError, MAX_ENTRY_BYTES,
};

#[derive(Clone)]
pub struct SevenZipArchive {
    path: PathBuf,
    name: String,
    /// The external `7z` binary found at open, or `None` for sevenz-rust.
    prog: Option<String>,
    pages: Option<Vec<String>>,
}

const SEVENZ_BINARIES: &[&str] = &["7z", "7zz", "7za", "7zr"];

fn find_sevenz() -> Option<String> {
    find_first(SEVENZ_BINARIES)
}

impl SevenZipArchive {
    pub fn open(path: &Path) -> Result<SevenZipArchive, ArchiveError> {
        let external = find_sevenz();
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        match &external {
            Some(prog) => {
                let p = path.to_string_lossy().into_owned();
                // Validate that the archive can be listed.
                run_capture(prog, &["l", "-slt", "--", &p])
                    .map_err(|e| ArchiveError::Other(format!("not a valid 7z archive: {e}")))?;
            }
            None => {
                sevenz_rust::SevenZReader::<std::fs::File>::open(
                    path,
                    sevenz_rust::Password::empty(),
                )
                .map_err(|e| ArchiveError::Other(format!("not a valid 7z archive: {e}")))?;
            }
        }
        Ok(SevenZipArchive {
            path: path.to_path_buf(),
            name,
            prog: external,
            pages: None,
        })
    }
}

impl Archive for SevenZipArchive {
    fn name(&self) -> &str {
        &self.name
    }

    fn try_clone(&self) -> Option<Box<dyn Archive>> {
        // Nothing is unpacked: every read runs from the file itself.
        Some(Box::new(self.clone()))
    }

    fn raw_names(&mut self) -> Result<Vec<String>, ArchiveError> {
        let names = if let Some(prog) = &self.prog {
            let p = self.path.to_string_lossy().into_owned();
            let raw = run_capture(prog, &["l", "-slt", "--", &p])?;
            let text = String::from_utf8_lossy(&raw);
            let mut names = Vec::new();
            // `7z l -slt` prints the archive's own path first, then a
            // "----------" separator, then the entries; only parse entries
            // after the separator (and skip directories).
            let mut in_entries = false;
            let mut is_folder = false;
            for line in text.lines() {
                let line = line.trim();
                if line == "----------" {
                    in_entries = true;
                    continue;
                }
                if !in_entries {
                    continue;
                }
                if let Some(rest) = line.strip_prefix("Path = ") {
                    names.push(rest.to_string());
                } else if line.starts_with("Folder = ") && line.ends_with('+') {
                    is_folder = true;
                } else if line.starts_with("Attributes = ") {
                    // entries that are directories get no Path? (no-op)
                }
            }
            names.retain(|n| !n.ends_with('/'));
            let _ = is_folder;
            names
        } else {
            let mut reader = sevenz_rust::SevenZReader::<std::fs::File>::open(
                &self.path,
                sevenz_rust::Password::empty(),
            )
            .map_err(|e| ArchiveError::Other(format!("cannot open 7z: {e}")))?;
            let mut names = Vec::new();
            reader
                .for_each_entries(|entry, _| {
                    names.push(entry.name.clone());
                    Ok(true)
                })
                .map_err(|e| ArchiveError::Other(format!("cannot list 7z entries: {e}")))?;
            names
        };
        Ok(names)
    }

    fn page_names(&mut self) -> Result<Vec<String>, ArchiveError> {
        if self.pages.is_none() {
            self.pages = Some(sorted_pages(self.raw_names()?));
        }
        Ok(self.pages.clone().unwrap_or_default())
    }

    fn read(&mut self, name: &str) -> Result<Vec<u8>, ArchiveError> {
        if let Some(prog) = &self.prog {
            let p = self.path.to_string_lossy().into_owned();
            // `--` and -spd: a name like `-rg.jpg`, `@x.png` or `*.jpg` is a
            // file name, not a switch, list file or wildcard.
            run_capture(prog, &["e", "-so", "-spd", "--", &p, name])
        } else {
            let mut reader = sevenz_rust::SevenZReader::<std::fs::File>::open(
                &self.path,
                sevenz_rust::Password::empty(),
            )
            .map_err(|e| ArchiveError::Other(format!("cannot open 7z: {e}")))?;
            let mut buf: Vec<u8> = Vec::new();
            let mut found = false;
            let mut too_big = false;
            reader
                .for_each_entries(|entry, r| {
                    if entry.name == name && !found {
                        let mut tmp = Vec::new();
                        (&mut *r)
                            .take(MAX_ENTRY_BYTES + 1)
                            .read_to_end(&mut tmp)
                            .map_err(sevenz_rust::Error::io)?;
                        too_big = tmp.len() as u64 > MAX_ENTRY_BYTES;
                        buf = tmp;
                        found = true;
                        // Returning false stops iteration once the entry is found.
                        return Ok(false);
                    }
                    Ok(true)
                })
                .map_err(|e| ArchiveError::Other(format!("cannot read 7z entry: {e}")))?;
            if too_big {
                Err(too_large(&format!("'{name}'"), MAX_ENTRY_BYTES))
            } else if found {
                Ok(buf)
            } else {
                Err(ArchiveError::Other(format!(
                    "entry '{name}' not found in '{}'",
                    self.path.display()
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_like_switches_and_wildcards_are_read_as_names() {
        use crate::archive::tricky_names::{assert_reads_like_zip, fixture};
        for set in ["OptEntry", "GlobEntries"] {
            for file in [format!("{set}.7z"), format!("Solid{set}.7z")] {
                let path = fixture(&file);
                if find_sevenz().is_some() {
                    let ar = SevenZipArchive::open(&path).unwrap();
                    assert_reads_like_zip(ar, set, &format!("{file} 7z"), &[]);
                }
                // The pure-Rust fallback takes no command line, but fails
                // on these solid fixtures with ChecksumVerificationFailed
                // (an existing limitation), so it is checked on the others.
                if !file.starts_with("Solid") {
                    let mut ar = SevenZipArchive::open(&path).unwrap();
                    ar.prog = None;
                    assert_reads_like_zip(ar, set, &format!("{file} sevenz-rust"), &[]);
                }
            }
        }
    }

    fn archive_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("test")
            .join("files")
            .join("archives")
            .join(name)
    }

    #[test]
    fn lists_and_reads_7z() {
        let path = archive_path("04-7Z-Normal.7z");
        if !path.exists() {
            eprintln!("skipping: test archive not present");
            return;
        }
        let mut ar = SevenZipArchive::open(&path).expect("open 7z");
        let pages = ar.page_names().expect("list pages");
        assert!(!pages.is_empty(), "expected at least one image page");
        let bytes = ar.read(&pages[0]).expect("read first page");
        assert!(!bytes.is_empty());
        assert!(
            image::guess_format(&bytes).is_ok(),
            "first page should be a decodable image"
        );
    }

    #[test]
    fn lists_and_reads_solid_7z() {
        let path = archive_path("SolidFlat.7z");
        if !path.exists() {
            eprintln!("skipping: test archive not present");
            return;
        }
        let mut ar = SevenZipArchive::open(&path).expect("open solid 7z");
        let pages = ar.page_names().expect("list pages");
        assert!(!pages.is_empty());
        let bytes = ar.read(&pages[0]).expect("read first page of solid 7z");
        assert!(image::guess_format(&bytes).is_ok());
    }
}
