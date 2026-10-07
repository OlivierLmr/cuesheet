//! The properties the plan asks each milestone to hold, checked against the three real runs.
//!
//! These are the tests that would catch a regression nobody was looking for: reproducibility,
//! stability under a one-tick change, and whether the overlap pass is doing anything at all.

use cuesheet::model::Theme;
use cuesheet::picture::{Anchor, Picture, Prim};
use cuesheet::{derive, font, layout, parse, svg};

const RUNS: [&str; 4] = [
    include_str!("../examples/relais-manquant.cuesheet"),
    include_str!("../examples/showcase.cuesheet"),
    include_str!("../examples/partition.cuesheet"),
    include_str!("../examples/mutex-egalites.cuesheet"),
];
const NAMES: [&str; 4] = ["relais-manquant", "showcase", "partition", "mutex-egalites"];
const STYLE: &str = include_str!("../examples/cuesheet.cuestyle");

fn picture(src: &str, style: Option<&str>, relax: bool) -> Picture {
    let doc = parse::document(src).expect("example should parse");
    let sheet = cuesheet::stylesheet(style).expect("style should parse");
    let run = derive::run(&doc, &sheet);
    layout::build_with(&doc, &sheet, &run, &font::Font::embedded(), relax)
}

fn render(src: &str) -> String {
    svg::render(&picture(src, Some(STYLE), true), &svg::Options::default())
}

/// A label's box, as it will print.
fn boxes(pic: &Picture) -> Vec<(f64, f64, f64, f64, String)> {
    let f = font::Font::embedded();
    pic.prims
        .iter()
        .filter_map(|p| match p {
            Prim::Text { x, y, text, size, anchor, rotate, .. } => {
                // A rotated label occupies a box along its own angle; approximating it by the
                // upright one is close enough for an overlap count and does not flatter the
                // relaxation, since it over-reports rather than under-reports.
                let w = f.width(text, *size) * rotate.to_radians().cos().abs().max(0.35);
                let cx = match anchor {
                    Anchor::Start => x + w * 0.5,
                    Anchor::Middle => *x,
                    Anchor::End => x - w * 0.5,
                };
                Some((cx, y - size * 0.35, w, size * 1.1, text.clone()))
            }
            _ => None,
        })
        .collect()
}

fn overlapping_pairs(pic: &Picture) -> usize {
    let bs = boxes(pic);
    let mut n = 0;
    for i in 0..bs.len() {
        for j in (i + 1)..bs.len() {
            let ox = (bs[i].2 + bs[j].2) * 0.5 - (bs[i].0 - bs[j].0).abs();
            let oy = (bs[i].3 + bs[j].3) * 0.5 - (bs[i].1 - bs[j].1).abs();
            if ox > 0.5 && oy > 0.5 {
                n += 1;
            }
        }
    }
    n
}

/// Labels sitting on a lifeline — the commonest collision on a space-time diagram, because the
/// midpoint of a long arrow lands on a lane.
fn labels_on_lanes(pic: &Picture) -> usize {
    let lanes: Vec<f64> = pic
        .prims
        .iter()
        .filter_map(|p| match p {
            Prim::Line { x1, x2, y1, y2, .. } if (x1 - x2).abs() < 0.01 && (y2 - y1).abs() > 40.0 => Some(*x1),
            _ => None,
        })
        .collect();
    boxes(pic)
        .iter()
        .filter(|(cx, _, w, _, _)| lanes.iter().any(|x| (cx - x).abs() < w * 0.5))
        .count()
}

// ---------------------------------------------------------------------------
// Reproducibility — asserted from the first milestone rather than added at the end
// ---------------------------------------------------------------------------

#[test]
fn the_same_document_renders_to_the_same_bytes() {
    for (src, name) in RUNS.iter().zip(NAMES) {
        assert_eq!(render(src), render(src), "{name} is not reproducible");
    }
}

/// Nothing in the pipeline may iterate a hash map, read the clock, or depend on an address. If any
/// of that crept in, rendering the same document from two separately parsed copies would differ.
#[test]
fn two_separately_parsed_copies_render_identically() {
    for (src, name) in RUNS.iter().zip(NAMES) {
        let a = render(src);
        let b = render(&format!("{src}\n"));
        assert_eq!(a, b, "{name} depends on something outside the document");
    }
}

