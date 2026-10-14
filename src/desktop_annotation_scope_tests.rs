//! Scope regressions: actual paragraph clicks, durable notes and painted marks.
use super::{ReaderView, ReadingMode};
use easyread_gpui::{
    annotations::HighlightColor,
    store::{Workspace, write_json},
    translation_layout,
};
use gpui_kit::{AppContext, Modifiers, TestAppContext, VisualTestContext, component::Root};
use serde_json::json;

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..32 {
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.draw(cx).clear(cx);
        });
    }
}

#[gpui_kit::test]
fn paragraph_annotations_save_and_paint_only_the_clicked_scope_across_formats_and_modes(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    for variant in [
        "plain",
        "saved_pdf",
        "legacy_pdf",
        "edited_pdf",
        "epub",
        "complex_pdf",
        "caption",
    ] {
        for (mode, primary, companion) in [
            (ReadingMode::Original, true, false),
            (ReadingMode::Translation, false, false),
            (ReadingMode::Bilingual, true, false),
            (ReadingMode::Bilingual, false, false),
            (ReadingMode::Bilingual, true, true),
            (ReadingMode::Bilingual, false, true),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let workspace = Workspace {
                root: temp.path().to_owned(),
            };
            let source = if variant == "complex_pdf" {
                "<p>First original.</p><ul><li>Second original.</li></ul><p>Last original.</p>"
            } else {
                "<p>First original.</p><p>Second original.</p><p>Last original.</p>"
            };
            let en = "First original.\n\nSecond original.\n\nLast original.";
            let plan = translation_layout::Plan::from_html(source).unwrap();
            let translated = plan
                .translate(
                    &plan
                        .input()
                        .replace("First original.", "第一段译文。")
                        .replace("Second original.", "第二段译文。")
                        .replace("Last original.", "末段译文。"),
                )
                .unwrap();
            let mut block = json!({"id":"page", "type":"para", "en":en, "zh":translated.markdown});
            if variant.contains("pdf") {
                block["source_kind"] = json!("pdf-text");
                block["page"] = json!(1);
                if variant != "legacy_pdf" {
                    block["native_translation_layout"] =
                        translation_layout::metadata(en, &translated);
                }
            } else if variant == "epub" {
                block["source_kind"] = json!("epub-xhtml");
                block["html"] = json!(source);
                block["chapter"] = json!(1);
            } else if variant == "caption" {
                block["type"] = json!("figure");
                block["caption_en"] = json!(en);
                block["caption_zh"] = json!(translated.markdown);
                block["en"] = json!("");
                block["zh"] = json!("");
            }
            let mut raw = json!({"paper":{"meta":{"title_en":"Paragraph scope regression", "format":if variant == "epub" {"epub"} else {"pdf"}},"blocks":[block]},"reader":{}});
            if variant == "edited_pdf" {
                raw["reader"]["edits"]["page"] =
                    json!({"zh":"第一段修订。\n\n第二段修订。\n\n末段修订。"});
            }
            write_json(&workspace.root.join("paper.json"), &raw["paper"]).unwrap();
            write_json(&workspace.root.join("reader.json"), &raw["reader"]).unwrap();
            let paper_before = std::fs::read(workspace.root.join("paper.json")).unwrap();
            let mut reader = None;
            let (_, cx) = cx.add_window_view(|window, cx| {
                let view = cx.new(|cx| {
                    let mut r = ReaderView::new(raw, None, Some(workspace.clone()), window, cx);
                    r.original_first = primary;
                    r.set_reading_mode(mode, cx);
                    if variant.contains("pdf") {
                        r.pdf_code_html.insert(
                            0,
                            (
                                en.into(),
                                Some(easyread_gpui::pdf::CodePresentation {
                                    html: source.into(),
                                    inline_figures: vec![],
                                    figures: vec![],
                                    math: vec![],
                                }),
                            ),
                        );
                    }
                    r
                });
                reader = Some(view.clone());
                Root::new(view, window, cx)
            });
            let reader = reader.unwrap();
            settle(cx);
            let paired = reader.read_with(cx, |r, _| r.paragraph_languages.contains_key(&0));
            let language = if companion && !paired {
                !primary
            } else {
                primary
            };
            let ordinal = if mode == ReadingMode::Bilingual && paired {
                if companion { 3 } else { 2 }
            } else {
                1
            };
            let selected_original = if companion { !primary } else { primary };
            let expected = if selected_original {
                "Second original."
            } else if variant == "edited_pdf" {
                "第二段修订。"
            } else {
                "第二段译文。"
            };
            let bounds = reader.read_with(cx, |r, cx| {
                r.text_views[&(0, language)]
                    .1
                    .read(cx)
                    .paragraph_bounds(ordinal)
                    .unwrap()
            });
            cx.simulate_click(bounds.center(), Modifiers::default());
            settle(cx);
            reader.read_with(cx, |r, _| {
                assert_eq!(
                    r.selection.as_ref().unwrap().quote,
                    expected,
                    "{variant} {mode:?} companion={companion}"
                )
            });
            // Same callback invoked by the toolbar's color controls; save via
            // real Workspace ops and the background refresh, not a mock mark.
            let underline = companion;
            cx.update(|window, cx| {
                reader.update(cx, |r, cx| {
                    r.selection_action(
                        if underline { "underline" } else { "highlight" },
                        HighlightColor::Yellow,
                        window,
                        cx,
                    )
                })
            });
            settle(cx);
            for _ in 0..100 {
                if !reader.read_with(cx, |r, _| r.busy) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
                cx.executor()
                    .advance_clock(std::time::Duration::from_millis(50));
                settle(cx);
            }
            assert!(
                !reader.read_with(cx, |r, _| r.busy),
                "Annotation write must finish"
            );
            let notes = workspace.load("reader").unwrap();
            let note = notes["notes"].as_object().unwrap().values().next().unwrap();
            assert_eq!(note["quote"], expected);
            assert_eq!(note["anchor"], "page");
            assert_eq!(
                note["key"],
                if variant == "caption" {
                    "page#caption"
                } else {
                    "page"
                }
            );
            assert_eq!(note["style"] == "underline", underline);
            assert_eq!(
                std::fs::read(workspace.root.join("paper.json")).unwrap(),
                paper_before
            );
            let bounds = reader.read_with(cx, |r, cx| {
                r.text_views[&(0, language)]
                    .1
                    .read(cx)
                    .paragraph_bounds(ordinal)
                    .unwrap()
            });
            cx.update(|window, _| {
                let color: gpui_kit::Hsla = gpui_kit::rgb(if underline {
                    HighlightColor::Yellow.line_rgb()
                } else {
                    HighlightColor::Yellow.rgb()
                })
                .into();
                let mut marks: Vec<_> = window
                    .painted_quads()
                    .into_iter()
                    .filter(|q| {
                        q.background == gpui_kit::Background::from(color)
                            && q.bounds.size.width.0 > 0.
                    })
                    .map(|q| q.bounds)
                    .collect();
                if underline {
                    marks.extend(
                        window
                            .painted_underlines()
                            .into_iter()
                            .filter(|line| line.color == color)
                            .map(|line| line.bounds),
                    );
                }
                assert!(
                    !marks.is_empty(),
                    "Saved mark must actually paint: {variant} {mode:?} companion={companion}"
                );
                let scale = window.scale_factor();
                assert!(
                    marks
                        .iter()
                        .all(|q| q.top().0 / scale >= f32::from(bounds.top()) - 1.
                            && q.bottom().0 / scale <= f32::from(bounds.bottom()) + 1.),
                    "Mark escaped its paragraph: {variant} {mode:?} companion={companion}"
                );
            });
        }
    }
}

