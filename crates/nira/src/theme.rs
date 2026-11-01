//! The app stylesheet and the runtime viewer background.

use std::cell::OnceCell;

use gtk::gdk;
use nira_core::settings::{Settings, Theme};

/// Install `style.css` for the whole display (once).
pub fn load_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(include_str!("style.css"));
    add_provider(&provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
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

/// Light or dark for dialogs (the main window's chrome is always dark).
/// `System` leaves GTK's own choice.
pub fn apply_theme(theme: Theme) {
    let Some(settings) = gtk::Settings::default() else {
        return;
    };
    match theme {
        Theme::System => settings.reset_property("gtk-application-prefer-dark-theme"),
        Theme::Dark => settings.set_gtk_application_prefer_dark_theme(true),
        Theme::Light => settings.set_gtk_application_prefer_dark_theme(false),
    }
}
