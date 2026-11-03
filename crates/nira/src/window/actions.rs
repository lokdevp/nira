//! The window's actions (`win.<name>`, named after `keymap::Action`) and
//! their keyboard accelerators.

use gtk::prelude::*;
use gtk::{gio, glib};
use nira_core::keymap::{Action, Keymap};
use nira_core::zoom::FitMode;

use super::NiraWindow;

/// An action that runs `f`.
fn simple(action: Action, f: fn(&NiraWindow)) -> gio::ActionEntry<NiraWindow> {
    gio::ActionEntry::builder(action.name())
        .activate(move |win: &NiraWindow, _, _| f(win))
        .build()
}

/// A radio action: its string state is one of several targets (segmented
/// controls and menu items bind to `win.<name>` with a target). `f` gets
/// the chosen target.
fn radio(name: &str, initial: &str, f: fn(&NiraWindow, &str)) -> gio::ActionEntry<NiraWindow> {
    gio::ActionEntry::builder(name)
        .parameter_type(Some(glib::VariantTy::STRING))
        .state(initial.to_variant())
        .activate(move |win: &NiraWindow, _, param| {
            if let Some(target) = param.and_then(|p| p.get::<String>()) {
                f(win, &target);
            }
        })
        .build()
}

/// A checkable action (shown as a toggle in menus) that calls `f` with its
/// new state.
fn toggle(action: Action, initial: bool, f: fn(&NiraWindow, bool)) -> gio::ActionEntry<NiraWindow> {
    gio::ActionEntry::builder(action.name())
        .state(initial.to_variant())
        .activate(move |win: &NiraWindow, act, _| {
            let on = !act.state().and_then(|s| s.get::<bool>()).unwrap_or(false);
            act.set_state(&on.to_variant());
            f(win, on);
        })
        .build()
}

