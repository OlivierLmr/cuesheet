//! The command line, end to end.
//!
//! Four flags, and a message that names the closed set whenever one of them is wrong — a language
//! people write by hand is only as good as what it says when they get it wrong, and the same is
//! true of the command that renders it.

use std::path::PathBuf;
use std::process::{Command, Output};

fn bin() -> PathBuf {
    // The integration test binary sits beside the one under test.
    let mut p = std::env::current_exe().expect("test binary has a path");
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("cuesheet")
}

fn run(args: &[&str]) -> Output {
    Command::new(bin()).args(args).output().expect("cuesheet should be built")
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}
fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

#[test]
fn rendering_a_document_writes_an_svg_to_stdout() {
    let o = run(&["render", "examples/relais-manquant.st"]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&o).starts_with("<svg"), "{}", &out(&o)[..80.min(out(&o).len())]);
}

#[test]
fn the_format_follows_the_output_extension_when_it_is_not_given() {
    let dir = std::env::temp_dir().join("cuesheet-cli-ext");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("out.html");
    let o = run(&["render", "examples/relais-manquant.st", "--out", path.to_str().unwrap()]);
    assert!(o.status.success(), "{}", err(&o));
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(body.starts_with("<!doctype html>"), "{}", &body[..60]);
}

#[test]
fn a_style_sheet_is_laid_over_the_defaults() {
    let dir = std::env::temp_dir().join("cuesheet-cli-style");
    std::fs::create_dir_all(&dir).unwrap();
    let sts = dir.join("s.sts");
    std::fs::write(&sts, "style arrow color=#123456").unwrap();
    let o = run(&["render", "examples/relais-manquant.st", "--style", sts.to_str().unwrap()]);
    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&o).contains("#123456"), "the override did not take");
    // And the defaults are still underneath: lifelines were never mentioned by that sheet.
    assert!(out(&o).contains("#c3c9d3"), "the default sheet was lost");
}

#[test]
fn a_parse_error_names_the_file_the_line_and_the_column() {
    let dir = std::env::temp_dir().join("cuesheet-cli-bad");
    std::fs::create_dir_all(&dir).unwrap();
    let bad = dir.join("bad.st");
    std::fs::write(&bad, "0 n0 deliver\nnope n1 deliver\n").unwrap();
    let o = run(&["render", bad.to_str().unwrap()]);
    assert!(!o.status.success());
    let e = err(&o);
    assert!(e.contains("bad.st:2:1"), "{e}");
    assert!(e.contains("not a time"), "{e}");
}

#[test]
fn an_unknown_flag_names_the_three_that_exist() {
    let o = run(&["render", "examples/relais-manquant.st", "--only", ".rb"]);
    assert!(!o.status.success());
    let e = err(&o);
    assert!(e.contains("--style") && e.contains("--out") && e.contains("--format"), "{e}");
}

#[test]
fn an_unknown_format_names_the_three_that_exist() {
    let o = run(&["render", "examples/relais-manquant.st", "--format", "pdf"]);
    assert!(!o.status.success());
    assert!(err(&o).contains("svg, png, html"), "{}", err(&o));
}

#[test]
fn an_unknown_command_says_there_is_one() {
    let o = run(&["draw", "x.st"]);
    assert!(!o.status.success());
    assert!(err(&o).contains("there is one, `render`"), "{}", err(&o));
}

#[test]
fn a_missing_file_says_so_rather_than_panicking() {
    let o = run(&["render", "nowhere/at/all.st"]);
    assert!(!o.status.success());
    assert!(err(&o).contains("all.st"), "{}", err(&o));
}

#[test]
fn help_and_version_work_without_a_document() {
    assert!(run(&["--help"]).status.success());
    let v = run(&["--version"]);
    assert!(v.status.success());
    assert!(out(&v).contains("cuesheet"), "{}", out(&v));
}

/// Without the feature, PNG has to say so rather than produce an empty file.
#[cfg(not(feature = "png"))]
#[test]
fn a_build_without_png_says_so() {
    let o = run(&["render", "examples/relais-manquant.st", "--format", "png"]);
    assert!(!o.status.success());
    assert!(err(&o).contains("--features png"), "{}", err(&o));
}

#[cfg(feature = "png")]
#[test]
fn png_renders_and_is_reproducible() {
    let dir = std::env::temp_dir().join("cuesheet-cli-png");
    std::fs::create_dir_all(&dir).unwrap();
    let (a, b) = (dir.join("a.png"), dir.join("b.png"));
    for p in [&a, &b] {
        let o = run(&["render", "examples/relais-manquant.st", "--out", p.to_str().unwrap()]);
        assert!(o.status.success(), "{}", err(&o));
    }
    let (x, y) = (std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
    assert_eq!(&x[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(x, y, "the same document rasterised to different bytes");
}

/// `--open` is the flag a student reaches for, so `--help` has to mention it.
#[test]
fn the_usage_names_every_flag_including_open() {
    let o = run(&["--help"]);
    assert!(o.status.success(), "{}", err(&o));
    let u = out(&o);
    for f in ["--style", "--out", "--format", "--open"] {
        assert!(u.contains(f), "usage never mentions {f}:\n{u}");
    }
}

/// The closed set in the error message has to keep pace with the set itself, or it teaches a lie.
#[test]
fn an_unknown_flag_names_all_four() {
    let o = run(&["render", "examples/partition.st", "--zoom"]);
    assert!(!o.status.success());
    let e = err(&o);
    assert!(e.contains("four"), "{e}");
    assert!(e.contains("--open"), "{e}");
}
