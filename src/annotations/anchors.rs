//! Quote matching, context and navigation anchors.
use super::*;

pub fn quote_range(text: &str, quote: &str, prefix: &str, suffix: &str) -> Option<Range<usize>> {
    quote_range_in_context(text, quote, prefix, suffix)
        .or_else(|| {
            text.match_indices(quote)
                .next()
                .filter(|_| !quote.is_empty())
                .map(|(i, _)| i..i + quote.len())
        })
        .or_else(|| {
            let (visible, offsets) = collapsed_whitespace(text);
            let (quote, _) = collapsed_whitespace(quote);
            if quote.is_empty() {
                return None;
            }
            visible
                .match_indices(&quote)
                .next()
                .map(|(i, _)| offsets[i].start..offsets[i + quote.len() - 1].end)
        })
}

/// Strict contextual matching for newly captured selections: unlike legacy
/// re-anchoring, do not silently attach an ambiguous quote to the first match.
pub fn quote_range_in_context(
    text: &str,
    quote: &str,
    prefix: &str,
    suffix: &str,
) -> Option<Range<usize>> {
    if quote.is_empty() {
        return None;
    }
    // Match context first, so the second occurrence of a repeated phrase stays
    // attached to the selected occurrence after saving and reopening.
    let exact = text
        .match_indices(quote)
        .find(|(i, _)| text[..*i].ends_with(prefix) && text[*i + quote.len()..].starts_with(suffix))
        .map(|(i, _)| i..i + quote.len());
    if exact.is_some() {
        return exact;
    }
    // HTML collapses source whitespace, including PDF extraction line breaks.
    // Match the visible quote while keeping offsets in the original text for
    // context and decoration; never rewrite the stored paper or selected quote.
    let (visible, offsets) = collapsed_whitespace(text);
    let (visible_quote, _) = collapsed_whitespace(quote);
    let (prefix, _) = collapsed_whitespace(prefix);
    let (suffix, _) = collapsed_whitespace(suffix);
    visible
        .match_indices(&visible_quote)
        .find(|(i, _)| {
            visible[..*i].ends_with(&prefix)
                && visible[*i + visible_quote.len()..].starts_with(&suffix)
        })
        .map(|(i, _)| offsets[i].start..offsets[i + visible_quote.len() - 1].end)
}

/// Anchor a click/selection using the native paragraph's text and surrounding
/// context, while retaining offsets in the undecorated application HTML.
/// None means the scope cannot be proven; never replace it with the whole page.
pub fn rendered_paragraph_quote(
    text: &str,
    native_text: &str,
    paragraph: Range<usize>,
    selected: Option<&str>,
) -> Option<(String, Range<usize>)> {
    let body = native_text.get(paragraph.clone())?;
    let quote = selected
        .map(str::to_owned)
        .unwrap_or_else(|| body.trim().to_owned());
    if quote.trim().is_empty() {
        return None;
    }
    let native_range = if let Some(range) = quote_range(body, &quote, "", "") {
        paragraph.start + range.start..paragraph.start + range.end
    } else if selected.is_some() {
        // A real drag may deliberately span several paragraphs in one block.
        let range = quote_range(native_text.get(paragraph.start..)?, &quote, "", "")?;
        paragraph.start + range.start..paragraph.start + range.end
    } else {
        return None;
    };
    let (prefix, suffix) = context(native_text, native_range);
    let range = quote_range_in_context(text, &quote, prefix.trim_start(), suffix.trim_end())
        .or_else(|| {
            // Different structural projections (e.g. table cells) may have
            // different separators. A unique quote is safe; repeats are not.
            let (visible, _) = collapsed_whitespace(text);
            let (quote, _) = collapsed_whitespace(&quote);
            (visible.match_indices(&quote).count() == 1)
                .then(|| quote_range(text, &quote, "", ""))
                .flatten()
        })?;
    Some((quote, range))
}

pub(super) fn collapsed_whitespace(text: &str) -> (String, Vec<Range<usize>>) {
    let mut visible = String::new();
    let mut offsets: Vec<Range<usize>> = Vec::new();
    let mut whitespace = false;
    for (i, c) in text.char_indices() {
        let end = i + c.len_utf8();
        if c.is_whitespace() {
            if whitespace {
                offsets.last_mut().unwrap().end = end;
            } else {
                visible.push(' ');
                offsets.push(i..end);
            }
            whitespace = true;
        } else {
            visible.push(c);
            offsets.extend(std::iter::repeat_n(i..end, c.len_utf8()));
            whitespace = false;
        }
    }
    (visible, offsets)
}

pub fn context(text: &str, range: Range<usize>) -> (String, String) {
    (
        text[..range.start]
            .chars()
            .rev()
            .take(32)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect(),
        text[range.end..].chars().take(32).collect(),
    )
}

pub fn navigation_anchor(html: &str, note: &Value) -> String {
    quote_anchor(html, note, "reader-navigation-target")
}

pub const MARGIN_EDITOR_ANCHOR: &str = "reader-margin-editor-target";

/// Locate a draft before it has a saved decoration. Keep the same anchor while
/// autosave adds the persistent note, so editing does not change its position.
pub fn margin_editor_anchor(html: &str, note: &Value) -> String {
    quote_anchor(html, note, MARGIN_EDITOR_ANCHOR)
}

pub(super) fn quote_anchor(html: &str, note: &Value, id: &str) -> String {
    let chars = characters(html);
    let text: String = chars.iter().map(|(c, _)| c).collect();
    let Some(range) = quote_range(
        &text,
        note["quote"].as_str().unwrap_or(""),
        note["prefix"].as_str().unwrap_or(""),
        note["suffix"].as_str().unwrap_or(""),
    ) else {
        return html.to_owned();
    };
    let mut offset = 0;
    for (c, source) in chars {
        let selected = range.contains(&offset) && !c.is_whitespace() && !source.is_empty();
        offset += c.len_utf8();
        if selected {
            return format!(
                "{}<span data-easyread-note=\"{id}\">{}</span>{}",
                &html[..source.start],
                &html[source.clone()],
                &html[source.end..]
            );
        }
    }
    html.to_owned()
}
