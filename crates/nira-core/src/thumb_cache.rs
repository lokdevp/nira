//! On-disk thumbnail cache, so re-opening a comic shows thumbnails instantly
//! instead of regenerating them.
//!
//! Each thumbnail is a PNG named by a stable hash of the book's path, its
//! modification time and the page. `prune` keeps the folder under a size
//! cap, dropping the thumbnails used longest ago first (a hit refreshes the
//! file's modification time).

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder, ImageFormat};

const DIR: &str = "thumbnails";
/// The most the cache may hold; `prune` trims it to `TRIM_TO` of this.
const CACHE_CAP: u64 = 256 * 1024 * 1024;
const TRIM_TO: f64 = 0.8;
/// Temp files older than this are left over from a crash.
const STALE_TMP: Duration = Duration::from_secs(3600);

fn cache_dir() -> PathBuf {
    crate::paths::cache_dir().join(DIR)
}

/// FNV-1a, 64-bit: fixed by its definition, unlike `DefaultHasher`, so
/// the cache stays valid across Rust releases.
struct Fnv(u64);

impl Fnv {
    fn new() -> Fnv {
        Fnv(0xcbf2_9ce4_8422_2325)
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

/// Cache key: book path + mtime + page index (editing the book
/// invalidates its thumbnails).
fn key(path: &Path, mtime_nanos: u128, page: usize) -> u64 {
    let mut h = Fnv::new();
    h.write(path.as_os_str().as_encoded_bytes());
    // A separator, so a path can't run into the numbers.
    h.write(&[0]);
    h.write(&mtime_nanos.to_le_bytes());
    h.write(&(page as u64).to_le_bytes());
    h.0
}

fn mtime_nanos(path: &Path) -> u128 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos())
}

fn file_name(key: u64) -> String {
    format!("{key:016x}.png")
}

/// Load a cached thumbnail: `(width, height, tight RGBA8)`.
pub fn load(path: &Path, page: usize) -> Option<(u32, u32, Vec<u8>)> {
    let key = key(path, mtime_nanos(path), page);
    load_in(&cache_dir(), key)
}

fn load_in(dir: &Path, key: u64) -> Option<(u32, u32, Vec<u8>)> {
    let file = dir.join(file_name(key));
    let bytes = fs::read(&file).ok()?;
    let img = match image::load_from_memory_with_format(&bytes, ImageFormat::Png) {
        Ok(img) => img.into_rgba8(),
        Err(e) => {
            log::debug!("dropping unreadable thumbnail {}: {e}", file.display());
            let _ = fs::remove_file(&file);
            return None;
        }
    };
    // Mark it as used, for `prune`.
    if let Ok(f) = fs::File::options().write(true).open(&file) {
        let _ = f.set_modified(SystemTime::now());
    }
    let (w, h) = img.dimensions();
    Some((w, h, img.into_raw()))
}

/// Store a `w x h` RGBA8 thumbnail.
pub fn store(path: &Path, page: usize, w: u32, h: u32, rgba: &[u8]) {
    let key = key(path, mtime_nanos(path), page);
    if let Err(e) = store_in(&cache_dir(), key, w, h, rgba) {
        log::debug!("cannot cache thumbnail {} of {}: {e}", page + 1, path.display());
    }
}

fn store_in(dir: &Path, key: u64, w: u32, h: u32, rgba: &[u8]) -> io::Result<()> {
    let expected = (w as usize)
        .checked_mul(h as usize)
        .and_then(|n| n.checked_mul(4));
    if expected != Some(rgba.len()) || w == 0 || h == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "bad pixel buffer"));
    }
    let mut png = Vec::new();
    PngEncoder::new_with_quality(&mut png, CompressionType::Fast, FilterType::Adaptive)
        .write_image(rgba, w, h, ExtendedColorType::Rgba8)
        .map_err(io::Error::other)?;
    fs::create_dir_all(dir)?;
    // Written whole then renamed, so a crash never leaves half a file.
    // No fsync: a lost thumbnail is only made again.
    let dest = dir.join(file_name(key));
    let tmp = dir.join(format!("{key:016x}.{}.tmp", std::process::id()));
    let result = fs::File::create(&tmp)
        .and_then(|mut f| f.write_all(&png))
        .and_then(|()| fs::rename(&tmp, &dest));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Keep the cache under its size cap, dropping the least recently used
/// thumbnails first. Run once per start, off the UI thread.
pub fn prune() {
    prune_dir(&cache_dir(), CACHE_CAP);
}

