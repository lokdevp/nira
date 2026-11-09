//! Comic archive support: format detection and per-format readers.

pub mod lha;
pub mod pdf;
pub mod rar;
pub mod sevenzip;
pub mod tar;
pub mod zip;

use std::fmt;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::natsort;

/// All supported image file extensions (subset of what Pillow + GTK understand).
/// Only formats enabled in the `image` crate's features belong here (no PCX:
/// the crate can't decode it).
pub const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "webp", "tif", "tiff", "ico", "ppm", "pbm", "pgm", "pnm",
    "tga", "qoi",
];

/// Extensions of archives (and PDFs) that may sit inside a book.
const ARCHIVE_EXTENSIONS: &[&str] = &[
    "zip", "cbz", "rar", "cbr", "7z", "cb7", "tar", "cbt", "gz", "bz2", "xz", "lha", "lzh", "pdf",
];

/// True if `name` looks like an archive (extension check, case-insensitive):
/// a book inside the book, which is not opened.
pub fn is_archive_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower
        .rsplit_once('.')
        .is_some_and(|(_, ext)| ARCHIVE_EXTENSIONS.contains(&ext))
}

/// True if `name` looks like an image file (extension check, case-insensitive).
pub fn is_image_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let Some(ext) = lower.rsplit('.').next() else {
        return false;
    };
    IMAGE_EXTENSIONS.contains(&ext)
}

/// Archive container formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    Zip,
    Rar,
    Tar,
    SevenZip,
    Lha,
    Pdf,
    Gzip,
    Bzip2,
    Xz,
}

impl fmt::Display for ArchiveKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            ArchiveKind::Zip => "ZIP",
            ArchiveKind::Rar => "RAR",
            ArchiveKind::Tar => "TAR",
            ArchiveKind::SevenZip => "7Z",
            ArchiveKind::Lha => "LHA",
            ArchiveKind::Pdf => "PDF",
            ArchiveKind::Gzip => "GZIP",
            ArchiveKind::Bzip2 => "BZIP2",
            ArchiveKind::Xz => "XZ",
        };
        f.write_str(s)
    }
}

/// Errors produced while opening or reading archives.
#[derive(Debug)]
pub enum ArchiveError {
    Unsupported(String),
    Io(std::io::Error),
    Other(String),
}

impl fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArchiveError::Unsupported(s) => write!(f, "unsupported archive: {s}"),
            ArchiveError::Io(e) => write!(f, "I/O error: {e}"),
            ArchiveError::Other(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for ArchiveError {}

impl From<std::io::Error> for ArchiveError {
    fn from(e: std::io::Error) -> Self {
        ArchiveError::Io(e)
    }
}

/// A comic archive: a list of page names plus on-demand byte access.
pub trait Archive: Send {
    /// The archive file name (as opened).
    fn name(&self) -> &str;

    /// All entries (unfiltered, unsorted): the page list is built from
    /// them, and `open_archive` uses them to tell a book of archives from
    /// an empty one. Defaults to the filtered page list.
    fn raw_names(&mut self) -> Result<Vec<String>, ArchiveError> {
        self.page_names()
    }

    /// Sorted list of image entries inside the archive.
    fn page_names(&mut self) -> Result<Vec<String>, ArchiveError>;

    /// Read the raw bytes of one entry (by name, as returned by `page_names`).
    fn read(&mut self, name: &str) -> Result<Vec<u8>, ArchiveError>;

    /// Release any held resources (temp dirs, file handles).
    fn close(&mut self) {}

    /// Another handle on the same archive, for reading from another thread.
    /// It shares whatever this one has already listed and unpacked, so
    /// nothing is listed or unpacked again. `None`: open the path again.
    fn try_clone(&self) -> Option<Box<dyn Archive>> {
        None
    }
}

/// A temp file made while unpacking (a decompressed tar), removed when the
/// last archive handle sharing it (see `Archive::try_clone`) is dropped.
pub(crate) struct TempPath(pub(crate) PathBuf);

impl Drop for TempPath {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_file(&self.0) {
            if e.kind() != std::io::ErrorKind::NotFound {
                log::warn!("cannot remove {}: {e}", self.0.display());
            }
        }
    }
}

