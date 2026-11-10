//! The reading area's floating controls: the page bar and click zones
//! (which fade out while reading), scrubbing, and the slideshow.

use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use nira_core::reader::Flip;
use nira_core::scroll::Place;

use super::NiraWindow;
use crate::chrome::pagebar::PageBar;

/// Pointer stillness before the controls fade.
const FADE_AFTER: Duration = Duration::from_millis(2500);

impl NiraWindow {
    /// Put the page bar and click zones over the scrolled view.
    pub(super) fn build_reading_area(&self) -> gtk::Overlay {
        let imp = self.imp();
        let bar = PageBar::new();
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&imp.scrolled));
        for zone in &bar.zones {
            overlay.add_overlay(zone);
        }
        overlay.add_overlay(&bar.root);
        self.build_lens(&overlay);

        bar.scale.connect_change_value(glib::clone!(
            #[weak(rename_to = win)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, _, value| {
                let page = (value.round().max(1.0) as usize) - 1;
                if page != win.with_reader(|r| r.page()) {
                    win.go_to(page);
                }
                glib::Propagation::Proceed
            }
        ));

        let motion = gtk::EventControllerMotion::new();
        motion.connect_motion(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_, x, y| {
                // GTK also reports motion when the widgets under a still
                // pointer change (a page turn); only real moves count.
                if win.imp().pointer.replace((x, y)) != (x, y) {
                    win.wake_controls();
                    win.update_lens(x, y);
                }
            }
        ));
        overlay.add_controller(motion);
        // The controls stay while the pointer is on the bar.
        let hover = gtk::EventControllerMotion::new();
        hover.connect_enter(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_, _, _| win.imp().over_bar.set(true)
        ));
        hover.connect_leave(glib::clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.imp().over_bar.set(false)
        ));
        bar.root.add_controller(hover);

        let _ = imp.pagebar.set(bar);
        overlay
    }

    /// Show the controls and restart the countdown to fading them.
    pub(super) fn wake_controls(&self) {
        let imp = self.imp();
        if let Some(bar) = imp.pagebar.get() {
            bar.set_faded(false);
        }
        if let Some(id) = imp.fade_source.take() {
            id.remove();
        }
        if !self.imp_settings().appearance.auto_hide_controls {
            return;
        }
        let id = glib::timeout_add_local_once(
            FADE_AFTER,
            glib::clone!(
                #[weak(rename_to = win)]
                self,
                move || {
                    let imp = win.imp();
                    imp.fade_source.take();
                    if imp.over_bar.get() {
                        win.wake_controls();
                    } else if let Some(bar) = imp.pagebar.get() {
                        log::debug!("fading the page bar");
                        bar.set_faded(true);
                    }
                }
            ),
        );
        imp.fade_source.replace(Some(id));
    }

    /// The left arrow and click zone: back in comics, forward in manga.
    pub(super) fn page_left(&self) {
        self.flip(self.manga(), 1);
    }

    pub(super) fn page_right(&self) {
        self.flip(!self.manga(), 1);
    }

    pub(super) fn go_to(&self, page: usize) {
        let flip = self.with_reader(|r| r.go_to(page));
        if flip == Flip::Moved {
            self.imp().edge.take();
            self.show_spread(Some(Place::Start));
        }
    }

    pub(super) fn set_slideshow(&self, on: bool) {
        let imp = self.imp();
        self.set_action_state("slideshow", on.to_variant());
        if let Some(bar) = imp.pagebar.get() {
            bar.set_playing(on);
        }
        if let Some(id) = imp.slideshow.take() {
            id.remove();
        }
        if !on {
            return;
        }
        let delay = Duration::from_millis(self.imp_settings().slideshow.delay_ms.max(500));
        let id = glib::timeout_add_local(
            delay,
            glib::clone!(
                #[weak(rename_to = win)]
                self,
                #[upgrade_or]
                glib::ControlFlow::Break,
                move || {
                    if win.with_reader(|r| r.is_loading()) {
                        return glib::ControlFlow::Continue;
                    }
                    if win.imp().opening.get() {
                        return glib::ControlFlow::Continue;
                    }
                    // The slideshow's own setting decides whether it goes
                    // on into the next book, whatever the Files setting.
                    let next_book = win.imp_settings().slideshow.next_book;
                    match win.turn_with(true, 1, next_book) {
                        Flip::PastEnd if !(next_book && win.imp().opening.get()) => {
                            win.imp().slideshow.take();
                            win.set_slideshow(false);
                            glib::ControlFlow::Break
                        }
                        _ => glib::ControlFlow::Continue,
                    }
                }
            ),
        );
        imp.slideshow.replace(Some(id));
    }
}
