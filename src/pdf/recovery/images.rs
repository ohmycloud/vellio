//! PDF images recovery.
use super::*;

pub(super) fn insertion(glyphs: &[Glyph], bounds: [f32; 4]) -> Option<(usize, bool)> {
    let [left, bottom, right, top] = bounds;
    let width = right - left;
    let height = top - bottom;
    let fits = |g: &Glyph| {
        !g.ch.is_whitespace()
            && !g.ch.is_control()
            && g.size > 0.
            && width >= g.size * 0.4
            && width <= g.size * 1.6
            && height >= g.size * 0.4
            && height <= g.size * 1.6
            && (g.y - bottom).abs() <= g.size * 0.3
    };
    // Reject overlaid pictures, even if their edge happens to touch a letter.
    if glyphs
        .iter()
        .any(|g| fits(g) && g.x < right - 0.5 && g.right > left + 0.5)
    {
        return None;
    }
    let next = glyphs
        .iter()
        .enumerate()
        .filter(|(_, g)| fits(g) && g.x >= right - 0.5 && g.x - right <= g.size * 0.6)
        .min_by(|a, b| a.1.x.total_cmp(&b.1.x));
    let prev = glyphs
        .iter()
        .enumerate()
        .filter(|(_, g)| fits(g) && g.right <= left + 0.5 && left - g.right <= g.size * 0.6)
        .max_by(|a, b| a.1.right.total_cmp(&b.1.right));
    match (prev, next) {
        (Some((a, prev)), Some((b, next)))
            if a < b && (prev.y - next.y).abs() <= next.size * 0.2 =>
        {
            Some((b, false))
        }
        // At a physical line's ends require an even tighter gap. A nearby icon
        // in the margin or on another baseline must remain a separate figure.
        (None, Some((i, g))) if g.x - right <= g.size * 0.5 => Some((i, false)),
        (Some((i, g)), None) if left - g.right <= g.size * 0.5 => Some((i, true)),
        _ => None,
    }
}

pub(super) fn candidates(
    page: &PdfPage<'_>,
    glyphs: &[Glyph],
) -> Vec<(usize, usize, bool, PdfQuadPoints)> {
    page.objects()
        .iter()
        .enumerate()
        .filter_map(|(index, object)| {
            object.as_image_object()?;
            let bounds = object.bounds().ok()?;
            let (anchor, after) = insertion(
                glyphs,
                [
                    bounds.left().value,
                    bounds.bottom().value,
                    bounds.right().value,
                    bounds.top().value,
                ],
            )?;
            Some((index, anchor, after, bounds))
        })
        .collect()
}

