//! The preferences dialog: everything in `settings.toml` that the View
//! panel doesn't show, applied as it changes. The Keys tab is the
//! keybinding editor (`keys.rs`).

use gtk::glib;
use gtk::prelude::*;
use nira_core::i18n::t;
use nira_core::settings::{Settings, THEME_SYSTEM};
use nira_core::zoom::{FitMode, FitSizeAxis};

use super::NiraWindow;
use crate::theme;

/// A page of the dialog: rows of label and control.
struct Page {
    root: gtk::Box,
}

impl Page {
    fn new() -> Page {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 14);
        root.set_margin_top(18);
        root.set_margin_bottom(18);
        root.set_margin_start(18);
        root.set_margin_end(18);
        Page { root }
    }

    fn heading(&self, text: &str) {
        let l = gtk::Label::new(Some(text));
        l.add_css_class("heading");
        l.set_xalign(0.0);
        l.set_margin_top(6);
        self.root.append(&l);
    }

    fn row(&self, title: &str, hint: Option<&str>, control: &impl IsA<gtk::Widget>) {
        let r = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text.set_hexpand(true);
        let l = gtk::Label::new(Some(title));
        l.set_xalign(0.0);
        l.set_wrap(true);
        text.append(&l);
        if let Some(hint) = hint {
            let h = gtk::Label::new(Some(hint));
            h.add_css_class("dim-label");
            h.set_xalign(0.0);
            h.set_wrap(true);
            text.append(&h);
        }
        r.append(&text);
        control.set_valign(gtk::Align::Center);
        r.append(control);
        self.root.append(&r);
    }
}

impl NiraWindow {
    /// Change a setting from the dialog and apply it.
    fn pref(&self, f: impl FnOnce(&mut Settings)) {
        self.update_settings(f);
        self.apply_settings();
    }

