//! Conservative PDF table recovery. Geometry supplies semantic cells and spans;
//! simple HTML tables and complex native Typst tables share the same text data.
use super::{Glyph, escape};
use pdfium_render::prelude::*;

#[path = "pdf_table_complex.rs"]
mod complex;
#[path = "pdf_table_vertical.rs"]
mod vertical;

pub(super) fn text_fallback(glyphs: &[Glyph], bounds: [f32; 4]) -> Option<ReadingTable> {
    complex::text_fallback(glyphs, bounds)
}

pub(super) struct ReadingTable {
    pub bounds: [f32; 4],
    pub lines: Vec<usize>,
    pub html: String,
    cells: Vec<Cell>,
}

struct Cell {
    html_range: std::ops::Range<usize>,
    indices: Vec<usize>,
    label: bool,
}

impl ReadingTable {
    /// Place math by original glyph indices inside its own cell, never by a
    /// page-wide text search (many cells contain the identical O(1)).
    pub fn restore_math(&mut self, glyphs: &[Glyph], math: &[super::math::ReadingMath]) {
        for cell in self.cells.iter().rev() {
            // Bold scientific notation can be split into several math-font
            // runs. Reconstruct the complete numeric cell from the raised
            // exponent, rather than leaving a baseline "10 18" behind.
            let visible: Vec<_> = cell
                .indices
                .iter()
                .map(|&i| &glyphs[i])
                .filter(|g| !g.ch.is_whitespace() && !g.ch.is_control())
                .collect();
            let compact: String = visible.iter().map(|g| g.ch).collect();
            if let Some((base, power)) = compact.split_once(['·', '×'])
                && base.parse::<f64>().is_ok()
                && let Some(exp) = power
                    .strip_prefix("10")
                    .filter(|e| !e.is_empty() && e.chars().all(|c| c.is_ascii_digit()))
                && let Some(first) = visible.first()
                && visible
                    .iter()
                    .rev()
                    .take(exp.chars().count())
                    .all(|g| g.size < first.size * 0.85 && g.y - first.y > first.size * 0.1)
            {
                let tex = format!("{base}\\cdot 10^{{{exp}}}");
                if crate::formula::layout(&tex, false).is_ok() {
                    let object = crate::formula::native_html(&tex, false);
                    let object = if visible
                        .iter()
                        .filter(|g| g.size >= first.size * 0.9)
                        .all(|g| g.bold)
                    {
                        format!("<strong>{object}</strong>")
                    } else {
                        object
                    };
                    self.html.replace_range(cell.html_range.clone(), &object);
                    continue;
                }
            }
            let expressions: Vec<_> = math
                .iter()
                .filter(|m| {
                    !m.presentation.display
                        && cell.indices.contains(&m.range.start)
                        && cell.indices.contains(&(m.range.end - 1))
                })
                .collect();
            if expressions.is_empty() {
                continue;
            }
            let mut html = String::new();
            let mut run = Vec::new();
            let append_run = |html: &mut String, run: &[&Glyph]| {
                // A run after a formula may begin with an explicit word space;
                // cell_html normally trims that space at the start of a cell.
                if run.first().is_some_and(|g| g.ch.is_whitespace()) && !html.is_empty() {
                    html.push(' ');
                }
                html.push_str(&cell_html(run, cell.label));
            };
            for &index in &cell.indices {
                if let Some(m) = expressions.iter().find(|m| m.range.contains(&index)) {
                    if index == m.range.start {
                        append_run(&mut html, &run);
                        run.clear();
                        html.push_str(&crate::formula::native_html(&m.presentation.tex, false));
                    }
                } else {
                    run.push(&glyphs[index]);
                }
            }
            append_run(&mut html, &run);
            self.html.replace_range(cell.html_range.clone(), &html);
        }
    }
}

pub(super) fn contains(outer: [f32; 4], inner: [f32; 4]) -> bool {
    inner[0] >= outer[0] - 1.
        && inner[1] >= outer[1] - 1.
        && inner[2] <= outer[2] + 1.
        && inner[3] <= outer[3] + 1.
}

fn page_rules(page: &PdfPage<'_>) -> Vec<[f32; 4]> {
    page.objects()
        .iter()
        .filter_map(|o| {
            o.as_path_object()?;
            let b = o.bounds().ok()?;
            Some([
                b.left().value,
                b.bottom().value,
                b.right().value,
                b.top().value,
            ])
        })
        .collect()
}

