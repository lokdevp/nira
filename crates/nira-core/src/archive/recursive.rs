//! Transparently handle archives embedded inside archives. Embedded
//! archives are extracted to a folder in this run's unpack directory
//! (`paths::unpack_dir`), opened recursively, and their pages are exposed
//! with a path prefix (the embedded archive's entry name).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use super::{open, sorted_pages, Archive, ArchiveError, TempPath};

/// Extensions considered embedded-archive candidates (mirrors
/// `archive_tools.is_archive_file`).
const SUBARCHIVE_EXTENSIONS: &[&str] = &[
    "zip", "cbz", "rar", "cbr", "7z", "cb7", "tar", "cbt", "gz", "bz2", "xz", "lha", "lzh", "pdf",
];

fn is_subarchive(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    match lower.rsplit('.').next() {
        Some(ext) => SUBARCHIVE_EXTENSIONS.contains(&ext),
        None => false,
    }
}

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct RecursiveArchive {
    name: String,
    /// All archives: index 0 is the main one; later entries are embedded
    /// archives (themselves possibly recursive).
    archives: Vec<Box<dyn Archive>>,
    /// Display name -> (archive index, entry name inside that archive).
    map: HashMap<String, (usize, String)>,
    /// Where embedded archives were extracted. Shared with clones
    /// (`try_clone`); removed with the last one. Declared after `archives`,
    /// so their file handles are dropped before the folder is removed.
    tmp_dir: Option<Arc<TempPath>>,
    listed: bool,
    pages: Vec<String>,
}

impl RecursiveArchive {
    pub fn new(main: Box<dyn Archive>) -> RecursiveArchive {
        let name = main.name().to_string();
        let archives: Vec<Box<dyn Archive>> = vec![main];
        RecursiveArchive {
            name,
            archives,
            map: HashMap::new(),
            tmp_dir: None,
            listed: false,
            pages: Vec::new(),
        }
    }

    fn ensure_tmp(&mut self) -> PathBuf {
        if self.tmp_dir.is_none() {
            let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = crate::paths::unpack_dir().join(format!("sub-{n}"));
            if let Err(e) = std::fs::create_dir_all(&dir) {
                log::warn!("cannot create {}: {e}", dir.display());
            }
            self.tmp_dir = Some(Arc::new(TempPath(dir.clone())));
            dir
        } else {
            self.tmp_dir.as_ref().map(|t| t.0.clone()).unwrap()
        }
    }

    /// List all pages, expanding embedded archives. Returns display names.
    fn do_list(&mut self) -> Result<Vec<String>, ArchiveError> {
        let mut out: Vec<String> = Vec::new();
        // Only the main archive is listed here. Embedded archives are opened
        // through `open`, which wraps them in their own RecursiveArchive, so
        // their `page_names` already expand any deeper nesting; listing them
        // again here would add their pages a second time without a prefix.
        let entries = self.archives[0].raw_names()?;
        let mut subs: Vec<String> = Vec::new();
        for entry in &entries {
            if is_subarchive(entry) {
                subs.push(entry.clone());
            } else {
                out.push(entry.clone());
                self.map.insert(entry.clone(), (0, entry.clone()));
            }
        }
        // Open embedded archives after listing the main one.
        for entry in subs {
            let bytes = match self.archives[0].read(&entry) {
                Ok(b) => b,
                Err(e) => {
                    log::warn!("cannot read embedded archive '{}': {e}", entry);
                    continue;
                }
            };
            let ext = entry
                .rsplit('.')
                .next()
                .unwrap_or("arc")
                .to_ascii_lowercase();
            // Made on first use: most books have no embedded archives.
            let tmp = self.ensure_tmp();
            let path = tmp.join(format!("sub-{:04}.{ext}", self.archives.len()));
            if let Err(e) = std::fs::write(&path, &bytes) {
                log::warn!("cannot write embedded archive {:?}: {e}", path);
                continue;
            }
            let sub = match open(&path) {
                Ok(a) => a,
                Err(e) => {
                    log::warn!("cannot open embedded archive '{}': {e}", entry);
                    continue;
                }
            };
            let sub_idx = self.archives.len();
            self.archives.push(sub);
            // List the sub-archive's pages (this also expands any nested
            // archives inside it).
            let sub_pages = match self.archives[sub_idx].page_names() {
                Ok(p) => p,
                Err(e) => {
                    log::warn!("cannot list embedded archive '{}': {e}", entry);
                    continue;
                }
            };
            for sp in sub_pages {
                let display = format!("{entry}/{sp}");
                out.push(display.clone());
                self.map.insert(display, (sub_idx, sp));
            }
        }
        // Natural sort of the flat display list (sub-archive groups sort by
        // their prefix).
        let mut sorted = sorted_pages(out);
        // sorted_pages filters images; embedded archives are already expanded
        // so nothing extra is lost. Keep only what was mapped.
        sorted.retain(|n| self.map.contains_key(n));
        Ok(sorted)
    }
}

