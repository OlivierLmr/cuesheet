//! Ticks into pixels.
//!
//! This is where every coordinate is decided, and the only place. It reads the derivation, which
//! is still entirely in logical time, and produces a [`Picture`], which is entirely in pixels.
//!
//! The part with any subtlety in it is label placement. A label rides its own arrow — one degree
//! of freedom, a parameter along the line — and relaxes against the others as though they
//! repelled. One dimension per label rather than two is what makes that tractable, and three
//! properties are built in rather than retrofitted: it is deterministic (fixed iterations, fixed
//! order, no randomness), it terminates, and it is stable, so a one-tick change to the input does
//! not reshuffle the page.

use crate::derive::{Fate, Run};
use crate::font::Font;
use crate::model::*;
use crate::picture::*;
use std::collections::BTreeMap;

const MARGIN_L: f64 = 78.0;
const MARGIN_R: f64 = 96.0;
const HEADER: f64 = 56.0;
const FOOTER: f64 = 34.0;
const LABEL_SIZE: f64 = 11.0;
const HEAD_SIZE: f64 = 13.0;
const GUTTER_SIZE: f64 = 11.0;
/// How tall a collapsed quiet stretch is drawn, in the compressed axis.
const BREAK_HEIGHT: f64 = 26.0;
/// How far a stub reaches when a message never got anywhere.
const STUB: f64 = 0.22;

/// The vertical map from logical time to pixels. Built once from every instant the run mentions,
/// then interpolated for anything in between.
pub struct Axis3 {
    knots: Vec<(Tick, f64)>,
    /// Instants where a quiet stretch was collapsed, for the mark that says so.
    pub breaks: Vec<Tick>,
    pub mode: Axis,
}

impl Axis3 {
    pub fn y(&self, t: Tick) -> f64 {
        match self.knots.binary_search_by_key(&t, |(k, _)| *k) {
            Ok(i) => self.knots[i].1,
            Err(0) => self.knots.first().map(|(_, y)| *y).unwrap_or(0.0),
            Err(i) if i >= self.knots.len() => self.knots.last().map(|(_, y)| *y).unwrap_or(0.0),
            Err(i) => {
                let (t0, y0) = self.knots[i - 1];
                let (t1, y1) = self.knots[i];
                if t1 == t0 {
                    y0
                } else {
                    y0 + (y1 - y0) * (t - t0) as f64 / (t1 - t0) as f64
                }
            }
        }
    }

    pub fn instants(&self) -> impl Iterator<Item = (Tick, f64)> + '_ {
        self.knots.iter().copied()
    }

    pub fn height(&self) -> f64 {
        self.knots.last().map(|(_, y)| *y).unwrap_or(HEADER)
    }
}

/// Every instant the run mentions, sorted and de-duplicated. The axis is defined on these, and
/// anything between two of them is interpolated.
fn instants(doc: &Document, run: &Run) -> Vec<Tick> {
    let mut v: Vec<Tick> = Vec::new();
    v.push(run.first);
    v.push(run.end);
    for e in &doc.events {
        v.push(e.time);
        if let Some(i) = e.instant {
            v.push(i.resolve(e.time));
        }
    }
    for m in &run.messages {
        v.push(m.depart);
        v.push(m.arrive);
    }
    for s in &run.spans {
        v.push(s.start);
        v.push(s.end);
    }
    v.retain(|t| *t <= run.end || run.end == run.first);
    v.sort_unstable();
    v.dedup();
    v
}

fn build_axis(doc: &Document, run: &Run, d: &Diagram) -> Axis3 {
    let ts = instants(doc, run);
    let mut knots = Vec::with_capacity(ts.len());
    let mut breaks = Vec::new();
    let mut y = HEADER;

    match d.axis {
        Axis::Linear => {
            for t in &ts {
                knots.push((*t, HEADER + (*t - run.first) as f64 * d.stretch));
            }
        }
        Axis::Compressed => {
            let mut prev: Option<Tick> = None;
            for t in &ts {
                if let Some(p) = prev {
                    let delta = *t - p;
                    if delta > d.gap {
                        // A quiet stretch collapses to a marked break rather than a page of
                        // blank paper. The break is marked so nobody reads the page as linear.
                        y += BREAK_HEIGHT;
                        breaks.push(p);
                    } else {
                        y += delta as f64 * d.stretch;
                    }
                }
                knots.push((*t, y));
                prev = Some(*t);
            }
        }
        Axis::Ordinal => {
            for (i, t) in ts.iter().enumerate() {
                knots.push((*t, HEADER + i as f64 * d.row_pitch));
            }
        }
    }
    Axis3 { knots, breaks, mode: d.axis }
}

