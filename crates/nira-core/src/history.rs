//! Reading history: where each recently read book was left, newest first.
//! It drives resuming a book at its last page and the start screen's
//! "Continue reading" list. Stored as `history.json` in the data directory.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Books kept, most recent first.
pub const MAX_ENTRIES: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub path: String,
    /// Last page read, 1-based.
    pub page: u32,
    /// The book's page count when it was last read.
    pub total: u32,
    /// Unix seconds of the last read.
    pub time: i64,
}

impl Entry {
    /// Read to the last page.
    pub fn finished(&self) -> bool {
        self.total > 0 && self.page >= self.total
    }

    /// The 0-based page to reopen the book at: where it was left, or the
    /// start once finished.
    pub fn resume_page(&self) -> usize {
        if self.finished() {
            0
        } else {
            self.page.saturating_sub(1) as usize
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct History {
    pub items: Vec<Entry>,
}

impl History {
    pub fn path() -> PathBuf {
        crate::paths::data_dir().join("history.json")
    }

    pub fn load() -> History {
        match fs::read_to_string(Self::path()) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => History::default(),
        }
    }

    pub fn save(&self) {
        match serde_json::to_string_pretty(self) {
            Ok(text) => crate::paths::write_config(&Self::path(), &text),
            Err(e) => log::warn!("cannot serialize the reading history: {e}"),
        }
    }

    pub fn get(&self, path: &str) -> Option<&Entry> {
        self.items.iter().find(|e| e.path == path)
    }

    /// Move `path` to the front with its new position (adding it if new).
    pub fn record(&mut self, path: &str, page: u32, total: u32, time: i64) {
        self.items.retain(|e| e.path != path);
        self.items.insert(
            0,
            Entry {
                path: path.to_string(),
                page,
                total,
                time,
            },
        );
        self.items.truncate(MAX_ENTRIES);
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_moves_to_front_and_caps() {
        let mut h = History::default();
        for i in 0..25 {
            h.record(&format!("/b{i}.cbz"), 1, 10, i);
        }
        assert_eq!(h.items.len(), MAX_ENTRIES);
        assert_eq!(h.items[0].path, "/b24.cbz");
        h.record("/b10.cbz", 7, 10, 100);
        assert_eq!(h.items[0].path, "/b10.cbz");
        assert_eq!(h.get("/b10.cbz").unwrap().page, 7);
        assert_eq!(h.items.iter().filter(|e| e.path == "/b10.cbz").count(), 1);
    }

    #[test]
    fn finished_books_resume_at_the_start() {
        let mut h = History::default();
        h.record("/a.cbz", 7, 10, 0);
        h.record("/b.cbz", 10, 10, 0);
        assert_eq!(h.get("/a.cbz").unwrap().resume_page(), 6);
        assert!(h.get("/b.cbz").unwrap().finished());
        assert_eq!(h.get("/b.cbz").unwrap().resume_page(), 0);
    }

    #[test]
    fn round_trips_json() {
        let mut h = History::default();
        h.record("/a.cbz", 3, 41, 1_793_000_000);
        let text = serde_json::to_string(&h).unwrap();
        let back: History = serde_json::from_str(&text).unwrap();
        assert_eq!(back.items, h.items);
    }
}
