//! The previous and next book: the neighbouring archive in the same folder
//! (natural order), and past the folder's ends, the neighbouring folder.

use std::fs;
use std::path::{Path, PathBuf};

use crate::archive;
use crate::natsort::natural_cmp;

/// Entries of `dir` that `keep` accepts, naturally sorted.
fn sorted_entries(dir: &Path, keep: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    let mut items: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| keep(p)).collect())
        .unwrap_or_default();
    items.sort_by(|a, b| natural_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
    items
}

fn is_archive(p: &Path) -> bool {
    p.is_file() && archive::detect(p).is_some()
}

/// A folder of images (readable as a book).
fn has_images(dir: &Path) -> bool {
    fs::read_dir(dir)
        .map(|rd| {
            rd.flatten().any(|e| {
                e.path().is_file() && archive::is_image_file(&e.file_name().to_string_lossy())
            })
        })
        .unwrap_or(false)
}

/// The item after (or before) `current` in `items`.
fn neighbour(items: &[PathBuf], current: &Path, forward: bool) -> Option<PathBuf> {
    let pos = items.iter().position(|p| p == current)?;
    let target = if forward {
        pos.checked_add(1)?
    } else {
        pos.checked_sub(1)?
    };
    items.get(target).cloned()
}

/// The book in folder `dir` to continue with: its first archive going
/// forward (last going back), or the folder itself if it holds images.
fn book_in(dir: &Path, forward: bool) -> Option<PathBuf> {
    let archives = sorted_entries(dir, is_archive);
    let pick = if forward {
        archives.first()
    } else {
        archives.last()
    };
    pick.cloned()
        .or_else(|| has_images(dir).then(|| dir.to_path_buf()))
}

/// The next (`forward`) or previous book after `current`, an archive or a
/// folder of images. With `folders`, the search continues into the
/// neighbouring folders once this folder's books run out.
pub fn sibling_book(current: &Path, forward: bool, folders: bool) -> Option<PathBuf> {
    let parent = current.parent()?;
    if current.is_dir() {
        // A folder book: its sibling folders that hold a book.
        let dirs = sorted_entries(parent, |p| p.is_dir());
        let pos = dirs.iter().position(|p| p == current)?;
        let mut rest: Box<dyn Iterator<Item = &PathBuf>> = if forward {
            Box::new(dirs[pos + 1..].iter())
        } else {
            Box::new(dirs[..pos].iter().rev())
        };
        return rest.find(|d| has_images(d)).cloned();
    }
    let archives = sorted_entries(parent, is_archive);
    if let Some(next) = neighbour(&archives, current, forward) {
        return Some(next);
    }
    if !folders {
        return None;
    }
    // Past this folder's ends: the neighbouring folders, nearest first.
    let grandparent = parent.parent()?;
    let dirs = sorted_entries(grandparent, |p| p.is_dir());
    let pos = dirs.iter().position(|p| p == parent)?;
    let mut rest: Box<dyn Iterator<Item = &PathBuf>> = if forward {
        Box::new(dirs[pos + 1..].iter())
    } else {
        Box::new(dirs[..pos].iter().rev())
    };
    rest.find_map(|d| book_in(d, forward))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tree of fixture copies: series/vol1/{ch1,ch2,ch10}.cbz,
    /// series/vol2/ch1.cbz, series/vol3 (empty), series/vol4/a.png.
    fn tree() -> PathBuf {
        let root = std::env::temp_dir().join(format!("nira-siblings-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let zip = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../test/files/archives/01-ZIP-Normal.zip");
        for (dir, files) in [
            ("vol1", &["ch1.cbz", "ch2.cbz", "ch10.cbz"][..]),
            ("vol2", &["ch1.cbz"][..]),
            ("vol3", &[][..]),
        ] {
            fs::create_dir_all(root.join(dir)).unwrap();
            for f in files {
                fs::copy(&zip, root.join(dir).join(f)).unwrap();
            }
        }
        fs::create_dir_all(root.join("vol4")).unwrap();
        fs::write(root.join("vol4/a.png"), b"not really a png").unwrap();
        fs::write(root.join("vol1/notes.txt"), b"skip me").unwrap();
        root
    }

    #[test]
    fn walks_archives_in_natural_order_then_folders() {
        let root = tree();
        let v1 = root.join("vol1");
        let next = |p: &Path| sibling_book(p, true, true);
        let prev = |p: &Path| sibling_book(p, false, true);
        assert_eq!(next(&v1.join("ch2.cbz")), Some(v1.join("ch10.cbz")));
        assert_eq!(prev(&v1.join("ch10.cbz")), Some(v1.join("ch2.cbz")));
        // Past the folder: the next folder's first book.
        assert_eq!(next(&v1.join("ch10.cbz")), Some(root.join("vol2/ch1.cbz")));
        // Going back into a folder lands on its last book.
        assert_eq!(prev(&root.join("vol2/ch1.cbz")), Some(v1.join("ch10.cbz")));
        // Empty folders are skipped; a folder of images is a book.
        assert_eq!(next(&root.join("vol2/ch1.cbz")), Some(root.join("vol4")));
        assert_eq!(prev(&v1.join("ch1.cbz")), None);
        // Without folders, the folder's ends are the ends.
        assert_eq!(sibling_book(&v1.join("ch10.cbz"), true, false), None);
        fs::remove_dir_all(&root).unwrap();
    }
}
