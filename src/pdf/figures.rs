//! Recover figure regions and labels without absorbing surrounding prose.
use super::*;

pub(crate) struct ReadingFigure {
    pub indices: Vec<usize>,
    pub bounds: [u32; 4],
    pub html: String,
    pub preview: FigurePreview,
}

/// Geometry-only QA view of the same source figure regions, without rendering
/// or encoding crops. The book audit compares font-run positions to these.
#[cfg(test)]
pub(crate) fn reading_visual_bounds(page: &PdfPage<'_>) -> Result<Vec<[u32; 4]>> {
    let config = PdfRenderConfig::new()
        .set_target_width(1200)
        .set_maximum_height(4000);
    let width = 1200;
    let height = (width as f32 * page.height().value / page.width().value).round() as u32;
    let (regions, labels) = page_regions(page, width, height, &config)?;
    let captions = caption_pixels(page, &config)?;
    Ok(captioned_regions(&visual_regions(&regions), &captions)
        .into_iter()
        .filter(|r| r.bounds[2] - r.bounds[0] >= 80 && r.bounds[3] - r.bounds[1] >= 40)
        .map(|r| r.with_labels(&labels))
        .collect())
}

/// On-demand display crops use painted bounds, not nearby prose/figure captions.
/// No writes: existing source blocks and annotation anchors remain unchanged.
pub(crate) fn reading_figures(
    page: &PdfPage<'_>,
    doc: &PdfDocument<'_>,
    n: usize,
) -> Result<(PdfRenderConfig, Vec<ReadingFigure>)> {
    use base64::Engine;
    let config = PdfRenderConfig::new()
        .set_target_width(1200)
        .set_maximum_height(4000)
        // Preserve unpainted paper as transparency, including antialiased
        // label edges. Intentional white fills and raster colors stay intact.
        .set_clear_color(PdfColor::new(0, 0, 0, 0))
        .render_form_data(false);
    if !page.objects().iter().any(|o| has_visual(&o, 0)) {
        return Ok((config, vec![]));
    }
    // Match PdfRenderConfig's aspect-preserving target/maximum calculation.
    // Code backgrounds, table rules and small inline icons count as visual
    // objects too; do not rasterize an entire 1200px page just to discover
    // afterwards that none of those objects is a displayable figure.
    let scale = (1200. / page.width().value).min(4000. / page.height().value);
    let width = (page.width().value * scale).round() as u32;
    let height = (page.height().value * scale).round() as u32;
    let (regions, labels) = page_regions(page, width, height, &config)?;
    let captions = caption_pixels(page, &config)?;
    let regions: Vec<_> = captioned_regions(&visual_regions(&regions), &captions)
        .into_iter()
        // Keep small, isolated icons in their existing figure blocks.
        .filter(|r| r.bounds[2] - r.bounds[0] >= 80 && r.bounds[3] - r.bounds[1] >= 40)
        .collect();
    if regions.is_empty() {
        return Ok((config, vec![]));
    }
    let mut page_image = None;
    let mut figures = Vec::new();
    for region in regions {
        let index = region.indices[0];
        let mut bounds = region.with_labels(&labels);
        if !region.raster {
            let (left, bottom) =
                page.pixels_to_points(bounds[0] as i32, bounds[3] as i32, &config)?;
            let (right, top) =
                page.pixels_to_points(bounds[2] as i32, bounds[1] as i32, &config)?;
            if let Some(b) = crate::pdf::recovery::diagram_callout_bounds(
                page,
                [left.value, bottom.value, right.value, top.value],
            ) {
                let (x0, y1) =
                    page.points_to_pixels(PdfPoints::new(b[0]), PdfPoints::new(b[1]), &config)?;
                let (x1, y0) =
                    page.points_to_pixels(PdfPoints::new(b[2]), PdfPoints::new(b[3]), &config)?;
                bounds = [
                    x0.max(0) as u32,
                    y0.max(0) as u32,
                    (x1.max(0) as u32).min(width),
                    (y1.max(0) as u32).min(height),
                ];
            }
        }
        let [left, top, right, bottom] = bounds;
        let mut png = std::io::Cursor::new(Vec::new());
        // Keep source-page geometry for ordering/omission, but repair a
        // standalone bitmap stretched by the PDF's placement transform.
        let bitmap = (region.indices.len() == 1)
            .then(|| undistorted_bitmap(page, doc, index, scale))
            .flatten();
        let display = if let Some(image) = bitmap {
            image
        } else {
            if page_image.is_none() {
                page_image = Some(page.render_with_config(&config)?.as_image()?);
            }
            page_image
                .as_ref()
                .unwrap()
                .crop_imm(left, top, right - left, bottom - top)
        };
        let crop = crate::figure_image::transparent_paper(display.to_rgba8());
        // These are transient display previews, not archival source images.
        // Adaptive PNG filtering tries several predictors on every scanline;
        // in a debug desktop build that alone can take seconds on a diagram.
        // A fixed Up predictor is lossless and avoids that repeated work.
        image::ImageEncoder::write_image(
            image::codecs::png::PngEncoder::new_with_quality(
                &mut png,
                image::codecs::png::CompressionType::Fast,
                image::codecs::png::FilterType::Up,
            ),
            crop.as_raw(),
            crop.width(),
            crop.height(),
            image::ExtendedColorType::Rgba8,
        )?;
        let png = png.into_inner();
        let data = base64::engine::general_purpose::STANDARD.encode(&png);
        let id = format!("pdf-p{n:03}-img{:03}", index + 1);
        figures.push(ReadingFigure {
            indices: region.indices,
            bounds,
            html: format!("<p><a href=\"easyread-image:{id}\"><img src=\"data:image/png;base64,{data}\" width=\"100%\" style=\"aspect-ratio:{}/{}\" alt=\"\" title=\"{id}\"></a></p>", crop.width(), crop.height()),
            preview: FigurePreview { id, png, width: crop.width(), height: crop.height() },
        });
    }
    figures.sort_by_key(|f| (f.bounds[1], f.bounds[0]));
    Ok((config, figures))
}