    fn pref_switch(
        &self,
        page: &Page,
        title: &str,
        hint: Option<&str>,
        get: fn(&Settings) -> bool,
        set: fn(&mut Settings, bool),
    ) {
        let s = gtk::Switch::new();
        s.set_active(get(&self.imp_settings()));
        s.connect_active_notify(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |s| {
                let on = s.is_active();
                win.pref(|st| set(st, on));
            }
        ));
        page.row(title, hint, &s);
    }

    #[allow(clippy::too_many_arguments)]
    fn pref_spin(
        &self,
        page: &Page,
        title: &str,
        (min, max, step): (f64, f64, f64),
        digits: u32,
        get: fn(&Settings) -> f64,
        set: fn(&mut Settings, f64),
    ) {
        let spin = gtk::SpinButton::with_range(min, max, step);
        spin.set_digits(digits);
        spin.set_value(get(&self.imp_settings()));
        spin.connect_value_changed(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |s| {
                let v = s.value();
                win.pref(|st| set(st, v));
            }
        ));
        page.row(title, None, &spin);
    }

    fn pref_choice(
        &self,
        page: &Page,
        title: &str,
        options: &[String],
        get: fn(&Settings) -> u32,
        set: fn(&mut Settings, u32),
    ) {
        let strings: Vec<&str> = options.iter().map(String::as_str).collect();
        let dd = gtk::DropDown::from_strings(&strings);
        dd.set_selected(get(&self.imp_settings()));
        dd.connect_selected_notify(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |dd| {
                let i = dd.selected();
                win.pref(|st| set(st, i));
            }
        ));
        page.row(title, None, &dd);
    }

    /// Push the settings the dialog changes into the reader and the window.
    fn apply_settings(&self) {
        let s = self.imp_settings().clone();
        let fit_changed = self.with_reader(|r| {
            let fit_changed = r.zoom.fit_mode != s.view.fit;
            r.zoom.scale_up = s.view.scale_up;
            r.zoom.fit_size = s.view.fit_size;
            let decode = r.view.exif_rotation != s.view.exif_rotation;
            let respread = (r.view.wide_alone, r.view.double_step, r.view.cover_alone)
                != (s.view.wide_alone, s.view.double_step, s.view.cover_alone);
            r.view.wide_alone = s.view.wide_alone;
            r.view.double_step = s.view.double_step;
            r.view.cover_alone = s.view.cover_alone;
            r.view.exif_rotation = s.view.exif_rotation;
            r.set_cache_pages(s.files.cached_pages);
            if decode {
                r.invalidate();
            } else if respread {
                r.refresh();
            }
            fit_changed
        });
        theme::apply_theme(&s.appearance.theme);
        self.set_action_state("cover-alone", s.view.cover_alone.to_variant());
        if fit_changed {
            self.set_fit(s.view.fit);
        } else {
            self.show_spread(None);
        }
    }

    pub(super) fn show_preferences(&self) {
        let dialog = gtk::Window::builder()
            .title(t("prefs-title"))
            .transient_for(self)
            .modal(true)
            .default_width(560)
            .default_height(640)
            .build();
        let notebook = gtk::Notebook::new();
        let tab = |page: &Page, title: &str| {
            let scroll = gtk::ScrolledWindow::new();
            scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
            scroll.set_child(Some(&page.root));
            notebook.append_page(&scroll, Some(&gtk::Label::new(Some(title))));
        };

        // Reading.
        let p = Page::new();
        p.heading(&t("prefs-appearance"));
        // "Follow the system", then every theme.
        let mut themes = vec![t("prefs-theme-system")];
        themes.extend(theme::THEMES.iter().map(|th| t(&format!("theme-{}", th.id))));
        self.pref_choice(
            &p,
            &t("prefs-theme"),
            &themes,
            |s| {
                theme::THEMES
                    .iter()
                    .position(|th| th.id == s.appearance.theme)
                    .map_or(0, |i| i as u32 + 1)
            },
            |s, i| {
                s.appearance.theme = match i.checked_sub(1) {
                    Some(i) => theme::THEMES.get(i as usize).map_or(THEME_SYSTEM, |th| th.id),
                    None => THEME_SYSTEM,
                }
                .to_string()
            },
        );
        p.heading(&t("prefs-fit"));
        let fits: Vec<String> = FitMode::ALL
            .iter()
            .map(|m| t(&format!("fit-{}", m.name())))
            .collect();
        self.pref_choice(
            &p,
            &t("prefs-default-fit"),
            &fits,
            |s| s.view.fit.index(),
            |s, i| s.view.fit = FitMode::from_index(i),
        );
        let axes = [
            t("prefs-axis-width"),
            t("prefs-axis-height"),
            t("prefs-axis-both"),
        ];
        self.pref_choice(
            &p,
            &t("prefs-fit-size-axis"),
            &axes,
            |s| match s.view.fit_size.axis {
                FitSizeAxis::Width => 0,
                FitSizeAxis::Height => 1,
                FitSizeAxis::Both => 2,
            },
            |s, i| {
                s.view.fit_size.axis = match i {
                    0 => FitSizeAxis::Width,
                    2 => FitSizeAxis::Both,
                    _ => FitSizeAxis::Height,
                }
            },
        );
        self.pref_spin(
            &p,
            &t("prefs-fit-size-px"),
            (100.0, 10000.0, 50.0),
            0,
            |s| s.view.fit_size.px as f64,
            |s, v| s.view.fit_size.px = v as u32,
        );
        self.pref_switch(
            &p,
            &t("prefs-scale-up"),
            Some(&t("prefs-scale-up-hint")),
            |s| s.view.scale_up,
            |s, v| s.view.scale_up = v,
        );
        p.heading(&t("prefs-spreads"));
        self.pref_switch(
            &p,
            &t("prefs-start-double"),
            None,
            |s| s.view.double_page,
            |s, v| s.view.double_page = v,
        );
        self.pref_switch(
            &p,
            &t("prefs-start-manga"),
            None,
            |s| s.view.manga,
            |s, v| s.view.manga = v,
        );
        self.pref_switch(
            &p,
            &t("panel-cover-alone"),
            None,
            |s| s.view.cover_alone,
            |s, v| s.view.cover_alone = v,
        );
        self.pref_switch(
            &p,
            &t("prefs-wide-alone"),
            Some(&t("prefs-wide-alone-hint")),
            |s| s.view.wide_alone,
            |s, v| s.view.wide_alone = v,
        );
        self.pref_switch(
            &p,
            &t("prefs-double-step"),
            None,
            |s| s.view.double_step,
            |s, v| s.view.double_step = v,
        );
        p.heading(&t("prefs-window"));
        self.pref_switch(
            &p,
            &t("prefs-start-fullscreen"),
            None,
            |s| s.view.fullscreen,
            |s, v| s.view.fullscreen = v,
        );
        self.pref_switch(
            &p,
            &t("prefs-exif"),
            Some(&t("prefs-exif-hint")),
            |s| s.view.exif_rotation,
            |s, v| s.view.exif_rotation = v,
        );
        self.pref_switch(
            &p,
            &t("prefs-page-numbers"),
            None,
            |s| s.appearance.thumbnail_page_numbers,
            |s, v| s.appearance.thumbnail_page_numbers = v,
        );
        tab(&p, &t("prefs-tab-reading"));

        // Scrolling.
        let p = Page::new();
        p.heading(&t("prefs-smart-scroll"));
        self.pref_switch(
            &p,
            &t("prefs-smart-scroll-on"),
            Some(&t("prefs-smart-scroll-hint")),
            |s| s.scroll.smart_scroll,
            |s, v| s.scroll.smart_scroll = v,
        );
        self.pref_switch(
            &p,
            &t("action-invert-smart-scroll"),
            Some(&t("prefs-invert-hint")),
            |s| s.scroll.invert_smart_scroll,
            |s, v| s.scroll.invert_smart_scroll = v,
        );
        self.pref_spin(
            &p,
            &t("prefs-smart-step"),
            (10.0, 100.0, 5.0),
            0,
            |s| (s.scroll.smart_scroll_fraction * 100.0).round(),
            |s, v| s.scroll.smart_scroll_fraction = v / 100.0,
        );
        p.heading(&t("prefs-keys-wheel"));
        self.pref_spin(
            &p,
            &t("prefs-key-step"),
            (5.0, 1000.0, 5.0),
            0,
            |s| s.scroll.key_step_px as f64,
            |s, v| s.scroll.key_step_px = v as u32,
        );
        self.pref_spin(
            &p,
            &t("prefs-wheel-step"),
            (5.0, 1000.0, 5.0),
            0,
            |s| s.scroll.wheel_step_px as f64,
            |s, v| s.scroll.wheel_step_px = v as u32,
        );
        self.pref_switch(
            &p,
            &t("prefs-flip-wheel"),
            Some(&t("prefs-flip-wheel-hint")),
            |s| s.scroll.flip_with_wheel,
            |s, v| s.scroll.flip_with_wheel = v,
        );
        self.pref_spin(
            &p,
            &t("prefs-edge-presses"),
            (1.0, 10.0, 1.0),
            0,
            |s| s.scroll.edge_presses_to_turn as f64,
            |s, v| s.scroll.edge_presses_to_turn = v as u32,
        );
        tab(&p, &t("prefs-tab-scrolling"));

        // Files.
        let p = Page::new();
        p.heading(&t("prefs-books"));
        self.pref_switch(
            &p,
            &t("prefs-open-next"),
            Some(&t("prefs-open-next-hint")),
            |s| s.files.open_next_book,
            |s, v| s.files.open_next_book = v,
        );
        self.pref_switch(
            &p,
            &t("prefs-open-next-folder"),
            None,
            |s| s.files.open_next_folder,
            |s, v| s.files.open_next_folder = v,
        );
        self.pref_switch(
            &p,
            &t("prefs-slideshow-next"),
            None,
            |s| s.slideshow.next_book,
            |s, v| s.slideshow.next_book = v,
        );
        self.pref_switch(
            &p,
            &t("prefs-reopen-last"),
            None,
            |s| s.files.reopen_last,
            |s, v| s.files.reopen_last = v,
        );
        self.pref_switch(
            &p,
            &t("prefs-escape-quits"),
            None,
            |s| s.files.escape_quits,
            |s, v| s.files.escape_quits = v,
        );
        p.heading(&t("prefs-memory"));
        self.pref_spin(
            &p,
            &t("prefs-cached-pages"),
            (1.0, 50.0, 1.0),
            0,
            |s| s.files.cached_pages as f64,
            |s, v| s.files.cached_pages = v as u32,
        );
        tab(&p, &t("prefs-tab-files"));

        // Keys.
        let p = Page::new();
        self.fill_keys_page(&p.root, &dialog);
        tab(&p, &t("prefs-tab-keys"));

        dialog.set_child(Some(&notebook));
        dialog.present();
    }
}
