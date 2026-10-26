//! Shared fixtures and focused regression suites.
use super::*;

mod audit_structure;
mod audit_typography;
mod headings_and_code;
mod links;
mod prose;
mod tables;

pub(super) fn test_lines(lines: &[(&str, f32, bool, bool)]) -> Vec<Glyph> {
    lines
        .iter()
        .enumerate()
        .flat_map(|(line, (text, size, bold, mono))| {
            text.chars()
                .chain(std::iter::once('\n'))
                .enumerate()
                .map(move |(i, ch)| Glyph {
                    ch,
                    mono: *mono,
                    x: 72. + i as f32 * 6.,
                    right: 78. + i as f32 * 6.,
                    advance_right: 78. + i as f32 * 6.,
                    y: 740. - line as f32 * 20.,
                    size: *size,
                    color: 0,
                    italic: false,
                    bold: *bold,
                    image_before: None,
                    image_after: None,
                    link: None,
                })
        })
        .collect()
}