/// The resolved look of one thing.
struct Look {
    props: Props,
}

impl Look {
    fn stroke(&self, fallback: Color) -> Stroke {
        Stroke {
            color: self.props.color.unwrap_or(fallback),
            width: self.props.width.unwrap_or(1.5),
            dash: self.props.dash.clone().unwrap_or(Dash::Solid),
            opacity: self.props.opacity.unwrap_or(1.0),
        }
    }
    fn fill(&self, fallback: Color) -> Fill {
        Fill {
            color: self.props.fill.or(self.props.color).unwrap_or(fallback),
            opacity: self.props.opacity.unwrap_or(1.0),
        }
    }
}

pub struct Palette {
    pub ink: Color,
    pub faint: Color,
    pub rule: Color,
    pub background: Color,
}

fn palette(theme: Theme) -> Palette {
    match theme {
        Theme::Light => Palette {
            ink: Color(0x14, 0x17, 0x1c),
            faint: Color(0x87, 0x8f, 0x9b),
            rule: Color(0xc3, 0xc9, 0xd3),
            background: Color(0xff, 0xff, 0xff),
        },
        Theme::Dark => Palette {
            ink: Color(0xe7, 0xe9, 0xed),
            faint: Color(0x6c, 0x75, 0x82),
            rule: Color(0x39, 0x41, 0x4e),
            background: Color(0x0e, 0x11, 0x16),
        },
    }
}

/// A label riding a rail, before relaxation has decided where along it to sit.
struct Rail {
    text: String,
    /// Start and end of the line the label rides.
    a: (f64, f64),
    b: (f64, f64),
    /// Parameter along the rail, and where it would rather be.
    f: f64,
    pref: f64,
    w: f64,
    h: f64,
    fill: Color,
    opacity: f64,
    tag: Tag,
    rotate: bool,
}

impl Rail {
    fn at(&self, f: f64) -> (f64, f64) {
        (self.a.0 + (self.b.0 - self.a.0) * f, self.a.1 + (self.b.1 - self.a.1) * f)
    }
    fn centre(&self) -> (f64, f64) {
        // Sit just off the line rather than on it, on the side the arrow is travelling away from.
        let (x, y) = self.at(self.f);
        let (dx, dy) = (self.b.0 - self.a.0, self.b.1 - self.a.1);
        let len = (dx * dx + dy * dy).sqrt().max(1e-6);
        let (nx, ny) = (-dy / len, dx / len);
        let off = self.h * 0.72;
        (x + nx * off, y + ny * off)
    }
    fn angle(&self) -> f64 {
        if !self.rotate {
            return 0.0;
        }
        let (dx, dy) = (self.b.0 - self.a.0, self.b.1 - self.a.1);
        let mut deg = dy.atan2(dx).to_degrees();
        // Keep text the right way up: an arrow pointing left would otherwise write upside down.
        if deg > 90.0 {
            deg -= 180.0;
        } else if deg < -90.0 {
            deg += 180.0;
        }
        deg
    }
}

/// How bad an arrangement is: overlap between label boxes, plus labels sitting on a lane.
///
/// Continuous rather than a count, so it can guide the search rather than only judge it.
fn cost(rails: &[Rail], lanes: &[f64], rules: &[f64], fixed: &[(f64, f64, f64, f64)]) -> f64 {
    let mut c = 0.0;
    for i in 0..rails.len() {
        let ci = rails[i].centre();
        for j in (i + 1)..rails.len() {
            let cj = rails[j].centre();
            let ox = (rails[i].w + rails[j].w) * 0.5 + 4.0 - (ci.0 - cj.0).abs();
            let oy = (rails[i].h + rails[j].h) * 0.5 + 2.0 - (ci.1 - cj.1).abs();
            if ox > 0.0 && oy > 0.0 {
                c += ox.min(oy);
            }
        }
        for (fx, fy, fw, fh) in fixed {
            let ox = (rails[i].w + fw) * 0.5 + 4.0 - (ci.0 - fx).abs();
            let oy = (rails[i].h + fh) * 0.5 + 2.0 - (ci.1 - fy).abs();
            if ox > 0.0 && oy > 0.0 {
                c += ox.min(oy);
            }
        }
        for x in lanes {
            let over = rails[i].w * 0.5 + 6.0 - (ci.0 - x).abs();
            if over > 0.0 {
                c += over * 0.6;
            }
        }
        for y in rules {
            let over = rails[i].h * 0.5 - (ci.1 - y).abs();
            if over > 0.0 {
                c += over * 0.5;
            }
        }
        // A weak preference for the middle of the arrow, so a label with nothing to avoid stays
        // where the reader expects it rather than drifting to an end.
        c += (rails[i].f - rails[i].pref).abs() * 2.0;
    }
    c
}