/// Detect the archive kind from magic bytes.
pub fn detect(path: &Path) -> Option<ArchiveKind> {
    let mut f = File::open(path).ok()?;
    let mut magic = [0u8; 262];
    let n = f.read(&mut magic).ok()?;
    let magic = &magic[..n];

    // ZIP: PK\x03\x04 (normal), PK\x05\x06 (empty), PK\x07\x08 (spanned)
    if magic.starts_with(b"PK\x03\x04")
        || magic.starts_with(b"PK\x05\x06")
        || magic.starts_with(b"PK\x07\x08")
    {
        return Some(ArchiveKind::Zip);
    }
    // RAR
    if magic.starts_with(b"Rar!\x1a\x07") {
        return Some(ArchiveKind::Rar);
    }
    // 7z
    if magic.starts_with(b"7z\xbc\xaf") {
        return Some(ArchiveKind::SevenZip);
    }
    // PDF
    if magic.starts_with(b"%PDF") {
        return Some(ArchiveKind::Pdf);
    }
    // LHA / LZH (offset 2: "-l")
    if magic.len() > 4 && &magic[2..4] == b"-l" {
        return Some(ArchiveKind::Lha);
    }
    // Tar (ustar magic at offset 257)
    if magic.len() >= 262 && &magic[257..262] == b"ustar" {
        return Some(ArchiveKind::Tar);
    }
    // Tar compressed with gzip / bzip2 (the ustar magic is inside the
    // compressed stream, so it can't be checked here)
    if magic.starts_with(b"BZh") {
        return Some(ArchiveKind::Bzip2);
    }
    if magic.starts_with(&[0x1f, 0x8b]) {
        return Some(ArchiveKind::Gzip);
    }
    // Tar compressed with xz / lzma (not recognized by `tar` directly)
    if magic.starts_with(&[0xfd, b'7', b'z', b'X', b'Z'])
        || magic.starts_with(&[0x5d, 0x00, 0x00, 0x80, 0x00])
    {
        return Some(ArchiveKind::Xz);
    }
    None
}

/// Open a folder of images or an archive. Archives stored inside an
/// archive are not opened: their entries are not pages.
pub fn open(path: &Path) -> Result<Box<dyn Archive>, ArchiveError> {
    if path.is_dir() {
        return Ok(Box::new(DirArchive::open(path)));
    }
    let kind = detect(path).ok_or_else(|| ArchiveError::Unsupported(path.display().to_string()))?;
    open_with_kind(path, kind)
}

/// Open an archive of an already-detected kind.
pub fn open_with_kind(path: &Path, kind: ArchiveKind) -> Result<Box<dyn Archive>, ArchiveError> {
    log::info!("detected {kind} archive: {}", path.display());
    match kind {
        ArchiveKind::Zip | ArchiveKind::Gzip | ArchiveKind::Bzip2 | ArchiveKind::Xz => {
            // Gzip/bzip2/xz always wrap a tar in the comic world; the zip reader
            // handles plain CBZ. Compressed tars are dispatched to the tar reader.
            if kind == ArchiveKind::Zip {
                Ok(Box::new(zip::ZipArchive::open(path)?))
            } else {
                Ok(Box::new(tar::TarArchive::open(path, Some(kind))?))
            }
        }
        ArchiveKind::Tar => Ok(Box::new(tar::TarArchive::open(path, None)?)),
        ArchiveKind::SevenZip => Ok(Box::new(sevenzip::SevenZipArchive::open(path)?)),
        ArchiveKind::Rar => Ok(Box::new(crate::archive::rar::RarArchive::open(path)?)),
        ArchiveKind::Lha => Ok(Box::new(crate::archive::lha::LhaArchive::open(path)?)),
        ArchiveKind::Pdf => Ok(Box::new(crate::archive::pdf::PdfArchive::open(path)?)),
    }
}

