use super::*;

#[gpui_kit::test]
fn complex_pdf_table_paints_native_typst_paths_and_copies_structured_text(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("complex-table.pdf");
    pdf_fixtures::complex_table_pdf_fixture(&source);
    let ws = easyread_gpui::store::Library::open(temp.path().join("library"))
        .unwrap()
        .import_pdf(&source, None)
        .unwrap();
    let raw = ws.state().unwrap();
    let original = raw.clone();
    let text = raw["paper"]["blocks"][0]["en"].as_str().unwrap().to_owned();
    let p = easyread_gpui::pdf::code_presentation(&source, 1, &text)
        .unwrap()
        .unwrap();
    assert!(p.html.contains("data-vellio-table=\"true\"") && p.figures.is_empty());
    assert!(!p.html.contains("<img "));
    let start = p.html.find("<table").unwrap();
    let end = p.html.find("</table>").unwrap() + 8;
    let grid = easyread_gpui::typst_table::TableGrid::from_html(&p.html[start..end]).unwrap();
    let (_, table) = grid.layout().unwrap();
    let tsv = grid.tsv();
    let mut reader = None;
    let (_root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut reader = translated_reader(raw, None, None, window, cx);
            reader.pdf_code_html.insert(0, (text, Some(p)));
            reader
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    for width in [25., 35.] {
        reader.update(cx, |reader, cx| {
            reader.appearance.line_width = width;
            reader.reset_rows();
            cx.notify();
        });
        draw(cx);
        cx.run_until_parked();
        draw(cx);
        cx.run_until_parked();
        draw(cx);
        let bounds = reader.read_with(cx, |reader, cx| {
            let view = reader.text_views[&(0, false)].1.read(cx);
            let bounds = view.paragraph_bounds(1).expect("native table must paint");
            assert!(bounds.size.height > px(50.), "{bounds:?}");
            assert!(view.paragraph_bounds(0).unwrap().bottom() <= bounds.top());
            assert!(bounds.bottom() <= view.paragraph_bounds(2).unwrap().top());
            assert!(
                reader.block_images.is_empty(),
                "No duplicate rule/figure blocks"
            );
            assert_eq!(reader.raw, original);
            bounds
        });
        assert!(cx.debug_bounds("reader-native-typst-table").is_some());
        let painted = cx
            .debug_bounds("reader-native-typst-table-paths")
            .expect("Native table paths must paint");
        let ratio = table.width / table.height;
        assert!(
            (f32::from(painted.size.width) / f32::from(painted.size.height) - ratio).abs() < 0.02,
            "{painted:?}"
        );
        cx.simulate_click(bounds.center(), gpui_kit::Modifiers::default());
        reader.update(cx, |reader, cx| {
            assert!(
                reader.image_preview.is_none(),
                "Table is not an image preview"
            );
            let view = reader.text_views[&(0, false)].1.clone();
            view.update(cx, |v, cx| v.select_all(cx));
            let copy = view.read(cx).selected_text();
            assert!(
                copy.contains(&tsv),
                "Table copy must retain rows, empty cells and values: {copy}"
            );
            view.update(cx, |v, cx| v.clear_selection(cx));
            cx.notify();
        });
    }
}

fn verify_vertical_table_ui(raw: Value, p: CodePresentation, cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let original = raw.clone();
    let text = raw["paper"]["blocks"][0]["en"].as_str().unwrap().to_owned();
    assert!(!p.html.contains("<img") && p.figures.is_empty());
    let start = p.html.find("<table").unwrap();
    let end = p.html.find("</table>").unwrap() + 8;
    let grid = easyread_gpui::typst_table::TableGrid::from_html(&p.html[start..end]).unwrap();
    let tsv = grid.tsv();
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut reader = translated_reader(raw, None, None, window, cx);
            reader.pdf_code_html.insert(0, (text, Some(p)));
            reader
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    for width in [25., 35.] {
        reader.update(cx, |reader, cx| {
            reader.appearance.line_width = width;
            reader.reset_rows();
            cx.notify();
        });
        for _ in 0..3 {
            draw(cx);
            cx.run_until_parked();
        }
        let bounds = cx
            .debug_bounds("reader-native-typst-table-paths")
            .expect("Table paints native vectors");
        cx.simulate_click(bounds.center(), gpui_kit::Modifiers::default());
        reader.update(cx, |reader, cx| {
            assert!(reader.image_preview.is_none());
            for (i, block) in reader.state.paper.blocks.iter().enumerate() {
                if block.kind == "figure" {
                    assert!(
                        reader.reader_suppresses_pdf_figure(i),
                        "Hide the legacy table screenshot"
                    );
                }
            }
            assert!(reader.block_images.is_empty());
            let view = reader.text_views[&(0, false)].1.clone();
            view.update(cx, |v, cx| v.select_all(cx));
            assert!(
                view.read(cx).selected_text().contains(&tsv),
                "Copy keeps every cell and row"
            );
            view.update(cx, |v, cx| v.clear_selection(cx));
            assert_eq!(reader.raw, original);
            cx.notify();
        });
    }
}

#[gpui_kit::test]
fn single_header_rule_table_paints_native_vectors_and_hides_legacy_screenshot(
    cx: &mut TestAppContext,
) {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("vertical-table.pdf");
    pdf_fixtures::vertical_table_pdf_fixture(&source);
    let ws = easyread_gpui::store::Library::open(temp.path().join("library"))
        .unwrap()
        .import_pdf(&source, None)
        .unwrap();
    let mut raw = ws.state().unwrap();
    let text = raw["paper"]["blocks"][0]["en"].as_str().unwrap();
    let p = easyread_gpui::pdf::code_presentation(&source, 1, text)
        .unwrap()
        .unwrap();
    raw["paper"]["blocks"].as_array_mut().unwrap().push(json!({
        "id":p.inline_figures[0], "type":"figure", "page":1,
        "src":"pages/page-001.webp", "source_kind":"pdf-image"
    }));
    verify_vertical_table_ui(raw, p, cx);
}

#[gpui_kit::test]
#[ignore = "requires EASYREAD_QA_WORKSPACE pointing to Agentic Large Language Models"]
fn agentic_page20_table_paints_native_vectors_and_hides_imported_image(cx: &mut TestAppContext) {
    let ws = easyread_gpui::store::Workspace {
        root: std::env::var("EASYREAD_QA_WORKSPACE").unwrap().into(),
    };
    let mut raw = ws.state().unwrap();
    raw["paper"]["blocks"]
        .as_array_mut()
        .unwrap()
        .retain(|b| b["page"] == 20);
    let text = raw["paper"]["blocks"][0]["en"].as_str().unwrap();
    let p = easyread_gpui::pdf::code_presentation(&ws.asset("source.pdf").unwrap(), 20, text)
        .unwrap()
        .unwrap();
    assert_eq!(p.html.matches("<th ").count(), 5);
    assert_eq!(p.html.matches("<td ").count(), 15);
    verify_vertical_table_ui(raw, p, cx);
}

#[gpui_kit::test]
#[ignore = "requires fresh Attention PDF QA table HTML"]
fn attention_complex_tables_paint_native_vectors_and_copy_all_real_rows(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    for number in [2, 3] {
        let page_html =
            std::fs::read_to_string(format!("target/attention-tables-qa/table-{number}.html"))
                .unwrap();
        let start = page_html.find("<table").unwrap();
        let end = page_html.find("</table>").unwrap() + 8;
        let table_html = &page_html[start..end];
        let grid = easyread_gpui::typst_table::TableGrid::from_html(table_html).unwrap();
        let tsv = grid.tsv();
        let html =
            format!("<p>Table {number}: real PDF table</p>{table_html}<p>Following prose.</p>");
        let source = easyread_gpui::annotations::plain_text(&html);
        let raw = json!({"paper":{"meta":{"title_en":"Real table UI QA"},"blocks":[{"id":"real-table","type":"para","page":1,"source_kind":"pdf-text","en":source}]},"reader":{}});
        let original = raw.clone();
        let mut reader = None;
        let (_root, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| {
                let mut reader = translated_reader(raw, None, None, window, cx);
                reader.pdf_code_html.insert(
                    0,
                    (
                        source,
                        Some(CodePresentation {
                            html,
                            inline_figures: vec![],
                            figures: vec![],
                            math: vec![],
                        }),
                    ),
                );
                reader
            });
            reader = Some(view.clone());
            Root::new(view, window, cx)
        });
        let reader = reader.unwrap();
        for width in [25., 35.] {
            reader.update(cx, |reader, cx| {
                reader.appearance.line_width = width;
                reader.reset_rows();
                cx.notify();
            });
            draw(cx);
            cx.run_until_parked();
            draw(cx);
            cx.run_until_parked();
            draw(cx);
            let painted = cx
                .debug_bounds("reader-native-typst-table-paths")
                .expect("Real table must paint native paths");
            assert!(painted.size.width > px(400.) && painted.size.height > px(120.));
            reader.update(cx, |reader, cx| {
                let view = reader.text_views[&(0, false)].1.clone();
                let state = view.read(cx);
                let bounds = state.paragraph_bounds(1).unwrap();
                assert!(state.paragraph_bounds(0).unwrap().bottom() <= bounds.top());
                assert!(bounds.bottom() <= state.paragraph_bounds(2).unwrap().top());
                view.update(cx, |v, cx| v.select_all(cx));
                assert!(
                    view.read(cx).selected_text().contains(&tsv),
                    "Every original row/empty cell must copy"
                );
                view.update(cx, |v, cx| v.clear_selection(cx));
                assert!(reader.image_preview.is_none() && reader.block_images.is_empty());
                assert_eq!(reader.raw, original);
            });
        }
    }
}
