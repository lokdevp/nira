//! Starts the page worker and opens books off the UI thread. Results come
//! back on async channels, so the window awaits them on the main loop
//! instead of polling.

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use nira_core::workers::{
    open_archive, spawn_page_worker, spawn_persister, spawn_thumbnailer, OpenError, OpenedArchive,
    PersistJob, ThumbCmd, ThumbResult, WorkerCmd, WorkerResult,
};

/// Start the page worker. Send it commands on the returned sender; its
/// results arrive on the receiver.
pub fn start_page_worker() -> (
    mpsc::Sender<WorkerCmd>,
    async_channel::Receiver<WorkerResult>,
) {
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (res_tx, res_rx) = mpsc::channel();
    spawn_page_worker(cmd_rx, res_tx);
    (cmd_tx, forward(res_rx))
}

/// Start the thumbnailer: send it the wanted pages; thumbnails arrive on
/// the receiver.
pub fn start_thumbnailer() -> (mpsc::Sender<ThumbCmd>, async_channel::Receiver<ThumbResult>) {
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (res_tx, res_rx) = mpsc::channel();
    spawn_thumbnailer(cmd_rx, res_tx);
    (cmd_tx, forward(res_rx))
}

/// Start the persistence thread: jobs run in order. Drop the sender and
/// join the handle to wait for the last write.
pub fn start_persister() -> (mpsc::Sender<PersistJob>, thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    (tx, spawn_persister(rx))
}

/// Relay a std channel onto an async one (the core's workers use std
/// channels; the UI awaits).
fn forward<T: Send + 'static>(rx: mpsc::Receiver<T>) -> async_channel::Receiver<T> {
    let (tx, async_rx) = async_channel::unbounded();
    thread::spawn(move || {
        for item in rx {
            if tx.send_blocking(item).is_err() {
                break;
            }
        }
    });
    async_rx
}

/// The previous or next book after `path` (see
/// `nira_core::siblings::sibling_book`), found on a background thread.
pub async fn sibling(path: PathBuf, forward: bool, folders: bool) -> Option<PathBuf> {
    let (tx, rx) = async_channel::bounded(1);
    thread::spawn(move || {
        let _ = tx.send_blocking(nira_core::siblings::sibling_book(&path, forward, folders));
    });
    rx.recv().await.ok().flatten()
}

/// Cached first-page thumbnails of `paths`: `(path, w, h, rgba)` for those
/// in the thumbnail cache.
pub async fn covers(paths: Vec<String>) -> Vec<(String, u32, u32, Vec<u8>)> {
    let (tx, rx) = async_channel::bounded(1);
    thread::spawn(move || {
        let found = paths
            .into_iter()
            .filter_map(|p| {
                let (w, h, rgba) = nira_core::thumb_cache::load(std::path::Path::new(&p), 0)?;
                Some((p, w, h, rgba))
            })
            .collect();
        let _ = tx.send_blocking(found);
    });
    rx.recv().await.unwrap_or_default()
}

/// Open and list `path` on a background thread (listing RAR, 7z, LHA and
/// PDF runs external tools, and compressed tars are unpacked).
pub async fn open(path: PathBuf) -> Result<OpenedArchive, OpenError> {
    let (tx, rx) = async_channel::bounded(1);
    thread::spawn(move || {
        let _ = tx.send_blocking(open_archive(&path));
    });
    rx.recv()
        .await
        .unwrap_or_else(|_| Err(OpenError::Open("the open thread stopped".into())))
}
