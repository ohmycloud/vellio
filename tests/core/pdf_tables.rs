use super::*;

#[test]
fn pdf_tables_restore_cells_links_and_annotations_without_rewriting_the_book() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("table.pdf");
    support::table_pdf_fixture(&source);
    let pdf_before = fs::read(&source).unwrap();
    let lib = Library::open(temp.path().join("library")).unwrap();
    let ws = lib.import_pdf(&source, None).unwrap();
    let before: Vec<_> = ["paper", "reader", "layout"]
        .iter()
        .map(|name| fs::read(ws.root.join(format!("{name}.json"))).unwrap())
        .collect();
    let state = ws.state().unwrap();
    let blocks = state["paper"]["blocks"].as_array().unwrap();
    assert_eq!(
        blocks.len(),
        1,
        "Borders must not become duplicate figure blocks"
    );
    let text = blocks[0]["en"].as_str().unwrap();
    let presentation = easyread_gpui::pdf::code_presentation(&source, 1, text)
        .unwrap()
        .unwrap();
    let html = &presentation.html;
    assert_eq!(html.matches("<table>").count(), 1, "{html}");
    assert_eq!(html.matches("<th ").count(), 3);
    assert_eq!(html.matches("<td ").count(), 6);
    assert!(html.contains("Projection-<br>based"), "{html}");
    assert!(
        html.contains("<em>Simple: </em>Lightweight &amp; fast."),
        "{html}"
    );
    assert!(
        html.contains("href=\"https://example.org/latency\""),
        "{html}"
    );
    assert!(html.contains("</table><p><span>Following body"), "{html}");
    let normalize = |s: &str| {
        s.replace("-\n", "-")
            .replace("\u{2}\n", "-")
            .replace('\u{2}', "-")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    assert_eq!(
        normalize(text),
        normalize(&easyread_gpui::annotations::plain_text(html))
    );
    let marked = easyread_gpui::annotations::decorate_interactive(
        html,
        &[json!({
            "id":"table-note", "quote":"Low latency and few parameters.", "kind":"highlight", "style":"underline", "color":"pink"
        })],
    );
    assert!(marked.contains("table-note"), "{marked}");
    assert!(marked.contains("text-decoration-color:#cc413a"));
    assert_eq!(
        easyread_gpui::annotations::plain_text(&marked),
        easyread_gpui::annotations::plain_text(html)
    );
    assert!(
        easyread_gpui::pdf::code_html(&source, 1, "edited source")
            .unwrap()
            .is_none()
    );
    assert_eq!(fs::read(&source).unwrap(), pdf_before);
    for (name, bytes) in ["paper", "reader", "layout"].iter().zip(before) {
        assert_eq!(
            fs::read(ws.root.join(format!("{name}.json"))).unwrap(),
            bytes
        );
    }
}

#[test]
fn complex_table_keeps_spanning_cells_and_native_typst_not_images() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("complex-table.pdf");
    support::complex_table_pdf_fixture(&source);
    let pdf_before = fs::read(&source).unwrap();
    let ws = Library::open(temp.path().join("library"))
        .unwrap()
        .import_pdf(&source, None)
        .unwrap();
    let before = ws.state().unwrap();
    let text = before["paper"]["blocks"][0]["en"].as_str().unwrap();
    assert_eq!(
        before["paper"]["blocks"].as_array().unwrap().len(),
        1,
        "Rules must not become duplicate figure assets"
    );
    let p = easyread_gpui::pdf::code_presentation(&source, 1, text)
        .unwrap()
        .unwrap();
    assert_eq!(p.html.matches("<table").count(), 1, "{}", p.html);
    assert!(p.html.contains("data-vellio-table=\"true\""));
    assert!(p.html.contains("ByteNet") && p.html.contains("Recurrent"));
    assert!(
        p.figures.is_empty() && !p.html.contains("<img ") && !p.html.contains("easyread-image:")
    );
    assert!(
        text.contains("ByteNet") && text.contains("Recurrent"),
        "Source extraction remains intact"
    );
    let visible = easyread_gpui::annotations::plain_text(&p.html);
    assert!(visible.contains("Table 2: Comparison"));
    assert!(visible.contains("Following prose remains selectable"));
    let start = p.html.find("<table").unwrap();
    let end = p.html.find("</table>").unwrap() + 8;
    let grid = easyread_gpui::typst_table::TableGrid::from_html(&p.html[start..end]).unwrap();
    assert_eq!(grid.columns.len(), 5, "Sparse rows must not erase a column");
    assert_eq!(
        grid.cells
            .iter()
            .find(|c| c.row == 0 && c.col == 0)
            .unwrap()
            .rowspan,
        2
    );
    assert_eq!(
        grid.cells
            .iter()
            .find(|c| c.row == 0 && c.col == 1)
            .unwrap()
            .colspan,
        2
    );
    let (_, frame) = grid.layout().unwrap();
    assert!(frame.is_valid() && frame.paths.len() > 100);
    let display = easyread_gpui::typst_table::prepare_html(&p.html).unwrap();
    assert!(display.contains("data-vellio-table-key=") && !display.contains("<img "));
    assert!(
        grid.tsv().contains("ByteNet\t23.75\t\t\t"),
        "{}",
        grid.tsv()
    );
    assert_eq!(fs::read(&source).unwrap(), pdf_before);
    assert_eq!(
        ws.state().unwrap(),
        before,
        "Do not rewrite text, edits or notes to fix display"
    );
}

