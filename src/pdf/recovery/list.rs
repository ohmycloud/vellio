//! Recover list structure from text markers and small vector markers next to
//! text baselines. Never add synthetic markers to persisted PDF extraction.
use super::{Glyph, line_text};
use pdfium_render::prelude::*;
use std::collections::{HashMap, HashSet};

pub(super) struct ReadingList {
    pub lines: Vec<usize>,
    pub objects: Vec<usize>,
    pub html: String,
}

struct Marker {
    start: usize,
    body: usize,
    object: Option<usize>,
}

pub(super) fn recover(
    page: &PdfPage<'_>,
    glyphs: &[Glyph],
    omitted: &HashSet<usize>,
) -> Vec<ReadingList> {
    let shapes: Vec<_> = page
        .objects()
        .iter()
        .enumerate()
        .filter_map(|(index, o)| {
            let path = o.as_path_object()?;
            // A filled circle / square, or a closed hollow circle / square.
            // Exclude invisible paths, line segments and complex small icons.
            let fill = path.fill_mode().ok()? != PdfPathFillMode::None
                && path.fill_color().ok()?.alpha() > 0;
            let stroke = path.is_stroked().ok()? && path.stroke_color().ok()?.alpha() > 0;
            if !fill && !stroke {
                return None;
            }
            let path_segments = path.segments();
            let segments: Vec<_> = path_segments.iter().collect();
            let moves = segments
                .iter()
                .filter(|s| s.segment_type() == PdfPathSegmentType::MoveTo)
                .count();
            let curves = segments
                .iter()
                .filter(|s| s.segment_type() == PdfPathSegmentType::BezierTo)
                .count();
            let square = segments.len() == 5 && segments.last().is_some_and(|s| s.is_close());
            if moves != 1 || !(curves >= 6 && segments.len() <= 17 || square) {
                return None;
            }
            if !fill && !segments.last().is_some_and(|s| s.is_close()) {
                return None;
            }
            let b = o.bounds().ok()?;
            Some((
                index,
                [
                    b.left().value,
                    b.bottom().value,
                    b.right().value,
                    b.top().value,
                ],
            ))
        })
        .collect();
    from_shapes(glyphs, &shapes, omitted)
}

fn from_shapes(
    glyphs: &[Glyph],
    shapes: &[(usize, [f32; 4])],
    omitted: &HashSet<usize>,
) -> Vec<ReadingList> {
    let lines: Vec<_> = glyphs.split(|g| g.ch == '\n').collect();
    let mut markers = HashMap::new();
    for (i, line) in lines
        .iter()
        .enumerate()
        .filter(|(i, _)| !omitted.contains(i))
    {
        let text = line_text(line);
        let trimmed = text.trim_start();
        if let Some(item) = crate::model::pdf_unordered_item(trimmed) {
            let skip = text.chars().count() - item.chars().count();
            let body = line
                .iter()
                .enumerate()
                .filter(|(_, g)| !g.ch.is_control() || g.ch == '\t')
                .nth(skip)
                .map(|(j, _)| j);
            if let Some(body) = body.filter(|&j| !super::is_code(&line[j..])) {
                markers.insert(
                    i,
                    Marker {
                        start: i,
                        body,
                        object: None,
                    },
                );
            }
            continue;
        }
        let Some((body, first)) = line
            .iter()
            .enumerate()
            .find(|(_, g)| !g.ch.is_whitespace() && !g.ch.is_control())
        else {
            continue;
        };
        if first.mono || first.size <= 0. || super::caption_start(line) {
            continue;
        }
        // A PDF extraction line that contains two separate baselines/cells
        // must not be claimed as one list item.
        if line.iter().any(|g| {
            !g.ch.is_whitespace() && !g.ch.is_control() && (g.y - first.y).abs() > first.size * 0.3
        }) {
            continue;
        }
        if let Some(&(index, _)) = shapes.iter().find(|(_, b)| {
            let w = b[2] - b[0];
            let h = b[3] - b[1];
            w >= first.size * 0.15
                && w <= first.size * 0.65
                && h >= first.size * 0.15
                && h <= first.size * 0.65
                && (w - h).abs() <= first.size * 0.12
                && first.x - b[2] >= first.size * 0.15
                && first.x - b[2] <= first.size * 2.
                && ((b[1] + b[3]) / 2. - first.y - first.size * 0.3).abs() <= first.size * 0.2
        }) {
            markers.insert(
                i,
                Marker {
                    start: i,
                    body,
                    object: Some(index),
                },
            );
        }
    }
    let mut lists = vec![];
    let mut i = 0;
    while i < lines.len() {
        let Some(first) = markers.get(&i) else {
            i += 1;
            continue;
        };
        let root = lines[i][first.body].x;
        let mut claimed = vec![];
        let mut objects = vec![];
        let mut items: Vec<(f32, f32, Vec<&Glyph>)> = vec![];
        loop {
            let marker = &markers[&i];
            let body = &lines[i][marker.body..];
            let first = &body[0];
            let (left, size) = (first.x, first.size);
            if let Some(index) = marker.object {
                objects.push(index);
            }
            let mut item: Vec<_> = body.iter().collect();
            claimed.push(marker.start);
            let mut previous = first;
            i += 1;
            // Only physical wraps with the item's hanging indent belong to
            // it. A return to the body margin ends the list even without a
            // final period; omissions (tables/diagrams) are hard boundaries.
            while i < lines.len() && !omitted.contains(&i) && !markers.contains_key(&i) {
                let Some(next) = lines[i]
                    .iter()
                    .find(|g| !g.ch.is_whitespace() && !g.ch.is_control())
                else {
                    break;
                };
                if next.mono
                    || (next.x - left).abs() > size * 0.5
                    || (next.size - size).abs() > size * 0.1
                    || previous.y - next.y < size * 0.5
                    || previous.y - next.y > size * 1.7
                    || super::caption_start(lines[i])
                {
                    break;
                }
                item.extend(lines[i]);
                claimed.push(i);
                previous = next;
                i += 1;
            }
            items.push((left, size, item));
            let Some(next) = markers.get(&i).filter(|_| !omitted.contains(&i)) else {
                break;
            };
            let next = &lines[i][next.body];
            if next.x < root - size * 0.5
                || next.x - left > size * 6.
                || previous.y - next.y < size * 0.5
                || previous.y - next.y > size * 4.
                || (next.size - size).abs() > size * 0.15
            {
                break;
            }
        }
        let mut cursor = 0;
        let html = render_items(&items, &mut cursor, root);
        lists.push(ReadingList {
            lines: claimed,
            objects,
            html,
        });
    }
    lists
}

