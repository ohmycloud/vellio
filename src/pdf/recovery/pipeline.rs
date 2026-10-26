//! PDF pipeline recovery.
use super::*;

pub(super) type ReadingHtml = (
    String,
    Option<String>,
    Vec<usize>,
    Vec<crate::pdf::FigurePreview>,
    Vec<crate::pdf::MathPresentation>,
);

pub(crate) fn reading_html(
    page: &PdfPage<'_>,
    doc: &PdfDocument<'_>,
    headings: &[crate::pdf::OutlineEntry],
    page_number: usize,
) -> anyhow::Result<ReadingHtml> {
    let text = page.text()?;
    // Extraction compatibility is deliberately unchanged for existing books.
    // Repaired mathematical scalars belong only to the disposable presentation.
    let source = text
        .chars()
        .iter()
        .filter_map(|g| g.unicode_char())
        .collect::<String>()
        .replace("\r\n", "\n");
    let mut glyphs = glyphs(&text);
    let hidden = crate::pdf::hidden_text_indices(page, &text);
    if !hidden.is_empty() {
        // Use the same decoded scalars and leading-record indices as glyphs(),
        // while leaving PDFium's generated line breaks intact.
        let indices = decoded_characters(&text)
            .into_iter()
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        glyphs = glyphs
            .into_iter()
            .zip(indices)
            .filter_map(|(g, index)| (!hidden.contains(&index)).then_some(g))
            .collect();
    }
    let (recovered, fragmented) = recover_fragmented_lines(&glyphs);
    glyphs = recovered;
    attach_links(page, &mut glyphs);
    let images = candidates(page, &glyphs);
    let mut indices: Vec<_> = crate::pdf::page_background_indices(page)
        .into_iter()
        .collect();
    if !images.is_empty() {
        for (index, anchor, after, bounds) in images {
            let object = page.objects().get(index)?;
            let Some(raster) = object.as_image_object() else {
                continue;
            };
            // Render the image object alone, including its soft mask. A page
            // crop would bake the white paper into the code background.
            let width = 64;
            let height = (width as f32 * bounds.height().value / bounds.width().value)
                .round()
                .max(1.) as i32;
            let image = raster.get_processed_image_with_size(doc, width, height)?;
            let mut png = std::io::Cursor::new(Vec::new());
            image.write_to(&mut png, image::ImageFormat::Png)?;
            let data = base64::engine::general_purpose::STANDARD.encode(png.into_inner());
            // The native inline flow sizes images against the current line,
            // so code font changes also scale the embedded glyph.
            let slot = if after {
                &mut glyphs[anchor].image_after
            } else {
                &mut glyphs[anchor].image_before
            };
            slot.get_or_insert_with(String::new).push_str(&format!(
                "<img src=\"data:image/png;base64,{data}\" alt=\"\">"
            ));
            indices.push(index);
        }
    }
    let (config, figures) = crate::pdf::reading_figures(page, doc, page_number)?;
    let lines: Vec<_> = glyphs.split(|g| g.ch == '\n').collect();
    let mut insertions = Vec::new();
    let mut omitted = page_number_lines(
        &glyphs,
        page.width().value,
        page.height().value,
        page_number,
    );
    let mut table_lines = std::collections::HashSet::new();
    let mut math_blocked = std::collections::HashSet::new();
    let regions = table::regions(page, &glyphs);
    // Prefer complete semantic tables, never a misleading partial grid.
    let tables: Vec<_> = table::recover(page, &glyphs)
        .into_iter()
        .filter(|t| {
            !regions
                .iter()
                .any(|r| table::contains(*r, t.bounds) && !table::contains(t.bounds, *r))
        })
        .collect();
    for table in &tables {
        table_lines.extend(table.lines.iter().copied());
        omitted.extend(table.lines.iter().copied());
        // Older imports may have mistaken shaded cells / rules for a figure.
        // Hide those existing figure blocks when the table is rendered as text.
        indices.extend(page.objects().iter().enumerate().filter_map(|(i, object)| {
            if object.as_text_object().is_some() {
                return None;
            }
            let b = object.bounds().ok()?;
            table::contains(
                table.bounds,
                [
                    b.left().value,
                    b.bottom().value,
                    b.right().value,
                    b.top().value,
                ],
            )
            .then_some(i)
        }));
    }
    for &bounds in &regions {
        if !tables.iter().any(|t| table::contains(t.bounds, bounds)) {
            if let Some(fallback) = table::text_fallback(&glyphs, bounds) {
                omitted.extend(&fallback.lines);
                insertions.push((fallback.lines[0], fallback.html));
            } else if let Some(before) = lines.iter().position(|line| {
                line.iter()
                    .any(|g| table::contains(bounds, [g.x, g.y, g.right, g.y]))
            }) {
                insertions.push((before,"<p data-vellio-table-unrecovered=\"true\">Table structure not recovered; mixed extraction text is retained below.</p>".to_owned()));
            }
            math_blocked.extend(
                glyphs
                    .iter()
                    .enumerate()
                    .filter(|(_, g)| table::contains(bounds, [g.x, g.y, g.right, g.y]))
                    .map(|(i, _)| i),
            );
        }
        // Suppress legacy table screenshots/rules independently of recovery.
        indices.extend(page.objects().iter().enumerate().filter_map(|(i, object)| {
            if object.as_text_object().is_some() {
                return None;
            }
            let b = object.bounds().ok()?;
            table::contains(
                bounds,
                [
                    b.left().value,
                    b.bottom().value,
                    b.right().value,
                    b.top().value,
                ],
            )
            .then_some(i)
        }));
    }
    let mut previews = Vec::new();
    for figure in figures {
        let figure_config = &config;
        let b = figure.bounds;
        // Mixed extraction lines can contain both diagram scripts and prose.
        // Exclude figure-contained glyphs independently of whole-line omission.
        for (i, g) in glyphs.iter().enumerate() {
            if let Ok((x, y)) = page.points_to_pixels(
                PdfPoints::new(g.x),
                PdfPoints::new(g.y + g.size * 0.3),
                figure_config,
            ) && x >= b[0] as i32 - 4
                && x <= b[2] as i32 + 4
                && y >= b[1] as i32 - 4
                && y <= b[3] as i32 + 4
            {
                math_blocked.insert(i);
            }
        }
        let mut before = lines.len();
        for (i, line) in lines.iter().enumerate() {
            let positions: Vec<_> = line
                .iter()
                .filter(|g| !g.ch.is_whitespace())
                .filter_map(|g| {
                    page.points_to_pixels(
                        PdfPoints::new(g.x),
                        PdfPoints::new(g.y + g.size * 0.3),
                        figure_config,
                    )
                    .ok()
                })
                .collect();
            if positions.is_empty() {
                continue;
            }
            // Character origins can lie just outside their tight painted
            // bounds (e.g. the S in an external panel title). Allow the same
            // small edge tolerance instead of duplicating crop text as prose.
            if positions.iter().all(|&(x, y)| {
                x >= b[0] as i32 - 4
                    && x <= b[2] as i32 + 4
                    && y >= b[1] as i32 - 4
                    && y <= b[3] as i32 + 4
            }) {
                omitted.insert(i);
            }
            if before == lines.len() && positions.iter().any(|&(_, y)| y >= b[1] as i32) {
                before = i;
            }
        }
        insertions.push((before, figure.html));
        indices.extend(figure.indices);
        previews.push(figure.preview);
    }
    // Table glyphs are omitted from prose, not from formula recovery. Keep
    // their expressions inline in the cells rather than inserting display
    // paragraphs that would duplicate or replace an entire table row.
    let math_omitted = omitted.difference(&table_lines).copied().collect();
    let recovered_math = math::recover(page, &glyphs, &math_omitted, &math_blocked, &table_lines)?;
    for mut table in tables {
        table.restore_math(&glyphs, &recovered_math);
        insertions.push((table.lines[0], table.html));
    }
    for expression in recovered_math.iter().filter(|m| m.presentation.display) {
        let object = crate::formula::native_html(&expression.presentation.tex, true);
        let number = expression
            .presentation
            .number
            .as_ref()
            .map(|n| format!(" &#8195;({n})"))
            .unwrap_or_default();
        insertions.push((
            expression.lines[0],
            format!(
                "<p style=\"text-align:center\" data-vellio-equation=\"true\">{object}{number}</p>"
            ),
        ));
        omitted.extend(expression.lines.iter().copied());
    }
    for (lines, html) in footnote::recover(page, &glyphs, &omitted) {
        // Do not consume a recovered table, formula or figure insertion.
        if !insertions.iter().any(|(i, _)| lines.contains(i)) {
            insertions.push((lines[0], html));
            omitted.extend(lines);
        }
    }
    for list in list::recover(page, &glyphs, &omitted) {
        insertions.push((list.lines[0], list.html));
        omitted.extend(list.lines);
        indices.extend(list.objects);
    }
    for definition in definition::recover(&glyphs, headings, &omitted) {
        insertions.push((definition.lines[0], definition.html));
        omitted.extend(definition.lines);
    }
    // Keep the source callout distinct using native quote paragraphs. The box
    // can continue onto another page without repeating a label. Existing code,
    // lists, tables and interior figures use the same renderer within the box.
    for bounds in crate::pdf::text_panel_bounds(page) {
        let contained: Vec<_> = lines
            .iter()
            .enumerate()
            .filter_map(|(i, line)| {
                let visible: Vec<_> = line
                    .iter()
                    .filter(|g| !g.ch.is_whitespace() && !g.ch.is_control())
                    .collect();
                (!visible.is_empty()
                    && visible.iter().all(|g| {
                        g.x >= bounds[0] - 1.
                            && g.x <= bounds[2] + 1.
                            && g.y >= bounds[1] - 1.
                            && g.y <= bounds[3] + 1.
                    }))
                .then_some(i)
            })
            .collect();
        let (Some(&start), Some(&end)) = (contained.first(), contained.last()) else {
            continue;
        };
        if contained.iter().all(|i| omitted.contains(i)) {
            continue;
        }
        let start_offset: usize = lines[..start].iter().map(|l| l.len() + 1).sum();
        let end_offset = start_offset
            + lines[start..=end]
                .iter()
                .map(|l| l.len() + 1)
                .sum::<usize>();
        let mut local = glyphs[start_offset..end_offset.min(glyphs.len())].to_vec();
        for g in &mut local {
            g.link = None;
        }
        let mut local_insertions: Vec<_> = insertions
            .iter()
            .filter(|(i, _)| *i >= start && *i <= end)
            .map(|(i, h)| (i - start, h.clone()))
            .collect();
        let mut local_omitted: std::collections::HashSet<_> = omitted
            .iter()
            .filter(|i| **i >= start && **i <= end)
            .map(|i| i - start)
            .collect();
        let title_glyphs: Vec<_> = lines[start]
            .iter()
            .filter(|g| !g.ch.is_whitespace() && !g.ch.is_control())
            .collect();
        if let (Some(first), Some(last)) = (title_glyphs.first(), title_glyphs.last())
            && (3..=40).contains(&title_glyphs.len())
            && first.size >= 10.
            && bounds[3] - first.y <= first.size * 3.5
            && ((first.x + last.right - bounds[0] - bounds[2]) * 0.5).abs()
                <= (bounds[2] - bounds[0]) * 0.12
            && title_glyphs.iter().all(|g| !g.mono)
            && (title_glyphs.iter().all(|g| g.bold)
                || title_glyphs
                    .iter()
                    .all(|g| !g.ch.is_alphabetic() || g.ch.is_uppercase()))
        {
            let title = escape(line_text(lines[start]).trim());
            local_insertions.retain(|(i, _)| *i != 0);
            local_insertions.push((
                0,
                format!("<p style=\"text-align:center\"><strong>{title}</strong></p>"),
            ));
            local_omitted.insert(0);
        }
        let inner = render_region(&local, &[], &local_insertions, &local_omitted, true)
            .expect("A recognized text panel always renders its source text");
        insertions.retain(|(i, _)| *i < start || *i > end);
        insertions.push((start, format!("<blockquote>{inner}</blockquote>")));
        omitted.extend(start..=end);
    }
    let html = render_page(&glyphs, headings, &insertions, &omitted)
        .or_else(|| {
            (fragmented || !recovered_math.is_empty())
                .then(|| crate::annotations::html(&crate::model::pdf_prose(&line_text(&glyphs))))
        })
        .map(|mut html| {
            for highlight in text_highlights(page) {
                html = crate::annotations::source_highlight(
                    &html,
                    &highlight.quote,
                    &highlight.prefix,
                    &highlight.suffix,
                    highlight.color,
                );
            }
            html
        });
    let html = html.map(|html| {
        let edits: Vec<_> = recovered_math
            .iter()
            .filter(|m| !m.presentation.display && !m.lines.iter().any(|i| table_lines.contains(i)))
            .map(|expression| {
                let r = &expression.range;
                let quote: String = glyphs[r.clone()].iter().map(|g| g.ch).collect();
                let prefix: String = glyphs[r.start.saturating_sub(40)..r.start]
                    .iter()
                    .map(|g| g.ch)
                    .collect();
                let suffix: String = glyphs[r.end..(r.end + 40).min(glyphs.len())]
                    .iter()
                    .map(|g| g.ch)
                    .collect();
                (
                    quote,
                    prefix,
                    suffix,
                    crate::formula::native_html(&expression.presentation.tex, false),
                )
            })
            .collect();
        crate::annotations::source_math_edits(&html, &edits)
    });
    if let Some(html) = &html {
        crate::typst_table::warm_html(html)?;
    }
    Ok((
        source,
        html,
        indices,
        previews,
        recovered_math.into_iter().map(|m| m.presentation).collect(),
    ))
}