/// Push labels along their own rails until they stop overlapping, or until the budget runs out and
/// they settle at least-bad.
///
/// Each label has one degree of freedom — a parameter along its own arrow — which is what makes
/// this tractable. Three properties are built in rather than retrofitted:
///
/// * **Deterministic.** A fixed number of passes, a fixed order, and no randomness anywhere, so
///   byte-identical output survives.
/// * **Terminating.** The budget is fixed, and a pass that moves nothing stops early.
/// * **Stable.** The spring pulls each label back toward its preferred fraction, so a label with
///   no conflict never moves and a small input change cannot cascade.
///
/// It keeps the best arrangement it has seen, *including the one it started from*. A relaxation
/// that cannot guarantee it improves on doing nothing should at least never make things worse —
/// and on a dense page the naive version genuinely does, by pushing one label into the space of
/// another it had not yet considered.
fn relax(rails: &mut [Rail], lanes: &[f64], rules: &[f64], fixed: &[(f64, f64, f64, f64)]) {
    const PASSES: usize = 120;
    const MIN_F: f64 = 0.12;
    const MAX_F: f64 = 0.88;

    if rails.is_empty() {
        return;
    }

    let mut best: Vec<f64> = rails.iter().map(|r| r.f).collect();
    let mut best_cost = cost(rails, lanes, rules, fixed);

    for pass in 0..PASSES {
        // Annealed: big steps to get out of the starting arrangement, small ones to settle into a
        // local minimum rather than oscillate around it.
        let step = 0.09 * (1.0 - pass as f64 / PASSES as f64).max(0.15);
        let mut moved = 0f64;

        // Gauss-Seidel: each label moves before the next one is considered, so two labels cannot
        // both step into the same gap on the strength of the same stale snapshot.
        for i in 0..rails.len() {
            let mut push = 0f64;
            let ci = rails[i].centre();
            let (dx, dy) = (rails[i].b.0 - rails[i].a.0, rails[i].b.1 - rails[i].a.1);
            let len = (dx * dx + dy * dy).sqrt().max(1e-6);
            let (ux, uy) = (dx / len, dy / len);

            let mut repel = |cx: f64, cy: f64, w: f64, h: f64, weight: f64, push: &mut f64| {
                let ox = (rails[i].w + w) * 0.5 + 4.0 - (ci.0 - cx).abs();
                let oy = (rails[i].h + h) * 0.5 + 2.0 - (ci.1 - cy).abs();
                if ox <= 0.0 || oy <= 0.0 {
                    return;
                }
                let d = (ci.0 - cx, ci.1 - cy);
                let dlen = (d.0 * d.0 + d.1 * d.1).sqrt().max(1e-6);
                // Only the component that this label's own rail can actually deliver. A label on a
                // near-horizontal rail cannot fix a vertical overlap, and pretending otherwise is
                // what makes the whole set jitter.
                let along = ux * (d.0 / dlen) + uy * (d.1 / dlen);
                *push += along * (ox.min(oy) / 24.0).min(1.0) * weight;
            };

            for j in 0..rails.len() {
                if j == i {
                    continue;
                }
                let cj = rails[j].centre();
                repel(cj.0, cj.1, rails[j].w, rails[j].h, 1.0, &mut push);
            }
            for (fx, fy, fw, fh) in fixed {
                repel(*fx, *fy, *fw, *fh, 1.0, &mut push);
            }
            // A label sitting on somebody else's lifeline is the commonest collision on a
            // space-time diagram, because the midpoint of a long arrow lands on a lane.
            for x in lanes {
                let over = rails[i].w * 0.5 + 6.0 - (ci.0 - x).abs();
                if over > 0.0 {
                    let dir = if ci.0 >= *x { 1.0 } else { -1.0 };
                    push += ux * dir * (over / 24.0).min(1.0) * 0.8;
                }
            }
            // And the gridlines, which run the other way. A slanted rail changes its label's y
            // as well as its x, so moving along one is a way off a rule as well as off a lane.
            for y in rules {
                let over = rails[i].h * 0.5 - (ci.1 - y).abs();
                if over > 0.0 {
                    let dir = if ci.1 >= *y { 1.0 } else { -1.0 };
                    push += uy * dir * (over / 12.0).min(1.0) * 0.9;
                }
            }
            push += (rails[i].pref - rails[i].f) * 0.35;

            let next = (rails[i].f + push * step).clamp(MIN_F, MAX_F);
            moved += (next - rails[i].f).abs();
            rails[i].f = next;
        }

        let c = cost(rails, lanes, rules, fixed);
        if c < best_cost {
            best_cost = c;
            best = rails.iter().map(|r| r.f).collect();
        }
        if moved < 1e-4 {
            break;
        }
    }

    for (r, f) in rails.iter_mut().zip(best) {
        r.f = f;
    }
}

