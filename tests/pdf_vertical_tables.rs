use easyread_gpui::{pdf, store::Library, typst_table::TableGrid};

#[allow(dead_code)]
mod support;

#[test]
fn vertical_table_import_and_presentation_never_rasterize_cells() -> anyhow::Result<()> {
    let temp = tempfile::tempdir()?;
    let source = temp.path().join("vertical-table.pdf");
    support::vertical_table_pdf_fixture(&source);
    let ws = Library::open(temp.path().join("library"))?.import_pdf(&source, None)?;
    let before = std::fs::read(ws.root.join("paper.json"))?;
    let raw = ws.state()?;
    let blocks = raw["paper"]["blocks"].as_array().unwrap();
    assert!(
        blocks.iter().all(|b| b["type"] != "figure"),
        "Table rules are not imported as art"
    );
    let text = blocks[0]["en"].as_str().unwrap();
    let p = pdf::code_presentation(&source, 1, text)?.unwrap();
    assert_eq!(p.html.matches("<table ").count(), 1);
    assert!(!p.html.contains("<img") && p.figures.is_empty());
    let start = p.html.find("<table").unwrap();
    let end = p.html.find("</table>").unwrap() + 8;
    let table = TableGrid::from_html(&p.html[start..end])?;
    let expected = "Course Goal\tParts\tCase Study\tChapters\tOptional\nComprehensive\tI, II, III, IV, V\t1, 2, 3, 4\t1-18\t19-22\nCore Assistants\tII, III, IV\t2, 3\t6-14\t2-5, 19\nCore Social Simulation\tII, III, V\t2, 4\t6-10, 11, 15-18\t2-5, 22";
    assert_eq!(table.tsv(), expected);
    assert!(table.layout()?.1.is_valid());
    assert!(!p.html[start..end].contains("CHAPTER") && !p.html[start..end].contains("Table 1.1:"));
    assert!(p.html[end..].contains("Table 1.1:") && p.html[end..].contains("Following prose"));
    assert_eq!(std::fs::read(ws.root.join("paper.json"))?, before);
    Ok(())
}
