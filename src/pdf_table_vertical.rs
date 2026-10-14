//! Captioned tables with one header rule and continuous vertical separators.
//! Row baselines supply the unruled data rows; real paths, not word gaps,
//! supply columns. Unsupported/mixed text remains a positioned-text fallback.
use super::{Cell, Glyph, ReadingTable, cell_html, contains};

struct Grid {
    bounds: [f32; 4],
    columns: Vec<f32>,
    header_bottom: f32,
}

fn grids(glyphs: &[Glyph], paths: &[[f32; 4]]) -> Vec<Grid> {
    let captions: Vec<_> = glyphs
        .split(|g| g.ch == '\n')
        .filter(|line| {
            super::super::caption_start(line)
                && super::super::line_text(line)
                    .trim_start()
                    .starts_with("Table ")
        })
        .filter_map(|line| line.iter().find(|g| !g.ch.is_whitespace()))
        .collect();
    let mut grids = Vec::new();
    for rule in paths
        .iter()
        .filter(|b| b[2] - b[0] >= 120. && b[3] - b[1] <= 2.)
    {
        let header_bottom = (rule[1] + rule[3]) * 0.5;
        let mut verticals: Vec<_> = paths
            .iter()
            .filter(|b| {
                b[2] - b[0] <= 2. && b[3] - b[1] > 2. && b[0] > rule[0] + 2. && b[2] < rule[2] - 2.
            })
            .map(|b| ((b[0] + b[2]) * 0.5, b[1], b[3]))
            .collect();
        verticals.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        let mut separators = Vec::new();
        let mut start = 0;
        while start < verticals.len() {
            let x = verticals[start].0;
            let mut end = start + 1;
            while end < verticals.len() && (verticals[end].0 - x).abs() <= 1. {
                end += 1;
            }
            // Join TeX's per-row strokes, but never bridge a missing segment
            // or two independent tables that happen to share a column edge.
            let mut segments = verticals[start..end].to_vec();
            segments.sort_by(|a, b| a.1.total_cmp(&b.1));
            let mut bottom = segments[0].1;
            let mut top = segments[0].2;
            for &(_, next_bottom, next_top) in &segments[1..] {
                if next_bottom <= top + 1. {
                    top = top.max(next_top);
                } else {
                    if bottom < header_bottom - 2. && top > header_bottom + 2. {
                        separators.push((x, bottom, top));
                    }
                    bottom = next_bottom;
                    top = next_top;
                }
            }
            if bottom < header_bottom - 2. && top > header_bottom + 2. {
                separators.push((x, bottom, top));
            }
            start = end;
        }
        if !(2..=63).contains(&separators.len()) {
            continue;
        }
        let bottom = separators.iter().map(|s| s.1).fold(f32::INFINITY, f32::min);
        let top = separators
            .iter()
            .map(|s| s.2)
            .fold(f32::NEG_INFINITY, f32::max);
        if separators
            .iter()
            .any(|s| (s.1 - bottom).abs() > 1. || (s.2 - top).abs() > 1.)
        {
            continue;
        }
        let bounds = [rule[0], bottom, rule[2], top];
        // This branch handles precisely one header divider. Multi-rule grids
        // stay with the existing ruled/spanning-cell reconstruction.
        if paths.iter().any(|b| {
            b[2] - b[0] >= 120.
                && b[3] - b[1] <= 2.
                && (b[0] - bounds[0]).abs() < 2.
                && (b[2] - bounds[2]).abs() < 2.
                && b[1] > bottom
                && b[3] < top
                && ((b[1] + b[3]) * 0.5 - header_bottom).abs() > 1.
        }) || !captions.iter().any(|g| {
            g.x >= bounds[0] - 1.
                && g.x < bounds[2]
                && ((g.y < bottom && bottom - g.y <= g.size * 8.)
                    || (g.y > top && g.y - top <= g.size * 8.))
        }) {
            continue;
        }
        if grids.iter().any(|g: &Grid| contains(g.bounds, bounds)) {
            continue;
        }
        let mut columns = vec![bounds[0]];
        columns.extend(separators.iter().map(|s| s.0));
        columns.push(bounds[2]);
        grids.push(Grid {
            bounds,
            columns,
            header_bottom,
        });
    }
    grids
}