pub(crate) fn inline_image_indices(page: &PdfPage<'_>) -> Vec<usize> {
    page.text()
        .map(|text| {
            candidates(page, &glyphs(&text))
                .into_iter()
                .map(|c| c.0)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn table_bounds(page: &PdfPage<'_>) -> Vec<[f32; 4]> {
    page.text()
        .map(|text| {
            let glyphs = recover_fragmented_lines(&glyphs(&text)).0;
            let mut bounds: Vec<_> = table::recover(page, &glyphs)
                .into_iter()
                .map(|t| t.bounds)
                .collect();
            for region in table::regions(page, &glyphs) {
                if !bounds.iter().any(|b| table::contains(*b, region)) {
                    bounds.push(region);
                }
            }
            bounds
        })
        .unwrap_or_default()
}

/// Captions are reading text, never external axis labels to bake into a crop.
pub(crate) fn caption_bounds(page: &PdfPage<'_>) -> Vec<[f32; 4]> {
    captions(page)
        .into_iter()
        .flat_map(|(_, lines)| lines)
        .collect()
}

/// One box per numbered Figure caption, including its wrapped physical lines.
/// Shared captions are the semantic evidence for grouping disconnected panels.
pub(crate) fn figure_caption_bounds(page: &PdfPage<'_>) -> Vec<[f32; 4]> {
    captions(page)
        .into_iter()
        .filter(|(figure, _)| *figure)
        .map(|(_, lines)| {
            lines.into_iter().fold(
                [
                    f32::INFINITY,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                    f32::NEG_INFINITY,
                ],
                |a, b| {
                    [
                        a[0].min(b[0]),
                        a[1].min(b[1]),
                        a[2].max(b[2]),
                        a[3].max(b[3]),
                    ]
                },
            )
        })
        .collect()
}

pub(super) fn captions(page: &PdfPage<'_>) -> Vec<(bool, Vec<[f32; 4]>)> {
    let Ok(text) = page.text() else {
        return vec![];
    };
    let (glyphs, _) = recover_fragmented_lines(&glyphs(&text));
    let lines: Vec<_> = glyphs.split(|g| g.ch == '\n').collect();
    let mut out = vec![];
    let mut i = 0;
    while i < lines.len() {
        if !caption_start(lines[i]) && !leading_caption(&lines, i) {
            i += 1;
            continue;
        }
        let Some(first) = lines[i].iter().find(|g| !g.ch.is_whitespace()) else {
            i += 1;
            continue;
        };
        let figure = line_text(lines[i]).starts_with("Figure ");
        // Physical-line recovery may join two independent captions on one
        // baseline. A second numbered label across a column-sized gap is not
        // a shared caption, even though both runs begin at the same y.
        let line = lines[i];
        let mut starts = vec![0];
        for j in 1..line.len() {
            if matches!(line[j].ch, 'F' | 'T')
                && caption_start(&line[j..])
                && line[..j]
                    .iter()
                    .rfind(|g| !g.ch.is_whitespace())
                    .is_some_and(|prev| line[j].x - prev.right > first.size * 2.)
            {
                starts.push(j);
            }
        }
        if starts.len() > 1 {
            starts.push(line.len());
            for range in starts.windows(2) {
                let fragment = &line[range[0]..range[1]];
                out.push((
                    line_text(fragment).trim_start().starts_with("Figure "),
                    vec![caption_line_bounds(fragment)],
                ));
            }
            i += 1;
            continue;
        }
        let mut bounds = vec![];
        loop {
            let line = lines[i];
            bounds.push(caption_line_bounds(line));
            i += 1;
            if i >= lines.len() || !caption_continuation(first, line, lines[i]) {
                break;
            }
        }
        out.push((figure, bounds));
    }
    out
}

pub(super) fn caption_line_bounds(line: &[Glyph]) -> [f32; 4] {
    let visible = || line.iter().filter(|g| !g.ch.is_whitespace());
    [
        visible()
            .map(|g| g.x - g.size * 0.2)
            .fold(f32::INFINITY, f32::min),
        visible()
            .map(|g| g.y - g.size * 0.4)
            .fold(f32::INFINITY, f32::min),
        visible()
            .map(|g| g.right + g.size * 0.2)
            .fold(f32::NEG_INFINITY, f32::max),
        visible()
            .map(|g| g.y + g.size)
            .fold(f32::NEG_INFINITY, f32::max),
    ]
}

pub(super) fn apply_prose_images(glyphs: &[Glyph], mut html: String) -> String {
    let mut offset = 0;
    for line in glyphs.split(|g| g.ch == '\n') {
        if !is_code(line) {
            for (j, g) in line.iter().enumerate() {
                for (image, after) in [(&g.image_before, false), (&g.image_after, true)] {
                    if let Some(image) = image {
                        // A phrase is a safer fallback than one repeated letter
                        // when figure labels or a footer were omitted by reflow.
                        let mut start = if after { j.saturating_sub(31) } else { j };
                        let mut end = if after {
                            j + 1
                        } else {
                            (j + 32).min(line.len())
                        };
                        while start < j && line[start].ch.is_whitespace() {
                            start += 1;
                        }
                        while end > j + 1 && line[end - 1].ch.is_whitespace() {
                            end -= 1;
                        }
                        let quote = line_text(&line[start..end]);
                        let start = offset + start;
                        let end = offset + end;
                        let prefix: String = glyphs[start.saturating_sub(80)..start]
                            .iter()
                            .map(|g| g.ch)
                            .collect();
                        let suffix: String = glyphs[end..(end + 80).min(glyphs.len())]
                            .iter()
                            .map(|g| g.ch)
                            .collect();
                        html = crate::annotations::source_image(
                            &html, &quote, &prefix, &suffix, image, after,
                        );
                    }
                }
            }
        }
        offset += line.len() + 1;
    }
    html
}

/// Parallel explanatory label columns immediately below a vector diagram
/// belong to that diagram, not to the surrounding body flow. Require two
/// aligned multi-line columns; never grow a crop by walking through prose or
/// a lone bold section title.
pub(crate) fn diagram_callout_bounds(page: &PdfPage<'_>, bounds: [f32; 4]) -> Option<[f32; 4]> {
    diagram_callouts(&glyphs(&page.text().ok()?), bounds, page.width().value)
}

pub(super) fn diagram_callouts(
    gs: &[Glyph],
    bounds: [f32; 4],
    page_width: f32,
) -> Option<[f32; 4]> {
    let rows: Vec<_> = gs
        .split(|g| g.ch == '\n')
        .filter_map(|line| {
            let visible: Vec<_> = line
                .iter()
                .filter(|g| !g.ch.is_whitespace() && !g.ch.is_control())
                .collect();
            let first = *visible.first()?;
            if visible.len() < 12
                || first.size <= 0.
                || visible.iter().any(|g| {
                    !g.bold
                        || g.italic
                        || g.mono
                        || (g.size - first.size).abs() > first.size * 0.05
                        || (g.y - first.y).abs() > first.size * 0.1
                })
            {
                return None;
            }
            let right = visible.iter().map(|g| g.right).fold(first.x, f32::max);
            (right - first.x <= page_width * 0.48).then_some((first, right))
        })
        .collect();
    let seeds: Vec<_> = rows
        .iter()
        .filter(|(g, right)| {
            bounds[1] - g.y >= 0.
                && bounds[1] - g.y <= g.size * 1.8
                && g.x < bounds[2]
                && *right > bounds[0]
        })
        .collect();
    let mut columns = Vec::new();
    for (first, first_right) in seeds {
        if columns
            .iter()
            .any(|b: &[f32; 4]| (b[0] - first.x).abs() <= first.size)
        {
            continue;
        }
        let mut bottom = first.y;
        let mut right = *first_right;
        let mut count = 1;
        loop {
            let next = rows
                .iter()
                .filter(|(g, _)| {
                    (g.x - first.x).abs() <= first.size * 0.5
                        && (g.size - first.size).abs() <= first.size * 0.05
                        && bottom - g.y >= first.size * 0.8
                        && bottom - g.y <= first.size * 1.6
                        && first.y - g.y <= first.size * 6.
                })
                .max_by(|(a, _), (b, _)| a.y.total_cmp(&b.y));
            let Some((g, edge)) = next else { break };
            bottom = g.y;
            right = right.max(*edge);
            count += 1;
        }
        if count >= 2 {
            columns.push([
                first.x,
                bottom - first.size * 0.3,
                right,
                first.y + first.size,
            ]);
        }
    }
    columns.sort_by(|a, b| a[0].total_cmp(&b[0]));
    if columns.len() < 2 || columns.windows(2).any(|p| p[0][2] >= p[1][0]) {
        return None;
    }
    let mut expanded = bounds;
    for b in columns {
        expanded = [
            expanded[0].min(b[0]),
            expanded[1].min(b[1]),
            expanded[2].max(b[2]),
            expanded[3].max(b[3]),
        ];
    }
    (expanded[2] - expanded[0] <= page_width * 0.85).then_some(expanded)
}
