//! PDF links recovery.
use super::*;

pub(super) fn attach_links(page: &PdfPage<'_>, glyphs: &mut [Glyph]) {
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

pub(super) fn link_glyphs(glyphs: &[Glyph], regions: &[PdfRect], fallback: bool) -> Vec<usize> {
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

pub(super) fn apply_links(glyphs: &[Glyph], eligible: &[bool], mut html: String) -> String {
    let mut i = 0;
    while i < glyphs.len() {
        let Some(url) = glyphs[i].link.as_ref().filter(|_| eligible[i]) else {
            i += 1;
            continue;
        };
        let start = i;
        while i < glyphs.len()
            && eligible[i]
            && glyphs[i].link.as_ref() == Some(url)
            && !glyphs[i].ch.is_control()
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
