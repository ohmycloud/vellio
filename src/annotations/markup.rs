//! Markdown and visible-character mapping that preserves source offsets.
use super::*;

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

pub(super) fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Each visible character retains its byte range in the HTML. Decorations can
/// then cross emphasis/link tags without consuming or corrupting those tags.
pub(super) fn characters(html: &str) -> Vec<(char, Range<usize>)> {
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
pub fn plain_text(html: &str) -> String {
    characters(html).iter().map(|(c, _)| c).collect()
}
