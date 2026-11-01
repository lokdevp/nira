//! The app stylesheet and its colour themes, the window icon and the
//! runtime viewer background.

use std::cell::{Cell, OnceCell, RefCell};

use gtk::gdk;
use nira_core::settings::{Settings, THEME_DEFAULT, THEME_SYSTEM};

/// The app icon, for runs where it isn't installed in the icon theme
/// (`cargo run`, the Windows zip, a copied binary).
const ICON_PNG: &[u8] = include_bytes!("../../../packaging/linux/icons/256x256/apps/nira.png");

/// Make the icon named `nira` resolve even when no package installed it:
/// write the embedded PNG to the cache and add that folder to the icon
/// theme's search path. Without this the title bar shows a missing-image
/// icon where the window icon goes.
pub fn ensure_app_icon() {
    let Some(display) = gdk::Display::default() else {
        return;
    };
    let theme = gtk::IconTheme::for_display(&display);
    if theme.has_icon("nira") {
        return;
    }
    let dir = nira_core::paths::cache_dir().join("icons");
    let path = dir.join("nira.png");
    if std::fs::read(&path).ok().as_deref() != Some(ICON_PNG) {
        if let Err(e) = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, ICON_PNG))
        {
            log::debug!("cannot write the app icon to {}: {e}", path.display());
            return;
        }
    }
    theme.add_search_path(&dir);
}

/// The main window's stylesheet; the theme's palette goes in front of it.
const STYLE: &str = include_str!("style.css");

/// A colour theme: a palette of `@nira_*` colours for `style.css`.
pub struct ThemeDef {
    /// The settings value; its name is the string `theme-<id>`.
    pub id: &'static str,
    /// Dark themes also make GTK's own dialogs dark.
    pub dark: bool,
    css: &'static str,
}

macro_rules! themes {
    ($($id:literal => $dark:literal),* $(,)?) => {
        &[$(ThemeDef {
            id: $id,
            dark: $dark,
            css: include_str!(concat!("themes/", $id, ".css")),
        }),*]
    };
}

/// Every theme, in the order Preferences lists them. Each file in
/// `themes/` defines the same colour names (a test checks).
pub const THEMES: &[ThemeDef] = themes![
    "dark" => true,
    "light" => false,
    "solarized-dark" => true,
    "solarized-light" => false,
    "vscode-dark" => true,
    "vscode-light" => false,
    "nord" => true,
    "dracula" => true,
    "gruvbox-dark" => true,
    "gruvbox-light" => false,
    "catppuccin-mocha" => true,
    "catppuccin-latte" => false,
];

/// The theme with this id, or the default for an unknown one.
pub fn theme_def(id: &str) -> &'static ThemeDef {
    THEMES
        .iter()
        .find(|t| t.id == id)
        .or_else(|| THEMES.iter().find(|t| t.id == THEME_DEFAULT))
        .unwrap_or(&THEMES[0])
}

/// Install the stylesheet for the whole display (once), with the default
/// theme until `apply_theme` picks one.
pub fn load_css() {
    load_palette(theme_def(THEME_DEFAULT));
}

/// (Re)load the main stylesheet with `theme`'s palette.
fn load_palette(theme: &'static ThemeDef) {
    STATE.with(|state| {
        if state.loaded.get() == Some(theme.id) {
            return;
        }
        state.loaded.set(Some(theme.id));
        let provider = state.style.get_or_init(|| {
            let p = gtk::CssProvider::new();
            add_provider(&p, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
            p
        });
        provider.load_from_string(&format!("{}\n{STYLE}", theme.css));
    });
}

fn add_provider(provider: &gtk::CssProvider, priority: u32) {
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(&display, provider, priority);
    }
}

thread_local! {
    /// Reused for every background change, so changes don't stack
    /// providers.
    static BACKGROUND: OnceCell<gtk::CssProvider> = const { OnceCell::new() };
    static STATE: ThemeState = ThemeState::default();
}

