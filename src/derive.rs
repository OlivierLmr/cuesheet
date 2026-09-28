//! What the document did not say.
//!
//! Every subject has a **liveness timeline**, built from its own events: taking part until a
//! `kills`, not taking part until a `revives`, and so on. That timeline is the entire derivation.
//!
//! A message is then drawn by three readings of it — its sender at the moment it departs, its
//! receiver at the moment it arrives, and the run at that same moment. Whichever is not taking
//! part names the picture.
//!
//! Nothing else derives anything. A pause, a partition and a timer are pictures: the document
//! already carries the arrival that actually happened, so there is nothing left to recompute.

use crate::model::*;
use std::collections::BTreeMap;

/// A subject's participation over time. Transitions are sorted and de-duplicated; the value is
/// whether the subject takes part **from that tick onward**.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Liveness {
    transitions: Vec<(Tick, bool)>,
}

impl Liveness {
    /// Whether the subject takes part at `t`.
    ///
    /// A `kills` takes effect *at* its own instant, not after it: a message landing on the same
    /// tick as the crash dies. The alternative — alive for one last instant — would make the
    /// picture depend on which of two simultaneous events the reader imagined happening first,
    /// which is exactly the ambiguity a logical clock exists to remove.
    pub fn at(&self, t: Tick) -> bool {
        let mut alive = true;
        for (tick, state) in &self.transitions {
            if *tick <= t {
                alive = *state;
            } else {
                break;
            }
        }
        alive
    }

    /// The first instant at or after `from` when the subject stops taking part, if it ever does.
    pub fn stops_after(&self, from: Tick) -> Option<Tick> {
        self.transitions.iter().find(|(t, alive)| !*alive && *t >= from).map(|(t, _)| *t)
    }

