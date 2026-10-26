//! Paint persisted highlights and interactive annotation metadata.
use super::*;

pub fn decorate(html: &str, notes: &[Value]) -> String {
    decorate_impl(html, notes, false)
}

pub fn decorate_interactive(html: &str, notes: &[Value]) -> String {
    decorate_impl(html, notes, true)
}

/// A transient geometry anchor, including notes with no visible decoration.
/// Wrap one source character inside its existing markup so emphasis, colors,
/// entities and the remaining annotation links stay intact.
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
