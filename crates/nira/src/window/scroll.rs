//! Scrolling: arrow keys and the wheel, which turn the page after a few
//! presses at its edge, and smart scroll, which reads through the spread in
//! reading order before turning.

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use nira_core::scroll::{line_step, Axis, Step};

use super::NiraWindow;

fn axis(adj: &gtk::Adjustment) -> Axis {
    Axis {
        value: adj.value(),
        lower: adj.lower(),
        upper: adj.upper(),
        page_size: adj.page_size(),
    }
}

impl NiraWindow {
    /// Arrow keys: `dx`/`dy` are -1, 0 or 1.
    pub(super) fn key_scroll(&self, dx: f64, dy: f64) {
        let px = self.imp_settings().scroll.key_step_px as f64;
        if dy != 0.0 {
            self.scroll_vertically(dy * px);
        } else {
            let adj = self.imp().scrolled.hadjustment();
            if let Step::To(v) = line_step(axis(&adj), dx * px) {
                adj.set_value(v);
            }
        }
    }

    /// Mouse wheel and touchpad. `dy` is in wheel steps (fractional for
    /// smooth scrolling).
    pub(super) fn connect_wheel(&self) {
        let ctrl = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        // Handled here instead of by the scrolled view, so the page can turn
        // at the edges.
        ctrl.set_propagation_phase(gtk::PropagationPhase::Capture);
        ctrl.connect_scroll(glib::clone!(
            #[weak(rename_to = win)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, _, dy| {
                if win.lens_wheel(dy) {
                    return glib::Propagation::Stop;
                }
                if dy != 0.0 {
                    let px = win.imp_settings().scroll.wheel_step_px as f64;
                    win.scroll_vertically(dy * px);
                }
                glib::Propagation::Stop
            }
        ));
        self.imp().scrolled.add_controller(ctrl);
    }

    fn scroll_vertically(&self, delta: f64) {
        let adj = self.imp().scrolled.vadjustment();
        match line_step(axis(&adj), delta) {
            Step::To(v) => {
                adj.set_value(v);
                self.imp().edge.take();
            }
            // A page that fits turns at once.
            Step::Fits => {
                if self.imp_settings().scroll.flip_with_wheel {
                    self.flip(delta > 0.0, 1);
                }
            }
            Step::Edge => self.press_edge(delta > 0.0),
        }
    }

    /// Space: scroll through the spread in reading order, then turn the
    /// page.
    pub(super) fn smart_scroll(&self, backwards: bool) {
        let imp = self.imp();
        let (smart, fraction, invert) = {
            let s = self.imp_settings();
            (
                s.scroll.smart_scroll,
                s.scroll.smart_scroll_fraction,
                s.scroll.invert_smart_scroll,
            )
        };
        if !smart {
            // Plain scrolling: down (or up) by the step, turning at the
            // edges like the arrow keys.
            let step = imp.scrolled.vadjustment().page_size() * fraction;
            self.scroll_vertically(if backwards { -step } else { step });
            return;
        }
        let (h, v) = (imp.scrolled.hadjustment(), imp.scrolled.vadjustment());
        let next = imp.layout.borrow_mut().as_mut().and_then(|l| {
            l.smart_step(
                (h.value(), v.value()),
                (h.page_size(), v.page_size()),
                fraction,
                backwards,
                invert,
            )
        });
        log::debug!(
            "smart scroll from ({}, {}) in {}x{} of {}x{}: {next:?}",
            h.value(),
            v.value(),
            h.page_size(),
            v.page_size(),
            h.upper(),
            v.upper()
        );
        match next {
            Some((x, y)) => {
                h.set_value(x);
                v.set_value(y);
                imp.edge.take();
            }
            None => self.press_edge(!backwards),
        }
    }

    /// A press against the page edge: turn after enough presses in a row.
    fn press_edge(&self, forward: bool) {
        let (enabled, needed) = {
            let s = self.imp_settings();
            (s.scroll.flip_with_wheel, s.scroll.edge_presses_to_turn)
        };
        let imp = self.imp();
        if !enabled {
            imp.edge.take();
            return;
        }
        let mut guard = imp.edge.get();
        let turn = guard.press(forward, needed);
        log::debug!("edge press forward={forward}: {guard:?} turn={turn}");
        imp.edge.set(guard);
        if turn {
            self.flip(forward, 1);
        }
    }
}
