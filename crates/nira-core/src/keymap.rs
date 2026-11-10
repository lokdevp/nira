//! Keyboard shortcuts: which accelerators trigger which action.
//!
//! Accelerators are GTK accelerator strings (`Page_Down`, `<Control>s`,
//! `KP_Add`); this module only stores them; the app validates and installs
//! them. User overrides live in `keys.toml`, one entry per action that differs
//! from the defaults:
//!
//! ```toml
//! next-page = ["Page_Down", "n"]
//! rotate-180 = []
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

macro_rules! actions {
    ($($variant:ident => $name:literal, [$($accel:literal),*];)*) => {
        /// A command the user can bind to a key. `name()` is also the
        /// GAction name (`win.<name>`) and the Fluent id suffix
        /// (`action-<name>`) of its label.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Action {
            $($variant,)*
        }

        impl Action {
            pub const ALL: &'static [Action] = &[$(Action::$variant,)*];

            pub fn name(self) -> &'static str {
                match self {
                    $(Action::$variant => $name,)*
                }
            }

            pub fn from_name(name: &str) -> Option<Action> {
                match name {
                    $($name => Some(Action::$variant),)*
                    _ => None,
                }
            }

            /// The accelerators bound when the user has not changed them.
            pub fn default_accels(self) -> &'static [&'static str] {
                match self {
                    $(Action::$variant => &[$($accel),*],)*
                }
            }
        }
    };
}

actions! {
    NextPage => "next-page", ["Page_Down", "KP_Page_Down", "<Alt>Right"];
    PrevPage => "previous-page", ["Page_Up", "KP_Page_Up", "BackSpace", "<Alt>Left"];
    ForwardTen => "forward-ten-pages", ["<Shift>Page_Down", "<Shift>KP_Page_Down"];
    BackTen => "back-ten-pages", ["<Shift>Page_Up", "<Shift>KP_Page_Up", "<Shift>BackSpace"];
    FirstPage => "first-page", ["Home", "KP_Home"];
    LastPage => "last-page", ["End", "KP_End"];
    GoToPage => "go-to-page", ["g"];
    ScrollUp => "scroll-up", ["Up", "KP_Up"];
    ScrollDown => "scroll-down", ["Down", "KP_Down"];
    ScrollLeft => "scroll-left", ["Left", "KP_Left"];
    ScrollRight => "scroll-right", ["Right", "KP_Right"];
    SmartScrollUp => "smart-scroll-up", ["<Shift>space"];
    SmartScrollDown => "smart-scroll-down", ["space"];
    ZoomIn => "zoom-in", ["plus", "KP_Add", "equal"];
    ZoomOut => "zoom-out", ["minus", "KP_Subtract"];
    ZoomReset => "zoom-reset", ["<Control>0", "KP_0"];
    FitBest => "fit-best", ["b"];
    FitWidth => "fit-width", ["w"];
    FitHeight => "fit-height", ["h"];
    FitSize => "fit-size", ["s"];
    FitManual => "fit-manual", ["a"];
    RotateCw => "rotate-cw", ["r"];
    RotateCcw => "rotate-ccw", ["<Shift>r"];
    Rotate180 => "rotate-180", [];
    FlipH => "flip-horizontal", [];
    FlipV => "flip-vertical", [];
    DoublePage => "double-page", ["d"];
    MangaMode => "manga-mode", ["m"];
    Fullscreen => "fullscreen", ["f", "F11"];
    ExitFullscreen => "exit-fullscreen", ["Escape"];
    Thumbnails => "thumbnails", ["F9"];
    Toolbar => "toolbar", ["<Control>m"];
    HideAll => "hide-all", ["i"];
    Slideshow => "slideshow", ["<Control>s"];
    InvertSmartScroll => "invert-smart-scroll", ["x"];
    PageInfo => "page-info", ["Tab"];
    Minimize => "minimize", ["n"];
    AddBookmark => "add-bookmark", ["<Control>d"];
    EditBookmarks => "edit-bookmarks", ["<Control>b"];
    Lens => "lens", ["l"];
}

