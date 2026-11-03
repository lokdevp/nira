//! Drawing the spread, refitting on resize, fit modes, zoom and the
//! double-page and manga toggles.

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib};
use nira_core::reader::Frame;
use nira_core::scroll::{Place, SpreadLayout};
use nira_core::zoom::{FitMode, ZoomModel};

use super::NiraWindow;
use crate::chrome::toolbar::Toolbar;
use crate::texture::{self, SizedPaintable};

impl NiraWindow {
    /// The area pages are fitted into. Before the window is mapped the
    /// scrolled view has no size yet; use the window's default size then.
    pub(super) fn viewport(&self) -> (f64, f64) {
        let scrolled = &self.imp().scrolled;
        let (w, h) = (scrolled.width(), scrolled.height());
        let (w, h) = if w > 50 && h > 50 {
            (w, h)
        } else {
            self.default_size()
        };
        ((w as f64 - 4.0).max(50.0), (h as f64 - 4.0).max(50.0))
    }

    /// Refit the pages when the scrolled view changes size. The
    /// adjustments' page size is the viewport size, so their notifications
    /// cover window resizes, maximizing and side panels. The work is
    /// deferred to an idle: they fire during size allocation.
    pub(super) fn watch_viewport(&self) {
        let scrolled = &self.imp().scrolled;
        for adj in [scrolled.hadjustment(), scrolled.vadjustment()] {
            adj.connect_page_size_notify(glib::clone!(
                #[weak(rename_to = win)]
                self,
                move |_| {
                    if !win.imp().viewport_check.replace(true) {
                        glib::idle_add_local_once(glib::clone!(
                            #[weak]
                            win,
                            move || win.check_viewport()
                        ));
                    }
                }
            ));
        }
    }

    fn check_viewport(&self) {
        let imp = self.imp();
        imp.viewport_check.set(false);
        let collapse = Toolbar::collapse_for(self.width());
        if collapse != imp.collapse.get() {
            imp.collapse.set(collapse);
            if let Some(toolbar) = imp.toolbar.get() {
                toolbar.set_collapse(collapse);
            }
        }
        let size = (imp.scrolled.width(), imp.scrolled.height());
        if size != imp.viewport.get() && size.0 > 50 && size.1 > 50 {
            imp.viewport.set(size);
            let viewport = self.viewport();
            self.with_reader(|r| r.set_viewport(viewport));
            self.show_spread(None);
        }
    }

    /// Draw the current spread if its pages are at hand (otherwise it is
    /// drawn when its decode arrives), then scroll to `place`.
    pub(super) fn show_spread(&self, place: Option<Place>) {
        if place.is_some() {
            self.imp().scroll_to.set(place);
        }
        self.update_chrome();
        let frame =
            self.with_reader(|r| (r.book().is_some() && !r.is_loading()).then(|| r.frame()));
        if let Some(frame) = frame {
            self.draw(frame);
        }
        self.pump();
    }

    pub(super) fn draw(&self, mut frame: Frame) {
        let imp = self.imp();
        {
            let mut textures = imp.textures.borrow_mut();
            let len = frame.spread.len().max(textures.len());
            textures.resize(len, None);
            for update in std::mem::take(&mut frame.updates) {
                textures[update.slot] = Some((update.page, texture::from_slot_image(update.image)));
            }
            // Slots still holding another page's pixels are cleared.
            for (slot, tex) in textures.iter_mut().enumerate() {
                if tex.as_ref().map(|(p, _)| *p) != frame.spread.get(slot).copied() {
                    *tex = None;
                }
            }
        }
        if frame.sizes.is_empty() {
            return;
        }
        while let Some(child) = imp.content.first_child() {
            imp.content.remove(&child);
        }
        {
            let mut pic_pages = [None; 2];
            for (pos, &slot) in frame.visual.iter().enumerate().take(2) {
                pic_pages[pos] = frame.spread.get(slot).copied();
            }
            imp.pic_pages.replace(pic_pages);
            let textures = imp.textures.borrow();
            for (pos, &slot) in frame.visual.iter().enumerate() {
                let pic = &imp.pics[pos.min(1)];
                let (w, h) = frame.sizes[slot];
                match textures.get(slot).and_then(Option::as_ref) {
                    Some((_, tex)) => pic.set_paintable(Some(&SizedPaintable::new(tex, w, h))),
                    None => pic.set_paintable(gdk::Paintable::NONE),
                }
                pic.set_size_request(w as i32, h as i32);
                imp.content.append(pic);
            }
        }
        self.update_background();
        let scrolled = (imp.scrolled.width() as f64, imp.scrolled.height() as f64);
        let manga = self.with_reader(|r| r.view.manga);
        imp.layout
            .replace(Some(SpreadLayout::new(&frame.sizes, scrolled, manga)));
        self.update_zoom_text(&frame);
        if imp.scroll_to.get().is_some() {
            self.apply_scroll_to();
            // Layout runs before default-priority idles: by then the
            // adjustments know the new content size.
            glib::idle_add_local_once(glib::clone!(
                #[weak(rename_to = win)]
                self,
                move || {
                    win.apply_scroll_to();
                    win.imp().scroll_to.set(None);
                }
            ));
        }
    }

    /// Scroll to where the last page turn wants to be.
    pub(super) fn apply_scroll_to(&self) {
        let imp = self.imp();
        let Some(place) = imp.scroll_to.get() else {
            return;
        };
        let Some((x, y)) = imp.layout.borrow_mut().as_mut().map(|l| l.position(place)) else {
            return;
        };
        imp.scrolled.hadjustment().set_value(x);
        imp.scrolled.vadjustment().set_value(y);
    }

