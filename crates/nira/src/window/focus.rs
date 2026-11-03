//! Focus mode: fullscreen with the chrome hidden. A hint fades after a
//! moment, the toolbar floats in at the top edge, and a page label and
//! progress line show the position.

use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use nira_core::keymap::Action;
use nira_core::ordering::focus_label;

use super::NiraWindow;
use crate::chrome::focus::FocusOverlay;

/// How long the hint stays.
const HINT_FOR: Duration = Duration::from_secs(2);
/// The pointer this close to the top edge brings the toolbar in.
const REVEAL_EDGE: f64 = 6.0;

impl NiraWindow {
    /// Put the focus overlays over `column` (the window's content).
    pub(super) fn build_focus(&self, column: &gtk::Box) -> gtk::Overlay {
        let imp = self.imp();
        let root = gtk::Overlay::new();
        root.set_child(Some(column));
        let focus = FocusOverlay::new();
        root.add_overlay(&focus.progress);
        root.add_overlay(&focus.label);
        root.add_overlay(&focus.hint);
        let motion = gtk::EventControllerMotion::new();
        motion.connect_motion(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_, _, y| win.focus_pointer(y)
        ));
        root.add_controller(motion);
        self.connect_fullscreened_notify(|win| {
            // Entering or leaving fullscreen from outside (the window
            // manager, -f) switches focus mode too.
            if win.is_fullscreen() != win.imp().focus.get() {
                win.apply_focus(win.is_fullscreen());
            }
        });
        let _ = imp.focus_overlay.set(focus);
        imp.root.replace(Some(root.clone()));
        root
    }

    /// Switch focus mode. The chrome changes at once, without waiting for
    /// the window manager to confirm fullscreen.
    pub(super) fn toggle_focus(&self) {
        let on = !self.imp().focus.get();
        self.apply_focus(on);
        if on {
            self.fullscreen();
        } else {
            self.unfullscreen();
        }
    }

    /// Escape: leave focus mode, or quit if the settings say so.
    pub(super) fn leave_fullscreen(&self) {
        if self.imp().focus.get() || self.is_fullscreen() {
            self.apply_focus(false);
            self.unfullscreen();
        } else if self.imp_settings().files.escape_quits {
            self.close();
        }
    }

    fn apply_focus(&self, on: bool) {
        let imp = self.imp();
        imp.focus.set(on);
        // Focus mode starts with both panels hidden; leaving it restores
        // the normal-mode choice. The toggles follow what is shown.
        let (sidebar, panel) = {
            let s = self.imp_settings();
            (
                !on && s.appearance.show_thumbnails,
                !on && s.appearance.show_view_panel,
            )
        };
        self.set_sidebar_visible(sidebar);
        self.show_view_panel(panel);
        self.set_action_state("thumbnails", sidebar.to_variant());
        self.set_action_state("view-panel", panel.to_variant());
        self.apply_progress_visibility();
        self.float_toolbar(on);
        if let Some(focus) = imp.focus_overlay.get() {
            focus.set_visible(on);
            focus.hint.remove_css_class("faded");
        }
        if let Some(id) = imp.hint_source.take() {
            id.remove();
        }
        if on {
            let id = glib::timeout_add_local_once(
                HINT_FOR,
                glib::clone!(
                    #[weak(rename_to = win)]
                    self,
                    move || {
                        win.imp().hint_source.take();
                        if let Some(focus) = win.imp().focus_overlay.get() {
                            focus.hint.add_css_class("faded");
                        }
                    }
                ),
            );
            imp.hint_source.replace(Some(id));
        }
        self.update_chrome();
    }

    /// In focus mode the toolbar leaves the window's column and floats over
    /// the page, hidden until the pointer reaches the top edge.
    fn float_toolbar(&self, on: bool) {
        let imp = self.imp();
        let (Some(toolbar), Some(root)) = (imp.toolbar.get(), imp.root.borrow().clone()) else {
            return;
        };
        let Some(column) = root.child().and_downcast::<gtk::Box>() else {
            return;
        };
        let widget = &toolbar.root;
        let floating = widget.parent().as_ref() == Some(root.upcast_ref());
        if on && !floating {
            column.remove(widget);
            widget.set_valign(gtk::Align::Start);
            widget.add_css_class("floating");
            root.add_overlay(widget);
            widget.set_visible(false);
        } else if !on && floating {
            root.remove_overlay(widget);
            widget.set_valign(gtk::Align::Fill);
            widget.remove_css_class("floating");
            column.prepend(widget);
            widget.set_visible(true);
        }
        if let Some(statusbar) = imp.statusbar.get() {
            statusbar.root.set_visible(!on);
        }
    }

    /// Show the floating toolbar at the top edge; hide it once the pointer
    /// is well below it.
    fn focus_pointer(&self, y: f64) {
        let imp = self.imp();
        if !imp.focus.get() {
            return;
        }
        let Some(toolbar) = imp.toolbar.get() else {
            return;
        };
        let shown = toolbar.root.is_visible();
        if !shown && y <= REVEAL_EDGE {
            toolbar.root.set_visible(true);
        } else if shown && y > toolbar.root.height() as f64 + 24.0 {
            toolbar.root.set_visible(false);
        }
    }

    /// Show or hide the page bar and click zones (outside focus mode) and
    /// the progress line (in focus mode), after the settings.
    pub(super) fn apply_progress_visibility(&self) {
        let imp = self.imp();
        let on = imp.focus.get();
        let appearance = self.imp_settings().appearance.clone();
        if let Some(bar) = imp.pagebar.get() {
            bar.root.set_visible(!on && appearance.show_page_bar);
            for zone in &bar.zones {
                zone.set_visible(!on);
            }
        }
        if let Some(focus) = imp.focus_overlay.get() {
            focus
                .progress
                .set_visible(on && appearance.show_focus_progress);
        }
    }

    /// The focus label and progress line for the current spread.
    pub(super) fn update_focus(&self) {
        let imp = self.imp();
        if !imp.focus.get() {
            return;
        }
        let (spread, total, manga) =
            self.with_reader(|r| (r.spread(), r.page_count(), r.view.manga));
        if total == 0 {
            return;
        }
        let last = spread.iter().max().map_or(0, |&p| p + 1);
        if let Some(focus) = imp.focus_overlay.get() {
            focus.update(
                &focus_label(&spread, total, manga),
                last as f64 / total as f64,
                manga,
            );
        }
    }

    /// The Esc keycap in the hint, from the keymap.
    pub(super) fn set_focus_keys(&self, keymap: &nira_core::keymap::Keymap) {
        let key = keymap
            .accels(Action::ExitFullscreen)
            .first()
            .and_then(|a| crate::chrome::accel_label(a));
        if let (Some(focus), Some(key)) = (self.imp().focus_overlay.get(), key) {
            focus.set_exit_key(&key);
        }
        let lens = keymap
            .accels(Action::Lens)
            .first()
            .and_then(|a| crate::chrome::accel_label(a));
        if let Some(toolbar) = self.imp().toolbar.get() {
            toolbar.set_lens_key(lens.map(|k| k.to_uppercase()).as_deref());
        }
    }
}
