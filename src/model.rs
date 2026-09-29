//! The types the whole pipeline agrees on.
//!
//! Two files feed it. A **document** states facts: who took part, and what happened when. A
//! **style sheet** says what the names in that document *are* — which of two structures, and
//! whether they end a subject's participation — and how they look. Nothing here knows what a
//! critical section or a partition means, and it must stay that way: the moment a type names a
//! domain concept, the tool starts claiming to understand something it cannot observe.

use std::collections::BTreeMap;

pub type Tick = i64;

/// A second time on a line, always spelled relative or absolute. There is no such thing as a
/// duration: `paused +200` means *until* 200 after this line, which is the same idea as a message
/// arriving, so both use this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instant {
    After(Tick),
    At(Tick),
}

impl Instant {
    /// Resolve against the instant the line itself carries.
    pub fn resolve(self, now: Tick) -> Tick {
        match self {
            Instant::After(d) => now + d,
            Instant::At(t) => t,
        }
    }
}

/// Who a statement is about. The subject decides where a thing is drawn, and how the bare words
/// after its kind are read.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Subject {
    Node(String),
    Network,
    Run,
}

impl Subject {
    pub fn name(&self) -> &str {
        match self {
            Subject::Node(n) => n,
            Subject::Network => "network",
            Subject::Run => "run",
        }
    }

    /// The pseudo-class a selector uses to reach this sort of subject — `:node`, `:network`,
    /// `:run`. A style sheet has no business naming `n0`, so this is how the two cases of a split
    /// are told apart.
    pub fn pseudo(&self) -> &'static str {
        match self {
            Subject::Node(_) => "node",
            Subject::Network => "network",
            Subject::Run => "run",
        }
    }

    pub fn parse(word: &str) -> Subject {
        match word {
            "network" => Subject::Network,
            "run" => Subject::Run,
            other => Subject::Node(other.to_string()),
        }
    }
}

/// The two structures. A `Point` happens at an instant; a `Span` lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Point,
    Span,
}

/// What a name in a document *is*. Declared in the style sheet, because the language ships no
/// vocabulary of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KindDecl {
    pub category: Category,
    pub name: String,
    /// A span declared with a second name is closed by that event rather than by an instant.
    pub closes: Option<String>,
    /// Its subject stops taking part.
    pub kills: bool,
    /// Its subject takes part again.
    pub revives: bool,
}

/// What a line says happened.
#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    /// `n0 -> n1 …` or `n0 -x n1 …`
    Message { target: String, eaten: bool },
    /// Any declared name.
    Named(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub time: Tick,
    pub subject: Subject,
    pub body: Body,
    /// Bare words after the kind: lane names on `network`, otherwise the label.
    pub words: Vec<String>,
    /// A quoted string is always the label, on any subject.
    pub label: Option<String>,
    pub classes: Vec<String>,
    pub instant: Option<Instant>,
    /// Opaque; the parser finds its end and never reads inside.
    pub detail: Option<String>,
    pub line: usize,
}

impl Event {
    /// The lanes a `network` statement covers. Empty means all of them.
    pub fn lanes(&self) -> &[String] {
        match self.subject {
            Subject::Network => &self.words,
            _ => &[],
        }
    }

    /// The label written on the line, before the style sheet gets a say.
    pub fn stated_label(&self) -> Option<String> {
        if let Some(l) = &self.label {
            return Some(l.clone());
        }
        match self.subject {
            Subject::Network => None,
            _ if !self.words.is_empty() => Some(self.words.join(" ")),
            _ => None,
        }
    }

    /// Everything a selector can match this event by, other than its kind: the classes written on
    /// the line, and the subject's own name, which is an implicit class.
    pub fn all_classes(&self) -> Vec<String> {
        let mut v = self.classes.clone();
        v.push(self.subject.name().to_string());
        if let Body::Message { .. } = self.body {
            // A message's type is its class, and there is no separate type slot.
        }
        v
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Document {
    pub title: Option<String>,
    pub participants: Vec<String>,
    pub events: Vec<Event>,
    /// Comment lines at the top, kept so provenance written by an emitter survives a round trip.
    pub header: Vec<String>,
}

// ---------------------------------------------------------------------------
// Style
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8);

impl Color {
    pub fn hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }

    pub fn parse(s: &str) -> Option<Color> {
        let s = s.strip_prefix('#')?;
        if s.len() != 6 {
            return None;
        }
        Some(Color(
            u8::from_str_radix(&s[0..2], 16).ok()?,
            u8::from_str_radix(&s[2..4], 16).ok()?,
            u8::from_str_radix(&s[4..6], 16).ok()?,
        ))
    }
}

/// `solid`, `dashed`, `dotted`, or a raw `ink,gap` in pixels. The named values exist because
/// `dash=3,3` is SVG's internals leaking into a language that should not have them.
#[derive(Debug, Clone, PartialEq)]
pub enum Dash {
    Solid,
    Pattern(f64, f64),
}

