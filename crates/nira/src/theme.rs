//! The app stylesheet and its colour themes, the window icon, the
//! runtime viewer background and the interface size.

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

/// The interface size: a factor on every size the app sets itself (its
/// stylesheet's px and the sizes in code). GTK on Windows scales only by
/// whole numbers, so at 150 % it draws at 100 % and this makes up the rest.
/// Set once at startup, before any widget is built.
pub fn ui_scale() -> f64 {
    UI_SCALE.with(Cell::get)
}

/// `n` logical pixels at the interface size.
pub fn px(n: i32) -> i32 {
    (n as f64 * ui_scale()).round() as i32
}

/// `n` logical pixels at the interface size, for sizes kept as floats.
pub fn pxf(n: f64) -> f64 {
    n * ui_scale()
}

/// Back from a size on screen to logical pixels (for sizes that are saved).
pub fn unpx(n: i32) -> i32 {
    (n as f64 / ui_scale()).round() as i32
}

/// Pick the interface size from the setting (percent, or `None` to follow
/// the display), and size GTK's own text (dialogs, menus, tooltips) to
/// match: its font sizes are in points, which follow the DPI, while the
/// app's px sizes don't.
pub fn set_ui_scale(percent: Option<u32>) {
    let f = percent.map_or_else(display_scale, |p| p as f64 / 100.0);
    UI_SCALE.with(|c| c.set(f));
    log::debug!("interface size {:.0} %", f * 100.0);
    if f != 1.0 {
        if let Some(settings) = gtk::Settings::default() {
            // -1 is GTK's default of 96 dpi; the value is in 1/1024 dpi.
            let base = match settings.gtk_xft_dpi() {
                d if d > 0 => d as f64,
                _ => 96.0 * 1024.0,
            };
            settings.set_gtk_xft_dpi((base * f).round() as i32);
        }
    }
}

/// The scale the display asks for beyond what GTK applies itself. On
/// Windows that is the display's scale over GTK's whole-number one (1.5 at
/// 150 %, 1.0 at 200 %, which GTK draws at 2); elsewhere GTK scales by
/// fractions itself.
fn display_scale() -> f64 {
    #[cfg(windows)]
    {
        use gtk::prelude::*;
        // SAFETY: no arguments; returns the system DPI (96 at 100 %).
        let dpi = unsafe { windows_sys::Win32::UI::HiDpi::GetDpiForSystem() };
        let gdk_scale = gdk::Display::default()
            .and_then(|d| d.monitors().item(0))
            .and_then(|m| m.downcast::<gdk::Monitor>().ok())
            .map_or(1, |m| m.scale_factor().max(1));
        if dpi > 0 {
            let f = dpi as f64 / 96.0 / gdk_scale as f64;
            log::debug!("display: {dpi} dpi, GTK scale {gdk_scale}");
            return f.clamp(1.0, 2.0);
        }
    }
    1.0
}

/// `css` with every length in px multiplied by `f`, rounded to a hundredth
/// (`13px` is `19.5px` at 1.5). Nothing else changes.
fn scale_px(css: &str, f: f64) -> String {
    if f == 1.0 {
        return css.to_string();
    }
    let bytes = css.as_bytes();
    let mut out = String::with_capacity(css.len() + css.len() / 8);
    let mut i = 0;
    let mut copied = 0;
    while i < bytes.len() {
        let c = bytes[i];
        // A number starts at a digit (or a '.' before one) that doesn't
        // continue a name such as `h1` or a colour such as `#1e1e2e`. A '-'
        // before it is a sign unless it follows a name (`col-2`).
        let word = |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'#' | b'_' | b'.');
        let in_name = match i.checked_sub(1).map(|j| bytes[j]) {
            Some(b'-') => i >= 2 && word(bytes[i - 2]),
            Some(b) => word(b),
            None => false,
        };
        let number = c.is_ascii_digit()
            || (c == b'.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit));
        if !number || in_name {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
            i += 1;
        }
        let unit_px = css[i..].starts_with("px")
            && !bytes.get(i + 2).is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'-');
        if !unit_px {
            continue;
        }
        let Ok(n) = css[start..i].parse::<f64>() else {
            continue;
        };
        out.push_str(&css[copied..start]);
        let v = (n * f * 100.0).round() / 100.0;
        out.push_str(&format!("{v}"));
        copied = i;
    }
    out.push_str(&css[copied..]);
    out
}

