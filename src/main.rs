//! `cuesheet render <doc> [--style S] [--out F] [--format svg|png|html] [--open]`
//!
//! Four flags. Everything that filters is v2: a blacklist beside a whitelist is two ways to say
//! one thing, and a filter is hard to design before knowing what is actually unreadable.

use cuesheet::model::Theme;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
cuesheet — draws a distributed run as a space-time diagram

  cuesheet render <doc.cuesheet> [options]

  --style <file.cuestyle>  styles, laid over the built-in defaults
  --out <file>             where to write; stdout if absent
  --format svg|png|html    defaults to the extension of --out; else html
                           when opening, svg when writing to stdout
  --open                   show the result, writing beside the document
                           if --out is absent
  --version
";

#[derive(Default)]
struct Args {
    doc: Option<PathBuf>,
    style: Option<PathBuf>,
    out: Option<PathBuf>,
    format: Option<String>,
    open: bool,
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    match run(&argv) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("cuesheet: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(argv: &[String]) -> Result<(), String> {
    if argv.is_empty() || argv[0] == "--help" || argv[0] == "-h" {
        print!("{USAGE}");
        return Ok(());
    }
    if argv[0] == "--version" {
        println!("cuesheet {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if argv[0] != "render" {
        return Err(format!("`{}` is not a command; there is one, `render`", argv[0]));
    }

    let mut a = Args::default();
    let mut i = 1;
    while i < argv.len() {
        let need = |i: usize, what: &str| -> Result<String, String> {
            argv.get(i + 1).cloned().ok_or_else(|| format!("{what} needs a value"))
        };
        match argv[i].as_str() {
            "--style" => {
                a.style = Some(need(i, "--style")?.into());
                i += 2;
            }
            "--out" => {
                a.out = Some(need(i, "--out")?.into());
                i += 2;
            }
            "--format" => {
                a.format = Some(need(i, "--format")?);
                i += 2;
            }
            "--open" => {
                a.open = true;
                i += 1;
            }
            other if other.starts_with('-') => {
                return Err(format!(
                    "`{other}` is not a flag; there are four: --style, --out, --format, --open"
                ))
            }
            other => {
                if a.doc.is_some() {
                    return Err("render takes one document".into());
                }
                a.doc = Some(other.into());
                i += 1;
            }
        }
    }

    let doc_path = a.doc.ok_or("render needs a document")?;
    let src = std::fs::read_to_string(&doc_path)
        .map_err(|e| format!("{}: {e}", doc_path.display()))?;
    let doc = cuesheet::parse::document(&src)
        .map_err(|e| format!("{}:{e}", doc_path.display()))?;

    let style_src = match &a.style {
        Some(p) => Some(std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?),
        None => None,
    };
    let sheet = cuesheet::stylesheet(style_src.as_deref()).map_err(|e| {
        match &a.style {
            Some(p) => format!("{}:{e}", p.display()),
            None => format!("built-in style sheet:{e}"),
        }
    })?;

    let format = a
        .format
        .clone()
        .or_else(|| {
            a.out
                .as_ref()
                .and_then(|p| p.extension())
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
        })
        // Nothing said which form, so infer it from where it is going. A browser is handed the
        // interactive one; a pipe is handed the one that embeds in a document.
        .unwrap_or_else(|| if a.open { "html".into() } else { "svg".into() });

    let run = cuesheet::derive::run(&doc, &sheet);
    let font = cuesheet::font::Font::embedded();
    let pic = cuesheet::layout::build(&doc, &sheet, &run, &font);

    let bytes: Vec<u8> = match format.as_str() {
        "svg" => cuesheet::svg::render(&pic, &cuesheet::svg::Options::default()).into_bytes(),
        "html" | "htm" => cuesheet::html::render(&pic, sheet.diagram.theme).into_bytes(),
        "png" => png_bytes(&pic)?,
        other => {
            return Err(format!(
                "`{other}` is not a format; there are three: svg, png, html"
            ))
        }
    };

    // Opening needs a file, so `--open` without `--out` picks one: beside the document, under its
    // own name. Predictable enough to find again, and to overwrite rather than litter.
    let written = match (&a.out, a.open) {
        (Some(p), _) => Some(p.clone()),
        (None, true) => Some(beside(&doc_path, &format)),
        (None, false) => None,
    };
    match &written {
        Some(p) => std::fs::write(p, &bytes).map_err(|e| format!("{}: {e}", p.display()))?,
        None => std::io::stdout().write_all(&bytes).map_err(|e| e.to_string())?,
    }
    if a.open {
        let p = written.expect("--open always writes a file");
        open_it(&p)?;
    }
    Ok(())
}

/// Where `--open` writes when nothing said where: the document's own path, reskinned.
fn beside(doc: &std::path::Path, format: &str) -> PathBuf {
    doc.with_extension(if format == "htm" { "html" } else { format })
}

/// The command this platform shows a file with.
///
/// Split out from the spawning so the choice can be tested without a browser opening during
/// `cargo test`.
fn opener() -> Option<(&'static str, &'static [&'static str])> {
    match std::env::consts::OS {
        "macos" => Some(("open", &[])),
        "windows" => Some(("cmd", &["/C", "start", ""])),
        "linux" | "freebsd" | "openbsd" | "netbsd" => Some(("xdg-open", &[])),
        _ => None,
    }
}

fn open_it(p: &std::path::Path) -> Result<(), String> {
    let (cmd, args) = opener()
        .ok_or_else(|| format!("no way to open a file on this platform; it is at {}", p.display()))?;
    std::process::Command::new(cmd)
        .args(args)
        .arg(p)
        .status()
        .map_err(|e| format!("could not run `{cmd}`: {e}; the file is at {}", p.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_writes_beside_the_document_when_nowhere_else_is_said() {
        assert_eq!(beside(std::path::Path::new("store/run/messages.cuesheet"), "html"),
                   PathBuf::from("store/run/messages.html"));
        assert_eq!(beside(std::path::Path::new("a/b.cuesheet"), "svg"), PathBuf::from("a/b.svg"));
    }

    /// `--format htm` is accepted, but a file called `.htm` is nobody's intent.
    #[test]
    fn the_short_spelling_of_html_still_writes_a_html_file() {
        assert_eq!(beside(std::path::Path::new("x.cuesheet"), "htm"), PathBuf::from("x.html"));
    }

    #[test]
    fn every_platform_this_runs_on_knows_how_to_open_a_file() {
        assert!(opener().is_some(), "{} has no opener", std::env::consts::OS);
    }
}

#[cfg(feature = "png")]
fn png_bytes(pic: &cuesheet::picture::Picture) -> Result<Vec<u8>, String> {
    cuesheet::png::render(pic)
}

#[cfg(not(feature = "png"))]
fn png_bytes(_pic: &cuesheet::picture::Picture) -> Result<Vec<u8>, String> {
    Err("this build has no PNG support; rebuild with `--features png`".into())
}

/// Theme is a style-sheet setting rather than a flag, so the CLI never has to know about it.
#[allow(dead_code)]
fn theme_name(t: Theme) -> &'static str {
    match t {
        Theme::Light => "light",
        Theme::Dark => "dark",
    }
}
