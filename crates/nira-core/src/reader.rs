//! The open book and everything about showing it that does not need GTK:
//! the current page and spread, the decode requests sent to the page worker,
//! the page cache and prefetch window, and which pixels each display slot
//! should show at the current zoom.
//!
//! The UI owns the worker channels and the textures. It calls into `Reader`
//! when the user navigates or the viewport changes, sends whatever
//! `take_commands` returns to the page worker, hands worker results back to
//! `on_result`, and draws the `Frame` that `frame` returns.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use crate::image_loader::Pixels;
use crate::lru::LruCache;
use crate::ordering::{spread_of, step_magnitude, visual_order, SpreadRules};
use crate::settings::{Enhance, View};
use crate::workers::{
    spread_targets, DecodedPage, PageCmd, PageResult, RescaleCmd, RescaleJob, RescaleResult,
    WorkerCmd, WorkerResult,
};
use crate::zoom::ZoomModel;

/// Decoded pages kept in memory, at most this many bytes.
const CACHE_BYTES: usize = 384 * 1024 * 1024;
/// The layout size of a page that could not be decoded, before any page
/// has been: a portrait page.
const FAILED_PAGE_SIZE: (u32, u32) = (1000, 1500);

/// An opened book: its path and page names in reading order.
#[derive(Debug, Clone)]
pub struct Book {
    pub path: PathBuf,
    pub pages: Arc<Vec<String>>,
    /// An archive (not a folder of images): only archives show the cover
    /// alone.
    pub is_archive: bool,
}

/// Rotation (degrees clockwise) and flips applied to every page.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Transform {
    pub rotation: i32,
    pub flip_h: bool,
    pub flip_v: bool,
}

/// Result of a page flip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flip {
    /// The page changed.
    Moved,
    /// Forward from the last spread: the caller may open the next book.
    PastEnd,
    /// Backward from the first page: the caller may open the previous book.
    BeforeStart,
    /// Nothing happened (no book, or a page is still decoding).
    Stay,
}

/// Pixels for one display slot.
#[derive(Debug, Clone)]
pub enum SlotImage {
    /// Pixels scaled for display, owned.
    Scaled { w: u32, h: u32, rgba: Vec<u8> },
    /// The page's full-resolution pixels, shared with the cache.
    Full { w: u32, h: u32, rgba: Pixels },
}

/// New pixels for a slot. `slot` is the page's position in the spread in
/// reading order (0 = first page).
#[derive(Debug, Clone)]
pub struct SlotUpdate {
    pub slot: usize,
    pub page: usize,
    pub image: SlotImage,
}

/// What to draw for the current spread.
#[derive(Debug, Clone, Default)]
pub struct Frame {
    /// Page indices of the spread, in reading order.
    pub spread: Vec<usize>,
    /// Slots in screen order, left to right.
    pub visual: Vec<usize>,
    /// Display size per slot (reading order). Empty until the spread's page
    /// sizes are known.
    pub sizes: Vec<(u32, u32)>,
    /// Slots whose pixels change. Slots not listed keep what they show; a
    /// slot showing another page than `spread[slot]` should be cleared.
    pub updates: Vec<SlotUpdate>,
    /// Slots (reading order) whose page could not be read or decoded: they
    /// get a placeholder of their size instead of pixels.
    pub failed: Vec<usize>,
}

/// What a slot shows: the page and the display size its pixels were made
/// for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shown {
    page: usize,
    key: (u32, u32),
}

