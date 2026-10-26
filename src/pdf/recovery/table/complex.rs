//! Recover sparse and spanning academic tables from physical rows and rules.
//! No paper-specific words or numbers: the fullest data row seeds columns,
//! other rows refine gutters, and ruled bands supply group rowspans.
use super::{Cell, ReadingTable, cell_html, contains};
use crate::pdf::recovery::Glyph;

#[derive(Clone)]
struct Run {
    indices: Vec<usize>,
    left: f32,
    right: f32,
    y: f32,
}
fn runs(glyphs: &[Glyph], indices: &[usize], size: f32) -> Vec<Run> {
    let mut sorted = indices.to_vec();
    sorted.sort_by(|&a, &b| glyphs[a].x.total_cmp(&glyphs[b].x));
    let mut runs: Vec<Run> = Vec::new();
    for i in sorted {
        let g = &glyphs[i];
        if runs.last().is_none_or(|r| g.x - r.right > size * 0.75) {
            runs.push(Run {
                indices: vec![],
                left: g.x,
                right: g.right,
                y: g.y,
            });
        }
        let r = runs.last_mut().unwrap();
        r.indices.push(i);
        r.right = r.right.max(g.advance_right.max(g.right));
    }
    runs
}
fn center(r: &Run) -> f32 {
    (r.left + r.right) * 0.5
}
fn seeded_column(r: &Run, seeds: &[f32], size: f32) -> usize {
    if (r.left - seeds[0]).abs() < size {
        return 0;
    }
    (1..seeds.len())
        .min_by(|&a, &b| {
            (center(r) - seeds[a])
                .abs()
                .total_cmp(&(center(r) - seeds[b]).abs())
        })
        .unwrap_or(0)
}
/// Honest non-image fallback: keep physical rows/column spacing as selectable
/// text, clearly marked as unrecovered. Never invent a semantic cell grid.
pub(super) fn text_fallback(glyphs: &[Glyph], bounds: [f32; 4]) -> Option<ReadingTable> {
    let inside = |g: &Glyph| contains(bounds, [g.x, g.y, g.right, g.y]);
    let mut lines = Vec::new();
    for (i, line) in glyphs.split(|g| g.ch == '\n').enumerate() {
        let visible: Vec<_> = line
            .iter()
            .filter(|g| !g.ch.is_whitespace() && !g.ch.is_control())
            .collect();
        if visible.iter().any(|g| inside(g)) {
            // Do not swallow neighboring prose in a mixed extraction record.
            if !visible.iter().all(|g| inside(g)) {
                return None;
            }
            lines.push(i);
        }
    }
    if lines.is_empty() {
        return None;
    }
    let visible: Vec<_> = glyphs
        .iter()
        .enumerate()
        .filter(|(_, g)| !g.ch.is_whitespace() && !g.ch.is_control() && inside(g))
        .map(|(i, _)| i)
        .collect();
    let mut sizes = std::collections::BTreeMap::<i32, usize>::new();
    for &i in &visible {
        *sizes
            .entry((glyphs[i].size * 10.).round() as i32)
            .or_default() += 1;
    }
    let size = sizes.into_iter().max_by_key(|(_, n)| *n)?.0 as f32 / 10.;
    if size <= 0. {
        return None;
    }
    let mut baselines: Vec<_> = visible.iter().map(|&i| glyphs[i].y).collect();
    baselines.sort_by(|a, b| b.total_cmp(a));
    baselines.dedup_by(|a, b| (*a - *b).abs() < size * 0.4);
    let mut rows = vec![Vec::new(); baselines.len()];
    for i in visible {
        let row = (0..baselines.len()).min_by(|&a, &b| {
            (glyphs[i].y - baselines[a])
                .abs()
                .total_cmp(&(glyphs[i].y - baselines[b]).abs())
        })?;
        rows[row].push(i);
    }
    let mut html = String::from(
        "<p data-vellio-table-unrecovered=\"true\">Table structure not recovered; original positioned text follows.</p><pre><code>",
    );
    for row in rows {
        let mut width = 0;
        for run in runs(glyphs, &row, size) {
            let column = ((run.left - bounds[0]) / (size * 0.5))
                .round()
                .clamp(0., 4096.) as usize;
            let pad = column.saturating_sub(width);
            html.push_str(&" ".repeat(pad));
            let cell = cell_html(
                &run.indices.iter().map(|&i| &glyphs[i]).collect::<Vec<_>>(),
                false,
            );
            width += pad + crate::annotations::plain_text(&cell).chars().count();
            html.push_str(&cell);
        }
        html.push('\n');
    }
    html.push_str("</code></pre>");
    Some(ReadingTable {
        lines,
        bounds,
        html,
        cells: vec![],
    })
}

