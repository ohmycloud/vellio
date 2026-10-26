//! PDF highlights recovery.
use super::*;

pub(crate) struct TextHighlight {
    pub index: usize,
    pub quote: String,
    pub prefix: String,
    pub suffix: String,
    pub color: u32,
}

/// A line-height fill behind monospace text is a code highlight, not art.
pub(crate) fn text_highlights(page: &PdfPage<'_>) -> Vec<TextHighlight> {
    let Ok(text) = page.text() else {
        return vec![];
    };
    let gs = glyphs(&text);
    page.objects()
        .iter()
        .enumerate()
        .filter_map(|(index, object)| {
            let path = object.as_path_object()?;
            crate::pdf::rectangle_bounds(&object)?;
            let b = object.bounds().ok()?;
            let color = path.fill_color().ok()?;
            if color.alpha() == 0 {
                return None;
            }
            let inside: Vec<_> = gs
                .iter()
                .enumerate()
                .filter(|(_, g)| {
                    !g.ch.is_whitespace()
                        && !g.ch.is_control()
                        && g.x >= b.left().value - 0.5
                        && g.x <= b.right().value
                        && g.y >= b.bottom().value
                        && g.y <= b.top().value
                })
                .collect();
            let &(start, first) = inside.first()?;
            let &(end, last) = inside.last()?;
            if inside.len() < 3
                || inside
                    .iter()
                    .any(|(_, g)| !g.mono || (g.y - first.y).abs() > first.size * 0.15)
                || b.height().value < first.size * 0.5
                || b.height().value > first.size * 1.5
                || last.right - first.x < b.width().value * 0.65
            {
                return None;
            }
            let quote = gs[start..=end].iter().map(|g| g.ch).collect();
            let mut prefix: Vec<_> = gs[..start]
                .iter()
                .rev()
                .take_while(|g| g.ch != '\n')
                .take(80)
                .map(|g| g.ch)
                .collect();
            prefix.reverse();
            let suffix = gs[end + 1..]
                .iter()
                .take_while(|g| g.ch != '\n')
                .take(80)
                .map(|g| g.ch)
                .collect();
            Some(TextHighlight {
                index,
                quote,
                prefix: prefix.into_iter().collect(),
                suffix,
                color: (color.red() as u32) << 16
                    | (color.green() as u32) << 8
                    | color.blue() as u32,
            })
        })
        .collect()
}
