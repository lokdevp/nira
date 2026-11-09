//! Background threads and their messages: the page decoder, the
//! thumbnailer, the archive opener and the persistence writer.
//! Nothing here touches GTK.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use log::warn;

use crate::archive::{self, Archive};
use crate::image_loader::{self, Pixels};
use crate::ordering::pair_shown_alone;
use crate::zoom::ZoomModel;

/// Commands for the lazy thumbnail manager thread.
pub enum ThumbCmd {
    /// Replace the set of wanted pages (in priority order).
    SetWanted {
        gen: u64,
        path: PathBuf,
        pages: Arc<Vec<String>>,
        indices: Vec<usize>,
    },
    /// Pause/resume generation (paused while a page is decoding).
    Pause(bool),
    /// Read `path` through this handle (a clone of the one the open thread
    /// listed), instead of opening and unpacking it again.
    UseArchive {
        path: PathBuf,
        archive: Box<dyn Archive>,
    },
}

/// A finished thumbnail: `(thumb generation, page index, w, h, rgba)`.
pub type ThumbResult = (u64, usize, u32, u32, Vec<u8>);

/// Command for the background page-decoding worker.
pub struct PageCmd {
    /// Display requests only: incremented per display push; results with an
    /// older id are dropped.
    pub req: u64,
    /// Archive generation (bumped when a new file is opened).
    pub gen: u64,
    /// Prefetch generation (bumped when transforms invalidate the cache).
    pub pgen: u64,
    /// True for display requests, false for background prefetch.
    pub display: bool,
    /// Viewport + zoom settings, so the worker can pre-scale display
    /// requests to the target size (it knows the page's full dimensions).
    pub viewport: (f64, f64),
    pub zoom: ZoomModel,
    pub path: PathBuf,
    /// Shared with the UI's page list (not copied per request).
    pub pages: Arc<Vec<String>>,
    /// Page indices to decode.
    pub indices: Vec<usize>,
    pub rotation: i32,
    pub flip_h: bool,
    pub flip_v: bool,
    /// Show wide pages alone: when a display request's pair turns out to
    /// hold a wide page, only the first page is sized for display.
    pub wide_alone: bool,
    pub brightness: f64,
    pub contrast: f64,
    pub auto_enhance: bool,
    /// Turn pages upright by their EXIF orientation.
    pub exif_rotation: bool,
}

/// One decoded page. `full` is the full-resolution image (kept for the
/// cache and the magnifying lens). `display` holds the pre-scaled pixels
/// (`w x h`) when the worker scaled the page down, and is `None` when the
/// full-resolution image is shown as is. `key` is the zoomed size the
/// display texture was made for, which `redraw` matches against.
pub struct DecodedPage {
    pub idx: usize,
    pub w: u32,
    pub h: u32,
    pub display: Option<Vec<u8>>,
    pub key: (u32, u32),
    pub full_w: u32,
    pub full_h: u32,
    pub full: Pixels,
    /// Size of the page's file in the archive.
    pub file_bytes: u64,
}

pub struct PageResult {
    pub req: u64,
    pub gen: u64,
    pub pgen: u64,
    pub display: bool,
    pub pages: Vec<DecodedPage>,
}

/// Scale already-decoded pages to a new display size (zoom change, cache-hit
/// page turn), off the UI thread.
pub struct RescaleCmd {
    pub req: u64,
    pub gen: u64,
    pub pages: Vec<RescaleJob>,
}

pub struct RescaleJob {
    pub idx: usize,
    pub full_w: u32,
    pub full_h: u32,
    pub full: Pixels,
    /// The zoomed size to scale to (smaller than the full image).
    pub target: (u32, u32),
}

/// One rescaled page: `(idx, w, h, rgba, key)`, where `key` is the target
/// size it was scaled for.
pub type ScaledPage = (usize, u32, u32, Vec<u8>, (u32, u32));

pub struct RescaleResult {
    pub req: u64,
    pub gen: u64,
    pub pages: Vec<ScaledPage>,
}

/// Commands for the page worker thread.
pub enum WorkerCmd {
    Decode(PageCmd),
    Rescale(RescaleCmd),
    /// Read `path` through this handle (the one the open thread listed and
    /// unpacked), instead of opening and unpacking it again. No result.
    UseArchive {
        path: PathBuf,
        archive: Box<dyn Archive>,
    },
}

/// Results from the page worker thread.
pub enum WorkerResult {
    Decoded(PageResult),
    Rescaled(RescaleResult),
}

/// A disk write run on the persistence thread.
pub type PersistJob = Box<dyn FnOnce() + Send>;

/// Why an archive could not be shown.
pub enum OpenError {
    Open(String),
    List(String),
    Empty,
}

