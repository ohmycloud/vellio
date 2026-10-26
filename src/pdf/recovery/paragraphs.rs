//! PDF paragraphs recovery.
use super::*;

/// Infer paragraph boundaries from visible body lines, after figures/tables
/// have been removed. Physical wraps stay together even without punctuation.
pub(super) fn paragraph_breaks(
    lines: &[&[Glyph]],
    omitted: &std::collections::HashSet<usize>,
) -> Vec<bool> {
    let body = |i: usize| {
        if omitted.contains(&i) {
            return None;
        }
        let first = lines[i]
            .iter()
            .find(|g| !g.ch.is_whitespace() && !g.ch.is_control())
            .filter(|g| !g.mono && !g.bold && g.size > 0.)?;
        // PDFium can put multiple physical wraps in one extracted line. Find
        // its first and last physical rows before choosing body baselines;
        // otherwise a longer second row looks like a large paragraph gap.
        let mut first_end = lines[i].len();
        let mut last_start = 0;
        let mut previous: Option<&Glyph> = None;
        for (index, g) in lines[i]
            .iter()
            .enumerate()
            .filter(|(_, g)| !g.ch.is_whitespace() && !g.ch.is_control())
        {
            if previous
                .is_some_and(|prev| prev.y - g.y >= first.size * 0.5 && prev.x - g.x >= first.size)
            {
                first_end = first_end.min(index);
                last_start = index;
            }
            previous = Some(g);
        }
        // Raised references do not define a physical row's baseline or indent.
        fn anchor(row: &[Glyph], size: f32) -> Option<&Glyph> {
            let mut baselines = std::collections::BTreeMap::<i32, usize>::new();
            for g in row
                .iter()
                .filter(|g| !g.ch.is_whitespace() && !g.ch.is_control())
            {
                *baselines.entry((g.y * 10.).round() as i32).or_default() += 1;
            }
            let baseline = baselines.into_iter().max_by_key(|(_, count)| *count)?.0;
            row.iter().find(|g| {
                !g.ch.is_whitespace()
                    && !g.ch.is_control()
                    && (g.y * 10.).round() as i32 == baseline
                    && (g.size - size).abs() <= size * 0.05
            })
        }
        Some((
            anchor(&lines[i][..first_end], first.size)?,
            anchor(&lines[i][last_start..], first.size)?,
        ))
    };
    let mut spacings = Vec::new();
    for i in 1..lines.len() {
        if let Some(((prev, prev_end), (next, _))) = body(i - 1).zip(body(i)) {
            let gap = prev_end.y - next.y;
            if (prev.size - next.size).abs() <= next.size * 0.05
                && (prev.x - next.x).abs() <= next.size * 0.5
                && gap >= next.size
                && gap <= next.size * 2.5
            {
                spacings.push((next.size, (gap / next.size * 20.).round() as u32));
            }
        }
    }
    let mut breaks = vec![false; lines.len()];
    for (i, boundary) in breaks.iter_mut().enumerate().skip(1) {
        let Some(((prev, prev_end), (next, _))) = body(i - 1).zip(body(i)) else {
            continue;
        };
        if (prev.size - next.size).abs() > next.size * 0.05 {
            continue;
        }
        let mut bins = std::collections::BTreeMap::<u32, usize>::new();
        for (_, gap) in spacings
            .iter()
            .filter(|(size, _)| (*size - next.size).abs() <= next.size * 0.05)
        {
            *bins.entry(*gap).or_default() += 1;
        }
        // Favor the smaller spacing only when frequencies tie.
        let typical = bins
            .into_iter()
            .max_by_key(|(gap, count)| (*count, std::cmp::Reverse(*gap)))
            .map_or(next.size * 1.25, |(gap, _)| next.size * gap as f32 / 20.);
        let gap = prev_end.y - next.y;
        *boundary =
            gap > typical * 1.3 || (next.x - prev.x >= next.size * 0.75 && gap >= typical * 0.8);
    }
    breaks
}

