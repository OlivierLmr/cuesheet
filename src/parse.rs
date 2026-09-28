//! Documents and style sheets, from text.
//!
//! Two shapes, and the event has two spellings:
//!
//! ```text
//! kind    category  name  [closing-name]  [kills|revives]
//! style   selector  key=value…
//! event   time  subject  kind  [args…]  [marked…]  [{]
//! event   time  subject  ->    target   [marked…]  [{]
//! ```
//!
//! A diagnostic names a line and a column, because a language people write by hand is only as good
//! as what it says when they get it wrong.

use crate::lex::{self, Span, Token};
use crate::model::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    pub message: String,
    pub span: Span,
}

impl Error {
    fn at(span: Span, message: impl Into<String>) -> Error {
        Error { message: message.into(), span }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.span.line, self.span.col, self.message)
    }
}

impl From<lex::LexError> for Error {
    fn from(e: lex::LexError) -> Error {
        Error { message: e.message, span: e.span }
    }
}

/// Split the source into logical lines, folding each detail block onto the line that opened it.
///
/// The block's contents are opaque: this finds the first line whose only content is `}` and never
/// looks inside, which is what lets a pretty-printed JSON body full of braces need no escaping.
fn fold_details(src: &str) -> Result<Vec<(usize, String, Option<String>)>, Error> {
    let raw: Vec<&str> = src.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let line_no = i + 1;
        let text = raw[i];
        let opens = lex::line(text, line_no)?.last().map(|l| l.token == Token::OpenDetail).unwrap_or(false);
        if !opens {
            out.push((line_no, text.to_string(), None));
            i += 1;
            continue;
        }
        let mut body = Vec::new();
        let mut j = i + 1;
        loop {
            if j >= raw.len() {
                return Err(Error::at(
                    Span { line: line_no, col: 1 },
                    "detail block is never closed; a line whose only content is `}` ends it",
                ));
            }
            if raw[j].trim() == "}" {
                break;
            }
            body.push(raw[j]);
            j += 1;
        }
        out.push((line_no, text.to_string(), Some(body.join("\n"))));
        i = j + 1;
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Document
// ---------------------------------------------------------------------------

pub fn document(src: &str) -> Result<Document, Error> {
    let mut doc = Document::default();
    let mut seen_statement = false;

    for (line_no, text, detail) in fold_details(src)? {
        // Leading comments are provenance an emitter may have written. Keep them so a round trip
        // does not quietly throw away which run a figure came from.
        if !seen_statement && text.trim_start().starts_with('#') {
            doc.header.push(text.trim_start().trim_start_matches('#').trim().to_string());
            continue;
        }
        let toks = lex::line(&text, line_no)?;
        if toks.is_empty() {
            continue;
        }
        seen_statement = true;
        let here = toks[0].span;

        // Declarations come first and carry no time.
        if let Token::Word(w) = &toks[0].token {
            match w.as_str() {
                "title" => {
                    let t = match toks.get(1).map(|l| &l.token) {
                        Some(Token::Quoted(s)) => s.clone(),
                        Some(Token::Word(s)) => s.clone(),
                        _ => return Err(Error::at(here, "title takes one quoted string")),
                    };
                    doc.title = Some(t);
                    continue;
                }
                "participants" => {
                    if toks.len() < 2 {
                        return Err(Error::at(here, "participants takes one or more node names"));
                    }
                    for l in &toks[1..] {
                        match &l.token {
                            Token::Word(n) => doc.participants.push(n.clone()),
                            _ => return Err(Error::at(l.span, "a participant is a bare node name")),
                        }
                    }
                    continue;
                }
                _ => {}
            }
        }

        doc.events.push(event(&toks, detail, line_no)?);
    }

    // Any node that speaks but was never declared still gets a lane, in first-seen order, so a
    // document that omits `participants` renders rather than failing.
    let mut known = doc.participants.clone();
    let note = |n: &str, known: &mut Vec<String>| {
        if !n.is_empty() && !known.iter().any(|k| k == n) {
            known.push(n.to_string());
        }
    };
    for e in &doc.events {
        if let Subject::Node(n) = &e.subject {
            note(n, &mut known);
        }
        if let Body::Message { target, .. } = &e.body {
            note(target, &mut known);
        }
        for lane in e.lanes() {
            note(lane, &mut known);
        }
    }
    doc.participants = known;
    Ok(doc)
}

fn event(toks: &[lex::Lexed], detail: Option<String>, line_no: usize) -> Result<Event, Error> {
    let here = toks[0].span;
    let time: Tick = match &toks[0].token {
        Token::Word(w) => w.parse().map_err(|_| {
            Error::at(here, format!("`{w}` is not a time; every event starts with an absolute time"))
        })?,
        _ => return Err(Error::at(here, "every event starts with an absolute time")),
    };

    let subj_tok = toks.get(1).ok_or_else(|| Error::at(here, "expected a subject after the time"))?;
    let subject = match &subj_tok.token {
        Token::Word(w) => Subject::parse(w),
        _ => return Err(Error::at(subj_tok.span, "a subject is a node name, `network` or `run`")),
    };

    let third = toks.get(2).ok_or_else(|| {
        Error::at(subj_tok.span, "expected a kind or an arrow after the subject")
    })?;

    let mut idx = 3;
    let body = match &third.token {
        Token::Arrow { eaten } => {
            let t = toks.get(3).ok_or_else(|| Error::at(third.span, "an arrow needs a target"))?;
            let target = match &t.token {
                Token::Word(w) => w.clone(),
                _ => return Err(Error::at(t.span, "an arrow's target is a bare node name")),
            };
            idx = 4;
            Body::Message { target, eaten: *eaten }
        }
        Token::Word(w) => Body::Named(w.clone()),
        _ => return Err(Error::at(third.span, "expected a kind or an arrow after the subject")),
    };

    let mut ev = Event {
        time,
        subject,
        body,
        words: vec![],
        label: None,
        classes: vec![],
        instant: None,
        detail,
        line: line_no,
    };

    for l in &toks[idx.min(toks.len())..] {
        match &l.token {
            Token::Word(w) => ev.words.push(w.clone()),
            Token::Quoted(s) => {
                if ev.label.is_some() {
                    return Err(Error::at(l.span, "two labels on one line"));
                }
                ev.label = Some(s.clone());
            }
            Token::Class(c) => ev.classes.push(c.clone()),
            Token::After(d) => set_instant(&mut ev, Instant::After(*d), l.span)?,
            Token::At(t) => set_instant(&mut ev, Instant::At(*t), l.span)?,
            Token::OpenDetail => {}
            Token::Arrow { .. } => return Err(Error::at(l.span, "only one arrow to a line")),
        }
    }

    // `->` promises an arrival and `-x` forbids one. The redundancy is deliberate: without it a
    // forgotten `+10` would silently become a lost message rather than an error.
    if let Body::Message { eaten, .. } = &ev.body {
        match (*eaten, ev.instant) {
            (false, None) => {
                return Err(Error::at(
                    here,
                    "`->` needs an arrival: `+d` after this line, or `@t` absolute. \
                     Write `-x` for a message the network ate",
                ))
            }
            (true, Some(_)) => {
                return Err(Error::at(here, "`-x` never arrives, so it takes no instant"))
            }
            _ => {}
        }
    }

    Ok(ev)
}

fn set_instant(ev: &mut Event, i: Instant, span: Span) -> Result<(), Error> {
    if ev.instant.is_some() {
        return Err(Error::at(span, "two instants on one line"));
    }
    ev.instant = Some(i);
    Ok(())
}

// ---------------------------------------------------------------------------
// Style sheet
// ---------------------------------------------------------------------------

pub fn stylesheet(src: &str) -> Result<StyleSheet, Error> {
    let mut sheet = StyleSheet::default();
    let mut order = 0usize;

    for (line_no, text, _) in fold_details(src)? {
        let toks = lex::line(&text, line_no)?;
        if toks.is_empty() {
            continue;
        }
        let here = toks[0].span;
        let head = match &toks[0].token {
            Token::Word(w) => w.as_str(),
            _ => return Err(Error::at(here, "a style sheet holds `kind` and `style` lines")),
        };
        match head {
            "kind" => {
                let decl = kind_decl(&toks)?;
                sheet.kinds.insert(decl.name.clone(), decl);
            }
            "style" => {
                let (selector, props, diagram) = style_rule(&toks)?;
                if let Some(d) = diagram {
                    sheet.diagram = d(sheet.diagram.clone());
                } else {
                    sheet.rules.push(Rule { selector, props, order });
                    order += 1;
                }
            }
            other => {
                return Err(Error::at(
                    here,
                    format!("`{other}` is not a style-sheet statement; expected `kind` or `style`"),
                ))
            }
        }
    }
    Ok(sheet)
}

fn kind_decl(toks: &[lex::Lexed]) -> Result<KindDecl, Error> {
    let here = toks[0].span;
    let word = |i: usize| -> Option<&String> {
        match toks.get(i).map(|l| &l.token) {
            Some(Token::Word(w)) => Some(w),
            _ => None,
        }
    };
    let cat = match word(1).map(String::as_str) {
        Some("point") => Category::Point,
        Some("span") => Category::Span,
        Some(other) => {
            return Err(Error::at(
                toks[1].span,
                format!("`{other}` is not a category; there are two, `point` and `span`"),
            ))
        }
        None => return Err(Error::at(here, "kind takes a category, then a name")),
    };
    let name = word(2)
        .ok_or_else(|| Error::at(here, "kind takes a category, then a name"))?
        .clone();

    let mut decl = KindDecl { category: cat, name, closes: None, kills: false, revives: false };
    for l in &toks[3..] {
        let w = match &l.token {
            Token::Word(w) => w,
            _ => return Err(Error::at(l.span, "a kind declaration takes bare words only")),
        };
        match w.as_str() {
            "kills" => decl.kills = true,
            "revives" => decl.revives = true,
            other => {
                if decl.category != Category::Span {
                    return Err(Error::at(
                        l.span,
                        format!("`{other}` closes a span, but `{}` is a point", decl.name),
                    ));
                }
                if decl.closes.is_some() {
                    return Err(Error::at(l.span, "a span is closed by one event, not two"));
                }
                decl.closes = Some(other.to_string());
            }
        }
    }
    // A thing that lasts cannot also be the instant something ends.
    if decl.category == Category::Span && (decl.kills || decl.revives) {
        return Err(Error::at(here, "`kills` and `revives` are meaningful only on a point"));
    }
    if decl.kills && decl.revives {
        return Err(Error::at(here, "one event cannot both end and resume participation"));
    }
    Ok(decl)
}

type DiagramPatch = Box<dyn Fn(Diagram) -> Diagram>;

fn style_rule(toks: &[lex::Lexed]) -> Result<(Selector, Props, Option<DiagramPatch>), Error> {
    let here = toks[0].span;
    let sel_tok = toks.get(1).ok_or_else(|| Error::at(here, "style takes a selector"))?;

    // The selector is one token, lexed as a word possibly followed by classes and states. Because
    // `.` and `:` are both in the bare-word set, `arrow.slow:died` arrives as a single word and is
    // split here rather than in the lexer, where it would have to know about selectors.
    let raw = match &sel_tok.token {
        Token::Word(w) => w.clone(),
        Token::Class(c) => format!(".{c}"),
        _ => return Err(Error::at(sel_tok.span, "a selector is a name, `.class` and `:state` parts")),
    };
    let selector = parse_selector(&raw, sel_tok.span)?;

    // `key=value` arrives as one word, because `=` and `#` are both bare. The one shape that does
    // not is `label="a b"`, where the lexer stops the word at the quote — so a `key=` with nothing
    // after it takes the quoted token that follows.
    let mut pairs: Vec<(String, String, Span)> = Vec::new();
    let mut i = 2;
    while i < toks.len() {
        let l = &toks[i];
        let w = match &l.token {
            Token::Word(w) => w,
            _ => return Err(Error::at(l.span, "a style property is written `key=value`")),
        };
        let (k, v) = w
            .split_once('=')
            .ok_or_else(|| Error::at(l.span, format!("`{w}` is not a `key=value` pair")))?;
        if v.is_empty() {
            match toks.get(i + 1).map(|l| &l.token) {
                Some(Token::Quoted(q)) => {
                    pairs.push((k.to_string(), q.clone(), l.span));
                    i += 2;
                    continue;
                }
                _ => return Err(Error::at(l.span, format!("`{k}=` has no value"))),
            }
        }
        pairs.push((k.to_string(), v.to_string(), l.span));
        i += 1;
    }

    if selector.name.as_deref() == Some("diagram") {
        let patch = diagram_patch(pairs)?;
        return Ok((selector, Props::default(), Some(patch)));
    }

    let mut props = Props::default();
    for (k, v, span) in pairs {
        apply_prop(&mut props, &k, &v, span)?;
    }
    Ok((selector, props, None))
}

fn parse_selector(raw: &str, span: Span) -> Result<Selector, Error> {
    let mut sel = Selector::default();
    let mut buf = String::new();
    let mut mode = 0u8; // 0 name, 1 class, 2 state
    let flush = |sel: &mut Selector, buf: &mut String, mode: u8| {
        if buf.is_empty() {
            return;
        }
        match mode {
            0 => sel.name = Some(std::mem::take(buf)),
            1 => sel.classes.push(std::mem::take(buf)),
            _ => sel.states.push(std::mem::take(buf)),
        }
    };
    for c in raw.chars() {
        match c {
            '.' => {
                flush(&mut sel, &mut buf, mode);
                mode = 1;
            }
            ':' => {
                flush(&mut sel, &mut buf, mode);
                mode = 2;
            }
            c => buf.push(c),
        }
    }
    flush(&mut sel, &mut buf, mode);
    if sel.specificity() == 0 {
        return Err(Error::at(span, "a selector needs at least one part"));
    }
    Ok(sel)
}

fn diagram_patch(pairs: Vec<(String, String, Span)>) -> Result<DiagramPatch, Error> {
    let mut ops: Vec<Box<dyn Fn(&mut Diagram)>> = Vec::new();
    for (k, v, span) in pairs {
        let bad = |what: &str| Error::at(span, format!("`{v}` is not {what}"));
        match k.as_str() {
            "stretch" => {
                let n: f64 = v.parse().map_err(|_| bad("a number of pixels per tick"))?;
                ops.push(Box::new(move |d| d.stretch = n));
            }
            "axis" => {
                let a = match v.as_str() {
                    "linear" => Axis::Linear,
                    "compressed" => Axis::Compressed,
                    "ordinal" => Axis::Ordinal,
                    _ => return Err(bad("an axis mode; there are three, `linear`, `compressed`, `ordinal`")),
                };
                ops.push(Box::new(move |d| d.axis = a));
            }
            "gap" => {
                let n: Tick = v.parse().map_err(|_| bad("a number of ticks"))?;
                ops.push(Box::new(move |d| d.gap = n));
            }
            "lane-pitch" => {
                let n: f64 = v.parse().map_err(|_| bad("a number of pixels"))?;
                ops.push(Box::new(move |d| d.lane_pitch = n));
            }
            "row-pitch" => {
                let n: f64 = v.parse().map_err(|_| bad("a number of pixels"))?;
                ops.push(Box::new(move |d| d.row_pitch = n));
            }
            "theme" => {
                let t = match v.as_str() {
                    "light" => Theme::Light,
                    "dark" => Theme::Dark,
                    _ => return Err(bad("a theme; there are two, `light` and `dark`")),
                };
                ops.push(Box::new(move |d| d.theme = t));
            }
            "bodies" => {
                let b = match v.as_str() {
                    "on" => true,
                    "off" => false,
                    _ => return Err(bad("`on` or `off`")),
                };
                ops.push(Box::new(move |d| d.bodies = b));
            }
            other => {
                return Err(Error::at(
                    span,
                    format!(
                        "`{other}` is not a diagram property; there are six: \
                         stretch, axis, gap, lane-pitch, row-pitch, theme, bodies"
                    ),
                ))
            }
        }
    }
    Ok(Box::new(move |mut d: Diagram| {
        for op in &ops {
            op(&mut d);
        }
        d
    }))
}

fn apply_prop(p: &mut Props, k: &str, v: &str, span: Span) -> Result<(), Error> {
    let bad = |what: &str| Error::at(span, format!("`{v}` is not {what}"));
    match k {
        "color" => p.color = Some(Color::parse(v).ok_or_else(|| bad("a `#rrggbb` colour"))?),
        "fill" => p.fill = Some(Color::parse(v).ok_or_else(|| bad("a `#rrggbb` colour"))?),
        "width" => p.width = Some(v.parse().map_err(|_| bad("a number of pixels"))?),
        "size" => p.size = Some(v.parse().map_err(|_| bad("a number of pixels"))?),
        "radius" => p.radius = Some(v.parse().map_err(|_| bad("a number of pixels"))?),
        "opacity" => {
            let n: f64 = v.parse().map_err(|_| bad("a number between 0 and 1"))?;
            if !(0.0..=1.0).contains(&n) {
                return Err(bad("a number between 0 and 1"));
            }
            p.opacity = Some(n);
        }
        "dash" => p.dash = Some(Dash::parse(v).ok_or_else(|| {
            bad("a dash; `solid`, `dashed`, `dotted`, or a raw `ink,gap`")
        })?),
        "mark" => {
            p.mark = Some(Mark::parse(v).ok_or_else(|| {
                bad(&format!("a mark; there are eight: {}", Mark::ALL.join(", ")))
            })?)
        }
        "edge" => p.edge = Some(Edge::parse(v).ok_or_else(|| bad("an edge; `flat` or `ragged`"))?),
        "label" => p.label = Some(v.to_string()),
        other => {
            return Err(Error::at(
                span,
                format!(
                    "`{other}` is not a style property; there are ten: \
                     color, width, dash, opacity, fill, mark, size, label, edge, radius"
                ),
            ))
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(src: &str) -> Document {
        document(src).unwrap_or_else(|e| panic!("should parse, got {e}"))
    }
    fn doc_err(src: &str) -> Error {
        document(src).unwrap_err()
    }
    fn sheet(src: &str) -> StyleSheet {
        stylesheet(src).unwrap_or_else(|e| panic!("should parse, got {e}"))
    }
    fn sheet_err(src: &str) -> Error {
        stylesheet(src).unwrap_err()
    }

    // ---- declarations -----------------------------------------------------

    #[test]
    fn a_title_may_be_quoted_or_bare() {
        assert_eq!(doc("title \"A relay\"").title, Some("A relay".into()));
        assert_eq!(doc("title relay").title, Some("relay".into()));
    }

    #[test]
    fn participants_fix_the_lane_order() {
        assert_eq!(doc("participants n2 n0 n1").participants, vec!["n2", "n0", "n1"]);
    }

    /// A process that never speaks still gets a lane — an empty lifeline is how a reader sees that
    /// somebody was left out — and one that speaks without being declared gets one too.
    #[test]
    fn undeclared_nodes_still_get_lanes_in_first_seen_order() {
        let d = doc("participants n0 n1\n0 n5 -> n9 .x +1");
        assert_eq!(d.participants, vec!["n0", "n1", "n5", "n9"]);
    }

    // ---- the two event spellings -----------------------------------------

    #[test]
    fn an_arrow_carries_a_target_and_an_arrival() {
        let e = &doc("0 n0 -> n1 .rb +10").events[0];
        assert_eq!(e.time, 0);
        assert_eq!(e.subject, Subject::Node("n0".into()));
        assert_eq!(e.body, Body::Message { target: "n1".into(), eaten: false });
        assert_eq!(e.classes, vec!["rb".to_string()]);
        assert_eq!(e.instant, Some(Instant::After(10)));
    }

    #[test]
    fn a_named_kind_takes_the_subject_it_is_written_on() {
        let d = doc("100 n0 crash\n50 network split n1 n3 +60\n377 run end quiescent");
        assert_eq!(d.events[0].body, Body::Named("crash".into()));
        assert_eq!(d.events[1].subject, Subject::Network);
        assert_eq!(d.events[1].lanes(), &["n1".to_string(), "n3".to_string()]);
        assert_eq!(d.events[2].subject, Subject::Run);
        assert_eq!(d.events[2].stated_label(), Some("quiescent".into()));
    }

    /// The marks are what make order after the positional tokens meaningless.
    #[test]
    fn marked_tokens_parse_the_same_in_any_order() {
        let a = &doc("90 n0 -> n1 .rb \"hi\" +30 .slow").events[0];
        let b = &doc("90 n0 -> n1 +30 .slow \"hi\" .rb").events[0];
        assert_eq!(a.instant, b.instant);
        assert_eq!(a.label, b.label);
        let (mut ac, mut bc) = (a.classes.clone(), b.classes.clone());
        ac.sort();
        bc.sort();
        assert_eq!(ac, bc);
    }

    #[test]
    fn a_self_addressed_message_is_ordinary_to_the_parser() {
        let e = &doc("100 n0 -> n0 .retry +20").events[0];
        assert_eq!(e.body, Body::Message { target: "n0".into(), eaten: false });
    }

    // ---- the arrow / instant contract ------------------------------------

    /// The redundancy is deliberate: without it a forgotten `+10` would silently become a lost
    /// message rather than an error.
    #[test]
    fn an_arrow_without_an_arrival_is_an_error_not_a_loss() {
        let e = doc_err("0 n0 -> n1 .rb");
        assert!(e.message.contains("needs an arrival"), "{}", e.message);
        assert!(e.message.contains("-x"), "the message should name the alternative: {}", e.message);
    }

    #[test]
    fn an_eaten_message_takes_no_instant() {
        assert!(doc("20 n1 -x n2 .rb").events[0].instant.is_none());
        assert!(doc_err("20 n1 -x n2 .rb +5").message.contains("never arrives"));
    }

    // ---- detail blocks ----------------------------------------------------

    #[test]
    fn a_detail_block_is_opaque_and_may_hold_braces() {
        let d = doc("90 n0 -> n1 .rb +30 {\n  {\"seq\": 4, \"n\": {\"a\": 1}}\n}");
        assert_eq!(d.events[0].detail.as_deref(), Some("  {\"seq\": 4, \"n\": {\"a\": 1}}"));
    }

    #[test]
    fn a_detail_block_spans_many_lines_and_the_event_after_it_still_parses() {
        let d = doc("0 n0 -> n1 .a +1 {\n  one\n  two\n}\n5 n1 deliver");
        assert_eq!(d.events.len(), 2);
        assert_eq!(d.events[0].detail.as_deref(), Some("  one\n  two"));
        assert_eq!(d.events[1].body, Body::Named("deliver".into()));
        assert_eq!(d.events[1].line, 5);
    }

    #[test]
    fn an_unclosed_detail_block_is_an_error() {
        assert!(doc_err("0 n0 -> n1 .a +1 {\n  forever").message.contains("never closed"));
    }

    // ---- provenance -------------------------------------------------------

    #[test]
    fn leading_comments_are_kept_as_a_header() {
        let d = doc("# drawn: seed 7\n# replay: cuelight run\n0 n0 deliver");
        assert_eq!(d.header, vec!["drawn: seed 7", "replay: cuelight run"]);
        assert_eq!(d.events.len(), 1);
    }

    // ---- malformed documents, one per rule -------------------------------

    #[test]
    fn every_malformed_event_names_its_line_and_column() {
        let cases: &[(&str, &str)] = &[
            ("nope n0 deliver", "not a time"),
            ("0", "expected a subject"),
            ("0 n0", "expected a kind or an arrow"),
            ("0 n0 ->", "arrow needs a target"),
            ("0 n0 -> n1 +1 +2", "two instants"),
            ("0 n0 deliver \"a\" \"b\"", "two labels"),
        ];
        for (src, want) in cases {
            let e = doc_err(src);
            assert!(e.message.contains(want), "{src:?} gave {:?}", e.message);
            assert_eq!(e.span.line, 1, "{src:?}");
            assert!(e.span.col >= 1);
        }
    }

    #[test]
    fn a_diagnostic_points_at_the_line_it_happened_on() {
        let e = doc_err("0 n0 deliver\n\n5 n1 deliver\nnope n2 deliver");
        assert_eq!(e.span.line, 4);
    }

    // ---- style sheet: kinds ----------------------------------------------

    #[test]
    fn a_kind_declares_a_category_and_optionally_how_it_closes_or_what_it_does() {
        let s = sheet(
            "kind point deliver\n\
             kind point crash kills\n\
             kind point recover revives\n\
             kind span enter_cs exit_cs\n\
             kind span paused",
        );
        assert_eq!(s.kinds["deliver"].category, Category::Point);
        assert!(s.kinds["crash"].kills);
        assert!(s.kinds["recover"].revives);
        assert_eq!(s.kinds["enter_cs"].closes.as_deref(), Some("exit_cs"));
        assert!(s.kinds["paused"].closes.is_none());
        assert_eq!(s.closed_by("enter_cs"), Some("exit_cs"));
        assert!(s.is_closer("exit_cs"));
        assert!(!s.is_closer("deliver"));
    }

    /// A thing that lasts cannot also be the instant something ends.
    #[test]
    fn kills_and_revives_are_meaningful_only_on_a_point() {
        assert!(sheet_err("kind span oops kills").message.contains("only on a point"));
        assert!(sheet_err("kind point oops kills revives").message.contains("cannot both"));
    }

    #[test]
    fn a_point_cannot_be_given_a_closing_name() {
        assert!(sheet_err("kind point deliver exit").message.contains("is a point"));
    }

    #[test]
    fn there_are_exactly_two_categories() {
        let e = sheet_err("kind region split");
        assert!(e.message.contains("not a category"), "{}", e.message);
        assert!(e.message.contains("point") && e.message.contains("span"));
    }

    // ---- style sheet: rules ----------------------------------------------

    #[test]
    fn a_selector_accumulates_classes_and_states_in_any_number() {
        let s = sheet("style arrow.slow.retry:died color=#cc79a7");
        let sel = &s.rules[0].selector;
        assert_eq!(sel.name.as_deref(), Some("arrow"));
        assert_eq!(sel.classes, vec!["slow".to_string(), "retry".to_string()]);
        assert_eq!(sel.states, vec!["died".to_string()]);
        assert_eq!(sel.specificity(), 4);
    }

    #[test]
    fn a_bare_class_selector_needs_no_name() {
        let s = sheet("style .rb color=#0072b2");
        assert_eq!(s.rules[0].selector.name, None);
        assert_eq!(s.rules[0].selector.classes, vec!["rb".to_string()]);
    }

    #[test]
    fn a_quoted_property_value_survives_the_lexer_splitting_it() {
        let s = sheet("style .rb label=\"RB round 2\" color=#0072b2");
        assert_eq!(s.rules[0].props.label.as_deref(), Some("RB round 2"));
        assert_eq!(s.rules[0].props.color, Color::parse("#0072b2"));
    }

    #[test]
    fn every_style_property_parses_and_an_unknown_one_lists_them() {
        let s = sheet(
            "style x color=#0072b2 width=2 dash=dashed opacity=0.5 fill=#f0f0f0 \
             mark=cross size=5 edge=ragged radius=4",
        );
        let p = &s.rules[0].props;
        assert_eq!(p.color, Color::parse("#0072b2"));
        assert_eq!(p.width, Some(2.0));
        assert_eq!(p.dash, Some(Dash::Pattern(4.0, 3.0)));
        assert_eq!(p.opacity, Some(0.5));
        assert_eq!(p.fill, Color::parse("#f0f0f0"));
        assert_eq!(p.mark, Some(Mark::Cross));
        assert_eq!(p.size, Some(5.0));
        assert_eq!(p.edge, Some(Edge::Ragged));
        assert_eq!(p.radius, Some(4.0));

        let e = sheet_err("style x colour=#000");
        assert!(e.message.contains("not a style property"), "{}", e.message);
        assert!(e.message.contains("radius"), "the message should list the set: {}", e.message);
    }

    #[test]
    fn a_bad_property_value_says_what_was_wanted() {
        assert!(sheet_err("style x color=blue").message.contains("#rrggbb"));
        assert!(sheet_err("style x mark=blob").message.contains("there are eight"));
        assert!(sheet_err("style x dash=wobbly").message.contains("solid"));
        assert!(sheet_err("style x opacity=2").message.contains("between 0 and 1"));
        assert!(sheet_err("style x edge=fuzzy").message.contains("flat"));
        assert!(sheet_err("style x width=thick").message.contains("pixels"));
    }

    #[test]
    fn every_diagram_property_parses_and_an_unknown_one_lists_them() {
        let s = sheet(
            "style diagram stretch=2.0 axis=compressed gap=300 lane-pitch=160 \
             row-pitch=30 theme=dark bodies=on",
        );
        assert_eq!(s.diagram.stretch, 2.0);
        assert_eq!(s.diagram.axis, Axis::Compressed);
        assert_eq!(s.diagram.gap, 300);
        assert_eq!(s.diagram.lane_pitch, 160.0);
        assert_eq!(s.diagram.row_pitch, 30.0);
        assert_eq!(s.diagram.theme, Theme::Dark);
        assert!(s.diagram.bodies);

        let e = sheet_err("style diagram wobble=3");
        assert!(e.message.contains("not a diagram property"), "{}", e.message);
    }

    #[test]
    fn the_three_axis_modes_are_the_only_ones() {
        for m in ["linear", "compressed", "ordinal"] {
            assert!(stylesheet(&format!("style diagram axis={m}")).is_ok(), "{m}");
        }
        assert!(sheet_err("style diagram axis=sideways").message.contains("three"));
    }

    #[test]
    fn diagram_settings_accumulate_across_lines() {
        let s = sheet("style diagram stretch=2.0\nstyle diagram axis=ordinal");
        assert_eq!(s.diagram.stretch, 2.0);
        assert_eq!(s.diagram.axis, Axis::Ordinal);
    }

    #[test]
    fn a_style_sheet_holds_nothing_but_kind_and_style() {
        assert!(sheet_err("set stretch 2").message.contains("not a style-sheet statement"));
    }

    // ---- the cascade ------------------------------------------------------

    /// Specificity first, source order to break ties, and only the properties each rule sets.
    #[test]
    fn the_cascade_resolves_by_specificity_then_order_and_merges_properties() {
        let s = sheet(
            "style arrow color=#14171c\n\
             style .rb label=\"RB\" color=#0072b2\n\
             style arrow.slow color=#e69f00 width=2\n\
             style arrow:died color=#cc79a7",
        );
        let classes = vec!["rb".to_string(), "slow".to_string()];

        // Two parts beats one; `label` survives from the one-part rule because rules merge.
        let p = s.resolve(Some("arrow"), &classes, &[]);
        assert_eq!(p.color, Color::parse("#e69f00"));
        assert_eq!(p.width, Some(2.0));
        assert_eq!(p.label.as_deref(), Some("RB"));

        // `arrow:died` is also two parts and written later, so it takes the colour back.
        let p = s.resolve(Some("arrow"), &classes, &["died".to_string()]);
        assert_eq!(p.color, Color::parse("#cc79a7"));
        assert_eq!(p.width, Some(2.0), "width still comes from arrow.slow");
        assert_eq!(p.label.as_deref(), Some("RB"));
    }

    #[test]
    fn a_rule_only_applies_when_every_one_of_its_parts_matches() {
        let s = sheet("style arrow.slow.retry color=#e69f00");
        assert!(s.resolve(Some("arrow"), &["slow".into()], &[]).color.is_none());
        assert!(s
            .resolve(Some("arrow"), &["slow".into(), "retry".into()], &[])
            .color
            .is_some());
    }

    #[test]
    fn a_selector_naming_a_kind_does_not_reach_a_different_kind() {
        let s = sheet("style crash mark=cross");
        assert!(s.resolve(Some("deliver"), &[], &[]).mark.is_none());
        assert_eq!(s.resolve(Some("crash"), &[], &[]).mark, Some(Mark::Cross));
    }

    /// A style sheet has no business naming `n0`, so the two cases of a split are told apart by
    /// which sort of subject they landed on.
    #[test]
    fn subject_pseudo_classes_separate_a_network_span_from_a_node_one() {
        let s = sheet("style split:network fill=#f0f0f0\nstyle split:node fill=#cc79a7");
        assert_eq!(s.resolve(Some("split"), &[], &["network".into()]).fill, Color::parse("#f0f0f0"));
        assert_eq!(s.resolve(Some("split"), &[], &["node".into()]).fill, Color::parse("#cc79a7"));
    }
}
