use super::*;

#[test]
#[ignore = "requires EASYREAD_QA_PDF pointing to Attention Is All You Need"]
fn attention_comparison_table_recovers_rows_and_columns() -> anyhow::Result<()> {
    let path = std::env::var("EASYREAD_QA_PDF")?;
    let pdfium = Pdfium::new(Pdfium::bind_to_library("assets/pdfium/libpdfium.dylib")?);
    let doc = pdfium.load_pdf_from_file(&path, None)?;
    let page = doc.pages().get(5)?;
    let (_, html, _, _, _) = reading_html(&page, &doc, &[], 6)?;
    let html = html.unwrap();
    assert_eq!(html.matches("<table>").count(), 1, "{html}");
    let table_start = html.find("<table>").unwrap();
    let table_end = html.find("</table>").unwrap();
    let table = &html[table_start..table_end];
    assert_eq!(table.matches("<tr>").count(), 5, "{table}");
    assert_eq!(table.matches("<th ").count(), 4, "{table}");
    assert_eq!(table.matches("<td ").count(), 16, "{table}");
    assert_eq!(table.matches("data-vellio-math=").count(), 12, "{table}");
    for tex in ["O(n^{2}\\cdot d)", "O(log_{k}(n))", "O(n/r)"] {
        assert!(
            table.contains(&crate::formula::native_html(tex, false)),
            "{tex}: {table}"
        );
    }
    assert_eq!(
        html[..table_start].matches("<p>").count(),
        1,
        "{}",
        &html[..table_start]
    );
    assert!(
        !crate::annotations::plain_text(&html)
            .trim_end()
            .ends_with("\n6")
    );
    Ok(())
}

#[test]
fn unrecovered_table_keeps_positioned_text_without_a_screenshot_or_guessed_cells() {
    let gs = test_lines(&[
        ("Model         Score       Cost", 10., false, false),
        ("ByteNet       23.75", 10., false, false),
        ("Transformer   27.3        3.3", 10., false, false),
    ]);
    let fallback = table::text_fallback(&gs, [72., 0., 540., 800.]).unwrap();
    assert_eq!(fallback.lines, vec![0, 1, 2]);
    assert!(
        fallback.html.contains("data-vellio-table-unrecovered=")
            && fallback.html.contains("<pre><code>")
    );
    assert!(!fallback.html.contains("<img") && !fallback.html.contains("<table"));
    let text = crate::annotations::plain_text(&fallback.html);
    assert!(text.contains("ByteNet") && text.contains("23.75") && text.contains("Transformer"));
    assert!(
        fallback.html.contains("ByteNet  "),
        "Physical column gaps must not collapse into prose: {}",
        fallback.html
    );
    assert!(
        table::text_fallback(&gs, [72., 0., 170., 800.]).is_none(),
        "Mixed extraction records must not lose neighboring text"
    );
}