pub(super) fn recover(page: &PdfPage<'_>, glyphs: &[Glyph]) -> Vec<ReadingTable> {
    let rules = page_rules(page);
    let regions = source_regions(glyphs, &rules);
    let mut tables = from_rules(glyphs, &rules);
    tables.retain(|t| {
        !regions
            .iter()
            .any(|r| contains(*r, t.bounds) && !contains(t.bounds, *r))
    });
    for region in regions {
        if !tables.iter().any(|t| contains(t.bounds, region))
            && let Some(table) = complex::build(glyphs, &rules, region)
        {
            tables.push(table);
        }
    }
    // Illustrated cells are not yet semantic chart data. Mark them unrecovered
    // rather than pretending a text-only grid is complete.
    tables.retain(|t| {
        !page.objects().iter().any(|o| {
            matches!(o, PdfPageObject::Image(_) | PdfPageObject::XObjectForm(_))
                && o.bounds().ok().is_some_and(|b| {
                    contains(
                        t.bounds,
                        [
                            b.left().value,
                            b.bottom().value,
                            b.right().value,
                            b.top().value,
                        ],
                    )
                })
        })
    });
    tables
}

/// Region detection is deliberately independent of cell reconstruction. A
/// merged header, empty cell, or unsupported column count must not cause the
/// table to disappear into prose. These regions can use a source-faithful view.
pub(super) fn regions(page: &PdfPage<'_>, glyphs: &[Glyph]) -> Vec<[f32; 4]> {
    source_regions(glyphs, &page_rules(page))
}

fn source_regions(glyphs: &[Glyph], paths: &[[f32; 4]]) -> Vec<[f32; 4]> {
    let mut borders: Vec<_> = paths
        .iter()
        .copied()
        .filter(|b| b[2] - b[0] >= 120. && b[3] - b[1] <= 2.)
        .collect();
    borders.sort_by(|a, b| b[3].total_cmp(&a[3]));
    let lines: Vec<_> = glyphs.split(|g| g.ch == '\n').collect();
    let captions: Vec<_> = lines
        .iter()
        .filter_map(|line| {
            (super::caption_start(line)
                && super::line_text(line).trim_start().starts_with("Table "))
            .then(|| line.iter().find(|g| !g.ch.is_whitespace()))
            .flatten()
        })
        .collect();
    let mut regions = vertical::regions(glyphs, paths);
    for (i, top) in borders.iter().enumerate() {
        if regions.iter().any(|b| contains(*b, *top)) {
            continue;
        }
        let mut matching = vec![*top];
        for bottom in &borders[i + 1..] {
            if (bottom[0] - top[0]).abs() > 2. || (bottom[2] - top[2]).abs() > 2. {
                continue;
            }
            // A new caption separates tables even when their widths match.
            if captions
                .iter()
                .any(|g| g.y < matching.last().unwrap()[1] && g.y > bottom[3])
            {
                break;
            }
            matching.push(*bottom);
        }
        if matching.len() < 3 {
            continue;
        }
        let bottom = matching.last().unwrap();
        let bounds = [top[0], bottom[1], top[2], top[3]];
        if !captions.iter().any(|g| {
            (g.y > bounds[3] && g.y - bounds[3] <= g.size * 8.)
                || (g.y < bounds[1] && bounds[1] - g.y <= g.size * 8.)
        }) {
            continue;
        }
        let mut rows = std::collections::BTreeMap::<i32, Vec<&Glyph>>::new();
        let mut crossing = false;
        for line in &lines {
            let visible: Vec<_> = line
                .iter()
                .filter(|g| !g.ch.is_whitespace() && !g.ch.is_control())
                .collect();
            let inside = |g: &&Glyph| contains(bounds, [g.x, g.y, g.right, g.y]);
            if visible.iter().any(inside) && !visible.iter().all(inside) {
                crossing = true;
                break;
            }
            for g in visible.into_iter().filter(inside) {
                rows.entry((g.y * 5.).round() as i32).or_default().push(g);
            }
        }
        // Require repeated side-by-side text runs, not just ruled paragraphs.
        let aligned_rows = rows
            .values()
            .filter(|row| {
                row.windows(2)
                    .any(|p| p[1].x - p[0].advance_right > p[0].size * 1.2)
            })
            .count();
        if !crossing && aligned_rows >= 2 {
            regions.push(bounds);
        }
    }
    regions
}

struct Rule {
    y: f32,
    left: f32,
    right: f32,
    edges: Vec<f32>,
}