#[derive(Default)]
struct ThemeState {
    /// The main stylesheet's provider.
    style: OnceCell<gtk::CssProvider>,
    /// The theme loaded into it.
    loaded: Cell<Option<&'static str>>,
    /// The theme setting, read again when the desktop's preference changes.
    theme: RefCell<String>,
    /// Whether the desktop's preference is being watched.
    watching: Cell<bool>,
}

/// Colour the reading area: the settings' background, or `page` (the shown
/// page's border colour) when the background follows the page.
pub fn apply_background(settings: &Settings, page: Option<[u8; 3]>) {
    let [r, g, b] = page
        .filter(|_| settings.appearance.match_page_color)
        .unwrap_or(settings.appearance.background);
    BACKGROUND.with(|cell| {
        let provider = cell.get_or_init(|| {
            let p = gtk::CssProvider::new();
            add_provider(&p, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1);
            p
        });
        provider.load_from_string(&format!(
            "window.nira-main scrolledwindow.viewer {{ background-color: rgb({r}, {g}, {b}); }}"
        ));
    });
}

/// Colour the main window with theme `id` (`"system"` follows the
/// desktop's light/dark preference, and keeps following it while the app
/// runs). GTK's own dialogs follow the theme's lightness.
pub fn apply_theme(id: &str) {
    STATE.with(|state| state.theme.replace(id.to_string()));
    watch_system();
    refresh_theme();
}

fn refresh_theme() {
    let id = STATE.with(|state| state.theme.borrow().clone());
    let theme = if id == THEME_SYSTEM {
        theme_def(if system_prefers_dark() { "dark" } else { "light" })
    } else {
        theme_def(&id)
    };
    if let Some(settings) = gtk::Settings::default() {
        set_dialogs_dark(&settings, theme.dark);
    }
    load_palette(theme);
}

/// GTK ≥ 4.20 property for light/dark; it replaces the deprecated
/// `gtk-application-prefer-dark-theme`, which now warns when set.
const COLOR_SCHEME: &str = "gtk-interface-color-scheme";

/// Make GTK's own widgets (dialogs, menus) dark or light.
fn set_dialogs_dark(settings: &gtk::Settings, dark: bool) {
    use gtk::glib::prelude::*;
    // Looked up by name: the bindings target GTK 4.12, which lacks it.
    let value = settings.find_property(COLOR_SCHEME).and_then(|pspec| {
        gtk::glib::EnumClass::with_type(pspec.value_type())?
            .to_value_by_nick(if dark { "dark" } else { "light" })
    });
    match value {
        Some(value) => {
            let number = |v: &gtk::glib::Value| {
                gtk::glib::EnumValue::from_value(v).map(|(_, e)| e.value())
            };
            if number(&settings.property_value(COLOR_SCHEME)) != number(&value) {
                settings.set_property_from_value(COLOR_SCHEME, &value);
            }
        }
        None => {
            if settings.is_gtk_application_prefer_dark_theme() != dark {
                settings.set_gtk_application_prefer_dark_theme(dark);
            }
        }
    }
}

/// The desktop's light/dark preference: the freedesktop portal's colour
/// scheme where there is one, otherwise a "dark" GTK theme name.
fn system_prefers_dark() -> bool {
    if let Some(scheme) = portal_color_scheme() {
        // 0: no preference, 1: dark, 2: light.
        return scheme == 1;
    }
    gtk::Settings::default()
        .and_then(|s| s.gtk_theme_name())
        .is_some_and(|name| name.to_lowercase().contains("dark"))
}

const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const PORTAL_SETTINGS: &str = "org.freedesktop.portal.Settings";

#[cfg(unix)]
fn session_bus() -> Option<gtk::gio::DBusConnection> {
    gtk::gio::bus_get_sync(gtk::gio::BusType::Session, gtk::gio::Cancellable::NONE).ok()
}

#[cfg(not(unix))]
fn session_bus() -> Option<gtk::gio::DBusConnection> {
    None
}