impl Archive for RecursiveArchive {
    fn name(&self) -> &str {
        &self.name
    }

    fn try_clone(&self) -> Option<Box<dyn Archive>> {
        let archives = self
            .archives
            .iter()
            .map(|a| a.try_clone())
            .collect::<Option<Vec<_>>>()?;
        Some(Box::new(RecursiveArchive {
            name: self.name.clone(),
            archives,
            map: self.map.clone(),
            tmp_dir: self.tmp_dir.clone(),
            listed: self.listed,
            pages: self.pages.clone(),
        }))
    }

    fn raw_names(&mut self) -> Result<Vec<String>, ArchiveError> {
        self.page_names()
    }

    fn page_names(&mut self) -> Result<Vec<String>, ArchiveError> {
        if !self.listed {
            self.pages = self.do_list()?;
            self.listed = true;
        }
        Ok(self.pages.clone())
    }

    fn read(&mut self, name: &str) -> Result<Vec<u8>, ArchiveError> {
        if !self.listed {
            self.page_names()?;
        }
        let (idx, entry) = self
            .map
            .get(name)
            .ok_or_else(|| ArchiveError::Other(format!("entry '{name}' not found")))?;
        let idx = *idx;
        let entry = entry.clone();
        self.archives[idx].read(&entry)
    }

    fn close(&mut self) {
        for a in &mut self.archives {
            a.close();
        }
        // The folder goes once no clone uses it (`TempPath`).
        self.tmp_dir = None;
    }
}

impl Drop for RecursiveArchive {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archive_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("test")
            .join("files")
            .join("archives")
            .join(name)
    }

    #[test]
    fn clones_share_the_extracted_archives() {
        // red_and_blues.tar holds red_and_blues.7z and red_and_blues.rar,
        // each with red.png and a further embedded archive of blues.
        let path = archive_path("red_and_blues.tar");
        let mut ar = RecursiveArchive::new(crate::archive::open_plain(&path).unwrap());
        let pages = ar.page_names().unwrap();
        assert!(
            pages.iter().any(|p| p.ends_with("/red.png")),
            "embedded pages are listed: {pages:?}"
        );
        let dir = ar
            .tmp_dir
            .as_ref()
            .expect("blues.7z was extracted")
            .0
            .clone();
        let mut twin = ar.try_clone().expect("every backend clones");
        assert_eq!(twin.page_names().unwrap(), pages);
        drop(ar);
        assert!(dir.is_dir(), "kept while the clone uses it");
        for page in &pages {
            let bytes = twin.read(page).unwrap();
            assert!(image::guess_format(&bytes).is_ok(), "{page}");
        }
        drop(twin);
        assert!(!dir.exists(), "removed with the last handle");
    }

    #[test]
    fn expands_embedded_archives() {
        // embedded_red_and_blues_rar.rar contains red_and_blues.rar, which
        // contains red.png + blues.7z (itself containing blue0..2.png).
        let path = archive_path("embedded_red_and_blues_rar.rar");
        if !path.exists() {
            eprintln!("skipping: test archive not present");
            return;
        }
        if crate::archive::detect(&path).is_none() {
            eprintln!("skipping: no backend for this archive");
            return;
        }
        let mut ar = crate::archive::open(&path).expect("open recursive archive");
        let pages = ar.page_names().expect("list pages");
        eprintln!("pages: {pages:?}");
        // Every page exactly once, prefixed by the embedded archive(s) it
        // came from.
        let mut got = pages.clone();
        got.sort();
        assert_eq!(
            got,
            [
                "red_and_blues.rar/blues.7z/blue0.png",
                "red_and_blues.rar/blues.7z/blue1.png",
                "red_and_blues.rar/blues.7z/blue2.png",
                "red_and_blues.rar/red.png",
            ]
        );
        // Reading the embedded page works through the mapping.
        let blue = pages
            .iter()
            .find(|p| p.ends_with("blue0.png"))
            .unwrap()
            .clone();
        let bytes = ar.read(&blue).expect("read embedded page");
        assert!(image::guess_format(&bytes).is_ok());
    }
}
