//! PDF render recovery.
use super::*;

#[cfg(test)]
pub(super) fn render(glyphs: &[Glyph], headings: &[crate::pdf::OutlineEntry]) -> Option<String> {
    render_page(glyphs, headings, &[], &std::collections::HashSet::new())
}

pub(super) fn render_page(
    glyphs: &[Glyph],
    headings: &[crate::pdf::OutlineEntry],
    figures: &[(usize, String)],
    omitted: &std::collections::HashSet<usize>,
) -> Option<String> {
    render_region(glyphs, headings, figures, omitted, !omitted.is_empty())
}

pub(super) fn render_region(
    glyphs: &[Glyph],
    headings: &[crate::pdf::OutlineEntry],
    figures: &[(usize, String)],
    omitted: &std::collections::HashSet<usize>,
    force: bool,
) -> Option<String> {
    let lines: Vec<_> = glyphs.split(|g| g.ch == '\n').collect();
    let paragraphs = paragraph_breaks(&lines, omitted);
    let contents = toc::recover(&lines, omitted);
    let mut scripts = superscripts(glyphs);
    let mut offset = 0;
    for (i, line) in lines.iter().enumerate() {
        if omitted.contains(&i) {
            // Tables/display equations already render their scripts natively.
            scripts[offset..offset + line.len()].fill(false);
        }
        offset += line.len() + 1;
    }
    let mut offset = 0;
    let script_lines: Vec<_> = lines
        .iter()
        .map(|line| {
            let slice = &scripts[offset..offset + line.len()];
            offset += line.len() + 1;
            slice
        })
        .collect();
    if !force
        && figures.is_empty()
        && contents.iter().all(Option::is_none)
        && !paragraphs.iter().any(|boundary| *boundary)
        && !scripts.iter().any(|script| *script)
        && !(0..lines.len()).any(|i| reading_heading(&lines, i, headings).is_some())
        && !(0..lines.len()).any(|i| italic_label(&lines, i).is_some())
        && !glyphs.iter().any(|g| g.link.is_some())
        && !glyphs.iter().any(|g| g.mono || g.bold || g.italic)
        && !glyphs
            .iter()
            .any(|g| g.image_before.is_some() || g.image_after.is_some())
        && !lines.iter().any(|line| is_code(line))
        && !lines
            .iter()
            .any(|line| crate::model::pdf_ordered_item(line_text(line).trim()))
        && !lines
            .iter()
            .any(|line| crate::model::pdf_unordered_item(line_text(line).trim()).is_some())
        && !lines.iter().any(|line| caption_start(line))
        && !(0..lines.len()).any(|i| leading_caption(&lines, i))
    {
        return None;
    }
    let mut html = String::new();
    let mut typography = vec![false; glyphs.len()];
    let mut link_eligible = vec![true; glyphs.len()];
    let mut line_offsets = vec![];
    let mut cursor = 0;
    for line in &lines {
        line_offsets.push(cursor);
        cursor += line.len() + 1;
    }
    let mut prose = String::new();
    let flush_prose = |html: &mut String, prose: &mut String| {
        if !prose.trim().is_empty() {
            let reflowed = crate::model::pdf_prose(prose);
            // A newly recovered PDF paragraph may begin with a literal
            // numbered item. Markdown lists hide that marker from selections
            // and can renumber it; keep the source's number in the text flow.
            let markdown = reflowed
                .lines()
                .map(|line| {
                    // PDF prose is literal text. Its font styles come from
                    // glyphs; Markdown punctuation must not consume source
                    // identifiers such as __init__ or literal links.
                    let literal: String = line
                        .chars()
                        .map(|c| {
                            if matches!(
                                c,
                                '&' | '<' | '>' | '\\' | '`' | '*' | '_' | '~' | '[' | ']'
                            ) {
                                format!("&#{};", c as u32)
                            } else {
                                c.to_string()
                            }
                        })
                        .collect();
                    if crate::model::pdf_ordered_item(line) {
                        format!("&#{};{}", literal.as_bytes()[0], &literal[1..])
                    } else {
                        literal
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            html.push_str(&crate::annotations::html(&markdown));
        }
        prose.clear();
    };
    let mut i = 0;
    while i < lines.len() {
        for (_, image) in figures.iter().filter(|(before, _)| *before == i) {
            flush_prose(&mut html, &mut prose);
            html.push_str(image);
        }
        if omitted.contains(&i) {
            i += 1;
            continue;
        }
        if let Some(entry) = &contents[i] {
            flush_prose(&mut html, &mut prose);
            html.push_str(entry);
            // These compact rows already carry their original PDF links.
            // Do not wrap them a second time using unshortened source quotes.
            link_eligible[line_offsets[i]..line_offsets[i] + lines[i].len()].fill(false);
            i += 1;
            continue;
        }
        let text = line_text(lines[i]);
        let continuation = leading_caption(&lines, i);
        if caption_start(lines[i]) || continuation {
            flush_prose(&mut html, &mut prose);
            let first = lines[i].iter().find(|g| !g.ch.is_whitespace()).unwrap();
            let mut caption = text.trim().to_owned();
            for j in 0..lines[i].len() {
                typography[line_offsets[i] + j] = lines[i][j].mono || lines[i][j].bold;
            }
            i += 1;
            while i < lines.len()
                && !omitted.contains(&i)
                && !figures.iter().any(|(before, _)| *before == i)
                && reading_heading(&lines, i, headings).is_none()
                && caption_continuation(first, lines[i - 1], lines[i])
            {
                for j in 0..lines[i].len() {
                    typography[line_offsets[i] + j] = lines[i][j].mono || lines[i][j].bold;
                }
                caption.push(' ');
                caption.push_str(line_text(lines[i]).trim());
                i += 1;
            }
            let alignment = if continuation || caption.starts_with("Figure ") {
                " style=\"text-align:center\""
            } else {
                ""
            };
            let tag = if first.italic { "em" } else { "span" };
            html.push_str(&format!(
                "<p{alignment}><{tag}>{}</{tag}></p>",
                escape(&caption)
            ));
            continue;
        }
        if let Some((count, level, title)) = reading_heading(&lines, i, headings) {
            flush_prose(&mut html, &mut prose);
            html.push_str(&format!("<h{level}>{}</h{level}>\n", escape(&title)));
            i += count;
            continue;
        }
        if let Some((count, title)) = italic_label(&lines, i)
            && (i..i + count).all(|line| {
                !omitted.contains(&line)
                    && (line == i || !figures.iter().any(|(before, _)| *before == line))
            })
        {
            flush_prose(&mut html, &mut prose);
            html.push_str(&format!("<p><em>{}</em></p>\n", escape(&title)));
            i += count;
            continue;
        }
        if !is_code(lines[i]) {
            typography[line_offsets[i]..line_offsets[i] + lines[i].len()].fill(true);
            if paragraphs[i] {
                flush_prose(&mut html, &mut prose);
            }
            if raised_item_start(lines[i], script_lines[i]) {
                // A marker split from the preceding body by PDFium is inline
                // when it remains on that body's physical baseline.
                let same_physical_line = i.checked_sub(1).is_some_and(|previous| {
                    let body = lines[i].iter().rev().find(|g| !g.ch.is_whitespace());
                    let last = lines[previous].iter().rev().find(|g| !g.ch.is_whitespace());
                    body.zip(last)
                        .is_some_and(|(body, last)| (body.y - last.y).abs() <= body.size * 0.1)
                });
                if !same_physical_line {
                    flush_prose(&mut html, &mut prose);
                }
            }
            prose.extend(lines[i].iter().filter(|g| g.ch != '\r').map(|g| g.ch));
            // PDFium sometimes inserts a line break immediately before a
            // raised footnote marker even though it shares the preceding baseline.
            let continued_script = lines
                .get(i + 1)
                .and_then(|line| line.iter().find(|g| !g.ch.is_whitespace()))
                .is_some_and(|g| {
                    let next_start: usize = lines[..=i].iter().map(|line| line.len() + 1).sum();
                    let index = next_start
                        + lines[i + 1]
                            .iter()
                            .position(|g| !g.ch.is_whitespace())
                            .unwrap();
                    scripts[index]
                        && lines[i]
                            .iter()
                            .rev()
                            .find(|g| !g.ch.is_whitespace())
                            .is_some_and(|previous| {
                                (g.y - previous.y).abs() <= previous.size * 0.65
                                    && g.x >= previous.x
                            })
                });
            prose.push(if continued_script { ' ' } else { '\n' });
            i += 1;
            continue;
        }
        flush_prose(&mut html, &mut prose);
        let start = i;
        while i < lines.len()
            && (i == start || !figures.iter().any(|(before, _)| *before == i))
            && !omitted.contains(&i)
            && (is_code(lines[i]) || lines[i].iter().all(|g| g.ch.is_whitespace()))
        {
            i += 1;
        }
        let block = &lines[start..i];
        let left = block
            .iter()
            .flat_map(|line| line.iter())
            .filter(|g| !g.ch.is_whitespace())
            .map(|g| g.x)
            .fold(f32::INFINITY, f32::min);
        let mut steps: Vec<_> = block
            .iter()
            .flat_map(|line| line.windows(2))
            .filter(|pair| !pair[0].ch.is_control() && !pair[1].ch.is_control())
            .map(|pair| pair[1].x - pair[0].x)
            .filter(|step| *step > 0.5)
            .collect();
        steps.sort_by(f32::total_cmp);
        let step = steps
            .get(steps.len() / 2)
            .copied()
            .unwrap_or_else(|| {
                block
                    .iter()
                    .flat_map(|line| line.iter())
                    .find(|g| !g.ch.is_whitespace())
                    .map(|g| g.size * 0.6)
                    .unwrap_or(6.)
            })
            .max(1.);
        let baselines: Vec<_> = block
            .iter()
            .filter_map(|line| line.iter().find(|g| !g.ch.is_whitespace()).map(|g| g.y))
            .collect();
        let spacing = baselines
            .windows(2)
            .map(|pair| (pair[0] - pair[1]).abs())
            .filter(|gap| *gap > 1.)
            .fold(f32::INFINITY, f32::min);
        html.push_str("<pre><code>");
        let mut previous_y: Option<f32> = None;
        for line in block {
            let Some(first) = line.iter().position(|g| !g.ch.is_whitespace()) else {
                html.push('\n');
                previous_y = None;
                continue;
            };
            if let Some(y) = previous_y {
                let gap = (y - line[first].y).abs();
                if spacing.is_finite() && gap > spacing * 1.5 {
                    html.push_str(
                        &"\n".repeat(((gap / spacing).round() as usize).saturating_sub(1).min(3)),
                    );
                }
            }
            previous_y = Some(line[first].y);
            let indent = ((line[first].x - left) / step).round().clamp(0., 120.) as usize;
            html.push_str(&" ".repeat(indent));
            let mut run = String::new();
            let mut style = (line[first].color, line[first].italic, line[first].bold);
            let flush =
                |html: &mut String, run: &mut String, (color, italic, bold): (u32, bool, bool)| {
                    if !run.is_empty() {
                        html.push_str(&format!(
                            "<span style=\"color:#{color:06x}{}{}\">{}</span>",
                            if italic { ";font-style:italic" } else { "" },
                            if bold { ";font-weight:bold" } else { "" },
                            escape(run)
                        ));
                        run.clear();
                    }
                };
            for glyph in &line[first..] {
                if glyph.ch.is_control() {
                    continue;
                }
                if let Some(image) = &glyph.image_before {
                    flush(&mut html, &mut run, style);
                    html.push_str(image);
                }
                let next = (glyph.color, glyph.italic, glyph.bold);
                if next != style {
                    flush(&mut html, &mut run, style);
                    style = next;
                }
                run.push(glyph.ch);
                if let Some(image) = &glyph.image_after {
                    flush(&mut html, &mut run, style);
                    html.push_str(image);
                }
            }
            flush(&mut html, &mut run, style);
            html.push('\n');
        }
        html.push_str("</code></pre>");
    }
    flush_prose(&mut html, &mut prose);
    for (_, image) in figures.iter().filter(|(before, _)| *before == lines.len()) {
        html.push_str(image);
    }
    Some(apply_superscripts(
        glyphs,
        &scripts,
        apply_links(
            glyphs,
            &link_eligible,
            apply_prose_images(glyphs, apply_typography(glyphs, &typography, html)),
        ),
    ))
}