/// Accelerators per action: the defaults with the user's overrides applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keymap {
    accels: BTreeMap<Action, Vec<String>>,
}

impl Default for Keymap {
    fn default() -> Self {
        let accels = Action::ALL
            .iter()
            .map(|&a| {
                (
                    a,
                    a.default_accels().iter().map(|s| s.to_string()).collect(),
                )
            })
            .collect();
        Keymap { accels }
    }
}

/// On-disk form: action name → accelerators, only for changed actions.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(transparent)]
struct KeyFile(BTreeMap<String, Vec<String>>);

impl Keymap {
    pub fn path() -> PathBuf {
        crate::paths::config_dir().join("keys.toml")
    }

    pub fn accels(&self, action: Action) -> &[String] {
        self.accels.get(&action).map_or(&[], Vec::as_slice)
    }

    pub fn set_accels(&mut self, action: Action, accels: Vec<String>) {
        self.accels.insert(action, accels);
    }

    /// The action bound to `accel`, if any.
    pub fn action_for(&self, accel: &str) -> Option<Action> {
        self.accels
            .iter()
            .find(|(_, v)| v.iter().any(|a| a == accel))
            .map(|(&a, _)| a)
    }

    /// Parse a `keys.toml` text over the defaults. Unknown actions are
    /// skipped with a warning.
    pub fn from_toml(text: &str) -> Result<Keymap, toml::de::Error> {
        let file: KeyFile = toml::from_str(text)?;
        let mut map = Keymap::default();
        for (name, accels) in file.0 {
            match Action::from_name(&name) {
                Some(a) => map.set_accels(a, accels),
                None => log::warn!("keys.toml: unknown action {name:?}"),
            }
        }
        Ok(map)
    }

    /// The `keys.toml` text: only the actions that differ from the defaults.
    pub fn to_toml(&self) -> String {
        let changed = self
            .accels
            .iter()
            .filter(|(a, v)| {
                v.iter()
                    .map(String::as_str)
                    .ne(a.default_accels().iter().copied())
            })
            .map(|(a, v)| (a.name().to_string(), v.clone()))
            .collect();
        toml::to_string(&KeyFile(changed)).unwrap_or_default()
    }

    pub fn load() -> Keymap {
        let path = Self::path();
        let Ok(text) = fs::read_to_string(&path) else {
            return Keymap::default();
        };
        Keymap::from_toml(&text).unwrap_or_else(|e| {
            log::warn!("cannot parse {}: {e}; using default keys", path.display());
            crate::paths::set_aside_corrupt(&path);
            Keymap::default()
        })
    }

    pub fn save(&self) {
        crate::paths::write_config(&Self::path(), &self.to_toml());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_and_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for &a in Action::ALL {
            assert_eq!(Action::from_name(a.name()), Some(a));
            assert!(seen.insert(a.name()), "duplicate name {}", a.name());
        }
    }

    #[test]
    fn no_accel_is_bound_twice_by_default() {
        let mut seen = std::collections::HashMap::new();
        for &a in Action::ALL {
            for accel in a.default_accels() {
                if let Some(prev) = seen.insert(*accel, a) {
                    panic!("{accel} bound to {prev:?} and {a:?}");
                }
            }
        }
    }

    #[test]
    fn only_changes_are_saved_and_reloaded() {
        let mut map = Keymap::default();
        assert_eq!(map.to_toml().trim(), "");
        map.set_accels(Action::Rotate180, vec!["<Shift>t".into()]);
        map.set_accels(Action::Lens, vec![]);
        let text = map.to_toml();
        assert!(text.contains("rotate-180"));
        assert!(!text.contains("next-page"));
        assert_eq!(Keymap::from_toml(&text).unwrap(), map);
        assert_eq!(map.action_for("<Shift>t"), Some(Action::Rotate180));
        assert_eq!(map.action_for("l"), None);
    }

    #[test]
    fn unknown_actions_are_ignored() {
        let map = Keymap::from_toml("bogus = [\"q\"]\n").unwrap();
        assert_eq!(map, Keymap::default());
    }
}
