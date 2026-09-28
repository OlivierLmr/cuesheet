//! How wide a piece of text is.
//!
//! The face is compiled into the binary. Asking the system for a font would make the output depend
//! on the machine that produced it, which would end reproducibility — and reproducibility is what
//! lets a regenerated diagram diff against the one before it, and what the golden tests rest on.
//!
//! Inter, SIL Open Font License 1.1. The licence travels with it in `assets/`.

use ttf_parser::Face;

const BYTES: &[u8] = include_bytes!("../assets/Inter-Regular.ttf");

pub struct Font {
    face: Face<'static>,
}

impl Font {
    pub fn embedded() -> Font {
        Font { face: Face::parse(BYTES, 0).expect("the embedded face is a valid TrueType font") }
    }

    /// Advance width of `text` at `size` pixels.
    ///
    /// Advances only — no kerning, no shaping, no ligatures. A label is a word or three of Latin
    /// text, and the error from ignoring kerning is smaller than the padding around a label box.
    /// Taking the simple road here also keeps the number the same on every machine.
    pub fn width(&self, text: &str, size: f64) -> f64 {
        let upem = self.face.units_per_em() as f64;
        let mut units = 0f64;
        for c in text.chars() {
            units += self.advance(c);
        }
        units * size / upem
    }

    fn advance(&self, c: char) -> f64 {
        self.face
            .glyph_index(c)
            .and_then(|g| self.face.glyph_hor_advance(g))
            .map(|a| a as f64)
            // A character the face has no glyph for still takes room on the page, and guessing
            // half an em is closer than pretending it takes none.
            .unwrap_or(self.face.units_per_em() as f64 * 0.5)
    }

    /// Distance from the baseline to the top of a capital, at `size`. Used to centre a label on
    /// the line it belongs to rather than hanging it off the baseline.
    pub fn cap_height(&self, size: f64) -> f64 {
        let upem = self.face.units_per_em() as f64;
        let cap = self.face.capital_height().map(|c| c as f64).unwrap_or(upem * 0.72);
        cap * size / upem
    }
}

impl Default for Font {
    fn default() -> Self {
        Font::embedded()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_face_parses() {
        let f = Font::embedded();
        assert!(f.width("x", 12.0) > 0.0);
    }

    #[test]
    fn width_grows_with_the_text_and_with_the_size() {
        let f = Font::embedded();
        assert!(f.width("mm", 12.0) > f.width("m", 12.0));
        assert!(f.width("m", 24.0) > f.width("m", 12.0));
    }

    #[test]
    fn width_scales_linearly_with_size() {
        let f = Font::embedded();
        let a = f.width("deliver", 10.0);
        let b = f.width("deliver", 20.0);
        assert!((b - a * 2.0).abs() < 1e-9, "{a} then {b}");
    }

    #[test]
    fn the_empty_string_is_no_wide() {
        assert_eq!(Font::embedded().width("", 12.0), 0.0);
    }

    /// A character the face cannot draw still has to take room, or a label holding one would be
    /// laid out as though it were shorter than it prints.
    #[test]
    fn a_character_with_no_glyph_still_takes_room() {
        let f = Font::embedded();
        assert!(f.width("\u{10FFFF}", 12.0) > 0.0);
    }

    /// The whole reason the face is embedded: the same text is the same width everywhere.
    #[test]
    fn width_is_the_same_on_every_call() {
        let a = Font::embedded().width("mx_request", 11.0);
        let b = Font::embedded().width("mx_request", 11.0);
        assert_eq!(a, b);
    }

    #[test]
    fn a_cap_height_is_a_sensible_fraction_of_the_size() {
        let f = Font::embedded();
        let c = f.cap_height(20.0);
        assert!(c > 8.0 && c < 20.0, "cap height was {c}");
    }
}
