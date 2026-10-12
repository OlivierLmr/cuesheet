//! The same SVG, wrapped.
//!
//! Not a second renderer. The page inlines exactly the bytes [`crate::svg`] produced — with a
//! class and a data attribute per element — and adds a hundred lines of JavaScript. Every
//! coordinate comes from the one layout pass, so a screenshot of this and the exported SVG are the
//! same picture, which is what makes it safe to put one in a handout and the other on screen.

use crate::derive::{Fate, Run};
use crate::model::{Axis, Subject, Theme};
use crate::picture::{Picture, Prim};
use crate::svg::{self, esc};

/// What the page does, once it is open.
const PAGE: &str = include_str!("../assets/page.js");

/// The layout, compiled for the browser and compressed: `wasm/`, built by `tools/build-wasm.sh`.
/// Committed, so that building cuesheet needs no WebAssembly toolchain; `tests/wasm.rs` fails
/// when it no longer matches the sources it was built from.
const LAYOUT: &[u8] = include_bytes!("../assets/cuesheet.wasm.gz");

/// What the page needs to lay the run out again itself: the two texts the picture was made from,
/// and which axis it was drawn with, so the page can say which one is showing.
pub struct Source<'a> {
    pub doc: &'a str,
    pub style: &'a str,
    pub axis: Axis,
}

