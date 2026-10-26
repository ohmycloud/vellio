//! Page geometry, clipping, backgrounds and visual-region classification.
use super::*;

pub(super) fn has_visual(object: &PdfPageObject<'_>, depth: usize) -> bool {
    if depth > 16 {
        return false;
    }
    match object {
        PdfPageObject::Image(_) | PdfPageObject::Path(_) | PdfPageObject::Shading(_) => true,
        PdfPageObject::XObjectForm(form) => form.iter().any(|child| has_visual(&child, depth + 1)),
        _ => false,
    }
}
pub(super) fn has_raster(object: &PdfPageObject<'_>, depth: usize) -> bool {
    if depth > 16 {
        return false;
    }
    match object {
        PdfPageObject::Image(_) => true,
        PdfPageObject::XObjectForm(form) => form.iter().any(|child| has_raster(&child, depth + 1)),
        _ => false,
    }
}

pub(super) fn page_media(
    page: &PdfPage<'_>,
    n: usize,
    image: &image::DynamicImage,
    config: &PdfRenderConfig,
    root: &Path,
) -> Result<Media> {
    let mut media = Media {
        blocks: vec![],
        layout: json!({}),
    };
    fs::create_dir_all(root.join("figures"))?;
    let (regions, labels) = page_regions(page, image.width(), image.height(), config)?;
    let captions = caption_pixels(page, config)?;
    for region in captioned_regions(&visual_regions(&regions), &captions) {
        let [left, top, right, bottom] = region.bounds;
        // Ignore isolated rules/decorations, but keep even small raster icons.
        if !region.raster && (right - left < 80 || bottom - top < 40) {
            continue;
        }
        let index = region.indices[0];
        let [left, top, right, bottom] = region.with_labels(&labels);
        let x0 = left.saturating_sub(4);
        let y0 = top.saturating_sub(4);
        let x1 = (right + 4).min(image.width());
        let y1 = (bottom + 4).min(image.height());
        let id = format!("pdf-p{n:03}-img{:03}", index + 1);
        let src = format!("figures/native-{id}.webp");
        image
            .crop_imm(x0, y0, x1 - x0, y1 - y0)
            .save(root.join(&src))?;
        media.blocks.push(json!({"id":id,"type":"figure","page":n,"src":src,"width":x1-x0,"height":y1-y0,"caption_zh":"","caption_en":"","source_kind":"pdf-image"}));
        media.layout[&id] = json!({"page":n,"src":"rust-pdfium-image","box":[x0 as f64/image.width() as f64,y0 as f64/image.height() as f64,x1 as f64/image.width() as f64,y1 as f64/image.height() as f64]});
    }
    Ok(media)
}

pub(super) type VisualRegion = (usize, [u32; 4], bool);

pub(super) fn extends_across_page(bounds: [f32; 4], width: f32, height: f32) -> bool {
    let overflow = |low: f32, high: f32, page: f32| {
        (low < 0. && high > page && high - low > page * 1.05)
            || ((low < 0. || high > page) && high - low > page * 1.5)
    };
    overflow(bounds[1], bounds[3], height) || overflow(bounds[0], bounds[2], width)
}

pub(crate) fn rectangle_bounds(object: &PdfPageObject<'_>) -> Option<[f32; 4]> {
    let path = object.as_path_object()?;
    let segments = path.segments().transform(path.matrix().ok()?);
    let segments: Vec<_> = segments.iter().collect();
    if !(4..=5).contains(&segments.len())
        || segments[0].segment_type() != PdfPathSegmentType::MoveTo
        || !segments.last()?.is_close()
        || segments[1..]
            .iter()
            .any(|s| s.segment_type() != PdfPathSegmentType::LineTo)
    {
        return None;
    }
    let points: Vec<_> = segments
        .iter()
        .map(|s| (s.x().value, s.y().value))
        .collect();
    if (0..points.len()).any(|i| {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        (a.0 - b.0).abs() > 0.01 && (a.1 - b.1).abs() > 0.01
    }) {
        return None;
    }
    let b = object.bounds().ok()?;
    Some([
        b.left().value,
        b.bottom().value,
        b.right().value,
        b.top().value,
    ])
}