pub(super) fn regions(glyphs: &[Glyph], paths: &[[f32; 4]]) -> Vec<[f32; 4]> {
    grids(glyphs, paths).into_iter().map(|g| g.bounds).collect()
}

pub(super) fn recover(glyphs: &[Glyph], paths: &[[f32; 4]]) -> Vec<ReadingTable> {
    grids(glyphs, paths)
        .iter()
        .filter_map(|grid| build(glyphs, grid))
        .collect()
}

fn build(glyphs: &[Glyph], grid: &Grid) -> Option<ReadingTable> {
    let inside = |g: &Glyph| contains(grid.bounds, [g.x, g.y, g.right, g.y]);
    let mut claimed = Vec::new();
    let mut visible = Vec::new();
    let mut offset = 0;
    for (i, line) in glyphs.split(|g| g.ch == '\n').enumerate() {
        let indices: Vec<_> = line
            .iter()
            .enumerate()
            .filter(|(_, g)| !g.ch.is_whitespace() && !g.ch.is_control())
            .map(|(j, _)| offset + j)
            .collect();
        offset += line.len() + 1;
        if !indices.iter().any(|&j| inside(&glyphs[j])) {
            continue;
        }
        if indices.iter().any(|&j| !inside(&glyphs[j])) {
            return None;
        }
        claimed.push(i);
        visible.extend(indices);
    }
    let size = glyphs[*visible.first()?].size;
    if size <= 0.
        || visible
            .iter()
            .any(|&i| (glyphs[i].size - size).abs() > size * 0.05)
    {
        return None;
    }
    visible.sort_by(|&a, &b| glyphs[b].y.total_cmp(&glyphs[a].y));
    let mut rows: Vec<(f32, Vec<Vec<usize>>)> = Vec::new();
    let col_count = grid.columns.len() - 1;
    for index in visible {
        let g = &glyphs[index];
        let col = grid
            .columns
            .windows(2)
            .position(|c| g.x >= c[0] - 1. && g.x < c[1])?;
        // No glyph may cross a real column separator. Merged cells and wraps
        // are deliberately not inferred by this simple-table adapter.
        if g.right > grid.columns[col + 1] + 0.5 {
            return None;
        }
        if rows.last().is_none_or(|r| (r.0 - g.y).abs() > size * 0.2) {
            rows.push((g.y, vec![Vec::new(); col_count]));
        }
        rows.last_mut()?.1[col].push(index);
    }
    if rows.len() < 3
        || rows[0].0 <= grid.header_bottom
        || rows[1].0 >= grid.header_bottom
        || rows[0].1.iter().any(Vec::is_empty)
        || rows
            .iter()
            .skip(1)
            .any(|r| r.1[0].is_empty() || r.1.iter().filter(|c| !c.is_empty()).count() < 2)
        || rows
            .windows(2)
            .any(|r| !(size * 0.7..=size * 4.).contains(&(r[0].0 - r[1].0)))
    {
        return None;
    }
    let vlines = (1..col_count)
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let mut html =
        format!("<table data-vellio-table=\"true\" data-hlines=\"1\" data-vlines=\"{vlines}\">");
    let mut slots = Vec::new();
    for (row, (_, cells)) in rows.iter_mut().enumerate() {
        html.push_str("<tr>");
        for (col, indices) in cells.iter_mut().enumerate() {
            // Restore source extraction order for inline math and emphasis.
            indices.sort_unstable();
            let tag = if row == 0 { "th" } else { "td" };
            let width = (grid.columns[col + 1] - grid.columns[col])
                / (grid.bounds[2] - grid.bounds[0])
                * 100.;
            html.push_str(&format!("<{tag} width=\"{width:.3}%\">"));
            let start = html.len();
            html.push_str(&cell_html(
                &indices.iter().map(|&i| &glyphs[i]).collect::<Vec<_>>(),
                col == 0,
            ));
            slots.push(Cell {
                html_range: start..html.len(),
                indices: indices.clone(),
                label: col == 0,
            });
            html.push_str(&format!(" </{tag}>\n"));
        }
        html.push_str("</tr>");
    }
    html.push_str("</table>");
    Some(ReadingTable {
        bounds: grid.bounds,
        lines: claimed,
        html,
        cells: slots,
    })
}
