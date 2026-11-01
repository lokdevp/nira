//! Image decoding, rotation, enhancement and thumbnails (RGBA8 pixels; the
//! GTK texture conversion lives in the app crate).

use image::imageops;
use image::RgbaImage;

use gdk_pixbuf::prelude::*;

/// Decode raw image bytes into an RGBA8 image.
pub fn decode_rgba(bytes: &[u8]) -> Result<RgbaImage, String> {
    decode_oriented(bytes, false)
}

/// Decode raw image bytes into an RGBA8 image, turned upright by its EXIF
/// orientation when `exif` is set.
pub fn decode_oriented(bytes: &[u8], exif: bool) -> Result<RgbaImage, String> {
    use image::ImageDecoder;
    let err = |e: image::ImageError| format!("cannot decode image: {e}");
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| format!("cannot read image: {e}"))?;
    let mut decoder = reader.into_decoder().map_err(err)?;
    let orientation = if exif {
        decoder.orientation().ok()
    } else {
        None
    };
    let mut img = image::DynamicImage::from_decoder(decoder).map_err(err)?;
    if let Some(o) = orientation {
        img.apply_orientation(o);
    }
    Ok(img.to_rgba8())
}

/// Apply rotation (degrees clockwise, 0/90/180/270) and flips.
/// Takes the image by value: with no rotation it is changed in place, not
/// copied.
pub fn transform(img: RgbaImage, rotation_deg: i32, flip_h: bool, flip_v: bool) -> RgbaImage {
    let mut out = img;
    let rot = rotation_deg.rem_euclid(360);
    match rot {
        // The image crate's rotate90 is clockwise, rotate270 counter-clockwise.
        90 => out = imageops::rotate90(&out),
        180 => out = imageops::rotate180(&out),
        270 => out = imageops::rotate270(&out),
        _ => {}
    }
    if flip_h {
        imageops::flip_horizontal_in_place(&mut out);
    }
    if flip_v {
        imageops::flip_vertical_in_place(&mut out);
    }
    out
}

/// Full-resolution RGBA8 pixels shared between the page cache, the lens and
/// GDK textures without copying.
pub type Pixels = std::sync::Arc<Vec<u8>>;

/// Resize an image to fit within `max_w x max_h`, preserving aspect ratio.
pub fn thumbnail(img: &RgbaImage, max_w: u32, max_h: u32) -> RgbaImage {
    let (w, h) = img.dimensions();
    let scale = ((max_w as f64 / w as f64).min(max_h as f64 / h as f64)).min(1.0);
    if scale >= 1.0 {
        return img.clone();
    }
    let nw = ((w as f64) * scale).max(1.0) as u32;
    let nh = ((h as f64) * scale).max(1.0) as u32;
    imageops::resize(img, nw, nh, imageops::FilterType::Triangle)
}

/// Build a small RGBA thumbnail using gdk-pixbuf's scaled decoding.
///
/// For JPEG (and other formats
/// with decoder-side scaling) the image is decoded at reduced resolution, so
/// thumbnails are far cheaper than a full decode + resize. Returns
/// `(width, height, tight RGBA8 pixels)`.
pub fn thumbnail_pixbuf_rgba(bytes: &[u8], max_w: u32, max_h: u32) -> Option<(u32, u32, Vec<u8>)> {
    // Load through a PixbufLoader with a size hint: JPEG is decoded at
    // reduced resolution (libjpeg DCT scaling), which is the big speedup.
    let loader = gdk_pixbuf::PixbufLoader::new();
    // `set_size` scales to exactly that size; choose it once the image's
    // own size is known, keeping the aspect ratio.
    loader.connect_size_prepared(move |loader, w, h| {
        let (nw, nh) = fit_within(w as u32, h as u32, max_w, max_h);
        if (nw, nh) != (w as u32, h as u32) {
            loader.set_size(nw as i32, nh as i32);
        }
    });
    loader.write(bytes).ok()?;
    loader.close().ok()?;
    let mut pb = loader.pixbuf()?;

    // Loaders that ignore the size hint (e.g. PNG) produce a full-size
    // pixbuf; scale it down explicitly.
    let (w, h) = (pb.width(), pb.height());
    if w > max_w as i32 || h > max_h as i32 {
        let scale = ((max_w as f64 / w as f64).min(max_h as f64 / h as f64)).min(1.0);
        let nw = ((w as f64) * scale).max(1.0) as i32;
        let nh = ((h as f64) * scale).max(1.0) as i32;
        pb = pb.scale_simple(nw, nh, gdk_pixbuf::InterpType::Bilinear)?;
    }

    let w = pb.width() as u32;
    let h = pb.height() as u32;
    let stride = pb.rowstride() as usize;
    let nch = pb.n_channels() as usize;
    let src = pb.pixel_bytes()?;
    let src = src.as_ref();
    let mut rgba = vec![0u8; (w as usize) * (h as usize) * 4];
    if nch == 4 && stride == (w as usize) * 4 {
        let n = rgba.len();
        rgba.copy_from_slice(&src[..n]);
    } else {
        for y in 0..h as usize {
            for x in 0..w as usize {
                let si = y * stride + x * nch;
                let di = (y * (w as usize) + x) * 4;
                rgba[di] = src[si];
                rgba[di + 1] = src[si + 1];
                rgba[di + 2] = src[si + 2];
                rgba[di + 3] = if nch == 4 { src[si + 3] } else { 255 };
            }
        }
    }
    Some((w, h, rgba))
}