pub struct Reader {
    book: Option<Book>,
    page: usize,
    pub view: View,
    pub zoom: ZoomModel,
    pub transform: Transform,
    pub enhance: Enhance,
    viewport: (f64, f64),
    /// Full-resolution sizes of decoded pages (as displayed, so rotated).
    dims: HashMap<usize, (u32, u32)>,
    /// File sizes of decoded pages.
    file_bytes: HashMap<usize, u64>,
    cache: LruCache<usize, (u32, u32, Pixels)>,
    cache_pages: usize,
    /// The pages to keep cached, most wanted first (`update_cache_window`).
    wanted: Vec<usize>,
    /// Pages that could not be read or decoded. Tried again when shown,
    /// not prefetched.
    failed: HashSet<usize>,
    /// The size of the last page decoded, for failed pages' placeholders.
    typical_size: Option<(u32, u32)>,
    shown: Vec<Option<Shown>>,
    commands: Vec<WorkerCmd>,
    /// Bumped per opened book.
    gen: u64,
    /// Bumped when decoded pages become invalid (transform, enhance).
    pgen: u64,
    /// Bumped per display request; older display results are dropped.
    req: u64,
    /// Bumped per rescale request.
    scale_req: u64,
    loading: bool,
    prefetching: bool,
    prefetch_queue: VecDeque<usize>,
    scaling: bool,
    scale_pending: Option<Vec<RescaleJob>>,
}

impl Reader {
    pub fn new(view: View, zoom: ZoomModel, enhance: Enhance, cache_pages: u32) -> Reader {
        let cache_pages = cache_pages.max(1) as usize;
        Reader {
            book: None,
            page: 0,
            view,
            zoom,
            transform: Transform::default(),
            enhance,
            viewport: (0.0, 0.0),
            dims: HashMap::new(),
            file_bytes: HashMap::new(),
            cache: LruCache::new(cache_pages, CACHE_BYTES),
            cache_pages,
            wanted: Vec::new(),
            failed: HashSet::new(),
            typical_size: None,
            shown: Vec::new(),
            commands: Vec::new(),
            gen: 0,
            pgen: 0,
            req: 0,
            scale_req: 0,
            loading: false,
            prefetching: false,
            prefetch_queue: VecDeque::new(),
            scaling: false,
            scale_pending: None,
        }
    }

    pub fn book(&self) -> Option<&Book> {
        self.book.as_ref()
    }

    /// Current page, 0-based (the first page of the spread).
    pub fn page(&self) -> usize {
        self.page
    }

    pub fn page_count(&self) -> usize {
        self.book.as_ref().map_or(0, |b| b.pages.len())
    }

    /// True while the current spread is being decoded.
    pub fn is_loading(&self) -> bool {
        self.loading
    }

    /// Full-resolution size of a decoded page.
    pub fn page_size(&self, page: usize) -> Option<(u32, u32)> {
        // A failed page's size is only a placeholder for the layout.
        if self.failed.contains(&page) {
            return None;
        }
        self.dims.get(&page).copied()
    }

    /// A cached page's full-resolution pixels: `(w, h, rgba)`.
    pub fn page_pixels(&self, page: usize) -> Option<(u32, u32, Pixels)> {
        self.cache.peek(&page).cloned()
    }

    /// Size of a decoded page's file in the book.
    pub fn page_file_bytes(&self, page: usize) -> Option<u64> {
        self.file_bytes.get(&page).copied()
    }

    /// The area pages are fitted into.
    pub fn set_viewport(&mut self, viewport: (f64, f64)) {
        self.viewport = viewport;
    }

    /// Commands for the page worker, in order.
    pub fn take_commands(&mut self) -> Vec<WorkerCmd> {
        std::mem::take(&mut self.commands)
    }

    /// Show `book` from `start` (0-based, clamped). Results of the previous
    /// book still in flight are dropped.
    pub fn open(&mut self, book: Book, start: usize) {
        self.gen += 1;
        self.page = start.min(book.pages.len().saturating_sub(1));
        self.book = Some(book);
        self.typical_size = None;
        self.forget_pages();
        self.loading = false;
        self.load_current();
    }

    pub fn close(&mut self) {
        self.gen += 1;
        self.book = None;
        self.page = 0;
        self.forget_pages();
        self.loading = false;
    }

    fn forget_pages(&mut self) {
        self.dims.clear();
        self.file_bytes.clear();
        self.cache.clear();
        self.shown.clear();
        self.prefetch_queue.clear();
        self.wanted.clear();
        self.failed.clear();
        self.prefetching = false;
        self.scale_pending = None;
        // A rescale in flight is of the forgotten pixels: its result must
        // not be painted.
        self.scale_req += 1;
    }

