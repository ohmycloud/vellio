//! Apply recovered source links, typography, images and math without editing storage.
use super::*;

pub(crate) fn source_link(
    html: &str,
    quote: &str,
    prefix: &str,
    suffix: &str,
    url: &str,
) -> String {
    let url = escape(url).replace('"', "&quot;");
    source_wrap(html, quote, prefix, suffix, |text| {
        format!("<a href=\"{url}\"><span style=\"color:#{LINK_COLOR:06x}\">{text}</span></a>")
    })
}

/// Replace an entire source expression across nested font/script tags with
/// one atomic native math node, while preserving adjacent prose and markup.
pub(crate) fn source_math_edits(html: &str, edits: &[(String, String, String, String)]) -> String {
    // Do not match original PDF fragments inside already reconstructed display
    // math. Resolve ALL ranges before editing, so repeated d/k/T variables do
    // not drift into prose or into newly inserted TeX.
    let protected: Vec<_> = html
        .match_indices("<span data-vellio-math=")
        .filter_map(|(i, _)| html[i..].find("</span>").map(|n| i..i + n + 7))
        .collect();
    let chars: Vec<_> = characters(html)
        .into_iter()
        .filter(|(_, r)| !protected.iter().any(|p| p.contains(&r.start)))
        .collect();
    let text: String = chars.iter().map(|(c, _)| c).collect();
    let scripts = math_script_tags(html);
    let mut replacements = vec![];
    for (quote, prefix, suffix, object) in edits {
        let Some(range) = math_quote_range(&text, quote, prefix, suffix) else {
            continue;
        };
        let mut segments: Vec<Range<usize>> = vec![];
        let mut offset = 0;
        for (ch, location) in &chars {
            if range.contains(&offset) && !location.is_empty() {
                if let Some(last) = segments.last_mut().filter(|r| r.end == location.start) {
                    last.end = location.end;
                } else {
                    segments.push(location.clone());
                }
            }
            offset += ch.len_utf8();
        }
        // Typst already owns scripts and the baseline. Remove fully consumed
        // PDF script wrappers, or the native object inherits a second shift
        // and a reduced font size (and numerator wrappers are left empty).
        for (open, close) in &scripts {
            let mut offset = 0;
            let mut selected = false;
            let mut outside = false;
            for (ch, location) in &chars {
                if location.start >= open.end
                    && location.end <= close.start
                    && !location.is_empty()
                    && !ch.is_whitespace()
                {
                    selected |= range.contains(&offset);
                    outside |= !range.contains(&offset);
                }
                offset += ch.len_utf8();
            }
            if selected && !outside {
                replacements.extend([
                    (open.clone(), String::new()),
                    (close.clone(), String::new()),
                ]);
            }
        }
        for (i, r) in segments.into_iter().enumerate() {
            replacements.push((
                r,
                if i == 0 {
                    object.clone()
                } else {
                    String::new()
                },
            ));
        }
    }
    replacements.sort_by_key(|(r, _)| r.start);
    // Conflicting matches mean extraction order/context was ambiguous. Keep
    // source text rather than silently consuming a neighboring expression.
    if replacements.windows(2).any(|r| r[0].0.end > r[1].0.start) {
        return html.to_owned();
    }
    let mut out = html.to_owned();
    for (r, object) in replacements.into_iter().rev() {
        out.replace_range(r, &object);
    }
    out
}

pub(super) fn math_script_tags(html: &str) -> Vec<(Range<usize>, Range<usize>)> {
    let mut stack = Vec::new();
    let mut tags = Vec::new();
    for (i, _) in html.match_indices('<') {
        let Some(end) = html[i..].find('>') else {
            break;
        };
        let tag = &html[i + 1..i + end];
        if tag == "sup" || tag == "sub" {
            stack.push((tag, i..i + end + 1));
        } else if let Some(name) = tag.strip_prefix('/')
            && stack.last().is_some_and(|(open, _)| *open == name)
        {
            let (_, open) = stack.pop().unwrap();
            tags.push((open, i..i + end + 1));
        }
    }
    tags
}

/// PDF recovery can remove page numbers, reflow line breaks, or substitute a
/// neighboring display equation. Rank repeated expressions by the surviving
/// context rather than falling back to the first identical variable.
pub(super) fn math_quote_range(
    text: &str,
    quote: &str,
    prefix: &str,
    suffix: &str,
) -> Option<Range<usize>> {
    let (visible, offsets) = collapsed_whitespace(text);
    let (quote, _) = collapsed_whitespace(quote);
    let (prefix, _) = collapsed_whitespace(prefix);
    let (suffix, _) = collapsed_whitespace(suffix);
    if quote.is_empty() {
        return None;
    }
    let candidates: Vec<_> = visible.match_indices(&quote).map(|(i, _)| i).collect();
    let mut scored: Vec<_> = candidates
        .iter()
        .map(|&i| {
            let before = visible[..i].chars().rev();
            let after = visible[i + quote.len()..].chars();
            let score = before
                .zip(prefix.chars().rev())
                .take_while(|(a, b)| a == b)
                .count()
                + after
                    .zip(suffix.chars())
                    .take_while(|(a, b)| a == b)
                    .count();
            (score, i)
        })
        .collect();
    scored.sort_unstable_by_key(|a| std::cmp::Reverse(a.0));
    let &(score, i) = scored.first()?;
    let full_context =
        visible[..i].ends_with(&prefix) && visible[i + quote.len()..].starts_with(&suffix);
    if candidates.len() > 1 && ((!full_context && score < 4) || scored[1].0 == score) {
        return None;
    }
    Some(offsets[i].start..offsets[i + quote.len() - 1].end)
}

