//! The same SVG, wrapped.
//!
//! Not a second renderer. The page inlines exactly the bytes [`crate::svg`] produced — with a
//! class and a data attribute per element — and adds a hundred lines of JavaScript. Every
//! coordinate comes from the one layout pass, so a screenshot of this and the exported SVG are the
//! same picture, which is what makes it safe to put one in a handout and the other on screen.

use crate::derive::{Fate, Run};
use crate::model::{Subject, Theme};
use crate::picture::{Picture, Prim};
use crate::svg::{self, esc};

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
                .map(|s| format!("{{\"e\":{},\"lane\":{},\"t\":{}}}", s.event, lane(&s.subject), s.start)),
        )
        .collect();
    format!("{{\"messages\":[{}],\"points\":[{}]}}", messages.join(","), points.join(","))
}

pub fn render(pic: &Picture, theme: Theme, run: &Run) -> String {
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
         \x20 body {{ margin: 0; background: {bg}; color: {fg}; font: 14px/1.5 Inter, ui-sans-serif, system-ui, sans-serif; }}\n\
         \x20 main {{ padding: 20px; }}\n\
         \x20 .plot {{ overflow: auto; }}\n\
         \x20 svg {{ max-width: 100%; height: auto; display: block; }}\n\
         \x20 .p[data-event] {{ cursor: pointer; }}\n\
         \x20 .dim {{ opacity: .15; }}\n\
         \x20 .off {{ opacity: .07; }}\n\
         \x20 .p.participant {{ cursor: pointer; }}\n\
         \x20 .p.participant.on {{ text-decoration: underline; }}\n\
         \x20 #following {{ padding: 0 20px 12px; font: 12px ui-monospace, SFMono-Regular, Menlo, monospace; opacity: .7; }}\n\
         \x20 #panel {{ position: fixed; right: 16px; bottom: 16px; max-width: min(46ch, 90vw); max-height: 50vh; overflow: auto; background: {panel}; border: 1px solid {rule}; border-radius: 4px; padding: 10px 12px; font: 12px/1.45 ui-monospace, SFMono-Regular, Menlo, monospace; white-space: pre-wrap; display: none; }}\n\
         \x20 #panel.on {{ display: block; }}\n\
         \x20 #panel b {{ display: block; margin-bottom: 6px; opacity: .6; font-weight: 400; }}\n\
         \x20 #legend {{ display: flex; flex-wrap: wrap; gap: 6px 14px; padding: 0 20px 16px; font: 12px ui-monospace, SFMono-Regular, Menlo, monospace; }}\n\
         \x20 #legend button {{ font: inherit; color: inherit; background: none; cursor: pointer; border: 1px solid {rule}; border-radius: 3px; padding: 2px 8px; }}\n\
         \x20 #legend button[aria-pressed=\"false\"] {{ opacity: .4; text-decoration: line-through; }}\n\
         \x20 #legend button:focus-visible {{ outline: 2px solid currentColor; outline-offset: 2px; }}\n"
    );

    let script = r#"(() => {
  const svg = document.querySelector('svg');
  const panel = document.getElementById('panel');
  const legend = document.getElementById('legend');
  const roleOf = el => (el.getAttribute('class') || '').split(' ')[1] || '';

  // Detail blocks, as written. Pretty-printed when they happen to be JSON, verbatim when not:
  // the format never promised they were anything in particular.
  const details = new Map();
  for (const s of document.querySelectorAll('script.detail')) {
    let text = s.textContent;
    try { text = JSON.stringify(JSON.parse(text), null, 2); } catch (_) {}
    details.set(s.dataset.event, text);
  }

  const show = (ev, role) => {
    const text = details.get(ev);
    if (!text) return;
    panel.textContent = '';
    const b = document.createElement('b');
    b.textContent = role || ('event ' + ev);
    panel.append(b, document.createTextNode(text));
    panel.classList.add('on');
  };
  const hide = () => panel.classList.remove('on');

  for (const el of svg.querySelectorAll('[data-event]')) {
    el.addEventListener('mouseenter', () => show(el.dataset.event, roleOf(el)));
    el.addEventListener('mouseleave', hide);
  }
  svg.addEventListener('mouseleave', hide);

  // One toggle per kind, so a dense plot can have a whole kind taken out of it. The names are the
  // ones the style sheet selects by, so what you learn in one transfers to the other.
  const skip = ['lifeline', 'gridline', 'tick', 'participant', 'elapsed', ''];
  const roles = [...new Set([...svg.querySelectorAll('.p')].map(roleOf))]
    .filter(r => !skip.includes(r)).sort();

  for (const role of roles) {
    const b = document.createElement('button');
    b.textContent = role;
    b.setAttribute('aria-pressed', 'true');
    b.addEventListener('click', () => {
      const on = b.getAttribute('aria-pressed') === 'true';
      b.setAttribute('aria-pressed', String(!on));
      for (const el of svg.querySelectorAll('.p')) {
        if (roleOf(el) === role) el.classList.toggle('dim', on);
      }
    });
    legend.append(b);
  }

  // Following one thing through the run. A click on a process keeps what involves it. A click on
  // an event keeps what it depends on and what depends on it — what happened before it and what
  // happened after it, in the sense of the partial order a run has — and fades what is concurrent
  // with it. The facts come from the derivation, not from the picture.
  const run = JSON.parse(document.getElementById('run').textContent);
  const following = document.getElementById('following');
  const marked = [...svg.querySelectorAll('[data-event]')];
  const labels = [...svg.querySelectorAll('.p.participant')];
  const idle = 'click a process to keep what involves it, or an event to keep its causes and consequences';
  let chosen = null;

  const paint = (keep, text) => {
    for (const el of marked) el.classList.toggle('off', keep !== null && !keep.has(+el.dataset.event));
    following.textContent = text;
  };
  const clear = () => {
    chosen = null;
    for (const l of labels) l.classList.remove('on');
    paint(null, idle);
  };

  const involving = name => {
    const keep = new Set();
    for (const m of run.messages) if (m.from === name || m.to === name) keep.add(m.e);
    // What belongs to no process — the network, the run itself — is never faded: it bears on all.
    for (const p of run.points) if (p.lane === name || p.lane === null) keep.add(p.e);
    return keep;
  };

  const around = ev => {
    // `since`: per process, the instant from which it has been reached by the event.
    // `until`: per process, the instant up to which it contributed to the event.
    const since = new Map(), until = new Map();
    const lower = (l, t) => (!since.has(l) || t < since.get(l)) && (since.set(l, t), true);
    const raise = (l, t) => (!until.has(l) || t > until.get(l)) && (until.set(l, t), true);

    const msg = run.messages.find(m => m.e === ev);
    const pt = run.points.find(p => p.e === ev);
    if (msg) {
      // A message is caused by what its sender had seen when it left, and causes what its
      // receiver does from the moment it lands — if it lands.
      if (msg.left) raise(msg.from, msg.depart);
      if (msg.landed) lower(msg.to, msg.arrive);
    } else if (pt && pt.lane !== null) {
      raise(pt.lane, pt.t);
      lower(pt.lane, pt.t);
    } else {
      return null;
    }
    // A message that landed carries its sender's past forward and its receiver's future back.
    for (let moved = true; moved;) {
      moved = false;
      for (const m of run.messages) {
        if (m.landed && since.has(m.from) && m.depart >= since.get(m.from)) moved = lower(m.to, m.arrive) || moved;
        if (m.landed && until.has(m.to) && m.arrive <= until.get(m.to)) moved = raise(m.from, m.depart) || moved;
      }
    }
    const keep = new Set([ev]);
    for (const m of run.messages) {
      if (m.left && since.has(m.from) && m.depart >= since.get(m.from)) keep.add(m.e);
      if (m.landed && until.has(m.to) && m.arrive <= until.get(m.to)) keep.add(m.e);
    }
    for (const p of run.points) {
      // What belongs to no process is never faded, here either.
      if (p.lane === null) { keep.add(p.e); continue; }
      if (since.has(p.lane) && p.t >= since.get(p.lane)) keep.add(p.e);
      if (until.has(p.lane) && p.t <= until.get(p.lane)) keep.add(p.e);
    }
    return keep;
  };

  for (const l of labels) {
    l.addEventListener('click', e => {
      e.stopPropagation();
      const name = l.textContent.trim();
      if (chosen === 'lane:' + name) return clear();
      clear();
      chosen = 'lane:' + name;
      l.classList.add('on');
      paint(involving(name), 'what involves ' + name + ' — click it again to clear');
    });
  }
  for (const el of marked) {
    el.addEventListener('click', e => {
      e.stopPropagation();
      const ev = +el.dataset.event;
      if (chosen === 'event:' + ev) return clear();
      const keep = around(ev);
      if (keep === null) return;
      clear();
      chosen = 'event:' + ev;
      paint(keep, 'causes and consequences of the ' + (roleOf(el) || 'event') + ' you clicked — click it again to clear');
    });
  }
  svg.addEventListener('click', clear);
  document.addEventListener('keydown', e => { if (e.key === 'Escape') clear(); });
  clear();
})();"#;

    format!(
        "<!doctype html>\n<html lang=\"en\">\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{title}</title>\n<style>\n{css}</style>\n\
         <main>\n  <div class=\"plot\">\n{body}  </div>\n</main>\n\
         <div id=\"following\"></div>\n\
         <div id=\"legend\"></div>\n\
         <div id=\"panel\" role=\"status\" aria-live=\"polite\"></div>\n\
         {data}<script type=\"application/json\" id=\"run\">{facts}</script>\n\
         <script>\n{script}\n</script>\n</html>\n",
        facts = facts(run)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{derive, font, layout, parse};

    fn build(src: &str) -> (Picture, Run) {
        let doc = parse::document(src).unwrap();
        let sheet = crate::stylesheet(None).unwrap();
        let run = derive::run(&doc, &sheet);
        (layout::build(&doc, &sheet, &run, &font::Font::embedded()), run)
    }

    fn page(src: &str) -> String {
        let (pic, run) = build(src);
        render(&pic, Theme::Light, &run)
    }

    /// The guarantee the whole split rests on: the page holds the same SVG, not another drawing.
    #[test]
    fn the_page_inlines_the_same_geometry_the_static_file_has() {
        let (pic, run) = build("0 n0 -> n1 .rb +10\n5 n1 deliver\n20 run end");
        let stat = svg::render(&pic, &svg::Options::default());
        let page = render(&pic, Theme::Light, &run);

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
        assert!(render(&pic, Theme::Light, &run).contains("background: #fafafb"));
        assert!(render(&pic, Theme::Dark, &run).contains("background: #0e1116"));
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
        let out = render(&pic, Theme::Light, &run);
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

    #[test]
    fn the_page_is_self_contained_with_no_external_requests() {
        let out = page("0 n0 -> n1 .rb +10");
        assert!(!out.contains("src=\"http"), "{out}");
        assert!(!out.contains("href=\"http"), "{out}");
        assert!(!out.contains("@import"));
    }
}