    /// Keep up to `pages` decoded pages in memory (drops the cache).
    pub fn set_cache_pages(&mut self, pages: u32) {
        let pages = pages.max(1) as usize;
        if pages != self.cache_pages {
            self.cache_pages = pages;
            self.cache = LruCache::new(pages, CACHE_BYTES);
            self.update_cache_window();
        }
    }

    /// Show the current spread again after a view change (double page):
    /// decodes what is not cached.
    pub fn refresh(&mut self) {
        if self.book.is_some() && !self.loading {
            self.load_current();
        }
    }

    /// Decoded pages no longer match the transform or enhancement: decode
    /// the spread again.
    pub fn invalidate(&mut self) {
        self.pgen += 1;
        self.forget_pages();
        self.loading = false;
        self.load_current();
    }

    pub fn spread_rules(&self) -> SpreadRules {
        SpreadRules {
            double_page: self.view.double_page,
            cover_alone: self.view.cover_alone && self.book.as_ref().is_some_and(|b| b.is_archive),
            wide_alone: self.view.wide_alone,
        }
    }

    /// The pages shown for the current page, in reading order.
    pub fn spread(&self) -> Vec<usize> {
        spread_of(self.page_count(), self.page, self.spread_rules(), |i| {
            self.dims.get(&i).copied()
        })
    }

    /// Turn `count` steps forward or back (a step is one spread).
    pub fn flip(&mut self, forward: bool, count: usize) -> Flip {
        let n = self.page_count();
        if n == 0 || self.loading {
            return Flip::Stay;
        }
        let double_step = self.view.double_page && self.view.double_step;
        let mut target = self.page as isize;
        for _ in 0..count.max(1) {
            let step = step_magnitude(
                n,
                target.clamp(0, n as isize - 1) as usize,
                forward,
                double_step,
                self.spread_rules(),
                |i| self.dims.get(&i).copied(),
            ) as isize;
            target += if forward { step } else { -step };
            if target < 0 || target >= n as isize {
                break;
            }
        }
        if target >= n as isize {
            if count > 1 && self.page + 1 < n {
                // A long jump stops at the last page before leaving the book.
                return self.go_to_page(n - 1);
            }
            return Flip::PastEnd;
        }
        if target < 0 {
            if self.page == 0 {
                return Flip::BeforeStart;
            }
            return self.go_to_page(0);
        }
        self.go_to_page(target as usize)
    }

    /// Jump to `page` (0-based, clamped).
    pub fn go_to(&mut self, page: usize) -> Flip {
        if self.page_count() == 0 || self.loading {
            return Flip::Stay;
        }
        self.go_to_page(page.min(self.page_count() - 1))
    }

    fn go_to_page(&mut self, page: usize) -> Flip {
        if page == self.page {
            return Flip::Stay;
        }
        self.page = page;
        self.load_current();
        Flip::Moved
    }

    /// The first page of the last spread.
    pub fn last_page(&self) -> usize {
        self.page_count().saturating_sub(1)
    }

    /// Decode the current spread unless it is cached. Afterwards
    /// `frame` shows the cached pages; otherwise the spread appears when its
    /// result reaches `on_result`.
    fn load_current(&mut self) {
        let spread = self.spread();
        if spread.is_empty() {
            return;
        }
        if spread.iter().all(|i| self.cache.contains(i)) {
            self.update_cache_window();
            return;
        }
        self.req += 1;
        self.loading = true;
        let cmd = self.decode_cmd(spread, true);
        self.commands.push(WorkerCmd::Decode(cmd));
    }

