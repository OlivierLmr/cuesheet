//! The SVG, rasterised.
//!
//! Not another drawing: this hands the same bytes the SVG emitter produced to resvg, which is
//! pure Rust and depends on no system libraries, so the same picture rasterises identically on
//! every platform. If this ever disagreed with the SVG, something would have been laid out twice.

use crate::picture::Picture;
use crate::svg;

/// Render at `scale`× the picture's own size. Two is a reasonable default for a page that will be
/// looked at on a screen; one is the true size.
pub fn render_scaled(pic: &Picture, scale: f32) -> Result<Vec<u8>, String> {
    let source = svg::render(pic, &svg::Options::default());

    let mut opt = resvg::usvg::Options::default();
    // The embedded face, for the same reason the metrics came from it: a system font would make
    // the raster depend on the machine.
    opt.fontdb_mut().load_font_data(include_bytes!("../assets/Inter-Regular.ttf").to_vec());
    opt.fontdb_mut().set_sans_serif_family("Inter");
    // Every generic family resolves to the one face the binary owns. A family resvg cannot resolve
    // is a family whose text silently does not get drawn.
    opt.fontdb_mut().set_monospace_family("Inter");
    opt.fontdb_mut().set_serif_family("Inter");

    let tree = resvg::usvg::Tree::from_str(&source, &opt).map_err(|e| e.to_string())?;
    let size = tree.size();
    let (w, h) = ((size.width() * scale).ceil() as u32, (size.height() * scale).ceil() as u32);
    let mut map = resvg::tiny_skia::Pixmap::new(w.max(1), h.max(1))
        .ok_or_else(|| "picture has no area".to_string())?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut map.as_mut(),
    );
    map.encode_png().map_err(|e| e.to_string())
}

pub fn render(pic: &Picture) -> Result<Vec<u8>, String> {
    render_scaled(pic, 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{derive, font, layout, parse};

    fn build(src: &str) -> Picture {
        let doc = parse::document(src).unwrap();
        let sheet = crate::stylesheet(None).unwrap();
        let run = derive::run(&doc, &sheet);
        layout::build(&doc, &sheet, &run, &font::Font::embedded())
    }

    #[test]
    fn a_picture_rasterises_to_a_png() {
        let bytes = render(&build("0 n0 -> n1 .rb +10\n5 n1 deliver")).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "should be a PNG");
        assert!(bytes.len() > 500);
    }

    /// The property the whole no-second-renderer argument rests on, checked rather than asserted.
    #[test]
    fn rasterising_is_reproducible() {
        let pic = build("0 n0 -> n1 .rb +10\n100 n1 crash\n120 run end");
        assert_eq!(render(&pic).unwrap(), render(&pic).unwrap());
    }

    #[test]
    fn scale_changes_the_pixels_and_not_the_picture() {
        let pic = build("0 n0 -> n1 .rb +10");
        let small = render_scaled(&pic, 1.0).unwrap();
        let big = render_scaled(&pic, 2.0).unwrap();
        assert!(big.len() > small.len());
    }
}
