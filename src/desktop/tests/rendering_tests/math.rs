use super::*;

#[gpui_kit::test]
fn translated_pdf_keeps_separate_heading_caption_and_native_equation(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let tex = r"\frac{QK^{T}}{\sqrt{d_{k}}}";
    let equation = easyread_gpui::formula::layout(tex, true).unwrap();
    easyread_gpui::formula::layout("d_{k}", false).unwrap();
    let recovered = format!(
        "<p style=\"text-align:center\"><span>Figure 1: Caption.</span></p><h3>3.2.1 Attention</h3><p><span>Use {}.</span></p><p style=\"text-align:center\">{} (1)</p>",
        easyread_gpui::formula::native_html("d_{k}", false),
        easyread_gpui::formula::native_html(tex, true)
    );
    let source = "Figure 1: Caption.\n3.2.1 Attention\nUse dk.\nf(x) = QKT/sqrt(dk) (1)";
    for saved in [false, true] {
        let (zh, metadata) = if saved {
            let plan = easyread_gpui::translation_layout::Plan::from_html(&recovered).unwrap();
            let result = plan
                .translate(
                    &plan
                        .input()
                        .replace("Figure 1: Caption.", "图 1：题注。")
                        .replace("3.2.1 Attention", "3.2.1 注意力")
                        .replace("Use ", "使用 "),
                )
                .unwrap();
            let metadata = easyread_gpui::translation_layout::metadata(source, &result);
            (result.markdown, metadata)
        } else {
            (
                "图 1：题注。\n3.2.1 注意力\n使用 dk。\nf(x) = QKᵀ/√dk (1)".into(),
                Value::Null,
            )
        };
        let raw = json!({"paper":{"meta":{"title_en":"Translation layout regression"},"blocks":[
            {"id":"translated-page","type":"para","source_kind":"pdf-text","page":1,"en":source,"zh":zh,"native_translation_layout":metadata}
        ]},"reader":{}});
        let original = raw.clone();
        let mut reader = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| {
                let mut reader = translated_reader(raw, None, None, window, cx);
                reader.pdf_code_html.insert(
                    0,
                    (
                        source.into(),
                        Some(CodePresentation {
                            html: recovered.clone(),
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
        for _ in 0..3 {
            draw(cx);
            cx.run_until_parked();
        }
        assert!(cx.debug_bounds("reader-native-typst-math").is_some());
        reader.read_with(cx, |reader, cx| {
            let html = reader.reader_layout_html(0, false);
            assert!(html.contains("<h3>3.2.1 注意力</h3>"));
            assert!(html.contains("text-align:center\"><span>图 1：题注。"));
            assert!(html.contains("data-display=\"true\""));
            let view = reader.text_views[&(0, false)].1.read(cx);
            for i in 0..3 {
                let a = view.paragraph_bounds(i).unwrap();
                let b = view.paragraph_bounds(i + 1).unwrap();
                assert!(
                    a.bottom() <= b.top(),
                    "Caption/heading/body/equation must not overlap: {a:?}, {b:?}"
                );
            }
            assert!(view.paragraph_bounds(3).unwrap().size.height >= px(equation.height));
            assert_eq!(
                reader.raw, original,
                "Rendering must not rewrite translations or reader data"
            );
        });
        reader.update(cx, |reader, _| {
            reader.raw["reader"]["edits"]["translated-page"] =
                json!({"zh":"## 用户标题\n\n用户手动修订"});
            reader.state =
                easyread_gpui::model::ReaderState::from_value(reader.raw.clone()).unwrap();
            let html = reader.reader_layout_html(0, false);
            assert!(html.contains("用户标题") && html.contains("用户手动修订"));
            assert!(
                !html.contains("3.2.1 注意力"),
                "Saved native layout must never replace a user edit"
            );
        });
    }
}

#[gpui_kit::test]
fn pdf_display_equation_paints_native_typst_paths_not_an_image_preview(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("equation.pdf");
    math_fixtures::fixture(&source, "display");
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
    let equation = p.math.iter().find(|m| m.display).unwrap().clone();
    assert!(
        equation.tex.contains("\\frac{QK^{T}}{\\sqrt{d_{k}}}"),
        "{}",
        equation.tex
    );
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
        let native = cx
            .debug_bounds("reader-native-typst-math")
            .expect("Native Typst math canvas must paint");
        let bounds = reader.read_with(cx, |reader, cx| {
            let view = reader.text_views[&(0, false)].1.read(cx);
            let bounds = view
                .paragraph_bounds(1)
                .expect("display equation must paint");
            let ratio = equation.layout.width / equation.layout.height;
            assert!(
                (f32::from(native.size.width) / f32::from(native.size.height) - ratio).abs() < 0.03
            );
            assert!(
                bounds.size.height >= native.size.height,
                "fraction must fit the text line at width {width}: {bounds:?}, {native:?}"
            );
            assert!(native.size.height >= px(equation.layout.height * 0.9));
            assert!(view.paragraph_bounds(0).unwrap().bottom() <= bounds.top());
            assert!(bounds.bottom() <= view.paragraph_bounds(2).unwrap().top());
            assert_eq!(reader.raw, original);
            bounds
        });
        cx.simulate_click(bounds.center(), gpui_kit::Modifiers::default());
        reader.update(cx, |reader, cx| {
            assert!(
                reader.image_preview.is_none(),
                "A native formula must not open an image preview"
            );
            cx.notify();
        });
    }
}

#[gpui_kit::test]
fn pdf_math_selection_opens_toolbar_and_fills_shared_line_cells(cx: &mut TestAppContext) {
    fn rows(cx: &mut VisualTestContext) -> Vec<gpui_kit::Bounds<gpui_kit::ScaledPixels>> {
        let mut cells = cx.update(|window, cx| {
            let color: gpui_kit::Background = gpui_kit::base::Theme::global(cx)
                .tokens
                .colors
                .selection
                .into();
            window
                .painted_quads()
                .into_iter()
                .filter(|q| {
                    q.background == color && q.bounds.size.width > gpui_kit::ScaledPixels(0.)
                })
                .map(|q| q.bounds)
                .collect::<Vec<_>>()
        });
        assert!(cells.len() > 3);
        cells.sort_by(|a, b| {
            a.top()
                .partial_cmp(&b.top())
                .unwrap()
                .then_with(|| a.left().partial_cmp(&b.left()).unwrap())
        });
        let mut rows: Vec<gpui_kit::Bounds<gpui_kit::ScaledPixels>> = Vec::new();
        for cell in cells {
            if let Some(row) = rows.last_mut().filter(|row| row.top() == cell.top()) {
                assert_eq!(
                    row.bottom(),
                    cell.bottom(),
                    "Formula and prose share the row height"
                );
                assert!(
                    (row.right() - cell.left()).0.abs() < 0.01,
                    "Formula selection cannot leave horizontal gaps: {row:?}, {cell:?}"
                );
                *row = gpui_kit::Bounds::from_corners(row.origin, cell.bottom_right());
            } else {
                rows.push(cell);
            }
        }
        for pair in rows.windows(2) {
            assert_eq!(
                pair[0].bottom(),
                pair[1].top(),
                "No formula-sized holes in selection rows: {rows:?}"
            );
        }
        rows
    }
    cx.update(gpui_kit::init);
    for tex in ["d_k", r"\frac{1}{\sqrt{d_k}}"] {
        easyread_gpui::formula::layout(tex, false).unwrap();
    }
    let markdown = r"While for small values of $d_k$ the two mechanisms perform similarly, additive attention outperforms dot product attention without scaling for larger values of $d_k$ [3](https://example.org/3). We suspect that for large values of $d_k$, the dot products grow large in magnitude, pushing the softmax function into regions where it has extremely small gradients. To counteract this effect, we scale the dot products by $\frac{1}{\sqrt{d_k}}$.";
    // Recovered PDF math copies TeX, while the stored extraction has separate
    // glyphs without dollar delimiters, subscripts, fraction or radical syntax.
    let source = markdown
        .replace("$d_k$", "dk")
        .replace(r"$\frac{1}{\sqrt{d_k}}$", "1√dk");
    let html = easyread_gpui::formula::native_markdown_html(markdown);
    let plain = easyread_gpui::annotations::plain_text(&html);
    let raw = json!({"paper":{"meta":{"title_en":"PDF math selection regression"},"blocks":[
        {"id":"pdf-math","type":"para","page":1,"source_kind":"pdf-text","en":source}
    ]},"reader":{}});
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
    for (width, font) in [(25., 21.), (35., 21.), (25., 27.)] {
        reader.update(cx, |reader, cx| {
            reader.appearance.line_width = width;
            reader.appearance.font_size = font;
            reader.reset_rows();
            cx.notify();
        });
        draw(cx);
        cx.run_until_parked();
        draw(cx);
        reader.update(cx, |reader, cx| {
            let view = reader.text_views[&(0, false)].1.clone();
            view.update(cx, |view, cx| view.select_all(cx));
            assert_eq!(view.read(cx).selected_text().trim_end(), plain);
        });
        draw(cx);
        reader.update(cx, |reader, cx| {
            reader.capture_selection(gpui_kit::point(px(400.), px(300.)), cx);
            let selection = reader
                .selection
                .as_ref()
                .expect("Math selection must open the toolbar");
            assert_eq!(selection.quote, plain);
            assert_eq!(selection.key, "pdf-math");
            assert_eq!(reader.raw, original);
        });
        let selected_rows = rows(cx);
        let scale = cx.update(|window, _| window.scale_factor());
        let first = selected_rows[0];
        let last = *selected_rows.last().unwrap();
        let start = gpui_kit::point(
            px(first.left().0 / scale + 20.),
            px(first.center().y.0 / scale),
        );
        let end = gpui_kit::point(
            px(last.right().0 / scale - 2.),
            px(last.center().y.0 / scale),
        );
        for (anchor, cursor) in [(start, end), (end, start)] {
            cx.simulate_mouse_down(
                anchor,
                gpui_kit::MouseButton::Left,
                gpui_kit::Modifiers::default(),
            );
            draw(cx);
            cx.simulate_mouse_move(
                cursor,
                gpui_kit::MouseButton::Left,
                gpui_kit::Modifiers::default(),
            );
            draw(cx);
            cx.simulate_mouse_up(
                cursor,
                gpui_kit::MouseButton::Left,
                gpui_kit::Modifiers::default(),
            );
            draw(cx);
            draw(cx);
            draw(cx);
            rows(cx);
            reader.read_with(cx, |reader, cx| {
                let selection = reader.selection.as_ref().expect("Mouse release must show the math selection toolbar");
                assert!(selection.quote.contains("$d_k$"));
                assert!(selection.quote.contains(r"$\frac{1}{\sqrt{d_k}}$"));
                assert!(plain.contains(&selection.quote));
                let marked = easyread_gpui::annotations::decorate_interactive(&reader.reader_layout_html(0, false), &[
                    json!({"id":"math-selection-note","quote":selection.quote,"prefix":selection.prefix,"suffix":selection.suffix,"color":"yellow"}),
                ]);
                assert!(marked.contains("math-selection-note"));
                assert_eq!(easyread_gpui::annotations::plain_text(&marked), plain);
                assert_eq!(reader.raw, original);
                assert!(!reader.text_views[&(0, false)].1.read(cx).is_selecting());
            });
        }
    }
}

#[gpui_kit::test]
fn saved_math_annotations_fill_formula_cells_and_keep_underlines_and_clicks(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    for tex in ["d_k", r"\frac{1}{\sqrt{d_k}}"] {
        easyread_gpui::formula::layout(tex, false).unwrap();
    }
    let markdown = r"While for small values of $d_k$ the two mechanisms perform similarly, additive attention outperforms dot product attention without scaling for larger values of $d_k$ [3](https://example.org/3). For large values of $d_k$, the dot products grow in magnitude. To counteract this effect, we scale the dot products by $\frac{1}{\sqrt{d_k}}$.";
    let html = easyread_gpui::formula::native_markdown_html(markdown);
    let plain = easyread_gpui::annotations::plain_text(&html);
    let source = markdown
        .replace("$d_k$", "dk")
        .replace(r"$\frac{1}{\sqrt{d_k}}$", "1√dk");
    for formula_only in [false, true] {
        let quote = if formula_only {
            r"$\frac{1}{\sqrt{d_k}}$"
        } else {
            &plain
        };
        let fill = if formula_only {
            easyread_gpui::annotations::HighlightColor::Green
        } else {
            easyread_gpui::annotations::HighlightColor::Yellow
        };
        // Open with existing persisted annotations, not a live text selection.
        let raw = json!({"paper":{"meta":{"title_en":"Saved math annotation regression"},"blocks":[
            {"id":"paragraph","type":"para","page":1,"source_kind":"pdf-text","en":source}
        ]},"reader":{"notes":{
            "fill":{"id":"fill","anchor":"paragraph","key":"paragraph","kind":"highlight","quote":quote,"color":fill.name()},
            "line":{"id":"line","anchor":"paragraph","key":"paragraph","kind":"highlight","style":"underline","quote":quote,"color":"blue"}
        }}});
        let original = raw.clone();
        let mut reader = None;
        let (_root, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| {
                let mut reader = translated_reader(raw, None, None, window, cx);
                reader.pdf_code_html.insert(
                    0,
                    (
                        source.clone(),
                        Some(CodePresentation {
                            html: html.clone(),
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
        for (width, font) in [(25., 21.), (35., 21.), (25., 27.)] {
            reader.update(cx, |reader, cx| {
                reader.appearance.line_width = width;
                reader.appearance.font_size = font;
                reader.annotation_popup = None;
                reader.reset_rows();
                cx.notify();
            });
            draw(cx);
            cx.run_until_parked();
            draw(cx);
            cx.run_until_parked();
            draw(cx);
            let native = cx
                .debug_bounds("reader-native-typst-math")
                .expect("Fraction must remain native math");
            let (mut cells, underlines, scale) = cx.update(|window, _| {
                let yellow: gpui_kit::Background = gpui_kit::rgb(fill.rgb()).into();
                let blue: gpui_kit::Hsla =
                    gpui_kit::rgb(easyread_gpui::annotations::HighlightColor::Blue.line_rgb())
                        .into();
                (
                    window
                        .painted_quads()
                        .into_iter()
                        .filter(|q| q.background == yellow)
                        .map(|q| q.bounds)
                        .collect::<Vec<_>>(),
                    window
                        .painted_underlines()
                        .into_iter()
                        .filter(|line| line.color == blue)
                        .collect::<Vec<_>>(),
                    window.scale_factor(),
                )
            });
            let fraction = gpui_kit::Bounds::from_corners(
                gpui_kit::point(
                    gpui_kit::ScaledPixels(f32::from(native.left()) * scale),
                    gpui_kit::ScaledPixels(f32::from(native.top()) * scale),
                ),
                gpui_kit::point(
                    gpui_kit::ScaledPixels(f32::from(native.right()) * scale),
                    gpui_kit::ScaledPixels(f32::from(native.bottom()) * scale),
                ),
            );
            assert!(
                cells.iter().any(|cell| cell.left() <= fraction.left()
                    && cell.right() >= fraction.right()
                    && cell.top() <= fraction.top()
                    && cell.bottom() >= fraction.bottom()),
                "Saved highlight must cover the formula and its row leading: {fraction:?}, {cells:?}"
            );
            assert!(
                underlines
                    .iter()
                    .any(|line| line.bounds.left() <= fraction.left()
                        && line.bounds.right() >= fraction.right()
                        && line.bounds.top() >= fraction.bottom()),
                "Saved underline must span the formula without striking its denominator: {fraction:?}, {underlines:?}"
            );
            if formula_only {
                assert_eq!(
                    cells.len(),
                    1,
                    "Formula-only marks must not color surrounding prose"
                );
            } else {
                cells.sort_by(|a, b| {
                    a.top()
                        .partial_cmp(&b.top())
                        .unwrap()
                        .then_with(|| a.left().partial_cmp(&b.left()).unwrap())
                });
                let mut rows: Vec<gpui_kit::Bounds<gpui_kit::ScaledPixels>> = Vec::new();
                for cell in cells {
                    if let Some(row) = rows.last_mut().filter(|row| row.top() == cell.top()) {
                        assert_eq!(row.bottom(), cell.bottom());
                        assert!(
                            (row.right() - cell.left()).0.abs() < 0.01,
                            "No saved-highlight holes around subscripts: {row:?}, {cell:?}"
                        );
                        *row = gpui_kit::Bounds::from_corners(row.origin, cell.bottom_right());
                    } else {
                        rows.push(cell);
                    }
                }
                for pair in rows.windows(2) {
                    assert_eq!(pair[0].bottom(), pair[1].top());
                }
            }
            reader.read_with(cx, |reader, cx| {
                assert_eq!(
                    reader.raw, original,
                    "Rendering must not rewrite existing annotations"
                );
                let view = reader.text_views[&(0, false)].1.read(cx);
                assert!(
                    view.annotation_offset("line").is_some(),
                    "Formula-only annotations need margin-note geometry"
                );
                assert_eq!(
                    easyread_gpui::annotations::plain_text(&reader.reader_body_html(0, false)),
                    plain
                );
            });
            cx.simulate_click(native.center(), gpui_kit::Modifiers::default());
            reader.read_with(cx, |reader, _| {
                assert_eq!(
                    reader.annotation_popup.as_ref().map(|(id, _)| id.as_str()),
                    Some("line"),
                    "Clicking the saved formula annotation must open its popup"
                );
            });
        }
        // Decoration-only color changes/removal reuse the existing TextView;
        // no reopen, source replacement or formula recompilation is required.
        let view_before =
            reader.read_with(cx, |reader, _| reader.text_views[&(0, false)].1.clone());
        for deleted in [false, true] {
            let mut updated = original.clone();
            updated["reader"]["notes"]["fill"]["color"] = json!("pink");
            for id in ["fill", "line"] {
                updated["reader"]["notes"][id]["deleted"] = json!(deleted);
            }
            reader.update(cx, |reader, cx| {
                assert!(!reader.apply_state(updated.clone()));
                reader.annotation_popup = None;
                reader.reset_rows();
                cx.notify();
            });
            draw(cx);
            cx.run_until_parked();
            draw(cx);
            assert!(cx.debug_bounds("reader-native-typst-math").is_some());
            cx.update(|window, _| {
                let red: gpui_kit::Background =
                    gpui_kit::rgb(easyread_gpui::annotations::HighlightColor::Red.rgb()).into();
                assert_eq!(
                    window
                        .painted_quads()
                        .iter()
                        .any(|quad| quad.background == red),
                    !deleted
                );
            });
            reader.read_with(cx, |reader, _| {
                assert_eq!(reader.text_views[&(0, false)].1, view_before);
                assert_eq!(reader.raw, updated);
                assert_eq!(
                    easyread_gpui::annotations::plain_text(&reader.reader_body_html(0, false)),
                    plain
                );
            });
        }
    }
}

#[gpui_kit::test]
fn inline_typst_fraction_wraps_with_prose_scales_and_copies_tex(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let tex = r"\frac{1}{\sqrt{d_k}}";
    let layout = easyread_gpui::formula::layout(tex, false).unwrap();
    let source = format!(
        "Before the inline formula we have ordinary prose and **emphasis** ${tex}$ followed by more ordinary prose that can wrap naturally.\n\nAfter the formula the body remains selectable."
    );
    let mut data = fixture();
    data["paper"]["blocks"] = json!([{"id":"inline-native","type":"para","page":1,"en":source}]);
    let original = data.clone();
    let mut reader = None;
    let (_root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| translated_reader(data, None, None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    for (width, font) in [(25., 21.), (35., 21.), (25., 27.)] {
        reader.update(cx, |reader, cx| {
            reader.appearance.line_width = width;
            reader.appearance.font_size = font;
            reader.reset_rows();
            cx.notify();
        });
        draw(cx);
        cx.run_until_parked();
        draw(cx);
        cx.run_until_parked();
        draw(cx);
        let native = cx
            .debug_bounds("reader-native-typst-math")
            .expect("inline native math canvas");
        assert!((f32::from(native.size.width) - layout.width * font / 16.).abs() <= 0.51);
        assert!(
            (f32::from(native.size.height) - layout.height * font / 16.).abs() <= 0.51,
            "native={native:?}, layout={}x{}, baseline={}, width={width}, font={font}",
            layout.width,
            layout.height,
            layout.baseline
        );
        reader.update(cx, |reader, cx| {
            let (source, view) = reader.text_views[&(0, false)].clone();
            assert!(
                source.contains("data-vellio-math")
                    && !source.contains("<img")
                    && !source.contains("data:image")
            );
            view.update(cx, |state, cx| {
                let first = state.paragraph_bounds(0).unwrap();
                let after = state.paragraph_bounds(1).unwrap();
                assert!(native.top() >= first.top() && native.bottom() <= first.bottom());
                assert!(
                    first.bottom() <= after.top(),
                    "fraction cannot overlap following prose"
                );
                state.select_all(cx);
                let copied = state.selected_text();
                assert!(
                    copied.contains(tex),
                    "native object copies TeX, not image alt text: {copied}"
                );
                assert!(copied.contains("After the formula"));
            });
            reader.capture_selection(gpui_kit::point(px(400.), px(300.)), cx);
            assert!(
                reader
                    .selection
                    .as_ref()
                    .is_some_and(|s| s.quote.contains(tex)),
                "Markdown formulas must also open the selection toolbar"
            );
            view.update(cx, |state, cx| state.clear_selection(cx));
            assert_eq!(reader.raw, original);
            assert!(reader.image_preview.is_none());
        });
    }
}

#[gpui_kit::test]
fn pdf_label_smoothing_equation_and_results_title_paint_separately(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("numbers.pdf");
    math_fixtures::numbers_fixture(&source);
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
    let equation = p.math[0].clone();
    assert_eq!(equation.tex.replace(' ', ""), r"\epsilon_{ls}=0.1");
    // The shared debug selector reports the last painted math object.
    let last_layout = p.math.last().unwrap().layout.clone();
    let mut reader = None;
    let (_root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut reader = translated_reader(raw, None, None, window, cx);
            reader.pdf_code_html.insert(0, (text, Some(p)));
            reader.appearance.font_size = 21.;
            reader.appearance.line_width = 35.;
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
    let native = cx
        .debug_bounds("reader-native-typst-math")
        .expect("PDF inline formulas must paint native paths");
    assert!((f32::from(native.size.width) - last_layout.width * 21. / 16.).abs() <= 0.51);
    reader.update(cx, |reader, cx| {
        let (source, view) = reader.text_views[&(0, false)].clone();
        assert!(source.contains("<h3>6 Results</h3>"));
        assert!(!source.contains("<img"));
        view.update(cx, |state, cx| {
            let body = state.paragraph_bounds(0).unwrap();
            let heading = state
                .paragraph_bounds(1)
                .expect("Results is a separate heading");
            let subheading = state.paragraph_bounds(2).unwrap();
            let following = state.paragraph_bounds(3).unwrap();
            assert!(native.top() >= following.top());
            assert!(body.bottom() <= heading.top());
            assert!(heading.bottom() <= subheading.top());
            assert!(subheading.bottom() <= following.top());
            state.select_all(cx);
            let copied = state.selected_text();
            assert!(copied.contains(&equation.tex), "{copied}");
            assert!(copied.contains("6 Results") && copied.contains("6.1 Machine Translation"));
            state.clear_selection(cx);
        });
        assert_eq!(reader.raw, original);
        assert!(reader.image_preview.is_none());
    });
}

#[gpui_kit::test]
fn inline_decimal_with_baseline_at_its_bottom_never_falls_back_to_tex(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let tex = ".1";
    let layout = easyread_gpui::formula::layout(tex, false).unwrap();
    assert_eq!(layout.baseline, layout.height);
    let mut data = fixture();
    data["paper"]["blocks"] = json!([{"id":"decimal-native","type":"para","page":1,"en":"Before **$.1$** after.\n\nFollowing prose."}]);
    let original = data.clone();
    let mut reader = None;
    let (_root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| translated_reader(data, None, None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    for font in [21., 22., 27.] {
        reader.update(cx, |reader, cx| {
            reader.appearance.font_size = font;
            reader.reset_rows();
            cx.notify();
        });
        for _ in 0..3 {
            draw(cx);
            cx.run_until_parked();
        }
        let native = cx
            .debug_bounds("reader-native-typst-math")
            .expect("decimal must paint native paths even after height rounding");
        assert!((f32::from(native.size.height) - layout.height * font / 16.).abs() <= 0.51);
        reader.update(cx, |reader, cx| {
            let (source, view) = reader.text_views[&(0, false)].clone();
            assert!(source.contains("data-vellio-math") && !source.contains("<img"));
            view.update(cx, |state, cx| {
                let first = state.paragraph_bounds(0).unwrap();
                let after = state.paragraph_bounds(1).unwrap();
                assert!(native.top() >= first.top() && native.bottom() <= first.bottom());
                assert!(first.bottom() <= after.top());
                state.select_all(cx);
                assert!(state.selected_text().contains(tex));
                state.clear_selection(cx);
            });
            assert_eq!(reader.raw, original);
            assert!(reader.image_preview.is_none());
        });
    }
}