    fn decode_cmd(&self, indices: Vec<usize>, display: bool) -> PageCmd {
        let book = self.book.as_ref().expect("decode without a book");
        PageCmd {
            req: self.req,
            gen: self.gen,
            pgen: self.pgen,
            display,
            viewport: self.viewport,
            zoom: self.zoom.clone(),
            path: book.path.clone(),
            pages: book.pages.clone(),
            indices,
            rotation: self.transform.rotation,
            flip_h: self.transform.flip_h,
            flip_v: self.transform.flip_v,
            wide_alone: self.spread_rules().wide_alone,
            brightness: self.enhance.brightness,
            contrast: self.enhance.contrast,
            auto_enhance: self.enhance.auto_enhance,
            exif_rotation: self.view.exif_rotation,
        }
    }

    /// Take a worker result. Returns the slot updates to draw now, or `None`
    /// if the result was stale or only filled the cache.
    pub fn on_result(&mut self, res: WorkerResult) -> Option<Frame> {
        match res {
            WorkerResult::Decoded(res) => self.on_decoded(res),
            WorkerResult::Rescaled(res) => self.on_rescaled(res),
        }
    }

    fn on_decoded(&mut self, res: PageResult) -> Option<Frame> {
        if res.gen != self.gen || res.pgen != self.pgen {
            if !res.display {
                self.prefetching = false;
                self.pump_prefetch();
            }
            return None;
        }
        // Sized like the pages beside them, or the last one decoded.
        let like = res.pages.first().map(|p| (p.full_w, p.full_h));
        self.note_failed(&res.failed, like);
        if !res.display {
            for p in res.pages {
                if !self.cache_prefetched(&p) {
                    // The cache is full of pages wanted more: the rest of
                    // the window, wanted less, would not fit either.
                    self.prefetch_queue.clear();
                }
            }
            self.prefetching = false;
            self.pump_prefetch();
            return None;
        }
        if res.req != self.req {
            return None;
        }
        self.loading = false;
        // Record the sizes first: they decide whether the pages pair.
        for p in &res.pages {
            self.dims.insert(p.idx, (p.full_w, p.full_h));
        }
        let spread = self.spread();
        self.shown = vec![None; spread.len()];
        let mut updates = Vec::new();
        for mut p in res.pages {
            self.cache_page(&p);
            let Some(slot) = spread.iter().position(|&s| s == p.idx) else {
                continue;
            };
            let image = match p.display.take() {
                Some(rgba) => SlotImage::Scaled {
                    w: p.w,
                    h: p.h,
                    rgba,
                },
                None => SlotImage::Full {
                    w: p.full_w,
                    h: p.full_h,
                    rgba: p.full.clone(),
                },
            };
            self.shown[slot] = Some(Shown {
                page: p.idx,
                key: p.key,
            });
            updates.push(SlotUpdate {
                slot,
                page: p.idx,
                image,
            });
        }
        let mut frame = self.frame();
        updates.extend(frame.updates);
        frame.updates = updates;
        self.update_cache_window();
        Some(frame)
    }

    fn on_rescaled(&mut self, res: RescaleResult) -> Option<Frame> {
        self.scaling = false;
        let mut frame = None;
        if res.gen == self.gen && res.req == self.scale_req {
            let spread = self.spread();
            let mut updates = Vec::new();
            for (idx, w, h, rgba, key) in res.pages {
                if let Some(slot) = spread.iter().position(|&s| s == idx) {
                    self.shown.resize(spread.len(), None);
                    self.shown[slot] = Some(Shown { page: idx, key });
                    updates.push(SlotUpdate {
                        slot,
                        page: idx,
                        image: SlotImage::Scaled { w, h, rgba },
                    });
                }
            }
            if !updates.is_empty() {
                let mut f = self.frame_layout(&spread);
                f.updates = updates;
                frame = Some(f);
            }
        }
        if let Some(jobs) = self.scale_pending.take() {
            self.send_rescale(jobs);
        }
        frame
    }

    /// Cache a page decoded for display. Only the pages on screen are
    /// sure to stay.
    fn cache_page(&mut self, p: &DecodedPage) {
        let bytes = self.note_page(p);
        let spread = self.spread();
        self.cache
            .put_keeping(p.idx, (p.full_w, p.full_h, p.full.clone()), bytes, |k| {
                spread.contains(k)
            });
    }