#[test]
fn comparison_tables_with_empty_headers_and_headerless_continuations_stay_selectable() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("comparison.pdf");
    support::split_comparison_pdf_fixture(&source);
    let pdf_before = fs::read(&source).unwrap();
    let ws = Library::open(temp.path().join("library"))
        .unwrap()
        .import_pdf(&source, None)
        .unwrap();
    let before: Vec<_> = ["paper", "reader", "layout"]
        .iter()
        .map(|name| fs::read(ws.root.join(format!("{name}.json"))).unwrap())
        .collect();
    let state = ws.state().unwrap();
    let blocks = state["paper"]["blocks"].as_array().unwrap();
    assert_eq!(blocks.len(), 2, "Neither shaded row becomes a figure");
    for (i, block) in blocks.iter().enumerate() {
        let text = block["en"].as_str().unwrap();
        let presentation = easyread_gpui::pdf::code_presentation(&source, i + 1, text)
            .unwrap()
            .unwrap();
        let html = &presentation.html;
        assert_eq!(html.matches("<table>").count(), 1, "{html}");
        assert_eq!(html.matches("<th ").count(), if i == 0 { 3 } else { 0 });
        assert_eq!(html.matches("<td ").count(), if i == 0 { 6 } else { 3 });
        assert!(!html.contains("<img "), "No duplicate table crops: {html}");
        let tag = if i == 0 { "th" } else { "td" };
        assert!(
            html.contains(&format!("<{tag} width=\"17.949%\"> </{tag}>")),
            "{html}"
        );
        let normalize = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        assert_eq!(
            normalize(text),
            normalize(&easyread_gpui::annotations::plain_text(html))
        );
        if i == 1 {
            assert!(html.contains("</table><p><span>Following body"), "{html}");
            assert!(
                html.contains("href=\"https://example.org/sources\""),
                "{html}"
            );
            let marked = easyread_gpui::annotations::decorate_interactive(
                html,
                &[
                    json!({"id":"continuation-note", "quote":"Too many sources.", "kind":"underline", "color":"blue"}),
                ],
            );
            assert!(marked.contains("continuation-note"));
            assert_eq!(
                easyread_gpui::annotations::plain_text(&marked),
                easyread_gpui::annotations::plain_text(html)
            );
        }
    }
    assert_eq!(fs::read(&source).unwrap(), pdf_before);
    for (name, bytes) in ["paper", "reader", "layout"].iter().zip(before) {
        assert_eq!(
            fs::read(ws.root.join(format!("{name}.json"))).unwrap(),
            bytes
        );
    }
}

