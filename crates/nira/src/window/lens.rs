//! The magnifying lens: an enlarged crop of the full-resolution page under
//! the pointer. The wheel changes the magnification while it is on.

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib};
use nira_core::i18n::t_args;
use nira_core::image_loader::crop_rgba;
use nira_core::reader::SlotImage;
use nira_core::settings::{LENS_MAX, LENS_MIN, LENS_STEP};

use super::NiraWindow;
use crate::chrome::lens::LensView;
use crate::texture::{self, SizedPaintable};

impl NiraWindow {
    /// Put the lens over the reading area `overlay`.
    pub(super) fn build_lens(&self, overlay: &gtk::Overlay) {
        let lens = LensView::new();
        overlay.add_overlay(&lens.root);
        overlay.add_overlay(&lens.label);
        let motion = gtk::EventControllerMotion::new();
        motion.connect_leave(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| {
                if let Some(lens) = win.imp().lens.get() {
                    lens.hide();
                }
            }
        ));
        overlay.add_controller(motion);
        let _ = self.imp().lens.set(lens);
    }

    pub(super) fn set_lens(&self, on: bool) {
        let imp = self.imp();
        imp.lens_on.set(on);
        self.set_action_state("lens", on.to_variant());
        let overlay = imp.scrolled.parent();
        if let Some(overlay) = overlay {
            overlay.set_cursor_from_name(on.then_some("none"));
        }
        if !on {
            if let Some(lens) = imp.lens.get() {
                lens.hide();
            }
        } else {
            let (x, y) = imp.pointer.get();
            self.update_lens(x, y);
        }
    }

    /// Follow the pointer at (`x`, `y`) in the reading area.
    pub(super) fn update_lens(&self, x: f64, y: f64) {
        let imp = self.imp();
        if !imp.lens_on.get() {
            return;
        }
        let Some(lens) = imp.lens.get() else {
            return;
        };
        let Some(overlay) = imp.scrolled.parent() else {
            return;
        };
        let (size, mag) = {
            let l = &self.imp_settings().lens;
            (l.size.max(40), l.magnification.clamp(LENS_MIN, LENS_MAX))
        };
        // The page under the pointer, and where on it.
        let pages = *imp.pic_pages.borrow();
        let hit = imp.pics.iter().zip(pages).find_map(|(pic, page)| {
            let page = page?;
            let b = pic.compute_bounds(&overlay)?;
            let inside = x >= b.x() as f64
                && x < (b.x() + b.width()) as f64
                && y >= b.y() as f64
                && y < (b.y() + b.height()) as f64;
            (inside && b.width() > 0.0).then_some((page, b))
        });
        let Some((page, bounds)) = hit else {
            lens.hide();
            return;
        };
        let Some((fw, fh, px)) = self.with_reader(|r| r.page_pixels(page)) else {
            lens.hide();
            return;
        };
        // Full-resolution pixels per screen pixel.
        let scale = fw as f64 / bounds.width() as f64;
        let side = ((size as f64 / mag) * scale).round().max(1.0) as u32;
        let cx = (x - bounds.x() as f64) * scale;
        let cy = (y - bounds.y() as f64) * scale;
        let rgba = crop_rgba(
            &px,
            fw,
            fh,
            (cx - side as f64 / 2.0).round() as i64,
            (cy - side as f64 / 2.0).round() as i64,
            side,
            side,
        );
        let tex: gdk::Texture = texture::from_slot_image(SlotImage::Scaled {
            w: side,
            h: side,
            rgba,
        });
        lens.picture
            .set_paintable(Some(&SizedPaintable::new(&tex, size, size)));
        lens.label
            .set_text(&t_args("lens-label", &[("zoom", format!("{mag}").into())]));
        lens.place(x, y, size, overlay.width(), overlay.height());
    }

    /// The wheel while the lens is on changes its magnification. True if
    /// the wheel was used for that.
    pub(super) fn lens_wheel(&self, dy: f64) -> bool {
        if !self.imp().lens_on.get() || dy == 0.0 {
            return false;
        }
        let step = if dy < 0.0 { LENS_STEP } else { -LENS_STEP };
        self.update_settings(|s| {
            s.lens.magnification = (s.lens.magnification + step).clamp(LENS_MIN, LENS_MAX);
        });
        let (x, y) = self.imp().pointer.get();
        self.update_lens(x, y);
        true
    }
}
