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
/// Baseline-to-baseline room a gutter number needs. Below it the number is dropped rather than
/// printed over its neighbour.
const GUTTER_MIN_GAP: f64 = GUTTER_SIZE + 2.0;
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

/// Where a label may go, and how much room it has to move.
///
/// Both kinds have the same shape: one continuous degree of freedom, and one discrete choice of
/// side. That is what lets a message label and a label beside a mark relax against each other
/// rather than colliding freely because neither knows the other exists.
enum Anchorage {
    /// Along an arrow. The continuous freedom is how far along.
    Rail { a: (f64, f64), b: (f64, f64), f: f64 },
    /// Beside a mark on a lane. The continuous freedom is a small vertical nudge — small because
    /// a label belongs to the instant it names, and one that drifts stops naming it.
    Pin { x: f64, y: f64, dy: f64, gap: f64 },
}

/// How far a pinned label may slide from the instant it belongs to.
const PIN_SLACK: f64 = 15.0;
const MIN_F: f64 = 0.12;
const MAX_F: f64 = 0.88;

struct Placement {
    text: String,
    w: f64,
    h: f64,
    fill: Color,
    opacity: f64,
    tag: Tag,
    /// Ride the rail's angle rather than sitting upright.
    rotate: bool,
    /// Which side of the line, or of the lane. `+1` is the preferred one.
    side: f64,
    anchorage: Anchorage,
}

impl Placement {
    /// Centre of the label's box, which is what everything else is measured against.
    fn centre(&self) -> (f64, f64) {
        match &self.anchorage {
            Anchorage::Rail { a, b, f } => {
                let (x, y) = (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f);
                let (nx, ny) = self.normal();
                let off = self.h * 0.72;
                (x + nx * off * self.side, y + ny * off * self.side)
            }
            Anchorage::Pin { x, y, dy, gap } => {
                (x + self.side * (gap + self.w * 0.5), y + dy)
            }
        }
    }

    /// The perpendicular a rail label is offset along, always pointing *up* the page.
    ///
    /// Taking it straight from the direction vector is what used to put some labels above their
    /// arrow and some below: a leftward arrow flips the perpendicular. Pinning the sign to the
    /// page rather than to the arrow makes the default side the same everywhere, and `side` is
    /// then a decision rather than an accident.
    fn normal(&self) -> (f64, f64) {
        match &self.anchorage {
            Anchorage::Rail { a, b, .. } => {
                let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                let len = (dx * dx + dy * dy).sqrt().max(1e-6);
                let (nx, ny) = (-dy / len, dx / len);
                if ny > 0.0 {
                    (-nx, -ny)
                } else {
                    (nx, ny)
                }
            }
            _ => (0.0, -1.0),
        }
    }

    fn param(&self) -> f64 {
        match &self.anchorage {
            Anchorage::Rail { f, .. } => *f,
            Anchorage::Pin { dy, .. } => *dy,
        }
    }

    fn set_param(&mut self, v: f64) {
        match &mut self.anchorage {
            Anchorage::Rail { f, .. } => *f = v.clamp(MIN_F, MAX_F),
            Anchorage::Pin { dy, .. } => *dy = v.clamp(-PIN_SLACK, PIN_SLACK),
        }
    }

    /// Where it would rather be, and how hard it is pulled back there.
    fn home(&self) -> f64 {
        match &self.anchorage {
            Anchorage::Rail { .. } => 0.5,
            Anchorage::Pin { .. } => 0.0,
        }
    }

    fn angle(&self) -> f64 {
        if !self.rotate {
            return 0.0;
        }
        match &self.anchorage {
            Anchorage::Rail { a, b, .. } => {
                let mut deg = (b.1 - a.1).atan2(b.0 - a.0).to_degrees();
                // Keep text the right way up: an arrow pointing left would write upside down.
                if deg > 90.0 {
                    deg -= 180.0;
                } else if deg < -90.0 {
                    deg += 180.0;
                }
                deg
            }
            _ => 0.0,
        }
    }

    /// Where the text is drawn and how it is anchored, once a side has been settled on.
    fn text_at(&self) -> (f64, f64, Anchor) {
        match &self.anchorage {
            Anchorage::Rail { .. } => {
                let c = self.centre();
                (c.0, c.1, Anchor::Middle)
            }
            Anchorage::Pin { x, y, dy, gap } => {
                let anchor = if self.side > 0.0 { Anchor::Start } else { Anchor::End };
                (x + self.side * gap, y + dy, anchor)
            }
        }
    }
}