    /// Cache a prefetched page if there is room, making it only by
    /// evicting pages wanted less (never the ones on screen). False if it
    /// didn't fit.
    fn cache_prefetched(&mut self, p: &DecodedPage) -> bool {
        let bytes = self.note_page(p);
        let spread = self.spread();
        let wanted = &self.wanted;
        let rank = |k: &usize| -> usize {
            if spread.contains(k) {
                0
            } else {
                wanted
                    .iter()
                    .position(|w| w == k)
                    .map_or(usize::MAX, |i| i + 1)
            }
        };
        let mine = rank(&p.idx);
        self.cache
            .put_if_room(p.idx, (p.full_w, p.full_h, p.full.clone()), bytes, |k| {
                rank(k) > mine
            })
    }

    /// Pages that failed get a placeholder size, so the spread is laid out
    /// (and a good page beside a bad one is shown). It is a real page's
    /// size where one is known: a spread's pages share one zoom, so an
    /// odd size would shrink or swell the page beside it.
    fn note_failed(&mut self, failed: &[usize], like: Option<(u32, u32)>) {
        let size = like.or(self.typical_size).unwrap_or(FAILED_PAGE_SIZE);
        for &idx in failed {
            self.failed.insert(idx);
            self.dims.entry(idx).or_insert(size);
        }
    }

    /// Record a decoded page's sizes; its size in memory.
    fn note_page(&mut self, p: &DecodedPage) -> usize {
        self.failed.remove(&p.idx);
        self.typical_size = Some((p.full_w, p.full_h));
        self.dims.insert(p.idx, (p.full_w, p.full_h));
        self.file_bytes.insert(p.idx, p.file_bytes);
        (p.full_w as usize) * (p.full_h as usize) * 4
    }

    fn frame_layout(&self, spread: &[usize]) -> Frame {
        let sizes: Vec<(u32, u32)> = spread
            .iter()
            .map(|i| self.dims.get(i).copied())
            .collect::<Option<_>>()
            .unwrap_or_default();
        let failed = if sizes.is_empty() {
            Vec::new()
        } else {
            (0..spread.len())
                .filter(|&slot| self.failed.contains(&spread[slot]))
                .collect()
        };
        Frame {
            spread: spread.to_vec(),
            visual: visual_order(spread.len(), self.view.manga),
            sizes: spread_targets(&self.zoom, &sizes, self.viewport),
            updates: Vec::new(),
            failed,
        }
    }

    /// What to draw for the current spread at the current zoom and
    /// viewport. Pages whose shown pixels don't match their display size get
    /// the nearest pixels now (GTK scales them) and a sharp copy is asked
    /// from the worker; it arrives through `on_result`.
    pub fn frame(&mut self) -> Frame {
        let spread = self.spread();
        let mut frame = self.frame_layout(&spread);
        if frame.sizes.is_empty() {
            return frame;
        }
        self.shown.resize(spread.len(), None);
        let mut rescale = Vec::new();
        for (slot, &page) in spread.iter().enumerate() {
            let target = frame.sizes[slot];
            let current = self.shown[slot].filter(|s| s.page == page);
            if current.is_some_and(|s| s.key == target) {
                continue;
            }
            let Some((fw, fh, full)) = self.cache.get(&page).cloned() else {
                continue;
            };
            if target.0 >= fw && target.1 >= fh {
                // At or above full size the full-res pixels are the sharpest.
                self.shown[slot] = Some(Shown { page, key: target });
                frame.updates.push(SlotUpdate {
                    slot,
                    page,
                    image: SlotImage::Full {
                        w: fw,
                        h: fh,
                        rgba: full,
                    },
                });
                continue;
            }
            if current.is_none() {
                // A cached page turned to: show it full-res until the sharp
                // copy arrives.
                self.shown[slot] = Some(Shown {
                    page,
                    key: (fw, fh),
                });
                frame.updates.push(SlotUpdate {
                    slot,
                    page,
                    image: SlotImage::Full {
                        w: fw,
                        h: fh,
                        rgba: full.clone(),
                    },
                });
            }
            rescale.push(RescaleJob {
                idx: page,
                full_w: fw,
                full_h: fh,
                full,
                target,
            });
        }
        if !rescale.is_empty() {
            self.request_rescale(rescale);
        }
        frame
    }

