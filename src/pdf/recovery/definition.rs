//! Recover paired terms and hanging descriptions from source typography and
//! coordinates. Equal-size identifiers are prose labels, not tiny code blocks.
use super::{Glyph, line_text};
use std::collections::HashSet;

pub(super) struct ReadingDefinition {
    pub lines: Vec<usize>,
    pub html: String,
}

struct Item {
    start: usize,
    end: usize,
    left: f32,
    size: f32,
    indent: f32,
    mono: bool,
}

fn item(
    lines: &[&[Glyph]],
    start: usize,
    headings: &[crate::pdf::OutlineEntry],
    omitted: &HashSet<usize>,
) -> Option<Item> {
    // A preceding definition may consume the last line of the page.
    let term = lines.get(start)?;
    if omitted.contains(&start) || super::bookmarked_heading(lines, start, headings).is_some() {
        return None;
    }
    let visible: Vec<_> = term
        .iter()
        .filter(|g| !g.ch.is_whitespace() && !g.ch.is_control())
        .collect();
    let first = *visible.first()?;
    if first.size <= 0.
        || visible.iter().any(|g| {
            (g.size - first.size).abs() > first.size * 0.05
                || (g.y - first.y).abs() > first.size * 0.2
        })
    {
        return None;
    }
    let mono = visible.iter().all(|g| g.mono);
    if !mono && !visible.iter().all(|g| g.bold) {
        return None;
    }
    let text = line_text(term);
    let text = text.trim();
    if text.len() > 100
        || text.split_whitespace().count() > 6
        || !text.chars().any(char::is_alphabetic)
        || !text
            .chars()
            .all(|c| c.is_alphanumeric() || c.is_whitespace() || "_./-()<>|".contains(c))
    {
        return None;
    }
    let body = lines.get(start + 1)?;
    let next = body
        .iter()
        .find(|g| !g.ch.is_whitespace() && !g.ch.is_control())?;
    let indent = (next.x - first.x) / first.size;
    let gap = (first.y - next.y) / first.size;
    if omitted.contains(&(start + 1))
        || super::is_code(body)
        || super::caption_start(body)
        || !(0.75..=6.).contains(&indent)
        || !(0.5..=3.).contains(&gap)
        || (next.size - first.size).abs() > first.size * 0.1
    {
        return None;
    }
    let mut end = start + 2;
    let mut previous = next;
    while let Some(line) = lines.get(end) {
        let Some(next) = line
            .iter()
            .find(|g| !g.ch.is_whitespace() && !g.ch.is_control())
        else {
            break;
        };
        if omitted.contains(&end)
            || super::is_code(line)
            || super::caption_start(line)
            || (next.x - first.x) / first.size < 0.75
            || (next.x - body.iter().find(|g| !g.ch.is_whitespace()).unwrap().x).abs()
                > first.size * 0.5
            || (next.size - first.size).abs() > first.size * 0.1
            || !(0.5..=1.8).contains(&((previous.y - next.y) / first.size))
        {
            break;
        }
        previous = next;
        end += 1;
    }
    Some(Item {
        start,
        end,
        left: first.x,
        size: first.size,
        indent,
        mono,
    })
}