#[gpui_kit::test]
fn real_text_drag_and_local_companion_click_do_not_capture_the_page(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let raw = json!({"paper":{"meta":{"title_en":"Drag and local scope"},"blocks":[{"id":"page","type":"para","en":"First original.\n\nSecond original.\n\nLast original.","zh":"第一段译文。\n\n第二段译文。\n\n末段译文。"}]},"reader":{}});
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(raw, None, None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    for (mode, primary) in [
        (ReadingMode::Original, true),
        (ReadingMode::Translation, false),
    ] {
        reader.update(cx, |r, cx| r.set_reading_mode(mode, cx));
        settle(cx);
        let bounds = reader.read_with(cx, |r, cx| {
            r.text_views[&(0, primary)]
                .1
                .read(cx)
                .paragraph_bounds(1)
                .unwrap()
        });
        cx.simulate_click(bounds.center(), Modifiers::default());
        settle(cx);
        reader.update(cx, |r, cx| r.toggle_block_language(0, cx));
        settle(cx);
        let companion = reader.read_with(cx, |r, cx| {
            r.text_views[&(0, primary)]
                .1
                .read(cx)
                .paragraph_bounds(2)
                .unwrap()
        });
        cx.simulate_click(companion.center(), Modifiers::default());
        settle(cx);
        reader.read_with(cx, |r, _| {
            assert_eq!(
                r.selection.as_ref().unwrap().quote,
                if primary {
                    "第二段译文。"
                } else {
                    "Second original."
                }
            )
        });
    }
    reader.update(cx, |r, cx| {
        r.original_first = true;
        r.set_reading_mode(ReadingMode::Bilingual, cx);
    });
    settle(cx);
    let bounds = reader.read_with(cx, |r, cx| {
        r.text_views[&(0, true)]
            .1
            .read(cx)
            .paragraph_bounds(2)
            .unwrap()
    });
    let start = gpui_kit::point(bounds.left() + gpui_kit::px(12.), bounds.center().y);
    let end = gpui_kit::point(bounds.left() + gpui_kit::px(92.), bounds.center().y);
    cx.simulate_mouse_down(start, gpui_kit::MouseButton::Left, Modifiers::default());
    settle(cx);
    cx.simulate_mouse_move(end, gpui_kit::MouseButton::Left, Modifiers::default());
    settle(cx);
    cx.simulate_mouse_up(end, gpui_kit::MouseButton::Left, Modifiers::default());
    settle(cx);
    reader.read_with(cx, |r, cx| {
        let selected = r.text_views[&(0, true)]
            .1
            .read(cx)
            .selected_text()
            .trim_end_matches('\n')
            .to_owned();
        assert!(!selected.is_empty() && selected.len() < "Second original.".len());
        assert!("Second original.".contains(&selected));
        assert_eq!(
            r.selection.as_ref().unwrap().quote,
            selected,
            "A drag must retain its exact text, not grow to a paragraph or page"
        );
    });
}

#[gpui_kit::test]
fn repeated_paragraph_click_anchors_the_second_occurrence_and_copy_is_scoped(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let raw = json!({"paper":{"meta":{"title_en":"Repeated paragraphs"},"blocks":[{"id":"page","type":"para","en":"Lead.\n\nRepeated paragraph.\n\nRepeated paragraph.\n\nTail."}]},"reader":{}});
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(raw, None, None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    settle(cx);
    let bounds = reader.read_with(cx, |r, cx| {
        r.text_views[&(0, true)]
            .1
            .read(cx)
            .paragraph_bounds(2)
            .unwrap()
    });
    cx.simulate_click(bounds.center(), Modifiers::default());
    settle(cx);
    reader.read_with(cx, |r, _| {
        let selection = r.selection.as_ref().unwrap();
        assert_eq!(selection.quote, "Repeated paragraph.");
        let text = easyread_gpui::annotations::plain_text(&r.reader_layout_html(0, true));
        let range = easyread_gpui::annotations::quote_range(
            &text,
            &selection.quote,
            &selection.prefix,
            &selection.suffix,
        )
        .unwrap();
        assert_eq!(
            range.start,
            text.match_indices("Repeated paragraph.").nth(1).unwrap().0
        );
    });
    cx.update(|window, cx| {
        reader.update(cx, |r, cx| {
            r.selection_action("copy", HighlightColor::Yellow, window, cx)
        })
    });
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "Repeated paragraph."
        )
    });
}