pub fn build(doc: &Document, sheet: &StyleSheet, run: &Run, font: &Font) -> Picture {
    build_with(doc, sheet, run, font, true)
}

/// The same, with the label relaxation turned off.
///
/// Exists so a test can measure the page with and without it. A test that cannot fail is not a
/// passing test but an absent one, and "the overlap pass works" is only meaningful against a page
/// where it did not run.
pub fn build_with(
    doc: &Document,
    sheet: &StyleSheet,
    run: &Run,
    font: &Font,
    relax_labels: bool,
) -> Picture {
    let d = &sheet.diagram;
    let pal = palette(d.theme);
    let axis = build_axis(doc, run, d);

    let lane_x: BTreeMap<&str, f64> = doc
        .participants
        .iter()
        .enumerate()
        .map(|(i, p)| (p.as_str(), MARGIN_L + i as f64 * d.lane_pitch))
        .collect();
    let lanes: Vec<f64> = doc.participants.iter().filter_map(|p| lane_x.get(p.as_str()).copied()).collect();
    let x_of = |n: &str| lane_x.get(n).copied().unwrap_or(MARGIN_L);

    let width = MARGIN_L
        + (doc.participants.len().max(1) as f64 - 1.0) * d.lane_pitch
        + MARGIN_R;
    let top = HEADER;
    let bottom = axis.height();
    let height = bottom + FOOTER;

    let mut prims: Vec<Prim> = Vec::new();
    let mut rails: Vec<Rail> = Vec::new();
    // Point labels sit beside their mark rather than on a rail, so they cannot relax along
    // anything — two events at the same instant on the same lane would print on top of each other.
    // Collected here and dodged downward once the whole set is known.
    let mut pins: Vec<(f64, f64, Anchor, String, Color, f64, Tag)> = Vec::new();

    let look = |kind: Option<&str>, classes: &[String], states: &[String]| Look {
        props: sheet.resolve(kind, classes, states),
    };

    // ---- the time gutter -------------------------------------------------
    gutter(&mut prims, &axis, d, &pal, width);

    // ---- lifelines -------------------------------------------------------
    for p in &doc.participants {
        let x = x_of(p);
        prims.push(Prim::Text {
            x,
            y: top - 22.0,
            text: p.clone(),
            size: HEAD_SIZE,
            anchor: Anchor::Middle,
            fill: pal.ink,
            opacity: 1.0,
            rotate: 0.0,
            mono: true,
            tag: Tag { event: None, role: "participant".into(), detail: None },
        });

        // Split the lifeline into the stretches where the process was taking part and the ones
        // where it was not, so a crashed process gets a ghost line without the renderer knowing
        // what a crash is.
        let subject = Subject::Node(p.clone());
        let mut cuts: Vec<Tick> = vec![run.first];
        for (t, _) in run.liveness.get(&subject).map(|l| l.transitions()).unwrap_or_default() {
            if t > run.first && t <= run.end {
                cuts.push(t);
            }
        }
        cuts.push(run.end);
        cuts.dedup();
        for w in cuts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let alive = run.liveness.get(&subject).map(|l| l.at(a)).unwrap_or(true);
            let states = vec![if alive { "alive".to_string() } else { "dead".to_string() }];
            let lk = look(Some("lifeline"), &[p.clone()], &states);
            let mut stroke = lk.stroke(if alive { pal.rule } else { pal.faint });
            if !alive && lk.props.dash.is_none() {
                stroke.dash = Dash::Pattern(2.0, 6.0);
            }
            prims.push(Prim::Line {
                x1: x,
                y1: if a == run.first { top - 12.0 } else { axis.y(a) },
                x2: x,
                y2: axis.y(b),
                stroke,
                head: false,
                tag: Tag { event: None, role: "lifeline".into(), detail: None },
            });
        }
    }

    // ---- spans, behind everything ----------------------------------------
    for s in &run.spans {
        let e = &doc.events[s.event];
        let mut states = vec![s.subject.pseudo().to_string()];
        if s.open {
            states.push("open".into());
        }
        let mut classes = e.classes.clone();
        classes.push(s.subject.name().to_string());
        let lk = look(Some(&s.kind), &classes, &states);
        let (y1, y2) = (axis.y(s.start), axis.y(s.end));
        let fill = lk.fill(pal.rule);
        let rx = lk.props.radius.unwrap_or(2.0);
        let half = lk.props.width.unwrap_or(7.0) * 0.5;

        // The subject decides where it is drawn: a span on a node paints its lifeline, one on the
        // network shades the lanes it names — each lane separately, so two sides of a split that
        // are not next to each other still work.
        let targets: Vec<f64> = match &s.subject {
            Subject::Node(n) => vec![x_of(n)],
            Subject::Network | Subject::Run => {
                let named = e.lanes();
                if named.is_empty() {
                    lanes.clone()
                } else {
                    named.iter().map(|n| x_of(n)).collect()
                }
            }
        };
        let w = match s.subject {
            Subject::Node(_) => half * 2.0,
            _ => d.lane_pitch * 0.5,
        };
        for x in targets {
            if s.open && lk.props.edge.unwrap_or(Edge::Ragged) == Edge::Ragged {
                prims.push(Prim::Path {
                    d: ragged(x - w * 0.5, y1, w, y2 - y1),
                    stroke: None,
                    fill: Some(fill.clone()),
                    head: false,
                    tag: Tag { event: Some(s.event), role: s.kind.clone(), detail: e.detail.clone() },
                });
            } else {
                prims.push(Prim::Rect {
                    x: x - w * 0.5,
                    y: y1,
                    w,
                    h: (y2 - y1).max(1.0),
                    rx,
                    fill: Some(fill.clone()),
                    stroke: None,
                    tag: Tag { event: Some(s.event), role: s.kind.clone(), detail: e.detail.clone() },
                });
            }
        }

        if let Some(text) = e.stated_label().or_else(|| lk.props.label.clone()) {
            let x = match &s.subject {
                Subject::Node(n) => x_of(n),
                _ => lanes.first().copied().unwrap_or(MARGIN_L),
            };
            prims.push(Prim::Text {
                x: x + w * 0.5 + 6.0,
                y: (y1 + y2) * 0.5 + font.cap_height(LABEL_SIZE) * 0.5,
                text,
                size: LABEL_SIZE,
                anchor: Anchor::Start,
                fill: lk.props.color.unwrap_or(pal.ink),
                opacity: lk.props.opacity.unwrap_or(1.0),
                rotate: 0.0,
                mono: false,
                tag: Tag { event: Some(s.event), role: s.kind.clone(), detail: None },
            });
        }
    }

    // ---- messages --------------------------------------------------------
    for m in &run.messages {
        let e = &doc.events[m.event];
        let mut classes = e.classes.clone();
        classes.push(e.subject.name().to_string());
        let states = vec![m.fate.state().to_string(), "node".to_string()];
        let lk = look(Some("arrow"), &classes, &states);
        let mut stroke = lk.stroke(pal.ink);

        let (x1, y1) = (x_of(&m.from), axis.y(m.depart));
        let (x2, y2) = (x_of(&m.to), axis.y(m.arrive.min(run.end)));

        // Redundant encoding: every fate is carried by line style as well as hue, so the picture
        // survives a monochrome printout and a reader who cannot tell the hues apart.
        let (ex, ey, head, cross) = match m.fate {
            Fate::Arrived => (x2, y2, true, false),
            Fate::DiedAtLifeline => (x2, y2, false, true),
            Fate::NeverLeft => {
                if lk.props.dash.is_none() {
                    stroke.dash = Dash::Pattern(3.0, 3.0);
                }
                (x1 + (x2 - x1) * STUB, y1 + (y2 - y1) * STUB, false, true)
            }
            Fate::Eaten => {
                if lk.props.dash.is_none() {
                    stroke.dash = Dash::Pattern(1.0, 3.0);
                }
                let (tx, ty) = (x_of(&m.to), y1 + 18.0);
                (x1 + (tx - x1) * STUB * 1.6, y1 + (ty - y1) * STUB * 1.6, false, true)
            }
            Fate::InFlight => {
                let yend = axis.y(run.end);
                let f = if (y2 - y1).abs() < 1e-6 {
                    1.0
                } else {
                    ((yend - y1) / (y2 - y1)).clamp(0.0, 1.0)
                };
                (x1 + (x2 - x1) * f, yend, false, false)
            }
        };

        let tag = Tag { event: Some(m.event), role: "arrow".into(), detail: e.detail.clone() };

        if m.is_self() {
            // A message to itself leaves the lane and returns to it lower down, so its delay is as
            // visible as it is on any other arrow.
            let bulge = d.lane_pitch * 0.28;
            prims.push(Prim::Path {
                d: format!(
                    "M {} {} C {} {}, {} {}, {} {}",
                    n(x1),
                    n(y1),
                    n(x1 + bulge),
                    n(y1),
                    n(x1 + bulge),
                    n(y2),
                    n(x2),
                    n(y2)
                ),
                stroke: Some(stroke.clone()),
                fill: None,
                head,
                tag: tag.clone(),
            });
        } else {
            prims.push(Prim::Line { x1, y1, x2: ex, y2: ey, stroke: stroke.clone(), head, tag: tag.clone() });
        }
        if cross {
            prims.extend(mark_prims(Mark::Cross, ex, ey, 5.0, stroke.color, stroke.opacity, tag.clone()));
        }

        // An explicit label on the line wins; then the style sheet's default for the class; then
        // the class name as it stands. With no class at all the arrow is simply unlabelled, which
        // is what you want when every message in the picture is the same.
        let text = e
            .stated_label()
            .or_else(|| lk.props.label.clone())
            .or_else(|| e.classes.first().cloned());
        if let Some(text) = text {
            if !text.is_empty() {
                rails.push(Rail {
                    w: font.width(&text, LABEL_SIZE),
                    h: LABEL_SIZE * 1.15,
                    text,
                    a: (x1, y1),
                    b: (ex, ey),
                    f: 0.5,
                    pref: 0.5,
                    fill: lk.props.color.unwrap_or(pal.ink),
                    opacity: lk.props.opacity.unwrap_or(1.0),
                    tag,
                    rotate: !m.is_self(),
                });
            }
        }
    }

    // ---- points ----------------------------------------------------------
    for p in &run.points {
        let e = &doc.events[p.event];
        let mut classes = e.classes.clone();
        classes.push(p.subject.name().to_string());
        let states = vec![p.subject.pseudo().to_string()];
        let lk = look(Some(&p.kind), &classes, &states);
        let y = axis.y(p.at);
        let colour = lk.props.color.unwrap_or(pal.ink);
        let size = lk.props.size.unwrap_or(4.0);
        let opacity = lk.props.opacity.unwrap_or(1.0);
        let tag = Tag { event: Some(p.event), role: p.kind.clone(), detail: e.detail.clone() };

        let xs: Vec<f64> = match &p.subject {
            Subject::Node(n) => vec![x_of(n)],
            Subject::Network => {
                let named = e.lanes();
                if named.is_empty() {
                    vec![]
                } else {
                    named.iter().map(|n| x_of(n)).collect()
                }
            }
            Subject::Run => vec![],
        };

        if xs.is_empty() {
            // A point on the network or the run with no lanes named is a line across every lane.
            prims.push(Prim::Line {
                x1: MARGIN_L - 34.0,
                y1: y,
                x2: width - MARGIN_R + 34.0,
                y2: y,
                stroke: Stroke {
                    color: colour,
                    width: lk.props.width.unwrap_or(1.0),
                    dash: lk.props.dash.clone().unwrap_or(Dash::Pattern(4.0, 3.0)),
                    opacity,
                },
                head: false,
                tag: tag.clone(),
            });
        }
        for x in &xs {
            prims.extend(mark_prims(
                lk.props.mark.unwrap_or(Mark::Dot),
                *x,
                y,
                size,
                colour,
                opacity,
                tag.clone(),
            ));
        }

        let text =
            e.stated_label().or_else(|| lk.props.label.clone()).unwrap_or_else(|| p.kind.clone());
        if !text.is_empty() {
            let (tx, anchor, ty) = match xs.first() {
                Some(x) => (x + size + 6.0, Anchor::Start, y + font.cap_height(LABEL_SIZE) * 0.5),
                // A line across every lane carries its label above itself, not through it.
                None => (width - MARGIN_R + 30.0, Anchor::End, y - 5.0),
            };
            pins.push((tx, ty, anchor, text, colour, opacity, tag));
        }
    }

    // ---- point labels, dodged downward ------------------------------------
    //
    // Sorted first so the result does not depend on the order events happened to be written in,
    // then each pushed below any earlier one it would sit on. Downward rather than in both
    // directions because a label belongs to the instant it names, and moving it up would put it
    // above an event that had not happened yet.
    pins.sort_by(|a, b| {
        (a.0, a.1, &a.3).partial_cmp(&(b.0, b.1, &b.3)).unwrap_or(std::cmp::Ordering::Equal)
    });
    let line_h = LABEL_SIZE * 1.25;
    for i in 0..pins.len() {
        for j in 0..i {
            let same_column = (pins[i].0 - pins[j].0).abs() < 1.0 && pins[i].2 == pins[j].2;
            if same_column && (pins[i].1 - pins[j].1).abs() < line_h {
                pins[i].1 = pins[j].1 + line_h;
            }
        }
    }
    for (x, y, anchor, text, fill, opacity, tag) in pins {
        prims.push(Prim::Text {
            x,
            y,
            text,
            size: LABEL_SIZE,
            anchor,
            fill,
            opacity,
            rotate: 0.0,
            mono: false,
            tag,
        });
    }

    // ---- message labels, once everything they must avoid exists -----------
    //
    // Last, because a label can only be placed against obstacles that already have positions: the
    // lanes, and every point label now pinned beside its mark.
    let fixed: Vec<(f64, f64, f64, f64)> = prims
        .iter()
        .filter_map(|p| match p {
            Prim::Text { x, y, text, size, anchor, .. } => {
                let w = font.width(text, *size);
                let cx = match anchor {
                    Anchor::Start => x + w * 0.5,
                    Anchor::Middle => *x,
                    Anchor::End => x - w * 0.5,
                };
                Some((cx, y - size * 0.35, w, size * 1.15))
            }
            _ => None,
        })
        .collect();
    // The horizontal rules already drawn, so a label can be pushed off one. They run the other
    // way from the lanes, and a slanted rail changes a label's y as well as its x, so the same one
    // degree of freedom gets it clear of both.
    let rules: Vec<f64> = prims
        .iter()
        .filter_map(|p| match p {
            Prim::Line { y1, y2, x1, x2, .. }
                if (y1 - y2).abs() < 0.01 && (x2 - x1).abs() > 100.0 =>
            {
                Some(*y1)
            }
            _ => None,
        })
        .collect();

    if relax_labels {
        relax(&mut rails, &lanes, &rules, &fixed);
    }
    for r in &rails {
        let (x, y) = r.centre();
        prims.push(Prim::Text {
            x,
            y,
            text: r.text.clone(),
            size: LABEL_SIZE,
            anchor: Anchor::Middle,
            fill: r.fill,
            opacity: r.opacity,
            rotate: r.angle(),
            mono: false,
            tag: r.tag.clone(),
        });
    }

    Picture {
        width,
        height,
        background: pal.background,
        prims,
        title: doc.title.clone(),
        header: doc.header.clone(),
    }
}

