//! The layout the page carries is a build of this crate, committed. Nothing rebuilds it when the
//! crate changes, so without this the page would go on laying runs out the old way, in silence.

use std::path::{Path, PathBuf};

/// Everything the browser build is made from.
fn sources() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    // The page itself, the command line and the PNG writer are not in the browser build.
    let not_built = ["html.rs", "main.rs", "png.rs"];
    let mut v: Vec<PathBuf> = std::fs::read_dir(root.join("src"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| !not_built.contains(&p.file_name().unwrap().to_str().unwrap()))
        .collect();
    for f in ["assets/default.cuestyle", "assets/Inter-Regular.ttf", "wasm/src/lib.rs", "wasm/Cargo.toml", "wasm/Cargo.lock"] {
        v.push(root.join(f));
    }
    v.sort();
    v
}

/// FNV-1a over every source, names included. Not a secure hash, and it need not be one: it stands
/// against forgetting, not against an adversary.
fn stamp() -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut h: u64 = 0xcbf29ce484222325;
    let mut eat = |bytes: &[u8]| {
        for b in bytes {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    };
    for p in sources() {
        eat(p.strip_prefix(root).unwrap().to_str().unwrap().as_bytes());
        eat(&[0]);
        eat(&std::fs::read(&p).unwrap());
        eat(&[0]);
    }
    format!("{h:016x}\n")
}

#[test]
fn the_layout_the_page_carries_was_built_from_these_sources() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/cuesheet.wasm.stamp");
    // `tools/build-wasm.sh` sets this once it has built, and only then.
    if std::env::var_os("CUESHEET_WASM_BUILT").is_some() {
        std::fs::write(&file, stamp()).unwrap();
    }
    assert_eq!(
        std::fs::read_to_string(&file).unwrap_or_default(),
        stamp(),
        "assets/cuesheet.wasm.gz was built from other sources than these: run tools/build-wasm.sh"
    );
}
