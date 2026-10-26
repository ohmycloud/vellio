//! PDF headings recovery.
use super::*;

pub(super) fn normalized_title(text: &str) -> String {
    text.replace('\u{ad}', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
/// Only whole physical lines on a bookmark's destination page are eligible.
/// Wrapped titles must keep the same typography; references inside sentences,
/// index entries and similarly worded headings on other pages remain prose.
pub(super) fn bookmarked_heading(
    lines: &[&[Glyph]],
    start: usize,
    headings: &[crate::pdf::OutlineEntry],
) -> Option<(usize, usize, String)> {
    let first = lines.get(start)?.iter().find(|g| !g.ch.is_whitespace())?;
    let mut title = String::new();
    for (offset, line) in lines[start..].iter().take(6).enumerate() {
        let visible: Vec<_> = line.iter().filter(|g| !g.ch.is_whitespace()).collect();
        if visible.is_empty()
            || visible
                .iter()
                .any(|g| g.mono || !heading_size_matches(first, g) || g.bold != first.bold)
        {
            break;
        }
        if !title.is_empty() {
            title.push(' ');
        }
        title.push_str(&normalized_title(&line_text(line)));
        if let Some(entry) = headings
            .iter()
            .find(|entry| normalized_title(&entry.title) == title)
        {
            return Some((offset + 1, (entry.depth + 1).min(6), title));
        }
        if !headings
            .iter()
            .any(|entry| normalized_title(&entry.title).starts_with(&format!("{title} ")))
        {
            break;
        }
    }
    None
}

/// Small-cap fonts encode lowercase letters at about 80% of the initial
/// capital's size. They still form a single title on the same baseline, not a
/// mixed-size body line or a title with a raised reference.
pub(super) fn heading_size_matches(first: &Glyph, glyph: &Glyph) -> bool {
    (glyph.size - first.size).abs() <= first.size * 0.05
        || (glyph.ch.is_lowercase()
            && glyph.size >= first.size * 0.75
            && glyph.size <= first.size * 0.85
            && (glyph.y - first.y).abs() <= first.size * 0.1)
}

/// Recover unbookmarked section titles only with a complete, distinctly bold
/// physical line followed by ordinary body text or an aligned, smaller bold
/// subheading. Inline emphasis stays prose.
pub(super) fn typographic_heading(
    lines: &[&[Glyph]],
    start: usize,
) -> Option<(usize, usize, String)> {
    let first = lines.get(start)?.iter().find(|g| !g.ch.is_whitespace())?;
    if !first.bold || first.mono {
        return None;
    }
    let mut title = String::new();
    for (offset, line) in lines[start..].iter().take(6).enumerate() {
        let visible: Vec<_> = line.iter().filter(|g| !g.ch.is_whitespace()).collect();
        if visible.is_empty()
            || visible
                .iter()
                .any(|g| !g.bold || g.mono || !heading_size_matches(first, g))
        {
            break;
        }
        if !title.is_empty() {
            title.push(' ');
        }
        title.push_str(&normalized_title(&line_text(line)));
        if title.chars().count() > 100
            || title.ends_with(['.', '?', '!', ':', ';', '。', '？', '！'])
        {
            return None;
        }
        let next_line = lines.get(start + offset + 1)?;
        let next = next_line.iter().find(|g| !g.ch.is_whitespace())?;
        let smaller_subheading = next.bold
            && first.size >= next.size * 1.1
            && (next.x - first.x).abs() <= first.size * 0.5
            && visible[0].y - next.y >= first.size * 0.75
            && next_line
                .iter()
                .filter(|g| !g.ch.is_whitespace())
                .all(|g| g.bold && !g.mono && (g.size - next.size).abs() <= next.size * 0.05);
        if (!next.bold && !next.mono && first.size >= next.size * 0.95 && first.y > next.y)
            || smaller_subheading
        {
            return Some((offset + 1, 3, title));
        }
    }
    None
}

pub(super) fn reading_heading(
    lines: &[&[Glyph]],
    start: usize,
    headings: &[crate::pdf::OutlineEntry],
) -> Option<(usize, usize, String)> {
    bookmarked_heading(lines, start, headings).or_else(|| {
        typographic_heading(lines, start).filter(|(_, _, title)| {
            !headings
                .iter()
                .any(|entry| normalized_title(&entry.title).starts_with(&format!("{title} ")))
        })
    })
}

/// A list label is not a chapter heading. Preserve its italic presentation and
/// paragraph boundaries, using the PDF typography rather than the marker alone.
pub(super) fn label_prefix(text: &str) -> Option<usize> {
    let end = text.find(char::is_whitespace)?;
    let marker = &text[..end];
    let token = if marker.starts_with('(') && marker.ends_with(')') {
        &marker[1..marker.len() - 1]
    } else {
        marker.strip_suffix(['.', ')'])?
    };
    let numeric =
        !token.is_empty() && token.len() <= 3 && token.chars().all(|c| c.is_ascii_digit());
    let alphabetic = token.len() == 1 && token.chars().all(|c| c.is_ascii_alphabetic());
    let roman = matches!(
        token.to_ascii_uppercase().as_str(),
        "II" | "III" | "IV" | "VI" | "VII" | "VIII" | "IX" | "XI" | "XII"
    );
    (numeric || alphabetic || roman).then_some(end)
}

pub(super) fn italic_label(lines: &[&[Glyph]], start: usize) -> Option<(usize, String)> {
    let text = normalized_title(&line_text(lines[start]));
    let numbered = label_prefix(&text);
    let prefix = numbered.unwrap_or(0);
    let title_start = text[prefix..]
        .trim_start_matches([' ', '('])
        .chars()
        .next()?;
    if !title_start.is_alphabetic() || (numbered.is_some() && title_start.is_lowercase()) {
        return None;
    }
    let first = lines[start].iter().find(|g| !g.ch.is_whitespace())?;
    if first.size <= 0. || first.mono {
        return None;
    }
    // PDFium can split a font run into an extraction line on the same baseline.
    if start > 0
        && lines[start - 1]
            .iter()
            .rev()
            .find(|g| !g.ch.is_whitespace())
            .is_some_and(|g| (g.y - first.y).abs() <= first.size * 0.2)
    {
        return None;
    }
    let mut title = String::new();
    for (offset, line) in lines[start..].iter().take(6).enumerate() {
        let line_start = line.iter().find(|g| !g.ch.is_whitespace())?;
        if offset > 0
            && (label_prefix(&normalized_title(&line_text(line))).is_some()
                || !caption_continuation(first, lines[start + offset - 1], line)
                || line_start.x < first.x - first.size * 0.5)
        {
            return None;
        }
        // The marker may be roman; all of the label's words must be italic.
        let marker_chars = if offset == 0 {
            text[..prefix].chars().count()
        } else {
            0
        };
        let words: Vec<_> = line
            .iter()
            .skip_while(|g| g.ch.is_whitespace())
            .skip(marker_chars)
            .filter(|g| !g.ch.is_whitespace())
            .collect();
        if words.is_empty()
            || words.iter().any(|g| {
                !g.italic || g.mono || g.bold || (g.size - first.size).abs() > first.size * 0.05
            })
        {
            return None;
        }
        if !title.is_empty() {
            title.push(' ');
        }
        title.push_str(&normalized_title(&line_text(line)));
        if title.chars().count() > 180 || title.ends_with(['.', '?', '!', ';', '。', '？', '！'])
        {
            return None;
        }
        let next = lines
            .get(start + offset + 1)?
            .iter()
            .find(|g| !g.ch.is_whitespace())?;
        if !next.italic
            && !next.bold
            && !next.mono
            && first.size >= next.size * 0.95
            && line_start.y - next.y > first.size * 0.2
        {
            // Unnumbered labels need layout evidence as well as italics:
            // an indented explanation or extra separation from its body.
            // This keeps ordinary italic sentences and quotations in prose.
            if numbered.is_none()
                && (title.chars().count() > 100
                    || title.split_whitespace().count() > 14
                    || (next.x < first.x + first.size * 0.75
                        && (title_start.is_lowercase()
                            || line_start.y - next.y < first.size * 1.6)))
            {
                return None;
            }
            return Some((offset + 1, title));
        }
    }
    None
}
