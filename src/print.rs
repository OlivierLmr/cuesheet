//! A document, back out as text.
//!
//! Two callers want this: the round-trip test, which needs parse → print → parse to be a fixed
//! point, and cuelight, which writes documents from journals. Both need the same thing — that the
//! bytes depend on the document and nothing else, so a regenerated diagram diffs cleanly against
//! the one before it.

use crate::lex::write_word;
use crate::model::*;

/// Whether to pad the columns. Off by default: padding makes a file pleasant to read and makes a
/// diff churn every time the widest row changes, and a generated file is read by a diff far more
/// often than by a person.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Off,
    On,
}

pub fn document(doc: &Document, align: Align) -> String {
    let mut rows: Vec<Vec<String>> = Vec::new();
    for e in &doc.events {
        rows.push(event_cells(e));
    }

    // Column widths, when asked for. Only the first three columns are padded: beyond the kind the
    // content varies too much for alignment to help.
    let widths = if align == Align::On {
        let mut w = [0usize; 3];
        for r in &rows {
            for (i, cell) in r.iter().take(3).enumerate() {
                w[i] = w[i].max(cell.chars().count());
            }
        }
        Some(w)
    } else {
        None
    };

    let mut out = String::new();
    for h in &doc.header {
        out.push_str("# ");
        out.push_str(h);
        out.push('\n');
    }
    if !doc.header.is_empty() {
        out.push('\n');
    }
    if let Some(t) = &doc.title {
        out.push_str(&format!("title {}\n", write_word(t)));
    }
    if !doc.participants.is_empty() {
        out.push_str("participants");
        for p in &doc.participants {
            out.push(' ');
            out.push_str(&write_word(p));
        }
        out.push('\n');
    }
    if doc.title.is_some() || !doc.participants.is_empty() {
        out.push('\n');
    }

    for (row, e) in rows.iter().zip(&doc.events) {
        let mut line = String::new();
        for (i, cell) in row.iter().enumerate() {
            if i > 0 {
                line.push(' ');
            }
            match widths {
                Some(w) if i < 3 => {
                    // Right-align the time so a column of instants reads as a column.
                    if i == 0 {
                        line.push_str(&" ".repeat(w[0].saturating_sub(cell.chars().count())));
                        line.push_str(cell);
                    } else {
                        line.push_str(cell);
                        line.push_str(&" ".repeat(w[i].saturating_sub(cell.chars().count())));
                    }
                }
                _ => line.push_str(cell),
            }
        }
        out.push_str(line.trim_end());
        if let Some(d) = &e.detail {
            out.push_str(" {\n");
            out.push_str(d);
            out.push_str("\n}");
        }
        out.push('\n');
    }
    out
}

/// One event as cells: time, subject, kind-or-arrow, then everything else in a canonical order —
/// words, label, classes, instant. Canonical because a fixed point needs the second printing to
/// match the first, whatever order the author happened to write the marks in.
fn event_cells(e: &Event) -> Vec<String> {
    let mut cells = vec![e.time.to_string(), write_word(e.subject.name())];
    match &e.body {
        Body::Message { target, eaten } => {
            cells.push(if *eaten { "-x".into() } else { "->".into() });
            cells.push(write_word(target));
        }
        Body::Named(k) => cells.push(write_word(k)),
    }
    for w in &e.words {
        cells.push(write_word(w));
    }
    if let Some(l) = &e.label {
        cells.push(quoted(l));
    }
    for c in &e.classes {
        cells.push(format!(".{}", write_word(c)));
    }
    if let Some(i) = e.instant {
        cells.push(match i {
            Instant::After(d) => format!("+{d}"),
            Instant::At(t) => format!("@{t}"),
        });
    }
    cells
}

/// A label is always quoted, even when it would survive bare. Without that, a one-word label and a
/// bare word after the kind would print identically and mean different things on the way back in.
fn quoted(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn round(src: &str) -> String {
        document(&parse::document(src).unwrap(), Align::Off)
    }

    /// The property the whole format depends on: printing a parsed document and parsing it again
    /// gives the same document, so a regenerated file diffs against its predecessor cleanly.
    #[test]
    fn parse_print_parse_is_a_fixed_point() {
        let src = "\
title \"A relay that never happened\"
participants n0 n1 n2 n3

0 n0 asked do_broadcast
0 n0 deliver
0 n0 -> n1 .rb +10
40 n1 -x n2 .rb
50 network split n1 n3 +60
90 n0 -> n1 .rb .slow \"request, 2\" +30
100 n0 crash
110 n0 recover
200 n1 enter_cs +50
377 run end quiescent
";
        let once = round(src);
        let twice = round(&once);
        assert_eq!(once, twice, "printing should be idempotent");
        let a = parse::document(&once).unwrap();
        let b = parse::document(&twice).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn marks_print_in_a_canonical_order_whatever_order_they_were_written_in() {
        let a = round("90 n0 -> n1 +30 .slow \"hi\" .rb");
        let b = round("90 n0 -> n1 .slow .rb \"hi\" +30");
        assert_eq!(a, b);
        assert!(a.contains("90 n0 -> n1 \"hi\" .slow .rb +30"), "{a}");
    }

    #[test]
    fn a_detail_block_survives_the_round_trip_untouched() {
        let src = "0 n0 -> n1 .rb +10 {\n  {\"seq\": 4, \"n\": {\"a\": 1}}\n}\n";
        let out = round(src);
        assert!(out.contains("{\"seq\": 4, \"n\": {\"a\": 1}}"), "{out}");
        assert_eq!(round(&out), out);
    }

    #[test]
    fn a_value_is_quoted_exactly_when_the_bare_form_would_not_round_trip() {
        let out = round("0 n0 -> n1 .\"odd type\" +1");
        assert!(out.contains(".\"odd type\""), "{out}");
        let plain = round("0 n0 -> n1 .rb +1");
        assert!(plain.contains(".rb") && !plain.contains("\".rb\""), "{plain}");
    }

    /// A label prints quoted even when it needn't be, because a bare word after the kind means
    /// something else — a lane, on `network`.
    #[test]
    fn a_label_is_always_quoted() {
        let out = round("50 network split n1 n3 \"A\" +60");
        assert!(out.contains("50 network split n1 n3 \"A\" +60"), "{out}");
        let back = parse::document(&out).unwrap();
        assert_eq!(back.events[0].label.as_deref(), Some("A"));
        assert_eq!(back.events[0].lanes(), &["n1".to_string(), "n3".to_string()]);
    }

    #[test]
    fn the_provenance_header_survives() {
        let out = round("# drawn: seed 7\n0 n0 deliver\n");
        assert!(out.starts_with("# drawn: seed 7\n"), "{out}");
        assert_eq!(round(&out), out);
    }

    /// Alignment is for reading and changes nothing a parser sees.
    #[test]
    fn aligning_pads_columns_without_changing_the_document() {
        let doc = parse::document("0 n0 deliver\n1000 n11 crash").unwrap();
        let plain = document(&doc, Align::Off);
        let padded = document(&doc, Align::On);
        assert_ne!(plain, padded);
        assert!(padded.contains("   0 n0  deliver"), "{padded}");
        assert_eq!(parse::document(&plain).unwrap(), parse::document(&padded).unwrap());
    }
}