/// Icon sizes at the interface size (GTK's own sizes, which `scale_px`
/// can't reach): the size icons inherit from their window, and GTK's two
/// icon size classes.
fn icon_css(f: f64) -> String {
    if f == 1.0 {
        return String::new();
    }
    let (normal, large) = ((16.0 * f).round(), (32.0 * f).round());
    format!(
        "\nwindow, popover {{ -gtk-icon-size: {normal}px; }}\n\
         .normal-icons {{ -gtk-icon-size: {normal}px; }}\n\
         .large-icons {{ -gtk-icon-size: {large}px; }}\n"
    )
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
        let f = ui_scale();
        let css = scale_px(&format!("{}\n{STYLE}", theme.css), f) + &icon_css(f);
        provider.load_from_string(&css);
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
    static UI_SCALE: Cell<f64> = const { Cell::new(1.0) };
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
    /// The portal's `color-scheme` (0: no preference, 1: dark, 2: light),
    /// once it has answered or signalled; `None` before that or without a
    /// portal.
    portal_scheme: Cell<Option<u32>>,
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
/// Cheap when `id` is the theme already applied, so callers needn't check.
pub fn apply_theme(id: &str) {
    let changed = STATE.with(|state| state.theme.replace(id.to_string()) != id);
    let first = watch_system();
    if changed || first {
        refresh_theme();
    }
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
/// scheme once it is known, otherwise a "dark" GTK theme name. Never
/// blocks: the portal is read asynchronously by `watch_system`.
fn system_prefers_dark() -> bool {
    if let Some(scheme) = STATE.with(|state| state.portal_scheme.get()) {
        return scheme == 1;
    }
    gtk::Settings::default()
        .and_then(|s| s.gtk_theme_name())
        .is_some_and(|name| name.to_lowercase().contains("dark"))
}

const PORTAL_NAME: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const PORTAL_SETTINGS: &str = "org.freedesktop.portal.Settings";
const APPEARANCE: &str = "org.freedesktop.appearance";
const COLOR_SCHEME_KEY: &str = "color-scheme";
/// Generous, as nothing waits on the reply: a portal started on demand
/// can take seconds to answer the first call.
const PORTAL_TIMEOUT_MS: i32 = 10_000;

/// Store the portal's colour scheme and recolour if the theme follows it.
fn set_portal_scheme(scheme: u32) {
    let (changed, system) = STATE.with(|state| {
        let changed = state.portal_scheme.replace(Some(scheme)) != Some(scheme);
        (changed, *state.theme.borrow() == THEME_SYSTEM)
    });
    log::debug!("portal color-scheme: {scheme}");
    if changed && system {
        refresh_theme();
    }
}

/// Subscribe to the portal's `SettingChanged`, then read `color-scheme`
/// once, all without blocking the main loop.
#[cfg(unix)]
async fn watch_portal() {
    use gtk::gio;
    use gtk::glib::prelude::*;
    let bus = match gio::bus_get_future(gio::BusType::Session).await {
        Ok(bus) => bus,
        Err(e) => {
            log::debug!("no session bus: {e}");
            return;
        }
    };
    // Subscribed first, so a change during the read isn't missed. The
    // subscription lasts for the whole run.
    let _ = bus.signal_subscribe(
        None,
        Some(PORTAL_SETTINGS),
        Some("SettingChanged"),
        Some(PORTAL_PATH),
        Some(APPEARANCE),
        gio::DBusSignalFlags::NONE,
        |_, _, _, _, _, params| {
            // (namespace, key, value): the new value comes with the signal.
            if params.n_children() == 3 && params.child_value(1).str() == Some(COLOR_SCHEME_KEY) {
                if let Some(scheme) = unwrap_u32(params.child_value(2)) {
                    set_portal_scheme(scheme);
                }
            }
        },
    );
    let args = (APPEARANCE, COLOR_SCHEME_KEY).to_variant();
    let call = |method: &str| {
        bus.call_future(
            Some(PORTAL_NAME),
            PORTAL_PATH,
            PORTAL_SETTINGS,
            method,
            Some(&args),
            None,
            gio::DBusCallFlags::NONE,
            PORTAL_TIMEOUT_MS,
        )
    };
    // `ReadOne` is portal version 2; `Read` (deprecated) for older ones.
    let reply = match call("ReadOne").await {
        Ok(reply) => Ok(reply),
        Err(e) if e.matches(gio::DBusError::UnknownMethod) => call("Read").await,
        Err(e) => Err(e),
    };
    match reply {
        Ok(reply) => {
            // A signal that came first is newer than this reply.
            let known = STATE.with(|state| state.portal_scheme.get().is_some());
            if let (false, Some(scheme)) = (known, unwrap_u32(reply.child_value(0))) {
                set_portal_scheme(scheme);
            }
        }
        Err(e) => log::debug!("cannot read the portal's color-scheme: {e}"),
    }
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

/// Start following the desktop's preference (portal, GTK theme name), once
/// per run. Returns whether this call started it.
fn watch_system() -> bool {
    if STATE.with(|state| state.watching.replace(true)) {
        return false;
    }
    if let Some(settings) = gtk::Settings::default() {
        settings.connect_gtk_theme_name_notify(|_| {
            if STATE.with(|state| *state.theme.borrow() == THEME_SYSTEM) {
                refresh_theme();
            }
        });
    }
    #[cfg(unix)]
    gtk::glib::MainContext::default().spawn_local(watch_portal());
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn scale_px_scales_only_px_lengths() {
        assert_eq!(scale_px("a { font-size: 13px; }", 1.5), "a { font-size: 19.5px; }");
        assert_eq!(
            scale_px("a { margin: 0 -4px 0.5px .5px; border: 1px solid; }", 2.0),
            "a { margin: 0 -8px 1px 1px; border: 2px solid; }"
        );
        // Other units, colours, names and plain numbers stay.
        let other = "h1.col-2px { width: 50%; color: #1e1e2e; opacity: 0.5; font-weight: 600; font-size: 2em; transition: 200ms; }";
        assert_eq!(scale_px(other, 1.5), other);
        assert_eq!(scale_px("a { padding: 7px; }", 1.25), "a { padding: 8.75px; }");
        assert_eq!(scale_px("a { padding: 1px; }", 1.0 / 3.0), "a { padding: 0.33px; }");
    }

    #[test]
    fn whole_stylesheet_scales_to_valid_css() {
        let css = scale_px(&format!("{}\n{STYLE}", THEMES[0].css), 1.5) + &icon_css(1.5);
        assert_eq!(css.matches("px").count(), STYLE.matches("px").count() + 3);
        assert!(css.contains("font-size: 19.5px"), "13px body text");
        assert!(css.contains("-gtk-icon-size: 24px"));
    }

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
