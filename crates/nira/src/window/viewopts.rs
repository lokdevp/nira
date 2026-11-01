//! The View panel's settings: enhancement, background and reading options.

use std::time::Duration;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};
use nira_core::settings::{Enhance, Settings};

use super::NiraWindow;
use crate::chrome::viewpanel::{background_preset, slider_factor, ViewPanel, BACKGROUNDS};
use crate::theme;

/// Slider changes wait this long before the pages are decoded again.
const ENHANCE_DEBOUNCE: Duration = Duration::from_millis(250);

/// A boolean action with `name` that calls `f` with its new state.
fn named_toggle(
    name: &str,
    initial: bool,
    f: fn(&NiraWindow, bool),
) -> gio::ActionEntry<NiraWindow> {
    gio::ActionEntry::builder(name)
        .state(initial.to_variant())
        .activate(move |win: &NiraWindow, act, _| {
            let on = !act.state().and_then(|s| s.get::<bool>()).unwrap_or(false);
            act.set_state(&on.to_variant());
            f(win, on);
        })
        .build()
}

fn radio(name: &str, initial: &str, f: fn(&NiraWindow, &str)) -> gio::ActionEntry<NiraWindow> {
    gio::ActionEntry::builder(name)
        .parameter_type(Some(glib::VariantTy::STRING))
        .state(initial.to_variant())
        .activate(move |win: &NiraWindow, act, param| {
            if let Some(target) = param.and_then(|p| p.get::<String>()) {
                act.set_state(&target.to_variant());
                f(win, &target);
            }
        })
        .build()
}

impl NiraWindow {
    pub(super) fn build_view_panel(&self) -> gtk::ScrolledWindow {
        let panel = ViewPanel::new();
        for scale in [&panel.brightness, &panel.contrast] {
            scale.connect_value_changed(glib::clone!(
                #[weak(rename_to = win)]
                self,
                move |_| win.on_enhance_slider()
            ));
        }
        let root = panel.root.clone();
        let _ = self.imp().viewpanel.set(panel);
        root
    }

    pub(super) fn add_view_actions(&self) {
        let s = self.imp_settings().clone();
        self.add_action_entries([
            named_toggle("view-panel", s.appearance.show_view_panel, |w, on| {
                w.show_view_panel(on);
                w.update_settings(|s| s.appearance.show_view_panel = on);
            }),
            named_toggle("auto-enhance", s.enhance.auto_enhance, |w, on| {
                w.update_settings(|s| s.enhance.auto_enhance = on);
                w.apply_enhance();
            }),
            radio(
                "background",
                background_preset(&s).unwrap_or("black"),
                |w, name| w.set_background(name),
            ),
            named_toggle("cover-alone", s.view.cover_alone, |w, on| {
                w.update_settings(|s| s.view.cover_alone = on);
                w.with_reader(|r| {
                    r.view.cover_alone = on;
                    r.refresh();
                });
                w.show_spread(None);
            }),
            named_toggle(
                "auto-hide-controls",
                s.appearance.auto_hide_controls,
                |w, on| {
                    w.update_settings(|s| s.appearance.auto_hide_controls = on);
                    w.wake_controls();
                },
            ),
            radio(
                "slideshow-delay",
                &(s.slideshow.delay_ms / 1000).to_string(),
                |w, secs| {
                    let secs: u64 = secs.parse().unwrap_or(3);
                    w.update_settings(|s| s.slideshow.delay_ms = secs * 1000);
                    w.sync_view_panel();
                    if w.imp().slideshow.borrow().is_some() {
                        w.set_slideshow(true); // restart with the new delay
                    }
                },
            ),
            gio::ActionEntry::builder("reset-view")
                .activate(|win: &NiraWindow, _, _| win.reset_view())
                .build(),
        ]);
        self.show_view_panel(s.appearance.show_view_panel);
        self.sync_view_panel();
    }

    fn show_view_panel(&self, on: bool) {
        if let Some(panel) = self.imp().viewpanel.get() {
            panel.root.set_visible(on);
        }
    }

