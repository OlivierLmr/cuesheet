//! cuesheet — draws a distributed run as a space-time diagram.
//!
//! A message is a segment from the instant it left to the instant it landed, so its slope is its
//! flight time. The pipeline runs one way only — parse → derive → layout → picture → emit — and
//! every coordinate is decided in `layout`, so the emitters cannot disagree about where anything is.
//!
//! ```no_run
//! let doc = cuesheet::parse::document("0 n0 -> n1 .rb +10").unwrap();
//! let svg = cuesheet::render(&doc, None).unwrap();
//! ```

pub mod derive;
pub mod font;
pub mod html;
pub mod layout;
pub mod lex;
pub mod model;
pub mod parse;
pub mod picture;
pub mod print;
pub mod svg;

#[cfg(feature = "png")]
pub mod png;

/// The default style sheet, compiled in.
///
/// Loaded first, always, with any sheet of the caller's laid over it. That is what lets a document
/// render at all when nobody has styled it — an undeclared kind comes out as a dot carrying its own
/// name — and what makes overriding one row a one-line change rather than a rewrite.
pub const DEFAULT_STYLE: &str = include_str!("../assets/default.sts");

/// Parse the default sheet, then the caller's on top.
pub fn stylesheet(extra: Option<&str>) -> Result<model::StyleSheet, parse::Error> {
    let mut sheet = parse::stylesheet(DEFAULT_STYLE)?;
    if let Some(src) = extra {
        let theirs = parse::stylesheet(src)?;
        sheet.kinds.extend(theirs.kinds);
        let base = sheet.rules.len();
        for mut r in theirs.rules {
            // Later in the file wins a tie, and the caller's file is later than the default one.
            r.order += base;
            sheet.rules.push(r);
        }
        sheet.diagram = theirs.diagram;
    }
    Ok(sheet)
}

/// Document plus optional style sheet, to SVG. The short road, for callers who want a picture.
pub fn render(doc: &model::Document, style: Option<&str>) -> Result<String, parse::Error> {
    let sheet = stylesheet(style)?;
    let run = derive::run(doc, &sheet);
    let pic = layout::build(doc, &sheet, &run, &font::Font::embedded());
    Ok(svg::render(&pic, &svg::Options::default()))
}
