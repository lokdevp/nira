//! Page turns and jumps.

use gtk::subclass::prelude::*;
use nira_core::reader::Flip;
use nira_core::scroll::Place;

use super::NiraWindow;

impl NiraWindow {
    pub(super) fn flip(&self, forward: bool, count: usize) {
        self.turn(forward, count);
    }

    /// Turn `count` spreads and say what happened. Turning past either
    /// end opens the neighbouring book if the Files setting allows.
    pub(super) fn turn(&self, forward: bool, count: usize) -> Flip {
        self.turn_with(forward, count, self.opens_next_book())
    }

    /// `turn`, with `next_book` deciding whether turning past either end
    /// opens the neighbouring book (the slideshow has its own setting).
    pub(super) fn turn_with(&self, forward: bool, count: usize, next_book: bool) -> Flip {
        let flip = self.with_reader(|r| r.flip(forward, count));
        log::debug!("flip forward={forward} count={count}: {flip:?}");
        let place = if forward { Place::Start } else { Place::End };
        self.after_move(flip, place, next_book);
        flip
    }

    pub(super) fn go_to_first(&self) {
        let flip = self.with_reader(|r| r.go_to(0));
        self.after_move(flip, Place::Start, self.opens_next_book());
    }

    pub(super) fn go_to_last(&self) {
        let flip = self.with_reader(|r| {
            let last = r.last_page();
            r.go_to(last)
        });
        self.after_move(flip, Place::Start, self.opens_next_book());
    }

    fn opens_next_book(&self) -> bool {
        self.imp_settings().files.open_next_book
    }

    fn after_move(&self, flip: Flip, place: Place, next_book: bool) {
        match flip {
            Flip::Moved => {}
            Flip::PastEnd => {
                if next_book {
                    self.turn_past_book(true);
                }
                return;
            }
            Flip::BeforeStart => {
                if next_book {
                    self.turn_past_book(false);
                }
                return;
            }
            Flip::Stay => return,
        }
        self.imp().edge.take();
        self.show_spread(Some(place));
    }
}
