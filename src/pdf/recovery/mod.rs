//! Recover code and bookmarked headings from PDF fonts, colors and glyph positions.
//! This adapter never changes extracted text, IDs, edits or annotation records.
use base64::Engine;
use pdfium_render::prelude::*;

mod definition;
mod footnote;
mod glyphs;
mod headings;
mod highlights;
mod images;
mod links;
mod list;
mod math;
mod paragraphs;
mod pipeline;
mod render;
mod table;
mod toc;
mod typography;

#[cfg(test)]
#[path = "tests/prose_acceptance.rs"]
mod prose_tests;
#[cfg(test)]
mod tests;

use glyphs::*;
use headings::*;
pub(crate) use highlights::text_highlights;
use images::*;
use links::*;
use paragraphs::*;
use render::*;
use typography::*;
#[derive(Clone, Debug)]
struct Glyph {
    ch: char,
    mono: bool,
    x: f32,
    right: f32,
    advance_right: f32,
    y: f32,
    size: f32,
    color: u32,
    italic: bool,
    bold: bool,
    image_before: Option<String>,
    image_after: Option<String>,
    link: Option<String>,
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn is_code(line: &[Glyph]) -> bool {
    let visible: Vec<_> = line.iter().filter(|g| !g.ch.is_whitespace()).collect();
    !visible.is_empty() && visible.iter().all(|g| g.mono)
}

fn line_text(line: &[Glyph]) -> String {
    line.iter()
        .filter(|g| !g.ch.is_control() || g.ch == '\t')
        .map(|g| g.ch)
        .collect()
}

pub(crate) use images::{
    caption_bounds, diagram_callout_bounds, figure_caption_bounds, inline_image_indices,
    table_bounds,
};
pub(crate) use links::destination_quote;
pub(crate) use pipeline::reading_html;