fn from_rules(glyphs: &[Glyph], paths: &[[f32; 4]]) -> Vec<ReadingTable> {
    let mut segments: Vec<_> = paths
        .iter()
        .copied()
        .filter(|b| b[2] - b[0] >= 24. && b[3] - b[1] <= 2.)
        .collect();
    segments.sort_by(|a, b| ((b[1] + b[3]) / 2.).total_cmp(&((a[1] + a[3]) / 2.)));
    let mut rules: Vec<Rule> = vec![];
    for b in segments {
        let y = (b[1] + b[3]) / 2.;
        if let Some(rule) = rules.last_mut().filter(|r| (r.y - y).abs() <= 1.) {
            rule.left = rule.left.min(b[0]);
            rule.right = rule.right.max(b[2]);
            rule.edges.extend([b[0], b[2]]);
        } else {
            rules.push(Rule {
                y,
                left: b[0],
                right: b[2],
                edges: vec![b[0], b[2]],
            });
        }
    }
    let mut tables = vec![];
    let mut start = 0;
    while start < rules.len() {
        let first = &rules[start];
        let mut end = start + 1;
        while end < rules.len()
            && (rules[end].left - first.left).abs() <= 2.
            && (rules[end].right - first.right).abs() <= 2.
            && !glyphs.split(|g| g.ch == '\n').any(|line| {
                super::caption_start(line)
                    && line
                        .iter()
                        .find(|g| !g.ch.is_whitespace())
                        .is_some_and(|g| g.y < rules[end - 1].y && g.y > rules[end].y)
            })
        {
            end += 1;
        }
        if end - start >= 2
            && let Some(table) = build(glyphs, &rules[start..end], paths)
        {
            tables.push(table);
        }
        start = end;
    }
    tables.extend(vertical::recover(glyphs, paths));
    tables
}