/// An archive opened and listed on a background thread.
pub struct OpenedArchive {
    /// Listed (and unpacked, if need be): for the page worker.
    pub archive: Box<dyn Archive>,
    /// A clone sharing what `archive` unpacked: for the thumbnailer.
    /// `None` if the backend can't clone; it then opens the path itself.
    pub twin: Option<Box<dyn Archive>>,
    pub pages: Vec<String>,
    /// `detect` result, for the log and the cover-alone rule.
    pub kind: Option<String>,
    /// Bytes on disk: the archive file, or the images of a directory.
    pub size: u64,
}

pub struct OpenResult {
    /// Matches the UI's current open token; results of superseded opens are dropped.
    pub token: u64,
    pub path: PathBuf,
    pub start_page: u32,
    pub outcome: Result<OpenedArchive, OpenError>,
}

/// Open and list an archive (runs off the UI thread: listing RAR, 7z, LHA
/// and PDF runs external tools, and compressed tars are unpacked).
pub fn open_archive(path: &Path) -> Result<OpenedArchive, OpenError> {
    let mut archive = archive::open(path).map_err(|e| OpenError::Open(e.to_string()))?;
    let pages = archive
        .page_names()
        .map_err(|e| OpenError::List(e.to_string()))?;
    if pages.is_empty() {
        return Err(OpenError::Empty);
    }
    // Cloned here, off the UI thread (a zip clone reads its directory again).
    let twin = archive.try_clone();
    let kind = archive::detect(path).map(|k| k.to_string());
    let size = if path.is_dir() {
        std::fs::read_dir(path)
            .map(|rd| {
                rd.flatten()
                    .filter_map(|e| e.metadata().ok())
                    .filter(|m| m.is_file())
                    .map(|m| m.len())
                    .sum()
            })
            .unwrap_or(0)
    } else {
        std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
    };
    Ok(OpenedArchive {
        archive,
        twin,
        pages,
        kind,
        size,
    })
}

/// Lazy thumbnail manager: processes the wanted set (window around the
/// current page + visible viewport) one page at a time, in priority order,
/// checking the disk cache first. A single low-rate thread keeps CPU load off
/// the page decoder.
pub fn spawn_thumbnailer(
    thumb_cmd_rx: std::sync::mpsc::Receiver<ThumbCmd>,
    thumb_res_tx: std::sync::mpsc::Sender<ThumbResult>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut ar: Option<Box<dyn Archive>> = None;
        let mut ar_path: Option<PathBuf> = None;
        let mut wanted: Vec<usize> = Vec::new();
        let mut wanted_pos: usize = 0;
        let mut done: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let mut gen: u64 = 0;
        let mut path: PathBuf = PathBuf::new();
        let mut pages: Arc<Vec<String>> = Arc::default();
        let mut paused = false;

        // Every command must be handled exactly once: commands drained
        // with try_recv, and the single command returned by the
        // blocking recv() (used while idle / paused).
        macro_rules! handle_cmd {
            ($cmd:expr) => {
                match $cmd {
                    ThumbCmd::SetWanted {
                        gen: g,
                        path: p,
                        pages: ps,
                        indices,
                    } => {
                        log::debug!("thumb manager: SetWanted gen={g} n={}", indices.len());
                        if g != gen {
                            gen = g;
                            path = p;
                            pages = ps;
                            done.clear();
                        }
                        wanted = indices;
                        wanted_pos = 0;
                    }
                    ThumbCmd::Pause(p) => {
                        log::debug!("thumb manager: pause={p}");
                        paused = p;
                    }
                    ThumbCmd::UseArchive { path: p, archive } => {
                        log::debug!("thumb manager: archive handed over for {}", p.display());
                        ar = Some(archive);
                        ar_path = Some(p);
                    }
                }
            };
        }

        'manager: loop {
            // Drain everything already queued. Stop as soon as the UI
            // drops its sender (shutdown), even with pages still
            // wanted, so the archive and its temp dir are released.
            loop {
                match thumb_cmd_rx.try_recv() {
                    Ok(cmd) => handle_cmd!(cmd),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => break 'manager,
                }
            }
            if paused {
                // Wait for a resume command and handle it directly.
                match thumb_cmd_rx.recv() {
                    Ok(cmd) => handle_cmd!(cmd),
                    Err(_) => break,
                }
                continue;
            }

            // Skip pages already processed.
            while wanted_pos < wanted.len() && done.contains(&wanted[wanted_pos]) {
                wanted_pos += 1;
            }
            if wanted_pos >= wanted.len() {
                // Nothing wanted right now: block for a command and
                // handle it (don't discard it!).
                match thumb_cmd_rx.recv() {
                    Ok(cmd) => handle_cmd!(cmd),
                    Err(_) => break,
                }
                continue;
            }
            let idx = wanted[wanted_pos];
            wanted_pos += 1;
            done.insert(idx);
            log::debug!("thumb manager: decode page {}", idx + 1);

            // Disk cache first: a hit needs neither the archive nor
            // the page bytes.
            if let Some((w, h, rgba)) = crate::thumb_cache::load(&path, idx) {
                if thumb_res_tx.send((gen, idx, w, h, rgba)).is_err() {
                    break;
                }
                continue;
            }
            if ar_path.as_deref() != Some(path.as_path()) {
                ar = archive::open(&path).ok();
                ar_path = Some(path.clone());
                if ar.is_none() {
                    warn!("thumbnail manager cannot open {}", path.display());
                }
            }
            if let Some(ar) = ar.as_mut() {
                if let Some(name) = pages.get(idx) {
                    if let Ok(bytes) = ar.read(name) {
                        let thumb = image_loader::thumbnail_pixbuf_rgba(&bytes, 160, 160)
                            .or_else(|| image_loader::thumbnail_rgba_fallback(&bytes, 160, 160));
                        if let Some((w, h, rgba)) = thumb {
                            crate::thumb_cache::store(&path, idx, w, h, &rgba);
                            if thumb_res_tx.send((gen, idx, w, h, rgba)).is_err() {
                                break;
                            }
                        }
                    }
                }
            }
        }
    })
}