    /// One rescale runs at a time; a newer request replaces a queued one.
    fn request_rescale(&mut self, jobs: Vec<RescaleJob>) {
        // Bumped even when queued, so the result in flight is recognised as
        // superseded.
        self.scale_req += 1;
        if self.scaling {
            self.scale_pending = Some(jobs);
            return;
        }
        self.send_rescale(jobs);
    }

    fn send_rescale(&mut self, jobs: Vec<RescaleJob>) {
        self.scaling = true;
        self.commands.push(WorkerCmd::Rescale(RescaleCmd {
            req: self.scale_req,
            gen: self.gen,
            pages: jobs,
        }));
    }

    /// Keep the pages around the current one cached: evict the rest and
    /// prefetch what is missing, nearest first (next spread, previous
    /// spread, then the rest of the window).
    fn update_cache_window(&mut self) {
        let n = self.page_count();
        if n == 0 {
            return;
        }
        let page = self.page;
        let width = if self.view.double_page { 2 } else { 1 };
        let start = page.saturating_sub(width);
        let mut wanted: Vec<usize> = (start..n).take(self.cache_pages).collect();
        wanted.sort_by_key(|&i| {
            if i == page {
                0
            } else if i == page + width {
                1
            } else if i + width == page {
                2
            } else {
                3
            }
        });
        let spread = self.spread();
        self.cache
            .retain(|k| wanted.contains(k) || spread.contains(k));
        self.prefetch_queue = wanted
            .iter()
            .copied()
            .filter(|i| !self.cache.contains(i) && !self.failed.contains(i))
            .collect();
        self.wanted = wanted;
        self.pump_prefetch();
    }

    fn pump_prefetch(&mut self) {
        if self.prefetching || self.loading {
            return;
        }
        let Some(idx) = self.prefetch_queue.pop_front() else {
            return;
        };
        self.prefetching = true;
        let cmd = self.decode_cmd(vec![idx], false);
        self.commands.push(WorkerCmd::Decode(cmd));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workers::{decode_pages, rescale_pages};

    /// Runs worker commands synchronously, the way the page worker would.
    fn run(reader: &mut Reader) -> Vec<Frame> {
        let mut frames = Vec::new();
        loop {
            let cmds = reader.take_commands();
            if cmds.is_empty() {
                return frames;
            }
            for cmd in cmds {
                let res = match cmd {
                    WorkerCmd::Decode(cmd) => {
                        let mut ar = crate::archive::open(&cmd.path).ok();
                        WorkerResult::Decoded(decode_pages(ar.as_mut(), &cmd))
                    }
                    WorkerCmd::Rescale(cmd) => WorkerResult::Rescaled(rescale_pages(cmd)),
                    WorkerCmd::UseArchive { .. } => {
                        unreachable!("the reader never hands over archives")
                    }
                };
                frames.extend(reader.on_result(res));
            }
        }
    }

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../test/files/archives")
            .join(name)
    }

    fn open(name: &str, view: View) -> Reader {
        let path = fixture(name);
        let mut ar = crate::archive::open(&path).unwrap();
        let pages = ar.page_names().unwrap();
        let mut r = Reader::new(view, ZoomModel::default(), Enhance::default(), 7);
        r.set_viewport((800.0, 600.0));
        r.open(
            Book {
                path,
                pages: Arc::new(pages),
                is_archive: true,
            },
            0,
        );
        r
    }

