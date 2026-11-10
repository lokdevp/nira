//! PDF support via MuPDF's `mutool`.
//!
//! Pages are rendered on demand to stdout (`mutool draw -o -`), so no temp
//! directory is needed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use super::{run_capture, which, Archive, ArchiveError};

/// Render DPI per (PDF path, page), shared by every `PdfArchive` in the
/// process. The page decoder and the thumbnailer each open their own handle;
/// with the shared cache the `mutool` trace pass runs once per page, not on
/// every read.
fn dpi_cache() -> &'static Mutex<HashMap<(PathBuf, String), u32>> {
    static CACHE: OnceLock<Mutex<HashMap<(PathBuf, String), u32>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Default DPI used when the trace pass finds no images (72 * 4).
const PDF_RENDER_DPI_DEF: u32 = 288;
/// Upper cap for the per-page optimal DPI (72 * 10).
const PDF_RENDER_DPI_MAX: u32 = 720;

/// Extract the value of an XML attribute like `name="value"` from a line.
fn attr<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let pat = format!("{name}=\"");
    let start = line.find(&pat)? + pat.len();
    let end = line[start..].find('"')? + start;
    Some(&line[start..end])
}

/// The transformation attribute: modern mutool (>= ~1.18) emits `transform=`,
/// older versions `matrix=`. Take whichever is present.
fn transform_attr(line: &str) -> Option<&str> {
    attr(line, "transform").or_else(|| attr(line, "matrix"))
}

/// Compute the optimal render DPI from a mutool trace (`-F trace`), mirroring
/// Find the largest embedded image and derive the DPI so it
/// is rendered at its native resolution (capped at PDF_RENDER_DPI_MAX).
fn parse_optimal_dpi(trace: &str) -> u32 {
    let mut max_size: u32 = 0;
    let mut max_dpi: u32 = PDF_RENDER_DPI_DEF;
    for line in trace.lines() {
        let line = line.trim();
        if !line.contains("<fill_image") {
            continue;
        }
        let (Some(matrix), Some(width), Some(height)) = (
            transform_attr(line),
            attr(line, "width"),
            attr(line, "height"),
        ) else {
            continue;
        };
        let (Ok(width), Ok(height)) = (width.parse::<f64>(), height.parse::<f64>()) else {
            continue;
        };
        let m: Vec<f64> = matrix
            .split_whitespace()
            .filter_map(|x| x.parse::<f64>().ok())
            .collect();
        if m.len() < 4 {
            continue;
        }
        for (size, c1, c2) in [(width, m[0], m[1]), (height, m[2], m[3])] {
            let size_u = size as u32;
            if size_u < max_size {
                continue;
            }
            let render_size = (c1 * c1 + c2 * c2).sqrt();
            let dpi = (size * 72.0 / render_size) as i64;
            let dpi = dpi.clamp(72, PDF_RENDER_DPI_MAX as i64) as u32;
            max_size = size_u;
            max_dpi = dpi;
        }
    }
    max_dpi
}

#[derive(Clone)]
pub struct PdfArchive {
    path: PathBuf,
    name: String,
    pages: Option<Vec<String>>,
}

impl PdfArchive {
    /// Optimal DPI for one page: run the mutool trace pass and parse the
    /// largest embedded image . Cached per page.
    pub fn optimal_dpi(&self, page: &str) -> Result<u32, ArchiveError> {
        let key = (self.path.clone(), page.to_string());
        let cached = dpi_cache().lock().ok().and_then(|c| c.get(&key).copied());
        if let Some(dpi) = cached {
            return Ok(dpi);
        }
        let p = self.path.to_string_lossy().into_owned();
        let raw = run_capture("mutool", &["draw", "-F", "trace", "--", &p, page])?;
        let dpi = parse_optimal_dpi(&String::from_utf8_lossy(&raw));
        if let Ok(mut cache) = dpi_cache().lock() {
            cache.insert(key, dpi);
        }
        Ok(dpi)
    }

    pub fn open(path: &Path) -> Result<PdfArchive, ArchiveError> {
        if which("mutool").is_none() {
            return Err(ArchiveError::Other(
                "PDF support requires the 'mutool' executable (mupdf-tools)".to_string(),
            ));
        }
        Ok(PdfArchive {
            path: path.to_path_buf(),
            name: path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string()),
            pages: None,
        })
    }
}