/// Recover intrinsic proportions only for an isolated, unrotated bitmap.
/// Composite diagrams, overlays, clipping, reflection and page rotation keep
/// the page crop: extracting their bitmap alone could discard visible content.
pub(super) fn undistorted_bitmap(
    page: &PdfPage<'_>,
    doc: &PdfDocument<'_>,
    index: usize,
    scale: f32,
) -> Option<image::DynamicImage> {
    if page.rotation().ok()? != PdfPageRenderRotation::None {
        return None;
    }
    let object = page.objects().get(index).ok()?;
    let raster = object.as_image_object()?;
    let matrix = raster.matrix().ok()?;
    if matrix.a() <= 0.
        || matrix.d() <= 0.
        || matrix.b().abs() > 0.001
        || matrix.c().abs() > 0.001
        || object.get_clip_path().is_some_and(|clip| !clip.is_empty())
    {
        return None;
    }
    let bounds = object.bounds().ok()?;
    if bounds.left().value < 0.
        || bounds.bottom().value < 0.
        || bounds.right().value > page.width().value
        || bounds.top().value > page.height().value
    {
        return None;
    }
    let width = raster.width().ok()?;
    let height = raster.height().ok()?;
    if width <= 0 || height <= 0 || i64::from(width) * i64::from(height) > 16_000_000 {
        return None;
    }
    let intrinsic = width as f32 / height as f32;
    let placed = bounds.width().value / bounds.height().value;
    if !placed.is_finite() || (placed / intrinsic - 1.).abs() <= 0.05 {
        return None;
    }
    for (other_index, other) in page.objects().iter().enumerate() {
        if other_index == index {
            continue;
        }
        let b = other.bounds().ok()?;
        // Also guard against text labels inside or immediately beside the
        // bitmap and nearby visual objects merged into the same figure.
        let gap = if has_visual(&other, 0) {
            12. / scale
        } else {
            1.
        };
        if b.left().value < bounds.right().value + gap
            && b.right().value > bounds.left().value - gap
            && b.bottom().value < bounds.top().value + gap
            && b.top().value > bounds.bottom().value - gap
        {
            return None;
        }
    }
    // Unlike raw extraction, processed rendering preserves soft masks and
    // filters. PDFium restores the object's matrix before returning.
    raster
        .get_processed_image_with_size(doc, width, height)
        .ok()
}

// Axis labels can sit outside all painted paths. Include nearby text against
// the original region only: never walk from a label into surrounding prose.
pub(super) fn include_labels(bounds: [u32; 4], labels: &[[u32; 4]]) -> [u32; 4] {
    let mut result = bounds;
    // Text objects can be individual letters. Judge the physical text line,
    // otherwise every letter of a wide body paragraph looks like a tiny label.
    for b in label_lines(labels) {
        let outside_y = b[3] < bounds[1] || b[1] > bounds[3];
        let broad_external_line =
            outside_y && u64::from(b[2] - b[0]) * 5 >= u64::from(bounds[2] - bounds[0]) * 3;
        let horizontal = b[0] >= bounds[0].saturating_sub(4)
            && b[2] <= bounds[2].saturating_add(4)
            && b[1] <= bounds[3].saturating_add(48)
            && b[3].saturating_add(48) >= bounds[1]
            && !broad_external_line;
        let vertical = b[1] >= bounds[1].saturating_sub(4)
            && b[3] <= bounds[3].saturating_add(4)
            && b[0] <= bounds[2].saturating_add(48)
            && b[2].saturating_add(48) >= bounds[0];
        if horizontal || vertical {
            result[0] = result[0].min(b[0]);
            result[1] = result[1].min(b[1]);
            result[2] = result[2].max(b[2]);
            result[3] = result[3].max(b[3]);
        }
    }
    result
}