impl NiraWindow {
    pub(super) fn add_actions(&self) {
        let (double, manga, fit, invert, thumbs) = {
            let s = self.imp_settings();
            (
                s.view.double_page,
                s.view.manga,
                s.view.fit,
                s.scroll.invert_smart_scroll,
                s.appearance.show_thumbnails,
            )
        };
        self.add_action_entries([
            gio::ActionEntry::builder("open")
                .activate(|win: &NiraWindow, _, _| win.choose_file())
                .build(),
            gio::ActionEntry::builder("preferences")
                .activate(|win: &NiraWindow, _, _| win.show_preferences())
                .build(),
            gio::ActionEntry::builder("about")
                .activate(|win: &NiraWindow, _, _| win.show_about())
                .build(),
            // Navigation.
            simple(Action::NextPage, |w| w.flip(true, 1)),
            simple(Action::PrevPage, |w| w.flip(false, 1)),
            simple(Action::ForwardTen, |w| w.flip(true, 10)),
            simple(Action::BackTen, |w| w.flip(false, 10)),
            simple(Action::FirstPage, |w| w.go_to_first()),
            gio::ActionEntry::builder("page-left")
                .activate(|win: &NiraWindow, _, _| win.page_left())
                .build(),
            gio::ActionEntry::builder("page-right")
                .activate(|win: &NiraWindow, _, _| win.page_right())
                .build(),
            toggle(Action::Slideshow, false, |w, on| w.set_slideshow(on)),
            toggle(Action::Thumbnails, thumbs, |w, on| w.show_sidebar(on)),
            toggle(Action::Lens, false, |w, on| w.set_lens(on)),
            simple(Action::GoToPage, |w| w.ask_page()),
            simple(Action::RotateCw, |w| w.rotate(90)),
            simple(Action::RotateCcw, |w| w.rotate(270)),
            simple(Action::Rotate180, |w| w.rotate(180)),
            simple(Action::FlipH, |w| w.transform(|t| t.flip_h = !t.flip_h)),
            simple(Action::FlipV, |w| w.transform(|t| t.flip_v = !t.flip_v)),
            gio::ActionEntry::builder("bookmark")
                .state(false.to_variant())
                .activate(|win: &NiraWindow, _, _| win.toggle_bookmark())
                .build(),
            simple(Action::AddBookmark, |w| w.toggle_bookmark()),
            simple(Action::LastPage, |w| w.go_to_last()),
            // Scrolling.
            simple(Action::ScrollUp, |w| w.key_scroll(0.0, -1.0)),
            simple(Action::ScrollDown, |w| w.key_scroll(0.0, 1.0)),
            simple(Action::ScrollLeft, |w| w.key_scroll(-1.0, 0.0)),
            simple(Action::ScrollRight, |w| w.key_scroll(1.0, 0.0)),
            simple(Action::SmartScrollUp, |w| w.smart_scroll(true)),
            simple(Action::SmartScrollDown, |w| w.smart_scroll(false)),
            toggle(Action::InvertSmartScroll, invert, |w, on| {
                w.update_settings(|s| s.scroll.invert_smart_scroll = on)
            }),
            // Zoom.
            simple(Action::ZoomIn, |w| w.zoom(|z| z.zoom_in())),
            simple(Action::ZoomOut, |w| w.zoom(|z| z.zoom_out())),
            simple(Action::ZoomReset, |w| w.zoom_reset()),
            simple(Action::FitBest, |w| w.set_fit(FitMode::Best)),
            simple(Action::FitWidth, |w| w.set_fit(FitMode::Width)),
            simple(Action::FitHeight, |w| w.set_fit(FitMode::Height)),
            simple(Action::FitSize, |w| w.set_fit(FitMode::Size)),
            simple(Action::FitManual, |w| w.set_fit(FitMode::Manual)),
            radio("fit", fit.name(), |w, name| {
                if let Some(mode) = FitMode::from_name(name) {
                    w.set_fit(mode);
                }
            }),
            // Layout. The keys toggle; the toolbar's segmented controls
            // pick a value of the radio actions.
            simple(Action::DoublePage, |w| w.set_double_page(!w.double_page())),
            simple(Action::MangaMode, |w| w.set_manga(!w.manga())),
            radio(
                "page-layout",
                if double { "spread" } else { "single" },
                |w, target| w.set_double_page(target == "spread"),
            ),
            radio(
                "direction",
                if manga { "rtl" } else { "ltr" },
                |w, target| w.set_manga(target == "rtl"),
            ),
            // Window.
            simple(Action::Fullscreen, |w| w.toggle_focus()),
            simple(Action::ExitFullscreen, |w| w.leave_fullscreen()),
            simple(Action::Minimize, |w| w.minimize()),
        ]);
        // The sidebar is built before the settings are loaded.
        self.set_sidebar_visible(thumbs);
    }

    /// Bind the keymap's accelerators to the window's actions. Actions the
    /// window does not have yet are skipped, and so are accelerators GTK
    /// cannot parse.
    pub(super) fn install_accels(&self, app: &gtk::Application, keymap: &Keymap) {
        app.set_accels_for_action("win.open", &["<Control>o"]);
        app.set_accels_for_action("win.preferences", &["<Control>comma"]);
        app.set_accels_for_action("window.close", &["<Control>w", "q"]);
        for &action in Action::ALL {
            if self.lookup_action(action.name()).is_none() {
                continue;
            }
            let accels: Vec<&str> = keymap
                .accels(action)
                .iter()
                .map(String::as_str)
                .filter(|a| {
                    let ok = gtk::accelerator_parse(*a).is_some();
                    if !ok {
                        log::warn!("{}: cannot parse key {a:?}", action.name());
                    }
                    ok
                })
                .collect();
            app.set_accels_for_action(&format!("win.{}", action.name()), &accels);
        }
    }

    /// Set a stateful action's state without running it.
    pub(super) fn set_action_state(&self, name: &str, state: glib::Variant) {
        if let Some(act) = self.lookup_action(name).and_downcast::<gio::SimpleAction>() {
            act.set_state(&state);
        }
    }
}