/// Hide only a standalone matching page number in the centered bottom margin.
/// This is presentation-only: source extraction and annotation records stay
/// intact. Numbers in prose, tables, equations and footnotes remain visible.
pub(super) fn page_number_lines(
    glyphs: &[Glyph],
    width: f32,
    height: f32,
    page_number: usize,
) -> std::collections::HashSet<usize> {
    let expected = page_number.to_string();
    glyphs
        .split(|g| g.ch == '\n')
        .enumerate()
        .filter_map(|(i, line)| {
            if line_text(line).trim() != expected {
                return None;
            }
            let visible: Vec<_> = line
                .iter()
                .filter(|g| !g.ch.is_whitespace() && !g.ch.is_control())
                .collect();
            let first = *visible.first()?;
            let last = *visible.last()?;
            (first.size > 0.
                && first.y > 0.
                && first.y < height * 0.08
                && ((first.x + last.right) * 0.5 - width * 0.5).abs() < width * 0.05
                && visible.iter().all(|g| {
                    !g.mono
                        && !g.bold
                        && g.link.is_none()
                        && (g.y - first.y).abs() < first.size * 0.1
                })
                && glyphs
                    .iter()
                    .filter(|g| {
                        !g.ch.is_whitespace()
                            && !g.ch.is_control()
                            && g.y > first.y + first.size * 2.
                    })
                    .count()
                    >= 20
                && !glyphs.iter().any(|g| {
                    !g.ch.is_whitespace()
                        && !g.ch.is_control()
                        && g.y > first.y + first.size * 0.2
                        && g.y < first.y + first.size * 2.
                }))
            .then_some(i)
        })
        .collect()
}

pub(super) fn caption_start(line: &[Glyph]) -> bool {
    let text = line_text(line);
    (text.starts_with("Figure ") || text.starts_with("Table "))
        && line
            .iter()
            .find(|g| !g.ch.is_whitespace())
            .is_some_and(|g| {
                !g.mono
                    && (g.italic
                        || text.split_whitespace().nth(1).is_some_and(|label| {
                            let number = label.trim_end_matches([':', '.']);
                            number != label
                                && number.chars().any(|c| c.is_ascii_digit())
                                && number
                                    .chars()
                                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
                        }))
            })
}

/// A caption can continue at the top of the following PDF page without its
/// Figure/Table prefix. Keep the smaller italic run separate from body text.
pub(super) fn leading_caption(lines: &[&[Glyph]], start: usize) -> bool {
    if caption_start(lines[start]) {
        return false;
    }
    if lines[..start]
        .iter()
        .any(|line| line.iter().any(|g| !g.ch.is_whitespace()))
    {
        return false;
    }
    let Some(first) = lines[start].iter().find(|g| !g.ch.is_whitespace()) else {
        return false;
    };
    if first.size <= 0. || first.mono || !first.italic {
        return false;
    }
    for i in start..(start + 4).min(lines.len()) {
        if !lines[i]
            .iter()
            .filter(|g| !g.ch.is_whitespace())
            .all(|g| g.italic && !g.mono && (g.size - first.size).abs() <= first.size * 0.05)
        {
            return false;
        }
        let Some(next) = lines
            .get(i + 1)
            .and_then(|line| line.iter().find(|g| !g.ch.is_whitespace()))
        else {
            return false;
        };
        if !next.italic && !next.mono && !next.bold {
            let current = lines[i].iter().find(|g| !g.ch.is_whitespace()).unwrap();
            return next.size >= first.size * 1.1
                && next.x <= first.x + first.size * 0.25
                && current.y - next.y >= first.size * 1.5;
        }
        if !caption_continuation(first, lines[i], lines[i + 1]) {
            return false;
        }
    }
    false
}

/// Centered captions can end with a single word on the next physical line.
/// Keep nearby lines in the same caption style together, stopping at body text
/// or a separate caption even when extraction supplies no blank separator.
pub(super) fn caption_continuation(first: &Glyph, previous: &[Glyph], next: &[Glyph]) -> bool {
    let Some(last) = previous.iter().find(|g| !g.ch.is_whitespace()) else {
        return false;
    };
    let Some(start) = next.iter().find(|g| !g.ch.is_whitespace()) else {
        return false;
    };
    let gap = last.y - start.y;
    !caption_start(next)
        && start.italic == first.italic
        && first.size > 0.
        && gap > first.size * 0.1
        && gap <= first.size * 1.8
        && next.iter().filter(|g| !g.ch.is_whitespace()).all(|g| {
            (g.italic == first.italic || !first.italic)
                && g.bold == first.bold
                && !g.mono
                && (g.size - first.size).abs() <= first.size * 0.05
        })
}