#[test]
fn every_number_in_the_output_is_finite() {
    for (src, name) in RUNS.iter().zip(NAMES) {
        let out = render(src);
        assert!(!out.contains("NaN"), "{name} produced NaN");
        assert!(!out.contains("inf"), "{name} produced an infinity");
    }
}

// ---------------------------------------------------------------------------
// The overlap pass — each class, and proof the pass is doing something
// ---------------------------------------------------------------------------

/// The acceptance criterion from M3: a test that fails when the pass is removed. Measured rather
/// than asserted, against the same page with the relaxation turned off.
#[test]
fn the_overlap_pass_beats_leaving_every_label_at_the_midpoint() {
    let mut improved = 0;
    for (src, name) in RUNS.iter().zip(NAMES) {
        let with = overlapping_pairs(&picture(src, Some(STYLE), true));
        let without = overlapping_pairs(&picture(src, Some(STYLE), false));
        assert!(
            with <= without,
            "{name}: relaxation made it worse — {with} overlaps against {without}"
        );
        if with < without {
            improved += 1;
        }
    }
    assert!(
        improved > 0,
        "the relaxation changed nothing on any of the four runs, so nothing here is being tested"
    );
}

#[test]
fn the_overlap_pass_pulls_labels_off_the_lifelines() {
    let with = labels_on_lanes(&picture(RUNS[3], Some(STYLE), true));
    let without = labels_on_lanes(&picture(RUNS[3], Some(STYLE), false));
    assert!(
        with < without,
        "labels on lanes: {with} with the pass, {without} without — the pass is not working"
    );
}

/// Label on its own arrow: a label is drawn off the line it names, never through it.
#[test]
fn a_label_sits_beside_its_arrow_rather_than_on_it() {
    let pic = picture("0 n0 -> n1 .rb +10", Some(STYLE), true);
    let line = pic
        .prims
        .iter()
        .find_map(|p| match p {
            Prim::Line { x1, y1, x2, y2, head: true, .. } => Some((*x1, *y1, *x2, *y2)),
            _ => None,
        })
        .expect("there should be an arrow");
    let label = boxes(&pic).into_iter().find(|(_, _, _, _, t)| t == "rb").expect("labelled");
    // Distance from the label's centre to the arrow's line.
    let (x1, y1, x2, y2) = line;
    let (dx, dy) = (x2 - x1, y2 - y1);
    let len = (dx * dx + dy * dy).sqrt();
    let dist = ((label.0 - x1) * dy - (label.1 - y1) * dx).abs() / len;
    assert!(dist > 2.0, "the label sits on its own arrow, {dist} away");
}

/// Label on a gridline.
///
/// A label pinned beside a mark sits at the instant it names, and a gridline may be drawn at that
/// same instant — those two coinciding is the truth, not a collision, and the design says to accept
/// it. What must not sit on a rule is a label that had the freedom to move: a message label, which
/// rides a rail and could have gone elsewhere.
#[test]
fn a_movable_label_is_not_left_sitting_on_a_gridline() {
    let pic = picture(RUNS[0], Some(STYLE), true);
    let rules: Vec<f64> = pic
        .prims
        .iter()
        .filter_map(|p| match p {
            Prim::Line { y1, y2, x1, x2, .. }
                if (y1 - y2).abs() < 0.01 && (x2 - x1).abs() > 200.0 =>
            {
                Some(*y1)
            }
            _ => None,
        })
        .collect();
    assert!(!rules.is_empty(), "the run should have gridlines");

    let movable: Vec<(f64, f64, f64, f64, String)> = pic
        .prims
        .iter()
        .filter_map(|p| match p {
            Prim::Text { y, size, rotate, text, .. } if rotate.abs() > 0.01 => {
                Some((0.0, *y, 0.0, *size, text.clone()))
            }
            _ => None,
        })
        .collect();
    assert!(!movable.is_empty(), "the run should have message labels");

    let on = movable
        .iter()
        .filter(|(_, cy, _, h, _)| rules.iter().any(|y| (cy - y).abs() < h * 0.25))
        .count();
    assert_eq!(on, 0, "{on} movable labels are centred on a gridline");
}

// ---------------------------------------------------------------------------
// Stability — a one-tick change must not reshuffle the page
// ---------------------------------------------------------------------------

