//! CBT / TAR support (plain, gzip, bzip2, xz compressed).
//!
//! The archive is walked once, recording where each entry's data starts, and
//! pages are then read by seeking. A compressed stream is decompressed once,
//! into a temp file, during that walk.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use super::{sorted_pages, Archive, ArchiveError, ArchiveKind, TempPath};

static TMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Where each regular entry's data sits in the uncompressed tar stream.
struct TarIndex {
    /// All entry names, in archive order (directories included).
    names: Vec<String>,
    /// Entry name -> (offset, size) of its data.
    entries: HashMap<String, (u64, u64)>,
}

pub struct TarArchive {
    path: PathBuf,
    name: String,
    kind: Option<ArchiveKind>,
    pages: Option<Vec<String>>,
    /// Shared with clones (`try_clone`), like `unpacked`.
    index: Option<Arc<TarIndex>>,
    /// Decompressed copy of a gzip/bzip2/xz stream, written while indexing
    /// so pages can be read by seeking instead of decompressing from the
    /// start for every page. Shared with clones; removed with the last one.
    unpacked: Option<Arc<TempPath>>,
}

fn open_tar_reader(
    path: &Path,
    kind: Option<ArchiveKind>,
) -> Result<Box<dyn Read + Send>, ArchiveError> {
    let file = File::open(path)?;
    let reader: Box<dyn Read + Send> = match kind {
        None => Box::new(BufReader::new(file)),
        Some(ArchiveKind::Gzip) => Box::new(flate2::read::GzDecoder::new(file)),
        Some(ArchiveKind::Bzip2) => Box::new(bzip2::read::BzDecoder::new(file)),
        Some(ArchiveKind::Xz) => Box::new(xz2::read::XzDecoder::new(file)),
        _ => Box::new(BufReader::new(file)),
    };
    Ok(reader)
}

/// True if `kind` is a compressed tar stream (not seekable as is).
fn is_compressed(kind: Option<ArchiveKind>) -> bool {
    matches!(
        kind,
        Some(ArchiveKind::Gzip) | Some(ArchiveKind::Bzip2) | Some(ArchiveKind::Xz)
    )
}

/// A reader that copies everything it reads into `out`.
struct Tee<'a, R: Read> {
    inner: R,
    out: &'a mut BufWriter<File>,
}

impl<R: Read> Read for Tee<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.out.write_all(&buf[..n])?;
        Ok(n)
    }
}

/// Walk the tar stream once, recording every entry's data offset and size.
/// Returns the reader so the caller can drain what follows the last entry.
fn build_index<R: Read>(reader: R) -> Result<(TarIndex, R), ArchiveError> {
    let mut archive = tar::Archive::new(reader);
    let mut names = Vec::new();
    let mut entries = HashMap::new();
    for entry in archive.entries()? {
        let entry = entry?;
        let name = entry.path()?.to_string_lossy().into_owned();
        if entry.header().entry_type().is_file() {
            entries.insert(name.clone(), (entry.raw_file_position(), entry.size()));
        }
        names.push(name);
    }
    Ok((TarIndex { names, entries }, archive.into_inner()))
}

impl TarArchive {
    pub fn open(path: &Path, kind: Option<ArchiveKind>) -> Result<TarArchive, ArchiveError> {
        // Validate that it really is a tar stream.
        // A decompressing reader may return a short first read, so read up
        // to the full header.
        let mut head = Vec::with_capacity(512);
        open_tar_reader(path, kind)?
            .take(512)
            .read_to_end(&mut head)?;
        if head.len() < 512 || head[257..262] != *b"ustar" {
            return Err(ArchiveError::Other(format!(
                "'{}' is not a valid TAR archive",
                path.display()
            )));
        }
        Ok(TarArchive {
            path: path.to_path_buf(),
            name: path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string()),
            kind,
            pages: None,
            index: None,
            unpacked: None,
        })
    }

    /// Decompress the stream into a temp file while indexing it.
    fn index_compressed(&mut self) -> Result<TarIndex, ArchiveError> {
        let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp = crate::paths::unpack_dir().join(format!("tar-{n}.tar"));
        let result = (|| {
            let mut out = BufWriter::new(File::create(&tmp)?);
            let tee = Tee {
                inner: open_tar_reader(&self.path, self.kind)?,
                out: &mut out,
            };
            let (index, mut rest) = build_index(tee)?;
            // Copy the trailer too, so the temp file is a whole tar.
            std::io::copy(&mut rest, &mut std::io::sink())?;
            drop(rest);
            out.flush()?;
            Ok::<_, ArchiveError>(index)
        })();
        match result {
            Ok(index) => {
                self.unpacked = Some(Arc::new(TempPath(tmp)));
                Ok(index)
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(e)
            }
        }
    }

    /// Index the archive on first use.
    fn ensure_index(&mut self) -> Result<&TarIndex, ArchiveError> {
        if self.index.is_none() {
            let index = if is_compressed(self.kind) {
                self.index_compressed()?
            } else {
                build_index(BufReader::new(File::open(&self.path)?))?.0
            };
            self.index = Some(Arc::new(index));
        }
        Ok(self.index.as_deref().expect("index was just built"))
    }
}