impl Dash {
    pub fn parse(s: &str) -> Option<Dash> {
        match s {
            "solid" => Some(Dash::Solid),
            "dashed" => Some(Dash::Pattern(4.0, 3.0)),
            "dotted" => Some(Dash::Pattern(1.0, 3.0)),
            raw => {
                let (a, b) = raw.split_once(',')?;
                Some(Dash::Pattern(a.trim().parse().ok()?, b.trim().parse().ok()?))
            }
        }
    }

    pub fn array(&self) -> Option<String> {
        match self {
            Dash::Solid => None,
            Dash::Pattern(a, b) => Some(format!("{a} {b}")),
        }
    }
}

/// The closed set of marker shapes. Every one is distinguishable in monochrome, which is what
/// makes the encoding survive a printout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Dot,
    Ring,
    Square,
    Diamond,
    Cross,
    Bar,
    Chevron,
    /// A short oblique arrow into the lane, shaft and all. For something arriving from outside the
    /// diagram — a request from whoever is using the module — where a bare arrowhead says too
    /// little about where it came from.
    Arrow,
    None,
}

impl Mark {
    pub fn parse(s: &str) -> Option<Mark> {
        Some(match s {
            "dot" => Mark::Dot,
            "ring" => Mark::Ring,
            "square" => Mark::Square,
            "diamond" => Mark::Diamond,
            "cross" => Mark::Cross,
            "bar" => Mark::Bar,
            "chevron" => Mark::Chevron,
            "arrow" => Mark::Arrow,
            "none" => Mark::None,
            _ => return None,
        })
    }

    pub const ALL: [&'static str; 9] =
        ["dot", "ring", "square", "diamond", "cross", "bar", "chevron", "arrow", "none"];

    /// How much room the mark needs beside the lane, before a label starts. An arrow has a shaft
    /// to clear; everything else is about as wide as it is tall.
    pub fn reach(&self, size: f64) -> f64 {
        match self {
            Mark::Arrow => size * 5.5,
            Mark::None => 2.0,
            _ => size + 2.0,
        }
    }
}

/// How a span's far end is drawn. `Ragged` is how an unclosed span says it never closed, which is
/// what a process dying inside a critical section looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Flat,
    Ragged,
}

impl Edge {
    pub fn parse(s: &str) -> Option<Edge> {
        match s {
            "flat" => Some(Edge::Flat),
            "ragged" => Some(Edge::Ragged),
            _ => None,
        }
    }
}

/// Properties a rule may set. Every field is optional because rules **merge**: one that sets only
/// `width` leaves an earlier rule's `color` alone.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Props {
    pub color: Option<Color>,
    pub width: Option<f64>,
    pub dash: Option<Dash>,
    pub opacity: Option<f64>,
    pub fill: Option<Color>,
    pub mark: Option<Mark>,
    pub size: Option<f64>,
    pub label: Option<String>,
    pub edge: Option<Edge>,
    pub radius: Option<f64>,
}

impl Props {
    /// Lay `other` over `self`, field by field. Only the properties `other` actually sets move.
    pub fn merge(&mut self, other: &Props) {
        macro_rules! take {
            ($($f:ident),*) => { $( if other.$f.is_some() { self.$f = other.$f.clone(); } )* };
        }
        take!(color, width, dash, opacity, fill, mark, size, label, edge, radius);
    }
}

/// One name and any number of qualifiers, in any order. Specificity is the number of parts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selector {
    pub name: Option<String>,
    pub classes: Vec<String>,
    pub states: Vec<String>,
}

impl Selector {
    pub fn specificity(&self) -> usize {
        self.name.is_some() as usize + self.classes.len() + self.states.len()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub selector: Selector,
    pub props: Props,
    /// Source order, which breaks ties between equally specific rules.
    pub order: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Linear,
    Compressed,
    Ordinal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}

/// Page-level settings, written `style diagram …`.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub stretch: f64,
    pub axis: Axis,
    pub gap: Tick,
    pub lane_pitch: f64,
    pub row_pitch: f64,
    pub theme: Theme,
    pub bodies: bool,
}

