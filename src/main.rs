//! `cuesheet render <doc> [--style S] [--out F] [--format svg|png|html]`
//!
//! Three flags. Everything that filters is v2: a blacklist beside a whitelist is two ways to say
//! one thing, and a filter is hard to design before knowing what is actually unreadable.

use cuesheet::model::Theme;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
cuesheet — draws a distributed run as a space-time diagram

  cuesheet render <doc.st> [options]

  --style <file.sts>   styles, laid over the built-in defaults
  --out <file>         where to write; stdout if absent
  --format svg|png|html
                       defaults to the extension of --out, else svg
  --version
";

#[derive(Default)]
struct Args {
    doc: Option<PathBuf>,
    style: Option<PathBuf>,
    out: Option<PathBuf>,
    format: Option<String>,
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
            other if other.starts_with('-') => {
                return Err(format!(
                    "`{other}` is not a flag; there are three: --style, --out, --format"
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
        .unwrap_or_else(|| "svg".into());

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

    match &a.out {
        Some(p) => std::fs::write(p, &bytes).map_err(|e| format!("{}: {e}", p.display()))?,
        None => std::io::stdout().write_all(&bytes).map_err(|e| e.to_string())?,
    }
    Ok(())
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
