//! The picture, as SVG.
//!
//! The only thing that actually draws. PNG is this rasterised and HTML is this wrapped, so
//! nothing about where anything sits is computed twice — which is what makes it safe to put one
//! in a handout and another on screen.
//!
//! Every number goes through [`n`], and nothing here iterates a hash map, so the same picture
//! produces the same bytes on every machine.

use crate::model::Color;
#[cfg(test)]
use crate::model::Dash;
use crate::picture::*;

/// Escape the five characters that would otherwise end an element or an attribute early.
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

fn stroke_attrs(s: &Stroke) -> String {
    let mut a = format!(
        " stroke=\"{}\" stroke-width=\"{}\"",
        s.color.hex(),
        n(s.width)
    );
    if let Some(d) = s.dash.array() {
        a.push_str(&format!(" stroke-dasharray=\"{d}\""));
    }
    if s.opacity < 1.0 {
        a.push_str(&format!(" stroke-opacity=\"{}\"", n(s.opacity)));
    }
    a
}

fn fill_attrs(f: &Option<Fill>) -> String {
    match f {
        None => " fill=\"none\"".to_string(),
        Some(f) => {
            let mut a = format!(" fill=\"{}\"", f.color.hex());
            if f.opacity < 1.0 {
                a.push_str(&format!(" fill-opacity=\"{}\"", n(f.opacity)));
            }
            a
        }
    }
}

/// Identity for the interactive page to hang behaviour on. Inert in a static file, and cheap
/// enough that emitting it always keeps the two outputs byte-identical in their geometry.
fn tag_attrs(t: &Tag, interactive: bool) -> String {
    if !interactive {
        return String::new();
    }
    let mut a = format!(" class=\"p {}\"", esc(&t.role));
    if let Some(e) = t.event {
        a.push_str(&format!(" data-event=\"{e}\""));
    }
    a
}

/// A colour that reads on the picture's own background, for the arrowhead marker. Markers cannot
/// inherit a stroke in every renderer that matters, so each distinct colour gets its own.
fn marker_id(c: Color) -> String {
    format!("h{}", c.hex().trim_start_matches('#'))
}

fn heads(prims: &[Prim]) -> Vec<Color> {
    let mut v: Vec<Color> = Vec::new();
    for p in prims {
        let (head, colour) = match p {
            Prim::Line { head, stroke, .. } => (*head, Some(stroke.color)),
            Prim::Path { head, stroke, .. } => (*head, stroke.as_ref().map(|s| s.color)),
            _ => (false, None),
        };
        if head {
            if let Some(c) = colour {
                if !v.contains(&c) {
                    v.push(c);
                }
            }
        }
    }
    // Sorted so the defs block does not depend on the order things happened to be drawn in.
    v.sort_by_key(|c| c.hex());
    v
}

pub struct Options {
    /// Emit ids and classes for the interactive page.
    pub interactive: bool,
    /// Wrap in `<svg>` with its own namespace, rather than expecting to be inlined.
    pub standalone: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { interactive: false, standalone: true }
    }
}

pub fn render(pic: &Picture, opt: &Options) -> String {
    let mut s = String::new();
    if opt.standalone {
        s.push_str(&format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" width=\"{}\" height=\"{}\" role=\"img\"",
            n(pic.width), n(pic.height), n(pic.width), n(pic.height)
        ));
    } else {
        s.push_str(&format!(
            "<svg viewBox=\"0 0 {} {}\" role=\"img\"",
            n(pic.width),
            n(pic.height)
        ));
    }
    s.push_str(">\n");

    // An accessible name, and the provenance the emitter wrote, so a figure in a handout can name
    // the run behind it.
    let title = pic.title.clone().unwrap_or_else(|| "space-time diagram".into());
    s.push_str(&format!("  <title>{}</title>\n", esc(&title)));
    if !pic.header.is_empty() {
        s.push_str(&format!("  <desc>{}</desc>\n", esc(&pic.header.join("\n"))));
    }

    let hs = heads(&pic.prims);
    if !hs.is_empty() {
        s.push_str("  <defs>\n");
        for c in &hs {
            s.push_str(&format!(
                "    <marker id=\"{}\" viewBox=\"0 0 10 10\" refX=\"9\" refY=\"5\" markerWidth=\"6\" markerHeight=\"6\" orient=\"auto-start-reverse\"><path d=\"M 0 1 L 9 5 L 0 9 z\" fill=\"{}\"/></marker>\n",
                marker_id(*c),
                c.hex()
            ));
        }
        s.push_str("  </defs>\n");
    }

    s.push_str(&format!(
        "  <rect x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" fill=\"{}\"/>\n",
        n(pic.width),
        n(pic.height),
        pic.background.hex()
    ));

    for p in &pic.prims {
        s.push_str("  ");
        s.push_str(&prim(p, opt.interactive));
        s.push('\n');
    }
    s.push_str("</svg>\n");
    s
}