/// `org.freedesktop.appearance color-scheme` from the settings portal.
fn portal_color_scheme() -> Option<u32> {
    use gtk::glib::prelude::*;
    let bus = session_bus()?;
    let reply = bus
        .call_sync(
            Some("org.freedesktop.portal.Desktop"),
            PORTAL_PATH,
            PORTAL_SETTINGS,
            "Read",
            Some(&("org.freedesktop.appearance", "color-scheme").to_variant()),
            None,
            gtk::gio::DBusCallFlags::NONE,
            500,
            gtk::gio::Cancellable::NONE,
        )
        .ok()?;
    unwrap_u32(reply.child_value(0))
}

/// The `u32` inside `value`, which `Read` wraps in one or two variants
/// depending on the portal's version. `as_variant` on anything that isn't a
/// variant makes GLib print a critical, so the type is checked first.
fn unwrap_u32(mut value: gtk::glib::Variant) -> Option<u32> {
    while value.is_type(gtk::glib::VariantTy::VARIANT) {
        value = value.as_variant()?;
    }
    value.get::<u32>()
}

/// Re-check the desktop's preference when it changes (portal signal, GTK
/// theme name), once per run.
fn watch_system() {
    if STATE.with(|state| state.watching.replace(true)) {
        return;
    }
    let on_change = || {
        if STATE.with(|state| *state.theme.borrow() == THEME_SYSTEM) {
            refresh_theme();
        }
    };
    if let Some(settings) = gtk::Settings::default() {
        settings.connect_gtk_theme_name_notify(move |_| on_change());
    }
    if let Some(bus) = session_bus() {
        // The subscription lasts for the whole run.
        let _ = bus.signal_subscribe(
            None,
            Some(PORTAL_SETTINGS),
            Some("SettingChanged"),
            Some(PORTAL_PATH),
            Some("org.freedesktop.appearance"),
            gtk::gio::DBusSignalFlags::NONE,
            move |_, _, _, _, _, _| on_change(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn names(css: &str) -> BTreeSet<&str> {
        css.lines()
            .filter_map(|l| l.strip_prefix("@define-color "))
            .filter_map(|l| l.split_whitespace().next())
            .collect()
    }

    #[test]
    fn every_theme_defines_the_same_colours() {
        let reference = names(theme_def(THEME_DEFAULT).css);
        assert!(reference.len() > 30);
        for theme in THEMES {
            assert_eq!(names(theme.css), reference, "theme {}", theme.id);
        }
    }

    #[test]
    fn style_uses_only_defined_colours() {
        let defined = names(theme_def(THEME_DEFAULT).css);
        for word in STYLE.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '@')) {
            // "@nira_*" in comments leaves "@nira_".
            if let Some(name) = word.strip_prefix('@').filter(|n| !n.ends_with('_')) {
                assert!(defined.contains(name), "style.css uses undefined @{name}");
            }
        }
    }

    #[test]
    fn theme_ids_are_unique_and_named() {
        let ids: BTreeSet<_> = THEMES.iter().map(|t| t.id).collect();
        assert_eq!(ids.len(), THEMES.len());
        assert!(!ids.contains(THEME_SYSTEM));
        let ftl = include_str!("../../../i18n/en-US/nira.ftl");
        for id in ids {
            assert!(
                ftl.lines().any(|l| l.starts_with(&format!("theme-{id} ="))),
                "no theme-{id} string"
            );
        }
    }

    #[test]
    fn portal_values_unwrap_without_criticals() {
        use gtk::glib::prelude::*;
        use gtk::glib::Variant;
        let once = Variant::from_variant(&1u32.to_variant());
        let twice = Variant::from_variant(&once);
        assert_eq!(unwrap_u32(1u32.to_variant()), Some(1));
        assert_eq!(unwrap_u32(once), Some(1));
        assert_eq!(unwrap_u32(twice), Some(1));
        assert_eq!(unwrap_u32("dark".to_variant()), None);
    }

    #[test]
    fn unknown_theme_falls_back_to_the_default() {
        assert_eq!(theme_def("no-such-theme").id, THEME_DEFAULT);
        assert_eq!(theme_def("nord").id, "nord");
    }
}
