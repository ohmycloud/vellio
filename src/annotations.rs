//! Render persisted quote annotations without changing the paper's Markdown.
use serde_json::Value;
use std::ops::Range;

/// Shared ink for PDF, Markdown and other document links in the reader.
pub const LINK_COLOR: u32 = 0x8e0012;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HighlightColor {
    #[default]
    Yellow,
    Green,
    Blue,
    Red,
}

impl HighlightColor {
    pub const ALL: [Self; 4] = [Self::Yellow, Self::Green, Self::Blue, Self::Red];
    pub fn from_name(name: &str) -> Self {
        match name {
            "green" => Self::Green,
            "blue" => Self::Blue,
            "pink" => Self::Red,
            _ => Self::Yellow,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Blue => "blue",
            Self::Red => "pink",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Yellow => "黄色",
            Self::Green => "绿色",
            Self::Blue => "蓝色",
            Self::Red => "红色",
        }
    }
    pub fn rgb(self) -> u32 {
        match self {
            Self::Yellow => 0xf2df8a,
            Self::Green => 0xd4e3c9,
            Self::Blue => 0xd2e0ed,
            Self::Red => 0xf0cec7,
        }
    }
    pub fn line_rgb(self) -> u32 {
        match self {
            Self::Yellow => 0xdca722,
            Self::Green => 0x58994c,
            Self::Blue => 0x477fc1,
            Self::Red => 0xcc413a,
        }
    }
}

pub fn html(source: &str) -> String {
    let mut options = markdown::Options::gfm();
    // Formula SVGs are application-generated data URI images. Keep image
    // sources while retaining the compiler's normal link protocol filtering.
    options.compile.allow_any_img_src = true;
    markdown::to_html_with_options(source, &options)
        .unwrap_or_else(|_| format!("<p>{}</p>", escape(source)))
        // GPUI treats an image directly inside a block as a block image.
        // An inline group keeps prose, formula images and marks in one flow.
        .replace("<p>", "<p><span>")
        .replace("</p>", "</span></p>")
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Each visible character retains its byte range in the HTML. Decorations can
/// then cross emphasis/link tags without consuming or corrupting those tags.
fn characters(html: &str) -> Vec<(char, Range<usize>)> {
    let mut chars = Vec::new();
    let mut offset = 0;
    let mut preformatted = false;
    while offset < html.len() {
        let rest = &html[offset..];
        if rest.starts_with('<') {
            let Some(end) = rest.find('>') else { break };
            let tag = &rest[..=end];
            if tag == "<pre>" || tag.starts_with("<pre ") {
                preformatted = true;
            } else if tag == "</pre>" {
                preformatted = false;
                chars.push(('\n', offset..offset));
            }
            if tag == "<br />" || tag == "<br>" || tag == "<br />\n" {
                chars.push(('\n', offset..offset));
            } else if tag == "</p>"
                || tag == "</li>"
                || tag == "</dt>"
                || tag == "</dd>"
                || tag.starts_with("</h")
            {
                chars.push(('\n', offset..offset));
                if tag != "</li>" && tag != "</dt>" {
                    chars.push(('\n', offset..offset));
                }
            }
            offset += end + 1;
            // to_html emits formatting newlines between block elements.
            if !preformatted && html[offset..].starts_with('\n') {
                offset += 1;
            }
            continue;
        }
        if rest.starts_with('&')
            && let Some(end) = rest.find(';').filter(|end| *end < 16)
        {
            let entity = &rest[1..end];
            let decoded = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => entity
                    .strip_prefix("#x")
                    .and_then(|v| u32::from_str_radix(v, 16).ok())
                    .or_else(|| entity.strip_prefix('#').and_then(|v| v.parse().ok()))
                    .and_then(char::from_u32),
            };
            if let Some(c) = decoded {
                chars.push((c, offset..offset + end + 1));
                offset += end + 1;
                continue;
            }
        }
        let c = rest.chars().next().unwrap();
        chars.push((c, offset..offset + c.len_utf8()));
        offset += c.len_utf8();
    }
    while chars.last().is_some_and(|(c, _)| *c == '\n') {
        chars.pop();
    }
    chars
}

/// Attach a source PDF link after prose reflow, using visible text and context.
/// Application annotations can still decorate this link without losing its target.
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

