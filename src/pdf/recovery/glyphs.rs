//! PDF glyphs recovery.
use super::*;

/// PDFium can expose a non-BMP mathematical letter as two UTF-16 records.
/// Keep one scalar at the leading record's geometry, never drop both halves.
pub(super) fn decoded_characters(text: &PdfPageText<'_>) -> Vec<(PdfPageTextCharIndex, char)> {
    let chars = text.chars();
    let mut iter = chars.iter().peekable();
    let mut decoded = Vec::new();
    while let Some(g) = iter.next() {
        let value = g.unicode_value();
        let ch = if (0xD800..=0xDBFF).contains(&value) {
            let scalar = iter.peek().and_then(|next| {
                let low = next.unicode_value();
                (0xDC00..=0xDFFF)
                    .contains(&low)
                    .then(|| char::from_u32(0x10000 + ((value - 0xD800) << 10) + low - 0xDC00))
                    .flatten()
            });
            if scalar.is_some() {
                iter.next();
            }
            scalar
        } else {
            char::from_u32(value)
        };
        if let Some(ch) = ch {
            decoded.push((g.index(), ch));
        }
    }
    decoded
}

pub(super) fn glyphs(text: &PdfPageText<'_>) -> Vec<Glyph> {
    let mut glyphs = Vec::new();
    for (index, c) in decoded_characters(text) {
        let Ok(ch) = text.chars().get(index) else {
            continue;
        };
        let font = ch.font_name().to_ascii_lowercase();
        let mono = ch.font_is_fixed_pitch()
            || [
                "mono",
                "courier",
                "consolas",
                "menlo",
                "typewriter",
                "sourcecode",
            ]
            .iter()
            .any(|name| font.contains(name));
        let (x, y) = ch
            .origin()
            .map(|(x, y)| (x.value, y.value))
            .unwrap_or_default();
        let color = ch
            .fill_color()
            .map(|c| (u32::from(c.red()) << 16) | (u32::from(c.green()) << 8) | u32::from(c.blue()))
            .unwrap_or(0x23211d);
        let weight = ch.font_weight();
        glyphs.push(Glyph {
            ch: c,
            mono,
            x,
            right: ch.tight_bounds().map(|b| b.right().value).unwrap_or(x),
            advance_right: ch.loose_bounds().map(|b| b.right().value).unwrap_or(x),
            y,
            size: ch.scaled_font_size().value,
            color,
            italic: ch.font_is_italic() || font.contains("italic") || font.contains("oblique"),
            bold: font.contains("bold") || font.contains("cmbx") || font.contains("cmmib") || font.contains("cmbsy") || font.contains("cmssbx") || ch.font_is_bold_reenforced() || matches!(weight, Some(PdfFontWeight::Weight600 | PdfFontWeight::Weight700Bold | PdfFontWeight::Weight800 | PdfFontWeight::Weight900)) || matches!(weight, Some(PdfFontWeight::Custom(weight)) if (600..=900).contains(&weight)),
            image_before: None,
            image_after: None,
            link: None,
        });
    }
    glyphs
}

/// Some transformed PDF fonts make PDFium emit a newline after each letter.
/// Recover only runs with repeated breaks on one physical baseline, using the
/// full glyph advance to distinguish word spaces from italic side bearings.
pub(super) fn recover_fragmented_lines(glyphs: &[Glyph]) -> (Vec<Glyph>, bool) {
    let lines: Vec<_> = glyphs.split_inclusive(|g| g.ch == '\n').collect();
    let same_baseline = |a: &[Glyph], b: &[Glyph]| {
        let Some(prev) = a
            .iter()
            .rev()
            .find(|g| !g.ch.is_whitespace() && !g.ch.is_control())
        else {
            return false;
        };
        let Some(next) = b
            .iter()
            .find(|g| !g.ch.is_whitespace() && !g.ch.is_control())
        else {
            return false;
        };
        let size = prev.size;
        size > 0.
            && !prev.mono
            && !next.mono
            && (prev.size - next.size).abs() <= size * 0.05
            && (prev.y - next.y).abs() <= size * 0.05
            && next.x > prev.x
            && next.x - prev.advance_right >= -size * 0.3
            && next.x - prev.advance_right <= size * 1.2
            && a.iter()
                .chain(b)
                .filter(|g| !g.ch.is_whitespace() && !g.ch.is_control())
                .all(|g| (g.y - prev.y).abs() <= size * 0.05)
    };
    let mut out = Vec::with_capacity(glyphs.len());
    let mut changed = false;
    let mut i = 0;
    while i < lines.len() {
        let start = i;
        i += 1;
        while i < lines.len() && same_baseline(lines[i - 1], lines[i]) {
            i += 1;
        }
        if i - start < 4 {
            out.extend(lines[start..i].iter().flat_map(|line| line.iter()).cloned());
            continue;
        }
        changed = true;
        let mut physical: Vec<Glyph> = Vec::new();
        for g in lines[start..i]
            .iter()
            .flat_map(|line| line.iter())
            .filter(|g| g.ch != '\n' && g.ch != '\r')
        {
            if !g.ch.is_whitespace()
                && let Some(prev) = physical.last().filter(|g| !g.ch.is_whitespace())
                && g.x - prev.advance_right > g.size * 0.12
            {
                let mut space = prev.clone();
                space.ch = ' ';
                space.x = prev.advance_right;
                space.right = g.x;
                space.advance_right = g.x;
                physical.push(space);
            }
            physical.push(g.clone());
        }
        out.extend(physical);
        out.extend(lines[i - 1].iter().filter(|g| g.ch == '\n').cloned());
    }
    (out, changed)
}

// A glyph-sized raster in a text baseline's gap is an inline object in prose,
// headings or code. Use painted glyph edges: proportional letters and PDFium's
// synthetic spaces do not have a reliable fixed character advance.
