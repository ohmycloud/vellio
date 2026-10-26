//! Semantic spanning tables -> in-process Typst -> native GPUI vector frames.
//! HTML cells remain the stored/translated representation; no image is produced.
use crate::formula::FormulaLayout;
use anyhow::{Context, Result, ensure};
use roxmltree::{Document, Node};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TableCell {
    pub row: usize,
    pub col: usize,
    pub rowspan: usize,
    pub colspan: usize,
    pub header: bool,
    pub html: String,
    pub text: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TableGrid {
    pub columns: Vec<f32>,
    pub rows: usize,
    pub cells: Vec<TableCell>,
    pub hlines: Vec<usize>,
    pub vlines: Vec<usize>,
}
static CACHE: LazyLock<Mutex<HashMap<String, Arc<FormulaLayout>>>> = LazyLock::new(Mutex::default);
pub fn cached_layout(key: &str) -> Option<Arc<FormulaLayout>> {
    CACHE.lock().ok()?.get(key).cloned()
}
fn attr_usize(node: Node<'_, '_>, name: &str, default: usize) -> Result<usize> {
    node.attribute(name).map_or(Ok(default), |s| {
        s.parse().context("Invalid table coordinate/span")
    })
}
impl TableGrid {
    pub fn from_html(html: &str) -> Result<Self> {
        let html = crate::translation_layout::xhtml(html);
        let doc = Document::parse(&html).context("Invalid semantic table HTML")?;
        Self::from_node(doc.root_element(), &html)
    }
    fn from_node(table: Node<'_, '_>, source: &str) -> Result<Self> {
        ensure!(table.has_tag_name("table"), "Expected table element");
        let rows: Vec<_> = table
            .descendants()
            .filter(|n| n.has_tag_name("tr"))
            .collect();
        ensure!(
            !rows.is_empty() && rows.len() <= 512,
            "Invalid table row count"
        );
        let mut cells: Vec<TableCell> = Vec::new();
        let mut column_hints = Vec::new();
        let mut count = 0;
        for (row, node) in rows.iter().enumerate() {
            let mut col = 0;
            for cell in node
                .children()
                .filter(|n| n.has_tag_name("td") || n.has_tag_name("th"))
            {
                if cell.attribute("data-col").is_none() {
                    while cells.iter().any(|c| {
                        row >= c.row
                            && row < c.row + c.rowspan
                            && col >= c.col
                            && col < c.col + c.colspan
                    }) {
                        col += 1;
                    }
                }
                col = attr_usize(cell, "data-col", col)?;
                let rowspan = attr_usize(cell, "rowspan", 1)?;
                let colspan = attr_usize(cell, "colspan", 1)?;
                ensure!(
                    col <= 64
                        && rowspan <= 512
                        && colspan <= 64
                        && rowspan > 0
                        && colspan > 0
                        && col + colspan <= 64
                        && row + rowspan <= rows.len(),
                    "Table span out of bounds"
                );
                let literal = &source[cell.range()];
                let start = literal.find('>').context("Missing cell opening tag")? + 1;
                let html = if literal.ends_with("/>") {
                    String::new()
                } else {
                    let end = literal.rfind("</").context("Missing cell closing tag")?;
                    literal[start..end].to_owned()
                };
                let width = cell
                    .attribute("width")
                    .and_then(|x| x.trim_end_matches('%').parse::<f32>().ok());
                if let Some(width) = width {
                    ensure!(width.is_finite() && width > 0., "Invalid column width");
                }
                column_hints.push((col, colspan, width));
                let text = crate::annotations::plain_text(&html).trim().to_owned();
                cells.push(TableCell {
                    row,
                    col,
                    rowspan,
                    colspan,
                    header: cell.has_tag_name("th"),
                    html,
                    text,
                });
                count = count.max(col + colspan);
                col += colspan;
            }
        }
        ensure!(count >= 2, "Table has fewer than two columns");
        let mut occupied = vec![vec![false; count]; rows.len()];
        let mut widths = vec![0.; count];
        for cell in &cells {
            for row in occupied.iter_mut().skip(cell.row).take(cell.rowspan) {
                for used in row.iter_mut().skip(cell.col).take(cell.colspan) {
                    ensure!(!*used, "Overlapping table cells");
                    *used = true;
                }
            }
        }
        ensure!(
            occupied.iter().flatten().all(|&b| b),
            "Incomplete table grid: missing cells are not zero values"
        );
        for (col, span, width) in column_hints {
            if span == 1
                && let Some(width) = width
            {
                widths[col] = width;
            }
        }
        for w in &mut widths {
            if *w <= 0. {
                *w = 100. / count as f32;
            }
        }
        let lines = |name: &str| -> Result<Vec<usize>> {
            table
                .attribute(name)
                .unwrap_or("")
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.parse().context("Invalid table rule position"))
                .collect()
        };
        let hlines = lines("data-hlines")?;
        let vlines = lines("data-vlines")?;
        ensure!(
            hlines.iter().all(|&i| i <= rows.len()) && vlines.iter().all(|&i| i <= count),
            "Table rule out of bounds"
        );
        Ok(Self {
            columns: widths,
            rows: rows.len(),
            cells,
            hlines,
            vlines,
        })
    }
    /// Empty/covered cells remain empty; group names are not invented or copied
    /// into every data row. This is the text representation used for copy.
    pub fn tsv(&self) -> String {
        let mut rows = vec![vec![String::new(); self.columns.len()]; self.rows];
        for c in &self.cells {
            rows[c.row][c.col] = c.text.replace(['\n', '\t'], " ");
        }
        rows.into_iter()
            .map(|r| r.join("\t"))
            .collect::<Vec<_>>()
            .join("\n")
    }
    pub fn typst_source(&self) -> Result<String> {
        let total = self.columns.iter().sum::<f32>();
        let cols = self
            .columns
            .iter()
            .map(|w| format!("{:.3}pt", 640. * w / total))
            .collect::<Vec<_>>()
            .join(",");
        let mut source = format!(
            "#import \"mitex.typ\": mi\n#set page(width:auto,height:auto,margin:0pt,fill:none)\n#set text(font:(\"New Computer Modern\",\"Noto Serif SC\"),size:16pt,fill:rgb(\"23211d\"))\n#set par(leading:0.2em)\n#table(columns:({cols}),inset:(x:2pt,y:3pt),stroke:none,\n"
        );
        for cell in &self.cells {
            let content = cell_content(&cell.html)?;
            let align = if cell.col == 0 {
                "left+horizon"
            } else {
                "center+horizon"
            };
            source.push_str(&format!(
                "table.cell(x:{},y:{},rowspan:{},colspan:{},align:{align})[{content}],\n",
                cell.col, cell.row, cell.rowspan, cell.colspan
            ));
        }
        for &y in &self.hlines {
            source.push_str(&format!("table.hline(y:{y},stroke:0.5pt),\n"));
        }
        for &x in &self.vlines {
            source.push_str(&format!("table.vline(x:{x},stroke:0.5pt),\n"));
        }
        source.push(')');
        Ok(source)
    }
    /// Vector export for QA only; the reader paints the native frame paths.
    pub fn svg(&self) -> Result<String> {
        let doc = crate::formula::compile_source(self.typst_source()?)?;
        Ok(typst_svg::svg(&doc.pages()[0], &Default::default()))
    }
    pub fn layout(&self) -> Result<(String, Arc<FormulaLayout>)> {
        let source = self.typst_source()?;
        let key = format!("{:x}", Sha256::digest(source.as_bytes()));
        if let Some(frame) = cached_layout(&key) {
            return Ok((key, frame));
        }
        let frame = Arc::new(crate::formula::typeset(source)?);
        CACHE
            .lock()
            .map_err(|_| anyhow::anyhow!("Table cache poisoned"))?
            .insert(key.clone(), frame.clone());
        Ok((key, frame))
    }
}
fn cell_content(html: &str) -> Result<String> {
    let wrapped = format!("<root>{}</root>", crate::translation_layout::xhtml(html));
    let doc = Document::parse(&wrapped).context("Invalid cell markup")?;
    fn visit(node: Node<'_, '_>) -> Result<String> {
        if node.is_text() {
            return Ok(format!(
                "#text({})",
                serde_json::to_string(node.text().unwrap_or(""))?
            ));
        }
        if let Some(tex) = node.attribute("data-vellio-math") {
            return Ok(format!("#mi({})", serde_json::to_string(tex)?));
        }
        if node.has_tag_name("br") {
            return Ok("#linebreak()".into());
        }
        let inner = node
            .children()
            .map(visit)
            .collect::<Result<Vec<_>>>()?
            .join("");
        Ok(if node.has_tag_name("strong") || node.has_tag_name("b") {
            format!("#strong[{inner}]")
        } else if node.has_tag_name("em") || node.has_tag_name("i") {
            format!("#emph[{inner}]")
        } else {
            inner
        })
    }
    visit(doc.root_element())
}
fn grids(html: &str) -> Result<Vec<(std::ops::Range<usize>, TableGrid)>> {
    let normalized = crate::translation_layout::xhtml(html);
    let wrapped = format!("<root>{normalized}</root>");
    let doc = Document::parse(&wrapped).context("Invalid reader HTML")?;
    doc.descendants()
        .filter(|n| n.has_tag_name("table") && n.attribute("data-vellio-table") == Some("true"))
        .map(|n| {
            Ok((
                n.range().start - 6..n.range().end - 6,
                TableGrid::from_node(n, &wrapped)?,
            ))
        })
        .collect()
}
/// Precompile in the serialized PDF worker, not on the UI paint thread.
pub fn warm_html(html: &str) -> Result<()> {
    if !html.contains("data-vellio-table=\"true\"") {
        return Ok(());
    }
    for (_, grid) in grids(html)? {
        grid.layout()?;
    }
    Ok(())
}
/// Display-only conversion. Semantic HTML remains available to translation,
/// storage, and annotations. The inline object's text is the structured TSV.
pub fn prepare_html(html: &str) -> Result<String> {
    if !html.contains("data-vellio-table=\"true\"") {
        return Ok(html.into());
    }
    let mut out = crate::translation_layout::xhtml(html);
    for (range, grid) in grids(&out)?.into_iter().rev() {
        let (key, _) = grid.layout()?;
        // Encode control whitespace before the HTML minifier sees attributes.
        // Raw tabs/newlines are otherwise collapsed, erasing empty cells/rows.
        let text = serde_json::to_string(&grid.tsv())?
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\t', "&#9;")
            .replace('\n', "&#10;");
        out.replace_range(range,&format!("<p><span data-vellio-table-key=\"{key}\" data-vellio-table-text-json=\"{text}\">{text}</span></p>"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    const HTML: &str = "<table data-vellio-table=\"true\" data-hlines=\"0,2,3\"><tr><th rowspan=\"2\">Model</th><th colspan=\"2\">Scores</th></tr><tr><th>A</th><th>B</th></tr><tr><td>base</td><td>27.3</td><td/></tr></table>";
    #[test]
    fn ordinary_html_spans_preserve_empty_cells_and_compile_native_vectors() {
        let grid = TableGrid::from_html(HTML).unwrap();
        assert_eq!((grid.rows, grid.columns.len()), (3, 3));
        assert_eq!((grid.cells[2].row, grid.cells[2].col), (1, 1));
        assert_eq!(grid.tsv(), "Model\tScores\t\n\tA\tB\nbase\t27.3\t");
        let (key, layout) = grid.layout().unwrap();
        assert!(layout.is_valid() && layout.paths.len() > 10);
        assert!(Arc::ptr_eq(&layout, &cached_layout(&key).unwrap()));
        let display = prepare_html(HTML).unwrap();
        assert!(display.contains("data-vellio-table-key=") && !display.contains("<img"));
        let doc = Document::parse(&display).unwrap();
        let text = doc
            .descendants()
            .find(|n| n.has_tag_name("span"))
            .unwrap()
            .attribute("data-vellio-table-text-json")
            .unwrap();
        assert_eq!(serde_json::from_str::<String>(text).unwrap(), grid.tsv());
        assert!(
            !text.contains(['\n', '\t']),
            "Encode control whitespace before HTML minification"
        );
    }
    #[test]
    fn invalid_overlapping_missing_or_overflowing_cells_are_rejected() {
        for html in [
            "<table><tr><td data-col=\"0\">x</td><td data-col=\"0\">y</td><td data-col=\"1\">z</td></tr></table>",
            "<table><tr><td data-col=\"0\">x</td><td data-col=\"2\">y</td></tr></table>",
            "<table><tr><td colspan=\"18446744073709551615\">x</td><td>y</td></tr></table>",
            "<table><tr><td width=\"-1\">x</td><td>y</td></tr></table>",
        ] {
            assert!(TableGrid::from_html(html).is_err(), "{html}");
        }
    }
    #[test]
    fn translation_keeps_table_geometry_numbers_and_supports_chinese_typesetting() {
        let plan = crate::translation_layout::Plan::from_html(HTML).unwrap();
        let translated = plan
            .translate(
                &plan
                    .input()
                    .replace("Model", "模型")
                    .replace("Scores", "得分"),
            )
            .unwrap();
        let original = TableGrid::from_html(HTML).unwrap();
        let grid = TableGrid::from_html(&translated.html).unwrap();
        assert_eq!(grid.rows, original.rows);
        for (a, b) in grid.cells.iter().zip(&original.cells) {
            assert_eq!(
                (a.row, a.col, a.rowspan, a.colspan),
                (b.row, b.col, b.rowspan, b.colspan)
            );
        }
        assert!(grid.tsv().contains("模型") && grid.tsv().contains("27.3"));
        let (key, frame) = grid.layout().unwrap();
        assert!(frame.is_valid());
        assert_ne!(key, original.layout().unwrap().0);
        assert!(grid.typst_source().unwrap().contains("Noto Serif SC"));
    }
}