/// The page decoder: decodes pages for display and prefetch, and rescales
/// cached pages. It keeps its own archive handle and reopens it when the
/// path changes.
pub fn spawn_page_worker(
    page_cmd_rx: std::sync::mpsc::Receiver<WorkerCmd>,
    page_res_tx: std::sync::mpsc::Sender<WorkerResult>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut ar: Option<Box<dyn Archive>> = None;
        let mut ar_path: Option<PathBuf> = None;
        while let Ok(cmd) = page_cmd_rx.recv() {
            let res = match cmd {
                WorkerCmd::Decode(cmd) => {
                    if ar_path.as_deref() != Some(cmd.path.as_path()) {
                        ar = archive::open(&cmd.path).ok();
                        ar_path = Some(cmd.path.clone());
                        if ar.is_none() {
                            warn!("page worker cannot open {}", cmd.path.display());
                        }
                    }
                    WorkerResult::Decoded(decode_pages(ar.as_mut(), &cmd))
                }
                WorkerCmd::Rescale(cmd) => WorkerResult::Rescaled(rescale_pages(cmd)),
                WorkerCmd::UseArchive { path, archive } => {
                    log::debug!("page worker: archive handed over for {}", path.display());
                    ar = Some(archive);
                    ar_path = Some(path);
                    continue;
                }
            };
            if page_res_tx.send(res).is_err() {
                break; // main thread is gone
            }
        }
    })
}

/// Runs disk writes (settings, keys, history, bookmarks) in order, off the
/// UI thread.
pub fn spawn_persister(rx: std::sync::mpsc::Receiver<PersistJob>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        for job in rx {
            job();
        }
    })
}

/// Display sizes for a spread: one zoom for all its pages, so they share a
/// height. Used by both the worker pre-scale and the UI's redraw, so the
/// textures the worker makes match what the redraw asks for.
pub fn spread_targets(
    zoom: &ZoomModel,
    sizes: &[(u32, u32)],
    viewport: (f64, f64),
) -> Vec<(u32, u32)> {
    if sizes.is_empty() || viewport.0 <= 0.0 || viewport.1 <= 0.0 {
        return Vec::new();
    }
    zoom.zoomed_sizes(sizes, viewport, 2.0, sizes.len() > 1)
}