fn gutter(prims: &mut Vec<Prim>, axis: &Axis3, d: &Diagram, pal: &Palette, width: f64) {
    let label = |prims: &mut Vec<Prim>, t: Tick, y: f64| {
        prims.push(Prim::Text {
            x: MARGIN_L - 42.0,
            y: y + 3.5,
            text: t.to_string(),
            size: GUTTER_SIZE,
            anchor: Anchor::End,
            fill: pal.faint,
            opacity: 1.0,
            rotate: 0.0,
            mono: true,
            tag: Tag { event: None, role: "tick".into(), detail: None },
        });
    };
    let rule = |prims: &mut Vec<Prim>, y: f64, dash: Dash| {
        prims.push(Prim::Line {
            x1: MARGIN_L - 34.0,
            y1: y,
            x2: width - MARGIN_R + 34.0,
            y2: y,
            stroke: Stroke { color: pal.rule, width: 1.0, dash, opacity: 0.55 },
            head: false,
            tag: Tag { event: None, role: "gridline".into(), detail: None },
        });
    };

    match axis.mode {
        Axis::Ordinal => {
            // Equal spacing costs the slope, so the numbers have to say what it cost: each row
            // carries its instant, and between two rows sits the time that actually elapsed.
            let all: Vec<(Tick, f64)> = axis.instants().collect();
            for (t, y) in &all {
                label(prims, *t, *y);
                rule(prims, *y, Dash::Pattern(1.0, 5.0));
            }
            for w in all.windows(2) {
                let (t0, y0) = w[0];
                let (t1, y1) = w[1];
                if t1 > t0 {
                    prims.push(Prim::Text {
                        x: MARGIN_L - 42.0,
                        y: (y0 + y1) * 0.5 + 3.0,
                        text: format!("+{}", t1 - t0),
                        size: GUTTER_SIZE - 1.5,
                        anchor: Anchor::End,
                        fill: pal.faint,
                        opacity: 0.7,
                        rotate: 0.0,
                        mono: true,
                        tag: Tag { event: None, role: "elapsed".into(), detail: None },
                    });
                }
            }
        }
        Axis::Compressed => {
            for (t, y) in axis.instants() {
                label(prims, t, y);
                let broken = axis.breaks.contains(&t);
                rule(prims, y, if broken { Dash::Pattern(6.0, 3.0) } else { Dash::Pattern(1.0, 5.0) });
            }
        }
        Axis::Linear => {
            // Round instants at a 1/2/5 step, so the gutter reads as a ruler rather than as a list
            // of whatever happened to happen.
            let all: Vec<(Tick, f64)> = axis.instants().collect();
            let (lo, hi) = match (all.first(), all.last()) {
                (Some((a, _)), Some((b, _))) => (*a, *b),
                _ => return,
            };
            let span = (hi - lo).max(1) as f64;
            let rough = span / 7.0;
            let mag = 10f64.powf(rough.log10().floor());
            let step = [1.0, 2.0, 5.0, 10.0]
                .iter()
                .map(|m| m * mag)
                .find(|s| *s >= rough)
                .unwrap_or(mag * 10.0)
                .max(1.0) as Tick;
            let mut t = (lo / step) * step;
            if t < lo {
                t += step;
            }
            while t <= hi {
                let y = axis.y(t);
                label(prims, t, y);
                rule(prims, y, Dash::Pattern(1.0, 5.0));
                t += step;
            }
            let _ = d;
        }
    }
}