fn render_items(items: &[(f32, f32, Vec<&Glyph>)], cursor: &mut usize, left: f32) -> String {
    let mut html = String::from("<ul>");
    while let Some((x, size, glyphs)) = items.get(*cursor) {
        if *x < left - size * 0.5 {
            break;
        }
        html.push_str("<li>");
        html.push_str(&super::table::cell_html(glyphs, false));
        *cursor += 1;
        if let Some((next, _, _)) = items
            .get(*cursor)
            .filter(|(x, _, _)| *x > left + size * 0.5)
        {
            html.push_str(&render_items(items, cursor, *next));
        }
        html.push_str("</li>");
    }
    html.push_str("</ul>");
    html
}

#[cfg(test)]
mod tests {
    use super::*;
    fn line(s: &str, x: f32, y: f32) -> Vec<Glyph> {
        s.chars()
            .chain(['\n'])
            .enumerate()
            .map(|(i, ch)| Glyph {
                ch,
                mono: false,
                x: x + i as f32 * 5.,
                right: x + (i + 1) as f32 * 5.,
                advance_right: x + (i + 1) as f32 * 5.,
                y,
                size: 12.,
                color: 0,
                italic: false,
                bold: false,
                image_before: None,
                image_after: None,
                link: None,
            })
            .collect()
    }
    #[test]
    fn vector_bullets_keep_wrapped_nested_items_and_following_prose_outside() {
        let mut glyphs = vec![];
        for (s, x, y) in [
            ("First & main", 114., 720.),
            ("wraps here", 114., 706.),
            ("Nested item", 138., 684.),
            ("Last item", 114., 660.),
            ("Following body without a full stop", 72., 636.),
        ] {
            glyphs.extend(line(s, x, y));
        }
        let shapes = [
            (0, [102., 721.2, 106.8, 726.]),
            (1, [126., 685.2, 130.8, 690.]),
            (2, [102., 661.2, 106.8, 666.]),
        ];
        let lists = from_shapes(&glyphs, &shapes, &HashSet::new());
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].lines, vec![0, 1, 2, 3]);
        assert_eq!(
            lists[0].html,
            "<ul><li>First &amp; main wraps here<ul><li>Nested item</li></ul></li><li>Last item</li></ul>"
        );
    }
    #[test]
    fn text_bullets_and_singletons_use_list_structure_but_code_and_diagrams_do_not() {
        let mut glyphs = line("• First item", 72., 720.);
        glyphs.extend(line("▪ Second item", 72., 700.));
        let lists = from_shapes(&glyphs, &[], &HashSet::new());
        assert_eq!(lists.len(), 1);
        assert_eq!(
            lists[0].html,
            "<ul><li>First item</li><li>Second item</li></ul>"
        );
        assert!(from_shapes(&glyphs, &[], &HashSet::from([0, 1])).is_empty());
        for g in &mut glyphs {
            g.mono = true;
        }
        assert!(from_shapes(&glyphs, &[], &HashSet::new()).is_empty());
        let glyphs = line("One item", 114., 720.);
        assert_eq!(
            from_shapes(&glyphs, &[(0, [102., 721.2, 106.8, 726.])], &HashSet::new()).len(),
            1
        );
        for b in [
            [102., 690., 106.8, 695.],
            [120., 721.2, 124.8, 726.],
            [72., 721.2, 76.8, 726.],
            [102., 721.2, 110.8, 722.],
        ] {
            assert!(
                from_shapes(&glyphs, &[(0, b)], &HashSet::new()).is_empty(),
                "{b:?}"
            );
        }
    }
}