/// Worker: read, decode, transform and enhance the requested pages, then
/// (for display requests) pre-scale them to their display size so page
/// turns never resize on the UI thread.
pub fn decode_pages(ar: Option<&mut Box<dyn Archive>>, cmd: &PageCmd) -> PageResult {
    // Decode every page first: the display size depends on the whole spread.
    let mut decoded: Vec<(usize, image::RgbaImage, u64)> = Vec::new();
    if let Some(ar) = ar {
        for &idx in &cmd.indices {
            let Some(name) = cmd.pages.get(idx) else {
                continue;
            };
            let bytes = match ar.read(name) {
                Ok(b) => b,
                Err(e) => {
                    warn!("cannot read page {name}: {e}");
                    continue;
                }
            };
            let img = match image_loader::decode_oriented(&bytes, cmd.exif_rotation) {
                Ok(img) => img,
                Err(e) => {
                    warn!("cannot decode page {name}: {e}");
                    continue;
                }
            };
            let img = image_loader::transform(img, cmd.rotation, cmd.flip_h, cmd.flip_v);
            let img = image_loader::enhance(img, cmd.brightness, cmd.contrast, cmd.auto_enhance);
            decoded.push((idx, img, bytes.len() as u64));
        }
    }
    let targets = if cmd.display {
        let mut sizes: Vec<(u32, u32)> =
            decoded.iter().map(|(_, img, _)| img.dimensions()).collect();
        // The UI did not know these sizes when it asked for the pair; apply
        // the same rule as `spread_of` so the shown page is sized alone. The
        // second page then only fills the cache.
        if cmd.wide_alone && sizes.len() == 2 && pair_shown_alone(sizes[0], sizes[1]) {
            sizes.truncate(1);
        }
        spread_targets(&cmd.zoom, &sizes, cmd.viewport)
    } else {
        Vec::new()
    };
    let mut pages = Vec::with_capacity(decoded.len());
    for (i, (idx, img, file_bytes)) in decoded.into_iter().enumerate() {
        let (full_w, full_h) = img.dimensions();
        let target = targets.get(i).copied();
        // Scale down when the page is larger than its display size. A page
        // shown at or above full size uses the full-res pixels as is (GTK
        // scales them up), so no copy is made.
        let scaled = match target {
            Some((tw, th)) if tw < full_w || th < full_h => {
                Some(image::imageops::thumbnail(&img, tw.max(1), th.max(1)))
            }
            _ => None,
        };
        let (w, h, display) = match scaled {
            Some(s) => {
                let (w, h) = s.dimensions();
                (w, h, Some(s.into_raw()))
            }
            None => (full_w, full_h, None),
        };
        pages.push(DecodedPage {
            idx,
            w,
            h,
            display,
            key: target.unwrap_or((full_w, full_h)),
            full_w,
            full_h,
            full: Arc::new(img.into_raw()),
            file_bytes,
        });
    }
    PageResult {
        req: cmd.req,
        gen: cmd.gen,
        pgen: cmd.pgen,
        display: cmd.display,
        pages,
    }
}

/// Worker: scale already-decoded pages down to a new display size.
pub fn rescale_pages(cmd: RescaleCmd) -> RescaleResult {
    let pages = cmd
        .pages
        .into_iter()
        .filter_map(|job| {
            let img = image::ImageBuffer::<image::Rgba<u8>, &[u8]>::from_raw(
                job.full_w,
                job.full_h,
                job.full.as_slice(),
            )?;
            let (tw, th) = job.target;
            let scaled = image::imageops::thumbnail(&img, tw.max(1), th.max(1));
            let (w, h) = scaled.dimensions();
            Some((job.idx, w, h, scaled.into_raw(), job.target))
        })
        .collect();
    RescaleResult {
        req: cmd.req,
        gen: cmd.gen,
        pages,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::ArchiveError;

    /// An archive that exists only in memory: one 4x3 PNG page.
    struct InMemory;

    impl Archive for InMemory {
        fn name(&self) -> &str {
            "in-memory"
        }

        fn page_names(&mut self) -> Result<Vec<String>, ArchiveError> {
            Ok(vec!["p.png".into()])
        }

        fn read(&mut self, _name: &str) -> Result<Vec<u8>, ArchiveError> {
            let mut png = Vec::new();
            image::RgbaImage::new(4, 3)
                .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .unwrap();
            Ok(png)
        }
    }

    #[test]
    fn the_page_worker_reads_through_a_handed_over_archive() {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
        let (res_tx, res_rx) = std::sync::mpsc::channel();
        let worker = spawn_page_worker(cmd_rx, res_tx);
        // No such file: a page can only come from the handed-over handle,
        // not from opening the path again.
        let path = PathBuf::from("/nonexistent/nira-test-book.cbz");
        cmd_tx
            .send(WorkerCmd::UseArchive {
                path: path.clone(),
                archive: Box::new(InMemory),
            })
            .unwrap();
        cmd_tx
            .send(WorkerCmd::Decode(PageCmd {
                req: 1,
                gen: 1,
                pgen: 0,
                display: false,
                viewport: (800.0, 600.0),
                zoom: ZoomModel::default(),
                path,
                pages: Arc::new(vec!["p.png".into()]),
                indices: vec![0],
                rotation: 0,
                flip_h: false,
                flip_v: false,
                wide_alone: false,
                brightness: 1.0,
                contrast: 1.0,
                auto_enhance: false,
                exif_rotation: false,
            }))
            .unwrap();
        let WorkerResult::Decoded(res) = res_rx.recv().unwrap() else {
            panic!("expected a decode result");
        };
        assert_eq!(res.pages.len(), 1);
        assert_eq!((res.pages[0].full_w, res.pages[0].full_h), (4, 3));
        drop(cmd_tx);
        worker.join().unwrap();
    }
}
