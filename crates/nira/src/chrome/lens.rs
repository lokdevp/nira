//! The magnifying lens: a round frame that follows the pointer and draws
//! part of the full-resolution page, enlarged, with a label under it.

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk};

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use nira_core::i18n::t_args;
use nira_core::image_loader::Pixels;
use nira_core::reader::SlotImage;

use super::label;
use crate::texture;
use crate::theme::px;

pub struct LensView {
    /// The round frame (clips the area to its rounded border).
    pub root: gtk::Box,
    pub area: LensArea,
    pub label: gtk::Label,
    /// The pixels the area's texture wraps, so a page is uploaded once.
    source: RefCell<Option<Pixels>>,
    /// The magnification the label shows.
    shown_mag: Cell<f64>,
}

impl LensView {
    pub fn new() -> LensView {
        let area = LensArea::default();
        area.set_hexpand(true);
        area.set_vexpand(true);
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("lens");
        root.set_overflow(gtk::Overflow::Hidden);
        root.append(&area);
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
            area,
            label,
            source: RefCell::new(None),
            shown_mag: Cell::new(f64::NAN),
        }
    }

    /// Show the `w x h` page `pixels`, making a texture over them (no
    /// copy) only when they aren't the ones already shown.
    pub fn set_page(&self, w: u32, h: u32, pixels: Pixels) {
        let same = self
            .source
            .borrow()
            .as_ref()
            .is_some_and(|p| Arc::ptr_eq(p, &pixels));
        if !same {
            let tex = texture::from_slot_image(SlotImage::Full {
                w,
                h,
                rgba: pixels.clone(),
            });
            self.area.set_texture(Some(tex));
            self.source.replace(Some(pixels));
        }
    }

    /// Label the lens with magnification `mag`.
    pub fn set_magnification(&self, mag: f64) {
        if self.shown_mag.replace(mag) != mag {
            self.label
                .set_text(&t_args("lens-label", &[("zoom", format!("{mag}").into())]));
        }
    }

    /// Hide the lens and let go of its page (when it is turned off).
    pub fn clear(&self) {
        self.hide();
        self.area.set_texture(None);
        self.source.replace(None);
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
        let label_top = (top + size + px(8)).min((height - px(28)).max(0));
        // Setting a property GTK already has still queues a resize, so
        // only what changed is set.
        set_if(&self.root, size, gtk::Widget::width_request, |w, v| {
            w.set_size_request(v, v)
        });
        set_if(&self.root, left, gtk::Widget::margin_start, |w, v| {
            w.set_margin_start(v)
        });
        set_if(&self.root, top, gtk::Widget::margin_top, |w, v| {
            w.set_margin_top(v)
        });
        set_if(
            &self.label,
            left + size / 4,
            gtk::Widget::margin_start,
            |w, v| w.set_margin_start(v),
        );
        set_if(&self.label, label_top, gtk::Widget::margin_top, |w, v| {
            w.set_margin_top(v)
        });
        self.root.set_visible(true);
        self.label.set_visible(true);
    }
}

fn set_if<W: IsA<gtk::Widget>>(
    widget: &W,
    value: i32,
    get: fn(&gtk::Widget) -> i32,
    set: fn(&gtk::Widget, i32),
) {
    let widget = widget.upcast_ref();
    if get(widget) != value {
        set(widget, value);
    }
}

mod imp {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    pub struct LensArea {
        /// The full-resolution page.
        pub texture: RefCell<Option<gdk::Texture>>,
        /// The point of the page (in its own pixels) drawn at the area's
        /// centre, and the area pixels per page pixel.
        pub view: Cell<(f32, f32, f32)>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LensArea {
        const NAME: &'static str = "NiraLensArea";
        type Type = super::LensArea;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for LensArea {}

    impl WidgetImpl for LensArea {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let Some(texture) = self.texture.borrow().clone() else {
                return;
            };
            let widget = self.obj();
            let (w, h) = (widget.width() as f32, widget.height() as f32);
            let (cx, cy, k) = self.view.get();
            let (pw, ph) = (texture.width() as f32 * k, texture.height() as f32 * k);
            let (x, y) = (w / 2.0 - cx * k, h / 2.0 - cy * k);
            snapshot.push_clip(&graphene::Rect::new(0.0, 0.0, w, h));
            // Linear: the page is only ever enlarged here, and trilinear
            // would build mipmaps of the whole full-resolution page.
            snapshot.append_scaled_texture(
                &texture,
                gsk::ScalingFilter::Linear,
                &graphene::Rect::new(x, y, pw, ph),
            );
            snapshot.pop();
        }
    }
}

glib::wrapper! {
    /// Draws a full-resolution page texture enlarged and offset so the
    /// point under the pointer is in the middle. Moving the lens only
    /// changes the offset: no pixels are copied and nothing is uploaded.
    pub struct LensArea(ObjectSubclass<imp::LensArea>)
        @extends gtk::Widget;
}

impl Default for LensArea {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl LensArea {
    /// Show `texture`, or nothing (and let it be freed).
    pub fn set_texture(&self, texture: Option<gdk::Texture>) {
        self.imp().texture.replace(texture);
        self.queue_draw();
    }

    /// Centre the area on the page's pixel (`cx`, `cy`), drawing `k` area
    /// pixels per page pixel.
    pub fn set_view(&self, cx: f32, cy: f32, k: f32) {
        if self.imp().view.replace((cx, cy, k)) != (cx, cy, k) {
            self.queue_draw();
        }
    }
}