    fn push(&mut self, t: Tick, alive: bool) {
        self.transitions.push((t, alive));
        self.transitions.sort_by_key(|(t, _)| *t);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fate {
    Arrived,
    /// Its sender was not taking part when it was due out.
    NeverLeft,
    /// Its receiver was not taking part when it landed.
    DiedAtLifeline,
    /// The run stopped before it landed.
    InFlight,
    /// Written `-x`: the network ate it.
    Eaten,
}

impl Fate {
    /// The pseudo-class a selector reaches this fate by.
    pub fn state(&self) -> &'static str {
        match self {
            Fate::Arrived => "arrived",
            Fate::NeverLeft => "never-left",
            Fate::DiedAtLifeline => "died",
            Fate::InFlight => "in-flight",
            Fate::Eaten => "eaten",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub from: String,
    pub to: String,
    pub depart: Tick,
    /// Where the head of the arrow belongs. For a fate that never got there this is where it
    /// *would* have, so the layout still has two points to draw between.
    pub arrive: Tick,
    pub fate: Fate,
    pub event: usize,
}

impl Message {
    pub fn is_self(&self) -> bool {
        self.from == self.to
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub subject: Subject,
    pub kind: String,
    pub start: Tick,
    pub end: Tick,
    /// It never closed — no instant, and no partner ever arrived. Drawn with a ragged edge,
    /// because a process dying inside its critical section is the thing worth seeing.
    pub open: bool,
    pub event: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Point {
    pub subject: Subject,
    pub kind: String,
    pub at: Tick,
    pub event: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub messages: Vec<Message>,
    pub spans: Vec<Span>,
    pub points: Vec<Point>,
    pub liveness: BTreeMap<Subject, Liveness>,
    /// Where the axis stops.
    pub end: Tick,
    /// The first and last instants anything happens at.
    pub first: Tick,
}

/// Build the liveness timelines, then read every message and span against them.
pub fn run(doc: &Document, sheet: &StyleSheet) -> Run {
    let mut liveness: BTreeMap<Subject, Liveness> = BTreeMap::new();
    for e in &doc.events {
        if let Body::Named(k) = &e.body {
            if let Some(decl) = sheet.kinds.get(k) {
                if decl.kills {
                    liveness.entry(e.subject.clone()).or_default().push(e.time, false);
                } else if decl.revives {
                    liveness.entry(e.subject.clone()).or_default().push(e.time, true);
                }
            }
        }
    }

    let empty = Liveness::default();
    let alive = |liveness: &BTreeMap<Subject, Liveness>, s: &Subject, t: Tick| -> bool {
        liveness.get(s).unwrap_or(&empty).at(t)
    };

    // Where the axis stops. An explicit `kills` on `run` is the end; without one, the last instant
    // anything reaches — so a document with no `run end` has nothing in flight, which is right.
    let explicit_end = liveness.get(&Subject::Run).and_then(|l| l.stops_after(Tick::MIN));
    let mut last = Tick::MIN;
    let mut first = Tick::MAX;
    for e in &doc.events {
        first = first.min(e.time);
        last = last.max(e.time);
        if let Some(i) = e.instant {
            last = last.max(i.resolve(e.time));
        }
    }
    if doc.events.is_empty() {
        first = 0;
        last = 0;
    }
    let end = explicit_end.unwrap_or(last);

    let mut messages = Vec::new();
    let mut points = Vec::new();
    let mut spans = Vec::new();
    // Open spans, FIFO per (subject, kind) — the same rule messages would use if they needed one.
    let mut pending: BTreeMap<(Subject, String), Vec<(Tick, usize)>> = BTreeMap::new();

    for (idx, e) in doc.events.iter().enumerate() {
        match &e.body {
            Body::Message { target, eaten } => {
                let depart = e.time;
                let arrive = e.instant.map(|i| i.resolve(e.time)).unwrap_or(e.time);
                let from = e.subject.name().to_string();
                let fate = if *eaten {
                    Fate::Eaten
                } else if !alive(&liveness, &e.subject, depart) {
                    // Never left takes precedence: a message its sender never got out did not
                    // exist to arrive anywhere.
                    Fate::NeverLeft
                } else if !alive(&liveness, &Subject::Node(target.clone()), arrive) {
                    Fate::DiedAtLifeline
                } else if arrive > end {
                    Fate::InFlight
                } else {
                    Fate::Arrived
                };
                messages.push(Message {
                    from,
                    to: target.clone(),
                    depart,
                    arrive,
                    fate,
                    event: idx,
                });
            }
            Body::Named(kind) => {
                let decl = sheet.kinds.get(kind);
                let is_span_opener =
                    decl.map(|d| d.category == Category::Span).unwrap_or(false);

                if is_span_opener {
                    let decl = decl.unwrap();
                    if let Some(i) = e.instant {
                        // Closed on the line.
                        spans.push(Span {
                            subject: e.subject.clone(),
                            kind: kind.clone(),
                            start: e.time,
                            end: i.resolve(e.time),
                            open: false,
                            event: idx,
                        });
                    } else if decl.closes.is_some() {
                        pending
                            .entry((e.subject.clone(), kind.clone()))
                            .or_default()
                            .push((e.time, idx));
                    } else {
                        // One name, no instant: it runs until its subject or the run stops.
                        spans.push(Span {
                            subject: e.subject.clone(),
                            kind: kind.clone(),
                            start: e.time,
                            end: open_end(&liveness, &e.subject, e.time, end),
                            open: true,
                            event: idx,
                        });
                    }
                    continue;
                }

                // The closing half of a span is not an event in its own right — unless it is
                // orphaned, in which case it becomes a bare point. Erroring instead would make a
                // legitimate trace unrepresentable: a node can crash mid-span, revive, and then
                // emit the close it owed.
                if let Some(opener) = opener_for(sheet, kind) {
                    let key = (e.subject.clone(), opener.to_string());
                    if let Some(q) = pending.get_mut(&key) {
                        if !q.is_empty() {
                            let (start, opened_at) = q.remove(0);
                            // A span cannot outlive its subject: if the subject stopped taking part
                            // between the two events, the span ended there and this close is late.
                            let cut = liveness
                                .get(&e.subject)
                                .and_then(|l| l.stops_after(start))
                                .filter(|t| *t < e.time);
                            match cut {
                                Some(t) => {
                                    spans.push(Span {
                                        subject: e.subject.clone(),
                                        kind: opener.to_string(),
                                        start,
                                        end: t.min(end),
                                        open: true,
                                        event: opened_at,
                                    });
                                    points.push(Point {
                                        subject: e.subject.clone(),
                                        kind: kind.clone(),
                                        at: e.time,
                                        event: idx,
                                    });
                                }
                                None => spans.push(Span {
                                    subject: e.subject.clone(),
                                    kind: opener.to_string(),
                                    start,
                                    end: e.time,
                                    open: false,
                                    event: opened_at,
                                }),
                            }
                            continue;
                        }
                    }
                }

                points.push(Point {
                    subject: e.subject.clone(),
                    kind: kind.clone(),
                    at: e.time,
                    event: idx,
                });
            }
        }
    }

    // Anything still pending never closed.
    for ((subject, kind), q) in pending {
        for (start, idx) in q {
            spans.push(Span {
                subject: subject.clone(),
                kind: kind.clone(),
                start,
                end: open_end(&liveness, &subject, start, end),
                open: true,
                event: idx,
            });
        }
    }

    // Deterministic order: by the line each came from, so two runs of the same input agree.
    messages.sort_by_key(|m| m.event);
    spans.sort_by_key(|s| s.event);
    points.sort_by_key(|p| p.event);

    Run { messages, spans, points, liveness, end, first }
}

/// Where a span with no close stops: its subject's next departure, or the end of the run.
fn open_end(
    liveness: &BTreeMap<Subject, Liveness>,
    subject: &Subject,
    start: Tick,
    run_end: Tick,
) -> Tick {
    liveness
        .get(subject)
        .and_then(|l| l.stops_after(start))
        .map(|t| t.min(run_end))
        .unwrap_or(run_end)
}

/// Which declared span, if any, this name closes.
fn opener_for<'a>(sheet: &'a StyleSheet, closer: &str) -> Option<&'a str> {
    sheet
        .kinds
        .values()
        .find(|k| k.closes.as_deref() == Some(closer))
        .map(|k| k.name.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    const SHEET: &str = "\
kind point deliver
kind point crash kills
kind point died kills
kind point recover revives
kind point end kills
kind span enter_cs exit_cs
kind span paused
";

    fn go(doc_src: &str) -> Run {
        let doc = parse::document(doc_src).unwrap();
        let sheet = parse::stylesheet(SHEET).unwrap();
        run(&doc, &sheet)
    }

    fn fates(doc_src: &str) -> Vec<Fate> {
        go(doc_src).messages.iter().map(|m| m.fate).collect()
    }

    // ---- liveness ---------------------------------------------------------

    #[test]
    fn a_subject_takes_part_until_it_is_killed_and_again_once_revived() {
        let r = go("100 n0 crash\n200 n0 recover\n300 n0 crash");
        let l = &r.liveness[&Subject::Node("n0".into())];
        assert!(l.at(99));
        assert!(!l.at(100), "a kills takes effect at its own instant");
        assert!(!l.at(150));
        assert!(l.at(200));
        assert!(l.at(250));
        assert!(!l.at(300));
    }

    #[test]
    fn a_subject_with_no_events_takes_part_throughout() {
        let r = go("0 n0 deliver");
        assert!(r.liveness.get(&Subject::Node("n9".into())).is_none());
        assert!(Liveness::default().at(0));
        assert!(Liveness::default().at(99999));
    }

    // ---- the fate cross product ------------------------------------------
    //
    // {sender alive, dead, revived} × {receiver alive, dead, revived} × {run alive, ended}
    // × {arrow, -x}. Every cell below is one row of that table.

    #[test]
    fn a_message_between_two_live_nodes_arrives() {
        assert_eq!(fates("0 n0 -> n1 .a +10"), vec![Fate::Arrived]);
    }

    #[test]
    fn a_message_from_a_dead_sender_never_left() {
        assert_eq!(fates("100 n0 crash\n159 n0 -> n2 .a +20"), vec![Fate::NeverLeft]);
    }

    #[test]
    fn a_message_to_a_dead_receiver_died_at_the_lifeline() {
        assert_eq!(fates("110 n1 crash\n90 n0 -> n1 .a +30"), vec![Fate::DiedAtLifeline]);
    }

    #[test]
    fn never_left_beats_died_at_the_lifeline_when_both_ends_are_dead() {
        // A message its sender never got out did not exist to arrive anywhere.
        let f = fates("10 n0 crash\n10 n1 crash\n20 n0 -> n1 .a +5");
        assert_eq!(f, vec![Fate::NeverLeft]);
    }

    #[test]
    fn a_message_arriving_after_the_run_ends_is_still_in_flight() {
        assert_eq!(fates("150 n0 -> n3 .a +40\n170 run end"), vec![Fate::InFlight]);
    }

    #[test]
    fn a_message_a_revived_sender_sends_leaves_normally() {
        assert_eq!(
            fates("100 n0 crash\n200 n0 recover\n210 n0 -> n1 .a +5"),
            vec![Fate::Arrived]
        );
    }

    #[test]
    fn a_message_to_a_revived_receiver_arrives() {
        assert_eq!(
            fates("100 n1 crash\n200 n1 recover\n210 n0 -> n1 .a +5"),
            vec![Fate::Arrived]
        );
    }

    #[test]
    fn a_message_landing_in_the_window_between_a_crash_and_a_revive_dies() {
        assert_eq!(
            fates("100 n1 crash\n200 n1 recover\n150 n0 -> n1 .a +5"),
            vec![Fate::DiedAtLifeline]
        );
    }

    #[test]
    fn an_eaten_message_is_eaten_whatever_its_ends_are_doing() {
        assert_eq!(fates("20 n1 -x n2 .a"), vec![Fate::Eaten]);
        assert_eq!(fates("10 n1 crash\n20 n1 -x n2 .a"), vec![Fate::Eaten]);
        assert_eq!(fates("10 n2 crash\n20 n1 -x n2 .a"), vec![Fate::Eaten]);
    }

    #[test]
    fn a_message_landing_exactly_on_the_crash_dies() {
        assert_eq!(fates("110 n1 crash\n100 n0 -> n1 .a +10"), vec![Fate::DiedAtLifeline]);
    }

    #[test]
    fn a_message_leaving_exactly_on_its_senders_crash_never_leaves() {
        assert_eq!(fates("110 n0 crash\n110 n0 -> n1 .a +10"), vec![Fate::NeverLeft]);
    }

    #[test]
    fn a_message_arriving_exactly_as_the_run_ends_still_arrived() {
        assert_eq!(fates("100 n0 -> n1 .a +70\n170 run end"), vec![Fate::Arrived]);
    }

    #[test]
    fn a_self_addressed_message_is_read_against_its_one_lane_twice() {
        let r = go("100 n0 -> n0 .retry +20");
        assert!(r.messages[0].is_self());
        assert_eq!(r.messages[0].fate, Fate::Arrived);
        let r = go("110 n0 crash\n100 n0 -> n0 .retry +20");
        assert_eq!(r.messages[0].fate, Fate::DiedAtLifeline);
    }

    // ---- spans ------------------------------------------------------------

    #[test]
    fn a_span_closes_on_its_own_instant() {
        let s = &go("200 n1 enter_cs +50").spans[0];
        assert_eq!((s.start, s.end, s.open), (200, 250, false));
    }

    #[test]
    fn a_span_closes_on_its_partner() {
        let s = &go("200 n1 enter_cs\n250 n1 exit_cs").spans[0];
        assert_eq!((s.start, s.end, s.open), (200, 250, false));
    }

    #[test]
    fn a_span_with_no_partner_runs_open_to_the_end_of_the_run() {
        let r = go("200 n1 enter_cs\n377 run end");
        assert_eq!((r.spans[0].start, r.spans[0].end, r.spans[0].open), (200, 377, true));
    }

    /// A process dying inside its critical section is the thing these diagrams exist to show.
    #[test]
    fn a_span_open_when_its_subject_dies_stops_there_and_is_marked_open() {
        let r = go("100 n1 enter_cs\n110 n1 crash\n377 run end");
        let s = &r.spans[0];
        assert_eq!((s.start, s.end, s.open), (100, 110, true));
    }

    #[test]
    fn a_one_name_span_needs_no_partner_to_be_well_formed() {
        let s = &go("5 n1 paused +200").spans[0];
        assert_eq!((s.start, s.end, s.open), (5, 205, false));
    }

    #[test]
    fn spans_of_one_kind_on_one_subject_pair_oldest_first() {
        let r = go("10 n1 enter_cs\n20 n1 enter_cs\n30 n1 exit_cs\n40 n1 exit_cs");
        let mut got: Vec<(Tick, Tick)> = r.spans.iter().map(|s| (s.start, s.end)).collect();
        got.sort();
        assert_eq!(got, vec![(10, 30), (20, 40)]);
    }

    /// The case the design left open, decided here: the span ended at the death, and the close
    /// that arrives after a revive is a bare point rather than an error — refusing it would make a
    /// legitimate trace unrepresentable.
    #[test]
    fn a_close_arriving_after_a_crash_and_revive_leaves_a_bare_point() {
        let r = go("100 n1 enter_cs\n110 n1 crash\n200 n1 recover\n250 n1 exit_cs\n300 run end");
        assert_eq!(r.spans.len(), 1);
        let s = &r.spans[0];
        assert_eq!((s.start, s.end, s.open), (100, 110, true));
        assert!(
            r.points.iter().any(|p| p.kind == "exit_cs" && p.at == 250),
            "the orphaned close should survive as a point: {:?}",
            r.points
        );
    }

    #[test]
    fn a_close_with_nothing_open_is_a_bare_point() {
        let r = go("250 n1 exit_cs");
        assert!(r.spans.is_empty());
        assert_eq!(r.points[0].kind, "exit_cs");
    }

    #[test]
    fn a_closing_event_that_did_close_something_draws_no_point_of_its_own() {
        let r = go("200 n1 enter_cs\n250 n1 exit_cs");
        assert_eq!(r.spans.len(), 1);
        assert!(r.points.is_empty(), "{:?}", r.points);
    }

    // ---- points and the axis ---------------------------------------------

    #[test]
    fn an_undeclared_kind_is_a_point() {
        let r = go("10 n0 something_nobody_declared");
        assert_eq!(r.points[0].kind, "something_nobody_declared");
    }

    #[test]
    fn the_axis_stops_at_an_explicit_end_and_otherwise_at_the_last_instant() {
        assert_eq!(go("0 n0 -> n1 .a +10\n170 run end").end, 170);
        assert_eq!(go("0 n0 -> n1 .a +10").end, 10);
        assert_eq!(go("0 n0 deliver\n5 n1 deliver").end, 5);
    }

    #[test]
    fn a_run_with_no_declared_end_has_nothing_in_flight() {
        assert_eq!(fates("0 n0 -> n1 .a +10"), vec![Fate::Arrived]);
    }

    // ---- the properties the plan asks for --------------------------------

    /// Running the derivation on its own output changes nothing, which is what lets a hand-written
    /// document and a generated one mean the same thing.
    #[test]
    fn the_derivation_is_idempotent() {
        use crate::print;
        let src = "\
0 n0 -> n1 .rb +10
90 n0 -> n1 .rb +30
110 n1 crash
130 n1 -> n2 .rb +10
150 n0 -> n3 .rb +40
100 n1 enter_cs
170 run end
";
        let doc = parse::document(src).unwrap();
        let sheet = parse::stylesheet(SHEET).unwrap();
        let once = run(&doc, &sheet);

        let printed = print::document(&doc, print::Align::Off);
        let doc2 = parse::document(&printed).unwrap();
        let twice = run(&doc2, &sheet);
        assert_eq!(once, twice);
    }

    /// The property that matters most: no message is ever drawn arriving at a lifeline that is
    /// already dead. Swept over a range of crash instants rather than asserted once.
    #[test]
    fn no_message_is_ever_drawn_arriving_at_a_dead_lifeline() {
        for crash in 0..60 {
            for depart in 0..40 {
                for flight in 1..20 {
                    let src = format!(
                        "{crash} n1 crash\n{depart} n0 -> n1 .a +{flight}\n100 run end"
                    );
                    let r = go(&src);
                    let m = &r.messages[0];
                    if m.fate == Fate::Arrived {
                        assert!(
                            r.liveness[&Subject::Node("n1".into())].at(m.arrive),
                            "arrived at a dead lifeline: crash={crash} depart={depart} flight={flight}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_node_that_crashes_revives_and_crashes_again_is_dead_in_exactly_two_stretches() {
        let r = go("10 n0 crash\n20 n0 recover\n30 n0 crash\n40 n0 recover\n50 run end");
        let l = &r.liveness[&Subject::Node("n0".into())];
        let dead: Vec<Tick> = (0..=50).filter(|t| !l.at(*t)).collect();
        let mut stretches = 0;
        let mut prev = None;
        for t in &dead {
            if prev.map(|p| t - p > 1).unwrap_or(true) {
                stretches += 1;
            }
            prev = Some(*t);
        }
        assert_eq!(stretches, 2, "dead at {dead:?}");
    }
}