pub(super) fn straight_rule(object: &PdfPageObject<'_>) -> bool {
    let Some(path) = object.as_path_object() else {
        return false;
    };
    let Ok(matrix) = path.matrix() else {
        return false;
    };
    let segments = path.segments().transform(matrix);
    let points: Vec<_> = segments.iter().collect();
    points.len() == 2
        && points[0].segment_type() == PdfPathSegmentType::MoveTo
        && points[1].segment_type() == PdfPathSegmentType::LineTo
        && ((points[0].x().value - points[1].x().value).abs() < 0.01
            || (points[0].y().value - points[1].y().value).abs() < 0.01)
}

/// Filled or border-only text panels and inline code highlights retain native
/// text. Exclude their decoration, while interior diagrams and raster objects
/// remain eligible for normal figure recovery.
pub(crate) fn page_background_indices(page: &PdfPage<'_>) -> std::collections::HashSet<usize> {
    let mut indices = text_panels(page).0;
    indices.extend(
        crate::pdf::recovery::text_highlights(page)
            .into_iter()
            .map(|h| h.index),
    );
    indices
}

pub(crate) fn text_panel_bounds(page: &PdfPage<'_>) -> Vec<[f32; 4]> {
    text_panels(page).1
}

pub(super) fn text_panels(page: &PdfPage<'_>) -> (std::collections::HashSet<usize>, Vec<[f32; 4]>) {
    let paths: Vec<_> = page
        .objects()
        .iter()
        .enumerate()
        .filter_map(|(i, object)| {
            let path = object.as_path_object()?;
            let b = object.bounds().ok()?;
            Some((
                i,
                [
                    b.left().value,
                    b.bottom().value,
                    b.right().value,
                    b.top().value,
                ],
                rectangle_bounds(&object).is_some() || straight_rule(&object),
                path.fill_color().ok(),
            ))
        })
        .collect();
    if paths.is_empty() {
        return (Default::default(), vec![]);
    }
    let chars = page
        .text()
        .ok()
        .map(|text| {
            text.chars()
                .iter()
                .filter_map(|g| {
                    let ch = g.unicode_char()?;
                    if ch.is_whitespace() || ch.is_control() {
                        return None;
                    }
                    let (x, y) = g.origin().ok()?;
                    Some((
                        x.value,
                        y.value,
                        ch,
                        g.scaled_font_size().value,
                        g.font_is_bold_reenforced()
                            || g.font_name().to_lowercase().contains("bold"),
                    ))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut filled: Vec<[f32; 4]> = paths
        .iter()
        .filter_map(|(_, b, rectangle, color)| {
            let color = color.as_ref()?;
            if !rectangle
                || b[2] - b[0] < page.width().value * 0.5
                || color.alpha() == 0
                || [color.red(), color.green(), color.blue()]
                    .into_iter()
                    .any(|c| c < 230)
                || color.red().abs_diff(color.green()) > 10
                || color.green().abs_diff(color.blue()) > 10
            {
                return None;
            }
            Some(*b)
        })
        .collect();
    // Publishers often paint a callout as overlapping rectangles, one per
    // paragraph/title. Classify the connected backdrop, not each fragment:
    // otherwise a one-line title or two-line continuation becomes a figure.
    let mut merged_fills: Vec<[f32; 4]> = Vec::new();
    for mut b in filled.drain(..) {
        loop {
            let matching = merged_fills.iter().position(|p| {
                (b[0] - p[0]).abs() <= 1.
                    && (b[2] - p[2]).abs() <= 1.
                    && b[1] <= p[3] + 1.
                    && p[1] <= b[3] + 1.
            });
            let Some(i) = matching else { break };
            let p = merged_fills.swap_remove(i);
            b = [
                b[0].min(p[0]),
                b[1].min(p[1]),
                b[2].max(p[2]),
                b[3].max(p[3]),
            ];
        }
        merged_fills.push(b);
    }
    let mut panels = Vec::new();
    // Border-only callouts may be four filled strips or one stroked rectangle.
    // Their text geometry, rather than a fill color or a NOTE keyword, proves
    // that these rules frame flowing prose. This also handles continued panels.
    let mut titles = std::collections::BTreeMap::<i32, (f32, f32, f32, bool, usize, bool)>::new();
    for &(x, y, ch, size, bold) in &chars {
        let line = titles
            .entry((y * 2.).round() as i32)
            .or_insert((x, x, size, true, 0, true));
        line.0 = line.0.min(x);
        line.1 = line.1.max(x + size * 0.5);
        line.2 = line.2.max(size);
        line.3 &= bold;
        line.4 += 1;
        line.5 &= !ch.is_alphabetic() || ch.is_uppercase();
    }
    let contains_prose = |b: [f32; 4]| {
        let mut lines = std::collections::BTreeMap::<i32, (usize, f32, f32)>::new();
        for &(x, y, ..) in &chars {
            if x >= b[0] && x <= b[2] && y >= b[1] && y <= b[3] {
                let line = lines.entry((y * 2.).round() as i32).or_insert((0, x, x));
                line.0 += 1;
                line.1 = line.1.min(x);
                line.2 = line.2.max(x);
            }
        }
        let long_lines = lines
            .values()
            .filter(|&&(count, left, right)| {
                count >= 20
                    && right - left >= (b[2] - b[0]) * 0.6
                    && left <= b[0] + (b[2] - b[0]) * 0.08
            })
            .count()
            >= 3;
        let titled =
            titles
                .iter()
                .any(|(baseline, &(left, right, size, bold, count, uppercase))| {
                    let y = *baseline as f32 * 0.5;
                    (3..=40).contains(&count)
                        && size >= 10.
                        && (bold || uppercase)
                        && left >= b[0]
                        && right <= b[2]
                        && y >= b[1]
                        && y <= b[3]
                        && b[3] - y <= size * 3.5
                        && ((left + right - b[0] - b[2]) * 0.5).abs() <= (b[2] - b[0]) * 0.12
                        && (b[1] < 0.
                            || b[3] > page.height().value
                            || lines.iter().any(|(baseline, &(count, left, _))| {
                                *baseline as f32 * 0.5 < y - size
                                    && count >= 20
                                    && left <= b[0] + (b[2] - b[0]) * 0.08
                            }))
                });
        long_lines || titled
    };
    for b in merged_fills {
        let mut rows = std::collections::BTreeMap::<i32, (usize, f32, f32, f32)>::new();
        for &(x, y, _, size, _) in &chars {
            if x >= b[0] && x <= b[2] && y >= b[1] && y <= b[3] {
                let row = rows
                    .entry((y * 2.).round() as i32)
                    .or_insert((0, x, x, size));
                row.0 += 1;
                row.1 = row.1.min(x);
                row.2 = row.2.max(x);
                row.3 = row.3.max(size);
            }
        }
        let prose_rows: Vec<_> = rows
            .values()
            .filter(|&&(count, left, right, _)| {
                count >= 20
                    && right - left >= (b[2] - b[0]) * 0.6
                    && left <= b[0] + (b[2] - b[0]) * 0.08
            })
            .collect();
        // Short shaded continuations also qualify, but a large chart with one
        // sentence-like label does not. Long panels still need repeated prose.
        let short_prose = !prose_rows.is_empty()
            && b[3] - b[1]
                <= (rows.len() as f32 + 2.)
                    * prose_rows.iter().map(|row| row.3).fold(0., f32::max)
                    * 1.5;
        if contains_prose(b) || short_prose {
            panels.push(b);
        }
    }
    for (_, b, rectangle, _) in &paths {
        if *rectangle
            && b[2] - b[0] >= page.width().value * 0.5
            && b[3] - b[1] > 12.
            && contains_prose(*b)
        {
            panels.push(*b);
        }
    }
    for (_, left, rectangle, _) in &paths {
        if !rectangle || left[2] - left[0] > 2. || left[3] - left[1] < 24. {
            continue;
        }
        for (_, right, rectangle, _) in &paths {
            if !rectangle
                || right[2] - right[0] > 2.
                || right[2] - left[0] < page.width().value * 0.5
                || (left[1] - right[1]).abs() > 2.
                || (left[3] - right[3]).abs() > 2.
            {
                continue;
            }
            let b = [left[0], left[1], right[2], left[3]];
            let end_rule = paths.iter().any(|(_, edge, rectangle, _)| {
                *rectangle
                    && edge[3] - edge[1] <= 2.
                    && (edge[0] - b[0]).abs() <= 2.
                    && (edge[2] - b[2]).abs() <= 2.
                    && ((edge[1] - b[1]).abs() <= 2. || (edge[3] - b[3]).abs() <= 2.)
            });
            if end_rule && contains_prose(b) {
                panels.push(b);
            }
        }
    }
    let indices = paths
        .into_iter()
        .filter_map(|(i, b, rectangle, color)| {
            let light_fill = color.as_ref().is_some_and(|c| {
                c.alpha() > 0
                    && c.red() >= 230
                    && c.green() >= 230
                    && c.blue() >= 230
                    && c.red().abs_diff(c.green()) <= 10
                    && c.green().abs_diff(c.blue()) <= 10
            });
            let panel_edge = rectangle
                && panels.iter().any(|p| {
                    b[0] >= p[0] - 1.
                        && b[1] >= p[1] - 1.
                        && b[2] <= p[2] + 1.
                        && b[3] <= p[3] + 1.
                        && (b[2] - b[0] <= 2.
                            || b[3] - b[1] <= 2.
                            || b.iter().zip(p).all(|(a, b)| (a - b).abs() <= 1.)
                            || (light_fill
                                && (b[0] - p[0]).abs() <= 1.
                                && (b[2] - p[2]).abs() <= 1.))
                });
            (panel_edge || extends_across_page(b, page.width().value, page.height().value))
                .then_some(i)
        })
        .collect();
    // A qualifying fill can contain several independently qualifying source
    // rectangles. Render one native callout, never duplicate overlapping text.
    let outer = panels.clone();
    panels.retain(|b| {
        !outer.iter().any(|p| {
            b[0] >= p[0]
                && b[1] >= p[1]
                && b[2] <= p[2]
                && b[3] <= p[3]
                && (p[2] - p[0]) * (p[3] - p[1]) > (b[2] - b[0]) * (b[3] - b[1]) + 1.
        })
    });
    panels.sort_by(|a, b| b[3].total_cmp(&a[3]));
    panels.dedup_by(|a, b| a.iter().zip(b.iter()).all(|(a, b)| (a - b).abs() <= 1.));
    (indices, panels)
}

/// PDFium object bounds include hidden content outside clip paths. For a form,
/// clip its children in local coordinates before applying the form transform;
/// clipping only the outer union misses nested clips and can swallow captions.
pub(super) fn visible_object_bounds(object: &PdfPageObject<'_>, depth: usize) -> Option<PdfRect> {
    if depth > 16 {
        return None;
    }
    let bounds = if let Some(form) = object.as_x_object_form_object() {
        let mut union: Option<[f32; 4]> = None;
        for child in form.iter() {
            let Some(b) = visible_object_bounds(&child, depth + 1) else {
                continue;
            };
            let b = [
                b.left().value,
                b.bottom().value,
                b.right().value,
                b.top().value,
            ];
            union = Some(match union {
                Some(a) => [
                    a[0].min(b[0]),
                    a[1].min(b[1]),
                    a[2].max(b[2]),
                    a[3].max(b[3]),
                ],
                None => b,
            });
        }
        let b = union?;
        PdfRect::new_from_values(b[1], b[0], b[3], b[2])
            .to_quad_points()
            .transform(form.matrix().ok()?)
            .to_rect()
    } else {
        object.bounds().ok()?.to_rect()
    };
    clip_object_bounds(object, bounds)
}

pub(super) fn clip_object_bounds(
    object: &PdfPageObject<'_>,
    mut bounds: PdfRect,
) -> Option<PdfRect> {
    if let Some(clip) = object.get_clip_path() {
        for path in clip.iter() {
            // A bounding box is conservative even for curved/nonrectangular
            // clips: Bezier control points enclose the visible curve. Separate
            // clip paths intersect, while subpaths within one path may unite.
            let points: Vec<_> = path.iter().map(|s| (s.x().value, s.y().value)).collect();
            if points.is_empty() {
                continue;
            }
            let left = points.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
            let bottom = points.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
            let right = points.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
            let top = points.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
            let left = bounds.left().value.max(left);
            let bottom = bounds.bottom().value.max(bottom);
            let right = bounds.right().value.min(right);
            let top = bounds.top().value.min(top);
            // PdfRect normalizes reversed endpoints, so reject an empty
            // intersection before construction rather than reviving it.
            if left >= right || bottom >= top {
                return None;
            }
            bounds = PdfRect::new_from_values(bottom, left, top, right);
        }
    }
    Some(bounds)
}

/// PDFium extracts even fully clipped text. Suppress it only in presentation,
/// not in the source string used for import matching and annotation anchors.
pub(crate) fn hidden_text_indices(
    page: &PdfPage<'_>,
    text: &PdfPageText<'_>,
) -> std::collections::HashSet<usize> {
    fn visit(
        object: &PdfPageObject<'_>,
        depth: usize,
        text: &PdfPageText<'_>,
        to_page: &dyn Fn(PdfRect) -> Option<PdfRect>,
        hidden: &mut std::collections::HashSet<usize>,
    ) {
        if depth > 16 {
            return;
        }
        if let Some(form) = object.as_x_object_form_object() {
            let Ok(matrix) = form.matrix() else { return };
            let transform = |b: PdfRect| {
                to_page(clip_object_bounds(
                    object,
                    b.to_quad_points().transform(matrix).to_rect(),
                )?)
            };
            for child in form.iter() {
                visit(&child, depth + 1, text, &transform, hidden);
            }
        } else if let Some(text_object) = object.as_text_object() {
            let Ok(bounds) = object.bounds() else { return };
            if clip_object_bounds(object, bounds.to_rect())
                .and_then(to_page)
                .is_none()
            {
                // Match object identity, not position: visible prose may occupy
                // the same coordinates as an invisible label behind a clip.
                if let Ok(chars) = text.chars_for_object(text_object) {
                    hidden.extend(chars.iter().map(|ch| ch.index()));
                }
            }
        }
    }
    let mut hidden = std::collections::HashSet::new();
    for object in page.objects().iter() {
        visit(&object, 0, text, &Some, &mut hidden);
    }
    hidden
}

pub(super) fn page_regions(
    page: &PdfPage<'_>,
    width: u32,
    height: u32,
    config: &PdfRenderConfig,
) -> Result<(Vec<VisualRegion>, Vec<[u32; 4]>)> {
    let mut regions: Vec<(usize, [u32; 4], bool)> = Vec::new();
    let mut labels = Vec::new();
    let inline_images = crate::pdf::recovery::inline_image_indices(page);
    let backgrounds = page_background_indices(page);
    let tables = crate::pdf::recovery::table_bounds(page);
    let captions = crate::pdf::recovery::caption_bounds(page);
    // PdfPageTextObject::text() loads/parses the ENTIRE text page each time.
    // Colored code can have hundreds of tiny text objects: share one handle
    // instead of turning figure geometry recovery into quadratic PDF work.
    let page_text = page.text().ok();
    for (index, object) in page.objects().iter().enumerate() {
        if inline_images.contains(&index) || backgrounds.contains(&index) {
            continue;
        }
        let visual = has_visual(&object, 0);
        let text = object.as_text_object().map(|object| {
            page_text
                .as_ref()
                .map(|text| text.for_object(object))
                .unwrap_or_default()
        });
        if !visual && text.as_ref().is_none_or(|text| text.trim().is_empty()) {
            continue;
        }
        let Some(bounds) = visible_object_bounds(&object, 0) else {
            continue;
        };
        if visual
            && tables.iter().any(|b| {
                bounds.left().value >= b[0] - 1.
                    && bounds.bottom().value >= b[1] - 1.
                    && bounds.right().value <= b[2] + 1.
                    && bounds.top().value <= b[3] + 1.
            })
        {
            continue;
        }
        let corners = [
            (bounds.left(), bounds.top()),
            (bounds.right(), bounds.top()),
            (bounds.left(), bounds.bottom()),
            (bounds.right(), bounds.bottom()),
        ];
        let pixels = corners
            .into_iter()
            .map(|(x, y)| page.points_to_pixels(x, y, config))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let x0 = pixels.iter().map(|p| p.0).min().unwrap().max(0) as u32;
        let y0 = pixels.iter().map(|p| p.1).min().unwrap().max(0) as u32;
        let x1 = (pixels.iter().map(|p| p.0).max().unwrap().max(0) as u32).min(width);
        let y1 = (pixels.iter().map(|p| p.1).max().unwrap().max(0) as u32).min(height);
        if x1 <= x0 || y1 <= y0 {
            continue;
        }
        if !visual {
            let x = (bounds.left().value + bounds.right().value) * 0.5;
            let y = (bounds.bottom().value + bounds.top().value) * 0.5;
            if !captions
                .iter()
                .any(|b| x >= b[0] && x <= b[2] && y >= b[1] && y <= b[3])
            {
                labels.push([x0, y0, x1, y1]);
            }
            continue;
        }
        let raster = has_raster(&object, 0);
        // Full-page vector backgrounds must not connect otherwise separate
        // illustrations. Actual full-page raster scans/covers remain intact.
        if !raster && (x1 - x0) as f64 * (y1 - y0) as f64 > width as f64 * height as f64 * 0.95 {
            continue;
        }
        regions.push((index, [x0, y0, x1, y1], raster));
    }
    Ok((regions, labels))
}
