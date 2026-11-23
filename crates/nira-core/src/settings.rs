//! User settings, stored as `settings.toml` in the config directory.
//!
//! Every section and field has a default, so a file only needs the keys the
//! user changed, and a file written by an older version keeps working.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::zoom::{FitMode, FitSize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub view: View,
    pub appearance: Appearance,
    pub scroll: Scroll,
    pub files: Files,
    pub slideshow: Slideshow,
    pub enhance: Enhance,
    pub lens: Lens,
    pub window: Window,
}

/// How pages are laid out when a book opens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct View {
    pub fit: FitMode,
    pub fit_size: FitSize,
    /// Let fitting enlarge pages beyond 100 %.
    pub scale_up: bool,
    pub double_page: bool,
    /// Right-to-left reading: the first page of a spread goes on the right.
    pub manga: bool,
    /// In double-page mode, show the cover alone.
    pub cover_alone: bool,
    /// In double-page mode, show wide pages (scanned spreads) alone.
    pub wide_alone: bool,
    /// In double-page mode, turn two pages at a time.
    pub double_step: bool,
    pub fullscreen: bool,
    /// Rotate pages by their EXIF orientation.
    pub exif_rotation: bool,
}

impl Default for View {
    fn default() -> Self {
        View {
            fit: FitMode::Best,
            fit_size: FitSize::default(),
            scale_up: false,
            double_page: false,
            manga: false,
            cover_alone: true,
            wide_alone: true,
            double_step: true,
            fullscreen: false,
            exif_rotation: true,
        }
    }
}

/// `Appearance::theme` value that follows the desktop's light/dark
/// preference (Nira Dark or Nira Light).
pub const THEME_SYSTEM: &str = "system";
/// The default theme: the design's dark palette.
pub const THEME_DEFAULT: &str = "dark";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    /// Colour theme id (the app's theme list, e.g. `"solarized-dark"`), or
    /// `"system"`. Unknown ids fall back to the default.
    pub theme: String,
    /// Viewer background, RGB.
    pub background: [u8; 3],
    /// Colour the viewer after the shown page's border instead.
    pub match_page_color: bool,
    pub show_thumbnails: bool,
    pub thumbnail_page_numbers: bool,
    pub show_view_panel: bool,
    /// The page sidebar in focus mode, remembered apart from normal mode.
    pub focus_show_thumbnails: bool,
    /// The View panel in focus mode, remembered apart from normal mode.
    pub focus_show_view_panel: bool,
    /// The page sidebar's width in pixels, dragged from its edge; shared
    /// by normal and focus mode. The app clamps it to the range it allows.
    pub sidebar_width: i32,
    /// Fade the page bar and click zones out while reading.
    pub auto_hide_controls: bool,
    /// Show the floating page bar outside focus mode.
    pub show_page_bar: bool,
    /// Show the progress line along the bottom edge in focus mode.
    pub show_focus_progress: bool,
    /// UI language (`"auto"` follows the system locale).
    pub language: String,
}

impl Appearance {
    /// (sidebar, View panel) shown in normal or focus mode.
    pub fn panels(&self, focus: bool) -> (bool, bool) {
        if focus {
            (self.focus_show_thumbnails, self.focus_show_view_panel)
        } else {
            (self.show_thumbnails, self.show_view_panel)
        }
    }
}