/// The largest size within `max_w x max_h` with the aspect ratio of
/// `w x h`, never larger than `w x h`.
pub fn fit_within(w: u32, h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    if w == 0 || h == 0 {
        return (w, h);
    }
    let scale = (max_w as f64 / w as f64)
        .min(max_h as f64 / h as f64)
        .min(1.0);
    (
        ((w as f64) * scale).round().max(1.0) as u32,
        ((h as f64) * scale).round().max(1.0) as u32,
    )
}

/// Fallback thumbnail via the pure-Rust `image` crate (used when gdk-pixbuf
/// cannot decode the format, e.g. some WebP/TIFF variants).
pub fn thumbnail_rgba_fallback(
    bytes: &[u8],
    max_w: u32,
    max_h: u32,
) -> Option<(u32, u32, Vec<u8>)> {
    let img = decode_rgba(bytes).ok()?;
    let thumb = thumbnail(&img, max_w, max_h);
    let (w, h) = thumb.dimensions();
    Some((w, h, thumb.into_raw()))
}

/// Share of pixels ignored at each end when finding the black and white
/// points, so specks and a few pure-white pixels don't decide them.
const LEVELS_CLIP: f64 = 0.005;

/// The value below which `share` of the histogram's pixels lie.
fn percentile(hist: &[u64; 256], total: u64, share: f64) -> u8 {
    let target = (total as f64 * share) as u64;
    let mut seen = 0;
    for (v, &n) in hist.iter().enumerate() {
        seen += n;
        if seen > target {
            return v as u8;
        }
    }
    255
}

/// Auto-enhance for scans: stretch the levels, neutralise yellowed paper,
/// then sharpen lightly.
///
/// The black point is shared by the three channels (from luminance), so
/// colours keep their balance. The white point is per channel, from each
/// channel's brightest pixels: that maps a tinted paper tone to white. It
/// is only used when the channels' white points are close, as they are for
/// paper; a picture whose brightest area is really coloured keeps one
/// shared white point.
pub fn auto_enhance(img: RgbaImage) -> RgbaImage {
    let mut out = img;
    let total = (out.width() as u64) * (out.height() as u64);
    if total == 0 {
        return out;
    }
    let mut luma = [0u64; 256];
    let mut chan = [[0u64; 256]; 3];
    for p in out.pixels() {
        let y = (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) as usize;
        luma[y.min(255)] += 1;
        for c in 0..3 {
            chan[c][p[c] as usize] += 1;
        }
    }
    let black = percentile(&luma, total, LEVELS_CLIP) as f32;
    let shared_white = percentile(&luma, total, 1.0 - LEVELS_CLIP) as f32;
    let mut white = [0f32; 3];
    for c in 0..3 {
        white[c] = percentile(&chan[c], total, 1.0 - LEVELS_CLIP) as f32;
    }
    let spread =
        white.iter().fold(0f32, |m, &w| m.max(w)) - white.iter().fold(255f32, |m, &w| m.min(w));
    if spread > 64.0 {
        white = [shared_white; 3];
    }
    // Nearly flat images (a blank page) are left alone.
    if white.iter().any(|&w| w - black < 32.0) {
        return out;
    }
    let mut lut = [[0u8; 256]; 3];
    for (table, white) in lut.iter_mut().zip(white) {
        let scale = 255.0 / (white - black);
        for (v, out) in table.iter_mut().enumerate() {
            *out = ((v as f32 - black) * scale).round().clamp(0.0, 255.0) as u8;
        }
    }
    for p in out.pixels_mut() {
        for c in 0..3 {
            p[c] = lut[c][p[c] as usize];
        }
    }
    imageops::unsharpen(&out, 0.8, 4)
}

