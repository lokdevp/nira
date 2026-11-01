//! Where Nira keeps its files: config (`~/.config/nira`), data
//! (`~/.local/share/nira`) and cache (`~/.cache/nira`). On Windows all three
//! live under `%APPDATA%\nira` (cache: `%LOCALAPPDATA%\nira`).

use std::path::PathBuf;

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
pub fn write_config(path: &std::path::Path, text: &str) {
    if let Some(dir) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            log::warn!("cannot create {}: {e}", dir.display());
            return;
        }
    }
    if let Err(e) = std::fs::write(path, text) {
        log::warn!("cannot write {}: {e}", path.display());
    }
}