    fn sync_view_panel(&self) {
        let settings = self.imp_settings().clone();
        if let Some(panel) = self.imp().viewpanel.get() {
            let syncing = self.imp().syncing_panel.replace(true);
            panel.update(&settings);
            self.imp().syncing_panel.set(syncing);
        }
    }

    /// A slider moved: show the value now, decode once it rests.
    fn on_enhance_slider(&self) {
        let imp = self.imp();
        let Some(panel) = imp.viewpanel.get() else {
            return;
        };
        if imp.syncing_panel.get() {
            return;
        }
        let brightness = slider_factor(panel.brightness.value());
        let contrast = slider_factor(panel.contrast.value());
        let same = {
            let e = &self.imp_settings().enhance;
            (e.brightness, e.contrast) == (brightness, contrast)
        };
        if same {
            return;
        }
        self.update_settings(|s| {
            s.enhance.brightness = brightness;
            s.enhance.contrast = contrast;
        });
        self.sync_view_panel();
        if let Some(id) = imp.enhance_source.take() {
            id.remove();
        }
        let id = glib::timeout_add_local_once(
            ENHANCE_DEBOUNCE,
            glib::clone!(
                #[weak(rename_to = win)]
                self,
                move || {
                    win.imp().enhance_source.take();
                    win.apply_enhance();
                }
            ),
        );
        imp.enhance_source.replace(Some(id));
    }

    /// Decode the pages again with the settings' enhancement.
    fn apply_enhance(&self) {
        let enhance = self.imp_settings().enhance.clone();
        self.with_reader(|r| {
            if r.enhance != enhance {
                r.enhance = enhance;
                r.invalidate();
            }
        });
        self.show_spread(None);
    }

    fn set_background(&self, name: &str) {
        let colour = BACKGROUNDS
            .iter()
            .find(|(n, _, _)| *n == name)
            .and_then(|(_, _, c)| *c);
        self.update_settings(|s| match colour {
            Some(c) => {
                s.appearance.background = c;
                s.appearance.match_page_color = false;
            }
            None => s.appearance.match_page_color = true,
        });
        self.update_background();
    }

    /// Colour the reading area: the chosen background, or the shown page's
    /// border with Auto.
    pub(super) fn update_background(&self) {
        let settings = self.imp_settings().clone();
        let page = settings
            .appearance
            .match_page_color
            .then(|| {
                self.with_reader(|r| {
                    let first = *r.spread().first()?;
                    let (w, h, px) = r.page_pixels(first)?;
                    nira_core::image_loader::edge_colour(w, h, &px)
                })
            })
            .flatten();
        theme::apply_background(&settings, page);
    }

    /// "Reset to defaults": only the settings the panel shows.
    fn reset_view(&self) {
        let d = Settings::default();
        self.update_settings(|s| {
            s.enhance = Enhance::default();
            s.appearance.background = d.appearance.background;
            s.appearance.match_page_color = d.appearance.match_page_color;
            s.appearance.auto_hide_controls = d.appearance.auto_hide_controls;
            s.view.cover_alone = d.view.cover_alone;
            s.slideshow.delay_ms = d.slideshow.delay_ms;
        });
        let s = self.imp_settings().clone();
        self.set_action_state("auto-enhance", s.enhance.auto_enhance.to_variant());
        self.set_action_state(
            "background",
            background_preset(&s).unwrap_or("black").to_variant(),
        );
        self.set_action_state("cover-alone", s.view.cover_alone.to_variant());
        self.set_action_state(
            "auto-hide-controls",
            s.appearance.auto_hide_controls.to_variant(),
        );
        self.set_action_state(
            "slideshow-delay",
            (s.slideshow.delay_ms / 1000).to_string().to_variant(),
        );
        self.sync_view_panel();
        self.update_background();
        self.with_reader(|r| {
            r.view.cover_alone = s.view.cover_alone;
            r.refresh();
        });
        self.apply_enhance();
    }
}
