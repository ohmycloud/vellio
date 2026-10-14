//! PDFium handles never leave the serialized worker scope. No Python runtime.
use anyhow::{Context, Result, ensure};
use pdfium_render::prelude::*;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

static PDF_LOCK: Mutex<()> = Mutex::new(());
static PDFIUM: OnceLock<Pdfium> = OnceLock::new();
pub struct Extracted {
    pub title: String,
    pub author: String,
    pub pages: Vec<Value>,
    pub blocks: Vec<Value>,
    pub layout: Value,
}
pub struct Media {
    pub blocks: Vec<Value>,
    pub layout: Value,
}

/// Read-only code and heading recovery for existing native PDF page blocks.
/// Require an exact extraction match before applying font/position metadata.
pub fn code_html(pdf: &Path, page: usize, expected: &str) -> Result<Option<String>> {
    Ok(code_presentation(pdf, page, expected)?.map(|p| p.html))
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CodePresentation {
    pub html: String,
    pub inline_figures: Vec<String>,
    pub figures: Vec<FigurePreview>,
    #[serde(default)]
    pub math: Vec<MathPresentation>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct MathPresentation {
    pub tex: String,
    pub display: bool,
    pub number: Option<String>,
    pub layout: crate::formula::FormulaLayout,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct FigurePreview {
    pub id: String,
    #[serde(with = "png_base64")]
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub fn code_presentation(
    pdf: &Path,
    page: usize,
    expected: &str,
) -> Result<Option<CodePresentation>> {
    code_presentations(pdf, &[(page, expected.to_owned())])?
        .pop()
        .context("Missing PDF presentation result")?
}

/// Prepare a small batch with one document open and one outline traversal.
/// Keep the batch bounded so a newly requested page can take priority between
/// batches, and do not let a malformed page discard its siblings' results.
pub fn code_presentations(
    pdf: &Path,
    requests: &[(usize, String)],
) -> Result<Vec<Result<Option<CodePresentation>>>> {
    prepare_presentations(pdf, requests, false)
}

/// Translation must distinguish a plain page from stale/mismatched extraction.
/// Never send a flattened fallback when geometry belongs to different text.
pub fn translation_presentations(
    pdf: &Path,
    requests: &[(usize, String)],
) -> Result<Vec<Result<Option<CodePresentation>>>> {
    prepare_presentations(pdf, requests, true)
}

fn prepare_presentations(
    pdf: &Path,
    requests: &[(usize, String)],
    require_match: bool,
) -> Result<Vec<Result<Option<CodePresentation>>>> {
    if requests.is_empty() {
        return Ok(Vec::new());
    }
    let _guard = PDF_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("PDF worker lock poisoned"))?;
    let doc = library(None)?.load_pdf_from_file(pdf, None)?;
    let outline = document_outline(&doc);
    Ok(requests
        .iter()
        .map(|(number, expected)| {
            let page = doc.pages().get(
                i32::try_from(*number)?
                    .checked_sub(1)
                    .context("PDF page numbers start at 1")?,
            )?;
            let headings: Vec<_> = outline
                .iter()
                .filter(|entry| entry.page == *number)
                .cloned()
                .collect();
            let (source, html, indices, figures, math) =
                crate::pdf_code::reading_html(&page, &doc, &headings, *number)?;
            ensure!(
                !require_match || source == *expected,
                "PDF source text no longer matches translation block on page {number}"
            );
            Ok((source == *expected)
                .then_some(html)
                .flatten()
                .map(|html| CodePresentation {
                    html,
                    figures,
                    math,
                    inline_figures: indices
                        .into_iter()
                        .map(|i| format!("pdf-p{number:03}-img{:03}", i + 1))
                        .collect(),
                }))
        })
        .collect())
}

mod png_base64 {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        STANDARD
            .decode(String::deserialize(deserializer)?)
            .map_err(serde::de::Error::custom)
    }
}

/// Resolve a generated local PDF link to a source line, without mutating the book.
pub fn link_destination(pdf: &Path, url: &str) -> Result<(usize, String)> {
    let target = url
        .strip_prefix("easyread-pdf:")
        .context("Not a local PDF link")?;
    let (page, y) = target.split_once(':').context("Invalid PDF destination")?;
    let page: usize = page.parse()?;
    ensure!(page > 0, "PDF page numbers start at 1");
    let y = if y.is_empty() {
        None
    } else {
        let y: f32 = y.parse()?;
        ensure!(y.is_finite(), "Invalid PDF coordinate");
        Some(y)
    };
    let _guard = PDF_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("PDF worker lock poisoned"))?;
    let doc = library(None)?.load_pdf_from_file(pdf, None)?;
    let target_page = doc.pages().get(i32::try_from(page)? - 1)?;
    Ok((page, crate::pdf_code::destination_quote(&target_page, y)?))
}

/// Read local bookmark destinations without changing the document or reader data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlineEntry {
    pub page: usize,
    pub title: String,
    pub depth: usize,
}

