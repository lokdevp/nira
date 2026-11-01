//! Bookmarks, persisted as JSON in the data directory. Changing the list
//! does not write it: call `save` (the app does so off the UI thread).

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bookmark {
    pub name: String,
    pub path: String,
    /// 1-based.
    pub page: u32,
    pub numpages: u32,
    pub archive_type: Option<String>,
    pub date_added: u64, // Unix epoch seconds
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Bookmarks {
    pub items: Vec<Bookmark>,
}

impl Bookmarks {
    pub fn path() -> PathBuf {
        crate::paths::data_dir().join("bookmarks.json")
    }

    pub fn load() -> Bookmarks {
        let path = Self::path();
        match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => Bookmarks::default(),
        }
    }

    pub fn save(&self) {
        match serde_json::to_string_pretty(self) {
            Ok(text) => crate::paths::write_config(&Self::path(), &text),
            Err(e) => log::warn!("cannot serialize bookmarks: {e}"),
        }
    }

    pub fn contains(&self, path: &str, page: u32) -> bool {
        self.items.iter().any(|b| b.path == path && b.page == page)
    }

    /// Add `b`, or remove the bookmark on its page if there is one. True if
    /// it was added.
    pub fn toggle(&mut self, b: Bookmark) -> bool {
        if self.contains(&b.path, b.page) {
            self.remove(&b.path, b.page);
            false
        } else {
            self.items.push(b);
            true
        }
    }

    pub fn remove(&mut self, path: &str, page: u32) {
        self.items.retain(|b| !(b.path == path && b.page == page));
    }

    pub fn remove_path(&mut self, path: &str) {
        self.items.retain(|b| b.path != path);
    }

    /// The bookmarks of one book, by page.
    pub fn same_path(&self, path: &str) -> Vec<&Bookmark> {
        let mut v: Vec<&Bookmark> = self.items.iter().filter(|b| b.path == path).collect();
        v.sort_by_key(|b| b.page);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark(path: &str, page: u32) -> Bookmark {
        Bookmark {
            name: "test".into(),
            path: path.into(),
            page,
            numpages: 20,
            archive_type: Some("ZIP".into()),
            date_added: 1783987200,
        }
    }

    #[test]
    fn roundtrip_json() {
        let mut b = Bookmarks::default();
        b.toggle(mark("/tmp/x.cbz", 5));
        let text = serde_json::to_string(&b).unwrap();
        let back: Bookmarks = serde_json::from_str(&text).unwrap();
        assert_eq!(back.items.len(), 1);
        assert_eq!(back.items[0].page, 5);
        assert_eq!(back.same_path("/tmp/x.cbz").len(), 1);
    }

    #[test]
    fn toggle_adds_then_removes_and_lists_by_page() {
        let mut b = Bookmarks::default();
        assert!(b.toggle(mark("/a.cbz", 9)));
        assert!(b.toggle(mark("/a.cbz", 2)));
        assert!(b.toggle(mark("/b.cbz", 9)));
        let pages: Vec<u32> = b.same_path("/a.cbz").iter().map(|m| m.page).collect();
        assert_eq!(pages, vec![2, 9]);
        assert!(!b.toggle(mark("/a.cbz", 9)));
        assert!(!b.contains("/a.cbz", 9));
        assert!(b.contains("/b.cbz", 9));
    }
}
