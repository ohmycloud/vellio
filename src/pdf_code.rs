//! Recover code and bookmarked headings from PDF fonts, colors and glyph positions.
//! This adapter never changes extracted text, IDs, edits or annotation records.
use base64::Engine;
use pdfium_render::prelude::*;

#[path = "pdf_definition.rs"]
mod definition;
#[path = "pdf_list.rs"]
mod list;
#[path = "pdf_math.rs"]
mod math;
#[path = "pdf_table.rs"]
mod table;

#[derive(Clone, Debug)]
struct Glyph {
    ch: char,
    mono: bool,
    x: f32,
    right: f32,
    advance_right: f32,
    y: f32,
    size: f32,
    color: u32,
    italic: bool,
    bold: bool,
    image_before: Option<String>,
    image_after: Option<String>,
    link: Option<String>,
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn glyphs(text: &PdfPageText<'_>) -> Vec<Glyph> {
    let mut glyphs = Vec::new();
    for ch in text.chars().iter() {
        let Some(c) = ch.unicode_char() else { continue };
        let font = ch.font_name().to_ascii_lowercase();
        let mono = ch.font_is_fixed_pitch()
            || [
                "mono",
                "courier",
                "consolas",
                "menlo",
                "typewriter",
                "sourcecode",
            ]
            .iter()
            .any(|name| font.contains(name));
        let (x, y) = ch
            .origin()
            .map(|(x, y)| (x.value, y.value))
            .unwrap_or_default();
        let color = ch
            .fill_color()
            .map(|c| (u32::from(c.red()) << 16) | (u32::from(c.green()) << 8) | u32::from(c.blue()))
            .unwrap_or(0x23211d);
        let weight = ch.font_weight();
        glyphs.push(Glyph {
            ch: c,
            mono,
            x,
            right: ch.tight_bounds().map(|b| b.right().value).unwrap_or(x),
            advance_right: ch.loose_bounds().map(|b| b.right().value).unwrap_or(x),
            y,
            size: ch.scaled_font_size().value,
            color,
            italic: ch.font_is_italic() || font.contains("italic") || font.contains("oblique"),
            bold: font.contains("bold") || font.contains("cmbx") || font.contains("cmmib") || font.contains("cmbsy") || font.contains("cmssbx") || ch.font_is_bold_reenforced() || matches!(weight, Some(PdfFontWeight::Weight600 | PdfFontWeight::Weight700Bold | PdfFontWeight::Weight800 | PdfFontWeight::Weight900)) || matches!(weight, Some(PdfFontWeight::Custom(weight)) if (600..=900).contains(&weight)),
            image_before: None,
            image_after: None,
            link: None,
        });
    }
    glyphs
}

/// Some transformed PDF fonts make PDFium emit a newline after each letter.
/// Recover only runs with repeated breaks on one physical baseline, using the
/// full glyph advance to distinguish word spaces from italic side bearings.
fn recover_fragmented_lines(glyphs: &[Glyph]) -> (Vec<Glyph>, bool) {
    let lines: Vec<_> = glyphs.split_inclusive(|g| g.ch == '\n').collect();
    let same_baseline = |a: &[Glyph], b: &[Glyph]| {
        let Some(prev) = a
            .iter()
            .rev()
            .find(|g| !g.ch.is_whitespace() && !g.ch.is_control())
        else {
            return false;
        };
        let Some(next) = b
            .iter()
            .find(|g| !g.ch.is_whitespace() && !g.ch.is_control())
        else {
            return false;
        };
        let size = prev.size;
        size > 0.
            && !prev.mono
            && !next.mono
            && (prev.size - next.size).abs() <= size * 0.05
            && (prev.y - next.y).abs() <= size * 0.05
            && next.x > prev.x
            && next.x - prev.advance_right >= -size * 0.3
            && next.x - prev.advance_right <= size * 1.2
            && a.iter()
                .chain(b)
                .filter(|g| !g.ch.is_whitespace() && !g.ch.is_control())
                .all(|g| (g.y - prev.y).abs() <= size * 0.05)
    };
    let mut out = Vec::with_capacity(glyphs.len());
    let mut changed = false;
    let mut i = 0;
    while i < lines.len() {
        let start = i;
        i += 1;
        while i < lines.len() && same_baseline(lines[i - 1], lines[i]) {
            i += 1;
        }
        if i - start < 4 {
            out.extend(lines[start..i].iter().flat_map(|line| line.iter()).cloned());
            continue;
        }
        changed = true;
        let mut physical: Vec<Glyph> = Vec::new();
        for g in lines[start..i]
            .iter()
            .flat_map(|line| line.iter())
            .filter(|g| g.ch != '\n' && g.ch != '\r')
        {
            if !g.ch.is_whitespace()
                && let Some(prev) = physical.last().filter(|g| !g.ch.is_whitespace())
                && g.x - prev.advance_right > g.size * 0.12
            {
                let mut space = prev.clone();
                space.ch = ' ';
                space.x = prev.advance_right;
                space.right = g.x;
                space.advance_right = g.x;
                physical.push(space);
            }
            physical.push(g.clone());
        }
        out.extend(physical);
        out.extend(lines[i - 1].iter().filter(|g| g.ch == '\n').cloned());
    }
    (out, changed)
}

// A glyph-sized raster in a text baseline's gap is an inline object in prose,
// headings or code. Use painted glyph edges: proportional letters and PDFium's
// synthetic spaces do not have a reliable fixed character advance.
fn insertion(glyphs: &[Glyph], bounds: [f32; 4]) -> Option<(usize, bool)> {
    let [left, bottom, right, top] = bounds;
    let width = right - left;
    let height = top - bottom;
    let fits = |g: &Glyph| {
        !g.ch.is_whitespace()
            && !g.ch.is_control()
            && g.size > 0.
            && width >= g.size * 0.4
            && width <= g.size * 1.6
            && height >= g.size * 0.4
            && height <= g.size * 1.6
            && (g.y - bottom).abs() <= g.size * 0.3
    };
    // Reject overlaid pictures, even if their edge happens to touch a letter.
    if glyphs
        .iter()
        .any(|g| fits(g) && g.x < right - 0.5 && g.right > left + 0.5)
    {
        return None;
    }
    let next = glyphs
        .iter()
        .enumerate()
        .filter(|(_, g)| fits(g) && g.x >= right - 0.5 && g.x - right <= g.size * 0.6)
        .min_by(|a, b| a.1.x.total_cmp(&b.1.x));
    let prev = glyphs
        .iter()
        .enumerate()
        .filter(|(_, g)| fits(g) && g.right <= left + 0.5 && left - g.right <= g.size * 0.6)
        .max_by(|a, b| a.1.right.total_cmp(&b.1.right));
    match (prev, next) {
        (Some((a, prev)), Some((b, next)))
            if a < b && (prev.y - next.y).abs() <= next.size * 0.2 =>
        {
            Some((b, false))
        }
        // At a physical line's ends require an even tighter gap. A nearby icon
        // in the margin or on another baseline must remain a separate figure.
        (None, Some((i, g))) if g.x - right <= g.size * 0.5 => Some((i, false)),
        (Some((i, g)), None) if left - g.right <= g.size * 0.5 => Some((i, true)),
        _ => None,
    }
}

fn candidates(page: &PdfPage<'_>, glyphs: &[Glyph]) -> Vec<(usize, usize, bool, PdfQuadPoints)> {
    page.objects()
        .iter()
        .enumerate()
        .filter_map(|(index, object)| {
            object.as_image_object()?;
            let bounds = object.bounds().ok()?;
            let (anchor, after) = insertion(
                glyphs,
                [
                    bounds.left().value,
                    bounds.bottom().value,
                    bounds.right().value,
                    bounds.top().value,
                ],
            )?;
            Some((index, anchor, after, bounds))
        })
        .collect()
}

pub(crate) fn inline_image_indices(page: &PdfPage<'_>) -> Vec<usize> {
    page.text()
        .map(|text| {
            candidates(page, &glyphs(&text))
                .into_iter()
                .map(|c| c.0)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn table_bounds(page: &PdfPage<'_>) -> Vec<[f32; 4]> {
    page.text()
        .map(|text| {
            let glyphs = recover_fragmented_lines(&glyphs(&text)).0;
            let mut bounds: Vec<_> = table::recover(page, &glyphs)
                .into_iter()
                .map(|t| t.bounds)
                .collect();
            for region in table::regions(page, &glyphs) {
                if !bounds.iter().any(|b| table::contains(*b, region)) {
                    bounds.push(region);
                }
            }
            bounds
        })
        .unwrap_or_default()
}

pub(crate) struct TextHighlight {
    pub index: usize,
    pub quote: String,
    pub prefix: String,
    pub suffix: String,
    pub color: u32,
}

/// A line-height fill behind monospace text is a code highlight, not art.
pub(crate) fn text_highlights(page: &PdfPage<'_>) -> Vec<TextHighlight> {
    let Ok(text) = page.text() else {
        return vec![];
    };
    let gs = glyphs(&text);
    page.objects()
        .iter()
        .enumerate()
        .filter_map(|(index, object)| {
            let path = object.as_path_object()?;
            crate::pdf::rectangle_bounds(&object)?;
            let b = object.bounds().ok()?;
            let color = path.fill_color().ok()?;
            if color.alpha() == 0 {
                return None;
            }
            let inside: Vec<_> = gs
                .iter()
                .enumerate()
                .filter(|(_, g)| {
                    !g.ch.is_whitespace()
                        && !g.ch.is_control()
                        && g.x >= b.left().value - 0.5
                        && g.x <= b.right().value
                        && g.y >= b.bottom().value
                        && g.y <= b.top().value
                })
                .collect();
            let &(start, first) = inside.first()?;
            let &(end, last) = inside.last()?;
            if inside.len() < 3
                || inside
                    .iter()
                    .any(|(_, g)| !g.mono || (g.y - first.y).abs() > first.size * 0.15)
                || b.height().value < first.size * 0.5
                || b.height().value > first.size * 1.5
                || last.right - first.x < b.width().value * 0.65
            {
                return None;
            }
            let quote = gs[start..=end].iter().map(|g| g.ch).collect();
            let mut prefix: Vec<_> = gs[..start]
                .iter()
                .rev()
                .take_while(|g| g.ch != '\n')
                .take(80)
                .map(|g| g.ch)
                .collect();
            prefix.reverse();
            let suffix = gs[end + 1..]
                .iter()
                .take_while(|g| g.ch != '\n')
                .take(80)
                .map(|g| g.ch)
                .collect();
            Some(TextHighlight {
                index,
                quote,
                prefix: prefix.into_iter().collect(),
                suffix,
                color: (color.red() as u32) << 16
                    | (color.green() as u32) << 8
                    | color.blue() as u32,
            })
        })
        .collect()
}

/// Captions are reading text, never external axis labels to bake into a crop.
pub(crate) fn caption_bounds(page: &PdfPage<'_>) -> Vec<[f32; 4]> {
    captions(page)
        .into_iter()
        .flat_map(|(_, lines)| lines)
        .collect()
}

/// One box per numbered Figure caption, including its wrapped physical lines.
/// Shared captions are the semantic evidence for grouping disconnected panels.
pub(crate) fn figure_caption_bounds(page: &PdfPage<'_>) -> Vec<[f32; 4]> {
    captions(page)
        .into_iter()
        .filter(|(figure, _)| *figure)
        .map(|(_, lines)| {
            lines.into_iter().fold(
                [
                    f32::INFINITY,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                    f32::NEG_INFINITY,
                ],
                |a, b| {
                    [
                        a[0].min(b[0]),
                        a[1].min(b[1]),
                        a[2].max(b[2]),
                        a[3].max(b[3]),
                    ]
                },
            )
        })
        .collect()
}

fn captions(page: &PdfPage<'_>) -> Vec<(bool, Vec<[f32; 4]>)> {
    let Ok(text) = page.text() else {
        return vec![];
    };
    let (glyphs, _) = recover_fragmented_lines(&glyphs(&text));
    let lines: Vec<_> = glyphs.split(|g| g.ch == '\n').collect();
    let mut out = vec![];
    let mut i = 0;
    while i < lines.len() {
        if !caption_start(lines[i]) && !leading_caption(&lines, i) {
            i += 1;
            continue;
        }
        let Some(first) = lines[i].iter().find(|g| !g.ch.is_whitespace()) else {
            i += 1;
            continue;
        };
        let figure = line_text(lines[i]).starts_with("Figure ");
        // Physical-line recovery may join two independent captions on one
        // baseline. A second numbered label across a column-sized gap is not
        // a shared caption, even though both runs begin at the same y.
        let line = lines[i];
        let mut starts = vec![0];
        for j in 1..line.len() {
            if matches!(line[j].ch, 'F' | 'T')
                && caption_start(&line[j..])
                && line[..j]
                    .iter()
                    .rfind(|g| !g.ch.is_whitespace())
                    .is_some_and(|prev| line[j].x - prev.right > first.size * 2.)
            {
                starts.push(j);
            }
        }
        if starts.len() > 1 {
            starts.push(line.len());
            for range in starts.windows(2) {
                let fragment = &line[range[0]..range[1]];
                out.push((
                    line_text(fragment).trim_start().starts_with("Figure "),
                    vec![caption_line_bounds(fragment)],
                ));
            }
            i += 1;
            continue;
        }
        let mut bounds = vec![];
        loop {
            let line = lines[i];
            bounds.push(caption_line_bounds(line));
            i += 1;
            if i >= lines.len() || !caption_continuation(first, line, lines[i]) {
                break;
            }
        }
        out.push((figure, bounds));
    }
    out
}

fn caption_line_bounds(line: &[Glyph]) -> [f32; 4] {
    let visible = || line.iter().filter(|g| !g.ch.is_whitespace());
    [
        visible()
            .map(|g| g.x - g.size * 0.2)
            .fold(f32::INFINITY, f32::min),
        visible()
            .map(|g| g.y - g.size * 0.4)
            .fold(f32::INFINITY, f32::min),
        visible()
            .map(|g| g.right + g.size * 0.2)
            .fold(f32::NEG_INFINITY, f32::max),
        visible()
            .map(|g| g.y + g.size)
            .fold(f32::NEG_INFINITY, f32::max),
    ]
}

type ReadingHtml = (
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
    let mut glyphs = glyphs(&text);
    let source: String = glyphs
        .iter()
        .map(|g| g.ch)
        .collect::<String>()
        .replace("\r\n", "\n");
    let hidden = crate::pdf::hidden_text_indices(page, &text);
    if !hidden.is_empty() {
        // glyphs() skips characters without Unicode; retain the same index
        // mapping while leaving PDFium's generated line breaks intact.
        let indices = text
            .chars()
            .iter()
            .filter(|ch| ch.unicode_char().is_some())
            .map(|ch| ch.index())
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

fn attach_links(page: &PdfPage<'_>, glyphs: &mut [Glyph]) {
    for annotation in page.annotations().iter() {
        let Some(annotation) = annotation.as_link_annotation() else {
            continue;
        };
        let Ok(link) = annotation.link() else {
            continue;
        };
        let Ok(rect) = link.rect() else {
            continue;
        };
        let action = link.action();
        let destination = link.destination().or_else(|| {
            action
                .as_ref()
                .and_then(|a| a.as_local_destination_action())
                .and_then(|a| a.destination().ok())
        });
        let url = if let Some(d) = destination {
            let Ok(index) = d.page_index() else {
                continue;
            };
            let y = match d.view_settings().ok() {
                Some(PdfDestinationViewSettings::SpecificCoordinatesAndZoom(_, y, _))
                | Some(PdfDestinationViewSettings::FitPageHorizontallyToWindow(y))
                | Some(PdfDestinationViewSettings::FitBoundsHorizontallyToWindow(y)) => {
                    y.map(|p| p.value)
                }
                Some(PdfDestinationViewSettings::FitPageToRectangle(r)) => Some(r.top().value),
                _ => None,
            };
            Some(format!(
                "easyread-pdf:{}:{}",
                index + 1,
                y.map(|y| y.to_string()).unwrap_or_default()
            ))
        } else {
            action
                .as_ref()
                .and_then(|a| a.as_uri_action())
                .and_then(|a| a.uri().ok())
                .filter(|url| {
                    url.starts_with("https://")
                        || url.starts_with("http://")
                        || url.starts_with("mailto:")
                })
        };
        let Some(url) = url else {
            continue;
        };
        let quads: Vec<_> = annotation.attachment_points().iter().collect();
        let regions: Vec<_> = if quads.is_empty() {
            vec![rect]
        } else {
            quads.iter().map(PdfQuadPoints::to_rect).collect()
        };
        for index in link_glyphs(glyphs, &regions, quads.is_empty()) {
            glyphs[index].link = Some(url.clone());
        }
    }
}

fn link_glyphs(glyphs: &[Glyph], regions: &[PdfRect], fallback: bool) -> Vec<usize> {
    let mut selected: Vec<_> = glyphs
        .iter()
        .enumerate()
        .filter(|(_, g)| !g.ch.is_control())
        .filter(|(_, g)| {
            let y = g.y + g.size * 0.3;
            regions.iter().any(|r| {
                g.x >= r.left().value - 0.5
                    && g.x < r.right().value
                    && y >= r.bottom().value
                    && y <= r.top().value
            })
        })
        .map(|(i, _)| i)
        .collect();
    if !fallback || selected.is_empty() {
        return selected;
    }
    let first = &glyphs[selected[0]];
    if selected
        .iter()
        .all(|&i| (glyphs[i].y - first.y).abs() < first.size * 0.5)
    {
        return selected;
    }
    // A wrapped link's bounding rectangle also covers unrelated prose. Some
    // PDFs omit QuadPoints; use a consistent distinct ink on every linked
    // baseline in that case. Without that evidence the label is ambiguous.
    let mut colors = std::collections::HashMap::new();
    for g in glyphs.iter().filter(|g| !g.ch.is_whitespace()) {
        *colors.entry(g.color).or_insert(0usize) += 1;
    }
    let body = colors
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(color, _)| color);
    let inks: std::collections::HashSet<_> = selected
        .iter()
        .map(|&i| &glyphs[i])
        .filter(|g| !g.ch.is_whitespace() && Some(g.color) != body)
        .map(|g| g.color)
        .collect();
    if inks.len() != 1 {
        return vec![];
    }
    let ink = *inks.iter().next().unwrap();
    if selected.iter().any(|&i| {
        let g = &glyphs[i];
        !g.ch.is_whitespace()
            && !selected.iter().any(|&j| {
                let other = &glyphs[j];
                !other.ch.is_whitespace()
                    && other.color == ink
                    && (other.y - g.y).abs() < g.size * 0.5
            })
    }) {
        return vec![];
    }
    selected.retain(|&i| glyphs[i].color == ink);
    selected
}

pub(crate) fn destination_quote(page: &PdfPage<'_>, y: Option<f32>) -> anyhow::Result<String> {
    let glyphs = glyphs(&page.text()?);
    let lines: Vec<_> = glyphs
        .split(|g| g.ch == '\n')
        .filter(|line| !line_text(line).trim().is_empty())
        .collect();
    let line = y
        .and_then(|y| {
            lines
                .iter()
                .filter_map(|line| {
                    let first = line.iter().find(|g| !g.ch.is_whitespace())?;
                    // A figure has no extracted text: the first line below its destination
                    // is its caption. Avoid selecting the preceding paragraph's last line.
                    (first.y <= y + first.size * 0.2).then_some((*line, (y - first.y).abs()))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|p| p.0)
        })
        .or_else(|| lines.first().copied());
    Ok(line.map(line_text).unwrap_or_default().trim().into())
}

fn apply_links(glyphs: &[Glyph], mut html: String) -> String {
    let mut i = 0;
    while i < glyphs.len() {
        let Some(url) = &glyphs[i].link else {
            i += 1;
            continue;
        };
        let start = i;
        while i < glyphs.len() && glyphs[i].link.as_ref() == Some(url) && !glyphs[i].ch.is_control()
        {
            i += 1;
        }
        if i == start {
            i += 1;
            continue;
        }
        let quote: String = glyphs[start..i]
            .iter()
            .map(|g| g.ch)
            .filter(|ch| !ch.is_control() || ch.is_whitespace())
            .collect();
        let prefix: String = glyphs[start.saturating_sub(40)..start]
            .iter()
            .map(|g| g.ch)
            .filter(|ch| !ch.is_control() || ch.is_whitespace())
            .collect();
        let suffix: String = glyphs[i..(i + 40).min(glyphs.len())]
            .iter()
            .map(|g| g.ch)
            .collect();
        html = crate::annotations::source_link(&html, &quote, &prefix, &suffix, url);
    }
    html
}

fn apply_prose_images(glyphs: &[Glyph], mut html: String) -> String {
    let mut offset = 0;
    for line in glyphs.split(|g| g.ch == '\n') {
        if !is_code(line) {
            for (j, g) in line.iter().enumerate() {
                for (image, after) in [(&g.image_before, false), (&g.image_after, true)] {
                    if let Some(image) = image {
                        // A phrase is a safer fallback than one repeated letter
                        // when figure labels or a footer were omitted by reflow.
                        let mut start = if after { j.saturating_sub(31) } else { j };
                        let mut end = if after {
                            j + 1
                        } else {
                            (j + 32).min(line.len())
                        };
                        while start < j && line[start].ch.is_whitespace() {
                            start += 1;
                        }
                        while end > j + 1 && line[end - 1].ch.is_whitespace() {
                            end -= 1;
                        }
                        let quote = line_text(&line[start..end]);
                        let start = offset + start;
                        let end = offset + end;
                        let prefix: String = glyphs[start.saturating_sub(80)..start]
                            .iter()
                            .map(|g| g.ch)
                            .collect();
                        let suffix: String = glyphs[end..(end + 80).min(glyphs.len())]
                            .iter()
                            .map(|g| g.ch)
                            .collect();
                        html = crate::annotations::source_image(
                            &html, &quote, &prefix, &suffix, image, after,
                        );
                    }
                }
            }
        }
        offset += line.len() + 1;
    }
    html
}

fn is_code(line: &[Glyph]) -> bool {
    let visible: Vec<_> = line.iter().filter(|g| !g.ch.is_whitespace()).collect();
    !visible.is_empty() && visible.iter().all(|g| g.mono)
}

fn line_text(line: &[Glyph]) -> String {
    line.iter()
        .filter(|g| !g.ch.is_control() || g.ch == '\t')
        .map(|g| g.ch)
        .collect()
}

fn normalized_title(text: &str) -> String {
    text.replace('\u{ad}', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Only whole physical lines on a bookmark's destination page are eligible.
/// Wrapped titles must keep the same typography; references inside sentences,
/// index entries and similarly worded headings on other pages remain prose.
fn bookmarked_heading(
    lines: &[&[Glyph]],
    start: usize,
    headings: &[crate::pdf::OutlineEntry],
) -> Option<(usize, usize, String)> {
    let first = lines.get(start)?.iter().find(|g| !g.ch.is_whitespace())?;
    let mut title = String::new();
    for (offset, line) in lines[start..].iter().take(6).enumerate() {
        let visible: Vec<_> = line.iter().filter(|g| !g.ch.is_whitespace()).collect();
        if visible.is_empty()
            || visible.iter().any(|g| {
                g.mono || (g.size - first.size).abs() > first.size * 0.05 || g.bold != first.bold
            })
        {
            break;
        }
        if !title.is_empty() {
            title.push(' ');
        }
        title.push_str(&normalized_title(&line_text(line)));
        if let Some(entry) = headings
            .iter()
            .find(|entry| normalized_title(&entry.title) == title)
        {
            return Some((offset + 1, (entry.depth + 1).min(6), title));
        }
        if !headings
            .iter()
            .any(|entry| normalized_title(&entry.title).starts_with(&format!("{title} ")))
        {
            break;
        }
    }
    None
}

/// Recover unbookmarked section titles only with a complete, distinctly bold
/// physical line followed by ordinary body text or an aligned, smaller bold
/// subheading. Inline emphasis stays prose.
fn typographic_heading(lines: &[&[Glyph]], start: usize) -> Option<(usize, usize, String)> {
    let first = lines.get(start)?.iter().find(|g| !g.ch.is_whitespace())?;
    if !first.bold || first.mono {
        return None;
    }
    let mut title = String::new();
    for (offset, line) in lines[start..].iter().take(6).enumerate() {
        let visible: Vec<_> = line.iter().filter(|g| !g.ch.is_whitespace()).collect();
        if visible.is_empty()
            || visible
                .iter()
                .any(|g| !g.bold || g.mono || (g.size - first.size).abs() > first.size * 0.05)
        {
            break;
        }
        if !title.is_empty() {
            title.push(' ');
        }
        title.push_str(&normalized_title(&line_text(line)));
        if title.chars().count() > 100
            || title.ends_with(['.', '?', '!', ':', ';', '。', '？', '！'])
        {
            return None;
        }
        let next_line = lines.get(start + offset + 1)?;
        let next = next_line.iter().find(|g| !g.ch.is_whitespace())?;
        let smaller_subheading = next.bold
            && first.size >= next.size * 1.1
            && (next.x - first.x).abs() <= first.size * 0.5
            && visible[0].y - next.y >= first.size * 0.75
            && next_line
                .iter()
                .filter(|g| !g.ch.is_whitespace())
                .all(|g| g.bold && !g.mono && (g.size - next.size).abs() <= next.size * 0.05);
        if (!next.bold && !next.mono && first.size >= next.size * 0.95 && first.y > next.y)
            || smaller_subheading
        {
            return Some((offset + 1, 3, title));
        }
    }
    None
}

fn reading_heading(
    lines: &[&[Glyph]],
    start: usize,
    headings: &[crate::pdf::OutlineEntry],
) -> Option<(usize, usize, String)> {
    bookmarked_heading(lines, start, headings).or_else(|| {
        typographic_heading(lines, start).filter(|(_, _, title)| {
            !headings
                .iter()
                .any(|entry| normalized_title(&entry.title).starts_with(&format!("{title} ")))
        })
    })
}

/// A list label is not a chapter heading. Preserve its italic presentation and
/// paragraph boundaries, using the PDF typography rather than the marker alone.
fn label_prefix(text: &str) -> Option<usize> {
    let end = text.find(char::is_whitespace)?;
    let marker = &text[..end];
    let token = if marker.starts_with('(') && marker.ends_with(')') {
        &marker[1..marker.len() - 1]
    } else {
        marker.strip_suffix(['.', ')'])?
    };
    let numeric =
        !token.is_empty() && token.len() <= 3 && token.chars().all(|c| c.is_ascii_digit());
    let alphabetic = token.len() == 1 && token.chars().all(|c| c.is_ascii_alphabetic());
    let roman = matches!(
        token.to_ascii_uppercase().as_str(),
        "II" | "III" | "IV" | "VI" | "VII" | "VIII" | "IX" | "XI" | "XII"
    );
    (numeric || alphabetic || roman).then_some(end)
}

fn italic_label(lines: &[&[Glyph]], start: usize) -> Option<(usize, String)> {
    let text = normalized_title(&line_text(lines[start]));
    let numbered = label_prefix(&text);
    let prefix = numbered.unwrap_or(0);
    let title_start = text[prefix..]
        .trim_start_matches([' ', '('])
        .chars()
        .next()?;
    if !title_start.is_alphabetic() || (numbered.is_some() && title_start.is_lowercase()) {
        return None;
    }
    let first = lines[start].iter().find(|g| !g.ch.is_whitespace())?;
    if first.size <= 0. || first.mono {
        return None;
    }
    // PDFium can split a font run into an extraction line on the same baseline.
    if start > 0
        && lines[start - 1]
            .iter()
            .rev()
            .find(|g| !g.ch.is_whitespace())
            .is_some_and(|g| (g.y - first.y).abs() <= first.size * 0.2)
    {
        return None;
    }
    let mut title = String::new();
    for (offset, line) in lines[start..].iter().take(6).enumerate() {
        let line_start = line.iter().find(|g| !g.ch.is_whitespace())?;
        if offset > 0
            && (label_prefix(&normalized_title(&line_text(line))).is_some()
                || !caption_continuation(first, lines[start + offset - 1], line)
                || line_start.x < first.x - first.size * 0.5)
        {
            return None;
        }
        // The marker may be roman; all of the label's words must be italic.
        let marker_chars = if offset == 0 {
            text[..prefix].chars().count()
        } else {
            0
        };
        let words: Vec<_> = line
            .iter()
            .skip_while(|g| g.ch.is_whitespace())
            .skip(marker_chars)
            .filter(|g| !g.ch.is_whitespace())
            .collect();
        if words.is_empty()
            || words.iter().any(|g| {
                !g.italic || g.mono || g.bold || (g.size - first.size).abs() > first.size * 0.05
            })
        {
            return None;
        }
        if !title.is_empty() {
            title.push(' ');
        }
        title.push_str(&normalized_title(&line_text(line)));
        if title.chars().count() > 180 || title.ends_with(['.', '?', '!', ';', '。', '？', '！'])
        {
            return None;
        }
        let next = lines
            .get(start + offset + 1)?
            .iter()
            .find(|g| !g.ch.is_whitespace())?;
        if !next.italic
            && !next.bold
            && !next.mono
            && first.size >= next.size * 0.95
            && line_start.y - next.y > first.size * 0.2
        {
            // Unnumbered labels need layout evidence as well as italics:
            // an indented explanation or extra separation from its body.
            // This keeps ordinary italic sentences and quotations in prose.
            if numbered.is_none()
                && (title.chars().count() > 100
                    || title.split_whitespace().count() > 14
                    || (next.x < first.x + first.size * 0.75
                        && (title_start.is_lowercase()
                            || line_start.y - next.y < first.size * 1.6)))
            {
                return None;
            }
            return Some((offset + 1, title));
        }
    }
    None
}

/// Infer paragraph boundaries from visible body lines, after figures/tables
/// have been removed. Physical wraps stay together even without punctuation.
fn paragraph_breaks(lines: &[&[Glyph]], omitted: &std::collections::HashSet<usize>) -> Vec<bool> {
    let body = |i: usize| {
        if omitted.contains(&i) {
            return None;
        }
        let first = lines[i]
            .iter()
            .find(|g| !g.ch.is_whitespace() && !g.ch.is_control())
            .filter(|g| !g.mono && !g.bold && g.size > 0.)?;
        // PDFium can put multiple physical wraps in one extracted line. Find
        // its first and last physical rows before choosing body baselines;
        // otherwise a longer second row looks like a large paragraph gap.
        let mut first_end = lines[i].len();
        let mut last_start = 0;
        let mut previous: Option<&Glyph> = None;
        for (index, g) in lines[i]
            .iter()
            .enumerate()
            .filter(|(_, g)| !g.ch.is_whitespace() && !g.ch.is_control())
        {
            if previous
                .is_some_and(|prev| prev.y - g.y >= first.size * 0.5 && prev.x - g.x >= first.size)
            {
                first_end = first_end.min(index);
                last_start = index;
            }
            previous = Some(g);
        }
        // Raised references do not define a physical row's baseline or indent.
        fn anchor(row: &[Glyph], size: f32) -> Option<&Glyph> {
            let mut baselines = std::collections::BTreeMap::<i32, usize>::new();
            for g in row
                .iter()
                .filter(|g| !g.ch.is_whitespace() && !g.ch.is_control())
            {
                *baselines.entry((g.y * 10.).round() as i32).or_default() += 1;
            }
            let baseline = baselines.into_iter().max_by_key(|(_, count)| *count)?.0;
            row.iter().find(|g| {
                !g.ch.is_whitespace()
                    && !g.ch.is_control()
                    && (g.y * 10.).round() as i32 == baseline
                    && (g.size - size).abs() <= size * 0.05
            })
        }
        Some((
            anchor(&lines[i][..first_end], first.size)?,
            anchor(&lines[i][last_start..], first.size)?,
        ))
    };
    let mut spacings = Vec::new();
    for i in 1..lines.len() {
        if let Some(((prev, prev_end), (next, _))) = body(i - 1).zip(body(i)) {
            let gap = prev_end.y - next.y;
            if (prev.size - next.size).abs() <= next.size * 0.05
                && (prev.x - next.x).abs() <= next.size * 0.5
                && gap >= next.size
                && gap <= next.size * 2.5
            {
                spacings.push((next.size, (gap / next.size * 20.).round() as u32));
            }
        }
    }
    let mut breaks = vec![false; lines.len()];
    for (i, boundary) in breaks.iter_mut().enumerate().skip(1) {
        let Some(((prev, prev_end), (next, _))) = body(i - 1).zip(body(i)) else {
            continue;
        };
        if (prev.size - next.size).abs() > next.size * 0.05 {
            continue;
        }
        let mut bins = std::collections::BTreeMap::<u32, usize>::new();
        for (_, gap) in spacings
            .iter()
            .filter(|(size, _)| (*size - next.size).abs() <= next.size * 0.05)
        {
            *bins.entry(*gap).or_default() += 1;
        }
        // Favor the smaller spacing only when frequencies tie.
        let typical = bins
            .into_iter()
            .max_by_key(|(gap, count)| (*count, std::cmp::Reverse(*gap)))
            .map_or(next.size * 1.25, |(gap, _)| next.size * gap as f32 / 20.);
        let gap = prev_end.y - next.y;
        *boundary =
            gap > typical * 1.3 || (next.x - prev.x >= next.size * 0.75 && gap >= typical * 0.8);
    }
    breaks
}

/// Hide only a standalone matching page number in the centered bottom margin.
/// This is presentation-only: source extraction and annotation records stay
/// intact. Numbers in prose, tables, equations and footnotes remain visible.
fn page_number_lines(
    glyphs: &[Glyph],
    width: f32,
    height: f32,
    page_number: usize,
) -> std::collections::HashSet<usize> {
    let expected = page_number.to_string();
    glyphs
        .split(|g| g.ch == '\n')
        .enumerate()
        .filter_map(|(i, line)| {
            if line_text(line).trim() != expected {
                return None;
            }
            let visible: Vec<_> = line
                .iter()
                .filter(|g| !g.ch.is_whitespace() && !g.ch.is_control())
                .collect();
            let first = *visible.first()?;
            let last = *visible.last()?;
            (first.size > 0.
                && first.y > 0.
                && first.y < height * 0.08
                && ((first.x + last.right) * 0.5 - width * 0.5).abs() < width * 0.05
                && visible.iter().all(|g| {
                    !g.mono
                        && !g.bold
                        && g.link.is_none()
                        && (g.y - first.y).abs() < first.size * 0.1
                })
                && glyphs
                    .iter()
                    .filter(|g| {
                        !g.ch.is_whitespace()
                            && !g.ch.is_control()
                            && g.y > first.y + first.size * 2.
                    })
                    .count()
                    >= 20
                && !glyphs.iter().any(|g| {
                    !g.ch.is_whitespace()
                        && !g.ch.is_control()
                        && g.y > first.y + first.size * 0.2
                        && g.y < first.y + first.size * 2.
                }))
            .then_some(i)
        })
        .collect()
}

fn caption_start(line: &[Glyph]) -> bool {
    let text = line_text(line);
    (text.starts_with("Figure ") || text.starts_with("Table "))
        && line
            .iter()
            .find(|g| !g.ch.is_whitespace())
            .is_some_and(|g| {
                !g.mono
                    && (g.italic
                        || text.split_whitespace().nth(1).is_some_and(|label| {
                            let number = label.trim_end_matches([':', '.']);
                            number != label
                                && number.chars().any(|c| c.is_ascii_digit())
                                && number
                                    .chars()
                                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
                        }))
            })
}

/// A caption can continue at the top of the following PDF page without its
/// Figure/Table prefix. Keep the smaller italic run separate from body text.
fn leading_caption(lines: &[&[Glyph]], start: usize) -> bool {
    if caption_start(lines[start]) {
        return false;
    }
    if lines[..start]
        .iter()
        .any(|line| line.iter().any(|g| !g.ch.is_whitespace()))
    {
        return false;
    }
    let Some(first) = lines[start].iter().find(|g| !g.ch.is_whitespace()) else {
        return false;
    };
    if first.size <= 0. || first.mono || !first.italic {
        return false;
    }
    for i in start..(start + 4).min(lines.len()) {
        if !lines[i]
            .iter()
            .filter(|g| !g.ch.is_whitespace())
            .all(|g| g.italic && !g.mono && (g.size - first.size).abs() <= first.size * 0.05)
        {
            return false;
        }
        let Some(next) = lines
            .get(i + 1)
            .and_then(|line| line.iter().find(|g| !g.ch.is_whitespace()))
        else {
            return false;
        };
        if !next.italic && !next.mono && !next.bold {
            let current = lines[i].iter().find(|g| !g.ch.is_whitespace()).unwrap();
            return next.size >= first.size * 1.1
                && next.x <= first.x + first.size * 0.25
                && current.y - next.y >= first.size * 1.5;
        }
        if !caption_continuation(first, lines[i], lines[i + 1]) {
            return false;
        }
    }
    false
}

/// Centered captions can end with a single word on the next physical line.
/// Keep nearby lines in the same caption style together, stopping at body text
/// or a separate caption even when extraction supplies no blank separator.
fn caption_continuation(first: &Glyph, previous: &[Glyph], next: &[Glyph]) -> bool {
    let Some(last) = previous.iter().find(|g| !g.ch.is_whitespace()) else {
        return false;
    };
    let Some(start) = next.iter().find(|g| !g.ch.is_whitespace()) else {
        return false;
    };
    let gap = last.y - start.y;
    !caption_start(next)
        && start.italic == first.italic
        && first.size > 0.
        && gap > first.size * 0.1
        && gap <= first.size * 1.8
        && next.iter().filter(|g| !g.ch.is_whitespace()).all(|g| {
            (g.italic == first.italic || !first.italic)
                && g.bold == first.bold
                && !g.mono
                && (g.size - first.size).abs() <= first.size * 0.05
        })
}

/// Require a raised baseline, not merely a small digit. Some publishers use
/// full-size raised reference labels, so size alone cannot identify a script.
fn superscripts(glyphs: &[Glyph]) -> Vec<bool> {
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
fn raised_item_start(line: &[Glyph], scripts: &[bool]) -> bool {
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

fn apply_superscripts(glyphs: &[Glyph], scripts: &[bool], mut html: String) -> String {
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

#[cfg(test)]
fn render(glyphs: &[Glyph], headings: &[crate::pdf::OutlineEntry]) -> Option<String> {
    render_page(glyphs, headings, &[], &std::collections::HashSet::new())
}

fn render_page(
    glyphs: &[Glyph],
    headings: &[crate::pdf::OutlineEntry],
    figures: &[(usize, String)],
    omitted: &std::collections::HashSet<usize>,
) -> Option<String> {
    render_region(glyphs, headings, figures, omitted, !omitted.is_empty())
}

fn render_region(
    glyphs: &[Glyph],
    headings: &[crate::pdf::OutlineEntry],
    figures: &[(usize, String)],
    omitted: &std::collections::HashSet<usize>,
    force: bool,
) -> Option<String> {
    let lines: Vec<_> = glyphs.split(|g| g.ch == '\n').collect();
    let paragraphs = paragraph_breaks(&lines, omitted);
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
            apply_prose_images(glyphs, apply_typography(glyphs, &typography, html)),
        ),
    ))
}

fn apply_typography(glyphs: &[Glyph], eligible: &[bool], mut html: String) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordered_items_keep_literal_numbers_and_annotation_text_on_separate_rows() {
        let gs = test_lines(&[
            ("The following steps:", 12., false, false),
            (
                "1. Embed all external documents the agent has no direct access to.",
                12.,
                false,
                false,
            ),
            ("2. Embed the user’s query.", 12., false, false),
            ("3. Compare the embeddings and create", 12., false, false),
            ("a similarity matrix.", 12., false, false),
            (
                "4. Return the documents with the highest similarity.",
                12.,
                false,
                false,
            ),
            ("5. Add those documents to the prompt.", 12., false, false),
            ("This gives us the following class:", 12., false, false),
        ]);
        let html = render(&gs, &[]).unwrap();
        let paragraphs: Vec<_> = html
            .split("</p>")
            .map(crate::annotations::plain_text)
            .filter(|p| !p.trim().is_empty())
            .collect();
        assert_eq!(
            paragraphs.iter().map(|p| p.trim()).collect::<Vec<_>>(),
            [
                "The following steps:",
                "1. Embed all external documents the agent has no direct access to.",
                "2. Embed the user’s query.",
                "3. Compare the embeddings and create a similarity matrix.",
                "4. Return the documents with the highest similarity.",
                "5. Add those documents to the prompt.",
                "This gives us the following class:",
            ],
            "{html}"
        );
        let marked = crate::annotations::decorate_interactive(
            &html,
            &[
                serde_json::json!({"id":"list-note", "quote":"3. Compare the embeddings and create a similarity matrix.", "kind":"highlight", "color":"blue"}),
            ],
        );
        assert!(marked.contains("list-note"), "{marked}");
        assert_eq!(
            crate::annotations::plain_text(&marked),
            crate::annotations::plain_text(&html)
        );
    }

    #[test]
    fn unnumbered_labels_and_physical_paragraphs_keep_wrapped_body_together() {
        for size in [10., 12., 16.] {
            let mut gs = test_lines(&[
                ("The data", size, false, false),
                ("First paragraph begins", size, false, false),
                ("and wraps without punctuation", size, false, false),
                ("Second paragraph begins", size, false, false),
                ("and also wraps", size, false, false),
                ("Tool-use data", size, false, false),
                ("A tool explanation begins", size, false, false),
                ("and continues on this line", size, false, false),
                ("Software engineering", size, false, false),
                ("tasks", size, false, false),
                ("A repository explanation follows", size, false, false),
            ]);
            let ys = [0., 1.75, 3., 4.75, 6., 8.25, 10., 11.25, 13.5, 14.75, 16.5];
            for (i, line) in gs.split_mut(|g| g.ch == '\n').enumerate() {
                for g in line {
                    g.y = 740. - ys[i] * size;
                    g.x += if i == 0 {
                        0.
                    } else if [6, 7, 10].contains(&i) {
                        3. * size
                    } else {
                        1.5 * size
                    };
                    g.italic = [0, 5, 8, 9].contains(&i);
                }
            }
            let html = render(&gs, &[]).unwrap();
            let ps: Vec<_> = html
                .split("</p>")
                .map(crate::annotations::plain_text)
                .filter(|p| !p.trim().is_empty())
                .collect();
            assert_eq!(
                ps.iter().map(|p| p.trim()).collect::<Vec<_>>(),
                [
                    "The data",
                    "First paragraph begins and wraps without punctuation",
                    "Second paragraph begins and also wraps",
                    "Tool-use data",
                    "A tool explanation begins and continues on this line",
                    "Software engineering tasks",
                    "A repository explanation follows"
                ],
                "{html}"
            );
            assert_eq!(html.matches("<p><em>").count(), 3);
            let marked = crate::annotations::decorate_interactive(
                &html,
                &[
                    serde_json::json!({"id":"body-note", "quote":"begins and continues", "kind":"highlight", "color":"pink"}),
                ],
            );
            assert!(marked.contains("body-note"));
            assert_eq!(
                crate::annotations::plain_text(&html),
                crate::annotations::plain_text(&marked)
            );
        }
    }

    #[test]
    fn bottom_page_numbers_are_hidden_without_dropping_body_numbers_or_footnotes() {
        for (text, x, y, page, hidden) in [
            ("5", 303., 40., 5, true),
            ("6", 303., 40., 5, false),
            ("(5)", 303., 40., 5, false),
            ("5 experiments", 303., 40., 5, false),
            ("5", 72., 40., 5, false),
            ("5", 303., 500., 5, false),
        ] {
            let mut gs = test_lines(&[
                (
                    "The model uses 5 layers and keeps all these body numbers",
                    12.,
                    false,
                    false,
                ),
                (text, 12., false, false),
            ]);
            for row in gs.split_mut(|g| g.ch == '\n').skip(1) {
                for g in row {
                    let dx = x - 72.;
                    g.x += dx;
                    g.right += dx;
                    g.advance_right += dx;
                    g.y = y;
                }
            }
            let omitted = page_number_lines(&gs, 612., 792., page);
            assert_eq!(omitted.contains(&1), hidden, "{text} at {x},{y}");
            if hidden {
                let html = render_page(&gs, &[], &[], &omitted).unwrap();
                assert_eq!(
                    crate::annotations::plain_text(&html).trim(),
                    "The model uses 5 layers and keeps all these body numbers"
                );
                let mut nearby = test_lines(&[("A nearby footnote", 12., false, false)]);
                for g in &mut nearby {
                    g.y = 55.;
                }
                gs.extend(nearby);
                assert!(page_number_lines(&gs, 612., 792., page).is_empty());
            }
        }
        let mut lone_number = test_lines(&[("5", 12., false, false)]);
        for g in &mut lone_number {
            g.x = 303.;
            g.y = 40.;
        }
        assert!(page_number_lines(&lone_number, 612., 792., 5).is_empty());
    }

    #[test]
    fn merged_physical_rows_use_first_and_last_baselines_for_paragraph_gaps() {
        for continuation in [
            "learned linear transformation and softmax function to convert the decoder output",
            "and continue",
        ] {
            let mut gs = test_lines(&[
                ("Similarly we convert the input", 12., false, false),
                (
                    "tokens and output tokens to vectors of dimension dmodel",
                    12.,
                    false,
                    false,
                ),
                (continuation, 12., false, false),
                ("This is still the same paragraph", 12., false, false),
                ("A new indented paragraph begins", 12., false, false),
                ("and wraps to the margin", 12., false, false),
                ("A paragraph after extra space", 12., false, false),
                ("also wraps to the margin", 12., false, false),
            ]);
            for (i, row) in gs.split_mut(|g| g.ch == '\n').enumerate() {
                for g in row.iter_mut() {
                    if i == 4 {
                        g.x += 18.;
                        g.right += 18.;
                        g.advance_right += 18.;
                    }
                    if i >= 6 {
                        g.y -= 10.;
                    }
                }
                // Inline subscripts must not become a physical row boundary.
                if i == 1 {
                    for g in row.iter_mut().rev().take(5) {
                        g.size = 8.;
                        g.y -= 2.;
                    }
                }
            }
            // Simulate PDFium omitting a newline between the two physical rows.
            let newline = gs
                .iter()
                .enumerate()
                .filter(|(_, g)| g.ch == '\n')
                .nth(1)
                .unwrap()
                .0;
            gs[newline].ch = ' ';
            let lines: Vec<_> = gs.split(|g| g.ch == '\n').collect();
            assert_eq!(
                paragraph_breaks(&lines, &Default::default()),
                [false, false, false, true, false, true, false, false],
            );
            let html = render(&gs, &[]).unwrap();
            let paragraphs: Vec<_> = html
                .split("</p>")
                .map(crate::annotations::plain_text)
                .filter(|p| !p.trim().is_empty())
                .collect();
            assert_eq!(paragraphs.len(), 3, "{html}");
            assert_eq!(
                paragraphs[0].trim(),
                format!(
                    "Similarly we convert the input tokens and output tokens to vectors of dimension dmodel {continuation} This is still the same paragraph"
                ),
            );
            assert_eq!(
                paragraphs[1].trim(),
                "A new indented paragraph begins and wraps to the margin"
            );
            assert_eq!(
                paragraphs[2].trim(),
                "A paragraph after extra space also wraps to the margin"
            );
        }
    }

    #[test]
    fn physical_paragraph_boundaries_work_without_italic_or_punctuation() {
        let mut gs = test_lines(&[
            ("An ordinary paragraph begins", 12., false, false),
            ("and continues", 12., false, false),
            ("A new indented paragraph", 12., false, false),
            ("returns to the margin", 12., false, false),
            ("A paragraph after extra space", 12., false, false),
        ]);
        for (i, line) in gs.split_mut(|g| g.ch == '\n').enumerate() {
            for g in line {
                g.y = [740., 725., 710., 695., 674.][i];
                if i == 2 {
                    g.x += 18.;
                }
            }
        }
        let html = render(&gs, &[]).unwrap();
        assert!(
            html.contains("begins and continues</span></p><p><span>A new"),
            "{html}"
        );
        assert!(
            html.contains("paragraph returns to the margin</span></p><p><span>A paragraph after"),
            "{html}"
        );
    }

    #[test]
    fn lower_case_and_parenthesized_labels_require_an_indented_explanation() {
        for title in ["add", "multiply", "subtract", "(Optional) Generator"] {
            let mut gs = test_lines(&[
                (title, 12., false, false),
                ("An indented explanation follows", 12., false, false),
            ]);
            for (i, line) in gs.split_mut(|g| g.ch == '\n').enumerate() {
                for g in line {
                    g.italic = i == 0;
                    g.y = 740. - i as f32 * 21.;
                    if i == 1 {
                        g.x += 18.;
                    }
                }
            }
            assert!(
                render(&gs, &[])
                    .unwrap()
                    .contains(&format!("<p><em>{title}</em></p>\n<p>"))
            );
            if title.chars().next().unwrap().is_lowercase() {
                for g in gs.split_mut(|g| g.ch == '\n').nth(1).unwrap() {
                    g.x -= 18.;
                }
                let lines: Vec<_> = gs.split(|g| g.ch == '\n').collect();
                assert!(italic_label(&lines, 0).is_none());
            }
        }
    }

    #[test]
    fn cross_page_caption_tail_keeps_emphasis_alignment_and_body_boundary() {
        let mut gs = test_lines(&[
            (
                "then pick the best using generated unit tests",
                9.,
                false,
                false,
            ),
            (
                "In a way, the approach works like retrieval",
                12.,
                false,
                false,
            ),
            ("and continues at normal body spacing", 12., false, false),
        ]);
        for (i, line) in gs.split_mut(|g| g.ch == '\n').enumerate() {
            for g in line {
                g.y = [702., 679., 664.][i];
                g.italic = i == 0;
                if i == 0 {
                    g.x += 100.;
                }
            }
        }
        let html = render(&gs, &[]).unwrap();
        assert!(html.starts_with("<p style=\"text-align:center\"><em>then pick the best using generated unit tests</em></p><p><span>In a way"), "{html}");
        assert!(html.contains("retrieval and continues"));
        for g in gs.iter_mut().take_while(|g| g.ch != '\n') {
            g.size = 12.;
        }
        let lines: Vec<_> = gs.split(|g| g.ch == '\n').collect();
        assert!(
            !leading_caption(&lines, 0),
            "Ordinary italic body paragraph"
        );
    }

    /// Full-source audit, deliberately separate from parallel fixture tests:
    /// PDFium's bindings are process-global. Run with EASYREAD_QA_PDF set.
    #[test]
    #[ignore]
    fn audit_book_structure() -> anyhow::Result<()> {
        let path = std::path::PathBuf::from(std::env::var("EASYREAD_QA_PDF")?);
        let pdfium = Pdfium::new(Pdfium::bind_to_library("assets/pdfium/libpdfium.dylib")?);
        let doc = pdfium.load_pdf_from_file(&path, None)?;
        let headings = crate::pdf::document_outline(&doc);
        let paper: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path.parent().unwrap().join("paper.json"))?)?;
        let output = std::path::Path::new("target/pdf-structure-qa");
        std::fs::create_dir_all(output)?;
        let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut records = Vec::new();
        let mut warnings = Vec::new();
        let mut progress = std::fs::File::create(output.join("progress.jsonl"))?;
        let pages: Vec<usize> = std::env::var("EASYREAD_QA_PAGES")
            .ok()
            .map(|s| s.split(',').map(|n| n.parse().unwrap()).collect())
            .unwrap_or_else(|| (1..=doc.pages().len() as usize).collect());
        for number in pages {
            let page = doc.pages().get(number as i32 - 1)?;
            let (source, presentation, images, _, _) =
                reading_html(&page, &doc, &headings, number)?;
            if let Some(block) = paper["blocks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|b| b["page"] == number && b["source_kind"] == "pdf-text")
            {
                assert_eq!(
                    source,
                    block["en"].as_str().unwrap(),
                    "Stored text changed on page {number}"
                );
            }
            let html = presentation
                .unwrap_or_else(|| crate::annotations::html(&crate::model::pdf_prose(&source)));
            if number == 542 {
                for label in [
                    "The data",
                    "Tool-use data",
                    "Multi-step data",
                    "Unit-test data",
                    "Software engineering tasks",
                    "Reinforcement learning",
                ] {
                    assert!(
                        html.contains(&format!("<p><em>{label}</em></p>")),
                        "Page 542 label: {label}"
                    );
                }
                assert!(html.contains("what the model will see when it’s in use, which includes:"));
                assert!(html.contains("<p><span>Coding task data:"));
            }
            if number == 83 {
                assert!(html.contains("<em>Figure 2-19. Each token flows through its own track in the stack, with only the final token’s track passed to the LM head to predict the next token</em>"), "Fragmented caption regression");
            }
            if number == 314 {
                assert!(html.contains("especially context engineering, as we discussed in"));
                assert!(html.contains("easyread-pdf:187:792"));
            }
            if number == 321 {
                for title in ["add", "multiply", "subtract"] {
                    assert!(html.contains(&format!("<p><em>{title}</em></p>")));
                }
            }
            if number == 429 {
                assert!(html.contains("<p><em>(Optional) Generator</em></p>"));
            }
            if number == 210 {
                for title in [
                    "Conversation history",
                    "Summaries of past events",
                    "User’s portrait",
                ] {
                    assert!(html.contains(&format!("<p><em>{title}</em></p>")));
                }
                assert!(images.contains(&0), "Suppress the legacy whole-panel crop");
                assert!(
                    html.contains("pdf-p210-img049"),
                    "Keep the real retention graph"
                );
                assert!(!html.contains("pdf-p210-img001"));
            }
            if number == 12 {
                let ps: Vec<_> = html
                    .split("</p>")
                    .map(crate::annotations::plain_text)
                    .collect();
                assert!(
                    ps.iter().any(|p| p.trim() == "support@oreilly.com"),
                    "Separate email paragraph"
                );
                assert!(
                    ps.iter()
                        .any(|p| p.trim() == "https://oreilly.com/about/contact.html"),
                    "Separate contact URL paragraph"
                );
            }
            for (n, ids) in [
                (58, vec![1565]),
                (61, vec![941]),
                (199, vec![712, 885]),
                (540, vec![964]),
            ] {
                if number == n {
                    for id in &ids {
                        assert!(images.contains(id), "Inline image regression on {n}: {id}");
                    }
                    assert!(html.matches("<img ").count() >= ids.len());
                    let clean = |s: &str| {
                        s.chars()
                            .filter(|c| (!c.is_control() || c.is_whitespace()) && *c != '\u{ad}')
                            .collect::<String>()
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ")
                    };
                    assert_eq!(
                        clean(&source),
                        clean(&crate::annotations::plain_text(&html)),
                        "Inline page text {number}"
                    );
                }
            }
            std::fs::write(output.join(format!("page-{number:03}.html")), &html)?;
            let visible = norm(&crate::annotations::plain_text(&html));
            let paragraphs: Vec<_> = html
                .split("</p>")
                .filter_map(|part| {
                    part.rfind("<p")
                        .map(|start| norm(&crate::annotations::plain_text(&part[start..])))
                })
                .collect();
            let recovered = recover_fragmented_lines(&glyphs(&page.text()?)).0;
            let lines: Vec<_> = recovered.split(|g| g.ch == '\n').collect();
            let mut labels = Vec::new();
            let mut absent = Vec::new();
            let mut i = 0;
            while i < lines.len() {
                if let Some((count, title)) = italic_label(&lines, i) {
                    if visible.contains(&title) {
                        if !paragraphs.iter().any(|p| p == &title)
                            && reading_heading(&lines, i, &headings).is_none()
                        {
                            warnings.push(
                                serde_json::json!({"page":number,"kind":"label","text":title}),
                            );
                        }
                        labels.push(title);
                    } else {
                        absent.push(title);
                    }
                    i += count;
                } else {
                    i += 1;
                }
            }
            let boundaries = paragraph_breaks(&lines, &Default::default());
            let mut starts = Vec::new();
            for (i, boundary) in boundaries.iter().enumerate() {
                if !boundary {
                    continue;
                }
                let text = norm(&line_text(lines[i]));
                if !text.is_empty() && visible.contains(&text) {
                    if !paragraphs.iter().any(|p| p.starts_with(&text))
                        && reading_heading(&lines, i, &headings).is_none()
                    {
                        warnings.push(
                            serde_json::json!({"page":number,"kind":"paragraph","text":text}),
                        );
                    }
                    starts.push(text);
                }
            }
            records.push(serde_json::json!({"page":number,"labels":labels,"labels_in_visual_regions":absent,"paragraph_starts":starts,"inline_or_recreated_figures":images}));
            use std::io::Write;
            writeln!(
                progress,
                "{}",
                serde_json::json!({"record":records.last(),"warnings":warnings.iter().filter(|w| w["page"] == number).collect::<Vec<_>>()})
            )?;
            if number % 25 == 0 {
                eprintln!("Audited {number} pages");
            }
        }
        std::fs::write(
            output.join("report.json"),
            serde_json::to_vec_pretty(&serde_json::json!({"pages":records,"warnings":warnings}))?,
        )?;
        eprintln!(
            "Audited {} pages, {} candidates to review",
            records.len(),
            warnings.len()
        );
        Ok(())
    }

    /// Discover captions and callout text directly in PDF font/line data,
    /// independently of image expansion and background classification.
    #[test]
    #[ignore]
    fn audit_callouts_and_captions() -> anyhow::Result<()> {
        let path = std::path::PathBuf::from(std::env::var("EASYREAD_QA_PDF")?);
        let pdfium = Pdfium::new(Pdfium::bind_to_library("assets/pdfium/libpdfium.dylib")?);
        let doc = pdfium.load_pdf_from_file(&path, None)?;
        let headings = crate::pdf::document_outline(&doc);
        let paper: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path.parent().unwrap().join("paper.json"))?)?;
        let out = std::path::Path::new("target/pdf-callout-qa");
        std::fs::create_dir_all(out)?;
        let compact = |s: &str| {
            s.chars()
                .filter(|c| !c.is_whitespace() && !c.is_control())
                .collect::<String>()
        };
        let mut rows = vec![];
        let mut unresolved = vec![];
        let mut captions = 0;
        let mut panels = 0;
        for (index, page) in doc.pages().iter().enumerate() {
            let number = index + 1;
            let text = page.text()?;
            let (gs, _) = recover_fragmented_lines(&glyphs(&text));
            let lines: Vec<_> = gs.split(|g| g.ch == '\n').collect();
            let mut expected_captions = vec![];
            let mut notes = vec![];
            for (i, line) in lines.iter().enumerate() {
                let value = line_text(line);
                let value = value.trim();
                let numbered = ["Figure ", "Table "].iter().any(|prefix| {
                    value.strip_prefix(prefix).is_some_and(|tail| {
                        let label = tail.split_whitespace().next().unwrap_or("");
                        label.ends_with('.')
                            && label.chars().next().is_some_and(|c| c.is_ascii_digit())
                    })
                });
                if numbered
                    && line
                        .iter()
                        .find(|g| !g.ch.is_whitespace())
                        .is_some_and(|g| g.italic && !g.mono)
                {
                    let mut caption = value.to_owned();
                    let first = line.iter().find(|g| !g.ch.is_whitespace()).unwrap();
                    let mut previous_y = first.y;
                    for next in lines.iter().skip(i + 1) {
                        let Some(g) = next.iter().find(|g| !g.ch.is_whitespace()) else {
                            break;
                        };
                        let next_text = line_text(next);
                        if !g.italic
                            || g.mono
                            || (g.size - first.size).abs() > first.size * 0.05
                            || previous_y - g.y < first.size * 0.5
                            || previous_y - g.y > first.size * 1.8
                            || next_text.starts_with("Figure ")
                            || next_text.starts_with("Table ")
                        {
                            break;
                        }
                        caption.push(' ');
                        caption.push_str(next_text.trim());
                        previous_y = g.y;
                    }
                    expected_captions.push(caption);
                }
                if ["NOTE", "TIP", "WARNING", "CAUTION", "IMPORTANT"].contains(&value) {
                    notes.push(value.to_owned());
                    for following in lines.iter().skip(i + 1).take(4) {
                        let value = line_text(following);
                        if !value.trim().is_empty() {
                            notes.push(value.trim().to_owned());
                        }
                    }
                }
            }
            // The reported sample continues without repeating its NOTE label.
            if number == 260 || number == 270 {
                notes.extend(
                    lines
                        .iter()
                        .take(4)
                        .map(|l| line_text(l))
                        .filter(|s| !s.trim().is_empty()),
                );
            }
            if expected_captions.is_empty() && notes.is_empty() {
                continue;
            }
            let (source, html, indices, figures, _) = reading_html(&page, &doc, &headings, number)?;
            let stored = paper["blocks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|b| b["page"] == number && b["source_kind"] == "pdf-text")
                .unwrap()["en"]
                .as_str()
                .unwrap();
            assert_eq!(source, stored, "Unchanged source page {number}");
            let html =
                html.unwrap_or_else(|| crate::annotations::html(&crate::model::pdf_prose(&source)));
            let visible = compact(&crate::annotations::plain_text(&html));
            for (kind, values) in [("caption", &expected_captions), ("callout", &notes)] {
                for value in values {
                    if !visible.contains(&compact(value)) {
                        unresolved
                            .push(serde_json::json!({"page":number,"kind":kind,"text":value}));
                    }
                }
            }
            captions += expected_captions.len();
            panels += usize::from(!notes.is_empty());
            std::fs::write(out.join(format!("page-{number}.html")), &html)?;
            rows.push(serde_json::json!({"page":number,"captions":expected_captions,"callout_lines":notes,"suppressed":indices,"figures":figures.iter().map(|f|&f.id).collect::<Vec<_>>() }));
            if rows.len() % 25 == 0 {
                eprintln!("Caption/callout audit: {} candidate pages", rows.len());
            }
        }
        let report = serde_json::json!({"scanned":doc.pages().len(),"candidate_pages":rows.len(),"captions":captions,"callout_pages":panels,"pages":rows,"unresolved":unresolved});
        std::fs::write(out.join("review.json"), serde_json::to_vec_pretty(&report)?)?;
        assert!(
            unresolved.is_empty(),
            "Unresolved captions/callouts: {unresolved:?}"
        );
        eprintln!("Verified {captions} captions and {panels} callout pages.");
        Ok(())
    }

    #[test]
    #[ignore]
    fn audit_unordered_lists() -> anyhow::Result<()> {
        let path = std::path::PathBuf::from(std::env::var("EASYREAD_QA_PDF")?);
        let pdfium = Pdfium::new(Pdfium::bind_to_library("assets/pdfium/libpdfium.dylib")?);
        let doc = pdfium.load_pdf_from_file(&path, None)?;
        let output = std::path::Path::new("target/pdf-list-qa");
        std::fs::create_dir_all(output)?;
        let mut records = vec![];
        for (number, page) in doc.pages().iter().enumerate() {
            let original = glyphs(&page.text()?);
            let recovered = recover_fragmented_lines(&original).0;
            let lines: Vec<_> = recovered.split(|g| g.ch == '\n').collect();
            let mut candidates = vec![];
            for (index, object) in page.objects().iter().enumerate() {
                if object.as_path_object().is_none() {
                    continue;
                }
                let b = object.bounds()?;
                let (w, h) = (b.width().value, b.height().value);
                if !(1.5..=10.).contains(&w)
                    || !(1.5..=10.).contains(&h)
                    || w / h < 0.6
                    || w / h > 1.6
                {
                    continue;
                }
                for (line, text) in lines.iter().enumerate() {
                    let Some(first) = text
                        .iter()
                        .find(|g| !g.ch.is_whitespace() && !g.ch.is_control())
                    else {
                        continue;
                    };
                    let gap = first.x - b.right().value;
                    let rise = (b.top().value + b.bottom().value) / 2. - first.y;
                    if (0.0..=24.).contains(&gap) && (-1.0..=10.).contains(&rise) {
                        candidates.push((index, line));
                    }
                }
            }
            let mut omitted = std::collections::HashSet::new();
            for table in table::recover(&page, &recovered) {
                omitted.extend(table.lines);
            }
            // Use the same figure boundary to classify diagram dots explicitly.
            let (config, figures) = crate::pdf::reading_figures(&page, &doc, number + 1)?;
            for figure in &figures {
                for (i, line) in lines.iter().enumerate() {
                    let positions: Vec<_> = line
                        .iter()
                        .filter(|g| !g.ch.is_whitespace() && !g.ch.is_control())
                        .filter_map(|g| {
                            page.points_to_pixels(
                                PdfPoints::new(g.x),
                                PdfPoints::new(g.y + g.size * 0.3),
                                &config,
                            )
                            .ok()
                        })
                        .collect();
                    if !positions.is_empty()
                        && positions.iter().all(|&(x, y)| {
                            x >= figure.bounds[0] as i32
                                && x <= figure.bounds[2] as i32
                                && y >= figure.bounds[1] as i32
                                && y <= figure.bounds[3] as i32
                        })
                    {
                        omitted.insert(i);
                    }
                }
            }
            let lists = list::recover(&page, &recovered, &omitted);
            let restored: std::collections::HashSet<_> = lists
                .iter()
                .flat_map(|l| l.objects.iter().copied())
                .collect();
            let claimed: std::collections::HashSet<_> =
                lists.iter().flat_map(|l| l.lines.iter().copied()).collect();
            let paths: Vec<_> = candidates.iter().map(|&(object, line)| serde_json::json!({"object":object,"line":line,"text":line_text(lines[line]),"status":if restored.contains(&object) {"restored"} else if omitted.contains(&line) {"diagram_or_table"} else {"review"}})).collect();
            let text_markers: Vec<_> = lines.iter().enumerate().filter_map(|(i,l)| {
                let text = line_text(l);
                let trimmed = text.trim_start();
                let c = trimmed.chars().next()?;
                if !"•‣◦▪▫●○■□".contains(c) && !trimmed.starts_with("¡\t") { return None; }
                Some(serde_json::json!({"line":i,"text":text,"status":if claimed.contains(&i) {"restored"} else if omitted.contains(&i) {"diagram_or_table"} else {"review"}}))
            }).collect();
            if !paths.is_empty() || !text_markers.is_empty() || !lists.is_empty() {
                let (raw, html, _, _, _) = reading_html(&page, &doc, &[], number + 1)?;
                assert_eq!(
                    raw,
                    original
                        .iter()
                        .map(|g| g.ch)
                        .collect::<String>()
                        .replace("\r\n", "\n")
                );
                std::fs::write(
                    output.join(format!("page-{}.html", number + 1)),
                    html.unwrap_or_default(),
                )?;
                records.push(serde_json::json!({"page":number+1,"paths":paths,"text_markers":text_markers,"lists":lists.len(),"items":lists.iter().map(|l| l.html.matches("<li>").count()).sum::<usize>()}));
            }
            if (number + 1) % 25 == 0 {
                eprintln!("List audit: {} pages", number + 1);
            }
        }
        std::fs::write(
            output.join("discovery.json"),
            serde_json::to_vec_pretty(
                &serde_json::json!({"scanned":doc.pages().len(),"pages":records}),
            )?,
        )?;
        Ok(())
    }

    /// Independently discover font runs and hanging terms from PDF objects,
    /// then render candidate pages and compare against the persisted book.
    #[test]
    #[ignore]
    fn audit_typography() -> anyhow::Result<()> {
        let path = std::path::PathBuf::from(std::env::var("EASYREAD_QA_PDF")?);
        let paper: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path.parent().unwrap().join("paper.json"))?)?;
        let pdfium = Pdfium::new(Pdfium::bind_to_library("assets/pdfium/libpdfium.dylib")?);
        let doc = pdfium.load_pdf_from_file(&path, None)?;
        let outline = crate::pdf::document_outline(&doc);
        let out = std::path::Path::new("target/pdf-typography-qa");
        std::fs::create_dir_all(out)?;
        let mut records = vec![];
        for (index, page) in doc.pages().iter().enumerate() {
            let number = index + 1;
            let text = page.text()?;
            let mut lines: Vec<Vec<serde_json::Value>> = vec![];
            for object in page.objects().iter() {
                let Some(t) = object.as_text_object() else {
                    continue;
                };
                let value = text.for_object(t);
                if value.trim().is_empty() {
                    continue;
                }
                let font = t.font();
                let name = font.name().to_lowercase();
                let mono =
                    font.is_fixed_pitch() || name.contains("mono") || name.contains("courier");
                let italic =
                    font.is_italic() || name.contains("italic") || name.contains("oblique");
                let bold = name.contains("bold")
                    || font.is_bold_reenforced()
                    || matches!(
                        font.weight().ok(),
                        Some(
                            PdfFontWeight::Weight600
                                | PdfFontWeight::Weight700Bold
                                | PdfFontWeight::Weight800
                                | PdfFontWeight::Weight900
                        )
                    );
                let (x, y) = t.get_translation();
                let size = t.scaled_font_size().value;
                let r = serde_json::json!({"text":value,"x":x.value,"y":y.value,"size":size,"mono":mono,"italic":italic,"bold":bold});
                if lines.last().and_then(|l| l.last()).is_none_or(|prev| {
                    (prev["y"].as_f64().unwrap() - y.value as f64).abs() > size as f64 * 0.2
                        || (x.value as f64) < prev["x"].as_f64().unwrap()
                }) {
                    lines.push(vec![]);
                }
                lines.last_mut().unwrap().push(r);
            }
            let mut inline = vec![];
            let mut terms = vec![];
            for (i, line) in lines.iter().enumerate() {
                let ordinary = line
                    .iter()
                    .any(|r| r["mono"] == false && r["bold"] == false && r["italic"] == false);
                if ordinary {
                    for (j, r) in line.iter().enumerate().filter(|(_, r)| {
                        r["mono"] == true || r["bold"] == true || r["italic"] == true
                    }) {
                        let mut r = r.clone();
                        r["prefix"] = line[..j]
                            .iter()
                            .map(|v| v["text"].as_str().unwrap())
                            .collect::<String>()
                            .into();
                        r["suffix"] = line[j + 1..]
                            .iter()
                            .map(|v| v["text"].as_str().unwrap())
                            .collect::<String>()
                            .into();
                        inline.push(r);
                    }
                }
                let Some(next) = lines.get(i + 1).and_then(|l| l.first()) else {
                    continue;
                };
                let first = &line[0];
                let label = line
                    .iter()
                    .map(|r| r["text"].as_str().unwrap())
                    .collect::<String>();
                let size = first["size"].as_f64().unwrap();
                let indent = (next["x"].as_f64().unwrap() - first["x"].as_f64().unwrap()) / size;
                let gap = (first["y"].as_f64().unwrap() - next["y"].as_f64().unwrap()) / size;
                if label.chars().count() < 100
                    && line.iter().all(|r| r["mono"] == true || r["bold"] == true)
                    && next["mono"] == false
                    && (0.5..=6.).contains(&indent)
                    && (0.4..=3.5).contains(&gap)
                    && (next["size"].as_f64().unwrap() - size).abs() < size * 0.15
                {
                    terms.push(serde_json::json!({"text":label,"indent":indent,"x":first["x"],"y":first["y"],"size":size}));
                }
            }
            if !inline.is_empty() || !terms.is_empty() {
                let headings: Vec<_> = outline
                    .iter()
                    .filter(|h| h.page == number)
                    .cloned()
                    .collect();
                let reuse = std::env::var_os("EASYREAD_QA_REUSE_HTML").is_some();
                let old_record = std::fs::read(out.join(format!("page-{number}.json")))
                    .ok()
                    .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok());
                let (raw, html, _, figures, _) =
                    if reuse && out.join(format!("page-{number}.html")).exists() {
                        (String::new(), None, vec![], vec![], vec![])
                    } else {
                        reading_html(&page, &doc, &headings, number)?
                    };
                let stored = paper["blocks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|b| b["page"] == number && b["source_kind"] == "pdf-text")
                    .unwrap()["en"]
                    .as_str()
                    .unwrap();
                if !reuse {
                    assert_eq!(raw, stored, "Persisted source page {number}");
                }
                if !reuse {
                    std::fs::write(
                        out.join(format!("page-{number}.html")),
                        html.unwrap_or_else(|| {
                            crate::annotations::html(&crate::model::pdf_prose(stored))
                        }),
                    )?;
                }
                let config = PdfRenderConfig::new()
                    .set_target_width(1200)
                    .set_maximum_height(4000);
                let bounds = crate::pdf::reading_visual_bounds(&page)?;
                for r in &mut inline {
                    let (x, y) = page.points_to_pixels(
                        PdfPoints::new(r["x"].as_f64().unwrap() as f32),
                        PdfPoints::new(
                            (r["y"].as_f64().unwrap() + r["size"].as_f64().unwrap() * 0.3) as f32,
                        ),
                        &config,
                    )?;
                    r["visual"] = bounds
                        .iter()
                        .any(|b| {
                            x >= b[0] as i32
                                && x <= b[2] as i32
                                && y >= b[1] as i32
                                && y <= b[3] as i32
                        })
                        .into();
                }
                let ids = if reuse {
                    old_record.unwrap()["figures"].clone()
                } else {
                    serde_json::json!(figures.iter().map(|f| &f.id).collect::<Vec<_>>())
                };
                let record =
                    serde_json::json!({"page":number,"inline":inline,"terms":terms,"figures":ids});
                std::fs::write(
                    out.join(format!("page-{number}.json")),
                    serde_json::to_vec_pretty(&record)?,
                )?;
                records.push(record);
            }
            if number % 25 == 0 {
                eprintln!("Typography audit: {number} pages");
            }
        }
        std::fs::write(
            out.join("discovery.json"),
            serde_json::to_vec_pretty(
                &serde_json::json!({"scanned":doc.pages().len(),"pages":records}),
            )?,
        )?;
        Ok(())
    }

    /// Broader discovery than the renderer: include lowercase and tightly
    /// spaced italic blocks, then review them against the rendered audit.
    #[test]
    #[ignore]
    fn audit_italic_blocks() -> anyhow::Result<()> {
        let path = std::path::PathBuf::from(std::env::var("EASYREAD_QA_PDF")?);
        let pdfium = Pdfium::new(Pdfium::bind_to_library("assets/pdfium/libpdfium.dylib")?);
        let doc = pdfium.load_pdf_from_file(&path, None)?;
        let mut records = Vec::new();
        let mut backgrounds = Vec::new();
        let mut changed_paragraph_pages = Vec::new();
        let mut continued_caption_pages = Vec::new();
        for (number, page) in doc.pages().iter().enumerate() {
            let decorations = crate::pdf::page_background_indices(&page);
            if !decorations.is_empty() {
                backgrounds.push(serde_json::json!({"page":number + 1,"objects":decorations}));
            }
            let recovered = recover_fragmented_lines(&glyphs(&page.text()?)).0;
            let lines: Vec<_> = recovered.split(|g| g.ch == '\n').collect();
            if (0..lines.len()).any(|i| leading_caption(&lines, i)) {
                continued_caption_pages.push(number + 1);
            }
            let current = paragraph_breaks(&lines, &Default::default());
            let mut roman_only = recovered.clone();
            for g in &mut roman_only {
                if g.italic {
                    g.mono = true;
                }
            }
            let old_lines: Vec<_> = roman_only.split(|g| g.ch == '\n').collect();
            if current != paragraph_breaks(&old_lines, &Default::default()) {
                changed_paragraph_pages.push(number + 1);
            }
            let lines: Vec<_> = recovered.split(|g| g.ch == '\n').collect();
            let mut i = 0;
            while i < lines.len() {
                let start = i;
                let italic = |line: &[Glyph]| {
                    line.iter().any(|g| g.ch.is_alphabetic())
                        && line
                            .iter()
                            .filter(|g| !g.ch.is_whitespace() && !g.ch.is_control())
                            .all(|g| g.italic && !g.mono && !g.bold)
                };
                while i < lines.len() && italic(lines[i]) {
                    i += 1;
                }
                if start == i {
                    i += 1;
                    continue;
                }
                let title = lines[start..i]
                    .iter()
                    .map(|line| line_text(line))
                    .collect::<Vec<_>>()
                    .join(" ");
                let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
                let first = lines[start].iter().find(|g| !g.ch.is_whitespace()).unwrap();
                let last = lines[i - 1].iter().find(|g| !g.ch.is_whitespace()).unwrap();
                let next = lines
                    .get(i)
                    .and_then(|line| line.iter().find(|g| !g.ch.is_whitespace()));
                if let Some(next) = next
                    && !next.italic
                    && !next.mono
                    && !next.bold
                    && title.chars().count() <= 100
                    && title.split_whitespace().count() <= 14
                    && !title.ends_with(['.', '?', '!', ';', ':'])
                    && !["Figure ", "Table "]
                        .iter()
                        .any(|prefix| title.starts_with(prefix))
                    && (next.x - first.x >= first.size * 0.5 || last.y - next.y >= first.size * 1.4)
                {
                    records.push(serde_json::json!({"page":number + 1,"text":title,"gap_in_font_sizes":(last.y-next.y)/first.size,"indent_in_font_sizes":(next.x-first.x)/first.size}));
                }
            }
        }
        std::fs::create_dir_all("target/pdf-structure-qa")?;
        std::fs::write(
            "target/pdf-structure-qa/italic-discovery.json",
            serde_json::to_vec_pretty(&records)?,
        )?;
        std::fs::write(
            "target/pdf-structure-qa/background-discovery.json",
            serde_json::to_vec_pretty(&backgrounds)?,
        )?;
        std::fs::write(
            "target/pdf-structure-qa/changed-paragraph-pages.json",
            serde_json::to_vec_pretty(&changed_paragraph_pages)?,
        )?;
        std::fs::write(
            "target/pdf-structure-qa/continued-caption-pages.json",
            serde_json::to_vec_pretty(&continued_caption_pages)?,
        )?;
        eprintln!("Discovered {} independent italic blocks", records.len());
        Ok(())
    }

    #[test]
    #[ignore = "requires EASYREAD_QA_PDF pointing to Attention Is All You Need"]
    fn embeddings_softmax_stays_one_paragraph_in_real_pdf() -> anyhow::Result<()> {
        let path = std::env::var("EASYREAD_QA_PDF")?;
        let pdfium = Pdfium::new(Pdfium::bind_to_library("assets/pdfium/libpdfium.dylib")?);
        let doc = pdfium.load_pdf_from_file(&path, None)?;
        let page = doc.pages().get(4)?;
        let gs = recover_fragmented_lines(&glyphs(&page.text()?)).0;
        let lines: Vec<_> = gs.split(|g| g.ch == '\n').collect();
        let breaks = paragraph_breaks(&lines, &Default::default());
        let tokens = lines
            .iter()
            .position(|line| line_text(line).starts_with("tokens and output tokens"))
            .unwrap();
        assert!(!breaks[tokens]);
        let headings: Vec<_> = crate::pdf::document_outline(&doc)
            .into_iter()
            .filter(|heading| heading.page == 5)
            .collect();
        let (_, html, _, _, _) = reading_html(&page, &doc, &headings, 5)?;
        let html = html.expect("Expected rich reading presentation");
        let start = html.find("Similarly to other sequence").unwrap();
        let end = html[start..].find("</p>").unwrap() + start;
        assert!(
            html[start..end].contains("tokens and output tokens"),
            "{}",
            &html[start..]
        );
        assert!(
            !crate::annotations::plain_text(&html)
                .trim_end()
                .ends_with("\n5")
        );
        assert!(html.contains(&crate::formula::native_html(
            "\\sqrt{d_{\\mathrm{model}}}",
            false
        )));
        Ok(())
    }

    #[test]
    #[ignore = "requires EASYREAD_QA_PDF pointing to Attention Is All You Need"]
    fn attention_comparison_table_recovers_rows_and_columns() -> anyhow::Result<()> {
        let path = std::env::var("EASYREAD_QA_PDF")?;
        let pdfium = Pdfium::new(Pdfium::bind_to_library("assets/pdfium/libpdfium.dylib")?);
        let doc = pdfium.load_pdf_from_file(&path, None)?;
        let page = doc.pages().get(5)?;
        let (_, html, _, _, _) = reading_html(&page, &doc, &[], 6)?;
        let html = html.unwrap();
        assert_eq!(html.matches("<table>").count(), 1, "{html}");
        let table_start = html.find("<table>").unwrap();
        let table_end = html.find("</table>").unwrap();
        let table = &html[table_start..table_end];
        assert_eq!(table.matches("<tr>").count(), 5, "{table}");
        assert_eq!(table.matches("<th ").count(), 4, "{table}");
        assert_eq!(table.matches("<td ").count(), 16, "{table}");
        assert_eq!(table.matches("data-vellio-math=").count(), 12, "{table}");
        for tex in ["O(n^{2}\\cdot d)", "O(log_{k}(n))", "O(n/r)"] {
            assert!(
                table.contains(&crate::formula::native_html(tex, false)),
                "{tex}: {table}"
            );
        }
        assert_eq!(
            html[..table_start].matches("<p>").count(),
            1,
            "{}",
            &html[..table_start]
        );
        assert!(
            !crate::annotations::plain_text(&html)
                .trim_end()
                .ends_with("\n6")
        );
        Ok(())
    }

    #[test]
    fn unrecovered_table_keeps_positioned_text_without_a_screenshot_or_guessed_cells() {
        let gs = test_lines(&[
            ("Model         Score       Cost", 10., false, false),
            ("ByteNet       23.75", 10., false, false),
            ("Transformer   27.3        3.3", 10., false, false),
        ]);
        let fallback = table::text_fallback(&gs, [72., 0., 540., 800.]).unwrap();
        assert_eq!(fallback.lines, vec![0, 1, 2]);
        assert!(
            fallback.html.contains("data-vellio-table-unrecovered=")
                && fallback.html.contains("<pre><code>")
        );
        assert!(!fallback.html.contains("<img") && !fallback.html.contains("<table"));
        let text = crate::annotations::plain_text(&fallback.html);
        assert!(text.contains("ByteNet") && text.contains("23.75") && text.contains("Transformer"));
        assert!(
            fallback.html.contains("ByteNet  "),
            "Physical column gaps must not collapse into prose: {}",
            fallback.html
        );
        assert!(
            table::text_fallback(&gs, [72., 0., 170., 800.]).is_none(),
            "Mixed extraction records must not lose neighboring text"
        );
    }

    #[test]
    #[ignore = "requires EASYREAD_QA_PDF pointing to Attention Is All You Need"]
    fn attention_all_tables_keep_structured_cells_without_images() -> anyhow::Result<()> {
        let path = std::env::var("EASYREAD_QA_PDF")?;
        let before = std::fs::read(&path)?;
        let pdfium = Pdfium::new(Pdfium::bind_to_library("assets/pdfium/libpdfium.dylib")?);
        let doc = pdfium.load_pdf_from_file(&path, None)?;
        let outline = crate::pdf::document_outline(&doc);
        let out = std::path::Path::new("target/attention-tables-qa");
        std::fs::create_dir_all(out)?;
        for (n, number, body) in [
            (6, 1, "Positional Encoding"),
            (8, 2, "Residual Dropout"),
            (9, 3, "We present these results in Table 3"),
            (10, 4, "Our results in Table 4 show"),
        ] {
            let page = doc.pages().get(n - 1)?;
            let gs = recover_fragmented_lines(&glyphs(&page.text()?)).0;
            let regions = table::regions(&page, &gs);
            eprintln!("table {number}, page {n}: regions {regions:?}");
            let dump: Vec<_> = gs.iter().filter(|g| !g.ch.is_whitespace() && !g.ch.is_control() && regions.iter().any(|b| table::contains(*b, [g.x,g.y,g.right,g.y]))).map(|g| serde_json::json!({"ch":g.ch,"x":g.x,"right":g.right,"y":g.y,"size":g.size,"bold":g.bold,"italic":g.italic})).collect();
            std::fs::write(
                out.join(format!("table-{number}-glyphs.json")),
                serde_json::to_vec_pretty(&dump)?,
            )?;
            assert_eq!(
                regions.len(),
                1,
                "Table {number} must be detected independently of cell parsing"
            );
            let headings: Vec<_> = outline
                .iter()
                .filter(|h| h.page == n as usize)
                .cloned()
                .collect();
            let (source, html, _, previews, _) = reading_html(&page, &doc, &headings, n as usize)?;
            let html = html.unwrap();
            assert!(source.contains(&format!("Table {number}:")));
            assert!(
                html.contains(&format!("Table {number}:")),
                "Caption lost: {number}"
            );
            assert!(
                crate::annotations::plain_text(&html).contains(body),
                "Prose lost: {number}"
            );
            assert_eq!(html.matches("<table").count(), 1, "{number}: {html}");
            assert!(
                !html.contains("<img ") && previews.is_empty(),
                "No table images: {number}"
            );
            if number == 2 || number == 3 {
                let start = html.find("<table").unwrap();
                let end = html.find("</table>").unwrap() + 8;
                let grid = crate::typst_table::TableGrid::from_html(&html[start..end])?;
                let cell = |r, c| {
                    grid.cells
                        .iter()
                        .find(|x| x.row == r && x.col == c)
                        .unwrap()
                };
                if number == 2 {
                    assert_eq!((grid.rows, grid.columns.len()), (12, 5));
                    assert_eq!(cell(0, 0).rowspan, 2);
                    assert_eq!((cell(0, 1).text.as_str(), cell(0, 1).colspan), ("BLEU", 2));
                    assert_eq!(cell(2, 1).text, "23.75");
                    assert!(cell(2, 2).text.is_empty() && cell(2, 3).text.is_empty());
                    assert_eq!(cell(10, 3).colspan, 2);
                    assert!(cell(10, 3).html.contains("3.3\\cdot 10^{18}"));
                    assert!(cell(10, 3).html.contains("<strong>"));
                } else {
                    assert_eq!((grid.rows, grid.columns.len()), (21, 13));
                    assert_eq!(
                        grid.vlines,
                        vec![1, 10],
                        "Segmented vertical separators must survive"
                    );
                    for (r, label, span) in
                        [(2, "(A)", 4), (6, "(B)", 2), (8, "(C)", 7), (15, "(D)", 4)]
                    {
                        assert_eq!(
                            (cell(r, 0).text.as_str(), cell(r, 0).rowspan),
                            (label, span)
                        );
                    }
                    assert_eq!(cell(11, 2).text, "256");
                    assert_eq!(cell(19, 1).colspan, 9);
                    assert_eq!(cell(20, 10).text, "4.33");
                    assert_eq!(cell(20, 12).text, "213");
                    assert!(cell(2, 1).text.is_empty());
                }
                let (_, frame) = grid.layout()?;
                assert!(frame.is_valid() && frame.paths.len() > 100);
                std::fs::write(
                    out.join(format!("table-{number}-grid.json")),
                    serde_json::to_vec_pretty(&grid)?,
                )?;
                std::fs::write(out.join(format!("table-{number}-typst.svg")), grid.svg()?)?;
                std::fs::write(
                    out.join(format!("table-{number}.typ")),
                    grid.typst_source()?,
                )?;
            }
            std::fs::write(out.join(format!("table-{number}.html")), &html)?;
        }
        assert_eq!(
            std::fs::read(&path)?,
            before,
            "Source PDF must remain unchanged"
        );
        Ok(())
    }

    #[test]
    fn fragmented_baselines_recover_words_without_merging_real_lines_or_code() {
        let caption = "Figure 1. A caption";
        let mut original = test_lines(&[(caption, 12., false, false)]);
        for g in &mut original {
            g.italic = true;
        }
        let mut broken = Vec::new();
        for g in original.iter().filter(|g| !g.ch.is_whitespace()) {
            broken.push(g.clone());
            let mut newline = g.clone();
            newline.ch = '\n';
            broken.push(newline);
        }
        let (recovered, changed) = recover_fragmented_lines(&broken);
        assert!(changed);
        assert_eq!(line_text(&recovered), caption);
        assert!(
            render(&recovered, &[])
                .unwrap()
                .contains(&format!("<em>{caption}</em>"))
        );
        let mut code = broken.clone();
        for g in &mut code {
            g.mono = true;
        }
        assert!(!recover_fragmented_lines(&code).1);
        let separate = test_lines(&[
            ("A", 12., false, false),
            ("B", 12., false, false),
            ("C", 12., false, false),
            ("D", 12., false, false),
        ]);
        assert!(!recover_fragmented_lines(&separate).1);
        let mut columns = broken.clone();
        for (i, g) in columns.iter_mut().enumerate() {
            g.x += (i / 2) as f32 * 100.;
            g.advance_right += (i / 2) as f32 * 100.;
        }
        assert!(!recover_fragmented_lines(&columns).1);
    }

    #[test]
    fn raster_insertion_requires_glyph_size_baseline_and_an_empty_gap() {
        let mut glyphs = test_lines(&[("AB", 12., false, false)]);
        glyphs[1].x = 96.;
        glyphs[1].right = 102.;
        assert_eq!(insertion(&glyphs, [82., 738., 94., 750.]), Some((1, false)));
        assert_eq!(
            insertion(&glyphs, [72., 738., 84., 750.]),
            None,
            "overlaps text"
        );
        assert_eq!(
            insertion(&glyphs, [82., 720., 94., 732.]),
            None,
            "different baseline"
        );
        assert_eq!(insertion(&glyphs, [82., 738., 94., 768.]), None, "too tall");
        assert_eq!(
            insertion(&glyphs, [130., 738., 142., 750.]),
            None,
            "isolated icon"
        );
        assert_eq!(
            insertion(&glyphs, [110., 738., 122., 750.]),
            None,
            "detached margin icon"
        );
    }

    #[test]
    fn pdf_prose_keeps_literal_markdown_punctuation_and_source_fonts() {
        let source = "Use __init__ and *args with [name](value), `literal` and <tag>.";
        let mut gs = test_lines(&[(source, 12., false, false)]);
        for g in gs.iter_mut().take(3) {
            g.bold = true;
        }
        let html = render(&gs, &[]).unwrap();
        assert_eq!(crate::annotations::plain_text(&html).trim(), source);
        assert!(html.contains("<strong>Use</strong>"));
        assert!(!html.contains("<a "));
    }

    #[test]
    fn prose_heading_and_caption_images_survive_reflow_and_repeated_letters() {
        let mut glyphs = test_lines(&[
            ("You read the introduction.", 12., false, false),
            ("Celebration", 12., true, false),
            ("Congratulations! You did it.", 12., false, false),
            ("Figure 1. You did it.", 10., false, false),
        ]);
        let index = |text: &str| {
            glyphs
                .iter()
                .map(|g| g.ch)
                .collect::<String>()
                .find(text)
                .unwrap()
        };
        let heading = index("Celebration");
        let prose = index("You did it.");
        let caption = index("Figure 1.") + "Figure 1. ".len();
        for g in &mut glyphs[caption - "Figure 1. ".len()..] {
            g.italic = true;
        }
        let image = "<img src=\"data:image/png;base64,AA==\" alt=\"\">";
        for i in [heading, prose, caption] {
            glyphs[i].image_before = Some(image.into());
        }
        let html = render(&glyphs, &[]).unwrap();
        assert_eq!(html.matches("<img ").count(), 3);
        assert!(html.contains("<h3><span><img"), "{html}");
        assert!(html.contains("Congratulations! <span><img"), "{html}");
        assert!(html.contains("Figure 1. <span><img"), "{html}");
        assert!(html.contains("You read the introduction."));
    }

    #[test]
    fn italic_labels_preserve_nested_boundaries_wraps_and_annotations() {
        for marker in ["1.", "2)", "(3)", "A)", "b.", "(c)", "IV.", "viii)"] {
            let mut glyphs = test_lines(&[
                ("The following behaviors:", 12., false, false),
                (
                    &format!("{marker} You hand the agent the file"),
                    12.,
                    false,
                    false,
                ),
                ("(for example, a spreadsheet)", 12., false, false),
                ("A simple tool retrieves the exact file.", 12., false, false),
                (
                    "The body continues on the next physical line.",
                    12.,
                    false,
                    false,
                ),
            ]);
            for (i, line) in glyphs.split_mut(|g| g.ch == '\n').enumerate() {
                for g in line {
                    g.italic = i == 1 || i == 2;
                    g.y = 740. - i as f32 * 15.;
                }
            }
            let html = render(&glyphs, &[]).unwrap();
            assert!(html.contains(&format!("</p><p><em>{marker} You hand the agent the file (for example, a spreadsheet)</em></p>\n<p><span>A simple tool")), "{html}");
            assert!(html.contains("exact file. The body continues"));
            let marked = crate::annotations::decorate_interactive(
                &html,
                &[
                    serde_json::json!({"id":"label-note", "quote":"file (for example", "kind":"highlight", "color":"pink"}),
                ],
            );
            assert!(marked.contains("label-note"));
            let source: String = glyphs.iter().map(|g| g.ch).collect();
            let normalize = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
            assert_eq!(
                normalize(&crate::annotations::plain_text(&marked)),
                normalize(&source)
            );
        }
    }

    #[test]
    fn italic_label_detection_requires_a_whole_label_and_physical_boundary() {
        for case in [
            "roman",
            "inline",
            "sentence",
            "version",
            "mixed",
            "code",
            "size",
            "gap",
            "blank",
            "lowercase",
            "unnumbered",
        ] {
            let label = match case {
                "sentence" => "1. This is a full sentence.",
                "version" => "1.2 Release notes",
                "lowercase" => "1. retrieve the data",
                "unnumbered" => "An italic quotation",
                _ => "1. Data retrieval",
            };
            let mut glyphs = test_lines(&[
                ("Prior body line", 12., false, false),
                (label, 12., false, false),
                ("continued label", 12., false, false),
                ("Ordinary body text.", 12., false, false),
            ]);
            for (i, line) in glyphs.split_mut(|g| g.ch == '\n').enumerate() {
                for (j, g) in line.iter_mut().enumerate() {
                    g.italic = (i == 1 || i == 2) && case != "roman";
                    g.y = 740. - i as f32 * 15.;
                    if i == 1 {
                        if case == "inline" {
                            g.y = 740.;
                        }
                        if case == "mixed" && j > 7 {
                            g.italic = false;
                        }
                        if case == "code" {
                            g.mono = true;
                        }
                    }
                    if i == 2 {
                        if case == "size" {
                            g.size = 10.;
                        }
                        if case == "gap" {
                            g.y -= 40.;
                        }
                        if case == "blank" {
                            g.ch = ' ';
                        }
                    }
                }
            }
            let lines: Vec<_> = glyphs.split(|g| g.ch == '\n').collect();
            assert!(italic_label(&lines, 1).is_none(), "{case}");
        }
    }

    #[test]
    fn italic_labels_support_roman_markers_indentation_and_escaped_text() {
        let mut glyphs = test_lines(&[
            ("  A) Tools & <Agents>", 12., false, false),
            ("The ordinary body follows.", 12., false, false),
        ]);
        for g in glyphs.iter_mut().take_while(|g| g.ch != '\n').skip(5) {
            g.italic = true;
        }
        let html = render(&glyphs, &[]).unwrap();
        assert!(html.starts_with("<p><em>A) Tools &amp; &lt;Agents&gt;</em></p>\n<p>"));
    }

    #[test]
    fn wrapped_caption_stays_one_paragraph_before_body_without_other_page_features() {
        let mut glyphs = test_lines(&[
            ("Figure 10-2. Coding agents solve complex", 9., false, false),
            ("problems", 9., false, false),
            (
                "The first shows a common pattern of asking LLMs.",
                12.,
                false,
                false,
            ),
        ]);
        for (i, line) in glyphs.split_mut(|g| g.ch == '\n').enumerate() {
            for g in line {
                g.y = 178.2 - i as f32 * 10.8;
                g.italic = i < 2;
            }
        }
        let source: String = glyphs.iter().map(|g| g.ch).collect();
        let html = render(&glyphs, &[]).unwrap();
        assert!(
            html.starts_with(
                "<p style=\"text-align:center\"><em>Figure 10-2. Coding agents solve complex problems</em></p><p>"
            ),
            "{html}"
        );
        let marked = crate::annotations::decorate_interactive(
            &html,
            &[
                serde_json::json!({"id":"caption-note", "quote":"complex problems", "kind":"highlight", "style":"underline", "color":"pink"}),
            ],
        );
        assert!(marked.contains("caption-note"));
        assert!(marked.contains("text-align:center"));
        assert!(marked.contains("text-decoration-color:#cc413a"));
        let normalize = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        assert_eq!(
            normalize(&crate::annotations::plain_text(&marked)),
            normalize(&source)
        );
    }

    #[test]
    fn caption_continuations_stop_at_style_spacing_and_block_boundaries() {
        for case in [
            "roman", "size", "gap", "blank", "caption", "code", "image", "omitted",
        ] {
            let next = if case == "caption" {
                "Table 2. Another caption"
            } else {
                "separate text"
            };
            let mut glyphs = test_lines(&[
                ("Figure 1. A caption", 10., false, false),
                (next, 10., false, false),
            ]);
            for (i, line) in glyphs.split_mut(|g| g.ch == '\n').enumerate() {
                for g in line {
                    g.italic = !(i == 1 && case == "roman");
                    g.y = 740. - i as f32 * if case == "gap" { 30. } else { 12. };
                    if i == 1 {
                        if case == "size" {
                            g.size = 12.;
                        }
                        if case == "code" {
                            g.mono = true;
                        }
                        if case == "blank" {
                            g.ch = ' ';
                        }
                    }
                }
            }
            let figures = if case == "image" {
                vec![(1, "<p><img alt=\"\"></p>".into())]
            } else {
                vec![]
            };
            let omitted = if case == "omitted" {
                [1].into_iter().collect()
            } else {
                Default::default()
            };
            let html = render_page(&glyphs, &[], &figures, &omitted).unwrap();
            assert!(
                html.starts_with("<p style=\"text-align:center\"><em>Figure 1. A caption</em></p>"),
                "{case}: {html}"
            );
        }
    }

    fn test_lines(lines: &[(&str, f32, bool, bool)]) -> Vec<Glyph> {
        lines
            .iter()
            .enumerate()
            .flat_map(|(line, (text, size, bold, mono))| {
                text.chars()
                    .chain(std::iter::once('\n'))
                    .enumerate()
                    .map(move |(i, ch)| Glyph {
                        ch,
                        mono: *mono,
                        x: 72. + i as f32 * 6.,
                        right: 78. + i as f32 * 6.,
                        advance_right: 78. + i as f32 * 6.,
                        y: 740. - line as f32 * 20.,
                        size: *size,
                        color: 0,
                        italic: false,
                        bold: *bold,
                        image_before: None,
                        image_after: None,
                        link: None,
                    })
            })
            .collect()
    }

    #[test]
    fn section_heading_before_smaller_subheading_stays_out_of_the_previous_paragraph() {
        let glyphs = test_lines(&[
            (
                "Label Smoothing During training we smooth labels.",
                10.,
                false,
                false,
            ),
            ("This improves accuracy and BLEU score.", 10., false, false),
            ("6 Results", 12., true, false),
            ("6.1 Machine Translation", 10., true, false),
            (
                "On the translation task the model improves.",
                10.,
                false,
                false,
            ),
        ]);
        let source: String = glyphs.iter().map(|g| g.ch).collect();
        let html = render(&glyphs, &[]).unwrap();
        let heading = html.find("<h3>6 Results</h3>").expect(&html);
        assert!(html[..heading].ends_with("</p>"), "{html}");
        assert!(
            html[heading..]
                .starts_with("<h3>6 Results</h3>\n<h3>6.1 Machine Translation</h3>\n<p>"),
            "{html}"
        );
        assert_eq!(html.matches("<p>").count(), 2, "{html}");
        assert_eq!(glyphs.iter().map(|g| g.ch).collect::<String>(), source);
        // A smaller bold prefix followed by normal prose is not a subheading.
        let mut mixed = glyphs.clone();
        let line = mixed.split_mut(|g| g.ch == '\n').nth(3).unwrap();
        for g in &mut line[4..] {
            g.bold = false;
        }
        assert!(
            typographic_heading(&mixed.split(|g| g.ch == '\n').collect::<Vec<_>>(), 2).is_none()
        );
    }

    #[test]
    fn full_size_raised_reference_labels_preserve_items_and_wrapped_body() {
        let mut glyphs = test_lines(&[
            (
                "20 Zhang, Susan et al. 2022. OPT: Open Pre-trained Transformer",
                9.,
                false,
                false,
            ),
            ("Language Models, arXiv, 2205.01068.", 9., false, false),
            (
                "21 Bai, Jinze et al. 2023. Qwen-VL: A Frontier Vision-Language Model.",
                9.,
                false,
                false,
            ),
            (
                "22 Alayrac, Jean-Baptiste et al. 2022. Flamingo: A Visual Language Model",
                9.,
                false,
                false,
            ),
            (
                "for Few-Shot Learning, Advances in Neural Information Processing Systems.",
                9.,
                false,
                false,
            ),
        ]);
        for line in glyphs.split_mut(|g| g.ch == '\n') {
            if line.first().is_some_and(|g| g.ch == '2') {
                for g in &mut line[..2] {
                    g.y += 3.6;
                    g.link = Some("easyread-pdf:1:100".into());
                }
            }
        }
        let source: String = glyphs.iter().map(|g| g.ch).collect();
        let html = render(&glyphs, &[]).unwrap();
        assert_eq!(html.matches("<p>").count(), 3, "{html}");
        for number in [20, 21, 22] {
            assert!(html.contains(&format!("<sup>{number}</sup>")), "{html}");
        }
        assert!(!html.contains("20<sup>22"), "{html}");
        assert!(html.contains("Transformer Language Models"), "{html}");
        assert!(html.contains("Model for Few-Shot"), "{html}");
        let normalize = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        let marked = crate::annotations::decorate_interactive(
            &html,
            &[serde_json::json!({
                "id":"reference-note", "quote":"22 Alayrac", "prefix":"Model.\n", "color":"green"
            })],
        );
        assert_eq!(
            normalize(&crate::annotations::plain_text(&marked)),
            normalize(&source)
        );
        assert!(marked.contains("<sup>"));
        assert!(marked.contains("data-easyread-note=\"reference-note\""));
    }

    #[test]
    fn same_size_inline_marker_does_not_start_a_paragraph_or_use_other_line_baselines() {
        let mut glyphs = test_lines(&[
            ("Body with reference 12 continues.", 9., false, false),
            ("20 Ordinary label and year 2022.", 9., false, false),
        ]);
        let marker = glyphs.iter().position(|g| g.ch == '1').unwrap();
        for g in &mut glyphs[marker..marker + 2] {
            g.y += 3.6;
        }
        let html = render(&glyphs, &[]).unwrap();
        assert!(html.contains("<sup>12</sup> continues."), "{html}");
        assert!(!html.contains("<sup>20"), "{html}");
        assert!(!html.contains("<sup>2022"), "{html}");
        let mut plain = test_lines(&[
            ("20 A normal label.", 9., false, false),
            ("A neighbouring line.", 9., false, false),
        ]);
        for g in plain.iter_mut().skip(19) {
            g.y = 736.4;
        }
        assert!(render(&plain, &[]).is_none());
    }

    #[test]
    fn bold_unnumbered_title_and_raised_marker_keep_prose_boundaries() {
        let mut glyphs = test_lines(&[
            ("Let’s find out!", 12., false, false),
            ("ViViT", 12., true, false),
            ("An early example is called (ViViT).", 12., false, false),
            ("8 This method uses Transformer models.", 12., false, false),
            (
                "8 Footnote text stays on its own baseline.",
                9.,
                false,
                false,
            ),
        ]);
        let marker = glyphs.iter().position(|g| g.ch == '8').unwrap();
        let previous = glyphs[marker - 2].clone();
        glyphs[marker].size = 9.;
        glyphs[marker].y = previous.y + 4.2;
        glyphs[marker].x = previous.x + 6.;
        glyphs[marker].link = Some("https://example.org/reference".into());
        let html = render(&glyphs, &[]).unwrap();
        assert!(
            html.contains("</p><h3>ViViT</h3>\n<p><span>An early"),
            "{html}"
        );
        assert!(html.contains("(ViViT). <a href="), "{html}");
        assert!(
            html.contains("<sup>8</sup></span></a> This method"),
            "{html}"
        );
        assert_eq!(html.matches("<sup>").count(), 1);
        let marked = crate::annotations::decorate_interactive(
            &html,
            &[serde_json::json!({
                "id":"sup-note", "quote":"8 This method", "kind":"underline", "color":"blue"
            })],
        );
        assert!(marked.contains("<sup>"));
        assert_eq!(
            crate::annotations::plain_text(&marked),
            crate::annotations::plain_text(&html)
        );
    }

    #[test]
    fn multi_reference_superscript_recovers_the_entire_raised_run_across_links() {
        let mut glyphs = test_lines(&[
            ("There are three connectors:", 12., false, false),
            ("12,13,123", 9., false, false),
            ("Following body.", 12., false, false),
            ("12,13 Normal small footnote text.", 9., false, false),
        ]);
        let marker = glyphs.iter().position(|g| g.ch == '1').unwrap();
        let anchor = glyphs[marker - 2].clone();
        for (offset, g) in glyphs[marker..marker + 9].iter_mut().enumerate() {
            g.x = anchor.x + 3. + offset as f32 * 5.;
            g.y = anchor.y + 4.2;
            g.link = match offset {
                0..=1 => Some("easyread-pdf:4:100".into()),
                3..=4 => Some("easyread-pdf:4:80".into()),
                _ => None,
            };
        }
        assert!(
            superscripts(&glyphs)[marker..marker + 9]
                .iter()
                .all(|script| *script)
        );
        let html = render(&glyphs, &[]).unwrap();
        assert!(
            html.contains("<sup>12</sup></span></a><sup>,</sup><a"),
            "{html}"
        );
        assert!(
            html.contains("<sup>13</sup></span></a><sup>,123</sup>"),
            "{html}"
        );
        assert!(!html.contains("<sup>12,13 Normal"));
        let normalize = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
        let source: String = glyphs.iter().map(|g| g.ch).collect();
        assert_eq!(
            normalize(&crate::annotations::plain_text(&html)),
            normalize(&source)
        );
    }

    #[test]
    fn ordinary_small_numbers_and_inline_bold_are_not_titles_or_superscripts() {
        let mut glyphs = test_lines(&[("There are 8 examples.", 12., false, false)]);
        for g in &mut glyphs[0..5] {
            g.bold = true;
        }
        glyphs[10].size = 9.;
        let html = render(&glyphs, &[]).unwrap();
        assert!(html.contains("<strong>There</strong>"));
        assert!(!html.contains("<h") && !html.contains("<sup>"));
        let html = render(
            &test_lines(&[
                ("Bold sentence.", 12., true, false),
                ("Body", 12., false, false),
            ]),
            &[],
        )
        .unwrap();
        assert!(html.contains("<strong>Bold sentence.</strong>"));
        assert!(!html.contains("<h") && !html.contains("<sup>"));
    }

    #[test]
    fn wrapped_link_rectangle_excludes_surrounding_body_text() {
        let mut glyphs = test_lines(&[
            ("We truncated the prediction. GitHub", 10., false, false),
            (
                "repository). The pass rate gives a score of 1.",
                10.,
                false,
                false,
            ),
        ]);
        let mut start = 0;
        for label in ["GitHub", "repository"] {
            let source: String = glyphs.iter().map(|g| g.ch).collect();
            let index = source[start..].find(label).unwrap() + start;
            for g in &mut glyphs[index..index + label.len()] {
                g.color = 0x8e0012;
            }
            start = index + label.len();
        }
        let rect = PdfRect::new_from_values(718., 72., 750., 400.);
        let selected = link_glyphs(&glyphs, &[rect], true);
        assert_eq!(
            selected.iter().map(|&i| glyphs[i].ch).collect::<String>(),
            "GitHubrepository"
        );
        for index in selected {
            glyphs[index].link = Some("https://example.org/repo".into());
        }
        let html = render(&glyphs, &[]).unwrap();
        assert!(html.contains(">GitHub</span></a> <a href="));
        assert!(html.contains(">repository</span></a>). The pass rate"));
        assert!(!html.contains(">We truncated</span></a>"));
        assert_eq!(
            crate::annotations::plain_text(&html),
            "We truncated the prediction. GitHub repository). The pass rate gives a score of 1."
        );

        for g in &mut glyphs {
            g.color = 0;
        }
        assert!(link_glyphs(&glyphs, &[rect], true).is_empty());
    }

    #[test]
    fn explicit_line_regions_preserve_uncolored_wrapped_links() {
        let glyphs = test_lines(&[
            ("Prose GitHub", 10., false, false),
            ("repository after", 10., false, false),
        ]);
        let regions = [
            PdfRect::new_from_values(738., 108., 750., 144.),
            PdfRect::new_from_values(718., 72., 730., 132.),
        ];
        assert_eq!(
            link_glyphs(&glyphs, &regions, false)
                .iter()
                .map(|&i| glyphs[i].ch)
                .collect::<String>(),
            "GitHubrepository"
        );
        assert_eq!(link_glyphs(&glyphs, &regions[..1], true).len(), 6);
    }

    #[test]
    fn prose_only_links_render_without_code_headings_or_figures() {
        let mut glyphs = test_lines(&[("See Figure 4-1 for details.", 10., false, false)]);
        for glyph in &mut glyphs[4..14] {
            glyph.link = Some("easyread-pdf:1:680".into());
            glyph.color = 0x8e0012;
        }
        let html = render(&glyphs, &[]).unwrap();
        assert!(html.contains("href=\"easyread-pdf:1:680\""));
        assert!(html.contains("color:#8e0012"));
        assert_eq!(
            crate::annotations::plain_text(&html),
            "See Figure 4-1 for details."
        );
        assert!(!html.contains("<pre>"));
    }

    #[test]
    fn heading_lookahead_past_end_of_page_returns_none() {
        let gs = test_lines(&[("Last line", 12., false, false)]);
        let lines: Vec<_> = gs.split(|g| g.ch == '\n').collect();
        let headings = [crate::pdf::OutlineEntry {
            title: "Last line".into(),
            page: 1,
            depth: 0,
        }];
        for start in [lines.len(), lines.len() + 1] {
            assert!(bookmarked_heading(&lines, start, &headings).is_none());
            assert!(typographic_heading(&lines, start).is_none());
            assert!(reading_heading(&lines, start, &headings).is_none());
        }
        assert!(reading_heading(&[], 0, &headings).is_none());
    }

    #[test]
    fn bookmarked_headings_preserve_code_and_escape_title_markup() {
        let glyphs = test_lines(&[
            ("Prior prose.", 10., false, false),
            ("Tools & <Agents>", 16., true, false),
            ("print(\"hello\")", 10., false, true),
            ("Following prose.", 10., false, false),
        ]);
        let headings = [crate::pdf::OutlineEntry {
            page: 1,
            title: "Tools & <Agents>".into(),
            depth: 1,
        }];
        let html = render(&glyphs, &headings).unwrap();
        assert!(html.contains("</p><h2>Tools &amp; &lt;Agents&gt;</h2>\n<pre><code>"));
        assert!(html.contains("</code></pre><p><span>Following prose."));
        assert_eq!(html.matches("<pre>").count(), 1);
    }

    #[test]
    fn title_matching_does_not_consume_body_text_or_monospace_code() {
        let headings = [crate::pdf::OutlineEntry {
            page: 1,
            title: "Building Code Agents".into(),
            depth: 1,
        }];
        for lines in [
            vec![("We discuss Building Code Agents here.", 10., false, false)],
            vec![
                ("Building Code", 16., true, false),
                ("Agents", 10., false, false),
            ],
            vec![
                ("Building Code", 16., true, false),
                ("", 16., true, false),
                ("Agents", 16., true, false),
            ],
        ] {
            let html = render(&test_lines(&lines), &headings).unwrap_or_default();
            assert!(!html.contains("<h"), "{html}");
        }
        let html = render(
            &test_lines(&[("Building Code Agents", 10., false, true)]),
            &headings,
        )
        .unwrap();
        assert!(html.contains("<pre><code>"));
        assert!(!html.contains("<h2>"));
    }

    #[test]
    fn reconstructs_indentation_blank_lines_source_colors_and_prose_boundaries() {
        let mut glyphs = vec![];
        for (text, mono, x, y) in [
            ("Chapter 4. Memory", false, 72., 754.),
            ("Explanation", false, 72., 730.),
            ("messages = [", true, 84., 700.),
            ("{\"role\": \"user\"},", true, 108., 688.),
            ("]", true, 84., 676.),
            ("print(messages)", true, 84., 652.),
            ("This gives us:", false, 72., 628.),
        ] {
            for (i, ch) in text.chars().chain(std::iter::once('\n')).enumerate() {
                glyphs.push(Glyph {
                    ch,
                    mono,
                    x: x + i as f32 * 6.,
                    right: x + (i + 1) as f32 * 6.,
                    advance_right: x + (i + 1) as f32 * 6.,
                    y,
                    size: 10.,
                    color: if ch == '"' { 0xff3300 } else { 0x0000aa },
                    italic: false,
                    bold: false,
                    image_before: None,
                    image_after: None,
                    link: None,
                });
            }
        }
        let html = render(&glyphs, &[]).unwrap();
        assert!(html.contains("<h1>Chapter 4. Memory</h1>\n<p><span>Explanation</span></p>"));
        assert_eq!(html.matches("<pre>").count(), 1);
        assert!(html.contains("\n    <span"));
        assert!(html.contains("\n\n<span"));
        assert!(html.contains("color:#ff3300"));
        let plain = crate::annotations::plain_text(&html);
        assert!(plain.contains("messages = [\n    {\"role\": \"user\"},\n]\n\nprint(messages)"));
        assert!(plain.contains("This gives us:"));
        assert_eq!(
            render(
                &[Glyph {
                    ch: 'x',
                    mono: false,
                    x: 0.,
                    right: 6.,
                    advance_right: 6.,
                    y: 0.,
                    size: 10.,
                    color: 0,
                    italic: false,
                    bold: false,
                    image_before: None,
                    image_after: None,
                    link: None
                }],
                &[]
            ),
            None
        );
    }
}
