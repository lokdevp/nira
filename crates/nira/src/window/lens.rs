//! The magnifying lens: the full-resolution page under the pointer,
//! enlarged. The wheel changes the magnification while it is on.

use std::cell::Cell;
use std::time::Duration;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::glib;
use nira_core::settings::{LENS_MAX, LENS_MIN, LENS_STEP};

use super::NiraWindow;
use crate::chrome::lens::LensView;

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
        imp.lens_wheel_acc.set(0.0);
        self.set_action_state("lens", on.to_variant());
        let overlay = imp.scrolled.parent();
        if let Some(overlay) = overlay {
            overlay.set_cursor_from_name(on.then_some("none"));
        }
        if !on {
            if let Some(lens) = imp.lens.get() {
                lens.clear();
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
        lens.set_page(fw, fh, px);
        lens.area.set_view(
            ((x - bounds.x() as f64) * scale) as f32,
            ((y - bounds.y() as f64) * scale) as f32,
            (mag / scale) as f32,
        );
        lens.set_magnification(mag);
        lens.place(x, y, size, overlay.width(), overlay.height());
    }

    /// The wheel while the lens is on changes its magnification: one step
    /// per wheel notch, or per notch's worth of touchpad movement. True if
    /// the wheel was used for that.
    pub(super) fn lens_wheel(&self, dy: f64) -> bool {
        let imp = self.imp();
        if !imp.lens_on.get() {
            return false;
        }
        let Some(notches) = take_notches(&imp.lens_wheel_acc, dy) else {
            return true;
        };
        let changed = {
            let mut settings = imp.settings.borrow_mut();
            let old = settings.lens.magnification;
            let new = (old - notches * LENS_STEP).clamp(LENS_MIN, LENS_MAX);
            settings.lens.magnification = new;
            new != old
        };
        if changed {
            self.schedule_lens_save();
            let (x, y) = imp.pointer.get();
            self.update_lens(x, y);
        }
        true
    }

    /// Save the magnification once the wheel has been still for a moment,
    /// rather than on every step.
    fn schedule_lens_save(&self) {
        let imp = self.imp();
        if let Some(id) = imp.lens_save.take() {
            id.remove();
        }
        let id = glib::timeout_add_local_once(
            LENS_SAVE_DELAY,
            glib::clone!(
                #[weak(rename_to = win)]
                self,
                move || {
                    win.imp().lens_save.take();
                    win.update_settings(|_| {});
                }
            ),
        );
        imp.lens_save.replace(Some(id));
    }

    /// Save a magnification change still waiting for its delay (on close).
    pub(super) fn flush_lens_save(&self) {
        if let Some(id) = self.imp().lens_save.take() {
            id.remove();
            self.update_settings(|_| {});
        }
    }
}

/// How long the wheel must be still before the magnification is saved.
const LENS_SAVE_DELAY: Duration = Duration::from_millis(500);

/// Add `dy` (in wheel notches) to the movement in `acc` and take out the
/// whole notches, keeping the remainder. Turning the other way starts
/// afresh, so a reversal acts at once. `None` until a whole notch.
fn take_notches(acc: &Cell<f64>, dy: f64) -> Option<f64> {
    let prev = acc.get();
    let total = if prev * dy < 0.0 { dy } else { prev + dy };
    let notches = total.trunc();
    acc.set(total - notches);
    (notches != 0.0).then_some(notches)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_notches_step_at_once() {
        let acc = Cell::new(0.0);
        assert_eq!(take_notches(&acc, -1.0), Some(-1.0));
        assert_eq!(take_notches(&acc, 2.0), Some(2.0));
        assert_eq!(acc.get(), 0.0);
    }

    #[test]
    fn small_deltas_add_up_to_a_notch() {
        let acc = Cell::new(0.0);
        let steps: Vec<_> = (0..10).filter_map(|_| take_notches(&acc, 0.25)).collect();
        assert_eq!(steps, [1.0, 1.0]);
        assert!((acc.get() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn reversing_drops_the_remainder() {
        let acc = Cell::new(0.0);
        assert_eq!(take_notches(&acc, 0.75), None);
        assert_eq!(take_notches(&acc, -0.5), None);
        assert!((acc.get() + 0.5).abs() < 1e-9);
        assert_eq!(take_notches(&acc, 0.0), None);
    }
}