impl Default for Diagram {
    fn default() -> Self {
        Diagram {
            stretch: 1.5,
            axis: Axis::Linear,
            gap: 200,
            lane_pitch: 160.0,
            row_pitch: 28.0,
            theme: Theme::Light,
            bodies: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StyleSheet {
    pub kinds: BTreeMap<String, KindDecl>,
    pub rules: Vec<Rule>,
    pub diagram: Diagram,
}

impl StyleSheet {
    /// Which declared span, if any, this name closes.
    pub fn closed_by(&self, opener: &str) -> Option<&str> {
        self.kinds.get(opener).and_then(|k| k.closes.as_deref())
    }

    /// Whether a name is the closing half of some span, and therefore not an event in its own
    /// right. Used so a `exit_cs` does not also draw a stray point.
    pub fn is_closer(&self, name: &str) -> bool {
        self.kinds.values().any(|k| k.closes.as_deref() == Some(name))
    }

    /// Resolve every rule matching a kind, its classes and its states, most specific last.
    pub fn resolve(&self, kind: Option<&str>, classes: &[String], states: &[String]) -> Props {
        let mut hits: Vec<&Rule> = self
            .rules
            .iter()
            .filter(|r| {
                if let Some(n) = &r.selector.name {
                    // `arrow` and `lifeline` are names too, and reach things with no declared kind.
                    if Some(n.as_str()) != kind {
                        return false;
                    }
                }
                r.selector.classes.iter().all(|c| classes.contains(c))
                    && r.selector.states.iter().all(|s| states.contains(s))
            })
            .collect();
        hits.sort_by_key(|r| (r.selector.specificity(), r.order));
        let mut props = Props::default();
        for r in hits {
            props.merge(&r.props);
        }
        props
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_instant_is_relative_or_absolute_and_nothing_else() {
        assert_eq!(Instant::After(10).resolve(90), 100);
        assert_eq!(Instant::At(10).resolve(90), 10);
    }

    #[test]
    fn a_subject_knows_which_pseudo_class_reaches_it() {
        assert_eq!(Subject::parse("n0").pseudo(), "node");
        assert_eq!(Subject::parse("network").pseudo(), "network");
        assert_eq!(Subject::parse("run").pseudo(), "run");
    }

    #[test]
    fn bare_words_are_lanes_on_the_network_and_a_label_anywhere_else() {
        let mk = |subject: Subject| Event {
            time: 0,
            subject,
            body: Body::Named("split".into()),
            words: vec!["n1".into(), "n3".into()],
            label: None,
            classes: vec![],
            instant: None,
            detail: None,
            line: 1,
        };
        assert_eq!(mk(Subject::Network).lanes(), &["n1".to_string(), "n3".to_string()]);
        assert_eq!(mk(Subject::Network).stated_label(), None);
        assert!(mk(Subject::Node("n0".into())).lanes().is_empty());
        assert_eq!(mk(Subject::Node("n0".into())).stated_label(), Some("n1 n3".into()));
    }

    #[test]
    fn a_quoted_label_wins_over_bare_words_on_any_subject() {
        let e = Event {
            time: 0,
            subject: Subject::Network,
            body: Body::Named("split".into()),
            words: vec!["n1".into()],
            label: Some("A | B".into()),
            classes: vec![],
            instant: None,
            detail: None,
            line: 1,
        };
        assert_eq!(e.stated_label(), Some("A | B".into()));
        assert_eq!(e.lanes(), &["n1".to_string()]);
    }

    #[test]
    fn dash_has_names_as_well_as_a_raw_pattern() {
        assert_eq!(Dash::parse("solid"), Some(Dash::Solid));
        assert_eq!(Dash::parse("dashed"), Some(Dash::Pattern(4.0, 3.0)));
        assert_eq!(Dash::parse("dotted"), Some(Dash::Pattern(1.0, 3.0)));
        assert_eq!(Dash::parse("3,2"), Some(Dash::Pattern(3.0, 2.0)));
        assert_eq!(Dash::parse("wobbly"), None);
        assert_eq!(Dash::Solid.array(), None);
    }

    #[test]
    fn every_named_mark_parses_and_nothing_else_does() {
        for name in Mark::ALL {
            assert!(Mark::parse(name).is_some(), "{name} should parse");
        }
        assert_eq!(Mark::parse("blob"), None);
    }

    #[test]
    fn colours_round_trip_through_hex() {
        assert_eq!(Color::parse("#0072b2"), Some(Color(0x00, 0x72, 0xb2)));
        assert_eq!(Color(0x00, 0x72, 0xb2).hex(), "#0072b2");
        assert_eq!(Color::parse("0072b2"), None);
        assert_eq!(Color::parse("#fff"), None);
    }

    /// Merging rather than replacing is what lets a `label` survive a later rule that only
    /// repaints.
    #[test]
    fn merging_moves_only_the_properties_the_later_rule_sets() {
        let mut a = Props { color: Color::parse("#0072b2"), label: Some("RB".into()), ..Default::default() };
        let b = Props { color: Color::parse("#cc79a7"), ..Default::default() };
        a.merge(&b);
        assert_eq!(a.color, Color::parse("#cc79a7"));
        assert_eq!(a.label, Some("RB".into()));
    }

    #[test]
    fn specificity_is_the_number_of_parts() {
        let s = |n: Option<&str>, c: &[&str], st: &[&str]| Selector {
            name: n.map(String::from),
            classes: c.iter().map(|x| x.to_string()).collect(),
            states: st.iter().map(|x| x.to_string()).collect(),
        };
        assert_eq!(s(Some("arrow"), &[], &[]).specificity(), 1);
        assert_eq!(s(Some("arrow"), &["slow"], &[]).specificity(), 2);
        assert_eq!(s(Some("arrow"), &["slow", "retry"], &[]).specificity(), 3);
        assert_eq!(s(None, &["rb"], &[]).specificity(), 1);
        assert_eq!(s(Some("arrow"), &["rb"], &["died"]).specificity(), 3);
    }
}
