use super::*;

#[gpui_kit::test]
fn translated_pdf_shared_images_stay_with_caption_without_duplicate_blocks(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("panels.pdf");
    pdf_fixtures::shared_caption_panels_fixture(&source, "roman");
    let ws = easyread_gpui::store::Library::open(temp.path().join("library"))
        .unwrap()
        .import_pdf(&source, None)
        .unwrap();
    let raw = ws.state().unwrap();
    let en = raw["paper"]["blocks"][0]["en"].as_str().unwrap();
    let presentation = easyread_gpui::pdf::code_presentation(&source, 1, en)
        .unwrap()
        .unwrap();
    assert_eq!(presentation.inline_figures.len(), 2);
    for saved in [false, true] {
        let mut raw = raw.clone();
        if saved {
            let plan =
                easyread_gpui::translation_layout::Plan::from_html(&presentation.html).unwrap();
            let translated = plan
                .translate(&plan.input().replace("Figure 2:", "图 2："))
                .unwrap();
            raw["paper"]["blocks"][0]["zh"] = json!(translated.markdown);
            let mut meta = easyread_gpui::translation_layout::metadata(en, &translated);
            meta["inline_figures"] = json!(presentation.inline_figures);
            raw["paper"]["blocks"][0]["native_translation_layout"] = meta;
        } else {
            raw["paper"]["blocks"][0]["zh"] = json!("图内标签\n图 2：左右两幅子图。\n后续正文。");
        }
        let mut reader = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| {
                let mut reader = translated_reader(raw, None, None, window, cx);
                if !saved {
                    reader
                        .pdf_code_html
                        .insert(0, (en.into(), Some(presentation.clone())));
                }
                reader
            });
            reader = Some(view.clone());
            Root::new(view, window, cx)
        });
        let reader = reader.unwrap();
        for _ in 0..3 {
            draw(cx);
            cx.run_until_parked();
        }
        reader.update(cx, |reader, _| {
            let html = reader.reader_layout_html(0, false);
            assert!(html.contains("<img "));
            assert!(html.find("<img ") < html.find("图 2："));
            let indices: Vec<_> = reader.state.paper.blocks.iter().enumerate()
                .filter(|(_, b)| presentation.inline_figures.contains(&b.id)).map(|(i,_)| i).collect();
            for index in &indices {
                assert!(reader.reader_suppresses_pdf_figure(*index), "Shared source panels already embedded in translation must not render a second time");
            }
            // User revisions take precedence; if they omit the embedded image,
            // keep the original figure blocks rather than hiding them.
            reader.raw["reader"]["edits"]["pdf-p001"] = json!({"zh":"用户手动修订"});
            reader.state = easyread_gpui::model::ReaderState::from_value(reader.raw.clone()).unwrap();
            for index in indices { assert!(!reader.reader_suppresses_pdf_figure(index)); }
        });
    }
}

#[gpui_kit::test]
fn shared_pdf_panels_paint_one_proportional_image_and_preview_without_legacy_duplicates(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("shared-panels.pdf");
    pdf_fixtures::shared_caption_panels_fixture(&source, "raster");
    let ws = easyread_gpui::store::Library::open(temp.path().join("library"))
        .unwrap()
        .import_pdf(&source, None)
        .unwrap();
    let mut raw = ws.state().unwrap();
    let text = raw["paper"]["blocks"][0]["en"].as_str().unwrap().to_owned();
    let p = easyread_gpui::pdf::code_presentation(&source, 1, &text)
        .unwrap()
        .unwrap();
    let figure = p.figures[0].clone();
    // Simulate a workspace imported before shared-caption grouping: two old
    // image IDs are still present and must not create rows below the page.
    let old = raw["paper"]["blocks"][1].clone();
    let mut other = old.clone();
    other["id"] = json!("pdf-p001-img005");
    raw["paper"]["blocks"] = json!([raw["paper"]["blocks"][0], old, other]);
    let original = raw.clone();
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
        reader.update(cx, |reader, cx| {
            reader.text_views[&(0, false)]
                .1
                .update(cx, |view, cx| view.set_track_paragraphs(true, cx));
        });
        draw(cx);
        cx.run_until_parked();
        draw(cx);
        let image_bounds = reader.read_with(cx, |reader, cx| {
            let view = reader.text_views[&(0, false)].1.read(cx);
            let image = view.paragraph_bounds(1).expect("shared image paragraph must paint");
            let ratio = figure.width as f32 / figure.height as f32;
            assert!((f32::from(image.size.width) / f32::from(image.size.height) - ratio).abs() < 0.02,
                "shared crop reserves proportional native geometry at width {width}: {image:?}, ratio={ratio}");
            assert!(view.paragraph_bounds(0).unwrap().bottom() <= image.top());
            assert!(image.bottom() <= view.paragraph_bounds(2).unwrap().top());
            assert!(reader.block_images.is_empty(), "both legacy image rows must stay suppressed");
            assert_eq!(reader.raw, original);
            image
        });
        cx.simulate_click(image_bounds.center(), gpui_kit::Modifiers::default());
        reader.update(cx, |reader, cx| {
            let preview = reader
                .image_preview
                .as_ref()
                .expect("shared crop click opens preview");
            assert_eq!(preview.image.bytes(), figure.png);
            assert_eq!(preview.aspect, figure.width as f32 / figure.height as f32);
            reader.image_preview = None;
            cx.notify();
        });
    }
}