/// How much two boxes overlap, as a single number. Zero when they are clear of each other.
fn overlap(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> f64 {
    let ox = (a.2 + b.2) * 0.5 + 4.0 - (a.0 - b.0).abs();
    let oy = (a.3 + b.3) * 0.5 + 2.0 - (a.1 - b.1).abs();
    if ox > 0.0 && oy > 0.0 {
        ox.min(oy)
    } else {
        0.0
    }
}

struct Obstacles<'a> {
    lanes: &'a [f64],
    rules: &'a [f64],
    fixed: &'a [(f64, f64, f64, f64)],
    /// The page. A label pushed off the edge is not placed, it is lost, so leaving is expensive.
    left: f64,
    right: f64,
    /// The arrows themselves. The fourth kind of overlap, and the one most easily forgotten,
    /// because an arrow is not a box and so does not show up in a box-against-box test.
    segments: &'a [(f64, f64, f64, f64)],
}

/// Distance from a point to a line segment.
fn dist_to_segment(p: (f64, f64), s: (f64, f64, f64, f64)) -> f64 {
    let (dx, dy) = (s.2 - s.0, s.3 - s.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 <= 1e-9 {
        0.0
    } else {
        (((p.0 - s.0) * dx + (p.1 - s.1) * dy) / len2).clamp(0.0, 1.0)
    };
    let (cx, cy) = (s.0 + dx * t, s.1 + dy * t);
    ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt()
}

/// What one placement costs where it currently sits.
fn local_cost(i: usize, ps: &[Placement], obs: &Obstacles) -> f64 {
    let p = &ps[i];
    let c = p.centre();
    let me = (c.0, c.1, p.w, p.h);
    let mut cost = 0.0;

    for (j, q) in ps.iter().enumerate() {
        if j == i {
            continue;
        }
        let d = q.centre();
        cost += overlap(me, (d.0, d.1, q.w, q.h));
    }
    for f in obs.fixed {
        cost += overlap(me, *f);
    }
    for x in obs.lanes {
        // A label beside a mark is *meant* to sit near its own lane, so only the lanes it does not
        // belong to are in its way.
        if let Anchorage::Pin { x: own, .. } = &p.anchorage {
            if (own - x).abs() < 1.0 {
                continue;
            }
        }
        let over = p.w * 0.5 + 6.0 - (c.0 - x).abs();
        if over > 0.0 {
            cost += over * 0.6;
        }
    }
    for y in obs.rules {
        let over = p.h * 0.5 - (c.1 - y).abs();
        if over > 0.0 {
            cost += over * 0.5;
        }
    }
    // The arrows. A message label rides just clear of its own line by design, so the threshold is
    // under that offset and its own arrow does not push it away from where it belongs.
    let clear = p.h * 0.45;
    for seg in obs.segments {
        let d = dist_to_segment(c, *seg);
        if d < clear {
            cost += (clear - d) * 1.8;
        }
    }

    // Off the page is not a placement. Weighted heavily, because every other cost here is a
    // matter of degree and this one is a label the reader simply never sees.
    let (l, r) = (c.0 - p.w * 0.5, c.0 + p.w * 0.5);
    if l < obs.left {
        cost += (obs.left - l) * 4.0;
    }
    if r > obs.right {
        cost += (r - obs.right) * 4.0;
    }

    // Home, and a mild preference for the default side so nothing flips without a reason.
    // A pinned label is pulled home gently: it may move, that is the point of it, and a strong
    // spring would leave it sitting on an arrow rather than a few pixels from its mark. A rail
    // label is pulled harder, because the middle of an arrow is where a reader looks for its name.
    cost += (p.param() - p.home()).abs() * if matches!(p.anchorage, Anchorage::Pin { .. }) { 0.15 } else { 2.0 };
    if p.side < 0.0 {
        cost += 1.5;
    }
    cost
}

fn total_cost(ps: &[Placement], obs: &Obstacles) -> f64 {
    (0..ps.len()).map(|i| local_cost(i, ps, obs)).sum()
}

/// Settle every label into somewhere it can be read.
///
/// Each has one continuous degree of freedom — how far along its arrow, or a small vertical nudge
/// beside its mark — and one discrete one, which side. The continuous part is relaxed by gradient
/// steps; the discrete part by trying the flip and keeping it only if it helps. Both run in a fixed
/// order with no randomness, so the same document always settles the same way.
///
/// It keeps the best arrangement it has seen, *including the one it started from*. A relaxation
/// that cannot guarantee it improves on doing nothing should at least never make things worse — and
/// on a dense page the naive version genuinely does, by pushing one label into the space of another
/// it had not yet considered.
fn relax(
    ps: &mut [Placement],
    lanes: &[f64],
    rules: &[f64],
    fixed: &[(f64, f64, f64, f64)],
    bounds: (f64, f64),
    segments: &[(f64, f64, f64, f64)],
) {
    const PASSES: usize = 140;
    if ps.is_empty() {
        return;
    }
    let obs = Obstacles { lanes, rules, fixed, left: bounds.0, right: bounds.1, segments };

    let mut best: Vec<(f64, f64)> = ps.iter().map(|p| (p.param(), p.side)).collect();
    let mut best_cost = total_cost(ps, &obs);

    for pass in 0..PASSES {
        let step = (1.0 - pass as f64 / PASSES as f64).max(0.15);
        let mut moved = 0.0;

        // Gauss-Seidel: each label moves before the next is considered, so two cannot both step
        // into the same gap on the strength of one stale snapshot.
        for i in 0..ps.len() {
            // Discrete first: the side is a bigger move than a nudge, and choosing it badly makes
            // the nudge meaningless.
            let before = local_cost(i, ps, &obs);
            ps[i].side = -ps[i].side;
            if local_cost(i, ps, &obs) >= before {
                ps[i].side = -ps[i].side;
            }

            // Continuous: sample either way along the freedom and walk downhill. Cheaper to reason
            // about than an analytic gradient through the overlap terms, and it cannot disagree
            // with the cost it is minimising.
            let reach = match ps[i].anchorage {
                Anchorage::Rail { .. } => 0.08 * step,
                Anchorage::Pin { .. } => 5.0 * step,
            };
            let here = ps[i].param();
            let at = |ps: &mut [Placement], v: f64| {
                ps[i].set_param(v);
                local_cost(i, ps, &obs)
            };
            let c0 = at(ps, here);
            let cm = at(ps, here - reach);
            let cp = at(ps, here + reach);
            let pick = if cm < c0 && cm <= cp {
                here - reach
            } else if cp < c0 {
                here + reach
            } else {
                here
            };
            ps[i].set_param(pick);
            moved += (pick - here).abs();
        }

        let c = total_cost(ps, &obs);
        if c < best_cost {
            best_cost = c;
            best = ps.iter().map(|p| (p.param(), p.side)).collect();
        }
        if moved < 1e-4 {
            break;
        }
    }

    for (p, (v, side)) in ps.iter_mut().zip(best) {
        p.set_param(v);
        p.side = side;
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

    // A caption needs a line of its own above the lane headings, and nothing else moves when
    // there is none.
    let caption_h = if doc.title.is_some() { 26.0 } else { 0.0 };
    // Bodies print in the right margin, so the margin has to be wide enough for the widest line
    // of the widest one. Measured rather than guessed: a body is whatever the protocol put there.
    let body_w = if d.bodies {
        doc.events
            .iter()
            .filter_map(|e| e.detail.as_ref())
            .flat_map(|b| b.lines())
            .map(|l| font.width(l.trim(), LABEL_SIZE))
            .fold(0.0f64, f64::max)
    } else {
        0.0
    };
    let width = MARGIN_L
        + (doc.participants.len().max(1) as f64 - 1.0) * d.lane_pitch
        + MARGIN_R
        + if body_w > 0.0 { body_w + 24.0 } else { 0.0 };
    let top = HEADER;
    let bottom = axis.height();
    let height = bottom + FOOTER;

    let mut prims: Vec<Prim> = Vec::new();
    // Every label, of either sort, in one set — so a message label and one beside a mark
    // relax against each other rather than colliding freely because neither knows the
    // other exists.
    let mut labels: Vec<Placement> = Vec::new();
    // Marks whose shape depends on which side their label settled on, drawn after placement.
    let mut sided: Vec<(usize, Mark, f64, f64, f64, Color, f64, Tag)> = Vec::new();

    if let Some(t) = &doc.title {
        prims.push(Prim::Text {
            x: MARGIN_L - 34.0,
            y: 22.0,
            text: t.clone(),
            size: 15.0,
            anchor: Anchor::Start,
            fill: pal.ink,
            opacity: 1.0,
            rotate: 0.0,
            mono: false,
            tag: Tag { event: None, role: "title".into(), detail: None },
        });
    }





    let look = |kind: Option<&str>, classes: &[String], states: &[String]| Look {
        props: sheet.resolve(kind, classes, states),
    };

    // ---- the time gutter -------------------------------------------------
    // Rules span the lanes, not the page: with bodies on, the page is wider than the plot.
    let plot_right = MARGIN_L
        + (doc.participants.len().max(1) as f64 - 1.0) * d.lane_pitch
        + 34.0;
    gutter(&mut prims, &axis, d, &pal, plot_right);

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
                // Space-separated rather than comma-separated. SVG accepts both, and commas
                // would make the coordinates unreadable to anything that walks the tokens —
                // including the shift that moves a picture down to make room for a caption.
                d: format!(
                    "M {} {} C {} {} {} {} {} {}",
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
            prims.extend(mark_prims(Mark::Cross, ex, ey, 5.0, stroke.color, stroke.opacity, 1.0, tag.clone()));
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
                labels.push(Placement {
                    w: font.width(&text, LABEL_SIZE),
                    h: LABEL_SIZE * 1.15,
                    text,
                    fill: lk.props.color.unwrap_or(pal.ink),
                    opacity: lk.props.opacity.unwrap_or(1.0),
                    tag,
                    rotate: !m.is_self(),
                    side: 1.0,
                    anchorage: Anchorage::Rail { a: (x1, y1), b: (ex, ey), f: 0.5 },
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
                x2: plot_right,
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
        let mark = lk.props.mark.unwrap_or(Mark::Dot);
        let text =
            e.stated_label().or_else(|| lk.props.label.clone()).unwrap_or_else(|| p.kind.clone());

        for (k, x) in xs.iter().enumerate() {
            // The label goes in the pool and the mark waits, because a mark that faces anywhere
            // should face the same way its label went.
            if k == 0 && !text.is_empty() {
                sided.push((labels.len(), mark, *x, y, size, colour, opacity, tag.clone()));
                labels.push(Placement {
                    w: font.width(&text, LABEL_SIZE),
                    h: LABEL_SIZE * 1.15,
                    text: text.clone(),
                    fill: colour,
                    opacity,
                    tag: tag.clone(),
                    rotate: false,
                    side: 1.0,
                    anchorage: Anchorage::Pin {
                        x: *x,
                        y: y + font.cap_height(LABEL_SIZE) * 0.5,
                        dy: 0.0,
                        gap: mark.reach(size),
                    },
                });
            } else {
                prims.extend(mark_prims(mark, *x, y, size, colour, opacity, 1.0, tag.clone()));
            }
        }

        // A line across every lane carries its label above itself, not through it.
        if xs.is_empty() && !text.is_empty() {
            prims.push(Prim::Text {
                x: plot_right - 4.0,
                y: y - 5.0,
                text,
                size: LABEL_SIZE,
                anchor: Anchor::End,
                fill: colour,
                opacity,
                rotate: 0.0,
                mono: false,
                tag,
            });
        }
    }

    // ---- detail blocks, when the style sheet asks for them ----------------
    //
    // Off by default because a body is usually several lines of JSON and a page of them buries the
    // diagram. On, they are printed in the right margin beside the instant they belong to, which is
    // the only place on a space-time diagram with room for several lines.
    if d.bodies {
        for (i, e) in doc.events.iter().enumerate() {
            let Some(body) = &e.detail else { continue };
            let y = axis.y(e.time);
            for (k, line) in body.lines().enumerate() {
                prims.push(Prim::Text {
                    x: plot_right + 16.0,
                    y: y + font.cap_height(LABEL_SIZE) * 0.5 + k as f64 * LABEL_SIZE * 1.15,
                    text: line.trim().to_string(),
                    size: LABEL_SIZE,
                    anchor: Anchor::Start,
                    fill: pal.faint,
                    opacity: 1.0,
                    rotate: 0.0,
                    mono: false,
                    tag: Tag { event: Some(i), role: "body".into(), detail: Some(body.clone()) },
                });
            }
        }
    }

    // ---- every label, settled together -----------------------------------
    //
    // Last, because a label can only be placed against obstacles that already have positions: the
    // lanes, the gridlines, and the text that cannot move.
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
        // Every arrow already drawn, so a label can be pushed off one.
        let segments: Vec<(f64, f64, f64, f64)> = prims
            .iter()
            .filter_map(|p| match p {
                Prim::Line { x1, y1, x2, y2, .. }
                    if (x2 - x1).abs() > 1.0 && (y2 - y1).abs() > 1.0 =>
                {
                    Some((*x1, *y1, *x2, *y2))
                }
                _ => None,
            })
            .collect();
        relax(&mut labels, &lanes, &rules, &fixed, (4.0, plot_right), &segments);
    }

    // Marks that face somewhere, now that their labels have chosen a side.
    for (i, mark, x, y, size, colour, opacity, tag) in sided {
        prims.extend(mark_prims(mark, x, y, size, colour, opacity, labels[i].side, tag));
    }

    for l in &labels {
        let (x, y, anchor) = l.text_at();
        prims.push(Prim::Text {
            x,
            y,
            text: l.text.clone(),
            size: LABEL_SIZE,
            anchor,
            fill: l.fill,
            opacity: l.opacity,
            rotate: l.angle(),
            mono: false,
            tag: l.tag.clone(),
        });
    }

    // Everything was laid out as though there were no caption; shifting once here is simpler than
    // threading an offset through every placement, and cannot be got wrong in one place only.
    if caption_h > 0.0 {
        for p in &mut prims {
            shift(p, caption_h);
        }
    }

    Picture {
        width,
        height: height + caption_h,
        background: pal.background,
        prims,
        title: doc.title.clone(),
        header: doc.header.clone(),
    }
}

/// Move one primitive down the page. The caption is the only thing that needs this, and it needs
/// it after everything else has been placed.
fn shift(p: &mut Prim, dy: f64) {
    match p {
        Prim::Line { y1, y2, .. } => {
            *y1 += dy;
            *y2 += dy;
        }
        Prim::Rect { y, .. } | Prim::Text { y, .. } => *y += dy,
        Prim::Circle { cy, .. } => *cy += dy,
        Prim::Polygon { points, .. } => {
            for (_, y) in points.iter_mut() {
                *y += dy;
            }
        }
        Prim::Path { d, .. } => {
            // Path data is already absolute and written as `CMD x y x y …`, so every second
            // number after a command letter is a y. A command resets the pairing.
            let mut seen = 0usize;
            let rebuilt: Vec<String> = d
                .split_whitespace()
                .map(|part| match part.parse::<f64>() {
                    Ok(v) => {
                        seen += 1;
                        crate::picture::n(if seen % 2 == 0 { v + dy } else { v })
                    }
                    Err(_) => {
                        seen = 0;
                        part.to_string()
                    }
                })
                .collect();
            *d = rebuilt.join(" ");
        }
    }
}

fn gutter(prims: &mut Vec<Prim>, axis: &Axis3, d: &Diagram, pal: &Palette, plot_right: f64) {
    // Two numbers closer together than the type is tall print on top of each other, and a run with
    // several events inside a few ticks turns its own gutter into a grey smear. The gridline still
    // marks every instant; only the number goes, and only where there was no room for it. First of
    // a cluster wins, so the choice is the reading order rather than an accident of iteration.
    let mut last_y: Option<f64> = None;
    let mut label = |prims: &mut Vec<Prim>, t: Tick, y: f64| {
        if let Some(prev) = last_y {
            if (y - prev).abs() < GUTTER_MIN_GAP {
                return;
            }
        }
        last_y = Some(y);
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
            x2: plot_right,
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
                if t1 > t0 && (y1 - y0).abs() >= GUTTER_MIN_GAP * 2.0 {
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
    // Which way the mark faces, for the ones that face anywhere. `+1` is from the right.
    side: f64,
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
        // An arrowhead with no shaft behind it.
        Mark::Chevron => vec![Prim::Polygon {
            points: vec![
                (x + side * s * 1.8, y - s),
                (x + side * s * 0.2, y),
                (x + side * s * 1.8, y + s),
            ],
            fill,
            stroke: None,
            tag,
        }],
        // A short oblique arrow into the lane, shaft and all. It comes in from the side the label
        // settled on, so the two read as one thing rather than as a mark and a caption that
        // happen to be near each other.
        Mark::Arrow => {
            let (dx, dy) = (side * s * 4.6, -s * 2.4);
            vec![Prim::Line {
                x1: x + dx,
                y1: y + dy,
                x2: x + side * s * 0.6,
                y2: y - s * 0.3,
                stroke: Stroke { width: 1.6, ..stroke },
                head: true,
                tag,
            }]
        }
    }
}
