//! The properties the plan asks each milestone to hold, checked against the three real runs.
//!
//! These are the tests that would catch a regression nobody was looking for: reproducibility,
//! stability under a one-tick change, and whether the overlap pass is doing anything at all.

use cuesheet::model::Theme;
use cuesheet::picture::{Anchor, Picture, Prim};
use cuesheet::{derive, font, layout, parse, svg};

const RUNS: [&str; 4] = [
    include_str!("../examples/relais-manquant.st"),
    include_str!("../examples/showcase.st"),
    include_str!("../examples/partition.st"),
    include_str!("../examples/mutex-egalites.st"),
];
const NAMES: [&str; 4] = ["relais-manquant", "showcase", "partition", "mutex-egalites"];
const STYLE: &str = include_str!("../examples/course.sts");

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