impl Archive for TarArchive {
    fn name(&self) -> &str {
        &self.name
    }

    fn try_clone(&self) -> Option<Box<dyn Archive>> {
        Some(Box::new(TarArchive {
            path: self.path.clone(),
            name: self.name.clone(),
            kind: self.kind,
            pages: self.pages.clone(),
            index: self.index.clone(),
            unpacked: self.unpacked.clone(),
        }))
    }

    fn raw_names(&mut self) -> Result<Vec<String>, ArchiveError> {
        let mut names = self.ensure_index()?.names.clone();
        names.retain(|n| !n.ends_with('/'));
        Ok(names)
    }

    fn page_names(&mut self) -> Result<Vec<String>, ArchiveError> {
        if self.pages.is_none() {
            self.pages = Some(sorted_pages(self.raw_names()?));
        }
        Ok(self.pages.clone().unwrap_or_default())
    }

    fn read(&mut self, name: &str) -> Result<Vec<u8>, ArchiveError> {
        let entry = self.ensure_index()?.entries.get(name).copied();
        let (offset, size) = entry.ok_or_else(|| {
            ArchiveError::Other(format!(
                "entry '{name}' not found in '{}'",
                self.path.display()
            ))
        })?;
        let data = self
            .unpacked
            .as_ref()
            .map_or(self.path.as_path(), |t| t.0.as_path());
        let mut file = File::open(data)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut buf = vec![0; size as usize];
        file.read_exact(&mut buf)?;
        Ok(buf)
    }

    fn close(&mut self) {
        // The unpacked file goes once no clone uses it (`TempPath`).
        self.unpacked = None;
        self.index = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::detect;

    fn archive_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("test")
            .join("files")
            .join("archives")
            .join(name)
    }

    #[test]
    fn detects_and_lists_plain_and_compressed_tars() {
        for (name, kind) in [
            ("SolidFlat.tar", ArchiveKind::Tar),
            ("SolidFlat.tar.gz", ArchiveKind::Gzip),
            ("SolidFlat.tar.bz2", ArchiveKind::Bzip2),
            ("SolidFlat.tar.xz", ArchiveKind::Xz),
        ] {
            let path = archive_path(name);
            assert_eq!(detect(&path), Some(kind), "{name}");
            let mut ar = crate::archive::open(&path).expect(name);
            assert_eq!(ar.page_names().expect(name).len(), 4, "{name}");
        }
    }

    #[test]
    fn reads_pages_by_offset_and_removes_temp_file() {
        let mut plain = TarArchive::open(&archive_path("SolidFlat.tar"), None).unwrap();
        let pages = plain.page_names().unwrap();
        let want: Vec<Vec<u8>> = pages.iter().map(|p| plain.read(p).unwrap()).collect();
        assert!(want.iter().all(|b| image::guess_format(b).is_ok()));
        assert!(plain.unpacked.is_none());

        for (name, kind) in [
            ("SolidFlat.tar.gz", ArchiveKind::Gzip),
            ("SolidFlat.tar.bz2", ArchiveKind::Bzip2),
            ("SolidFlat.tar.xz", ArchiveKind::Xz),
        ] {
            let mut ar = TarArchive::open(&archive_path(name), Some(kind)).unwrap();
            assert_eq!(ar.page_names().unwrap(), pages, "{name}");
            // Read out of order: every read seeks, nothing is decompressed again.
            for (i, p) in pages.iter().enumerate().rev() {
                assert_eq!(ar.read(p).unwrap(), want[i], "{name} {p}");
            }
            let tmp = ar
                .unpacked
                .as_ref()
                .expect("compressed tar is unpacked")
                .0
                .clone();
            assert!(tmp.exists());
            // A clone reads the same unpacked file: nothing is unpacked again.
            let mut twin = ar.try_clone().expect("tars clone");
            assert_eq!(twin.read(&pages[0]).unwrap(), want[0], "{name} clone");
            ar.close();
            assert!(tmp.exists(), "{name}: kept while a clone uses it");
            drop(twin);
            assert!(
                !tmp.exists(),
                "{name}: temp file removed with the last handle"
            );
        }
    }
}