pub(super) fn label_lines(labels: &[[u32; 4]]) -> Vec<[u32; 4]> {
    let mut lines: Vec<[u32; 4]> = Vec::new();
    for &bounds in labels {
        let mut line = bounds;
        // A later font run can bridge two previously separate fragments.
        // Merge to a fixed point so PDF object order does not change the crop.
        loop {
            let matching = lines.iter().position(|b| {
                let overlap = b[3].min(line[3]).saturating_sub(b[1].max(line[1]));
                let shorter = (b[3] - b[1]).min(line[3] - line[1]);
                shorter > 0
                    && overlap * 2 >= shorter
                    && b[0] <= line[2].saturating_add(12)
                    && line[0] <= b[2].saturating_add(12)
            });
            let Some(index) = matching else { break };
            let b = lines.swap_remove(index);
            line = [
                line[0].min(b[0]),
                line[1].min(b[1]),
                line[2].max(b[2]),
                line[3].max(b[3]),
            ];
        }
        lines.push(line);
    }
    lines
}

pub(super) struct FigureRegion {
    // Keep every pre-grouping component key to suppress old imported crops.
    pub(super) indices: Vec<usize>,
    pub(super) bounds: [u32; 4],
    panels: Vec<[u32; 4]>,
    pub(super) raster: bool,
}

impl FigureRegion {
    pub(super) fn with_labels(&self, labels: &[[u32; 4]]) -> [u32; 4] {
        if self.raster && self.indices.len() == 1 {
            self.bounds
        } else {
            let mut bounds = include_labels(self.bounds, labels);
            if self.panels.len() > 1 {
                // A panel title may be wider than its bitmap and sit above it.
                // Match against original panel centers, not an expanding crop,
                // so titles cannot walk outward into surrounding paragraphs.
                for title in label_lines(labels) {
                    if self.panels.iter().any(|p| {
                        title[3] <= p[1]
                            && p[1] - title[3] <= 48
                            && (title[2] - title[0]) as f64 <= (p[2] - p[0]) as f64 * 2.5
                            && ((title[0] as f64 + title[2] as f64) - (p[0] as f64 + p[2] as f64))
                                .abs()
                                <= (p[2] - p[0]) as f64 * 0.5
                    }) {
                        bounds = [
                            bounds[0].min(title[0]),
                            bounds[1].min(title[1]),
                            bounds[2].max(title[2]),
                            bounds[3].max(title[3]),
                        ];
                    }
                }
            }
            bounds
        }
    }
}

pub(super) fn caption_pixels(
    page: &PdfPage<'_>,
    config: &PdfRenderConfig,
) -> Result<Vec<[u32; 4]>> {
    crate::pdf::recovery::figure_caption_bounds(page)
        .into_iter()
        .map(|b| {
            let corners = [(b[0], b[1]), (b[0], b[3]), (b[2], b[1]), (b[2], b[3])]
                .into_iter()
                .map(|(x, y)| page.points_to_pixels(PdfPoints::new(x), PdfPoints::new(y), config))
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok([
                corners.iter().map(|p| p.0).min().unwrap().max(0) as u32,
                corners.iter().map(|p| p.1).min().unwrap().max(0) as u32,
                corners.iter().map(|p| p.0).max().unwrap().max(0) as u32,
                corners.iter().map(|p| p.1).max().unwrap().max(0) as u32,
            ])
        })
        .collect()
}