/// Bytes as text, for a page that is one file: standard base64, padded.
fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(ABC[(n >> 18) as usize & 63] as char);
        out.push(ABC[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { ABC[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { ABC[n as usize & 63] as char } else { '=' });
    }
    out
}

/// Detail blocks, keyed by the event they belong to, for the hover panel.
fn details(pic: &Picture) -> Vec<(usize, String)> {
    let mut v: Vec<(usize, String)> = Vec::new();
    for p in &pic.prims {
        let tag = match p {
            Prim::Line { tag, .. }
            | Prim::Path { tag, .. }
            | Prim::Rect { tag, .. }
            | Prim::Circle { tag, .. }
            | Prim::Polygon { tag, .. }
            | Prim::Text { tag, .. } => tag,
        };
        if let (Some(e), Some(d)) = (tag.event, tag.detail.as_ref()) {
            if !v.iter().any(|(k, _)| *k == e) {
                v.push((e, d.clone()));
            }
        }
    }
    v.sort_by_key(|(e, _)| *e);
    v
}

/// A string as JSON, safe inside a `<script>` element: `<` is written as an escape, so no name can
/// close the element it sits in.
fn js(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '<' => out.push_str("\\u003c"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// What happened, as the page needs it to follow one thing through the run: who each event
/// belongs to and when, and for a message where it left, where it landed, and whether it did.
///
/// Facts the derivation already established, handed over rather than recomputed: the page must
/// not come to its own conclusion about whether a message arrived.
fn facts(run: &Run) -> String {
    let lane = |s: &Subject| match s {
        Subject::Node(n) => js(n),
        _ => "null".into(),
    };
    let messages: Vec<String> = run
        .messages
        .iter()
        .map(|m| {
            format!(
                "{{\"e\":{},\"from\":{},\"to\":{},\"depart\":{},\"arrive\":{},\"left\":{},\"landed\":{}}}",
                m.event,
                js(&m.from),
                js(&m.to),
                m.depart,
                m.arrive,
                m.fate != Fate::NeverLeft,
                m.fate == Fate::Arrived
            )
        })
        .collect();
    let points: Vec<String> = run
        .points
        .iter()
        .map(|p| format!("{{\"e\":{},\"lane\":{},\"t\":{}}}", p.event, lane(&p.subject), p.at))
        .chain(
            run.spans
                .iter()
                .map(|s| format!("{{\"e\":{},\"lane\":{},\"t\":{},\"lasts\":true}}", s.event, lane(&s.subject), s.start)),
        )
        .collect();
    format!("{{\"messages\":[{}],\"points\":[{}]}}", messages.join(","), points.join(","))
}

pub fn render(pic: &Picture, theme: Theme, run: &Run, source: &Source) -> String {
    let body = svg::render(pic, &svg::Options { interactive: true, standalone: false });
    let title = esc(&pic.title.clone().unwrap_or_else(|| "cuesheet".into()));

    let (scheme, bg, fg, panel, rule) = match theme {
        Theme::Light => ("light", "#fafafb", "#14171c", "#ffffff", "#dbdfe6"),
        Theme::Dark => ("dark", "#0e1116", "#e7e9ed", "#171b22", "#272d37"),
    };

    let mut data = String::new();
    for (e, d) in details(pic) {
        data.push_str(&format!(
            "<script type=\"application/json\" class=\"detail\" data-event=\"{e}\">{}</script>\n",
            esc(d.trim())
        ));
    }

    let css = format!(
        "  :root {{ color-scheme: {scheme}; }}\n\
         \x20 html {{ overflow-anchor: none; }}\n\
         \x20 body {{ margin: 0; background: {bg}; color: {fg}; font: 14px/1.5 Inter, ui-sans-serif, system-ui, sans-serif; }}\n\
         \x20 main {{ padding: 20px 20px calc(var(--bar, 64px) + 20px); }}\n\
         \x20 .plot {{ overflow: auto; }}\n\
         \x20 svg {{ max-width: 100%; height: auto; display: block; }}\n\
         \x20 .p[data-event] {{ cursor: pointer; }}\n\
         \x20 .dim {{ opacity: .15; }}\n\
         \x20 .off {{ opacity: .07; }}\n\
         \x20 .p.participant {{ cursor: pointer; }}\n\
         \x20 .p.participant.on {{ text-decoration: underline; }}\n\
         \x20 #bar {{ position: fixed; left: 0; right: 0; bottom: 0; z-index: 2; display: flex; align-items: center; gap: 8px 20px; flex-wrap: wrap; padding: 8px 20px; background: {panel}; border-top: 1px solid {rule}; font: 12px ui-monospace, SFMono-Regular, Menlo, monospace; }}\n\
         \x20 #scale {{ display: flex; align-items: center; gap: 8px; white-space: nowrap; }}\n\
         \x20 #scale input {{ width: 160px; }}\n\
         \x20 #scale output {{ min-width: 5ch; text-align: right; }}\n\
         \x20 #following {{ flex: 1 0 100%; opacity: .7; }}\n\
         \x20 #panel {{ position: fixed; right: 16px; bottom: calc(var(--bar, 64px) + 12px); z-index: 3; max-width: min(46ch, 90vw); max-height: 50vh; overflow: auto; background: {panel}; border: 1px solid {rule}; border-radius: 4px; padding: 10px 12px; font: 12px/1.45 ui-monospace, SFMono-Regular, Menlo, monospace; white-space: pre-wrap; display: none; }}\n\
         \x20 #panel.on {{ display: block; }}\n\
         \x20 #panel b {{ display: block; margin-bottom: 6px; opacity: .6; font-weight: 400; }}\n\
         \x20 #legend {{ display: flex; flex-wrap: wrap; gap: 6px 14px; flex: 1 1 0; }}\n\
         \x20 #bar button {{ font: inherit; color: inherit; background: none; cursor: pointer; border: 1px solid {rule}; border-radius: 3px; padding: 2px 8px; }}\n\
         \x20 #legend button[aria-pressed=\"false\"] {{ opacity: .4; text-decoration: line-through; }}\n\
         \x20 #bar button:focus-visible {{ outline: 2px solid currentColor; outline-offset: 2px; }}\n\
         \x20 #axis {{ display: flex; align-items: center; gap: 4px; margin-left: auto; white-space: nowrap; }}\n\
         \x20 #axis button[aria-pressed=\"false\"] {{ opacity: .75; }}\n\
         \x20 #axis button[aria-pressed=\"true\"] {{ border-color: currentColor; }}\n\
         \x20 #axis button:disabled {{ cursor: default; }}\n\
         \x20 #axis button:disabled[aria-pressed=\"false\"] {{ opacity: .25; }}\n\
         \x20 #busy {{ opacity: 0; }}\n\
         \x20 #bar[data-busy] #busy {{ opacity: .7; transition: opacity .1s .25s; }}\n"
    );

    let script = PAGE.trim_end();

    format!(
        "<!doctype html>\n<html lang=\"en\">\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{title}</title>\n<style>\n{css}</style>\n\
         <main>\n  <div class=\"plot\">\n{body}  </div>\n</main>\n\
         <footer id=\"bar\">\n\
         \x20 <div id=\"following\"></div>\n\
         \x20 <div id=\"legend\"></div>\n\
         \x20 <span id=\"axis\" role=\"group\" aria-label=\"time axis\">\
         <span id=\"busy\" role=\"status\">laying out…</span>\
         <button data-axis=\"0\" disabled title=\"an instant takes as much room as it lasted: the slope of an arrow is its flight time\">linear</button>\
         <button data-axis=\"1\" disabled title=\"linear, with each long quiet stretch folded into a marked break\">compressed</button>\
         <button data-axis=\"2\" disabled title=\"one row per distinct instant: order only, no durations\">ordinal</button></span>\n\
         \x20 <label id=\"scale\" title=\"stretch or squeeze time; double-click to reset\">time <input type=\"range\" min=\"-4\" max=\"3\" step=\"0.05\" value=\"0\"><output>×1.00</output></label>\n\
         </footer>\n\
         <div id=\"panel\" role=\"status\" aria-live=\"polite\"></div>\n\
         {data}<script type=\"application/json\" id=\"run\">{facts}</script>\n\
         <script type=\"application/json\" id=\"source\">{{\"doc\":{doc},\"style\":{style},\"axis\":{axis}}}</script>\n\
         <script type=\"application/octet-stream\" id=\"layout\">{layout}</script>\n\
         <script>\n{script}\n</script>\n</html>\n",
        facts = facts(run),
        doc = js(source.doc),
        style = js(source.style),
        axis = match source.axis {
            Axis::Linear => 0,
            Axis::Compressed => 1,
            Axis::Ordinal => 2,
        },
        layout = base64(LAYOUT)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{derive, font, layout, parse};

    /// For the tests that are about something else than laying out again.
    const NO_SOURCE: Source = Source { doc: "", style: "", axis: Axis::Linear };

    fn build(src: &str) -> (Picture, Run) {
        let doc = parse::document(src).unwrap();
        let sheet = crate::stylesheet(None).unwrap();
        let run = derive::run(&doc, &sheet);
        (layout::build(&doc, &sheet, &run, &font::Font::embedded()), run)
    }

    fn page(src: &str) -> String {
        let (pic, run) = build(src);
        render(&pic, Theme::Light, &run, &NO_SOURCE)
    }

    /// The guarantee the whole split rests on: the page holds the same SVG, not another drawing.
    #[test]
    fn the_page_inlines_the_same_geometry_the_static_file_has() {
        let (pic, run) = build("0 n0 -> n1 .rb +10\n5 n1 deliver\n20 run end");
        let stat = svg::render(&pic, &svg::Options::default());
        let page = render(&pic, Theme::Light, &run, &NO_SOURCE);

        let coords: Vec<&str> = stat
            .split_whitespace()
            .filter(|w| {
                w.starts_with("x1=") || w.starts_with("y1=") || w.starts_with("x2=") || w.starts_with("y2=")
            })
            .collect();
        assert!(!coords.is_empty());
        for c in coords {
            assert!(page.contains(c), "page is missing {c}");
        }
    }

    #[test]
    fn a_detail_block_reaches_the_page_for_the_hover_panel() {
        let out = page("0 n0 -> n1 .rb +10 {\n  {\"seq\": 4}\n}");
        assert!(out.contains("class=\"detail\""), "{out}");
        assert!(out.contains("&quot;seq&quot;: 4"), "{out}");
    }

    #[test]
    fn a_document_with_no_details_still_produces_a_page() {
        let out = page("0 n0 -> n1 .rb +10");
        assert!(out.contains("<svg"));
        assert!(!out.contains("class=\"detail\""));
    }

    /// The page paints its own ground rather than borrowing the viewer's.
    #[test]
    fn both_themes_paint_a_background() {
        let (pic, run) = build("0 n0 deliver");
        assert!(render(&pic, Theme::Light, &run, &NO_SOURCE).contains("background: #fafafb"));
        assert!(render(&pic, Theme::Dark, &run, &NO_SOURCE).contains("background: #0e1116"));
    }

    #[test]
    fn detail_text_is_escaped_so_a_body_cannot_close_the_script_element() {
        let out = page("0 n0 -> n1 .rb +10 {\n  </script><script>alert(1)</script>\n}");
        assert!(!out.contains("<script>alert(1)"), "{out}");
        assert!(out.contains("&lt;/script&gt;"), "{out}");
    }

    #[test]
    fn interactive_elements_carry_the_event_they_came_from() {
        assert!(page("0 n0 -> n1 .rb +10").contains("data-event=\"0\""));
    }

    /// The page follows an event through the run from facts the derivation established: where
    /// each message left and landed, and whether it did. It must not work that out from pixels.
    #[test]
    fn the_page_is_handed_what_happened_so_it_can_follow_an_event() {
        let out = page("participants n0 n1\n0 n0 -> n1 .rb +10\n5 n1 deliver\n20 run end");
        assert!(out.contains(r#"{"e":0,"from":"n0","to":"n1","depart":0,"arrive":10,"left":true,"landed":true}"#), "{out}");
        assert!(out.contains(r#"{"e":1,"lane":"n1","t":5}"#), "{out}");
        // And it carries the control that stretches time.
        assert!(out.contains("id=\"scale\""), "{out}");
        // The end of the run belongs to no process.
        assert!(out.contains(r#""lane":null"#), "{out}");
    }

    /// A message its sender never got out is said so, and one that landed nowhere too: neither
    /// carries anything forward.
    #[test]
    fn a_message_that_did_not_arrive_is_not_handed_over_as_one_that_did() {
        let sheet = "kind point crash kills";
        let doc = parse::document("participants n0 n1\n5 n1 crash\n0 n0 -> n1 +10\n20 run end").unwrap();
        let sheet = crate::stylesheet(Some(sheet)).unwrap();
        let run = derive::run(&doc, &sheet);
        let pic = layout::build(&doc, &sheet, &run, &font::Font::embedded());
        let out = render(&pic, Theme::Light, &run, &NO_SOURCE);
        assert!(out.contains(r#""left":true,"landed":false"#), "{out}");
    }

    /// A participant's name goes into a script element; it must not be able to close it.
    #[test]
    fn a_name_cannot_close_the_element_that_carries_the_facts() {
        assert_eq!(js("a</script>\"b"), r#""a\u003c/script>\"b""#);
    }

    /// A browser shows an SVG's `<title>` as a tooltip over the whole drawing, on top of the event
    /// the pointer is reading. The page names the picture where nothing is displayed instead.
    #[test]
    fn the_page_puts_no_tooltip_over_the_drawing() {
        let out = page("title \"a run\"\n0 n0 -> n1 .rb +10");
        let drawing = &out[out.find("<svg").unwrap()..out.find("</svg>").unwrap()];
        assert!(!drawing.contains("<title>"), "{drawing}");
        assert!(drawing.contains("aria-label=\"a run\""), "{drawing}");
    }

    /// The page lays the run out again itself, so it must hold what the first layout was given:
    /// the document, the caller's sheet, the axis showing, and the layout to run them through.
    #[test]
    fn the_page_carries_what_it_takes_to_lay_the_run_out_again() {
        let src = "0 n0 -> n1 .rb +10 \"a</script>\"";
        let (pic, run) = build(src);
        let out = render(
            &pic,
            Theme::Light,
            &run,
            &Source { doc: src, style: "style diagram axis=ordinal", axis: Axis::Ordinal },
        );
        assert!(out.contains(r#""style":"style diagram axis=ordinal","axis":2}"#), "no sheet or axis");
        // The document is there as written, and cannot close the element that carries it.
        assert!(out.contains(r#"{"doc":"0 n0 -> n1 .rb +10 \"a\u003c/script>\"","#), "no document");
        // The compiled layout, as text: a gzip stream starts with 1f 8b, which base64 writes `H4`.
        assert!(out.contains("id=\"layout\">H4"), "no layout");
        for axis in ["linear", "compressed", "ordinal"] {
            assert!(out.contains(&format!(">{axis}</button>")), "no {axis} button");
        }
    }

    #[test]
    fn base64_pads_what_does_not_fill_three_bytes() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn the_page_is_self_contained_with_no_external_requests() {
        let out = page("0 n0 -> n1 .rb +10");
        assert!(!out.contains("src=\"http"), "{out}");
        assert!(!out.contains("href=\"http"), "{out}");
        assert!(!out.contains("@import"));
    }
}
