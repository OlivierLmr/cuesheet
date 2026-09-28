//! The same SVG, wrapped.
//!
//! Not a second renderer. The page inlines exactly the bytes [`crate::svg`] produced — with a
//! class and a data attribute per element — and adds a hundred lines of JavaScript. Every
//! coordinate comes from the one layout pass, so a screenshot of this and the exported SVG are the
//! same picture, which is what makes it safe to put one in a handout and the other on screen.

use crate::model::Theme;
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

pub fn render(pic: &Picture, theme: Theme) -> String {
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
})();"#;

    format!(
        "<!doctype html>\n<html lang=\"en\">\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{title}</title>\n<style>\n{css}</style>\n\
         <main>\n  <div class=\"plot\">\n{body}  </div>\n</main>\n\
         <div id=\"legend\"></div>\n\
         <div id=\"panel\" role=\"status\" aria-live=\"polite\"></div>\n\
         {data}<script>\n{script}\n</script>\n</html>\n"
    )
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

    fn page(src: &str) -> String {
        render(&build(src), Theme::Light)
    }

    /// The guarantee the whole split rests on: the page holds the same SVG, not another drawing.
    #[test]
    fn the_page_inlines_the_same_geometry_the_static_file_has() {
        let pic = build("0 n0 -> n1 .rb +10\n5 n1 deliver\n20 run end");
        let stat = svg::render(&pic, &svg::Options::default());
        let page = render(&pic, Theme::Light);

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
        let pic = build("0 n0 deliver");
        assert!(render(&pic, Theme::Light).contains("background: #fafafb"));
        assert!(render(&pic, Theme::Dark).contains("background: #0e1116"));
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

    #[test]
    fn the_page_is_self_contained_with_no_external_requests() {
        let out = page("0 n0 -> n1 .rb +10");
        assert!(!out.contains("src=\"http"), "{out}");
        assert!(!out.contains("href=\"http"), "{out}");
        assert!(!out.contains("@import"));
    }
}