    #[test]
    fn opening_shows_the_first_page_and_prefetches() {
        let mut r = open("double-pages-test.cbz", View::default());
        assert!(r.is_loading());
        let frames = run(&mut r);
        assert!(!r.is_loading());
        let first = &frames[0];
        assert_eq!(first.spread, vec![0]);
        assert_eq!(first.updates[0].slot, 0);
        assert_eq!(first.sizes.len(), 1);
        let (w, h) = first.sizes[0];
        assert!(w <= 800 && h <= 600, "fits the viewport: {w}x{h}");
        // The cache window was filled in the background.
        assert!(r.cache.contains(&1));
    }

    #[test]
    fn flipping_to_a_cached_page_needs_no_decode() {
        let mut r = open("double-pages-test.cbz", View::default());
        run(&mut r);
        assert_eq!(r.flip(true, 1), Flip::Moved);
        assert!(!r.is_loading(), "page 2 was prefetched");
        let frame = r.frame();
        assert_eq!(frame.spread, vec![1]);
        assert_eq!(frame.updates.len(), 1);
        assert_eq!(r.flip(false, 1), Flip::Moved);
        assert_eq!(r.flip(false, 1), Flip::BeforeStart);
    }

    #[test]
    fn navigation_is_ignored_while_decoding() {
        let mut r = open("double-pages-test.cbz", View::default());
        assert_eq!(r.flip(true, 1), Flip::Stay);
        assert_eq!(r.page(), 0);
    }

    #[test]
    fn stale_results_are_dropped_after_reopening() {
        let mut r = open("double-pages-test.cbz", View::default());
        let stale = r.take_commands();
        let book = r.book().unwrap().clone();
        r.open(book, 1);
        for cmd in stale {
            if let WorkerCmd::Decode(cmd) = cmd {
                let mut ar = crate::archive::open(&cmd.path).ok();
                let res = WorkerResult::Decoded(decode_pages(ar.as_mut(), &cmd));
                assert!(r.on_result(res).is_none());
            }
        }
        let frames = run(&mut r);
        assert_eq!(frames[0].spread, vec![1]);
    }

    #[test]
    fn double_page_shows_two_pages_and_jumps_stop_at_the_end() {
        let view = View {
            double_page: true,
            cover_alone: false,
            wide_alone: false,
            ..View::default()
        };
        let mut r = open("double-pages-test.cbz", view);
        let frames = run(&mut r);
        assert_eq!(frames[0].spread, vec![0, 1]);
        let n = r.page_count();
        assert_eq!(r.flip(true, 100), Flip::Moved);
        run(&mut r);
        assert_eq!(r.page(), n - 1);
        assert_eq!(r.flip(true, 1), Flip::PastEnd);
    }

    #[test]
    fn zooming_rescales_the_shown_page() {
        let mut r = open("double-pages-test.cbz", View::default());
        run(&mut r);
        let before = r.frame();
        assert!(before.updates.is_empty(), "already sharp");
        // The fixture pages are 210x297: a smaller viewport shrinks them.
        r.set_viewport((100.0, 100.0));
        let frame = r.frame();
        assert!(frame.sizes[0].1 <= 100);
        assert!(
            matches!(frame.updates.as_slice(), []),
            "the page keeps its pixels until the sharp copy arrives"
        );
        let sharp = run(&mut r);
        let SlotImage::Scaled { w, h, .. } = &sharp[0].updates[0].image else {
            panic!("expected scaled pixels");
        };
        assert_eq!((*w, *h), frame.sizes[0]);
    }

    #[test]
    fn a_full_cache_keeps_the_shown_page_and_its_neighbours() {
        let mut r = open("double-pages-test.cbz", View::default());
        // The fixture pages are 210x297: room for three of them, while the
        // prefetch window asks for seven.
        let page_bytes = 210 * 297 * 4;
        r.cache = LruCache::new(7, page_bytes * 3 + page_bytes / 2);
        let mut decodes = 0;
        loop {
            let cmds = r.take_commands();
            if cmds.is_empty() {
                break;
            }
            for cmd in cmds {
                let WorkerCmd::Decode(cmd) = cmd else {
                    continue;
                };
                decodes += 1;
                let mut ar = crate::archive::open(&cmd.path).ok();
                r.on_result(WorkerResult::Decoded(decode_pages(ar.as_mut(), &cmd)));
            }
        }
        // The page on screen and the nearest ones stay, the far ones were
        // never squeezed in.
        assert!(r.cache.contains(&0) && r.cache.contains(&1) && r.cache.contains(&2));
        assert!(!r.cache.contains(&3));
        // Prefetching stopped at the first page that didn't fit.
        assert_eq!(decodes, 4);
        // So the next page needs no decode.
        assert_eq!(r.flip(true, 1), Flip::Moved);
        assert!(!r.is_loading());
    }

