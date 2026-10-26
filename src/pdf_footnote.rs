//! Conservative page-bottom footnotes: require a short separator, smaller text
//! and a literal note marker. Never infer footnotes from small type alone.
use super::*;
use std::collections::HashSet;

pub(super) fn recover(
    page: &PdfPage<'_>,
    glyphs: &[Glyph],
    omitted: &HashSet<usize>,
) -> Vec<(Vec<usize>, String)> {
    let rules = page
        .objects()
        .iter()
        .filter_map(|object| {
            let path = object.as_path_object()?;
            let segments = path.segments().transform(path.matrix().ok()?);
            let points: Vec<_> = segments.iter().collect();
            if points.len() != 2
                || points[0].segment_type() != PdfPathSegmentType::MoveTo
                || points[1].segment_type() != PdfPathSegmentType::LineTo
                || !path.is_stroked().ok()?
                || path.stroke_color().ok()?.alpha() == 0
                || (points[0].y().value - points[1].y().value).abs() > 0.5
            {
                return None;
            }
            Some([
                points[0].x().value.min(points[1].x().value),
                points[0].y().value,
                points[0].x().value.max(points[1].x().value),
            ])
        })
        .collect::<Vec<_>>();
    let indices = lines(
        glyphs,
        page.width().value,
        page.height().value,
        &rules,
        omitted,
    );
    if indices.is_empty() {
        return Vec::new();
    }
    let rows: Vec<_> = glyphs.split(|g| g.ch == '\n').collect();
    let local = indices
        .iter()
        .flat_map(|&i| {
            // The outer renderer restores links and inline images against the
            // complete page context. Do not apply them twice in this fragment.
            let mut row: Vec<_> = rows[i]
                .iter()
                .cloned()
                .map(|mut g| {
                    g.link = None;
                    g.image_before = None;
                    g.image_after = None;
                    g
                })
                .collect();
            if let Some(last) = row.last().cloned() {
                row.push(Glyph { ch: '\n', ..last });
            }
            row
        })
        .collect::<Vec<_>>();
    let html = render_region(&local, &[], &[], &HashSet::new(), true).unwrap_or_default();
    // Keep top-level paragraphs, so PairLayout and paragraph edit counters stay
    // unchanged. The browser moves these paragraphs into a separate disclosure.
    let html = html.replace("<p>", "<p data-vellio-footnote=\"true\">");
    if html.is_empty() {
        return Vec::new();
    }
    vec![(indices, html)]
}