impl Archive for PdfArchive {
    fn name(&self) -> &str {
        &self.name
    }

    fn try_clone(&self) -> Option<Box<dyn Archive>> {
        // Nothing is unpacked: every read runs from the file itself.
        Some(Box::new(self.clone()))
    }

    fn page_names(&mut self) -> Result<Vec<String>, ArchiveError> {
        if self.pages.is_none() {
            let p = self.path.to_string_lossy().into_owned();
            // `mutool show -- <pdf> pages` prints one `page N = ...` line per
            // page. Names are "N.png".
            let raw = run_capture("mutool", &["show", "--", &p, "pages"])?;
            let text = String::from_utf8_lossy(&raw);
            let mut names = Vec::new();
            for line in text.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("page ") {
                    if let Some(num) = rest.split_whitespace().next() {
                        if num.chars().all(|c| c.is_ascii_digit()) {
                            names.push(format!("{num}.png"));
                        }
                    }
                }
            }
            self.pages = Some(names);
        }
        Ok(self.pages.clone().unwrap_or_default())
    }

    fn read(&mut self, name: &str) -> Result<Vec<u8>, ArchiveError> {
        let page = name
            .strip_suffix(".png")
            .ok_or_else(|| ArchiveError::Other(format!("invalid PDF page name '{name}'")))?;
        let p = self.path.to_string_lossy().into_owned();
        let dpi = self.optimal_dpi(page)?.to_string();
        log::debug!("PDF page {page}: rendering at {dpi} DPI");
        // `mutool draw -F png -r <dpi> -o - -- <pdf> <page>` writes the
        // rendered page to stdout.
        run_capture(
            "mutool",
            &["draw", "-F", "png", "-r", &dpi, "-o", "-", "--", &p, page],
        )
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
    fn lists_and_reads_pdf() {
        if which("mutool").is_none() {
            eprintln!("skipping: mutool not available");
            return;
        }
        let path = archive_path("01-PDF-Normal.pdf");
        if !path.exists() {
            eprintln!("skipping: test archive not present");
            return;
        }
        let mut ar = PdfArchive::open(&path).expect("open pdf");
        let pages = ar.page_names().expect("list pages");
        assert_eq!(pages.len(), 2);
        let bytes = ar.read(&pages[0]).expect("render first page");
        assert!(image::guess_format(&bytes).is_ok());
        // A second handle (as the thumbnailer has) reuses the traced DPI.
        let key = (path.clone(), "1".to_string());
        assert!(dpi_cache().lock().unwrap().contains_key(&key));
        let mut other = PdfArchive::open(&path).expect("open pdf again");
        assert_eq!(other.read(&pages[0]).expect("render again"), bytes);
    }
}

#[test]
fn optimal_dpi_uses_the_shared_cache() {
    // No mutool needed: a cached page never runs the trace pass.
    let ar = PdfArchive {
        path: PathBuf::from("/nonexistent/cached.pdf"),
        name: "cached.pdf".into(),
        pages: None,
    };
    dpi_cache()
        .lock()
        .unwrap()
        .insert((ar.path.clone(), "7".into()), 123);
    assert_eq!(ar.optimal_dpi("7").unwrap(), 123);
}

#[test]
fn parses_trace_dpi() {
    // Modern mutool emits transform=; a 2000px image drawn at 0.5 scale.
    let trace =
        "\n  <fill_image transform=\"0.5 0 0 0.5 10 20\" width=\"2000\" height=\"3000\"/>\n";
    let dpi = parse_optimal_dpi(trace);
    // dpi = 2000 * 72 / 0.5 = 288000 -> capped at 720
    assert_eq!(dpi, PDF_RENDER_DPI_MAX);

    // Old mutool used matrix=; also parsed.
    let trace = "<fill_image matrix=\"1 0 0 1 0 0\" width=\"200\" height=\"300\"/>";
    // width: 200*72/1 -> capped; height: 300*72/1 -> capped
    assert_eq!(parse_optimal_dpi(trace), PDF_RENDER_DPI_MAX);

    // A 200x300px image covering 200x300pt (72 dpi scan): native 72 dpi.
    let trace = "<fill_image transform=\"200 0 0 300 0 0\" width=\"200\" height=\"300\"/>";
    assert_eq!(parse_optimal_dpi(trace), 72);

    // No images -> default.
    assert_eq!(parse_optimal_dpi(""), PDF_RENDER_DPI_DEF);
}