struct Positioned {
    row: usize,
    col: usize,
    rowspan: usize,
    colspan: usize,
    indices: Vec<usize>,
}

pub(super) fn build(
    glyphs: &[Glyph],
    paths: &[[f32; 4]],
    bounds: [f32; 4],
) -> Option<ReadingTable> {
    let mut size_bins = std::collections::BTreeMap::<i32, usize>::new();
    let visible: Vec<_> = glyphs
        .iter()
        .enumerate()
        .filter(|(_, g)| {
            !g.ch.is_whitespace()
                && !g.ch.is_control()
                && contains(bounds, [g.x, g.y, g.right, g.y])
        })
        .map(|(i, _)| i)
        .collect();
    for &i in &visible {
        *size_bins
            .entry((glyphs[i].size * 10.).round() as i32)
            .or_default() += 1;
    }
    let size = size_bins.into_iter().max_by_key(|(_, n)| *n)?.0 as f32 / 10.;
    if size <= 0. {
        return None;
    }
    let mut rules: Vec<_> = paths
        .iter()
        .filter(|b| {
            b[3] - b[1] <= 2. && (b[0] - bounds[0]).abs() < 2. && (b[2] - bounds[2]).abs() < 2.
        })
        .map(|b| (b[1] + b[3]) * 0.5)
        .collect();
    rules.sort_by(|a, b| b.total_cmp(a));
    rules.dedup_by(|a, b| (*a - *b).abs() < 1.);
    if rules.len() < 3 {
        return None;
    }
    let header_bottom = rules[1];
    // Normal-sized baselines, excluding raised/sunken scripts. Rows with just
    // a group label are not extra data rows; they belong to a ruled band.
    let mut baseline_bins = std::collections::BTreeMap::<i32, Vec<usize>>::new();
    for &i in &visible {
        let g = &glyphs[i];
        if g.y < header_bottom && (g.size - size).abs() < size * 0.05 {
            baseline_bins
                .entry((g.y * 5.).round() as i32)
                .or_default()
                .push(i);
        }
    }
    let mut data: Vec<_> = baseline_bins
        .values()
        .map(|v| runs(glyphs, v, size))
        .filter(|r| r.len() >= 2)
        .collect();
    data.sort_by(|a, b| b[0].y.total_cmp(&a[0].y));
    let template = data.iter().max_by_key(|row| row.len())?;
    let mut seeds: Vec<_> = template
        .iter()
        .enumerate()
        .map(|(i, r)| if i == 0 { r.left } else { center(r) })
        .collect();
    // A sparse table may have no fully populated data row. Leaf headers still
    // define columns whose values are blank everywhere in this sample.
    let mut header_bins = std::collections::BTreeMap::<i32, Vec<usize>>::new();
    for &i in &visible {
        let g = &glyphs[i];
        if g.y >= header_bottom && (g.size - size).abs() < size * 0.05 {
            header_bins
                .entry((g.y * 5.).round() as i32)
                .or_default()
                .push(i);
        }
    }
    let leaves = header_bins
        .values()
        .map(|v| runs(glyphs, v, size))
        .max_by_key(Vec::len)
        .unwrap_or_default();
    let first_in_leaves = leaves
        .first()
        .is_some_and(|r| (r.left - seeds[0]).abs() < size);
    let header_count = leaves.len() + usize::from(!first_in_leaves);
    let use_headers = header_count > seeds.len();
    if use_headers {
        let first = seeds[0];
        seeds = leaves.iter().map(center).collect();
        if !first_in_leaves {
            seeds.insert(0, first);
        } else {
            seeds[0] = first;
        }
    }
    let col_count = seeds.len();
    if !(3..=64).contains(&col_count) || data.len() < 2 {
        return None;
    }
    let mut left = vec![f32::INFINITY; col_count];
    let mut right = vec![-f32::INFINITY; col_count];
    if use_headers {
        for (i, r) in leaves.iter().enumerate() {
            let c = i + usize::from(!first_in_leaves);
            left[c] = r.left;
            right[c] = r.right;
        }
    }
    for row in &data {
        for run in row {
            let c = seeded_column(run, &seeds, size);
            let local_gap = if c == 0 {
                seeds[1] - seeds[0]
            } else if c + 1 == col_count {
                seeds[c] - seeds[c - 1]
            } else {
                (seeds[c + 1] - seeds[c]).min(seeds[c] - seeds[c - 1])
            };
            // A wide centered note / merged cost must not erase column gutters.
            if c > 0
                && (run.right - run.left > local_gap * 1.35
                    || (center(run) - seeds[c]).abs() > size * 0.7)
            {
                continue;
            }
            left[c] = left[c].min(run.left);
            right[c] = right[c].max(run.right);
        }
    }
    let mut columns = vec![bounds[0]];
    for c in 1..col_count {
        if !right[c - 1].is_finite() || !left[c].is_finite() || right[c - 1] >= left[c] {
            return None;
        }
        columns.push((right[c - 1] + left[c]) * 0.5);
    }
    columns.push(bounds[2]);
    let partial: Vec<_> = paths
        .iter()
        .filter(|b| {
            b[3] - b[1] <= 2.
                && b[2] - b[0] > size * 2.
                && b[1] > header_bottom + 1.
                && b[3] < bounds[3] - 1.
                && (b[0] > bounds[0] + 2. || b[2] < bounds[2] - 2.)
        })
        .collect();
    let header_rows = if partial.is_empty() { 1 } else { 2 };
    let mut edges = vec![bounds[3]];
    if header_rows == 2 {
        edges.push(partial.iter().map(|b| (b[1] + b[3]) * 0.5).sum::<f32>() / partial.len() as f32);
    }
    edges.push(header_bottom);
    // Actual rules separate bands; midpoint boundaries separate physical rows
    // inside a band. This prevents centered group labels from becoming rows.
    for pair in data.windows(2) {
        let high = pair[0][0].y;
        let low = pair[1][0].y;
        edges.push(
            rules
                .iter()
                .copied()
                .find(|&y| y < high && y > low)
                .unwrap_or((high + low) * 0.5),
        );
    }
    edges.push(bounds[1]);
    let row_count = edges.len() - 1;
    let mut positioned: Vec<Positioned> = vec![];
    let mut occupied = vec![vec![false; col_count]; row_count];
    // Assign all scripts and references to their nearest body baseline. Their
    // extraction newlines do not define new rows or change their cell.
    for row in 0..row_count {
        let header = row < header_rows;
        let y = if header {
            (edges[row] + edges[row + 1]) * 0.5
        } else {
            data[row - header_rows][0].y
        };
        let row_indices: Vec<_> = visible
            .iter()
            .copied()
            .filter(|&i| {
                let g = &glyphs[i];
                if header {
                    g.y >= header_bottom
                        && g.y < bounds[3]
                        && (header_rows == 1 || (g.y < edges[row] && g.y >= edges[row + 1]))
                } else {
                    g.y < header_bottom
                        && (g.y - y).abs() <= size * 0.55
                        && data
                            .iter()
                            .enumerate()
                            .min_by(|(_, a), (_, b)| {
                                (g.y - a[0].y).abs().total_cmp(&(g.y - b[0].y).abs())
                            })
                            .is_some_and(|(j, _)| j == row - header_rows)
                }
            })
            .collect();
        let row_runs = runs(glyphs, &row_indices, size);
        for run in &row_runs {
            let first =
                (0..col_count).find(|&c| run.left < columns[c + 1] && run.right > columns[c])?;
            let last = (first..col_count)
                .rev()
                .find(|&c| run.right > columns[c] && run.left < columns[c + 1])?;
            let mut col = first;
            let mut colspan = last - first + 1;
            let mut start = row;
            let mut rowspan = 1;
            if !header && first == 0 {
                let band_high = rules
                    .iter()
                    .copied()
                    .filter(|&r| r > run.y)
                    .min_by(f32::total_cmp)?;
                let band_low = rules
                    .iter()
                    .copied()
                    .filter(|&r| r < run.y)
                    .max_by(f32::total_cmp)?;
                let band: Vec<_> = data
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| r[0].y < band_high && r[0].y > band_low)
                    .map(|(i, _)| i + header_rows)
                    .collect();
                let label_count = visible
                    .iter()
                    .filter(|&&i| {
                        glyphs[i].x < columns[1]
                            && glyphs[i].y < band_high
                            && glyphs[i].y > band_low
                            && (glyphs[i].size - size).abs() < size * 0.05
                    })
                    .count();
                if band.len() > 1 && label_count == run.indices.len() {
                    start = *band.first()?;
                    rowspan = band.len();
                }
            }
            // A multi-column note occupies the surrounding empty columns up to
            // the next populated cell (e.g. the positional-embedding variant).
            if !header && colspan > 1 && run.right - run.left > size * 5. {
                col = row_runs
                    .iter()
                    .filter(|r| r.right < run.left)
                    .map(|r| {
                        (0..col_count)
                            .rev()
                            .find(|&c| r.right > columns[c])
                            .unwrap_or(0)
                            + 1
                    })
                    .max()
                    .unwrap_or(0);
                let end = row_runs
                    .iter()
                    .filter(|r| r.left > run.right)
                    .filter_map(|r| (0..col_count).find(|&c| r.left < columns[c + 1]))
                    .min()
                    .unwrap_or(col_count);
                colspan = end - col;
            }
            if header && header_rows == 2 && first == 0 {
                start = 0;
                rowspan = 2;
            }
            if (start..start + rowspan).any(|r| (col..col + colspan).any(|c| occupied[r][c])) {
                return None;
            }
            for cells in occupied.iter_mut().skip(start).take(rowspan) {
                for used in cells.iter_mut().skip(col).take(colspan) {
                    *used = true;
                }
            }
            positioned.push(Positioned {
                row: start,
                col,
                rowspan,
                colspan,
                indices: run.indices.clone(),
            });
        }
    }
    // Include singleton group labels halfway between body rows, not collected
    // by the nearest-row threshold above.
    let assigned: std::collections::HashSet<_> = positioned
        .iter()
        .flat_map(|c| c.indices.iter().copied())
        .collect();
    for band in rules.windows(2).skip(1) {
        let missing: Vec<_> = visible
            .iter()
            .copied()
            .filter(|i| {
                !assigned.contains(i)
                    && glyphs[*i].x < columns[1]
                    && glyphs[*i].y < band[0]
                    && glyphs[*i].y > band[1]
            })
            .collect();
        if missing.is_empty() {
            continue;
        }
        let rows: Vec<_> = data
            .iter()
            .enumerate()
            .filter(|(_, r)| r[0].y < band[0] && r[0].y > band[1])
            .map(|(i, _)| i + header_rows)
            .collect();
        let start = *rows.first()?;
        if rows.iter().any(|&r| occupied[r][0]) {
            return None;
        }
        for &r in &rows {
            occupied[r][0] = true;
        }
        positioned.push(Positioned {
            row: start,
            col: 0,
            rowspan: rows.len(),
            colspan: 1,
            indices: missing,
        });
    }
    let assigned: std::collections::HashSet<_> = positioned
        .iter()
        .flat_map(|c| c.indices.iter().copied())
        .collect();
    if visible.iter().any(|i| !assigned.contains(i)) {
        return None;
    }
    let hlines: Vec<_> = (0..=row_count)
        .filter(|&i| i == 0 || i == row_count || rules.iter().any(|&y| (y - edges[i]).abs() < 1.))
        .map(|i| i.to_string())
        .collect();
    let vlines: Vec<_> = (1..col_count)
        .filter(|&c| {
            // TeX often paints a continuous vertical rule as one path per
            // row/band. Measure their union, not just the longest segment.
            let mut segments: Vec<_> = paths
                .iter()
                .filter(|b| {
                    b[2] - b[0] <= 2.
                        && b[3] - b[1] > 2.
                        && ((b[0] + b[2]) * 0.5 - columns[c]).abs() < size * 0.4
                })
                .map(|b| (b[1].max(bounds[1]), b[3].min(bounds[3])))
                .filter(|(a, b)| b > a)
                .collect();
            segments.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut end = bounds[1];
            let mut covered = 0.;
            for (a, b) in segments {
                covered += (b - a.max(end)).max(0.);
                end = end.max(b);
            }
            covered > (bounds[3] - bounds[1]) * 0.8
        })
        .map(|i| i.to_string())
        .collect();
    let mut html = format!(
        "<table data-vellio-table=\"true\" data-hlines=\"{}\" data-vlines=\"{}\">",
        hlines.join(","),
        vlines.join(",")
    );
    let mut slots = vec![];
    for (row, occupied_row) in occupied.iter().enumerate() {
        html.push_str("<tr>");
        for (col, &is_occupied) in occupied_row.iter().enumerate() {
            let p = positioned.iter().find(|p| p.row == row && p.col == col);
            if p.is_none() && is_occupied {
                continue;
            }
            let tag = if row < header_rows { "th" } else { "td" };
            let (rowspan, colspan) = p.map_or((1, 1), |p| (p.rowspan, p.colspan));
            let width = (columns[col + colspan] - columns[col]) / (bounds[2] - bounds[0]) * 100.;
            html.push_str(&format!("<{tag} width=\"{width:.3}%\" rowspan=\"{rowspan}\" colspan=\"{colspan}\" data-col=\"{col}\">"));
            let start = html.len();
            let mut indices = p.map_or_else(Vec::new, |p| p.indices.clone());
            // Original extraction order is retained for formula ranges. Add
            // geometric word spaces where PDFium supplied synthetic breaks.
            indices.sort_unstable();
            let refs: Vec<_> = indices.iter().map(|&i| &glyphs[i]).collect();
            html.push_str(&cell_html(&refs, col == 0));
            slots.push(Cell {
                html_range: start..html.len(),
                indices,
                label: col == 0,
            });
            html.push_str(&format!(" </{tag}>\n"));
        }
        html.push_str("</tr>");
    }
    html.push_str("</table>");
    let lines = glyphs
        .split(|g| g.ch == '\n')
        .enumerate()
        .filter(|(_, line)| {
            line.iter().any(|g| {
                !g.ch.is_whitespace()
                    && !g.ch.is_control()
                    && contains(bounds, [g.x, g.y, g.right, g.y])
            })
        })
        .map(|(i, _)| i)
        .collect();
    Some(ReadingTable {
        bounds,
        lines,
        html,
        cells: slots,
    })
}
