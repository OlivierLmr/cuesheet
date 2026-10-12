#!/bin/sh
# Rebuild the layout the HTML page carries: wasm/ → assets/cuesheet.wasm.gz.
#
# Needed after any change to what the browser build is made from — `cargo test` says when. Needs
# a Rust toolchain that has the wasm32-unknown-unknown target, which is the one thing building
# cuesheet itself does not need:
#
#   rustup target add wasm32-unknown-unknown
#
# CARGO names the cargo to use, for a machine whose default one has no such target:
#
#   CARGO="cargo +stable" tools/build-wasm.sh
set -eu
cd "$(dirname "$0")/.."

(cd wasm && ${CARGO:-cargo} build --release --target wasm32-unknown-unknown)
built=wasm/target/wasm32-unknown-unknown/release/cuesheet_wasm.wasm

# -n: no name and no date in the archive, so the same build gives the same bytes.
gzip -9 -n -c "$built" > assets/cuesheet.wasm.gz
echo "assets/cuesheet.wasm.gz: $(wc -c < assets/cuesheet.wasm.gz) bytes"

# The page's layout must draw what the command line draws, byte for byte.
if command -v node >/dev/null; then
    cargo build --release --quiet
    for doc in examples/*.cuesheet; do
        node tools/wasm-draw.js "$built" "$doc" examples/cuesheet.cuestyle > "$built.svg"
        ./target/release/cuesheet render "$doc" --style examples/cuesheet.cuestyle --format html \
            | awk '/<svg/,/<\/svg>/' > "$built.native.svg"
        cmp "$built.svg" "$built.native.svg" || { echo "$doc: the browser build draws something else" >&2; exit 1; }
    done
    echo "the browser build draws the examples exactly as the command line does"
else
    echo "node not found: the browser build was not compared with the command line" >&2
fi

CUESHEET_WASM_BUILT=1 cargo test --quiet --test wasm
