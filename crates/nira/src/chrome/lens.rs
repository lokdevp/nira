//! The magnifying lens: a round, clipped picture that follows the pointer,
//! with a label under it.

use gtk::prelude::*;

use super::label;

pub struct LensView {
    /// The round frame (clips the picture).
    pub root: gtk::Box,
    pub picture: gtk::Picture,
    pub label: gtk::Label,
}

impl LensView {
    pub fn new() -> LensView {
        let picture = gtk::Picture::new();
        picture.set_can_shrink(true);
        picture.set_content_fit(gtk::ContentFit::Fill);
        picture.set_hexpand(true);
        picture.set_vexpand(true);
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("lens");
        root.set_overflow(gtk::Overflow::Hidden);
        root.append(&picture);
        let label = label("", &["lens-label"]);
        for w in [root.upcast_ref::<gtk::Widget>(), label.upcast_ref()] {
            w.set_halign(gtk::Align::Start);
            w.set_valign(gtk::Align::Start);
            // The pointer passes through to the page.
            w.set_can_target(false);
            w.set_visible(false);
        }
        LensView {
            root,
            picture,
            label,
        }
    }

    pub fn hide(&self) {
        self.root.set_visible(false);
        self.label.set_visible(false);
    }

    /// Place the lens (diameter `size`) centred on (`x`, `y`) within an
    /// area `width x height`, with the label under it.
    pub fn place(&self, x: f64, y: f64, size: u32, width: i32, height: i32) {
        let size = size as i32;
        let left = (x as i32 - size / 2).clamp(0, (width - size).max(0));
        let top = (y as i32 - size / 2).clamp(0, (height - size).max(0));
        self.root.set_size_request(size, size);
        self.root.set_margin_start(left);
        self.root.set_margin_top(top);
        self.label.set_margin_start(left + size / 4);
        self.label
            .set_margin_top((top + size + 8).min((height - 28).max(0)));
        self.root.set_visible(true);
        self.label.set_visible(true);
    }
}