pub fn outline(pdf: &Path) -> Result<Vec<OutlineEntry>> {
    let _guard = PDF_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("PDF worker lock poisoned"))?;
    let doc = library(None)?.load_pdf_from_file(pdf, None)?;
    Ok(document_outline(&doc))
}

pub(crate) fn document_outline(doc: &PdfDocument<'_>) -> Vec<OutlineEntry> {
    let mut entries = Vec::new();
    let mut depths: std::collections::HashMap<PdfBookmark<'_>, usize> =
        std::collections::HashMap::new();
    for bookmark in doc.bookmarks().iter().take(10_000) {
        // The iterator visits parents first. parent() only retains the direct
        // parent handle, so walking parent().parent() loses deeper ancestry.
        let depth = bookmark
            .parent()
            .and_then(|parent| depths.get(&parent).copied())
            .map_or(0usize, |depth| (depth + 1).min(32));
        depths.insert(bookmark.clone(), depth);
        let Some(title) = bookmark.title().filter(|s| !s.trim().is_empty()) else {
            continue;
        };
        let page = bookmark
            .destination()
            .and_then(|d| d.page_index().ok())
            .or_else(|| {
                bookmark.action().and_then(|a| {
                    a.as_local_destination_action()
                        .and_then(|a| a.destination().ok())
                        .and_then(|d| d.page_index().ok())
                })
            });
        if let Some(page) = page.and_then(|page| usize::try_from(page).ok())
            && page < usize::try_from(doc.pages().len()).unwrap_or(0)
        {
            entries.push(OutlineEntry {
                page: page + 1,
                title,
                depth,
            });
        }
    }
    entries
}

fn has_visual(object: &PdfPageObject<'_>, depth: usize) -> bool {
    if depth > 16 {
        return false;
    }
    match object {
        PdfPageObject::Image(_) | PdfPageObject::Path(_) | PdfPageObject::Shading(_) => true,
        PdfPageObject::XObjectForm(form) => form.iter().any(|child| has_visual(&child, depth + 1)),
        _ => false,
    }
}
fn has_raster(object: &PdfPageObject<'_>, depth: usize) -> bool {
    if depth > 16 {
        return false;
    }
    match object {
        PdfPageObject::Image(_) => true,
        PdfPageObject::XObjectForm(form) => form.iter().any(|child| has_raster(&child, depth + 1)),
        _ => false,
    }
}

