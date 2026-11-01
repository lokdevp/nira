//! The app stylesheet with its light or dark palette, the window icon and
//! the runtime viewer background.

use std::cell::{Cell, OnceCell};

use gtk::gdk;
use nira_core::settings::{Settings, Theme};

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

/// The main window's stylesheet; the palette goes in front of it.
const STYLE: &str = include_str!("style.css");
const PALETTE_DARK: &str = include_str!("palette-dark.css");
const PALETTE_LIGHT: &str = include_str!("palette-light.css");

/// Install the stylesheet for the whole display (once), with the dark
/// palette until `apply_theme` picks one.
pub fn load_css() {
    load_palette(true);
}

/// (Re)load the main stylesheet with the dark or light palette.
fn load_palette(dark: bool) {
    STATE.with(|state| {
        if state.dark.get() == Some(dark) {
            return;
        }
        state.dark.set(Some(dark));
        let provider = state.style.get_or_init(|| {
            let p = gtk::CssProvider::new();
            add_provider(&p, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
            p
        });
        let palette = if dark { PALETTE_DARK } else { PALETTE_LIGHT };
        provider.load_from_string(&format!("{palette}\n{STYLE}"));
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
    /// The palette loaded into it.
    dark: Cell<Option<bool>>,
    /// The theme setting, read again when the desktop's preference changes.
    theme: Cell<Theme>,
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

/// Light or dark for the main window and GTK's own dialogs. `System`
/// follows the desktop and keeps following it while the app runs.
pub fn apply_theme(theme: Theme) {
    STATE.with(|state| state.theme.set(theme));
    watch_system();
    refresh_theme();
}

fn refresh_theme() {
    let theme = STATE.with(|state| state.theme.get());
    let dark = match theme {
        Theme::Dark => true,
        Theme::Light => false,
        Theme::System => system_prefers_dark(),
    };
    if let Some(settings) = gtk::Settings::default() {
        if settings.is_gtk_application_prefer_dark_theme() != dark {
            settings.set_gtk_application_prefer_dark_theme(dark);
        }
    }
    load_palette(dark);
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
    // `Read` wraps the value in one or two variants depending on the
    // portal's version.
    let mut value = reply.child_value(0);
    while let Some(inner) = value.as_variant() {
        value = inner;
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
        if STATE.with(|state| state.theme.get()) == Theme::System {
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