/// The acceptance criterion from M3, and the property that makes a regenerated diagram diff
/// cleanly against the one before it.
#[test]
fn perturbing_one_arrival_by_one_tick_moves_only_its_own_neighbourhood() {
    let before = picture(RUNS[0], Some(STYLE), true);
    let after = picture(&RUNS[0].replace("103  n3 -> n1 .rb +20", "103  n3 -> n1 .rb +21"), Some(STYLE), true);

    let (a, b) = (boxes(&before), boxes(&after));
    assert_eq!(a.len(), b.len(), "the same labels should be drawn");
    let moved = a
        .iter()
        .zip(&b)
        .filter(|(p, q)| (p.0 - q.0).abs() > 2.0 || (p.1 - q.1).abs() > 2.0)
        .count();
    assert!(
        moved * 4 <= a.len(),
        "{moved} of {} labels moved — a one-tick change should not cascade",
        a.len()
    );
}

#[test]
fn a_document_that_says_nothing_still_renders() {
    let out = render("participants n0 n1");
    assert!(out.contains("<svg"));
    assert!(out.contains(">n0<"), "{out}");
}

// ---------------------------------------------------------------------------
// Styles — the acceptance criteria from M4
// ---------------------------------------------------------------------------

/// An undeclared kind renders as a dot carrying its own name, so a document from a run nobody has
/// styled is never a parse error.
#[test]
fn an_undeclared_kind_renders_as_a_dot_with_its_own_name() {
    let pic = picture("10 n0 something_nobody_declared", None, true);
    assert!(
        pic.prims.iter().any(|p| matches!(p, Prim::Circle { .. })),
        "should have drawn a dot"
    );
    assert!(boxes(&pic).iter().any(|(_, _, _, _, t)| t == "something_nobody_declared"));
}

/// Explicit label beats the style sheet's default, which beats the class name.
#[test]
fn the_label_precedence_runs_line_then_style_then_class() {
    let style = "style .rb label=\"RB\"";
    let from_line = picture("0 n0 -> n1 .rb \"on the line\" +10", Some(style), true);
    assert!(boxes(&from_line).iter().any(|(_, _, _, _, t)| t == "on the line"));

    let from_style = picture("0 n0 -> n1 .rb +10", Some(style), true);
    assert!(boxes(&from_style).iter().any(|(_, _, _, _, t)| t == "RB"));

    let from_class = picture("0 n0 -> n1 .rb +10", None, true);
    assert!(boxes(&from_class).iter().any(|(_, _, _, _, t)| t == "rb"));

    let unlabelled = picture("0 n0 -> n1 +10", None, true);
    assert!(boxes(&unlabelled).iter().all(|(_, _, _, _, t)| t != "rb"));
}

/// A style sheet has no business naming a node, so the two cases of a split are told apart by
/// which sort of subject they landed on.
#[test]
fn a_subject_pseudo_class_styles_the_network_case_apart_from_the_node_one() {
    let style = "kind span split\nstyle split:network fill=#112233\nstyle split:node fill=#445566";
    let net = svg::render(&picture("0 network split n1 +10", Some(style), true), &svg::Options::default());
    let node = svg::render(&picture("0 n1 split +10", Some(style), true), &svg::Options::default());
    assert!(net.contains("#112233"), "{net}");
    assert!(node.contains("#445566"), "{node}");
}

// ---------------------------------------------------------------------------
// The axis modes — the acceptance criteria from M6
// ---------------------------------------------------------------------------

fn axis_picture(mode: &str) -> Picture {
    let style = format!("{STYLE}\nstyle diagram axis={mode}");
    picture(RUNS[3], Some(&style), true)
}

/// `mutex-egalites` has two bursts 2700 ticks apart. Linear spends a page of blank paper on the
/// gap; compressed does not.
#[test]
fn compressed_fits_a_run_with_a_dead_zone_that_linear_cannot() {
    let linear = axis_picture("linear").height;
    let compressed = axis_picture("compressed").height;
    assert!(
        compressed * 2.0 < linear,
        "compressed was {compressed} against linear {linear} — the gap was not collapsed"
    );
}