fn page_media(
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

type VisualRegion = (usize, [u32; 4], bool);

fn extends_across_page(bounds: [f32; 4], width: f32, height: f32) -> bool {
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

fn straight_rule(object: &PdfPageObject<'_>) -> bool {
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
        crate::pdf_code::text_highlights(page)
            .into_iter()
            .map(|h| h.index),
    );
    indices
}

pub(crate) fn text_panel_bounds(page: &PdfPage<'_>) -> Vec<[f32; 4]> {
    text_panels(page).1
}

fn text_panels(page: &PdfPage<'_>) -> (std::collections::HashSet<usize>, Vec<[f32; 4]>) {
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
    let mut panels: Vec<_> = paths
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
            let mut lines = std::collections::BTreeMap::<i32, (usize, f32, f32)>::new();
            for &(x, y, ..) in &chars {
                if x < b[0] || x > b[2] || y < b[1] || y > b[3] {
                    continue;
                }
                let line = lines.entry((y * 2.).round() as i32).or_insert((0, x, x));
                line.0 += 1;
                line.1 = line.1.min(x);
                line.2 = line.2.max(x);
            }
            (lines
                .values()
                .filter(|&&(count, left, right)| count >= 20 && right - left >= (b[2] - b[0]) * 0.6)
                .count()
                >= 3)
                .then_some(*b)
        })
        .collect();
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
        .filter_map(|(i, b, rectangle, _)| {
            let panel_edge = rectangle
                && panels.iter().any(|p| {
                    b[0] >= p[0] - 1.
                        && b[1] >= p[1] - 1.
                        && b[2] <= p[2] + 1.
                        && b[3] <= p[3] + 1.
                        && (b[2] - b[0] <= 2.
                            || b[3] - b[1] <= 2.
                            || b.iter().zip(p).all(|(a, b)| (a - b).abs() <= 1.))
                });
            (panel_edge || extends_across_page(b, page.width().value, page.height().value))
                .then_some(i)
        })
        .collect();
    panels.sort_by(|a, b| b[3].total_cmp(&a[3]));
    panels.dedup_by(|a, b| a.iter().zip(b.iter()).all(|(a, b)| (a - b).abs() <= 1.));
    (indices, panels)
}