fn lines(
    glyphs: &[Glyph],
    width: f32,
    height: f32,
    rules: &[[f32; 3]],
    omitted: &HashSet<usize>,
) -> Vec<usize> {
    let mut body_sizes: Vec<_> = glyphs
        .iter()
        .filter(|g| g.ch.is_alphabetic() && !g.mono && g.y > height * 0.28 && g.y < height * 0.85)
        .map(|g| g.size)
        .collect();
    body_sizes.sort_by(f32::total_cmp);
    let Some(&body_size) = body_sizes.get(body_sizes.len() / 2) else {
        return Vec::new();
    };
    let rows: Vec<_> = glyphs.split(|g| g.ch == '\n').collect();
    for &[left, y, right] in rules {
        if y < height * 0.06
            || y > height * 0.28
            || right - left < width * 0.12
            || right - left > width * 0.55
        {
            continue;
        }
        let indices: Vec<_> = rows
            .iter()
            .enumerate()
            .filter_map(|(i, row)| {
                let visible: Vec<_> = row
                    .iter()
                    .filter(|g| !g.ch.is_whitespace() && !g.ch.is_control())
                    .collect();
                (!omitted.contains(&i)
                    && !visible.is_empty()
                    && visible.iter().all(|g| g.y < y && g.y > height * 0.055))
                .then_some(i)
            })
            .collect();
        let Some(&start) = indices.first() else {
            continue;
        };
        let first = rows[start]
            .iter()
            .find(|g| !g.ch.is_whitespace() && !g.ch.is_control())
            .unwrap();
        if !(first.ch.is_ascii_digit() || matches!(first.ch, '*' | '†' | '‡'))
            || y - first.y > body_size * 3.
            || (first.x - left).abs() > body_size * 3.
        {
            continue;
        }
        let small = indices.iter().all(|&i| {
            let mut sizes: Vec<_> = rows[i]
                .iter()
                .filter(|g| g.ch.is_alphabetic())
                .map(|g| g.size)
                .collect();
            sizes.sort_by(f32::total_cmp);
            sizes
                .get(sizes.len() / 2)
                .is_some_and(|&size| size > 0. && size <= body_size * 0.96)
                && !rows[i].iter().any(|g| g.mono)
        });
        if small {
            return indices;
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Glyph> {
        let mut gs = super::super::tests::test_lines(&[
            (
                "Ordinary body with enough text to determine its font size",
                10.,
                false,
                false,
            ),
            ("4A smaller footnote with wrapped text", 9., false, false),
            ("and its continuation.", 9., false, false),
            ("4", 10., false, false),
        ]);
        let mut row = 0;
        for g in &mut gs {
            g.x -= 32.;
            g.y = [400., 100., 89., 36.][row];
            if g.ch == '\n' {
                row += 1;
            }
        }
        gs
    }

    #[test]
    #[ignore = "requires EASYREAD_QA_PDF pointing to Attention Is All You Need"]
    fn attention_footnote_is_not_body_prose() -> anyhow::Result<()> {
        let path = std::env::var("EASYREAD_QA_PDF")?;
        let pdfium = Pdfium::new(Pdfium::bind_to_library("assets/pdfium/libpdfium.dylib")?);
        let doc = pdfium.load_pdf_from_file(&path, None)?;
        let page = doc.pages().get(3)?;
        let headings = crate::pdf::document_outline(&doc)
            .into_iter()
            .filter(|h| h.page == 4)
            .collect::<Vec<_>>();
        let (_, html, _, _, _) = reading_html(&page, &doc, &headings, 4)?;
        let html = html.unwrap();
        let wrapped = format!("<root>{}</root>", crate::translation_layout::xhtml(&html));
        let parsed = roxmltree::Document::parse(&wrapped)?;
        let notes = parsed
            .descendants()
            .filter(|n| n.attribute("data-vellio-footnote") == Some("true"))
            .collect::<Vec<_>>();
        assert_eq!(notes.len(), 1, "{html}");
        let note = &wrapped[notes[0].range()];
        assert!(
            crate::annotations::plain_text(note).contains("To illustrate why the dot products"),
            "{note}"
        );
        assert!(
            note.contains("data-vellio-math"),
            "Inline footnote math must survive: {note}"
        );
        assert!(!note.contains("Instead of performing"));
        assert_eq!(
            crate::translation_layout::Plan::from_html(&html)?
                .roles()
                .as_array()
                .unwrap()
                .iter()
                .filter(|u| u["kind"] == "footnote")
                .count(),
            1
        );
        Ok(())
    }

    #[test]
    fn requires_a_bottom_rule_marker_and_smaller_type_and_ignores_page_number() {
        let gs = sample();
        assert_eq!(
            lines(&gs, 612., 792., &[[40., 113., 210.]], &HashSet::new()),
            vec![1, 2]
        );
        assert!(lines(&gs, 612., 792., &[], &HashSet::new()).is_empty());
        assert!(lines(&gs, 612., 792., &[[40., 400., 210.]], &HashSet::new()).is_empty());
        assert!(lines(&gs, 612., 792., &[[40., 113., 590.]], &HashSet::new()).is_empty());
        let mut normal = gs.clone();
        for g in &mut normal {
            g.size = 10.;
        }
        assert!(lines(&normal, 612., 792., &[[40., 113., 210.]], &HashSet::new()).is_empty());
        let mut unmarked = gs;
        unmarked.iter_mut().find(|g| g.y == 100.).unwrap().ch = 'A';
        assert!(lines(&unmarked, 612., 792., &[[40., 113., 210.]], &HashSet::new()).is_empty());
    }
}
