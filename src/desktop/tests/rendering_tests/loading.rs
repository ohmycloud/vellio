use super::*;

#[gpui_kit::test]
fn pdf_formatting_and_html_parsing_both_finish_before_body_is_painted(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut reader = None;
    let (_root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut reader = translated_reader(fixture(), None, None, window, cx);
            // Hold the PDF worker pending so the first frame is deterministic.
            reader.pdf_code_loading.insert(0);
            reader
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    draw(cx);
    assert!(cx.debug_bounds("reader-text-loading").is_some());
    assert!(
        cx.debug_bounds("reader-text-ready").is_none(),
        "do not paint flattened extraction while PDF metadata is pending"
    );
    reader.read_with(cx, |reader, cx| {
        assert_eq!(
            reader.text_views[&(0, false)]
                .1
                .read(cx)
                .bounds()
                .size
                .height,
            px(0.)
        );
    });

    // Large rich HTML also parses asynchronously. Even after PDF recovery has
    // completed, the old paragraph AST must not appear during this second stage.
    let html = format!(
        "<pre><code>{}</code></pre>",
        "def run():\n    return 'formatted code'\n".repeat(150)
    );
    finish_presentation(&reader, Some(html), cx);
    draw(cx);
    assert!(cx.debug_bounds("reader-text-loading").is_some());
    assert!(cx.debug_bounds("reader-text-ready").is_none());
    reader.read_with(cx, |reader, cx| {
        assert!(!reader.text_views[&(0, false)].1.read(cx).is_content_ready());
    });

    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("reader-text-loading").is_none());
    assert!(cx.debug_bounds("reader-text-ready").is_some());
    reader.read_with(cx, |reader, cx| {
        let (source, view) = &reader.text_views[&(0, false)];
        assert!(source.starts_with("<pre>"));
        assert!(view.read(cx).is_content_ready());
        assert!(view.read(cx).bounds().size.height > px(0.));
    });

    // Re-entering a cached page should paint its final formatting immediately.
    reader.update(cx, |reader, cx| {
        reader.reset_rows();
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("reader-text-ready").is_some());
    assert!(cx.debug_bounds("reader-text-loading").is_none());
}

#[gpui_kit::test]
fn background_pdf_preparation_warms_the_view_and_finishes_its_queue(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("terms.pdf");
    pdf_fixtures::definitions_pdf_fixture(&source);
    let ws = easyread_gpui::store::Library::open(temp.path().join("library"))
        .unwrap()
        .import_pdf(&source, None)
        .unwrap();
    let data = ws.state().unwrap();
    let mut reader = None;
    let (_root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| translated_reader(data, None, Some(ws.clone()), window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    draw(cx);
    cx.run_until_parked();
    reader.update(cx, |reader, cx| {
        assert!(!reader.pdf_code_active);
        assert!(!reader.pdf_code_needs_plan);
        assert!(reader.pdf_code_loading.is_empty());
        assert_eq!(reader.pdf_code_prepared.len(), 1);
        assert!(reader.pdf_code_html[&0].1.is_some());
        assert!(reader.text_views[&(0, false)].1.read(cx).is_content_ready());
        // Reopening consumes the same disposable cache and prepares the AST.
        reader.load_outline(cx);
    });
    cx.run_until_parked();
    reader.update(cx, |reader, cx| {
        assert!(!reader.pdf_code_active);
        assert_eq!(reader.pdf_code_prepared.len(), 1);
        assert!(reader.text_views[&(0, false)].1.read(cx).is_content_ready());
    });
    draw(cx);
    assert!(cx.debug_bounds("reader-text-ready").is_some());
    assert!(cx.debug_bounds("reader-text-loading").is_none());
    assert!(ws.root.join(".native-pdf-layout-cache").is_dir());
}

#[gpui_kit::test]
fn fast_page_turns_use_prewarmed_asts_without_running_the_parser_between_turns(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let mut data = fixture();
    data["paper"]["blocks"] = Value::Array(
        (0..12)
            .map(|i| {
                json!({
                    "id": format!("page-{i}"), "type": "para", "page": i + 1,
                    "source_kind": "pdf-text", "en": format!("Code on page {i}")
                })
            })
            .collect(),
    );
    let mut reader = None;
    let (_root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut reader = translated_reader(data, None, None, window, cx);
            for i in 0..12 {
                let html = format!(
                    "<pre><code>{}</code></pre>",
                    format!("print('page {i}')\n").repeat(400)
                );
                assert!(html.len() > 4096);
                reader.pdf_code_html.insert(
                    i,
                    (
                        reader.state.paper.blocks[i].en.clone(),
                        Some(CodePresentation {
                            html,
                            math: vec![],
                            inline_figures: vec![],
                            figures: vec![],
                        }),
                    ),
                );
                reader.warm_pdf_text(i, cx);
            }
            reader
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    cx.run_until_parked(); // Complete offscreen preparation once, before flipping.
    for i in [0, 1, 2, 3, 4, 5, 6, 7, 6, 5, 4, 3, 2, 1] {
        reader.update(cx, |reader, cx| {
            assert!(reader.text_views[&(i, false)].1.read(cx).is_content_ready());
            reader.list.scroll_to(ListOffset {
                item_ix: i,
                offset_in_item: px(0.),
            });
            cx.notify();
        });
        draw(cx); // No run_until_parked: a newly created AST would still be pending.
        assert!(
            cx.debug_bounds("reader-text-ready").is_some(),
            "page {i} not ready"
        );
        assert!(
            cx.debug_bounds("reader-text-loading").is_none(),
            "loader flashed on page {i}"
        );
    }
}

#[gpui_kit::test]
fn translated_pages_are_prefetched_even_while_pdf_recovery_is_pending(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut data = fixture();
    data["paper"]["blocks"] = Value::Array(
        (0..12)
            .map(|i| {
                json!({
                    "id": format!("page-{i}"), "type": "para", "page": i + 1,
                    "source_kind": "pdf-text", "en": format!("Original page {i}"),
                    "zh": format!("第 {i} 页的中文译文。\n").repeat(300)
                })
            })
            .collect(),
    );
    let mut reader = None;
    let (_root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut reader = translated_reader(data, None, None, window, cx);
            reader.pdf_code_loading.extend(0..12);
            reader
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    draw(cx); // Starts nearby translated ASTs without any PDF result.
    cx.run_until_parked();
    for i in [1, 2, 3, 4, 5, 6, 7, 6, 5, 4] {
        reader.update(cx, |reader, cx| {
            assert!(reader.pdf_code_pending(i));
            let (source, view) = &reader.text_views[&(i, false)];
            assert!(source.len() > 4096);
            assert!(view.read(cx).is_content_ready());
            reader.list.scroll_to(ListOffset {
                item_ix: i,
                offset_in_item: px(0.),
            });
            cx.notify();
        });
        draw(cx);
        assert!(cx.debug_bounds("reader-text-ready").is_some());
        assert!(
            cx.debug_bounds("reader-text-loading").is_none(),
            "translated page {i} flashed loader"
        );
    }
}

#[gpui_kit::test]
fn missing_rich_presentation_uses_plain_text_after_analysis_not_an_endless_loader(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let mut reader = None;
    let (_root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut reader = translated_reader(fixture(), None, None, window, cx);
            reader.pdf_code_loading.insert(0);
            reader
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    draw(cx);
    assert!(cx.debug_bounds("reader-text-ready").is_none());
    finish_presentation(&reader, None, cx);
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("reader-text-loading").is_none());
    assert!(cx.debug_bounds("reader-text-ready").is_some());
}

#[gpui_kit::test]
fn translated_body_is_not_held_back_by_original_pdf_recovery(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut data = fixture();
    data["paper"]["blocks"][0]["zh"] = json!("译文应该立即显示，而不是等待原文排版。");
    let (reader, cx) = cx.add_window_view(|window, cx| {
        let mut reader = ReaderView::new(data, None, None, window, cx);
        reader.pdf_code_loading.insert(0);
        reader.reading_mode = ReadingMode::Translation;
        reader
    });
    draw(cx);
    assert!(cx.debug_bounds("reader-text-ready").is_some());
    assert!(cx.debug_bounds("reader-text-loading").is_none());
    reader.read_with(cx, |reader, cx| {
        assert!(reader.text_views[&(0, false)].1.read(cx).is_content_ready());
    });
}