/// Convenience: sort page names naturally and filter to images only.
pub fn sorted_pages(names: Vec<String>) -> Vec<String> {
    let mut v: Vec<String> = names.into_iter().filter(|n| is_image_file(n)).collect();
    natsort::natural_sort(&mut v);
    v
}

/// The first of `cmds` that is on PATH.
pub(crate) fn find_first(cmds: &[&str]) -> Option<String> {
    cmds.iter()
        .find(|c| which(c).is_some())
        .map(|c| c.to_string())
}

/// Find an executable on PATH.
pub(crate) fn which(cmd: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(cmd);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let candidate = dir.join(format!("{cmd}.exe"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// The largest page Nira reads out of an archive. Real comic pages are a
/// few MiB; the image decoder refuses more than 512 MiB of pixels anyway.
/// The cap stops a crafted entry (a zip bomb, or a tool printing without
/// end) from filling memory before the decoder gets to say no.
pub(crate) const MAX_ENTRY_BYTES: u64 = 256 * 1024 * 1024;

/// The most a compressed tar may unpack to on disk.
pub(crate) const MAX_UNPACKED_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// Read all of `r`, failing once it passes `limit` bytes. `what` names the
/// data in the error.
pub(crate) fn read_capped(r: impl Read, limit: u64, what: &str) -> Result<Vec<u8>, ArchiveError> {
    let mut buf = Vec::new();
    r.take(limit + 1).read_to_end(&mut buf)?;
    if buf.len() as u64 > limit {
        return Err(too_large(what, limit));
    }
    Ok(buf)
}

pub(crate) fn too_large(what: &str, limit: u64) -> ArchiveError {
    ArchiveError::Other(format!(
        "{what} is larger than {} MiB",
        limit / (1024 * 1024)
    ))
}

/// Run a command and capture stdout (and stderr on failure), at most
/// `MAX_ENTRY_BYTES` of it.
pub(crate) fn run_capture(prog: &str, args: &[&str]) -> Result<Vec<u8>, ArchiveError> {
    run_tool(prog, args, None, MAX_ENTRY_BYTES)
}

/// Run `prog`, write `stdin` to it (or close its input), and return its
/// stdout. Output past `limit` bytes stops the tool and fails.
pub(crate) fn run_tool(
    prog: &str,
    args: &[&str],
    stdin: Option<&str>,
    limit: u64,
) -> Result<Vec<u8>, ArchiveError> {
    let failed = |e: std::io::Error| ArchiveError::Other(format!("failed to run {prog}: {e}"));
    let mut child = Command::new(prog)
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(failed)?;
    if let (Some(text), Some(mut input)) = (stdin, child.stdin.take()) {
        // Small (a file name); dropping `input` closes the tool's stdin.
        input.write_all(text.as_bytes()).map_err(failed)?;
    }
    // Drain stderr alongside stdout, so a tool filling one pipe while we
    // wait on the other can't stall.
    let stderr = child.stderr.take().map(|mut err| {
        std::thread::spawn(move || {
            let mut text = Vec::new();
            let _ = err.by_ref().take(64 * 1024).read_to_end(&mut text);
            let _ = std::io::copy(&mut err, &mut std::io::sink());
            text
        })
    });
    let stdout = child.stdout.take().expect("stdout is piped");
    let out = read_capped(stdout, limit, &format!("the output of {prog}"));
    if out.is_err() {
        let _ = child.kill();
    }
    let status = child.wait().map_err(failed)?;
    let stderr = stderr.and_then(|h| h.join().ok()).unwrap_or_default();
    let out = out?;
    if !status.success() {
        let stderr = String::from_utf8_lossy(&stderr);
        return Err(ArchiveError::Other(format!(
            "{prog} failed: {}",
            stderr.trim()
        )));
    }
    Ok(out)
}

/// A directory of loose images (opened directly).
#[derive(Clone)]
pub struct DirArchive {
    dir: PathBuf,
    name: String,
    pages: Option<Vec<String>>,
}

impl DirArchive {
    pub fn open(path: &Path) -> DirArchive {
        DirArchive {
            dir: path.to_path_buf(),
            name: path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string()),
            pages: None,
        }
    }
}

impl Archive for DirArchive {
    fn name(&self) -> &str {
        &self.name
    }

    fn try_clone(&self) -> Option<Box<dyn Archive>> {
        // Nothing is unpacked: every read runs from the file itself.
        Some(Box::new(self.clone()))
    }

    fn page_names(&mut self) -> Result<Vec<String>, ArchiveError> {
        if self.pages.is_none() {
            let mut names = Vec::new();
            for entry in std::fs::read_dir(&self.dir)? {
                let entry = entry?;
                let p = entry.path();
                if p.is_file() {
                    names.push(p.to_string_lossy().into_owned());
                }
            }
            self.pages = Some(sorted_pages(names));
        }
        Ok(self.pages.clone().unwrap_or_default())
    }

    fn read(&mut self, name: &str) -> Result<Vec<u8>, ArchiveError> {
        Ok(std::fs::read(name)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_are_capped() {
        let data = [7u8; 10];
        assert_eq!(read_capped(&data[..], 10, "x").unwrap().len(), 10);
        assert!(read_capped(&data[..], 11, "x").is_ok());
        let err = read_capped(&data[..], 9, "the page").unwrap_err();
        assert!(err.to_string().contains("the page is larger than"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn tool_output_is_capped_and_stdin_is_passed() {
        let flood = run_tool("sh", &["-c", "head -c 100 /dev/zero"], None, 10);
        assert!(flood.is_err());
        // A tool that never stops printing is stopped too.
        assert!(run_tool("yes", &[], None, 1024).is_err());
        let echoed = run_tool("cat", &[], Some("@eh.png\n"), 1024).unwrap();
        assert_eq!(echoed, b"@eh.png\n");
        let fail = run_tool("sh", &["-c", "echo oops >&2; exit 3"], None, 1024).unwrap_err();
        assert!(fail.to_string().contains("oops"), "{fail}");
    }
}

/// Fixtures whose entry names look like switches (`-rg.jpeg`, `--o.JPG`,
/// `+ar.jpg`, `@eh.png`) or wildcards (`*oo.JPG`, `?eh.png`, `[rg.jpeg`),
/// for checking that every backend reads them as plain names.
#[cfg(test)]
pub(crate) mod tricky_names {
    use super::*;
    use std::collections::BTreeMap;

    pub fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../test/files/archives")
            .join(name)
    }

    /// `<set>.zip` read with the pure-Rust zip reader, which takes no
    /// command line: the bytes every other backend must match.
    fn reference(set: &str) -> BTreeMap<String, Vec<u8>> {
        let mut zip = zip::ZipArchive::open(&fixture(&format!("{set}.zip"))).unwrap();
        zip.page_names()
            .unwrap()
            .into_iter()
            .map(|n| {
                let bytes = zip.read(&n).unwrap();
                (n, bytes)
            })
            .collect()
    }

    /// Read every page of `ar` (a copy of fixture set `set`, "OptEntry" or
    /// "GlobEntries") and compare it with the zip reference. `unlisted`:
    /// names the backend's tool itself never lists.
    pub fn assert_reads_like_zip(mut ar: impl Archive, set: &str, label: &str, unlisted: &[&str]) {
        let mut want = reference(set);
        want.retain(|name, _| !unlisted.contains(&name.as_str()));
        let mut names = ar.page_names().unwrap();
        names.sort();
        assert_eq!(names, want.keys().cloned().collect::<Vec<_>>(), "{label}");
        for (name, bytes) in &want {
            let got = ar
                .read(name)
                .unwrap_or_else(|e| panic!("{label} {name}: {e}"));
            assert!(
                got == *bytes,
                "{label} {name}: {} bytes, want {}",
                got.len(),
                bytes.len()
            );
        }
    }
}