impl Default for Appearance {
    fn default() -> Self {
        Appearance {
            theme: THEME_DEFAULT.to_string(),
            background: [0x0E, 0x0F, 0x11],
            match_page_color: false,
            show_thumbnails: true,
            thumbnail_page_numbers: true,
            show_view_panel: false,
            focus_show_thumbnails: false,
            focus_show_view_panel: false,
            sidebar_width: 232,
            auto_hide_controls: true,
            show_page_bar: true,
            show_focus_progress: true,
            language: "auto".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Scroll {
    pub key_step_px: u32,
    pub wheel_step_px: u32,
    /// Space scrolls through the page in reading order, then turns it.
    pub smart_scroll: bool,
    pub invert_smart_scroll: bool,
    /// Share of the viewport one smart-scroll step moves (0–1).
    pub smart_scroll_fraction: f64,
    /// Presses against a page edge before the page turns.
    pub edge_presses_to_turn: u32,
    /// The mouse wheel turns the page at the page edges.
    pub flip_with_wheel: bool,
}

impl Default for Scroll {
    fn default() -> Self {
        Scroll {
            key_step_px: 50,
            wheel_step_px: 50,
            smart_scroll: true,
            invert_smart_scroll: false,
            smart_scroll_fraction: 0.5,
            edge_presses_to_turn: 3,
            flip_with_wheel: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Files {
    /// After the last page, open the next book in the folder.
    pub open_next_book: bool,
    /// After the last book, continue in the next folder.
    pub open_next_folder: bool,
    /// Reopen the last book at startup.
    pub reopen_last: bool,
    /// Decoded pages kept in memory.
    pub cached_pages: u32,
    /// Escape quits instead of leaving fullscreen.
    pub escape_quits: bool,
}

impl Default for Files {
    fn default() -> Self {
        Files {
            open_next_book: true,
            open_next_folder: true,
            reopen_last: false,
            cached_pages: 7,
            escape_quits: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Slideshow {
    pub delay_ms: u64,
    /// Continue into the next book at the end.
    pub next_book: bool,
}

impl Default for Slideshow {
    fn default() -> Self {
        Slideshow {
            delay_ms: 3000,
            next_book: true,
        }
    }
}

/// Brightness and contrast (1.0 = unchanged).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Enhance {
    pub brightness: f64,
    pub contrast: f64,
    /// Levels, de-yellowing and sharpening for scans.
    pub auto_enhance: bool,
}

impl Default for Enhance {
    fn default() -> Self {
        Enhance {
            brightness: 1.0,
            contrast: 1.0,
            auto_enhance: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Lens {
    /// Diameter in pixels.
    pub size: u32,
    /// Enlargement over the page as shown (the wheel changes it while the
    /// lens is on).
    pub magnification: f64,
}

/// Lens magnifications the wheel steps through.
pub const LENS_MIN: f64 = 1.5;
pub const LENS_MAX: f64 = 6.0;
pub const LENS_STEP: f64 = 0.5;

impl Default for Lens {
    fn default() -> Self {
        Lens {
            size: 200,
            magnification: 2.5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Window {
    pub width: i32,
    pub height: i32,
}

impl Default for Window {
    fn default() -> Self {
        Window {
            width: 1024,
            height: 720,
        }
    }
}

impl Settings {
    pub fn path() -> PathBuf {
        crate::paths::config_dir().join("settings.toml")
    }

    pub fn from_toml(text: &str) -> Result<Settings, toml::de::Error> {
        toml::from_str(text)
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }

    /// Load the settings, or the defaults if the file is missing or invalid.
    pub fn load() -> Settings {
        let path = Self::path();
        let Ok(text) = fs::read_to_string(&path) else {
            return Settings::default();
        };
        Settings::from_toml(&text).unwrap_or_else(|e| {
            log::warn!("cannot parse {}: {e}; using defaults", path.display());
            crate::paths::set_aside_corrupt(&path);
            Settings::default()
        })
    }

    pub fn save(&self) {
        crate::paths::write_config(&Self::path(), &self.to_toml());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zoom::FitSizeAxis;

    #[test]
    fn round_trips_through_toml() {
        let mut s = Settings::default();
        s.view.fit = FitMode::Width;
        s.view.fit_size.axis = FitSizeAxis::Both;
        s.appearance.theme = "solarized-dark".to_string();
        assert_eq!(Settings::from_toml(&s.to_toml()).unwrap(), s);
    }

    #[test]
    fn old_theme_values_still_load() {
        for theme in ["system", "dark", "light"] {
            let s = Settings::from_toml(&format!("[appearance]\ntheme = \"{theme}\"\n")).unwrap();
            assert_eq!(s.appearance.theme, theme);
        }
    }

    #[test]
    fn missing_keys_keep_their_defaults() {
        let s = Settings::from_toml("[view]\nfit = \"height\"\n[window]\nwidth = 800\n").unwrap();
        assert_eq!(s.view.fit, FitMode::Height);
        assert!(s.view.double_step, "unset bool keeps its default true");
        assert_eq!(s.window.width, 800);
        assert_eq!(s.window.height, 720);
        assert_eq!(s.lens, Lens::default());
    }

    #[test]
    fn each_mode_has_its_own_panels() {
        let mut a = Appearance::default();
        assert_eq!(a.panels(true), (false, false), "focus starts without panels");
        a.show_thumbnails = true;
        a.show_view_panel = false;
        a.focus_show_thumbnails = false;
        a.focus_show_view_panel = true;
        assert_eq!(a.panels(false), (true, false));
        assert_eq!(a.panels(true), (false, true));
    }

    #[test]
    fn empty_file_is_all_defaults() {
        assert_eq!(Settings::from_toml("").unwrap(), Settings::default());
    }
}