fn build(glyphs: &[Glyph], rules: &[Rule], paths: &[[f32; 4]]) -> Option<ReadingTable> {
    let first = &rules[0];
    let bounds = [first.left, rules.last()?.y, first.right, first.y];
    let header: Vec<_> = glyphs
        .iter()
        .filter(|g| {
            !g.ch.is_whitespace()
                && !g.ch.is_control()
                && g.y < first.y
                && g.y > rules[1].y
                && g.x >= first.left
                && g.x < first.right
        })
        .collect();
    let size = header.first()?.size;
    if size <= 0. || header.iter().any(|g| g.mono) {
        return None;
    }
    let bold_header = header.iter().all(|g| g.bold);
    // Academic booktabs tables often use a regular-weight, wrapped header and
    // only top/header/bottom rules. Require a nearby numbered table caption
    // plus persistent empty vertical gutters, not ordinary word spaces.
    let captioned = rules.len() == 3
        && glyphs.split(|g| g.ch == '\n').any(|line| {
            super::caption_start(line)
                && super::line_text(line).trim_start().starts_with("Table ")
                && line
                    .iter()
                    .find(|g| !g.ch.is_whitespace())
                    .is_some_and(|g| g.y > first.y && g.y - first.y <= size * 8.)
        });
    let inferred_columns = captioned.then(|| gutter_columns(glyphs, bounds, size));
    let booktabs = !bold_header
        && inferred_columns
            .as_ref()
            .is_some_and(|c| (4..=9).contains(&c.len()));
    let has_header = bold_header || booktabs;
    // Many PDFs paint horizontal borders separately for each cell. Repeated
    // segment endpoints recover exact source column proportions.
    let mut columns = vec![first.left];
    for &edge in &first.edges {
        if edge > first.left + size * 2.
            && edge < first.right - size * 2.
            && rules
                .iter()
                .skip(1)
                .any(|r| r.edges.iter().any(|x| (*x - edge).abs() <= 1.))
            && !columns.iter().any(|x| (*x - edge).abs() <= 1.)
        {
            columns.push(edge);
        }
    }
    if booktabs {
        columns = inferred_columns?;
        columns.pop(); // The common path below appends the right table edge.
    }
    // Unsegmented full-width rules: use separated bold header runs. Ordinary
    // word spaces are too small to create a column boundary.
    if columns.len() == 1 && bold_header {
        for pair in header.windows(2) {
            if (pair[0].y - pair[1].y).abs() <= size * 0.2 && pair[1].x - pair[0].x > size * 2. {
                columns.push(pair[1].x - size * 0.5);
            }
        }
    }
    columns.push(first.right);
    columns.sort_by(f32::total_cmp);
    if columns.len() < 3 || columns.len() > 9 {
        return None;
    }
    if has_header {
        if rules.len() < 3 {
            return None;
        }
    } else {
        // A continuation page may contain a single data row, no caption and
        // no repeated header. Require an actual cell grid: repeated segmented
        // borders plus a matching rectangle for every cell in every row.
        // Two full-width rules around ordinary prose are not enough evidence.
        if !rules.windows(2).all(|row| {
            columns.windows(2).all(|col| {
                paths.iter().any(|b| {
                    (b[0] - col[0]).abs() <= 1.
                        && (b[2] - col[1]).abs() <= 1.
                        && (b[1] - row[1].y).abs() <= 1.
                        && (b[3] - row[0].y).abs() <= 1.
                })
            })
        }) {
            return None;
        }
    }
    let mut row_edges: Vec<_> = rules.iter().map(|r| r.y).collect();
    if booktabs {
        // No rules between data rows: group normal-sized glyphs by physical
        // baseline, ignoring scripts. Every inferred row must occupy every
        // column, so wrapped prose cannot masquerade as an aligned table.
        let mut body: Vec<_> = glyphs
            .iter()
            .filter(|g| {
                !g.ch.is_whitespace()
                    && !g.ch.is_control()
                    && contains(bounds, [g.x, g.y, g.right, g.y])
                    && g.y < rules[1].y
                    && (g.size - size).abs() <= size * 0.05
            })
            .collect();
        body.sort_by(|a, b| b.y.total_cmp(&a.y));
        let mut rows: Vec<(f32, std::collections::HashSet<usize>)> = vec![];
        for g in body {
            let col = columns.windows(2).position(|c| g.x >= c[0] && g.x < c[1])?;
            if rows
                .last()
                .is_none_or(|(y, _)| (*y - g.y).abs() > size * 0.2)
            {
                rows.push((g.y, std::collections::HashSet::new()));
            }
            rows.last_mut()?.1.insert(col);
        }
        if rows.len() < 2 || rows.iter().any(|(_, cols)| cols.len() != columns.len() - 1) {
            return None;
        }
        row_edges = vec![first.y, rules[1].y];
        row_edges.extend(rows.windows(2).map(|pair| (pair[0].0 + pair[1].0) * 0.5));
        row_edges.push(rules.last()?.y);
    }
    let mut cells = vec![vec![Vec::new(); columns.len() - 1]; row_edges.len() - 1];
    let mut claimed = vec![];
    let mut indices = vec![vec![Vec::new(); columns.len() - 1]; row_edges.len() - 1];
    let mut offset = 0;
    for (i, line) in glyphs.split(|g| g.ch == '\n').enumerate() {
        let line_start = offset;
        offset += line.len() + 1;
        let inside = |g: &Glyph| {
            g.x >= bounds[0] - 1. && g.x < bounds[2] && g.y > bounds[1] && g.y < bounds[3]
        };
        let visible: Vec<_> = line
            .iter()
            .filter(|g| !g.ch.is_whitespace() && !g.ch.is_control())
            .collect();
        if !visible.iter().any(|g| inside(g)) {
            continue;
        }
        // Do not eat prose or partial extraction lines crossing the table.
        if visible.iter().any(|g| !inside(g)) {
            return None;
        }
        claimed.push(i);
        for (position, g) in line
            .iter()
            .enumerate()
            .filter(|(_, g)| inside(g) && (!g.ch.is_control() || g.ch == '\u{2}'))
        {
            let row = row_edges
                .windows(2)
                .position(|r| g.y < r[0] && g.y > r[1])?;
            let col = columns
                .windows(2)
                .position(|x| g.x >= x[0] - 1. && g.x < x[1])?;
            cells[row][col].push(g);
            indices[row][col].push(line_start + position);
        }
    }
    // Empty cells (including the corner header of a comparison table) are
    // valid. Require at least two populated columns in the first band, and
    // text in every ruled band to reject decorations and empty grids.
    if claimed.is_empty()
        || cells[0]
            .iter()
            .filter(|c| c.iter().any(|g| !g.ch.is_whitespace()))
            .count()
            < 2
        || cells.iter().any(|r| r.iter().all(Vec::is_empty))
    {
        return None;
    }
    let mut html = String::from("<table>");
    let mut slots = Vec::new();
    for (row, cells) in cells.iter().enumerate() {
        html.push_str("<tr>");
        for (col, cell) in cells.iter().enumerate() {
            let tag = if row == 0 && has_header { "th" } else { "td" };
            let width = (columns[col + 1] - columns[col]) / (bounds[2] - bounds[0]) * 100.;
            html.push_str(&format!("<{tag} width=\"{width:.3}%\">"));
            let start = html.len();
            html.push_str(&cell_html(cell, col == 0));
            slots.push(Cell {
                html_range: start..html.len(),
                indices: indices[row][col].clone(),
                label: col == 0,
            });
            html.push_str(&format!(" </{tag}>\n"));
        }
        html.push_str("</tr>");
    }
    html.push_str("</table>");
    Some(ReadingTable {
        bounds,
        lines: claimed,
        html,
        cells: slots,
    })
}