/// Disconnected side-by-side panels with ONE shared caption form an atomic
/// crop. Never infer a relationship from horizontal proximity alone: separate
/// captions, vertically stacked images and uncaptained art stay independent.
pub(super) fn captioned_regions(
    regions: &[VisualRegion],
    captions: &[[u32; 4]],
) -> Vec<FigureRegion> {
    let mut groups: Vec<_> = regions
        .iter()
        .map(|&(index, bounds, raster)| FigureRegion {
            indices: vec![index],
            bounds,
            panels: vec![bounds],
            raster,
        })
        .collect();
    let substantial = |b: [u32; 4]| b[2] - b[0] >= 80 && b[3] - b[1] >= 40;
    let overlaps_x = |a: [u32; 4], b: [u32; 4]| a[0] < b[2] && b[0] < a[2];
    let independently_captioned = |b: [u32; 4], caption: [u32; 4]| {
        captions.iter().any(|&other| {
            other != caption
                && overlaps_x(b, other)
                && other[1] >= b[3].saturating_sub(4)
                && other[1] <= caption[1]
        })
    };
    for &caption in captions {
        let max_gap = (caption[3] - caption[1]).saturating_mul(3).max(48);
        let anchor = groups
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                substantial(r.bounds)
                    && caption[0] <= r.bounds[2].saturating_add(r.bounds[2] - r.bounds[0])
                    && r.bounds[0] <= caption[2].saturating_add(r.bounds[2] - r.bounds[0])
                    && r.bounds[3] <= caption[1].saturating_add(4)
                    && caption[1].saturating_sub(r.bounds[3]) <= max_gap
                    && !independently_captioned(r.bounds, caption)
            })
            .min_by_key(|(_, r)| caption[1].saturating_sub(r.bounds[3]))
            .map(|(i, _)| i);
        let Some(anchor) = anchor else { continue };
        let a = groups[anchor].bounds;
        let mut members = vec![anchor];
        for (i, region) in groups.iter().enumerate() {
            let b = region.bounds;
            if i == anchor || !substantial(b) || b[3] > caption[1].saturating_add(4) {
                continue;
            }
            let overlap = a[3].min(b[3]).saturating_sub(a[1].max(b[1]));
            let shorter = (a[3] - a[1]).min(b[3] - b[1]);
            if overlap * 2 < shorter || (a[0] < b[2] && b[0] < a[2]) {
                continue;
            }
            let left = a[0].min(b[0]);
            let right = a[2].max(b[2]);
            let center = (caption[0] as f64 + caption[2] as f64) * 0.5;
            let centers_covered = caption[0] <= (a[0] + a[2]).min(b[0] + b[2]) / 2
                && caption[2] >= (a[0] + a[2]).max(b[0] + b[2]) / 2;
            let gap_centered = center >= a[2].min(b[2]) as f64
                && center <= a[0].max(b[0]) as f64
                && (center - (left as f64 + right as f64) * 0.5).abs()
                    <= (right - left) as f64 * 0.2;
            if !centers_covered && !gap_centered {
                continue;
            }
            // A nearer caption belonging to this panel proves independence.
            if independently_captioned(b, caption) {
                continue;
            }
            members.push(i);
        }
        if members.len() < 2 {
            continue;
        }
        members.sort_unstable();
        let mut merged = FigureRegion {
            indices: vec![],
            bounds: [u32::MAX, u32::MAX, 0, 0],
            panels: vec![],
            raster: false,
        };
        for i in members.into_iter().rev() {
            let r = groups.remove(i);
            merged.indices.extend(r.indices);
            merged.panels.extend(r.panels);
            merged.bounds = [
                merged.bounds[0].min(r.bounds[0]),
                merged.bounds[1].min(r.bounds[1]),
                merged.bounds[2].max(r.bounds[2]),
                merged.bounds[3].max(r.bounds[3]),
            ];
            merged.raster |= r.raster;
        }
        merged.indices.sort_unstable();
        groups.push(merged);
    }
    groups
}

// Connected components over original object bounds, rather than their union
// boxes, keep unrelated regions across a gap from absorbing each other.
pub(super) fn visual_regions(regions: &[(usize, [u32; 4], bool)]) -> Vec<(usize, [u32; 4], bool)> {
    let mut roots: Vec<usize> = (0..regions.len()).collect();
    fn root(roots: &mut [usize], mut index: usize) -> usize {
        while roots[index] != index {
            roots[index] = roots[roots[index]];
            index = roots[index];
        }
        index
    }
    for i in 0..regions.len() {
        for j in 0..i {
            let a = regions[i].1;
            let b = regions[j].1;
            if a[0] <= b[2].saturating_add(12)
                && b[0] <= a[2].saturating_add(12)
                && a[1] <= b[3].saturating_add(12)
                && b[1] <= a[3].saturating_add(12)
            {
                let ri = root(&mut roots, i);
                let rj = root(&mut roots, j);
                roots[ri] = rj;
            }
        }
    }
    let mut combined = std::collections::BTreeMap::<usize, (usize, [u32; 4], bool)>::new();
    for (i, (index, bounds, raster)) in regions.iter().enumerate() {
        combined
            .entry(root(&mut roots, i))
            .and_modify(|(key, b, has_image)| {
                *key = (*key).min(*index);
                b[0] = b[0].min(bounds[0]);
                b[1] = b[1].min(bounds[1]);
                b[2] = b[2].max(bounds[2]);
                b[3] = b[3].max(bounds[3]);
                *has_image |= raster;
            })
            .or_insert((*index, *bounds, *raster));
    }
    combined.into_values().collect()
}
