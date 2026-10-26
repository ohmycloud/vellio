//! PDF typography recovery.
use super::*;

/// Require a raised baseline, not merely a small digit. Some publishers use
/// full-size raised reference labels, so size alone cannot identify a script.
pub(super) fn superscripts(glyphs: &[Glyph]) -> Vec<bool> {
    let mut scripts: Vec<bool> = glyphs
        .iter()
        .enumerate()
        .map(|(i, g)| {
            if g.ch.is_whitespace() || g.mono || g.size <= 0. {
                return false;
            }
            glyphs[i.saturating_sub(60)..(i + 61).min(glyphs.len())]
                .iter()
                .filter(|body| !body.ch.is_whitespace() && !body.mono && body.size >= g.size * 1.2)
                .any(|body| {
                    let rise = g.y - body.y;
                    rise >= body.size * 0.15
                        && rise <= body.size * 0.65
                        && (g.x - body.x).abs() <= body.size * 2.
                })
        })
        .collect();
    // Full-size numeric runs need a neighbouring body anchor on the same
    // extracted line. This avoids borrowing a baseline from another paragraph
    // (or mistaking years and ordinary footnote labels for superscripts).
    let mut offset = 0;
    for line in glyphs.split(|g| g.ch == '\n') {
        let mut i = 0;
        while i < line.len() {
            if !line[i].ch.is_ascii_digit() || line[i].mono || line[i].size <= 0. {
                i += 1;
                continue;
            }
            let start = i;
            let first = &line[start];
            i += 1;
            while i < line.len()
                && (line[i].ch.is_ascii_digit() || line[i].ch == ',')
                && !line[i].mono
                && (line[i].size - first.size).abs() <= first.size * 0.05
                && (line[i].y - first.y).abs() <= first.size * 0.05
                && line[i].x >= line[i - 1].x
                && line[i].x - line[i - 1].x <= first.size * 1.2
            {
                i += 1;
            }
            let anchors = [
                line[..start].iter().rev().find(|g| !g.ch.is_whitespace()),
                line[i..].iter().find(|g| !g.ch.is_whitespace()),
            ];
            if anchors.into_iter().flatten().any(|body| {
                let rise = first.y - body.y;
                !body.mono
                    && !body.ch.is_ascii_digit()
                    && (body.size - first.size).abs() <= first.size * 0.05
                    && rise >= body.size * 0.2
                    && rise <= body.size * 0.65
                    && (start..i).any(|j| (line[j].x - body.x).abs() <= body.size * 2.)
            }) {
                scripts[offset + start..offset + i].fill(true);
            }
        }
        offset += line.len() + 1;
    }
    // Recognize the whole raised run from its nearby body anchor. The last
    // digit of a multi-reference marker can be beyond the body's proximity
    // limit; punctuation may also use a different font on the same baseline.
    let mut start = 0;
    while start < glyphs.len() {
        let first = &glyphs[start];
        if first.ch.is_whitespace() || first.mono || first.size <= 0. {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < glyphs.len() {
            let previous = &glyphs[end - 1];
            let next = &glyphs[end];
            if next.ch.is_whitespace()
                || next.mono
                || (next.size - first.size).abs() > first.size * 0.05
                || (next.y - first.y).abs() > first.size * 0.05
                || next.x < previous.x
                || next.x - previous.x > first.size * 1.2
            {
                break;
            }
            end += 1;
        }
        if scripts[start..end].iter().any(|script| *script) {
            scripts[start..end].fill(true);
        }
        start = end;
    }
    scripts
}

/// A raised numeric label to the left of its body starts an independent item.
/// Physical wraps inside that item still pass through ordinary prose reflow.
pub(super) fn raised_item_start(line: &[Glyph], scripts: &[bool]) -> bool {
    let Some(start) = line.iter().position(|g| !g.ch.is_whitespace()) else {
        return false;
    };
    let end = (start..line.len())
        .find(|&i| !scripts[i] || !line[i].ch.is_ascii_digit())
        .unwrap_or(line.len());
    if end == start {
        return false;
    }
    let Some(body) = line[end..].iter().find(|g| !g.ch.is_whitespace()) else {
        return false;
    };
    let marker = &line[end - 1];
    !body.mono
        && body.ch.is_alphabetic()
        && marker.x < body.x
        && body.x - marker.x <= body.size * 3.
        && marker.y - body.y >= body.size * 0.15
        && marker.y - body.y <= body.size * 0.65
}

pub(super) fn apply_superscripts(glyphs: &[Glyph], scripts: &[bool], mut html: String) -> String {
    let mut i = 0;
    while i < glyphs.len() {
        if !scripts[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i < glyphs.len() && scripts[i] {
            i += 1;
        }
        let quote: String = glyphs[start..i].iter().map(|g| g.ch).collect();
        let prefix: String = glyphs[start.saturating_sub(40)..start]
            .iter()
            .map(|g| g.ch)
            .collect();
        let suffix: String = glyphs[i..(i + 40).min(glyphs.len())]
            .iter()
            .map(|g| g.ch)
            .collect();
        html = crate::annotations::source_superscript(&html, &quote, &prefix, &suffix);
    }
    html
}

pub(super) fn apply_typography(glyphs: &[Glyph], eligible: &[bool], mut html: String) -> String {
    let mut i = 0;
    while i < glyphs.len() {
        let first = &glyphs[i];
        let style = (first.mono, first.italic, first.bold);
        if !eligible[i]
            || first.ch.is_whitespace()
            || first.ch.is_control()
            || style == (false, false, false)
        {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < glyphs.len()
            && eligible[i]
            && (glyphs[i].ch.is_whitespace()
                || glyphs[i].ch.is_control()
                || (glyphs[i].mono, glyphs[i].italic, glyphs[i].bold) == style)
        {
            i += 1;
        }
        let quote: String = glyphs[start..i].iter().map(|g| g.ch).collect();
        let mut before: Vec<_> = glyphs[..start]
            .iter()
            .rev()
            .take_while(|g| g.ch != '\n')
            .take(80)
            .map(|g| g.ch)
            .collect();
        before.reverse();
        let prefix: String = before.into_iter().collect();
        let suffix: String = glyphs[i..]
            .iter()
            .take_while(|g| g.ch != '\n')
            .take(80)
            .map(|g| g.ch)
            .filter(|ch| !ch.is_control() || ch.is_whitespace())
            .collect();
        html = crate::annotations::source_typography(&html, &quote, &prefix, &suffix, style);
    }
    html
}