/// A rectangle whose far end is torn, for a span that never closed.
fn ragged(x: f64, y: f64, w: f64, h: f64) -> String {
    let teeth = 4;
    let mut d = format!("M {} {} L {} {}", n(x), n(y), n(x + w), n(y));
    let bottom = y + h;
    d.push_str(&format!(" L {} {}", n(x + w), n(bottom - 4.0)));
    for i in 0..teeth {
        let fx = x + w - w * (i as f64 + 0.5) / teeth as f64;
        let fy = if i % 2 == 0 { bottom } else { bottom - 4.0 };
        d.push_str(&format!(" L {} {}", n(fx), n(fy)));
    }
    d.push_str(&format!(" L {} {} Z", n(x), n(bottom - 4.0)));
    d
}

/// One marker, as primitives. Every shape is distinguishable in monochrome, which is what makes
/// the encoding survive a printout.
fn mark_prims(
    mark: Mark,
    x: f64,
    y: f64,
    s: f64,
    colour: Color,
    opacity: f64,
    tag: Tag,
) -> Vec<Prim> {
    let fill = Some(Fill { color: colour, opacity });
    let stroke = Stroke { color: colour, width: 1.8, dash: Dash::Solid, opacity };
    match mark {
        Mark::None => vec![],
        Mark::Dot => vec![Prim::Circle { cx: x, cy: y, r: s, fill, stroke: None, tag }],
        Mark::Ring => vec![Prim::Circle {
            cx: x,
            cy: y,
            r: s,
            fill: None,
            stroke: Some(Stroke { width: 1.4, ..stroke }),
            tag,
        }],
        Mark::Square => vec![Prim::Rect {
            x: x - s,
            y: y - s,
            w: s * 2.0,
            h: s * 2.0,
            rx: 0.0,
            fill,
            stroke: None,
            tag,
        }],
        Mark::Diamond => vec![Prim::Polygon {
            points: vec![(x, y - s), (x + s, y), (x, y + s), (x - s, y)],
            fill,
            stroke: None,
            tag,
        }],
        Mark::Cross => vec![
            Prim::Line {
                x1: x - s,
                y1: y - s,
                x2: x + s,
                y2: y + s,
                stroke: stroke.clone(),
                head: false,
                tag: tag.clone(),
            },
            Prim::Line { x1: x + s, y1: y - s, x2: x - s, y2: y + s, stroke, head: false, tag },
        ],
        Mark::Bar => vec![Prim::Line {
            x1: x - s,
            y1: y,
            x2: x + s,
            y2: y,
            stroke: Stroke { width: 2.2, ..stroke },
            head: false,
            tag,
        }],
        // An arrowhead with no shaft behind it: something arrived from outside the diagram.
        Mark::Chevron => vec![Prim::Polygon {
            points: vec![(x - s * 1.8, y - s), (x - s * 0.2, y), (x - s * 1.8, y + s)],
            fill,
            stroke: None,
            tag,
        }],
    }
}
