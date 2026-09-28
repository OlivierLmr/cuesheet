//! cuesheet — draws a distributed run as a space-time diagram.
//!
//! A message is a segment from the instant it left to the instant it landed, so its slope is its
//! flight time. The pipeline runs one way only — parse → derive → layout → picture → emit — and
//! every coordinate is decided in `layout`, so the emitters cannot disagree about where anything is.

pub mod derive;
pub mod lex;
pub mod model;
pub mod parse;
pub mod print;