    /// A book of three pages whose middle page is not an image.
    fn book_with_a_broken_page(name: &str) -> PathBuf {
        use std::io::Write;
        let path = std::env::temp_dir().join(format!("nira-{}-{name}.cbz", std::process::id()));
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(30, 40, image::Rgba([200, 10, 10, 255]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        for (entry, bytes) in [
            ("1.png", &png[..]),
            ("2.png", b"not a png"),
            ("3.png", &png[..]),
        ] {
            zip.start_file(entry, opts).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    fn open_path(path: PathBuf, view: View, start: usize) -> Reader {
        let mut ar = crate::archive::open(&path).unwrap();
        let pages = ar.page_names().unwrap();
        let mut r = Reader::new(view, ZoomModel::default(), Enhance::default(), 7);
        r.set_viewport((800.0, 600.0));
        r.open(
            Book {
                path,
                pages: Arc::new(pages),
                is_archive: true,
            },
            start,
        );
        r
    }

    #[test]
    fn a_page_that_fails_gets_a_placeholder() {
        let path = book_with_a_broken_page("single");
        let mut r = open_path(path.clone(), View::default(), 0);
        run(&mut r);
        // Prefetching found the broken page, and won't decode it again.
        assert!(r.failed.contains(&1));
        assert!(!r.prefetch_queue.contains(&1));
        assert_eq!(r.flip(true, 1), Flip::Moved);
        run(&mut r);
        let frame = r.frame();
        assert_eq!(frame.spread, vec![1]);
        assert_eq!(frame.failed, vec![0], "the slot is marked");
        assert_eq!(frame.sizes.len(), 1, "and still laid out");
        assert_eq!(r.page_size(1), None, "its placeholder size isn't shown");
        assert!(!r.is_loading(), "navigation goes on");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_spread_keeps_its_good_page_beside_a_failed_one() {
        let path = book_with_a_broken_page("spread");
        let view = View {
            double_page: true,
            cover_alone: false,
            ..View::default()
        };
        let mut r = open_path(path.clone(), view, 0);
        let frames = run(&mut r);
        let drawn = frames.last().unwrap();
        assert_eq!(drawn.spread, vec![0, 1]);
        assert_eq!(drawn.sizes.len(), 2);
        // The placeholder takes the good page's size, so both share one
        // height instead of the good page shrinking beside it.
        assert_eq!(drawn.sizes[0], drawn.sizes[1]);
        assert_eq!(drawn.failed, vec![1]);
        assert!(r.cache.contains(&0));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_rescale_from_before_an_invalidation_is_dropped() {
        let mut r = open("double-pages-test.cbz", View::default());
        run(&mut r);
        r.set_viewport((100.0, 100.0));
        r.frame();
        let cmds = r.take_commands();
        let Some(WorkerCmd::Rescale(cmd)) = cmds
            .into_iter()
            .find(|c| matches!(c, WorkerCmd::Rescale(_)))
        else {
            panic!("expected a rescale");
        };
        // Rotation or enhancement changed while the rescale was running.
        r.invalidate();
        let stale = r.on_result(WorkerResult::Rescaled(rescale_pages(cmd)));
        assert!(stale.is_none(), "the old pixels were painted");
        // The fresh decode still arrives and is drawn.
        let frames = run(&mut r);
        assert!(frames.iter().any(|f| !f.updates.is_empty()));
    }
}
