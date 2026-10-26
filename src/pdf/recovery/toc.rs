//! Display-only recovery of printed PDF contents rows. Dot leaders are padding,
//! not prose: keep each title/page pair separate and compact when reflowing.
use super::{Glyph, escape, line_text};
use std::collections::HashSet;

fn leader(ch: char) -> bool {
    matches!(ch, '.' | '·' | '․' | '…' | '‥')
}

fn entry(line: &[Glyph]) -> Option<(usize, usize)> {
    if super::is_code(line) {
        return None;
    }
    let end = line
        .iter()
        .rposition(|g| !g.ch.is_whitespace() && !g.ch.is_control())?
        + 1;
    let page_start = line[..end]
        .iter()
        .rposition(|g| g.ch.is_whitespace() || leader(g.ch))?
        + 1;
    let page = &line[page_start..end];
    if page.is_empty()
        || page.len() > 10
        || !(page.iter().all(|g| g.ch.is_ascii_digit())
            || page.iter().all(|g| {
                matches!(
                    g.ch,
                    'i' | 'v'
                        | 'x'
                        | 'l'
                        | 'c'
                        | 'd'
                        | 'm'
                        | 'I'
                        | 'V'
                        | 'X'
                        | 'L'
                        | 'C'
                        | 'D'
                        | 'M'
                )
            }))
    {
        return None;
    }
    let mut title_end = page_start;
    let mut dots = 0;
    while title_end > 0 {
        let ch = line[title_end - 1].ch;
        if leader(ch) {
            dots += match ch {
                '…' => 3,
                '‥' => 2,
                _ => 1,
            };
        } else if !ch.is_whitespace() {
            break;
        }
        title_end -= 1;
    }
    (dots >= 4 && line[..title_end].iter().any(|g| g.ch.is_alphanumeric()))
        .then_some((title_end, page_start))
}

fn contents_heading(line: &[Glyph]) -> bool {
    let text: String = line_text(line)
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect();
    matches!(
        text.to_lowercase().as_str(),
        "目录" | "目錄" | "contents" | "tableofcontents"
    )
}

/// Preserve source links and emphasis without matching the removed dot padding.
fn inline(glyphs: &[Glyph]) -> String {
    let mut html = String::new();
    let mut start = 0;
    while start < glyphs.len() {
        let first = &glyphs[start];
        let mut end = start + 1;
        while end < glyphs.len()
            && glyphs[end].link == first.link
            && glyphs[end].bold == first.bold
            && glyphs[end].italic == first.italic
        {
            end += 1;
        }
        let mut text = escape(&line_text(&glyphs[start..end]));
        if first.bold {
            text = format!("<strong>{text}</strong>");
        }
        if first.italic {
            text = format!("<em>{text}</em>");
        }
        if let Some(url) = &first.link {
            let url = escape(url).replace('"', "&quot;");
            let color = crate::annotations::LINK_COLOR;
            text =
                format!("<a href=\"{url}\"><span style=\"color:#{color:06x}\">{text}</span></a>");
        }
        html.push_str(&text);
        start = end;
    }
    html
}