/// Delete stale temp files, then, if the PNGs in `dir` take more than
/// `cap` bytes, the least recently used until they fit in `TRIM_TO` of it.
fn prune_dir(dir: &Path, cap: u64) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    let mut files = Vec::new();
    let mut total = 0u64;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        match path.extension().and_then(|e| e.to_str()) {
            Some("png") => {
                total += meta.len();
                files.push((modified, meta.len(), path));
            }
            Some("tmp") => {
                let age = now.duration_since(modified).unwrap_or_default();
                if age > STALE_TMP {
                    let _ = fs::remove_file(&path);
                }
            }
            _ => {}
        }
    }
    if total <= cap {
        return;
    }
    let target = (cap as f64 * TRIM_TO) as u64;
    files.sort_by_key(|(modified, ..)| *modified);
    let mut removed = 0usize;
    for (_, len, path) in files {
        if total <= target {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            total -= len;
            removed += 1;
        }
    }
    log::debug!("thumbnail cache: removed {removed} old thumbnails");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nira-thumbs-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn pixels(w: u32, h: u32, seed: u8) -> Vec<u8> {
        (0..w * h * 4).map(|i| (i as u8).wrapping_mul(seed)).collect()
    }

    fn set_age(path: &Path, secs_ago: u64) {
        let f = fs::File::options().write(true).open(path).unwrap();
        f.set_modified(SystemTime::now() - Duration::from_secs(secs_ago))
            .unwrap();
    }

    #[test]
    fn fnv_matches_the_reference_values() {
        let fnv = |s: &str| {
            let mut h = Fnv::new();
            h.write(s.as_bytes());
            h.0
        };
        // From the FNV reference test suite.
        assert_eq!(fnv(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv("a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv("foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn the_key_is_stable() {
        // A changed value means every user's cache is lost: only change it
        // together with DIR.
        assert_eq!(
            key(Path::new("/books/a.cbz"), 1_700_000_000_000_000_000, 7),
            0x2949_57cc_43f9_6826
        );
        assert_ne!(
            key(Path::new("/books/a.cbz"), 1, 7),
            key(Path::new("/books/a.cbz"), 1, 8)
        );
    }

    #[test]
    fn stores_and_loads_the_same_pixels() {
        let dir = scratch("roundtrip");
        let px = pixels(13, 21, 7);
        store_in(&dir, 1, 13, 21, &px).unwrap();
        assert_eq!(load_in(&dir, 1), Some((13, 21, px)));
        assert_eq!(load_in(&dir, 2), None);
        // Nothing but the PNG is left behind.
        let names: Vec<_> = fs::read_dir(&dir).unwrap().flatten().collect();
        assert_eq!(names.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn bad_buffers_are_not_stored() {
        let dir = scratch("bad");
        assert!(store_in(&dir, 1, 10, 10, &[0; 12]).is_err());
        assert!(store_in(&dir, 1, u32::MAX, u32::MAX, &[0; 4]).is_err());
        assert!(store_in(&dir, 1, 0, 0, &[]).is_err());
        assert_eq!(load_in(&dir, 1), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_files_are_misses_and_removed() {
        let dir = scratch("corrupt");
        store_in(&dir, 1, 8, 8, &pixels(8, 8, 3)).unwrap();
        let file = dir.join(file_name(1));
        let bytes = fs::read(&file).unwrap();
        fs::write(&file, &bytes[..bytes.len() / 2]).unwrap();
        assert_eq!(load_in(&dir, 1), None);
        assert!(!file.exists());
        fs::write(dir.join(file_name(2)), b"not a png").unwrap();
        assert_eq!(load_in(&dir, 2), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn prune_drops_the_least_recently_used_first() {
        let dir = scratch("prune");
        for k in 0..10u64 {
            store_in(&dir, k, 32, 32, &pixels(32, 32, k as u8 + 1)).unwrap();
            // Key 0 is the oldest.
            set_age(&dir.join(file_name(k)), 1000 - k * 10);
        }
        // Using the oldest makes it the newest.
        assert!(load_in(&dir, 0).is_some());
        let size = |k: u64| fs::metadata(dir.join(file_name(k))).unwrap().len();
        let total: u64 = (0..10).map(size).sum();
        // Under the cap: nothing goes.
        prune_dir(&dir, total);
        assert!((0..10).all(|k| dir.join(file_name(k)).exists()));
        // Over it: trimmed to TRIM_TO of the cap, oldest first.
        let cap = total / 2;
        prune_dir(&dir, cap);
        let left: Vec<u64> = (0..10).filter(|&k| dir.join(file_name(k)).exists()).collect();
        let left_total: u64 = left.iter().map(|&k| size(k)).sum();
        assert!(left_total <= (cap as f64 * TRIM_TO) as u64);
        assert!(left.contains(&0) && left.contains(&9), "kept {left:?}");
        assert!(!left.contains(&1), "kept {left:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn prune_removes_stale_temp_files_only() {
        let dir = scratch("tmp");
        let stale = dir.join("a.1.tmp");
        let fresh = dir.join("b.2.tmp");
        fs::write(&stale, b"x").unwrap();
        fs::write(&fresh, b"x").unwrap();
        set_age(&stale, STALE_TMP.as_secs() + 60);
        prune_dir(&dir, CACHE_CAP);
        assert!(!stale.exists());
        assert!(fresh.exists(), "another instance may be writing it");
        let _ = fs::remove_dir_all(&dir);
    }
}