#[test]
fn ordinal_spaces_every_instant_equally() {
    let pic = axis_picture("ordinal");
    // The gridlines it draws are one per distinct instant, so their spacing is the row pitch.
    let mut ys: Vec<f64> = pic
        .prims
        .iter()
        .filter_map(|p| match p {
            Prim::Line { y1, y2, x1, x2, .. } if (y1 - y2).abs() < 0.01 && (x2 - x1).abs() > 200.0 => Some(*y1),
            _ => None,
        })
        .collect();
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    ys.dedup();
    let gaps: Vec<f64> = ys.windows(2).map(|w| w[1] - w[0]).collect();
    assert!(gaps.len() > 10, "expected many rows, got {}", gaps.len());
    let first = gaps[0];
    for g in &gaps {
        assert!((g - first).abs() < 0.01, "ordinal rows are not evenly spaced: {gaps:?}");
    }
}

/// Equal spacing costs the slope, so the numbers have to say what it cost.
#[test]
fn ordinal_writes_the_elapsed_time_between_rows() {
    let pic = axis_picture("ordinal");
    let elapsed: Vec<&str> = pic
        .prims
        .iter()
        .filter_map(|p| match p {
            Prim::Text { text, .. } if text.starts_with('+') => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(!elapsed.is_empty(), "no elapsed times in the gutter");
    // The 2700-tick dead zone is the one worth being able to read.
    assert!(
        elapsed.iter().any(|t| t.trim_start_matches('+').parse::<i64>().unwrap_or(0) > 2000),
        "the long gap should be labelled with what it cost: {elapsed:?}"
    );
}

#[test]
fn the_row_pitch_is_configurable() {
    let tight = picture(RUNS[3], Some(&format!("{STYLE}\nstyle diagram axis=ordinal row-pitch=10")), true);
    let loose = picture(RUNS[3], Some(&format!("{STYLE}\nstyle diagram axis=ordinal row-pitch=40")), true);
    assert!(loose.height > tight.height * 2.0);
}

#[test]
fn every_axis_mode_renders_every_run() {
    for mode in ["linear", "compressed", "ordinal"] {
        for (src, name) in RUNS.iter().zip(NAMES) {
            let style = format!("{STYLE}\nstyle diagram axis={mode}");
            let out = svg::render(&picture(src, Some(&style), true), &svg::Options::default());
            assert!(out.contains("<svg"), "{name} in {mode}");
            assert!(out.len() > 500, "{name} in {mode} produced almost nothing");
        }
    }
}

// ---------------------------------------------------------------------------
// Themes
// ---------------------------------------------------------------------------

#[test]
fn both_themes_paint_their_own_background() {
    let light = svg::render(&picture(RUNS[0], Some(STYLE), true), &svg::Options::default());
    let dark = svg::render(
        &picture(RUNS[0], Some(&format!("{STYLE}\nstyle diagram theme=dark")), true),
        &svg::Options::default(),
    );
    assert!(light.contains("fill=\"#ffffff\""), "light should paint white");
    assert!(dark.contains("fill=\"#0e1116\""), "dark should paint its own ground");
    let _ = Theme::Light;
}

// ---------------------------------------------------------------------------
// The two things a document can say that the style sheet only shows on request
// ---------------------------------------------------------------------------

#[test]
fn a_title_is_drawn_as_a_caption_and_costs_a_line_when_there_is_one() {
    let with = picture("title \"A relay\"\n0 n0 deliver", None, true);
    let without = picture("0 n0 deliver", None, true);
    assert!(
        boxes(&with).iter().any(|(_, _, _, _, t)| t == "A relay"),
        "the caption was not drawn"
    );
    assert!(with.height > without.height, "the caption should make room for itself");
    // And nothing else moves relative to it: the whole picture shifted by one line.
    let dy = with.height - without.height;
    assert!(dy > 10.0 && dy < 40.0, "the caption took {dy}px");
}

/// Shifting for the caption must move every kind of primitive, including a path, or a self-loop
/// would detach from the lane it belongs to.
#[test]
fn the_caption_shift_moves_paths_too() {
    let with = picture("title \"t\"\n100 n0 -> n0 .retry +20", None, true);
    let without = picture("100 n0 -> n0 .retry +20", None, true);
    let path_y = |p: &Picture| -> Vec<f64> {
        p.prims
            .iter()
            .filter_map(|q| match q {
                Prim::Path { d, .. } => Some(
                    d.split_whitespace()
                        .filter_map(|t| t.parse::<f64>().ok())
                        .skip(1)
                        .step_by(2)
                        .fold(0.0, f64::max),
                ),
                _ => None,
            })
            .collect()
    };
    let (a, b) = (path_y(&with), path_y(&without));
    assert_eq!(a.len(), b.len());
    assert!(!a.is_empty(), "the self-loop should be a path");
    for (x, y) in a.iter().zip(&b) {
        assert!(x > y, "the path did not move with everything else: {x} against {y}");
    }
}

/// Bodies are off by default, because a page of JSON buries the diagram.
#[test]
fn a_detail_block_is_drawn_only_when_the_style_sheet_asks() {
    let doc = "0 n0 -> n1 .rb +10 {\n  {\"seq\": 4}\n}";
    let off = picture(doc, None, true);
    assert!(boxes(&off).iter().all(|(_, _, _, _, t)| !t.contains("seq")));

    let on = picture(doc, Some("style diagram bodies=on"), true);
    assert!(
        boxes(&on).iter().any(|(_, _, _, _, t)| t.contains("seq")),
        "bodies=on should print the block"
    );
}

#[test]
fn a_multi_line_body_prints_every_line() {
    let on = picture(
        "0 n0 -> n1 .rb +10 {\n  one\n  two\n  three\n}",
        Some("style diagram bodies=on"),
        true,
    );
    for want in ["one", "two", "three"] {
        assert!(
            boxes(&on).iter().any(|(_, _, _, _, t)| t == want),
            "{want} is missing from the page"
        );
    }
}

/// A body prints in the right margin, so the page has to be wide enough for the widest line of the
/// widest one. Measured rather than guessed, because a body is whatever the protocol put there.
#[test]
fn a_long_body_widens_the_page_rather_than_running_off_it() {
    let long = "x".repeat(90);
    let doc = format!("0 n0 -> n1 .rb +10 {{\n  {long}\n}}");
    let pic = picture(&doc, Some("style diagram bodies=on"), true);
    let f = font::Font::embedded();
    for (cx, _, w, _, t) in boxes(&pic) {
        if t.starts_with("xxx") {
            let right = cx + w * 0.5;
            assert!(
                right <= pic.width,
                "the body runs {}px past the page edge",
                right - pic.width
            );
        }
    }
    let narrow = picture("0 n0 -> n1 .rb +10", Some("style diagram bodies=on"), true);
    assert!(pic.width > narrow.width + f.width(&long, 11.0) * 0.8);
}

// ---------------------------------------------------------------------------
// The closed lists, each checked to actually reach the page
// ---------------------------------------------------------------------------

/// A shape nobody can draw is a shape nobody will use. Every name in the list produces something,
/// and `none` produces nothing, which is the whole point of it.
#[test]
fn every_marker_shape_draws_something_except_none() {
    for shape in ["dot", "ring", "square", "diamond", "cross", "bar", "chevron"] {
        let pic = picture("10 n0 thing", Some(&format!("style thing mark={shape}")), true);
        let marks = pic
            .prims
            .iter()
            .filter(|p| {
                matches!(p, Prim::Circle { .. } | Prim::Polygon { .. })
                    || matches!(p, Prim::Rect { .. })
                    || matches!(p, Prim::Line { x1, x2, y1, y2, .. }
                        if (x2 - x1).abs() < 30.0 && (y2 - y1).abs() < 30.0)
            })
            .count();
        assert!(marks > 0, "{shape} drew nothing");
    }
    let none = picture("10 n0 thing", Some("style thing mark=none"), true);
    assert!(
        !none.prims.iter().any(|p| matches!(p, Prim::Circle { .. })),
        "`none` should draw no marker"
    );
}

/// Every pseudo-class the derivation can produce is reachable from a selector. One that never
/// matches is a promise the style sheet cannot keep.
#[test]
fn every_pseudo_class_is_reachable_from_a_selector() {
    // (document, selector, the colour it must take)
    let cases: &[(&str, &str, &str)] = &[
        ("0 n0 -> n1 .m +10", "arrow:arrived", "#110011"),
        ("5 n0 crash\n10 n0 -> n1 .m +10", "arrow:never-left", "#110022"),
        ("5 n1 crash\n0 n0 -> n1 .m +10", "arrow:died", "#110033"),
        ("0 n0 -x n1 .m", "arrow:eaten", "#110044"),
        ("0 n0 -> n1 .m +99\n10 run end", "arrow:in-flight", "#110055"),
        ("5 n0 crash\n10 run end", "lifeline:dead", "#110066"),
        ("0 n0 deliver\n10 run end", "lifeline:alive", "#110077"),
        ("0 n0 s\n10 run end", "s:open", "#110088"),
        ("participants n0 n1\n0 network s +5", "s:network", "#110099"),
        ("0 n0 s +5", "s:node", "#1100aa"),
        ("participants n0\n0 run r", "r:run", "#1100bb"),
    ];
    let base = "kind point crash kills\nkind span s\nkind point r\n";
    for (doc, selector, colour) in cases {
        let style = format!("{base}style {selector} color={colour} fill={colour}");
        let out = svg::render(&picture(doc, Some(&style), true), &svg::Options::default());
        assert!(
            out.contains(colour),
            "`{selector}` never matched anything in `{doc}`"
        );
    }
}

// ---------------------------------------------------------------------------
// Placement: one engine, two kinds of freedom
// ---------------------------------------------------------------------------

/// Distance from a point to a segment, and where on it the nearest point was.
fn nearest(p: (f64, f64), s: (f64, f64, f64, f64)) -> (f64, (f64, f64)) {
    let (dx, dy) = (s.2 - s.0, s.3 - s.1);
    let l2 = dx * dx + dy * dy;
    let t = if l2 < 1e-9 { 0.0 } else { (((p.0 - s.0) * dx + (p.1 - s.1) * dy) / l2).clamp(0.0, 1.0) };
    let c = (s.0 + dx * t, s.1 + dy * t);
    (((p.0 - c.0).powi(2) + (p.1 - c.1).powi(2)).sqrt(), c)
}

fn slanted(pic: &Picture) -> Vec<(f64, f64, f64, f64)> {
    pic.prims
        .iter()
        .filter_map(|p| match p {
            Prim::Line { x1, y1, x2, y2, .. } if (x2 - x1).abs() > 1.0 && (y2 - y1).abs() > 1.0 => {
                Some((*x1, *y1, *x2, *y2))
            }
            _ => None,
        })
        .collect()
}

/// Every message label sits on the same side of its arrow, whichever way the arrow points.
///
/// They used not to: the perpendicular was taken straight from the direction vector, so a leftward
/// arrow flipped it and the page ended up with some labels above their line and some below, for no
/// reason a reader could see. Tested on a pair that differs only in direction, because in a busy
/// picture the segment nearest a label is often not the one it belongs to.
#[test]
fn a_label_sits_above_its_arrow_whichever_way_the_arrow_points() {
    for doc in ["0 n0 -> n1 .a +10\n50 run end", "0 n1 -> n0 .a +10\n50 run end"] {
        let pic = picture(doc, Some(STYLE), true);
        let seg = slanted(&pic).into_iter().next().expect("one arrow");
        let mid = ((seg.0 + seg.2) * 0.5, (seg.1 + seg.3) * 0.5);
        let label = pic
            .prims
            .iter()
            .find_map(|p| match p {
                Prim::Text { x, y, text, .. } if text == "a" => Some((*x, *y)),
                _ => None,
            })
            .expect("the arrow is labelled");
        // Same x, so the comparison is purely which side of the line it went.
        let on_line = seg.1 + (seg.3 - seg.1) * ((label.0 - seg.0) / (seg.2 - seg.0));
        assert!(
            label.1 < on_line,
            "{doc:?}: the label sits below its arrow ({} against {on_line})",
            label.1
        );
        let _ = mid;
    }
}

/// A pinned label may move, but only so far: it belongs to the instant it names, and one that
/// drifted would stop naming it.
///
/// A pinned label is the one anchored beside a mark, which is what the anchor says — a message
/// label is centred on its rail. Rotation cannot be used to tell them apart, because a
/// near-horizontal arrow has none.
#[test]
fn a_pinned_label_stays_near_the_instant_it_belongs_to() {
    let pic = picture(RUNS[3], Some(STYLE), true);
    let lanes: Vec<f64> = pic
        .prims
        .iter()
        .filter_map(|p| match p {
            Prim::Line { x1, x2, y1, y2, .. } if (x1 - x2).abs() < 0.01 && (y2 - y1).abs() > 40.0 => Some(*x1),
            _ => None,
        })
        .collect();
    let mut checked = 0;
    for p in &pic.prims {
        let Prim::Text { x, text, anchor, tag, .. } = p else { continue };
        if *anchor == Anchor::Middle || tag.event.is_none() || tag.role == "body" {
            continue;
        }
        let d = lanes
            .iter()
            .map(|l| (x - l).abs())
            .fold(f64::INFINITY, f64::min);
        assert!(d < 60.0, "a pinned label sits {d}px from any lane: {text:?}");
        checked += 1;
    }
    assert!(checked >= 8, "only {checked} pinned labels were checked");
}

/// A label pushed off the edge is not placed, it is lost.
#[test]
fn no_label_runs_off_the_page() {
    let f = font::Font::embedded();
    for (src, name) in RUNS.iter().zip(NAMES) {
        let pic = picture(src, Some(STYLE), true);
        for p in &pic.prims {
            let Prim::Text { x, text, size, anchor, rotate, .. } = p else { continue };
            if rotate.abs() > 0.01 {
                continue; // a rotated box is not axis-aligned; its centre is checked elsewhere
            }
            let w = f.width(text, *size);
            let left = match anchor {
                Anchor::Start => *x,
                Anchor::Middle => x - w * 0.5,
                Anchor::End => x - w,
            };
            assert!(left >= -1.0, "{name}: {text:?} starts at {left}, off the left edge");
            assert!(
                left + w <= pic.width + 1.0,
                "{name}: {text:?} ends at {}, past the right edge {}",
                left + w,
                pic.width
            );
        }
    }
}

/// The fourth kind of overlap the design names, and the one most easily forgotten: an arrow is not
/// a box, so it does not show up in a box-against-box test.
#[test]
fn knowing_about_the_arrows_keeps_labels_off_them() {
    let count_on = |pic: &Picture| {
        let segs = slanted(pic);
        pic.prims
            .iter()
            .filter(|p| matches!(p, Prim::Text { rotate, .. } if rotate.abs() < 0.01))
            .filter(|p| {
                let Prim::Text { x, y, .. } = p else { return false };
                segs.iter().any(|s| nearest((*x, *y - 4.0), *s).0 < 3.0)
            })
            .count()
    };
    let with = count_on(&picture(RUNS[0], Some(STYLE), true));
    let without = count_on(&picture(RUNS[0], Some(STYLE), false));
    assert!(
        with <= without,
        "the pass left {with} labels on an arrow against {without} without it"
    );
}

/// A bare arrowhead says too little about where something came from.
#[test]
fn the_arrow_mark_draws_a_shaft_and_the_chevron_does_not() {
    let arrow = picture("10 n0 asked req\n200 run end", Some("style asked mark=arrow"), true);
    let shafts = arrow
        .prims
        .iter()
        .filter(|p| matches!(p, Prim::Line { head: true, x1, x2, .. } if (x2 - x1).abs() > 4.0))
        .count();
    assert!(shafts > 0, "the arrow mark drew no shaft");

    let chevron = picture("10 n0 asked req\n200 run end", Some("style asked mark=chevron"), true);
    assert!(
        chevron.prims.iter().any(|p| matches!(p, Prim::Polygon { .. })),
        "the chevron should still be a bare head"
    );
}

/// The mark faces the way its label went, so the two read as one thing.
#[test]
fn the_arrow_mark_faces_the_side_its_label_settled_on() {
    // n0 is the leftmost lane, so its label has nowhere to go but right, and the shaft follows.
    let pic = picture("10 n0 asked a_long_enough_label\n200 run end", Some("style asked mark=arrow"), true);
    let lane = pic
        .prims
        .iter()
        .find_map(|p| match p {
            Prim::Line { x1, x2, y1, y2, .. } if (x1 - x2).abs() < 0.01 && (y2 - y1).abs() > 40.0 => Some(*x1),
            _ => None,
        })
        .expect("a lifeline");
    let shaft = pic
        .prims
        .iter()
        .find_map(|p| match p {
            Prim::Line { x1, x2, head: true, .. } if (x2 - x1).abs() > 4.0 => Some((*x1, *x2)),
            _ => None,
        })
        .expect("a shaft");
    let label_x = pic
        .prims
        .iter()
        .find_map(|p| match p {
            Prim::Text { x, text, .. } if text == "a_long_enough_label" => Some(*x),
            _ => None,
        })
        .expect("a label");
    let label_side = (label_x - lane).signum();
    let shaft_side = (shaft.0 - lane).signum();
    assert_eq!(label_side, shaft_side, "the shaft points away from its own label");
}