pub(super) fn recover(lines: &[&[Glyph]], omitted: &HashSet<usize>) -> Vec<Option<String>> {
    let entries: Vec<_> = lines
        .iter()
        .enumerate()
        .map(|(i, line)| (!omitted.contains(&i)).then(|| entry(line)).flatten())
        .collect();
    let heading = lines
        .iter()
        .enumerate()
        .position(|(i, line)| !omitted.contains(&i) && contents_heading(line));
    let count = entries.iter().flatten().count();
    let mut recovered = vec![None; lines.len()];
    // A lone ellipsis followed by a number in ordinary prose is not a TOC.
    // Continuation pages need no repeated Contents heading.
    if count < 2 && !(count == 1 && heading.is_some()) {
        return recovered;
    }
    for (i, entry) in entries.iter().enumerate() {
        if let Some((title_end, page_start)) = entry {
            recovered[i] = Some(format!(
                "<p data-vellio-toc=\"true\">{} … {}</p>",
                inline(&lines[i][..*title_end]),
                inline(&lines[i][*page_start..])
            ));
        }
    }
    if let Some(heading) = heading {
        recovered[heading] = Some(format!("<h2>{}</h2>", inline(lines[heading])));
        // A short title-page preamble is separate metadata, not a paragraph
        // continuing into the first contents entry.
        if heading <= 8 {
            for i in 0..heading {
                if !omitted.contains(&i)
                    && recovered[i].is_none()
                    && !line_text(lines[i]).trim().is_empty()
                {
                    recovered[i] = Some(format!("<p>{}</p>", inline(lines[i])));
                }
            }
        }
    }
    recovered
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::recovery::{render, tests::test_lines};

    #[test]
    fn printed_contents_rows_keep_titles_pages_links_and_separate_preamble() {
        let mut glyphs = test_lines(&[
            ("新生 300 课图解教程", 12., false, false),
            ("股票交易新手入门", 12., false, false),
            ("日期：2026-04-17", 12., false, false),
            ("目录", 12., false, false),
            ("第 1 节：价托 . . . . . . . . . . 9", 12., false, false),
            ("第 2 节：量托 . . . . . . . . . . 12", 12., false, false),
        ]);
        let mut row = 0;
        for glyph in &mut glyphs {
            if glyph.ch == '\n' {
                row += 1;
            } else if row >= 4 {
                glyph.link = Some(format!("easyread-pdf:{}:0", row + 5));
            }
        }
        let before: String = glyphs.iter().map(|g| g.ch).collect();
        let html = render(&glyphs, &[]).unwrap();
        assert!(html.contains("<h2>目录</h2>"), "{html}");
        let xml = format!("<root>{html}</root>");
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let paragraphs: Vec<_> = doc
            .descendants()
            .filter(|n| n.has_tag_name("p"))
            .map(|n| {
                n.descendants()
                    .filter(|n| n.is_text())
                    .map(|n| n.text().unwrap())
                    .collect::<String>()
            })
            .collect();
        assert_eq!(
            paragraphs,
            [
                "新生 300 课图解教程",
                "股票交易新手入门",
                "日期：2026-04-17",
                "第 1 节：价托 … 9",
                "第 2 节：量托 … 12"
            ]
        );
        assert!(html.contains("href=\"easyread-pdf:9:0\""));
        assert!(html.contains("href=\"easyread-pdf:10:0\""));
        assert_eq!(before, glyphs.iter().map(|g| g.ch).collect::<String>());
        assert!(
            !doc.descendants()
                .any(|n| n.has_tag_name("a") && n.ancestors().skip(1).any(|a| a.has_tag_name("a")))
        );
    }

    #[test]
    fn continuation_pages_and_roman_numerals_need_no_contents_heading() {
        let glyphs = test_lines(&[
            ("Preface ………… ix", 12., false, false),
            ("Chapter 1 ........... 10", 12., false, false),
            ("Chapter 2 · · · · · 23", 12., false, false),
        ]);
        let html = render(&glyphs, &[]).unwrap();
        assert_eq!(html.matches("data-vellio-toc").count(), 3, "{html}");
        assert!(html.contains("Preface … ix"), "{html}");
    }

    #[test]
    #[ignore = "requires EASYREAD_QA_TOC_BOOK pointing to an imported PDF book"]
    fn printed_contents_real_pdf_keeps_each_entry_and_target() {
        let root = std::path::PathBuf::from(std::env::var("EASYREAD_QA_TOC_BOOK").unwrap());
        let bytes = std::fs::read(root.join("paper.json")).unwrap();
        let paper: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let requests: Vec<_> = paper["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|b| b["source_kind"] == "pdf-text")
            .take(2)
            .map(|b| {
                (
                    b["page"].as_u64().unwrap() as usize,
                    b["en"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        let presentations =
            crate::pdf::code_presentations(&root.join("source.pdf"), &requests).unwrap();
        for ((page, source), presentation) in requests.iter().zip(presentations) {
            let html = presentation.unwrap().unwrap().html;
            let expected = source
                .lines()
                .filter(|line| {
                    let gs = test_lines(&[(line, 12., false, false)]);
                    entry(&gs).is_some()
                })
                .count();
            assert!(
                expected > 10,
                "Page {page} must contain a real printed contents list"
            );
            assert_eq!(
                html.matches("data-vellio-toc").count(),
                expected,
                "page {page}"
            );
            assert!(
                !html.contains(". . . . . . ."),
                "Leaders must reflow compactly"
            );
            assert!(
                html.matches("easyread-pdf:").count() >= expected,
                "Contents targets must survive"
            );
        }
        assert_eq!(std::fs::read(root.join("paper.json")).unwrap(), bytes);
    }

    #[test]
    fn ordinary_prose_ellipsis_versions_and_omitted_rows_are_not_contents() {
        let glyphs = test_lines(&[
            ("An unfinished thought ... 9", 12., false, false),
            ("The release version is 1.2.3.4", 12., false, false),
            ("Continue reading the prose", 12., false, false),
        ]);
        let lines: Vec<_> = glyphs.split(|g| g.ch == '\n').collect();
        assert!(recover(&lines, &HashSet::new()).iter().all(Option::is_none));
        let glyphs = test_lines(&[
            ("One ........ 1", 12., false, false),
            ("Two ........ 2", 12., false, false),
        ]);
        let lines: Vec<_> = glyphs.split(|g| g.ch == '\n').collect();
        assert!(
            recover(&lines, &HashSet::from([1]))
                .iter()
                .all(Option::is_none)
        );
        let mut code = glyphs.clone();
        for glyph in &mut code {
            glyph.mono = true;
        }
        let lines: Vec<_> = code.split(|g| g.ch == '\n').collect();
        assert!(recover(&lines, &HashSet::new()).iter().all(Option::is_none));
    }
}