pub(super) fn recover(
    glyphs: &[Glyph],
    headings: &[crate::pdf::OutlineEntry],
    omitted: &HashSet<usize>,
) -> Vec<ReadingDefinition> {
    let lines: Vec<_> = glyphs.split(|g| g.ch == '\n').collect();
    let mut out = vec![];
    let mut i = 0;
    while i < lines.len() {
        let Some(first) = item(&lines, i, headings, omitted) else {
            i += 1;
            continue;
        };
        let left = first.left;
        let size = first.size;
        let mut items = vec![first];
        i = items[0].end;
        while let Some(next) = item(&lines, i, headings, omitted) {
            if (next.left - left).abs() > size * 0.3 || (next.size - size).abs() > size * 0.1 {
                break;
            }
            i = next.end;
            items.push(next);
        }
        // A single bold heading followed by a first-line paragraph indent is
        // ambiguous. Repeated term/body pairs are required for roman labels.
        if items.len() == 1 && !items[0].mono {
            continue;
        }
        let mut html = String::from("<dl>");
        let mut claimed = vec![];
        for item in items {
            let term: Vec<_> = lines[item.start].iter().collect();
            let body: Vec<_> = lines[item.start + 1..item.end]
                .iter()
                .flat_map(|l| l.iter())
                .collect();
            let term = super::table::cell_html(&term, false);
            html.push_str(&format!(
                "<dt><strong>{term}</strong></dt><dd data-easyread-indent=\"{:.3}\">{}</dd>",
                item.indent,
                super::table::cell_html(&body, false)
            ));
            claimed.extend(item.start..item.end);
        }
        html.push_str("</dl>");
        out.push(ReadingDefinition {
            lines: claimed,
            html,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn line(text: &str, x: f32, y: f32, size: f32, mono: bool, bold: bool) -> Vec<Glyph> {
        text.chars()
            .chain(['\n'])
            .enumerate()
            .map(|(i, ch)| Glyph {
                ch,
                x: x + i as f32 * 5.,
                y,
                size,
                mono,
                bold,
                italic: false,
                right: x + (i + 1) as f32 * 5.,
                advance_right: x + (i + 1) as f32 * 5.,
                color: 0,
                image_before: None,
                image_after: None,
                link: None,
            })
            .collect()
    }
    #[test]
    fn paired_terms_keep_hanging_wraps_and_stop_at_prose_and_code() {
        let mut gs = vec![];
        for (s, x, y, m) in [
            ("Introduction:", 72., 750., false),
            ("description", 72., 720., true),
            ("A description of all tools", 90., 698., false),
            ("and their inputs.", 90., 684., false),
            ("prompt", 72., 660., true),
            ("Instructions for the LLM", 90., 638., false),
            ("Following ordinary body.", 72., 610., false),
            ("x = 1", 72., 580., true),
        ] {
            gs.extend(line(s, x, y, 12., m, false));
        }
        let recovered = recover(&gs, &[], &HashSet::new());
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].lines, vec![1, 2, 3, 4, 5]);
        assert!(recovered[0].html.contains("<dt><strong><code>description"));
        assert!(recovered[0].html.contains("data-easyread-indent=\"1.500\""));
        assert!(
            recovered[0]
                .html
                .contains("A description of all tools and their inputs.")
        );
        assert!(!recovered[0].html.contains("Following"));
    }
    #[test]
    fn code_small_captions_single_headings_and_omitted_regions_stay_outside() {
        for (size, x, bold, term) in [
            (9., 90., false, "prompt"),
            (12., 72., false, "prompt"),
            (12., 90., true, "Heading"),
            (12., 90., false, "x = 1"),
        ] {
            let mut gs = line(term, 72., 720., size, !bold, bold);
            gs.extend(line("Some body text.", x, 698., 12., false, false));
            assert!(
                recover(&gs, &[], &HashSet::new()).is_empty(),
                "{term} {size} {x}"
            );
        }
        let mut gs = line("name", 72., 720., 12., true, false);
        gs.extend(line("Its explanation", 90., 698., 12., false, false));
        assert!(recover(&gs, &[], &HashSet::from([1])).is_empty());
    }

    #[test]
    fn definition_at_end_of_page_without_trailing_newline() {
        for terms in [vec!["description"], vec!["description", "prompt"]] {
            let mut gs = vec![];
            for (i, term) in terms.iter().enumerate() {
                let y = 720. - i as f32 * 60.;
                gs.extend(line(term, 72., y, 12., true, false));
                gs.extend(line("Its explanation", 90., y - 22., 12., false, false));
            }
            gs.pop(); // PDFium need not emit a newline at the end of a page.
            let recovered = recover(&gs, &[], &HashSet::new());
            assert_eq!(recovered.len(), 1);
            assert_eq!(recovered[0].lines, (0..terms.len() * 2).collect::<Vec<_>>());
            assert_eq!(recovered[0].html.matches("<dt>").count(), terms.len());
            assert_eq!(recovered[0].html.matches("<dd ").count(), terms.len());
        }
    }

    #[test]
    fn special_tokens_are_terms_but_single_roman_headings_are_not() {
        for term in ["<bos>", "<|turn>system", "<|think|>", "_execute_action"] {
            let mut gs = line(term, 72., 720., 12., true, false);
            gs.extend(line("Its explanation", 90., 698., 12., false, false));
            let recovered = recover(&gs, &[], &HashSet::new());
            assert_eq!(recovered.len(), 1, "{term}");
            assert!(recovered[0].html.contains("data-easyread-indent=\"1.500\""));
        }
        let mut gs = line("Constant width", 72., 720., 12., false, true);
        gs.extend(line("Its explanation", 90., 698., 12., false, false));
        gs.extend(line("Constant width bold", 72., 674., 12., false, true));
        gs.extend(line("Another explanation", 90., 652., 12., false, false));
        assert_eq!(
            recover(&gs, &[], &HashSet::new())[0]
                .html
                .matches("<dt>")
                .count(),
            2
        );
    }
}