#[test]
#[ignore = "requires EASYREAD_QA_PDF pointing to Attention Is All You Need"]
fn attention_all_tables_keep_structured_cells_without_images() -> anyhow::Result<()> {
    let path = std::env::var("EASYREAD_QA_PDF")?;
    let before = std::fs::read(&path)?;
    let pdfium = Pdfium::new(Pdfium::bind_to_library("assets/pdfium/libpdfium.dylib")?);
    let doc = pdfium.load_pdf_from_file(&path, None)?;
    let outline = crate::pdf::document_outline(&doc);
    let out = std::path::Path::new("target/attention-tables-qa");
    std::fs::create_dir_all(out)?;
    for (n, number, body) in [
        (6, 1, "Positional Encoding"),
        (8, 2, "Residual Dropout"),
        (9, 3, "We present these results in Table 3"),
        (10, 4, "Our results in Table 4 show"),
    ] {
        let page = doc.pages().get(n - 1)?;
        let gs = recover_fragmented_lines(&glyphs(&page.text()?)).0;
        let regions = table::regions(&page, &gs);
        eprintln!("table {number}, page {n}: regions {regions:?}");
        let dump: Vec<_> = gs.iter().filter(|g| !g.ch.is_whitespace() && !g.ch.is_control() && regions.iter().any(|b| table::contains(*b, [g.x,g.y,g.right,g.y]))).map(|g| serde_json::json!({"ch":g.ch,"x":g.x,"right":g.right,"y":g.y,"size":g.size,"bold":g.bold,"italic":g.italic})).collect();
        std::fs::write(
            out.join(format!("table-{number}-glyphs.json")),
            serde_json::to_vec_pretty(&dump)?,
        )?;
        assert_eq!(
            regions.len(),
            1,
            "Table {number} must be detected independently of cell parsing"
        );
        let headings: Vec<_> = outline
            .iter()
            .filter(|h| h.page == n as usize)
            .cloned()
            .collect();
        let (source, html, _, previews, _) = reading_html(&page, &doc, &headings, n as usize)?;
        let html = html.unwrap();
        assert!(source.contains(&format!("Table {number}:")));
        assert!(
            html.contains(&format!("Table {number}:")),
            "Caption lost: {number}"
        );
        assert!(
            crate::annotations::plain_text(&html).contains(body),
            "Prose lost: {number}"
        );
        assert_eq!(html.matches("<table").count(), 1, "{number}: {html}");
        assert!(
            !html.contains("<img ") && previews.is_empty(),
            "No table images: {number}"
        );
        if number == 2 || number == 3 {
            let start = html.find("<table").unwrap();
            let end = html.find("</table>").unwrap() + 8;
            let grid = crate::typst_table::TableGrid::from_html(&html[start..end])?;
            let cell = |r, c| {
                grid.cells
                    .iter()
                    .find(|x| x.row == r && x.col == c)
                    .unwrap()
            };
            if number == 2 {
                assert_eq!((grid.rows, grid.columns.len()), (12, 5));
                assert_eq!(cell(0, 0).rowspan, 2);
                assert_eq!((cell(0, 1).text.as_str(), cell(0, 1).colspan), ("BLEU", 2));
                assert_eq!(cell(2, 1).text, "23.75");
                assert!(cell(2, 2).text.is_empty() && cell(2, 3).text.is_empty());
                assert_eq!(cell(10, 3).colspan, 2);
                assert!(cell(10, 3).html.contains("3.3\\cdot 10^{18}"));
                assert!(cell(10, 3).html.contains("<strong>"));
            } else {
                assert_eq!((grid.rows, grid.columns.len()), (21, 13));
                assert_eq!(
                    grid.vlines,
                    vec![1, 10],
                    "Segmented vertical separators must survive"
                );
                for (r, label, span) in
                    [(2, "(A)", 4), (6, "(B)", 2), (8, "(C)", 7), (15, "(D)", 4)]
                {
                    assert_eq!(
                        (cell(r, 0).text.as_str(), cell(r, 0).rowspan),
                        (label, span)
                    );
                }
                assert_eq!(cell(11, 2).text, "256");
                assert_eq!(cell(19, 1).colspan, 9);
                assert_eq!(cell(20, 10).text, "4.33");
                assert_eq!(cell(20, 12).text, "213");
                assert!(cell(2, 1).text.is_empty());
            }
            let (_, frame) = grid.layout()?;
            assert!(frame.is_valid() && frame.paths.len() > 100);
            std::fs::write(
                out.join(format!("table-{number}-grid.json")),
                serde_json::to_vec_pretty(&grid)?,
            )?;
            std::fs::write(out.join(format!("table-{number}-typst.svg")), grid.svg()?)?;
            std::fs::write(
                out.join(format!("table-{number}.typ")),
                grid.typst_source()?,
            )?;
        }
        std::fs::write(out.join(format!("table-{number}.html")), &html)?;
    }
    assert_eq!(
        std::fs::read(&path)?,
        before,
        "Source PDF must remain unchanged"
    );
    Ok(())
}