pub(crate) fn source_superscript(html: &str, quote: &str, prefix: &str, suffix: &str) -> String {
    source_wrap(html, quote, prefix, suffix, |text| {
        format!("<sup>{text}</sup>")
    })
}

pub(crate) fn source_highlight(
    html: &str,
    quote: &str,
    prefix: &str,
    suffix: &str,
    color: u32,
) -> String {
    source_wrap(html, quote, prefix, suffix, |text| {
        format!("<mark color=\"#{color:06x}\">{text}</mark>")
    })
}

/// Restore a PDF's inline font run after whitespace reflow without changing
/// visible text or crossing existing links, annotations or HTML leaves.
pub(crate) fn source_typography(
    html: &str,
    quote: &str,
    prefix: &str,
    suffix: &str,
    style: (bool, bool, bool),
) -> String {
    // A wrapped URL or discretionary PDF break can be joined without a space.
    // Match font runs through those layout separators while retaining the
    // original HTML offsets and occurrence context.
    let text = plain_text(html);
    let compact = |s: &str| {
        s.chars()
            .filter(|c| !c.is_whitespace() && !c.is_control())
            .collect::<String>()
    };
    let mut visible = String::new();
    let mut offsets = Vec::new();
    for (i, ch) in text.char_indices() {
        if !ch.is_whitespace() && !ch.is_control() {
            visible.push(ch);
            offsets.extend(std::iter::repeat_n(i..i + ch.len_utf8(), ch.len_utf8()));
        }
    }
    let needle = compact(quote);
    if needle.is_empty() {
        return html.to_owned();
    }
    let prefix = compact(prefix);
    let suffix = compact(suffix);
    let candidates: Vec<_> = visible.match_indices(&needle).collect();
    let matched = candidates.iter().max_by_key(|(i, _)| {
        (
            usize::from(visible[..*i].ends_with(&prefix))
                + usize::from(visible[*i + needle.len()..].starts_with(&suffix)),
            std::cmp::Reverse(*i),
        )
    });
    let Some((start, _)) = matched else {
        return html.to_owned();
    };
    let range = offsets[*start].start..offsets[*start + needle.len() - 1].end;
    let quote = &text[range.clone()];
    let prefix = &text[..range.start];
    let suffix = &text[range.end..];
    source_wrap(html, quote, prefix, suffix, |text| {
        let (mono, italic, bold) = style;
        let text = if mono {
            format!("<code>{text}</code>")
        } else {
            text.to_owned()
        };
        let text = if italic {
            format!("<em>{text}</em>")
        } else {
            text
        };
        if bold {
            format!("<strong>{text}</strong>")
        } else {
            text
        }
    })
}

/// An empty-alt inline object adds no characters or annotation offsets.
pub(crate) fn source_image(
    html: &str,
    quote: &str,
    prefix: &str,
    suffix: &str,
    image: &str,
    after: bool,
) -> String {
    let chars = characters(html);
    let text: String = chars.iter().map(|(c, _)| c).collect();
    let Some(range) = quote_range(&text, quote, prefix.trim_start(), suffix.trim_end()) else {
        return html.into();
    };
    let mut offset = 0;
    let mut position = None;
    for (ch, location) in chars {
        if range.contains(&offset) && !location.is_empty() {
            position = Some(if after { location.end } else { location.start });
            if !after {
                break;
            }
        }
        offset += ch.len_utf8();
    }
    let Some(position) = position else {
        return html.into();
    };
    format!(
        "{}<span>{image}</span>{}",
        &html[..position],
        &html[position..]
    )
}

pub(super) fn source_wrap(
    html: &str,
    quote: &str,
    prefix: &str,
    suffix: &str,
    wrap: impl Fn(&str) -> String,
) -> String {
    let chars = characters(html);
    let text: String = chars.iter().map(|(c, _)| c).collect();
    let Some(range) = quote_range(&text, quote, prefix, suffix) else {
        return html.into();
    };
    let mut offset = 0;
    let mut segments: Vec<Range<usize>> = Vec::new();
    for (ch, location) in chars {
        let selected = offset >= range.start && offset < range.end;
        offset += ch.len_utf8();
        if !selected || location.is_empty() {
            continue;
        }
        if let Some(last) = segments
            .last_mut()
            .filter(|last| last.end == location.start)
        {
            last.end = location.end;
        } else {
            segments.push(location);
        }
    }
    let mut html = html.to_owned();
    // Keep anchors inside each existing text leaf. A link across emphasis or
    // colored code spans must not cross their opening/closing HTML tags.
    for segment in segments.into_iter().rev() {
        // Preserve word separators in the surrounding text leaf, outside the
        // clickable/underlined range. Decode entities when finding the edges.
        let visible = characters(&html[segment.clone()]);
        let Some((_, first)) = visible.iter().find(|(ch, _)| !ch.is_whitespace()) else {
            continue;
        };
        let (_, last) = visible.iter().rfind(|(ch, _)| !ch.is_whitespace()).unwrap();
        let segment = segment.start + first.start..segment.start + last.end;
        let linked = wrap(&html[segment.clone()]);
        html.replace_range(segment, &linked);
    }
    html
}
