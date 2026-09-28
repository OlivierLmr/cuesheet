//! What the emitters draw.
//!
//! The picture is the single artifact the three stages produce, and the only place a coordinate is
//! ever decided. An emitter that reaches past it into the layout would quietly end the guarantee
//! that SVG, PNG and HTML cannot disagree — so the primitives here are deliberately dumb, and
//! there is nothing in them an emitter has to interpret.

use crate::model::{Color, Dash};

/// Everything a stroke needs. Split out because lines, paths and shapes all carry the same set.
#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    pub color: Color,
    pub width: f64,
    pub dash: Dash,
    pub opacity: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fill {
    pub color: Color,
    pub opacity: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

/// A handle back to what a primitive came from, so the HTML emitter can hang an id and a data
/// attribute on it without the layout knowing anything about HTML.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tag {
    /// Index of the event in the document, when there is one.
    pub event: Option<usize>,
    /// The kind or `arrow`/`lifeline`, for a CSS class in the interactive page.
    pub role: String,
    /// The detail block, verbatim, for the hover panel.
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Prim {
    Line {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        stroke: Stroke,
        /// Draw an arrowhead at (x2, y2).
        head: bool,
        tag: Tag,
    },
    /// An SVG path, already expressed as absolute commands. Used for self-addressed messages,
    /// which leave a lane and return to it, and for ragged edges.
    Path {
        d: String,
        stroke: Option<Stroke>,
        fill: Option<Fill>,
        head: bool,
        tag: Tag,
    },
    Rect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        rx: f64,
        fill: Option<Fill>,
        stroke: Option<Stroke>,
        tag: Tag,
    },
    Circle {
        cx: f64,
        cy: f64,
        r: f64,
        fill: Option<Fill>,
        stroke: Option<Stroke>,
        tag: Tag,
    },
    Polygon {
        points: Vec<(f64, f64)>,
        fill: Option<Fill>,
        stroke: Option<Stroke>,
        tag: Tag,
    },
    Text {
        x: f64,
        y: f64,
        text: String,
        size: f64,
        anchor: Anchor,
        fill: Color,
        opacity: f64,
        /// Degrees, clockwise, about (x, y). A message label rides its arrow, so this is the
        /// arrow's own angle.
        rotate: f64,
        /// Monospace rather than the body face, for the time gutter.
        mono: bool,
        tag: Tag,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub width: f64,
    pub height: f64,
    pub background: Color,
    pub prims: Vec<Prim>,
    /// Title, for the accessible name and the browser tab.
    pub title: Option<String>,
    /// Provenance, copied into the output as metadata.
    pub header: Vec<String>,
}

/// Round to a fixed number of decimals before formatting.
///
/// Floats are where reproducibility leaks: the same computation can print with a different tail on
/// a different platform, and a golden test would then fail for no reason anybody could see. Two
/// decimals is finer than any display can show and coarse enough to be stable.
pub fn n(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    // `-0` and `0` are the same place; printing two spellings of it would make byte-equality
    // depend on the sign of a rounding error.
    let r = if r == 0.0 { 0.0 } else { r };
    let s = format!("{r:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    if s.is_empty() || s == "-" {
        "0".into()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_print_short_and_stable() {
        assert_eq!(n(1.0), "1");
        assert_eq!(n(1.5), "1.5");
        assert_eq!(n(1.25), "1.25");
        assert_eq!(n(0.0), "0");
        assert_eq!(n(100.0), "100");
    }

    /// Two spellings of zero would make byte-equality depend on the sign of a rounding error.
    #[test]
    fn negative_zero_prints_as_zero() {
        assert_eq!(n(-0.0), "0");
        assert_eq!(n(-0.001), "0");
    }

    #[test]
    fn rounding_is_to_two_decimals() {
        assert_eq!(n(3.14159), "3.14");
        assert_eq!(n(2.999), "3");
        assert_eq!(n(-2.345), "-2.35");
        assert_eq!(n(123.456), "123.46");
    }

    /// The property the golden tests rest on: the same float prints the same way every time.
    #[test]
    fn the_same_number_always_prints_the_same_way() {
        let v = 1.0f64 / 3.0;
        assert_eq!(n(v), n(v));
        assert_eq!(n(v), "0.33");
    }
}