/// Intersect whitespace across all header/data baselines. A gutter must stay
/// empty through the entire table, including the longest label and scripts.
fn gutter_columns(glyphs: &[Glyph], bounds: [f32; 4], size: f32) -> Vec<f32> {
    let mut ink: Vec<_> = glyphs
        .iter()
        .filter(|g| {
            !g.ch.is_whitespace()
                && !g.ch.is_control()
                && g.x >= bounds[0]
                && g.right <= bounds[2]
                && g.y > bounds[1]
                && g.y < bounds[3]
        })
        .collect();
    ink.sort_by(|a, b| a.x.total_cmp(&b.x));
    let mut columns = vec![bounds[0]];
    let mut right = bounds[0];
    for g in ink {
        if right > bounds[0] && g.x - right >= size * 0.8 {
            columns.push((right + g.x) * 0.5);
        }
        right = right.max(g.advance_right.max(g.right));
    }
    columns.push(bounds[2]);
    columns
}

pub(super) fn cell_html(glyphs: &[&Glyph], label_column: bool) -> String {
    let mut html = String::new();
    let mut run = String::new();
    let mut style = (false, false, false);
    let mut previous: Option<&Glyph> = None;
    let flush = |html: &mut String, run: &mut String, (italic, bold, mono): (bool, bool, bool)| {
        if run.is_empty() {
            return;
        }
        let text = escape(run);
        let text = if mono {
            let left = run.len() - run.trim_start().len();
            let right = run.trim_end().len();
            format!(
                "{}<code>{}</code>{}",
                escape(&run[..left]),
                escape(run.trim()),
                escape(&run[right..])
            )
        } else {
            text
        };
        let text = if italic {
            format!("<em>{text}</em>")
        } else {
            text
        };
        html.push_str(&if bold {
            format!("<strong>{text}</strong>")
        } else {
            text
        });
        run.clear();
    };
    for &g in glyphs {
        if g.ch.is_whitespace() {
            if !run.ends_with(' ') && previous.is_some() {
                run.push(' ');
            }
            continue;
        }
        if previous.is_some_and(|p| {
            (p.y - g.y).abs() <= g.size * 0.3 && g.x - p.advance_right > g.size * 0.12
        }) && !run.ends_with(' ')
        {
            run.push(' ');
        }
        if previous.is_some_and(|p| (p.y - g.y).abs() > g.size * 0.3) {
            // Discretionary hyphens are PDF control characters in this book.
            // Keep the visible hyphen and join the word across its source wrap.
            if label_column && run.ends_with('-') {
                flush(&mut html, &mut run, style);
                html.push_str("<br>");
            } else if !run.ends_with(['-', ' ']) {
                run.push(' ');
            }
        }
        let next = (g.italic, g.bold, g.mono);
        if next != style {
            flush(&mut html, &mut run, style);
            style = next;
        }
        run.push(if g.ch == '\u{2}' { '-' } else { g.ch });
        previous = Some(g);
    }
    flush(&mut html, &mut run, style);
    html
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, x: f32, y: f32, bold: bool) -> Vec<Glyph> {
        text.chars()
            .enumerate()
            .map(|(i, ch)| Glyph {
                ch,
                mono: false,
                x: x + i as f32 * 5.,
                right: x + (i + 1) as f32 * 5.,
                advance_right: x + (i + 1) as f32 * 5.,
                y,
                size: 10.,
                color: 0,
                italic: false,
                bold,
                image_before: None,
                image_after: None,
                link: None,
            })
            .collect()
    }

    fn fixture() -> (Vec<Glyph>, Vec<[f32; 4]>) {
        let mut glyphs = vec![];
        // One extraction line can cross all three cells. Physical wraps have
        // independent baselines, and one body cell is intentionally empty.
        for (text, x, y, bold) in [
            ("Type", 76., 730., true),
            ("Benefits", 186., 730., true),
            ("Costs", 366., 730., true),
            ("Projection\u{2}", 76., 700., false),
            ("Fast & simple", 186., 700., false),
            ("Many tokens", 366., 700., false),
            ("based", 76., 685., false),
            ("Low latency", 186., 685., false),
            ("Query", 76., 635., false),
            ("More training", 366., 635., false),
        ] {
            glyphs.extend(line(text, x, y, bold));
            glyphs.extend(line("\n", 0., 0., false));
        }
        let mut paths = vec![];
        for y in [748., 720., 660., 610.] {
            for (left, right) in [(72., 180.), (180., 360.), (360., 540.)] {
                paths.push([left, y, right, y + 0.6]);
                paths.push([left, y, right, y + 0.6]); // repeated shared border
            }
        }
        (glyphs, paths)
    }

    #[test]
    fn ruled_cells_keep_wraps_empty_cells_emphasis_and_source_widths() {
        let (mut glyphs, paths) = fixture();
        for g in &mut glyphs {
            if g.y == 700. && g.x >= 180. && g.x < 360. {
                g.italic = true;
            }
        }
        let tables = from_rules(&glyphs, &paths);
        assert_eq!(tables.len(), 1);
        let html = &tables[0].html;
        assert_eq!(html.matches("<tr>").count(), 3);
        assert_eq!(html.matches("<td ").count(), 6);
        assert!(html.contains("Projection-<br>based"), "{html}");
        assert!(
            html.contains("<em>Fast &amp; simple </em>Low latency"),
            "{html}"
        );
        assert!(html.contains("width=\"23.077%\""));
        assert!(html.contains("<td width=\"38.462%\"> </td>"));
        assert_eq!(tables[0].lines.len(), 10);
    }

    #[test]
    fn captioned_booktabs_recovers_plain_wrapped_headers_rows_and_cell_math() {
        let mut glyphs = Vec::new();
        for (text, x, y) in [
            ("Table 1: A comparison", 72., 775.),
            ("Layer", 76., 730.),
            ("Cost per layer", 186., 730.),
            ("Sequential", 336., 730.),
            ("Max path", 426., 730.),
            ("Operations", 336., 715.),
            ("Self attention", 76., 685.),
            ("O(n2d)", 225., 685.),
            ("O(1)", 350., 685.),
            ("O(1)", 460., 685.),
            ("Recurrent", 76., 660.),
            ("O(nd2)", 225., 660.),
            ("O(1)", 350., 660.),
            ("O(1)", 460., 660.),
            ("Restricted", 76., 635.),
            ("O(rnd)", 225., 635.),
            ("O(1)", 350., 635.),
            ("O(1)", 460., 635.),
        ] {
            glyphs.extend(line(text, x, y, false));
            glyphs.extend(line("\n", 0., 0., false));
        }
        for g in &mut glyphs {
            if g.ch == '2' {
                g.size = 7.;
                g.y += 3.5;
            }
        }
        let paths: Vec<_> = [748., 700., 610.]
            .iter()
            .map(|&y| [72., y, 540., y + 0.6])
            .collect();
        let mut tables = from_rules(&glyphs, &paths);
        assert_eq!(tables.len(), 1);
        let table = &mut tables[0];
        assert_eq!(table.html.matches("<tr>").count(), 4);
        assert_eq!(table.html.matches("<th ").count(), 4);
        assert_eq!(table.html.matches("<td ").count(), 12);
        assert!(table.html.contains("Sequential Operations"));
        assert!(!table.html.contains("Table 1:"));
        let object = crate::formula::native_html("O(1)", false);
        let layout = crate::formula::layout("O(1)", false).unwrap();
        let math: Vec<_> = glyphs
            .windows(4)
            .enumerate()
            .filter(|(_, span)| span.iter().map(|g| g.ch).collect::<String>() == "O(1)")
            .map(|(i, _)| super::super::math::ReadingMath {
                range: i..i + 4,
                lines: vec![],
                presentation: crate::pdf::MathPresentation {
                    tex: "O(1)".into(),
                    display: false,
                    number: None,
                    layout: (*layout).clone(),
                },
            })
            .collect();
        table.restore_math(&glyphs, &math);
        assert_eq!(table.html.matches(&object).count(), 6, "{}", table.html);
        // Regular headers require both a caption and fully populated aligned
        // rows; neither ordinary ruled prose nor partial rows qualify.
        let without_caption: Vec<_> = glyphs.iter().filter(|g| g.y < 750.).cloned().collect();
        assert!(from_rules(&without_caption, &paths).is_empty());
        let partial: Vec<_> = glyphs
            .iter()
            .filter(|g| !(g.y == 635. && g.x >= 420.))
            .cloned()
            .collect();
        assert!(from_rules(&partial, &paths).is_empty());
    }

    #[test]
    fn wide_sparse_table_region_survives_unsupported_column_count() {
        let mut glyphs = line("Table 3: Many columns and blank values", 72., 775., false);
        glyphs.extend(line("\n", 0., 0., false));
        for row in 0..3 {
            for col in 0..12 {
                if row == 2 && col % 3 == 0 {
                    continue;
                }
                glyphs.extend(line(
                    if row == 0 { "N" } else { "16" },
                    76. + col as f32 * 38.,
                    730. - row as f32 * 25.,
                    false,
                ));
                glyphs.extend(line("\n", 0., 0., false));
            }
        }
        let paths: Vec<_> = [748., 720., 660.]
            .iter()
            .map(|&y| [72., y, 540., y + 0.6])
            .collect();
        assert!(
            from_rules(&glyphs, &paths).is_empty(),
            "No guessed grid for unsupported sparse tables"
        );
        assert_eq!(source_regions(&glyphs, &paths), [[72., 660., 540., 748.6]]);
        let no_caption: Vec<_> = glyphs.iter().filter(|g| g.y < 750.).cloned().collect();
        assert!(source_regions(&no_caption, &paths).is_empty());
        let prose = line(
            "Ordinary ruled prose does not define a table",
            76.,
            730.,
            false,
        );
        assert!(source_regions(&prose, &paths).is_empty());
    }

    fn single_header_rule_fixture() -> (Vec<Glyph>, Vec<[f32; 4]>) {
        let mut glyphs = Vec::new();
        for (y, cells) in [
            (730., ["Goal", "Parts", "Cases", "Chapters", "Optional"]),
            (700., ["Complete", "I, II, III", "1, 2, 3", "1-18", "19-22"]),
            (670., ["Assistants", "II, III", "2, 3", "6-14", "2-5, 19"]),
            (640., ["Social", "II, V", "2, 4", "6-10, 15-18", "2-5, 22"]),
        ] {
            for (text, x) in cells.into_iter().zip([76., 186., 276., 346., 466.]) {
                glyphs.extend(line(text, x, y, false));
            }
            glyphs.extend(line("\n", 0., 0., false));
        }
        glyphs.extend(line("Table 1.1: Course goals", 72., 590., false));
        glyphs.extend(line("\n", 0., 0., false));
        let mut paths = vec![[72., 719.7, 540., 720.3]];
        for x in [180., 270., 340., 460.] {
            // TeX emits a separate vertical segment for each row.
            for (bottom, top) in [(720., 748.), (690., 720.), (660., 690.), (610., 660.)] {
                paths.push([x - 0.3, bottom, x + 0.3, top]);
            }
        }
        (glyphs, paths)
    }

    #[test]
    fn single_header_rule_and_segmented_verticals_recover_native_cells() {
        let (glyphs, paths) = single_header_rule_fixture();
        let tables = from_rules(&glyphs, &paths);
        assert_eq!(tables.len(), 1);
        let table = &tables[0];
        assert_eq!(table.lines, vec![0, 1, 2, 3]);
        assert_eq!(table.html.matches("<tr>").count(), 4);
        assert_eq!(table.html.matches("<th ").count(), 5);
        assert_eq!(table.html.matches("<td ").count(), 15);
        assert!(table.html.contains("data-hlines=\"1\""));
        assert!(table.html.contains("data-vlines=\"1,2,3,4\""));
        assert!(table.html.contains("6-10, 15-18"));
        assert!(!table.html.contains("Table 1.1") && !table.html.contains("<img"));
        assert_eq!(source_regions(&glyphs, &paths).len(), 1);
        assert!(crate::typst_table::TableGrid::from_html(&table.html).is_ok());
    }

    #[test]
    fn vertical_tables_keep_empty_cells_and_separate_tables_on_shared_column_edges() {
        let (mut glyphs, mut paths) = single_header_rule_fixture();
        glyphs.retain(|g| !(g.y == 670. && g.x >= 460.));
        let tables = from_rules(&glyphs, &paths);
        assert_eq!(tables.len(), 1);
        assert!(tables[0].html.contains("<td width=\"17.094%\"> </td>"));
        let mut second = glyphs.clone();
        for g in &mut second {
            g.y -= 300.;
        }
        glyphs.extend(second);
        let second_paths: Vec<_> = paths
            .iter()
            .map(|b| [b[0], b[1] - 300., b[2], b[3] - 300.])
            .collect();
        paths.extend(second_paths);
        let tables = from_rules(&glyphs, &paths);
        assert_eq!(tables.len(), 2);
        assert_eq!(source_regions(&glyphs, &paths).len(), 2);
        assert!(tables.iter().all(|t| t.html.matches("<tr>").count() == 4));
    }

    #[test]
    fn vertical_table_detection_rejects_incomplete_geometry_and_crossing_prose() {
        let (glyphs, paths) = single_header_rule_fixture();
        let no_caption: Vec<_> = glyphs.iter().filter(|g| g.y > 600.).cloned().collect();
        assert!(from_rules(&no_caption, &paths).is_empty());
        assert!(source_regions(&no_caption, &paths).is_empty());
        let broken: Vec<_> = paths.iter().copied().filter(|b| b[1] != 660.).collect();
        assert!(from_rules(&glyphs, &broken).is_empty());
        let mut crossing = glyphs.clone();
        crossing[0].x = 40.;
        assert!(from_rules(&crossing, &paths).is_empty());
        let mut across_column = glyphs.clone();
        across_column[0].right = 190.;
        assert!(from_rules(&across_column, &paths).is_empty());
        let decorations = vec![paths[0], [180., 610., 180.6, 748.]];
        assert!(from_rules(&glyphs, &decorations).is_empty());
        let only_header: Vec<_> = glyphs
            .iter()
            .filter(|g| g.y > 720. || g.y < 610.)
            .cloned()
            .collect();
        assert!(from_rules(&only_header, &paths).is_empty());
        let mut broken_rows = glyphs.clone();
        for g in &mut broken_rows {
            if g.y == 700. && g.x >= 180. {
                g.y -= 5.;
            }
        }
        assert!(from_rules(&broken_rows, &paths).is_empty());
    }

    #[test]
    fn full_width_rules_recover_columns_from_separated_headers() {
        let (glyphs, _) = fixture();
        let paths: Vec<_> = [748., 720., 660., 610.]
            .iter()
            .map(|&y| [72., y, 540., y + 0.6])
            .collect();
        let tables = from_rules(&glyphs, &paths);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].html.matches("<th ").count(), 3);
        assert!(tables[0].html.contains("Projection-<br>based"));
    }

    #[test]
    fn comparison_table_keeps_empty_corner_header_and_column_positions() {
        let (glyphs, paths) = fixture();
        let glyphs: Vec<_> = glyphs
            .into_iter()
            .filter(|g| !(g.y == 730. && g.x < 180.))
            .collect();
        let tables = from_rules(&glyphs, &paths);
        assert_eq!(tables.len(), 1);
        assert!(
            tables[0]
                .html
                .starts_with("<table><tr><th width=\"23.077%\"> </th>\n<th width=\"38.462%\">")
        );
        assert_eq!(tables[0].html.matches("<th ").count(), 3);
        assert_eq!(tables[0].html.matches("<td ").count(), 6);
    }

    #[test]
    fn headerless_shaded_row_keeps_empty_label_and_is_never_a_header() {
        let mut glyphs = line("Lower accuracy ceiling", 186., 730., false);
        glyphs.extend(line("\n", 0., 0., false));
        glyphs.extend(line("drives costs.", 366., 730., false));
        let mut paths = vec![];
        for (left, right) in [(72., 180.), (180., 360.), (360., 540.)] {
            paths.extend([
                [left, 748., right, 748.6],
                [left, 710., right, 710.6],
                [left, 710.6, right, 748.6],
            ]);
        }
        let tables = from_rules(&glyphs, &paths);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].html.matches("<th ").count(), 0);
        assert_eq!(tables[0].html.matches("<td ").count(), 3);
        assert!(tables[0].html.contains("<td width=\"23.077%\"> </td>"));
        paths.retain(|b| b[3] - b[1] < 2.);
        assert!(from_rules(&glyphs, &paths).is_empty());
        // A single callout rectangle around prose is not a grid of cells.
        paths.push([72., 710.6, 540., 748.6]);
        assert!(from_rules(&glyphs, &paths).is_empty());
    }

    #[test]
    fn ordinary_prose_partial_lines_and_empty_bands_are_not_tables() {
        let (mut glyphs, paths) = fixture();
        for g in &mut glyphs {
            g.bold = false;
        }
        assert!(from_rules(&glyphs, &paths).is_empty());
        let (mut glyphs, paths) = fixture();
        glyphs[0].x = 40.; // partially overlaps a line outside the table
        assert!(from_rules(&glyphs, &paths).is_empty());
        let (glyphs, mut paths) = fixture();
        paths.extend([
            [72., 600., 180., 600.6],
            [180., 600., 360., 600.6],
            [360., 600., 540., 600.6],
        ]);
        assert!(from_rules(&glyphs, &paths).is_empty());
    }
}
