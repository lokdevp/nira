//! Where Nira keeps its files: config (`~/.config/nira`), data
//! (`~/.local/share/nira`) and cache (`~/.cache/nira`). On Windows all three
//! live under `%APPDATA%\nira` (cache: `%LOCALAPPDATA%\nira`).

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Home directory: `$HOME` on Unix, `%APPDATA%` on Windows.
pub fn home_dir() -> PathBuf {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."))
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

/// Settings and keybindings.
pub fn config_dir() -> PathBuf {
    #[cfg(windows)]
    {
        home_dir().join("nira")
    }
    #[cfg(not(windows))]
    {
        xdg_dir("XDG_CONFIG_HOME", ".config")
    }
}

/// Library, bookmarks and last-read pages.
pub fn data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        home_dir().join("nira")
    }
    #[cfg(not(windows))]
    {
        xdg_dir("XDG_DATA_HOME", ".local/share")
    }
}

/// Thumbnails and unpacked archives.
pub fn cache_dir() -> PathBuf {
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(home_dir)
            .join("nira")
    }
    #[cfg(not(windows))]
    {
        xdg_dir("XDG_CACHE_HOME", ".cache")
    }
}

#[cfg(not(windows))]
fn xdg_dir(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(fallback))
        .join("nira")
}

/// Write `text` to `path`, creating its directory first. Failures are
/// logged, not returned: losing a settings write must not stop the app.
///
/// The write is atomic: the text goes to a temp file next to `path`, which
/// is flushed to disk and renamed over it. A crash or a full disk leaves
/// the old file intact instead of an empty or truncated one. A symlinked
/// `path` is written through to its target, so it stays a link.
pub fn write_config(path: &Path, text: &str) {
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if let Some(dir) = path.parent() {
        if let Err(e) = fs::create_dir_all(dir) {
            log::warn!("cannot create {}: {e}", dir.display());
            return;
        }
    }
    if let Err(e) = write_atomic(&path, text.as_bytes()) {
        log::warn!("cannot write {}: {e}", path.display());
    }
}

fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    // The process id keeps two running instances off each other's temp file.
    let tmp = with_suffix(path, &format!("{}.tmp", std::process::id()));
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        // Keep the old file's permissions (a user may have made it private).
        if let Ok(meta) = fs::metadata(path) {
            file.set_permissions(meta.permissions())?;
        }
        file.write_all(data)?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Move a file that could not be parsed to `<name>.corrupt`, so the
/// defaults used instead don't overwrite it on the next save.
pub fn set_aside_corrupt(path: &Path) {
    let dest = with_suffix(path, "corrupt");
    match fs::rename(path, &dest) {
        Ok(()) => log::warn!("kept the unreadable file as {}", dest.display()),
        Err(e) => log::warn!("cannot move {} aside: {e}", path.display()),
    }
}

/// `path` with `.suffix` appended to its file name.
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".");
    name.push(suffix);
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nira-paths-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn writes_replace_the_file_and_leave_no_temp_file() {
        let dir = scratch("replace");
        let path = dir.join("sub/settings.toml");
        write_config(&path, "first");
        write_config(&path, "second");
        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
        assert_eq!(names(&dir.join("sub")), ["settings.toml"]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn writes_go_through_a_symlink() {
        let dir = scratch("symlink");
        let target = dir.join("dotfiles-settings.toml");
        let link = dir.join("settings.toml");
        fs::write(&target, "old").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        write_config(&link, "new");
        assert!(fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_files_are_set_aside() {
        let dir = scratch("corrupt");
        let path = dir.join("history.json");
        fs::write(&path, "{\"items\": [").unwrap();
        set_aside_corrupt(&path);
        assert!(!path.exists());
        assert_eq!(
            fs::read_to_string(dir.join("history.json.corrupt")).unwrap(),
            "{\"items\": ["
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