#[test]
fn text_panel_backgrounds_keep_code_prose_and_notes_selectable_but_preserve_diagrams() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("text-panels.pdf");
    support::text_panels_pdf_fixture(&source);
    let lib = Library::open(temp.path().join("library")).unwrap();
    let ws = lib.import_pdf(&source, None).unwrap();
    let before: Vec<_> = ["paper", "reader", "layout"]
        .iter()
        .map(|n| fs::read(ws.root.join(format!("{n}.json"))).unwrap())
        .collect();
    let raw = ws.state().unwrap();
    let blocks = raw["paper"]["blocks"].as_array().unwrap();
    assert_eq!(
        blocks
            .iter()
            .filter(|b| b["source_kind"] == "pdf-image")
            .count(),
        1
    );
    let text = blocks
        .iter()
        .find(|b| b["source_kind"] == "pdf-text")
        .unwrap()["en"]
        .as_str()
        .unwrap();
    let p = easyread_gpui::pdf::code_presentation(&source, 1, text)
        .unwrap()
        .unwrap();
    assert_eq!(p.html.matches("<img ").count(), 1, "{}", p.html);
    assert_eq!(p.html.matches("MEMORYBANK").count(), 1);
    assert!(p.html.contains("TinyAgent/"));
    assert!(p.html.contains("An interesting take on RAG"));
    assert!(p.inline_figures.contains(&"pdf-p001-img001".into()));
    assert!(p.inline_figures.contains(&"pdf-p001-img006".into()));
    let marked = easyread_gpui::annotations::decorate_interactive(
        &p.html,
        &[
            json!({"id":"panel-note", "quote":"An interesting take on RAG", "kind":"highlight", "color":"blue"}),
            json!({"id":"tree-note", "quote":"agent.py", "kind":"underline", "color":"pink"}),
        ],
    );
    assert!(marked.contains("panel-note") && marked.contains("tree-note"));
    assert_eq!(
        easyread_gpui::annotations::plain_text(&marked),
        easyread_gpui::annotations::plain_text(&p.html)
    );
    for (n, bytes) in ["paper", "reader", "layout"].iter().zip(before) {
        assert_eq!(fs::read(ws.root.join(format!("{n}.json"))).unwrap(), bytes);
    }
}

#[test]
fn fragmented_body_between_figures_stays_selectable_and_outside_the_crops() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("prose-between-figures.pdf");
    support::prose_between_figures_fixture(&source);
    let pdf_before = fs::read(&source).unwrap();
    let ws = Library::open(temp.path().join("library"))
        .unwrap()
        .import_pdf(&source, None)
        .unwrap();
    let before: Vec<_> = ["paper", "reader", "layout"]
        .iter()
        .map(|n| fs::read(ws.root.join(format!("{n}.json"))).unwrap())
        .collect();
    let state = ws.state().unwrap();
    let text = state["paper"]["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["source_kind"] == "pdf-text")
        .unwrap()["en"]
        .as_str()
        .unwrap();
    let p = easyread_gpui::pdf::code_presentation(&source, 1, text)
        .unwrap()
        .unwrap();
    let prose = "Let’s go over some examples of how these agentic RAG systems work through impactful papers and implementations in the field.";
    assert!(
        p.html.contains(&format!("<p><span>{prose}</span></p>")),
        "{}",
        easyread_gpui::annotations::plain_text(&p.html)
    );
    assert_eq!(p.figures.len(), 1);
    assert!(
        p.figures[0].height < 140,
        "Body must not extend the upper crop"
    );
    assert!(p.html.contains("<blockquote>"), "{}", p.html);
    assert!(
        easyread_gpui::annotations::plain_text(&p.html).contains("Text inside the note panel.")
    );
    assert!(
        p.html.find("</blockquote>").unwrap() < p.html.find("Following body paragraph.").unwrap()
    );
    let marked = easyread_gpui::annotations::decorate_interactive(
        &p.html,
        &[json!({"id":"body-note", "quote":prose, "kind":"underline", "color":"blue"})],
    );
    assert!(marked.contains("body-note"));
    assert_eq!(
        easyread_gpui::annotations::plain_text(&marked),
        easyread_gpui::annotations::plain_text(&p.html)
    );
    assert_eq!(fs::read(source).unwrap(), pdf_before);
    for (name, bytes) in ["paper", "reader", "layout"].iter().zip(before) {
        assert_eq!(
            fs::read(ws.root.join(format!("{name}.json"))).unwrap(),
            bytes
        );
    }
}