fn head_attr(head: bool, stroke: Option<&Stroke>) -> String {
    match (head, stroke) {
        (true, Some(st)) => format!(" marker-end=\"url(#{})\"", marker_id(st.color)),
        _ => String::new(),
    }
}

fn prim(p: &Prim, inter: bool) -> String {
    match p {
        Prim::Line { x1, y1, x2, y2, stroke, head, tag } => format!(
            "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"{}{}{}/>",
            n(*x1),
            n(*y1),
            n(*x2),
            n(*y2),
            stroke_attrs(stroke),
            head_attr(*head, Some(stroke)),
            tag_attrs(tag, inter)
        ),
        Prim::Path { d, stroke, fill, head, tag } => format!(
            "<path d=\"{}\"{}{}{}{}/>",
            d,
            fill_attrs(fill),
            stroke.as_ref().map(stroke_attrs).unwrap_or_default(),
            head_attr(*head, stroke.as_ref()),
            tag_attrs(tag, inter)
        ),
        Prim::Rect { x, y, w, h, rx, fill, stroke, tag } => format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{}{}{}{}/>",
            n(*x),
            n(*y),
            n(*w),
            n(*h),
            if *rx > 0.0 { format!(" rx=\"{}\"", n(*rx)) } else { String::new() },
            fill_attrs(fill),
            stroke.as_ref().map(stroke_attrs).unwrap_or_default(),
            tag_attrs(tag, inter)
        ),
        Prim::Circle { cx, cy, r, fill, stroke, tag } => format!(
            "<circle cx=\"{}\" cy=\"{}\" r=\"{}\"{}{}{}/>",
            n(*cx),
            n(*cy),
            n(*r),
            fill_attrs(fill),
            stroke.as_ref().map(stroke_attrs).unwrap_or_default(),
            tag_attrs(tag, inter)
        ),
        Prim::Polygon { points, fill, stroke, tag } => {
            let pts: Vec<String> =
                points.iter().map(|(x, y)| format!("{},{}", n(*x), n(*y))).collect();
            format!(
                "<polygon points=\"{}\"{}{}{}/>",
                pts.join(" "),
                fill_attrs(fill),
                stroke.as_ref().map(stroke_attrs).unwrap_or_default(),
                tag_attrs(tag, inter)
            )
        }
        Prim::Text { x, y, text, size, anchor, fill, opacity, rotate, mono, tag } => {
            let a = match anchor {
                Anchor::Start => "start",
                Anchor::Middle => "middle",
                Anchor::End => "end",
            };
            // One face for everything, because the layout measured everything with one face. Asking
            // a viewer for a monospace family here would mean the text it draws is not the text the
            // layout made room for. Digits that must line up get tabular numerals instead, which is
            // a property of this face rather than a different one.
            let mut attrs = format!(
                "x=\"{}\" y=\"{}\" font-size=\"{}\" text-anchor=\"{}\" fill=\"{}\" font-family=\"Inter, ui-sans-serif, system-ui, sans-serif\"",
                n(*x),
                n(*y),
                n(*size),
                a,
                fill.hex()
            );
            if *mono {
                attrs.push_str(" font-variant-numeric=\"tabular-nums\"");
            }
            if *opacity < 1.0 {
                attrs.push_str(&format!(" fill-opacity=\"{}\"", n(*opacity)));
            }
            if rotate.abs() > 0.01 {
                attrs.push_str(&format!(
                    " transform=\"rotate({} {} {})\"",
                    n(*rotate),
                    n(*x),
                    n(*y)
                ));
            }
            format!("<text {}{}>{}</text>", attrs, tag_attrs(tag, inter), esc(text))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pic(prims: Vec<Prim>) -> Picture {
        Picture {
            width: 100.0,
            height: 50.0,
            background: Color(0xff, 0xff, 0xff),
            prims,
            title: Some("t".into()),
            header: vec![],
        }
    }

    fn stroke() -> Stroke {
        Stroke { color: Color(0, 0, 0), width: 1.5, dash: Dash::Solid, opacity: 1.0 }
    }

    #[test]
    fn the_five_dangerous_characters_are_escaped() {
        assert_eq!(esc("a<b>c&d\"e'f"), "a&lt;b&gt;c&amp;d&quot;e&apos;f");
    }

    /// A label holding a `<` would otherwise end the element early and produce a file no renderer
    /// could open.
    #[test]
    fn a_label_full_of_markup_stays_text() {
        let p = pic(vec![Prim::Text {
            x: 0.0,
            y: 0.0,
            text: "<script>alert(1)</script>".into(),
            size: 11.0,
            anchor: Anchor::Start,
            fill: Color(0, 0, 0),
            opacity: 1.0,
            rotate: 0.0,
            mono: false,
            tag: Tag::default(),
        }]);
        let out = render(&p, &Options::default());
        assert!(!out.contains("<script>"), "{out}");
        assert!(out.contains("&lt;script&gt;"));
    }

    #[test]
    fn a_solid_stroke_writes_no_dash_array() {
        let out = render(&pic(vec![Prim::Line {
            x1: 0.0,
            y1: 0.0,
            x2: 1.0,
            y2: 1.0,
            stroke: stroke(),
            head: false,
            tag: Tag::default(),
        }]), &Options::default());
        assert!(!out.contains("stroke-dasharray"), "{out}");
    }

    #[test]
    fn a_dashed_stroke_writes_its_pattern() {
        let mut s = stroke();
        s.dash = Dash::Pattern(3.0, 2.0);
        let out = render(&pic(vec![Prim::Line {
            x1: 0.0,
            y1: 0.0,
            x2: 1.0,
            y2: 1.0,
            stroke: s,
            head: false,
            tag: Tag::default(),
        }]), &Options::default());
        assert!(out.contains("stroke-dasharray=\"3 2\""), "{out}");
    }

    /// One marker per distinct head colour, and the defs block sorted, so the bytes do not depend
    /// on the order things happened to be drawn in.
    #[test]
    fn arrowhead_markers_are_one_per_colour_and_sorted() {
        let mut a = stroke();
        a.color = Color(0xff, 0, 0);
        let mut b = stroke();
        b.color = Color(0, 0, 0xff);
        let line = |s: Stroke| Prim::Line {
            x1: 0.0,
            y1: 0.0,
            x2: 1.0,
            y2: 1.0,
            stroke: s,
            head: true,
            tag: Tag::default(),
        };
        let out = render(&pic(vec![line(a.clone()), line(b.clone()), line(a)]), &Options::default());
        assert_eq!(out.matches("<marker").count(), 2, "{out}");
        let blue = out.find("id=\"h0000ff\"").unwrap();
        let red = out.find("id=\"hff0000\"").unwrap();
        assert!(blue < red, "markers should be sorted by colour");
    }

    #[test]
    fn a_static_file_carries_no_interactive_attributes() {
        let p = pic(vec![Prim::Line {
            x1: 0.0,
            y1: 0.0,
            x2: 1.0,
            y2: 1.0,
            stroke: stroke(),
            head: false,
            tag: Tag { event: Some(3), role: "arrow".into(), detail: None },
        }]);
        let plain = render(&p, &Options { interactive: false, standalone: true });
        assert!(!plain.contains("data-event"), "{plain}");
        let inter = render(&p, &Options { interactive: true, standalone: true });
        assert!(inter.contains("data-event=\"3\""), "{inter}");
    }

    /// The property PNG and HTML both rest on: turning interactivity on adds attributes and moves
    /// nothing.
    #[test]
    fn interactivity_changes_no_coordinate() {
        use crate::{derive, font, layout, parse};
        let doc = parse::document("0 n0 -> n1 .rb +10\n5 n1 deliver").unwrap();
        let sheet = parse::stylesheet(crate::DEFAULT_STYLE).unwrap();
        let run = derive::run(&doc, &sheet);
        let pic = layout::build(&doc, &sheet, &run, &font::Font::embedded());
        let coords = |s: &str| -> Vec<String> {
            s.split_whitespace()
                .filter(|w| w.starts_with("x1=") || w.starts_with("y1=") || w.starts_with("x=") || w.starts_with("y="))
                .map(String::from)
                .collect()
        };
        let a = render(&pic, &Options { interactive: false, standalone: true });
        let b = render(&pic, &Options { interactive: true, standalone: true });
        assert_eq!(coords(&a), coords(&b));
    }

    #[test]
    fn an_accessible_name_and_the_provenance_are_written() {
        let mut p = pic(vec![]);
        p.header = vec!["drawn: seed 7".into()];
        let out = render(&p, &Options::default());
        assert!(out.contains("role=\"img\""));
        assert!(out.contains("<title>t</title>"));
        assert!(out.contains("<desc>drawn: seed 7</desc>"), "{out}");
    }

    #[test]
    fn the_background_is_painted_rather_than_left_to_the_viewer() {
        let out = render(&pic(vec![]), &Options::default());
        assert!(out.contains("fill=\"#ffffff\""), "{out}");
    }
}