/// The `w x h` region of a `full_w x full_h` RGBA8 image whose top-left
/// corner is at (`x`, `y`), which may lie partly or wholly outside the
/// image; pixels outside are transparent.
pub fn crop_rgba(full: &[u8], full_w: u32, full_h: u32, x: i64, y: i64, w: u32, h: u32) -> Vec<u8> {
    let mut out = vec![0u8; (w as usize) * (h as usize) * 4];
    let (fw, fh) = (full_w as i64, full_h as i64);
    let x0 = x.max(0);
    let x1 = (x + w as i64).min(fw);
    if x1 <= x0 || full.len() < (fw * fh * 4) as usize {
        return out;
    }
    let row_bytes = ((x1 - x0) * 4) as usize;
    for row in 0..h as i64 {
        let sy = y + row;
        if sy < 0 || sy >= fh {
            continue;
        }
        let src = ((sy * fw + x0) * 4) as usize;
        let dst = ((row * w as i64 + (x0 - x)) * 4) as usize;
        out[dst..dst + row_bytes].copy_from_slice(&full[src..src + row_bytes]);
    }
    out
}

/// The average colour of an image's border (`w x h` RGBA8 pixels): the
/// background that blends with the page.
pub fn edge_colour(w: u32, h: u32, rgba: &[u8]) -> Option<[u8; 3]> {
    let (w, h) = (w as usize, h as usize);
    if w == 0 || h == 0 || rgba.len() < w * h * 4 {
        return None;
    }
    let (mut sum, mut n) = ([0u64; 3], 0u64);
    let mut add = |x: usize, y: usize| {
        let i = (y * w + x) * 4;
        for c in 0..3 {
            sum[c] += rgba[i + c] as u64;
        }
        n += 1;
    };
    for x in 0..w {
        add(x, 0);
        if h > 1 {
            add(x, h - 1);
        }
    }
    for y in 1..h.saturating_sub(1) {
        add(0, y);
        if w > 1 {
            add(w - 1, y);
        }
    }
    Some([(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8])
}

/// Brightness and contrast (1.0 = unchanged) and auto-enhance. Takes the
/// image by value, so the default settings cost nothing.
pub fn enhance(img: RgbaImage, brightness: f64, contrast: f64, auto: bool) -> RgbaImage {
    let mut out = img;
    if auto {
        out = auto_enhance(out);
    }
    if (brightness - 1.0).abs() > 0.001 {
        let delta = ((brightness - 1.0) * 255.0).clamp(-255.0, 255.0) as i32;
        image::imageops::colorops::brighten_in_place(&mut out, delta);
    }
    if (contrast - 1.0).abs() > 0.001 {
        let delta = ((contrast - 1.0) * 127.0).clamp(-255.0, 255.0) as f32;
        image::imageops::colorops::contrast_in_place(&mut out, delta);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// A yellowed scan: tinted paper with a dark text block.
    fn scan(paper: [u8; 3], ink: [u8; 3]) -> RgbaImage {
        RgbaImage::from_fn(100, 100, |x, y| {
            let [r, g, b] = if (30..70).contains(&x) && (30..70).contains(&y) {
                ink
            } else {
                paper
            };
            Rgba([r, g, b, 255])
        })
    }

    #[test]
    fn auto_enhance_whitens_yellowed_paper() {
        let out = auto_enhance(scan([238, 228, 178], [40, 38, 30]));
        let paper = out.get_pixel(5, 5);
        assert!(paper.0[..3].iter().all(|&v| v >= 250), "paper {paper:?}");
        let ink = out.get_pixel(50, 50);
        assert!(ink.0[..3].iter().all(|&v| v <= 10), "ink {ink:?}");
    }

    #[test]
    fn auto_enhance_keeps_colour_pictures_coloured() {
        // The brightest area is really red, not tinted paper.
        let out = auto_enhance(scan([230, 40, 40], [20, 20, 20]));
        let p = out.get_pixel(5, 5);
        assert!(p[0] > 200 && p[1] < 80 && p[2] < 80, "still red: {p:?}");
    }

    #[test]
    fn auto_enhance_leaves_blank_pages_alone() {
        let blank = scan([200, 200, 200], [205, 205, 205]);
        let out = auto_enhance(blank.clone());
        assert_eq!(out.get_pixel(5, 5), blank.get_pixel(5, 5));
    }

    #[test]
    fn fit_within_keeps_the_aspect_ratio() {
        assert_eq!(fit_within(210, 297, 160, 160), (113, 160));
        assert_eq!(fit_within(400, 100, 160, 160), (160, 40));
        assert_eq!(fit_within(50, 80, 160, 160), (50, 80), "never enlarged");
    }

    #[test]
    fn pixbuf_thumbnails_keep_the_aspect_ratio() {
        let page = RgbaImage::from_pixel(210, 297, Rgba([200, 10, 10, 255]));
        let mut png = Vec::new();
        page.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let (w, h, rgba) = thumbnail_pixbuf_rgba(&png, 160, 160).unwrap();
        assert_eq!((w, h), (113, 160));
        assert_eq!(rgba.len(), (w * h * 4) as usize);
    }

    #[test]
    fn rotation_is_clockwise() {
        // 2x1: red on the left, blue on the right.
        let img = RgbaImage::from_fn(2, 1, |x, _| {
            if x == 0 {
                Rgba([255, 0, 0, 255])
            } else {
                Rgba([0, 0, 255, 255])
            }
        });
        // Clockwise, the left pixel goes to the top.
        let cw = transform(img.clone(), 90, false, false);
        assert_eq!(cw.dimensions(), (1, 2));
        assert_eq!(cw.get_pixel(0, 0)[0], 255);
        let ccw = transform(img, 270, false, false);
        assert_eq!(ccw.get_pixel(0, 1)[0], 255);
    }

    #[test]
    fn crop_copies_inside_and_clears_outside() {
        // 3x2 image, pixel value = index.
        let full: Vec<u8> = (0..6u8).flat_map(|i| [i, i, i, 255]).collect();
        assert_eq!(
            crop_rgba(&full, 3, 2, 1, 0, 2, 2),
            [1, 1, 1, 255, 2, 2, 2, 255, 4, 4, 4, 255, 5, 5, 5, 255]
        );
        // Hanging off the top-left corner: the outside is transparent.
        let c = crop_rgba(&full, 3, 2, -1, -1, 2, 2);
        assert_eq!(&c[..12], &[0; 12]);
        assert_eq!(&c[12..], &[0, 0, 0, 255]);
        // Wholly outside.
        assert!(crop_rgba(&full, 3, 2, 10, 10, 2, 2).iter().all(|&b| b == 0));
    }

    #[test]
    fn edge_colour_averages_the_border() {
        // 3x3: white border, black centre.
        let mut px = vec![255u8; 3 * 3 * 4];
        px[(3 + 1) * 4..(3 + 1) * 4 + 3].copy_from_slice(&[0, 0, 0]);
        assert_eq!(edge_colour(3, 3, &px), Some([255, 255, 255]));
        assert_eq!(edge_colour(1, 1, &[10, 20, 30, 255]), Some([10, 20, 30]));
        assert_eq!(edge_colour(0, 4, &[]), None);
    }
}

#[cfg(test)]
mod bench {
    use super::*;
    use std::time::Instant;

    fn load_image(name: &str) -> Vec<u8> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("test")
            .join("files")
            .join("images")
            .join(name);
        std::fs::read(path).expect("read test image")
    }

    #[test]
    #[ignore]
    fn thumbnail_speed() {
        let names = [
            "landscape-no-exif.jpg",
            "portrait-no-exif.jpg",
            "pattern.jpg",
        ];
        for name in names {
            let bytes = load_image(name);
            let n = 200;
            let t0 = Instant::now();
            for _ in 0..n {
                let _ = thumbnail_pixbuf_rgba(&bytes, 160, 160);
            }
            let t1 = Instant::now();
            for _ in 0..n {
                let _ = thumbnail_rgba_fallback(&bytes, 160, 160);
            }
            let t2 = Instant::now();
            eprintln!(
                "{name}: gdk-pixbuf {:?} vs image-crate {:?} ({:.1}x)",
                t1 - t0,
                t2 - t1,
                (t2 - t1).as_secs_f64() / (t1 - t0).as_secs_f64().max(1e-9)
            );
        }
    }
}