    /// Zoom shown in the toolbar: display width over the first page's full
    /// width.
    fn update_zoom_text(&self, frame: &Frame) {
        let full = frame
            .spread
            .first()
            .and_then(|&p| self.with_reader(|r| r.page_size(p)));
        let text = match (full, frame.sizes.first()) {
            (Some((fw, _)), Some(&(w, _))) if fw > 0 => {
                format!("{}%", (w as f64 / fw as f64 * 100.0).round())
            }
            _ => String::new(),
        };
        self.imp().zoom_text.replace(text);
        self.update_chrome();
    }

    /// Pick a fit mode and save it.
    pub(super) fn set_fit(&self, mode: FitMode) {
        self.apply_fit(mode);
        self.update_settings(|s| s.view.fit = mode);
    }

    /// Show a fit mode without saving it.
    fn apply_fit(&self, mode: FitMode) {
        self.set_action_state("fit", mode.name().to_variant());
        self.with_reader(|r| r.zoom.fit_mode = mode);
        self.show_spread(Some(Place::Start));
    }

    /// Change the user zoom; the view keeps its position.
    pub(super) fn zoom(&self, f: fn(&mut ZoomModel)) {
        self.with_reader(|r| f(&mut r.zoom));
        self.show_spread(None);
    }

    /// Actual size: no fitting, no user zoom.
    pub(super) fn zoom_reset(&self) {
        self.with_reader(|r| r.zoom.reset_user_zoom());
        self.set_fit(FitMode::Manual);
    }

    /// Rotate every page by `degrees` clockwise (this session only).
    pub(super) fn rotate(&self, degrees: i32) {
        self.transform(|t| t.rotation = (t.rotation + degrees).rem_euclid(360));
    }

    pub(super) fn transform(&self, f: impl FnOnce(&mut nira_core::reader::Transform)) {
        self.with_reader(|r| {
            f(&mut r.transform);
            r.invalidate();
        });
        self.imp().textures.borrow_mut().clear();
        self.show_spread(Some(nira_core::scroll::Place::Start));
    }

    /// A small dialog to type a page number.
    pub(super) fn ask_page(&self) {
        let (page, total) = self.with_reader(|r| (r.page() + 1, r.page_count()));
        if total == 0 {
            return;
        }
        let dialog = gtk::Window::builder()
            .title(nira_core::i18n::t("action-go-to-page"))
            .transient_for(self)
            .modal(true)
            .resizable(false)
            .build();
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row.set_margin_top(16);
        row.set_margin_bottom(16);
        row.set_margin_start(16);
        row.set_margin_end(16);
        let spin = gtk::SpinButton::with_range(1.0, total as f64, 1.0);
        spin.set_value(page as f64);
        let of = gtk::Label::new(Some(&format!("/ {total}")));
        let go = gtk::Button::with_label(&nira_core::i18n::t("go-to-page-go"));
        go.add_css_class("suggested-action");
        row.append(&spin);
        row.append(&of);
        row.append(&go);
        dialog.set_child(Some(&row));
        go.connect_clicked(glib::clone!(
            #[weak(rename_to = win)]
            self,
            #[weak]
            dialog,
            #[weak]
            spin,
            move |_| {
                spin.update();
                win.go_to(spin.value() as usize - 1);
                dialog.close();
            }
        ));
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed(glib::clone!(
            #[weak]
            dialog,
            #[weak]
            go,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, _| {
                use gtk::gdk::Key;
                match key {
                    Key::Escape => dialog.close(),
                    Key::Return | Key::KP_Enter => go.emit_clicked(),
                    _ => return glib::Propagation::Proceed,
                }
                glib::Propagation::Stop
            }
        ));
        dialog.add_controller(keys);
        dialog.present();
        spin.grab_focus();
    }

    pub(super) fn double_page(&self) -> bool {
        self.with_reader(|r| r.view.double_page)
    }

    pub(super) fn manga(&self) -> bool {
        self.with_reader(|r| r.view.manga)
    }

    /// Pick Single or Spread and save it.
    pub(super) fn set_double_page(&self, on: bool) {
        self.apply_double_page(on);
        self.update_settings(|s| s.view.double_page = on);
    }

    /// Pick the reading direction and save it.
    pub(super) fn set_manga(&self, on: bool) {
        self.apply_manga(on);
        self.update_settings(|s| s.view.manga = on);
    }

    /// Command-line options: this run only, never saved.
    pub fn apply_overrides(&self, manga: bool, double_page: bool, fit: Option<FitMode>) {
        if manga {
            self.apply_manga(true);
        }
        if double_page {
            self.apply_double_page(true);
        }
        if let Some(fit) = fit {
            self.apply_fit(fit);
        }
    }

    /// Show Single or Spread without saving it.
    pub(super) fn apply_double_page(&self, on: bool) {
        let layout = if on { "spread" } else { "single" };
        self.set_action_state("page-layout", layout.to_variant());
        self.with_reader(|r| {
            r.view.double_page = on;
            r.refresh();
        });
        self.show_spread(Some(Place::Start));
    }

    /// Show a reading direction without saving it.
    pub(super) fn apply_manga(&self, on: bool) {
        self.set_action_state("direction", if on { "rtl" } else { "ltr" }.to_variant());
        self.with_reader(|r| r.view.manga = on);
        self.show_spread(Some(Place::Start));
    }
}