fn math_script_tags(html: &str) -> Vec<(Range<usize>, Range<usize>)> {
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
fn math_quote_range(text: &str, quote: &str, prefix: &str, suffix: &str) -> Option<Range<usize>> {
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

fn source_wrap(
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

pub fn plain_text(html: &str) -> String {
    characters(html).iter().map(|(c, _)| c).collect()
}

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

fn collapsed_whitespace(text: &str) -> (String, Vec<Range<usize>>) {
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

pub fn decorate(html: &str, notes: &[Value]) -> String {
    decorate_impl(html, notes, false)
}

pub fn decorate_interactive(html: &str, notes: &[Value]) -> String {
    decorate_impl(html, notes, true)
}

/// A transient geometry anchor, including notes with no visible decoration.
/// Wrap one source character inside its existing markup so emphasis, colors,
/// entities and the remaining annotation links stay intact.
pub fn navigation_anchor(html: &str, note: &Value) -> String {
    quote_anchor(html, note, "reader-navigation-target")
}

pub const MARGIN_EDITOR_ANCHOR: &str = "reader-margin-editor-target";

/// Locate a draft before it has a saved decoration. Keep the same anchor while
/// autosave adds the persistent note, so editing does not change its position.
pub fn margin_editor_anchor(html: &str, note: &Value) -> String {
    quote_anchor(html, note, MARGIN_EDITOR_ANCHOR)
}

fn quote_anchor(html: &str, note: &Value, id: &str) -> String {
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

fn decorate_impl(html: &str, notes: &[Value], interactive: bool) -> String {
    let chars = characters(html);
    let text: String = chars.iter().map(|(c, _)| c).collect();
    let mut marks: Vec<_> = notes
        .iter()
        .filter(|n| !n["deleted"].as_bool().unwrap_or(false))
        .filter(|n| n["decoration"] != "none")
        .filter_map(|n| {
            let mut range = quote_range(
                &text,
                n["quote"].as_str()?,
                n["prefix"].as_str().unwrap_or(""),
                n["suffix"].as_str().unwrap_or(""),
            )?;
            // GPUI trims whitespace at inline group edges. Leave boundary
            // spaces outside the mark so highlighting cannot join two words.
            let selected = &text[range.clone()];
            range.start += selected.len() - selected.trim_start().len();
            range.end -= selected.len() - selected.trim_end().len();
            let color = HighlightColor::from_name(n["color"].as_str().unwrap_or("yellow"));
            let updated = n["updated"]
                .as_str()
                .or_else(|| n["created"].as_str())
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|t| t.timestamp_millis())
                .unwrap_or(0);
            Some((
                range,
                n["style"] == "underline",
                color,
                updated,
                n["id"].as_str().unwrap_or(""),
                n["color"].is_string(),
                true,
            ))
        })
        .collect();
    if marks.is_empty() {
        return html.to_owned();
    }
    marks.sort_by(|a, b| (a.3, a.4).cmp(&(b.3, b.4)));
    let mut result = String::new();
    let mut previous_end = 0;
    let mut text_offset = 0;
    let mut active = (None, None, None);
    let close = |out: &mut String,
                 (highlight, underline, note): (
        Option<HighlightColor>,
        Option<Option<HighlightColor>>,
        Option<&str>,
    )| {
        if underline.is_some() {
            out.push_str("</u>");
        }
        if highlight.is_some() {
            out.push_str("</mark>");
        }
        if note.is_some() {
            out.push_str("</span>");
        }
    };
    let open = |out: &mut String,
                (highlight, underline, note): (
        Option<HighlightColor>,
        Option<Option<HighlightColor>>,
        Option<&str>,
    )| {
        if let Some(id) = note {
            out.push_str(&format!(
                "<span data-easyread-note=\"{}\">",
                escape(id).replace('"', "&quot;")
            ));
        }
        if let Some(color) = highlight {
            out.push_str(&format!("<mark color=\"#{:06x}\">", color.rgb()));
        }
        if let Some(color) = underline {
            if let Some(color) = color {
                out.push_str(&format!(
                    "<u style=\"text-decoration-color:#{:06x}\">",
                    color.line_rgb()
                ));
            } else {
                out.push_str("<u>");
            }
        }
    };
    for (c, source) in chars {
        let mut next = (None, None, None);
        for (range, underline, color, _, id, explicit_color, editable) in &marks {
            if range.contains(&text_offset) {
                if interactive && *editable && !id.is_empty() {
                    next.2 = Some(*id);
                }
                if *underline {
                    next.1 = Some(explicit_color.then_some(*color));
                } else {
                    next.0 = Some(*color);
                }
            }
        }
        text_offset += c.len_utf8();
        if source.is_empty() {
            continue;
        }
        if source.start != previous_end || active != next {
            close(&mut result, active);
            result.push_str(&html[previous_end..source.start]);
            open(&mut result, next);
            active = next;
        }
        result.push_str(&html[source.clone()]);
        previous_end = source.end;
    }
    close(&mut result, active);
    result.push_str(&html[previous_end..]);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_paragraph_scope_keeps_repeated_unicode_paragraphs_and_partial_selections_local() {
        let html = "<p>开头。</p><p>重复 <strong>段落</strong>。</p><blockquote><p>English companion.</p></blockquote><p>重复 <em>段落</em>。</p><p>结尾。</p>";
        let text = plain_text(html);
        let native = "开头。\n重复 段落。\nEnglish companion.\n重复 段落。\n结尾。\n";
        let start = native.match_indices("重复 段落。").nth(1).unwrap().0;
        let paragraph = start..start + "重复 段落。".len();
        let (quote, range) =
            rendered_paragraph_quote(&text, native, paragraph.clone(), None).unwrap();
        assert_eq!(quote, "重复 段落。");
        assert_eq!(
            range.start,
            text.match_indices("重复 段落。").nth(1).unwrap().0
        );
        let (quote, range) =
            rendered_paragraph_quote(&text, native, paragraph, Some("段落")).unwrap();
        assert_eq!(quote, "段落");
        assert_eq!(range.start, text.match_indices("段落").nth(1).unwrap().0);
    }

    #[test]
    fn unproven_native_scope_does_not_fall_back_to_the_page_or_first_duplicate() {
        assert!(
            rendered_paragraph_quote(
                "Same.\nSame.",
                "Missing lead.\nSame.\nMissing tail.",
                14..19,
                None
            )
            .is_none()
        );
        assert!(rendered_paragraph_quote("Page text", "", 0..0, None).is_none());
        assert!(rendered_paragraph_quote("Page text", "Native", 0..99, None).is_none());
        assert!(quote_range_in_context("Same. Same.", "Same.", "Missing", "").is_none());
        // Existing saved-note re-anchoring remains backward compatible.
        assert_eq!(
            quote_range("Same. Same.", "Same.", "Missing", ""),
            Some(0..5)
        );
    }

    #[test]
    fn typography_matches_wrapped_identifiers_and_repeated_unicode_with_context() {
        let html =
            "<p>Tools first; 更新 Tools second.</p><p>Reference https://example.org/longpath.</p>";
        let styled = source_typography(html, "Tools", "更新 ", " second.", (true, false, true));
        assert!(styled.contains("Tools first; 更新 <strong><code>Tools</code></strong> second."));
        let styled = source_typography(
            &styled,
            "https://example.org/long\u{2}\npath",
            "Reference ",
            ".",
            (false, true, false),
        );
        assert!(styled.contains("<em>https://example.org/longpath</em>"));
        assert_eq!(plain_text(html), plain_text(&styled));
    }
    use serde_json::json;

    #[test]
    fn repeated_reference_numbers_use_reflowed_context_before_first_match() {
        let html = "<p>18 Wu et al. 2025. First paper.</p><p>20 Zhang et al. 2022. OPT.</p><p>22 Alayrac et al. 2022. Flamingo.</p>";
        let linked = source_link(
            html,
            "20",
            "First paper.\n",
            " Zhang et al. 2022.",
            "easyread-pdf:1:100",
        );
        let marked = source_superscript(&linked, "20", "First paper.\n", " Zhang et al. 2022.");
        let marked = source_superscript(&marked, "22", "OPT.\n", " Alayrac et al. 2022.");
        assert!(
            marked.contains("<sup>20</sup></span></a> Zhang"),
            "{marked}"
        );
        assert!(marked.contains("<p><sup>22</sup> Alayrac"), "{marked}");
        assert!(marked.contains("2025. First"), "{marked}");
        assert!(marked.contains("2022. OPT"), "{marked}");
        assert_eq!(plain_text(&marked), plain_text(html));
    }

    #[test]
    fn source_link_keeps_boundary_spaces_outside_anchors() {
        for (source, quote, expected) in [
            (
                "<p>the website next</p>",
                " website ",
                ">website</span></a> next",
            ),
            (
                "<p>the <em> website </em> next</p>",
                " website ",
                ">website</span></a> </em>",
            ),
            (
                "<p>the&#32;web &amp; docs&#32;next</p>",
                " web & docs ",
                ">web &amp; docs</span></a>&#32;next",
            ),
            (
                "<p>the\u{2003}网址\u{2003}next</p>",
                "\u{2003}网址\u{2003}",
                ">网址</span></a>\u{2003}next",
            ),
        ] {
            let linked = source_link(source, quote, "", "", "https://example.org");
            assert!(linked.contains(expected), "{linked}");
            assert_eq!(plain_text(&linked), plain_text(source));
        }
    }

    #[test]
    fn pdf_links_cross_markup_and_use_repeated_unicode_quote_context() {
        let source = "<p><span>First 图 4-1, then <em>图 4</em>-1.</span></p>";
        let linked = source_link(source, "图 4-1", "then ", ".", "easyread-pdf:187:511");
        assert!(linked.contains("First 图 4-1, then <em><a href=\"easyread-pdf:187:511\""));
        assert!(linked.contains("</span></a></em><a href="));
        assert_eq!(plain_text(source), plain_text(&linked));
        let marked = decorate_interactive(
            &linked,
            &[
                serde_json::json!({"id":"note", "quote":"图 4-1", "prefix":"then ", "suffix":".", "kind":"highlight"}),
            ],
        );
        assert_eq!(plain_text(&marked), plain_text(source));
        assert!(marked.contains("easyread-pdf:187:511"));
    }

    #[test]
    fn code_annotations_preserve_newlines_indentation_and_source_colors() {
        let source = "<pre><code><span style=\"color:#000088\">messages</span> = [\n    <span style=\"color:#ff3300\">&quot;中文&quot;</span>,\n]\n</code></pre><p>After</p>";
        let text = plain_text(source);
        assert!(text.contains("messages = [\n    \"中文\",\n]"));
        let quote = "messages = [\n    \"中文\",";
        let note = json!({"id":"code-note", "quote":quote,"color":"green","style":"underline"});
        let result = decorate_interactive(source, &[note]);
        assert_eq!(plain_text(&result), text);
        assert!(result.contains("color:#000088"));
        assert!(result.contains("color:#ff3300"));
        assert!(result.contains("data-easyread-note=\"code-note\""));
        assert!(result.contains("text-decoration-color:#58994c"));
    }

    #[test]
    fn margin_editor_locates_unsaved_quote_and_keeps_anchor_after_save() {
        let source = html("Early quote.\n\nLater **quote & 中文** at the end.");
        let draft = json!({"id":"new-note", "quote":"quote & 中文",
            "prefix":"Later ", "suffix":" at the end.", "body":"", "style":"underline"});
        let anchored = margin_editor_anchor(&source, &draft);
        let marker = "<span data-easyread-note=\"reader-margin-editor-target\">q</span>";
        assert!(anchored.contains(&format!("Later <strong>{marker}uote &amp; 中文</strong>")));
        assert_eq!(plain_text(&anchored), plain_text(&source));
        assert!(!anchored.contains("<u"));
        assert!(!anchored.contains("<mark"));

        let mut saved = draft.clone();
        saved["body"] = json!("Saved note");
        let decorated = decorate_interactive(&source, &[saved.clone()]);
        let editing_saved = margin_editor_anchor(&decorated, &saved);
        assert!(editing_saved.contains(marker));
        assert_eq!(plain_text(&editing_saved), plain_text(&source));
        assert!(editing_saved.contains("new-note"));

        let attached = json!({"id":"attached", "quote":"quote & 中文",
            "prefix":"Later ", "decoration":"none", "source_annotation":"new-note"});
        assert!(margin_editor_anchor(&decorated, &attached).contains(marker));
        assert_eq!(
            margin_editor_anchor(&source, &json!({"quote":"missing"})),
            source
        );
        assert_eq!(margin_editor_anchor(&source, &json!({"quote":""})), source);
    }

    #[test]
    fn navigation_anchor_preserves_markup_and_uses_repeated_quote_context() {
        let source = html("first & then **& 中文** end");
        let note =
            json!({"quote":"& 中文", "prefix":"then ", "suffix":" end", "decoration":"none"});
        let anchored = navigation_anchor(&source, &note);
        assert_eq!(plain_text(&anchored), plain_text(&source));
        assert!(anchored.contains("<strong><span data-easyread-note=\"reader-navigation-target\">&amp;</span> 中文</strong>"));
        assert!(!anchored.contains("<mark"));
        assert_eq!(
            navigation_anchor(&source, &json!({"quote":"missing"})),
            source
        );
        assert_eq!(navigation_anchor(&source, &json!({"quote":""})), source);
    }

    #[test]
    fn legacy_colors_restore_and_latest_overlap_wins_without_losing_underline() {
        let notes = [
            json!({"id":"red","quote":"bc","color":"pink","updated":"2026-10-03T00:00:02Z"}),
            json!({"id":"green","quote":"abc","color":"green","updated":"2026-10-03T00:00:01Z"}),
            json!({"id":"underline","quote":"b","style":"underline"}),
        ];
        let result = decorate(&html("abc"), &notes);
        assert!(result.contains("#d4e3c9\">a</mark>"));
        assert!(result.contains("#f0cec7\"><u>b</u>"));
        assert_eq!(plain_text(&result), "abc");
        let reversed: Vec<_> = notes.into_iter().rev().collect();
        assert_eq!(result, decorate(&html("abc"), &reversed));
        assert!(
            decorate(&html("word"), &[json!({"quote":"word","color":"unknown"})])
                .contains("#f2df8a")
        );
    }

    #[test]
    fn interactive_marks_keep_identity_across_links_emphasis_and_overlap() {
        let source = html("前 **选中** [文字](https://example.org) 后");
        let notes = [
            json!({"id":"old", "kind":"highlight", "quote":"选中 文字", "color":"green", "updated":"2026-10-03T00:00:01Z"}),
            json!({"id":"new", "kind":"highlight", "quote":"文字", "style":"underline", "color":"blue", "updated":"2026-10-03T00:00:02Z"}),
            json!({"id":"deleted", "quote":"前", "deleted":true}),
            json!({"id":"question", "kind":"question", "quote":"后", "color":"yellow"}),
        ];
        let decorated = decorate_interactive(&source, &notes);
        assert!(decorated.contains("data-easyread-note=\"old\""));
        assert!(decorated.contains("data-easyread-note=\"new\""));
        assert!(!decorated.contains("data-easyread-note=\"deleted\""));
        assert!(decorated.contains("data-easyread-note=\"question\""));
        assert!(decorated.contains("href=\"https://example.org\""));
        assert_eq!(plain_text(&decorated), plain_text(&source));
        assert!(!decorate(&source, &notes).contains("data-easyread-note"));
    }

    #[test]
    fn colored_underlines_keep_independent_highlights_and_latest_line_color() {
        let source = html("前文 **选中文字** & 后文");
        let mut notes = vec![
            json!({"quote":"选中文字 & 后文", "color":"green"}),
            json!({"id":"old", "quote":"选中文字 & 后文", "style":"underline", "color":"yellow", "updated":"2026-10-03T00:00:01Z"}),
            json!({"id":"new", "quote":"中文字", "style":"underline", "color":"blue", "updated":"2026-10-03T00:00:02Z"}),
        ];
        let result = decorate(&source, &notes);
        assert!(result.contains("text-decoration-color:#477fc1\">中文字</u>"));
        assert!(result.contains("text-decoration-color:#dca722\">选</u>"));
        assert!(result.contains("<mark color=\"#d4e3c9\">"));
        assert_eq!(plain_text(&result), plain_text(&source));
        notes.reverse();
        assert_eq!(decorate(&source, &notes), result);
        for color in HighlightColor::ALL {
            let result = decorate(
                &source,
                &[json!({"quote":"选中文字", "style":"underline", "color":color.name()})],
            );
            assert!(result.contains(&format!("text-decoration-color:#{:06x}", color.line_rgb())));
            assert!(!result.contains("<mark"));
        }
    }

    #[test]
    fn visible_selection_matches_pdf_line_breaks_and_keeps_source_offsets() {
        let source = html("开头\nPDF   text\nwith **emphasis** 结束");
        let text = plain_text(&source);
        let range = quote_range(&text, "开头 PDF text with emphasis", "", "").unwrap();
        assert_eq!(&text[range.clone()], "开头\nPDF   text\nwith emphasis");
        assert_eq!(context(&text, range), (String::new(), " 结束".into()));
        let marked = decorate(&source, &[json!({"quote":"开头 PDF text with emphasis"})]);
        assert!(marked.contains("<strong><mark"));
        assert_eq!(plain_text(&marked), text);
        let repeated = "same\nquote then same\nquote";
        assert_eq!(
            quote_range(repeated, "same quote", "then ", ""),
            Some(16..26)
        );
        assert_eq!(quote_range("无关正文", "未选中", "", ""), None);
        let marked = decorate(
            &html("Native Markdown supports prose"),
            &[json!({"quote":"Markdown "})],
        );
        assert!(marked.contains("Markdown</mark> supports"));
    }

    #[test]
    fn recovered_math_uses_surviving_context_when_page_footer_is_removed() {
        let html = "<p>Small <em>dk</em> values. Scale by <em>√</em> <sup>1</sup> dk .</p><p>Mean and variance <em>dk</em>.</p>";
        let edits = vec![
            (
                "dk".into(),
                "Small ".into(),
                " values. Scale".into(),
                crate::formula::native_html("d_{k}", false),
            ),
            (
                "√\r\n1\r\ndk".into(),
                "Scale by ".into(),
                "\r\n.\r\nMean and variance".into(),
                crate::formula::native_html(r"\frac{1}{\sqrt{d_{k}}}", false),
            ),
            (
                "dk".into(),
                "Mean and variance ".into(),
                ".\r\n4".into(),
                crate::formula::native_html("d_{k}", false),
            ),
        ];
        let result = source_math_edits(html, &edits);
        assert_eq!(result.matches("data-vellio-math=").count(), 3, "{result}");
        assert!(result.contains(r"\frac{1}{\sqrt{d_{k}}}"));
        assert!(!result.contains("<sup>"));
        assert!(result.contains("Mean and variance <em><span data-vellio-math="));
        // Reordering the input edits must not change their source locations.
        assert_eq!(
            source_math_edits(html, &edits.into_iter().rev().collect::<Vec<_>>()),
            result
        );
    }

    #[test]
    fn recovered_math_drops_pdf_script_shift_but_retains_adjacent_footnotes() {
        let html = "<p>Use <em><sup>dk</sup></em> and <sup>4</sup> footnote.</p>";
        let object = crate::formula::native_html("d_{k}", false);
        let result = source_math_edits(
            html,
            &[("dk".into(), "Use ".into(), " and ".into(), object.clone())],
        );
        assert_eq!(
            result,
            format!("<p>Use <em>{object}</em> and <sup>4</sup> footnote.</p>")
        );
        assert_eq!(math_quote_range("dk and dk", "dk", "", ""), None);
        assert_eq!(math_quote_range("dk and dk", "", "", ""), None);
        assert_eq!(math_quote_range("dk和dk", "dk", "", "和dk"), Some(0..2));
        assert_eq!(math_quote_range("dk和dk", "dk", "dk和", ""), Some(5..7));
    }

    #[test]
    fn native_formula_objects_survive_html_conversion_and_decoration() {
        let source = crate::formula::native_markdown_html(
            r"行内公式 $x_k$ 与中文混排。 [link](javascript:alert(1))",
        );
        let marked = decorate(
            &source,
            &[json!({"quote":"与中文混排", "style":"underline"})],
        );
        assert!(marked.contains("data-vellio-math=\"x_k\""));
        assert!(!marked.contains("<img") && !marked.contains("data:image"));
        assert_eq!(plain_text(&marked), plain_text(&source));
        assert!(marked.contains("<u>与中文混排</u>"));
        assert!(marked.contains("href=\"\""));
    }

    #[test]
    fn annotations_cross_emphasis_and_entities_without_changing_text() {
        let source = html("前文 **selected** & 后文 🦀");
        let marked = decorate(
            &source,
            &[json!({"quote":"selected & 后文", "style":"underline"})],
        );
        assert!(marked.contains("<strong><u>selected</u></strong>"));
        assert!(marked.contains("<u> &amp; 后文</u>"));
        assert_eq!(plain_text(&marked), plain_text(&source));
    }

    #[test]
    fn attached_margin_note_does_not_cover_the_source_underline() {
        let source = html("words");
        let marked = decorate_interactive(
            &source,
            &[
                json!({"id":"mark","quote":"words","style":"underline","kind":"highlight","color":"blue"}),
                json!({"id":"note","quote":"words","kind":"note","color":"yellow","decoration":"none"}),
            ],
        );
        assert!(!marked.contains("<mark"));
        assert!(marked.contains("#477fc1"));
        assert!(marked.contains("data-easyread-note=\"mark\""));
    }

    #[test]
    fn repeated_quote_context_and_overlapping_marks_survive() {
        let source = html("first word then word end");
        let marked = decorate(
            &source,
            &[
                json!({"quote":"word", "prefix":"then ", "suffix":" end"}),
                json!({"quote":"word end", "style":"underline", "prefix":"then "}),
                json!({"quote":"first", "deleted":true}),
            ],
        );
        assert!(marked.contains("first word then <mark"));
        assert!(marked.contains("<u>word</u></mark><u> end</u>"));
        assert_eq!(plain_text(&marked), "first word then word end");
    }
}
