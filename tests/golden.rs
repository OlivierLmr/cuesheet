//! Golden files.
//!
//! The same document must produce the same bytes, on every machine and after every refactor. That
//! is not a nicety: a regenerated diagram is only diffable against the one before it if nothing
//! but the run can move the output, and the golden files are what notice when something else can.
//!
//! Run with `UPDATE_GOLDEN=1` to rewrite them after a deliberate change, then read the diff.

use cuesheet::{derive, font, layout, parse, svg};
use std::path::Path;

const STYLE: &str = include_str!("../examples/cuesheet.cuestyle");

fn render(name: &str) -> String {
    let src = std::fs::read_to_string(format!("examples/{name}.cuesheet")).expect("example exists");
    let doc = parse::document(&src).expect("example parses");
    let sheet = cuesheet::stylesheet(Some(STYLE)).expect("style parses");
    let run = derive::run(&doc, &sheet);
    let pic = layout::build(&doc, &sheet, &run, &font::Font::embedded());
    svg::render(&pic, &svg::Options::default())
}

fn check(name: &str) {
    let got = render(name);
    let path = format!("tests/golden/{name}.svg");

    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all("tests/golden").ok();
        std::fs::write(&path, &got).expect("should write the golden file");
        return;
    }

    let want = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!("{path} is missing; run with UPDATE_GOLDEN=1 to create it")
    });

    if got != want {
        // Say *where* it diverged rather than printing two files nobody can compare by eye.
        let (g, w): (Vec<&str>, Vec<&str>) = (got.lines().collect(), want.lines().collect());
        let at = g.iter().zip(&w).position(|(a, b)| a != b);
        match at {
            Some(i) => panic!(
                "{name} changed at line {}:\n  was:  {}\n  now:  {}\n\
                 ({} lines before, {} now). If this was deliberate, UPDATE_GOLDEN=1 and read the diff.",
                i + 1,
                w[i].trim(),
                g[i].trim(),
                w.len(),
                g.len()
            ),
            None => panic!(
                "{name} changed length only: {} lines before, {} now",
                w.len(),
                g.len()
            ),
        }
    }
}

#[test]
fn relais_manquant() {
    check("relais-manquant");
}

#[test]
fn showcase() {
    check("showcase");
}

#[test]
fn partition() {
    check("partition");
}

#[test]
fn mutex_egalites() {
    check("mutex-egalites");
}

/// The golden files are only worth anything if they are actually checked in.
#[test]
fn every_example_has_a_golden_file() {
    // Under UPDATE_GOLDEN the files are being written by the other tests in this binary, and test
    // order is not defined — checking for them here would fail on the very run that creates them.
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        return;
    }
    for entry in std::fs::read_dir("examples").expect("examples/ exists") {
        let p = entry.expect("readable").path();
        if p.extension().and_then(|e| e.to_str()) != Some("cuesheet") {
            continue;
        }
        let stem = p.file_stem().and_then(|s| s.to_str()).expect("named");
        assert!(
            Path::new(&format!("tests/golden/{stem}.svg")).exists(),
            "examples/{stem}.cuesheet has no golden file; run with UPDATE_GOLDEN=1"
        );
    }
}
