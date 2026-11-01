//! Page turns and jumps.

use gtk::subclass::prelude::*;
use nira_core::reader::Flip;
use nira_core::scroll::Place;

use super::NiraWindow;

impl NiraWindow {
    pub(super) fn flip(&self, forward: bool, count: usize) {
        self.turn(forward, count);
    }

    /// Turn `count` spreads and say what happened.
    pub(super) fn turn(&self, forward: bool, count: usize) -> Flip {
        let flip = self.with_reader(|r| r.flip(forward, count));
        log::debug!("flip forward={forward} count={count}: {flip:?}");
        self.after_move(flip, if forward { Place::Start } else { Place::End });
        flip
    }

    pub(super) fn go_to_first(&self) {
        let flip = self.with_reader(|r| r.go_to(0));
        self.after_move(flip, Place::Start);
    }

    pub(super) fn go_to_last(&self) {
        let flip = self.with_reader(|r| {
            let last = r.last_page();
            r.go_to(last)
        });
        self.after_move(flip, Place::Start);
    }

    fn after_move(&self, flip: Flip, place: Place) {
        match flip {
            Flip::Moved => {}
            Flip::PastEnd => {
                self.turn_past_book(true);
                return;
            }
            Flip::BeforeStart => {
                self.turn_past_book(false);
                return;
            }
            Flip::Stay => return,
        }
        self.imp().edge.take();
        self.show_spread(Some(place));
    }
}
