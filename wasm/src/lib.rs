//! cuesheet's layout, callable from a page.
//!
//! Three functions over plain memory, and nothing generated: the page writes two strings in, asks
//! for a picture, and reads one string out. No binding generator, because the whole interface is
//! this file and the forty lines of JavaScript that call it.

use cuesheet::model::Axis;
use cuesheet::{derive, font, layout, parse, svg};

/// Room for `len` bytes, for the page to write a string into.
#[no_mangle]
pub extern "C" fn room(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// Give back what `room` or `draw` handed out.
///
/// # Safety
/// `ptr` and `len` must be exactly what was handed out.
#[no_mangle]
pub unsafe extern "C" fn release(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
}

/// Lay the document out and draw it, as the interactive SVG the page inlines.
///
/// `axis` is 0 linear, 1 compressed, 2 ordinal; anything else keeps the style sheet's. `scale`
/// multiplies how much paper an instant takes — the sheet's `stretch`, and the row height of the
/// ordinal axis, which has no other notion of scale.
///
/// Returns a pointer to eight bytes: the address and the length of the answer, each a
/// little-endian `u32`. The answer lives where the address says. It starts with `+` when it is a
/// picture and with `-` when it is the reason there is none.
///
/// # Safety
/// The two strings must be UTF-8 the page wrote into memory it got from `room`.
#[no_mangle]
pub unsafe extern "C" fn draw(
    doc: *const u8,
    doc_len: usize,
    style: *const u8,
    style_len: usize,
    axis: i32,
    scale: f64,
) -> *const u8 {
    let doc = std::str::from_utf8_unchecked(std::slice::from_raw_parts(doc, doc_len));
    let style = std::str::from_utf8_unchecked(std::slice::from_raw_parts(style, style_len));
    let out = match picture(doc, style, axis, scale) {
        Ok(s) => format!("+{s}"),
        Err(e) => format!("-{e}"),
    }
    .into_bytes()
    .into_boxed_slice();
    // Bytes rather than two integers, so that `release` gives back exactly what was taken: memory
    // must be returned with the alignment it was asked for with.
    let mut head = Vec::with_capacity(8);
    head.extend_from_slice(&(out.as_ptr() as u32).to_le_bytes());
    head.extend_from_slice(&(out.len() as u32).to_le_bytes());
    let head = head.into_boxed_slice();
    std::mem::forget(out);
    Box::into_raw(head) as *const u8
}

fn picture(doc: &str, style: &str, axis: i32, scale: f64) -> Result<String, String> {
    let doc = parse::document(doc).map_err(|e| e.to_string())?;
    let mut sheet = cuesheet::stylesheet(Some(style)).map_err(|e| e.to_string())?;
    match axis {
        0 => sheet.diagram.axis = Axis::Linear,
        1 => sheet.diagram.axis = Axis::Compressed,
        2 => sheet.diagram.axis = Axis::Ordinal,
        _ => {}
    }
    if scale.is_finite() && scale > 0.0 {
        sheet.diagram.stretch *= scale;
        sheet.diagram.row_pitch *= scale;
    }
    let run = derive::run(&doc, &sheet);
    let pic = layout::build(&doc, &sheet, &run, &font::Font::embedded());
    Ok(svg::render(&pic, &svg::Options { interactive: true, standalone: false }))
}