/// PDFium object bounds include hidden content outside clip paths. For a form,
/// clip its children in local coordinates before applying the form transform;
/// clipping only the outer union misses nested clips and can swallow captions.
fn visible_object_bounds(object: &PdfPageObject<'_>, depth: usize) -> Option<PdfRect> {
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

fn clip_object_bounds(object: &PdfPageObject<'_>, mut bounds: PdfRect) -> Option<PdfRect> {
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

fn page_regions(
    page: &PdfPage<'_>,
    width: u32,
    height: u32,
    config: &PdfRenderConfig,
) -> Result<(Vec<VisualRegion>, Vec<[u32; 4]>)> {
    let mut regions: Vec<(usize, [u32; 4], bool)> = Vec::new();
    let mut labels = Vec::new();
    let inline_images = crate::pdf_code::inline_image_indices(page);
    let backgrounds = page_background_indices(page);
    let tables = crate::pdf_code::table_bounds(page);
    let captions = crate::pdf_code::caption_bounds(page);
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
        let bounds = region.with_labels(&labels);
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
fn undistorted_bitmap(
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
fn include_labels(bounds: [u32; 4], labels: &[[u32; 4]]) -> [u32; 4] {
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

fn label_lines(labels: &[[u32; 4]]) -> Vec<[u32; 4]> {
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

struct FigureRegion {
    // Keep every pre-grouping component key to suppress old imported crops.
    indices: Vec<usize>,
    bounds: [u32; 4],
    panels: Vec<[u32; 4]>,
    raster: bool,
}

impl FigureRegion {
    fn with_labels(&self, labels: &[[u32; 4]]) -> [u32; 4] {
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

fn caption_pixels(page: &PdfPage<'_>, config: &PdfRenderConfig) -> Result<Vec<[u32; 4]>> {
    crate::pdf_code::figure_caption_bounds(page)
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
fn captioned_regions(regions: &[VisualRegion], captions: &[[u32; 4]]) -> Vec<FigureRegion> {
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
fn visual_regions(regions: &[(usize, [u32; 4], bool)]) -> Vec<(usize, [u32; 4], bool)> {
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

/// Extract only visual assets for previously imported text-only workspaces.
/// Never replace their paper text or reader-owned annotations/edits.
pub fn media(
    pdf: &Path,
    root: &Path,
    cancel: &std::sync::atomic::AtomicBool,
    mut progress: impl FnMut(usize, usize),
) -> Result<Media> {
    let _guard = PDF_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("PDF worker lock poisoned"))?;
    let doc = library(None)?.load_pdf_from_file(pdf, None)?;
    let mut result = Media {
        blocks: vec![],
        layout: json!({}),
    };
    for (index, page) in doc.pages().iter().enumerate() {
        ensure!(
            !cancel.load(std::sync::atomic::Ordering::Relaxed),
            "Image extraction cancelled"
        );
        let config = PdfRenderConfig::new()
            .set_target_width(1200)
            .set_maximum_height(4000)
            .render_form_data(false);
        let image = page.render_with_config(&config)?.as_image()?;
        let extracted = page_media(&page, index + 1, &image, &config, root)?;
        result.blocks.extend(extracted.blocks);
        result
            .layout
            .as_object_mut()
            .unwrap()
            .extend(extracted.layout.as_object().unwrap().clone());
        progress(index + 1, doc.pages().len() as usize);
    }
    Ok(result)
}
fn library(explicit: Option<&Path>) -> Result<&'static Pdfium> {
    if let Some(pdfium) = PDFIUM.get() {
        return Ok(pdfium);
    }
    let path = explicit
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("EASYREAD_PDFIUM_LIBRARY").map(PathBuf::from))
        .unwrap_or_else(|| {
            let adjacent = std::env::current_exe()
                .unwrap_or_default()
                .parent()
                .unwrap_or(Path::new("."))
                .join(Pdfium::pdfium_platform_library_name());
            if adjacent.exists() {
                adjacent
            } else {
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("assets/pdfium")
                    .join(Pdfium::pdfium_platform_library_name())
            }
        });
    let bindings = Pdfium::bind_to_library(&path).with_context(|| {
        format!(
            "PDFium unavailable at {}. Set EASYREAD_PDFIUM_LIBRARY to a compatible native library.",
            path.display()
        )
    })?;
    let _ = PDFIUM.set(Pdfium::new(bindings));
    Ok(PDFIUM.get().unwrap())
}
pub fn prepare(
    pdf: &Path,
    root: &Path,
    explicit: Option<&Path>,
    cancel: &std::sync::atomic::AtomicBool,
    mut progress: impl FnMut(usize, usize),
) -> Result<Extracted> {
    let _guard = PDF_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("PDF worker lock poisoned"))?;
    let doc = library(explicit)?
        .load_pdf_from_file(pdf, None)
        .context("PDF cannot be opened (invalid or password protected)")?;
    ensure!(!doc.pages().is_empty(), "PDF has no pages");
    fs::create_dir_all(root.join("pages"))?;
    fs::create_dir_all(root.join("extract"))?;
    let mut result = Extracted {
        title: doc
            .metadata()
            .get(PdfDocumentMetadataTagType::Title)
            .map(|t| t.value().into())
            .unwrap_or_default(),
        author: doc
            .metadata()
            .get(PdfDocumentMetadataTagType::Author)
            .map(|t| t.value().into())
            .unwrap_or_default(),
        pages: vec![],
        blocks: vec![],
        layout: json!({}),
    };
    for (index, page) in doc.pages().iter().enumerate() {
        ensure!(
            !cancel.load(std::sync::atomic::Ordering::Relaxed),
            "PDF import cancelled"
        );
        let n = index + 1;
        let config = PdfRenderConfig::new()
            .set_target_width(1200)
            .set_maximum_height(4000)
            .render_form_data(false);
        let bitmap = page.render_with_config(&config)?;
        let image = bitmap.as_image()?;
        let (w, h) = (image.width() as f64, image.height() as f64);
        let name = format!("pages/page-{n:03}.webp");
        image.save(root.join(&name))?;
        let textpage = page.text()?;
        let text = textpage
            .chars()
            .iter()
            .filter_map(|ch| ch.unicode_char())
            .collect::<String>()
            .replace("\r\n", "\n");
        fs::write(root.join(format!("extract/page-{n:03}.txt")), &text)?;
        let mut chars = Vec::new();
        for ch in textpage.chars().iter() {
            let Some(c) = ch.unicode_char() else {
                continue;
            };
            if c.is_control() {
                continue;
            }
            if let Ok(bounds) = ch.tight_bounds() {
                let corners = [
                    (bounds.left(), bounds.top()),
                    (bounds.right(), bounds.top()),
                    (bounds.left(), bounds.bottom()),
                    (bounds.right(), bounds.bottom()),
                ];
                let pixels = corners
                    .into_iter()
                    .map(|(x, y)| page.points_to_pixels(x, y, &config))
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                let x0 = pixels.iter().map(|p| p.0).min().unwrap() as f64 / w;
                let x1 = pixels.iter().map(|p| p.0).max().unwrap() as f64 / w;
                let y0 = pixels.iter().map(|p| p.1).min().unwrap() as f64 / h;
                let y1 = pixels.iter().map(|p| p.1).max().unwrap() as f64 / h;
                chars.push(json!([
                    c.to_string(),
                    x0.clamp(0., 1.),
                    y0.clamp(0., 1.),
                    x1.clamp(0., 1.),
                    y1.clamp(0., 1.)
                ]));
            }
        }
        crate::store::write_json(
            &root.join(format!("extract/page-{n:03}.chars.json")),
            &json!(chars),
        )?;
        result
            .pages
            .push(json!({"n":n,"w":page.width().value,"h":page.height().value,"img":name}));
        // Conservative source view: do not pretend extraction identifies tables/math/reading order.
        if !text.trim().is_empty() {
            let id = format!("pdf-p{n:03}");
            result.blocks.push(
                json!({"id":id,"type":"para","page":n,"en":text,"zh":"","source_kind":"pdf-text"}),
            );
            result.layout[&id] = json!({"page":n,"src":"rust-pdfium","box":[0.,0.,1.,1.]});
            let media = page_media(&page, n, &image, &config, root)?;
            result.blocks.extend(media.blocks);
            result
                .layout
                .as_object_mut()
                .unwrap()
                .extend(media.layout.as_object().unwrap().clone());
        } else {
            // Keep image-only/scanned pages visible without inventing a caption.
            // A figure's actual caption may start on the following source page;
            // no extracted text here does not establish an OCR failure.
            let id = format!("pdf-p{n:03}-page");
            result.blocks.push(json!({"id":id,"type":"figure","page":n,"src":name,"width":image.width(),"height":image.height(),"caption_zh":"","caption_en":"","source_kind":"pdf-page"}));
            result.layout[&id] = json!({"page":n,"src":"rust-pdfium","box":[0.,0.,1.,1.]});
        }
        progress(n, doc.pages().len() as usize);
    }
    Ok(result)
}
pub fn crop(ws: &crate::store::Workspace, page: usize, box_: [f64; 4], out: &Path) -> Result<()> {
    ensure!(
        page > 0
            && box_
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            && box_[0] < box_[2]
            && box_[1] < box_[3],
        "Invalid crop box"
    );
    let pages = ws.load("paper")?;
    let rel = pages["meta"]["pages"][page - 1]["img"]
        .as_str()
        .context("Page not found")?;
    let mut image = image::open(ws.asset(rel)?)?;
    let (w, h) = (image.width(), image.height());
    let x0 = (box_[0] * w as f64).floor() as u32;
    let y0 = (box_[1] * h as f64).floor() as u32;
    let x1 = (box_[2] * w as f64).ceil().min(w as f64) as u32;
    let y1 = (box_[3] * h as f64).ceil().min(h as f64) as u32;
    image.crop(x0, y0, x1 - x0, y1 - y0).save(out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn clipped_callout_backgrounds_do_not_absorb_prose_or_real_figures() {
        assert!(super::extends_across_page(
            [72., -2365., 540., 401.4],
            612.,
            792.
        ));
        assert!(super::extends_across_page(
            [72., -1717., 540., 1049.],
            612.,
            792.
        ));
        assert!(super::extends_across_page(
            [72., -1717., 72.6, 1049.],
            612.,
            792.
        ));
        assert!(!super::extends_across_page(
            [93., 292., 519., 696.],
            612.,
            792.
        ));
        assert!(!super::extends_across_page(
            [0., 0., 612., 792.],
            612.,
            792.
        ));
        assert!(!super::extends_across_page(
            [72., -40., 540., 600.],
            612.,
            792.
        ));
    }
    use super::*;
    #[test]
    fn chart_labels_expand_crop_without_following_surrounding_prose() {
        assert_eq!(
            include_labels(
                [100, 100, 600, 500],
                &[
                    [65, 180, 82, 400],   // Rotated vertical axis label.
                    [240, 520, 460, 540], // Horizontal axis label.
                    [240, 568, 460, 590], // Do not walk out through the first label.
                    [60, 60, 700, 82],    // A wider prose line is not part of the chart.
                ]
            ),
            [65, 100, 600, 540]
        );
    }
    #[test]
    fn fragmented_prose_below_a_wide_figure_is_not_a_label() {
        let bounds = [100, 100, 1000, 300];
        // A body line at exactly the same left margin, made of tiny PDF runs.
        let mut labels: Vec<_> = (0..85)
            .map(|i| [100 + i * 10, 325, 108 + i * 10, 340])
            .collect();
        labels.extend([[420, 80, 600, 95], [50, 130, 80, 250]]);
        assert_eq!(include_labels(bounds, &labels), [50, 80, 1000, 300]);
        labels.reverse();
        assert_eq!(include_labels(bounds, &labels), [50, 80, 1000, 300]);
    }
    #[test]
    fn shared_captions_group_rows_not_stacks_or_independently_captioned_art() {
        let panels = [
            (2, [100, 100, 220, 300], true),
            (4, [320, 90, 480, 330], true),
            (6, [580, 100, 700, 300], false),
            (8, [100, 600, 700, 800], true),
        ];
        let groups = captioned_regions(&panels, &[[80, 350, 720, 390]]);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[1].indices, [2, 4, 6]);
        assert_eq!(groups[1].bounds, [100, 90, 700, 330]);
        assert_eq!(captioned_regions(&panels, &[]).len(), 4);
        assert_eq!(
            captioned_regions(
                &panels,
                &[
                    [100, 350, 230, 370],
                    [320, 350, 470, 370],
                    [580, 350, 700, 370]
                ]
            )
            .len(),
            4
        );
        // A short centered shared caption also identifies two panels.
        assert_eq!(
            captioned_regions(&panels[..2], &[[270, 350, 310, 370]]).len(),
            1
        );
        // A later, wider caption must not absorb a panel with its own label.
        assert_eq!(
            captioned_regions(&panels[..2], &[[100, 310, 230, 330], [80, 350, 720, 390]]).len(),
            2
        );
        assert_eq!(
            captioned_regions(&panels[..2], &[[320, 340, 480, 345], [80, 350, 720, 390]]).len(),
            2
        );
        assert_eq!(
            captioned_regions(&[panels[0], panels[3]], &[[80, 820, 720, 840]]).len(),
            2
        );
    }

    #[test]
    fn shared_panel_titles_do_not_walk_into_prose() {
        let groups = captioned_regions(
            &[
                (1, [180, 100, 300, 300], true),
                (3, [500, 100, 700, 330], true),
            ],
            &[[150, 350, 720, 370]],
        );
        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0].with_labels(&[
                [130, 70, 350, 85], // Wider than the left bitmap, centered on it.
                [520, 70, 680, 85],
                [80, 40, 780, 55],  // Broad preceding body, not a title.
                [130, 10, 350, 25], // Must not walk upward from the first title.
            ]),
            [130, 70, 700, 330]
        );
    }

    #[test]
    fn vector_connectors_group_icons_but_keep_separate_diagrams() {
        let regions = [
            (0, [0, 0, 100, 80], false),
            (1, [20, 20, 50, 50], true),
            (2, [100, 30, 190, 40], false),
            (3, [190, 0, 290, 80], false),
            (4, [400, 0, 500, 80], false),
        ];
        let groups = visual_regions(&regions);
        assert_eq!(groups.len(), 2);
        assert!(groups.contains(&(0, [0, 0, 290, 80], true)));
        assert!(groups.contains(&(4, [400, 0, 500, 80], false)));
    }
}
