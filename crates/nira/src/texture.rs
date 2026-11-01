//! GDK textures over decoded pixels, and a paintable that keeps a fixed
//! layout size whatever texture backs it.

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk};
use nira_core::image_loader::Pixels;
use nira_core::reader::SlotImage;

/// Lets `glib::Bytes` borrow shared pixels.
struct PixelBytes(Pixels);

impl AsRef<[u8]> for PixelBytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Wrap `w x h` RGBA8 pixels in a texture without copying them.
fn memory_texture(w: u32, h: u32, bytes: glib::Bytes) -> gdk::Texture {
    gdk::MemoryTexture::new(
        w as i32,
        h as i32,
        gdk::MemoryFormat::R8g8b8a8,
        &bytes,
        (w as usize) * 4,
    )
    .into()
}

/// A texture over a slot's pixels (no copy; the texture keeps them alive).
pub fn from_slot_image(image: SlotImage) -> gdk::Texture {
    match image {
        SlotImage::Scaled { w, h, rgba } => memory_texture(w, h, glib::Bytes::from_owned(rgba)),
        SlotImage::Full { w, h, rgba } => {
            memory_texture(w, h, glib::Bytes::from_owned(PixelBytes(rgba)))
        }
    }
}

mod imp {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    pub struct SizedPaintable {
        pub texture: RefCell<Option<gdk::Texture>>,
        pub size: Cell<(i32, i32)>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SizedPaintable {
        const NAME: &'static str = "NiraSizedPaintable";
        type Type = super::SizedPaintable;
        type Interfaces = (gdk::Paintable,);
    }

    impl ObjectImpl for SizedPaintable {}

    impl PaintableImpl for SizedPaintable {
        fn intrinsic_width(&self) -> i32 {
            self.size.get().0
        }

        fn intrinsic_height(&self) -> i32 {
            self.size.get().1
        }

        fn flags(&self) -> gdk::PaintableFlags {
            // A new paintable is made for every change; this one never changes.
            gdk::PaintableFlags::SIZE | gdk::PaintableFlags::CONTENTS
        }

        fn snapshot(&self, snapshot: &gdk::Snapshot, width: f64, height: f64) {
            let Some(texture) = self.texture.borrow().clone() else {
                return;
            };
            let Some(snapshot) = snapshot.downcast_ref::<gtk::Snapshot>() else {
                return;
            };
            // Trilinear filtering keeps a texture drawn far below its size
            // (the interim full-res page) clean until the sharp copy arrives.
            snapshot.append_scaled_texture(
                &texture,
                gsk::ScalingFilter::Trilinear,
                &graphene::Rect::new(0.0, 0.0, width as f32, height as f32),
            );
        }
    }
}

glib::wrapper! {
    /// `gtk::Picture` takes its natural size from its paintable. A page may
    /// briefly show a texture of another size (the cached full-res page, or
    /// the previous zoom level) until the worker's sharp copy arrives; this
    /// paintable keeps the layout at the display size throughout, so
    /// swapping textures never moves the page.
    pub struct SizedPaintable(ObjectSubclass<imp::SizedPaintable>)
        @implements gdk::Paintable;
}

impl SizedPaintable {
    /// `texture`, laid out as `width x height` whatever its own size.
    pub fn new(texture: &gdk::Texture, width: u32, height: u32) -> Self {
        let paintable: Self = glib::Object::new();
        let imp = paintable.imp();
        imp.texture.replace(Some(texture.clone()));
        imp.size.set((
            width.min(i32::MAX as u32) as i32,
            height.min(i32::MAX as u32) as i32,
        ));
        paintable
    }
}
