use super::{ReaderView, ReadingMode};
use gpui_kit::{AppContext, Modifiers, TestAppContext, VisualTestContext, component::Root, px};
use serde_json::{Value, json};

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..24 {
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.draw(cx).clear(cx);
        });
    }
}

fn fixture(epub: bool) -> Value {
    json!({"paper":{"meta":{"title_en":"Language modes", "format":if epub {"epub"} else {"pdf"}},"blocks":[
        {"id":"first","type":"para","en":"An original paragraph to read.","zh":"用于阅读的译文段落。"},
        {"id":"second","type":"para","en":"A second original paragraph.","zh":"第二个译文段落。"},
        {"id":"untranslated","type":"para","en":"Not translated yet."}
    ]},"reader":{}})
}

#[gpui_kit::test]
fn language_companions_have_no_left_border_or_indent_in_all_reading_modes(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut reader = ReaderView::new(fixture(false), None, None, window, cx);
            for mode in [
                ReadingMode::Original,
                ReadingMode::Translation,
                ReadingMode::Bilingual,
            ] {
                reader.reading_mode = mode;
                for original_first in [true, false] {
                    reader.original_first = original_first;
                    let original = reader.primary_original();
                    let style = super::typography::reader_rich_style(
                        &reader,
                        0,
                        original,
                        reader
                            .fonts
                            .reading(reader.appearance.reading_font(), original),
                        21.,
                        cx,
                    );
                    assert_eq!(
                        style.companion().border_widths.left,
                        Some(px(0.).into()),
                        "En / 译文 companions must not inherit the ordinary blockquote border"
                    );
                    assert_eq!(style.companion().padding.left, Some(px(0.).into()));
                    assert_eq!(style.companion().padding.right, Some(px(0.).into()));
                }
            }
            reader
        });
        Root::new(view, window, cx)
    });
    settle(cx);
}

#[gpui_kit::test]
fn bilingual_paragraph_click_captures_only_the_clicked_translation(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let source = "<p>First original.</p><p>Second original.</p><p>Last original.</p>";
    let plan = easyread_gpui::translation_layout::Plan::from_html(source).unwrap();
    let translated = plan
        .translate(
            &plan
                .input()
                .replace("First original.", "第一段译文。")
                .replace("Second original.", "第二段译文。")
                .replace("Last original.", "末段译文。"),
        )
        .unwrap();
    let en = "First original.\n\nSecond original.\n\nLast original.";
    let raw = json!({"paper":{"meta":{"title_en":"Paragraph highlight scope"},"blocks":[
        {"id":"page","type":"para","source_kind":"pdf-text","page":1,"en":en,"zh":translated.markdown,"native_translation_layout":easyread_gpui::translation_layout::metadata(en, &translated)}
    ]},"reader":{}});
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut r = ReaderView::new(raw, None, None, window, cx);
            r.reading_mode = ReadingMode::Bilingual;
            r.original_first = true;
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
            r
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    settle(cx);
    let paragraph = reader.read_with(cx, |r, cx| {
        r.text_views[&(0, true)]
            .1
            .read(cx)
            .paragraph_bounds(3)
            .unwrap()
    });
    cx.simulate_click(paragraph.center(), Modifiers::default());
    settle(cx);
    reader.read_with(cx, |r, _| {
        assert_eq!(
            r.selection.as_ref().unwrap().quote,
            "第二段译文。",
            "A page block is not the paragraph the user clicked"
        )
    });
}

#[gpui_kit::test]
fn reading_mode_toolbar_switches_language_and_bilingual_order(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(fixture(false), None, None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    settle(cx);
    reader.read_with(cx, |r, _| {
        assert_eq!(r.reading_mode, ReadingMode::Original);
        assert_eq!(r.visible_languages(0), vec![true]);
        assert!(r.text_views.contains_key(&(0, true)));
        assert!(!r.text_views.contains_key(&(0, false)));
    });
    let translation = cx.debug_bounds("reading-mode-translation").unwrap();
    cx.simulate_click(translation.center(), Modifiers::default());
    settle(cx);
    reader.read_with(cx, |r, _| {
        assert_eq!(r.visible_languages(0), vec![false]);
        assert_eq!(r.reading_mode, ReadingMode::Translation);
    });
    let bilingual = cx.debug_bounds("reading-mode-bilingual").unwrap();
    cx.simulate_click(bilingual.center(), Modifiers::default());
    settle(cx);
    reader.read_with(cx, |r, _| {
        assert_eq!(r.visible_languages(0), vec![true, false]);
        assert_eq!(r.visible_languages(2), vec![true]);
        assert_eq!(r.rendered_languages(0), vec![true]);
        let html = &r.text_views[&(0, true)].0;
        assert!(html.find("An original paragraph") < html.find("用于阅读的译文"));
    });
    let toggle = reader.read_with(cx, |r, _| r.reading_order_button_bounds.unwrap());
    cx.simulate_click(toggle.center(), Modifiers::default());
    settle(cx);
    assert!(cx.debug_bounds("reading-order-menu").is_some());
    let option = cx.debug_bounds("reading-order-translation-first").unwrap();
    cx.simulate_click(option.center(), Modifiers::default());
    settle(cx);
    reader.read_with(cx, |r, _| {
        assert_eq!(r.reading_mode, ReadingMode::Bilingual);
        assert_eq!(r.visible_languages(0), vec![false, true]);
        assert_eq!(r.visible_languages(2), vec![false]);
        assert!(!r.reading_order_open);
        assert_eq!(r.rendered_languages(0), vec![false]);
        let html = &r.text_views[&(0, false)].0;
        assert!(html.find("用于阅读的译文") < html.find("An original paragraph"));
    });
    cx.simulate_click(toggle.center(), Modifiers::default());
    settle(cx);
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert!(cx.debug_bounds("reading-order-menu").is_none());
}

#[gpui_kit::test]
fn reading_mode_original_paragraph_click_expands_and_collapses_only_its_translation(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(fixture(false), None, None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    settle(cx);
    let source = reader.read_with(cx, |r, cx| {
        r.text_views[&(0, true)]
            .1
            .read(cx)
            .paragraph_bounds(0)
            .unwrap()
    });
    cx.simulate_click(source.center(), Modifiers::default());
    settle(cx);
    assert!(cx.debug_bounds("selection-toolbar").is_some());
    reader.read_with(cx, |r, _| {
        assert_eq!(r.selection.as_ref().unwrap().block, 0)
    });
    let toggle = cx.debug_bounds("selection-language-toggle").unwrap();
    cx.simulate_click(toggle.center(), Modifiers::default());
    settle(cx);
    reader.read_with(cx, |r, cx| {
        assert_eq!(r.reading_mode, ReadingMode::Original);
        assert_eq!(r.visible_languages(0), vec![true, false]);
        assert_eq!(r.visible_languages(1), vec![true]);
        assert!(
            r.text_views[&(0, true)].1.read(cx).bounds().top()
                < r.text_views[&(0, false)].1.read(cx).bounds().top()
        );
        let primary = r.text_views[&(0, true)]
            .1
            .read(cx)
            .paragraph_bounds(0)
            .unwrap();
        let secondary = r.text_views[&(0, false)]
            .1
            .read(cx)
            .paragraph_bounds(0)
            .unwrap();
        assert!(
            (primary.left() - secondary.left()).abs() < px(1.),
            "Expanded translation must align with its source, not retain the old border padding"
        );
    });
    cx.simulate_click(source.center(), Modifiers::default());
    settle(cx);
    let toggle = cx.debug_bounds("selection-language-toggle").unwrap();
    cx.simulate_click(toggle.center(), Modifiers::default());
    settle(cx);
    reader.read_with(cx, |r, _| {
        assert_eq!(r.visible_languages(0), vec![true]);
        assert_eq!(r.visible_languages(1), vec![true]);
    });
    reader.update(cx, |r, cx| r.set_reading_mode(ReadingMode::Translation, cx));
    settle(cx);
    reader.update(cx, |r, cx| r.toggle_block_language(0, cx));
    settle(cx);
    reader.read_with(cx, |r, _| {
        assert_eq!(r.visible_languages(0), vec![false, true])
    });
    reader.update(cx, |r, cx| r.set_reading_mode(ReadingMode::Original, cx));
    reader.read_with(cx, |r, _| assert!(r.original_blocks.is_empty()));
}

#[gpui_kit::test]
fn translation_paragraph_toggle_renders_original_text_not_a_second_translation(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    for variant in ["plain", "epub", "legacy_pdf", "saved_pdf", "edited"] {
        let mut raw = fixture(variant == "epub");
        if variant == "edited" {
            raw["reader"]["edits"]["first"] = json!({"zh":"用户修订的用于阅读的段落。"});
        }
        let block = &mut raw["paper"]["blocks"][0];
        let source_html = "<p>An <em>original</em> paragraph to read.</p>";
        if variant == "epub" {
            block["source_kind"] = json!("epub-xhtml");
            block["html"] = json!(source_html);
        } else if variant.ends_with("pdf") {
            block["source_kind"] = json!("pdf-text");
            block["page"] = json!(1);
        }
        let plan = easyread_gpui::translation_layout::Plan::from_html(source_html).unwrap();
        let translated = plan
            .translate(
                &plan
                    .input()
                    .replace("An ", "一个 ")
                    .replace("original", "原文")
                    .replace(" paragraph to read.", " 用于阅读的段落。"),
            )
            .unwrap();
        block["zh"] = json!(translated.markdown);
        if variant == "saved_pdf" {
            block["native_translation_layout"] = easyread_gpui::translation_layout::metadata(
                block["en"].as_str().unwrap(),
                &translated,
            );
        }
        let mut reader = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| {
                let mut r = ReaderView::new(raw, None, None, window, cx);
                if variant.ends_with("pdf") {
                    let en = r.state.paper.blocks[0].en.clone();
                    r.pdf_code_html.insert(
                        0,
                        (
                            en,
                            Some(easyread_gpui::pdf::CodePresentation {
                                html: source_html.into(),
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
        let translation = cx.debug_bounds("reading-mode-translation").unwrap();
        cx.simulate_click(translation.center(), Modifiers::default());
        settle(cx);
        let paragraph = reader.read_with(cx, |r, cx| {
            r.text_views[&(0, false)]
                .1
                .read(cx)
                .paragraph_bounds(0)
                .unwrap()
        });
        cx.simulate_click(paragraph.center(), Modifiers::default());
        settle(cx);
        let toggle = cx.debug_bounds("selection-language-toggle").unwrap();
        cx.simulate_click(toggle.center(), Modifiers::default());
        settle(cx);
        reader.update(cx, |r, cx| {
            assert_eq!(r.visible_languages(0), vec![false, true]);
            let translated = r.text_views[&(0, false)].1.clone();
            let original = r.text_views[&(0, true)].1.clone();
            translated.update(cx, |v, cx| {
                v.select_all(cx);
                assert!(v.selected_text().contains("用于阅读的段落"));
                assert!(!v.selected_text().contains("An original"));
                v.clear_selection(cx);
            });
            original.update(cx, |v, cx| {
                v.select_all(cx);
                assert_eq!(v.selected_text().trim(), "An original paragraph to read.");
                v.clear_selection(cx);
            });
        });
        cx.simulate_click(paragraph.center(), Modifiers::default());
        settle(cx);
        let toggle = cx.debug_bounds("selection-language-toggle").unwrap();
        cx.simulate_click(toggle.center(), Modifiers::default());
        settle(cx);
        reader.read_with(cx, |r, _| assert_eq!(r.visible_languages(0), vec![false]));
    }
}

#[gpui_kit::test]
fn translated_pdf_click_inserts_only_its_source_before_next_translation(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let en_encoder =
        "Encoder: Each layer contains attention and a feed-forward network. ".repeat(5);
    let en_decoder =
        "Decoder: Each layer uses masked attention to the previous outputs. ".repeat(5);
    let zh_encoder = "编码器：每层包含自注意力机制和逐位置前馈网络。".repeat(5);
    let zh_decoder = "解码器：每层使用掩码注意力，只依赖前面位置上的输出。".repeat(5);
    let raw = json!({"paper":{"meta":{"title_en":"Multi-paragraph PDF expansion"},"blocks":[
        {"id":"page","type":"para","source_kind":"pdf-text","page":3,
         "en":format!("3.1 Encoder and Decoder Stacks\n{en_encoder}\n{en_decoder}"),
         "zh":format!("### 3.1 编码器和解码器堆栈\n\n{zh_encoder}\n\n{zh_decoder}")}
    ]},"reader":{}});
    let original_html = format!(
        "<p><img src=\"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/l9sAAAAASUVORK5CYII=\" width=\"300\" height=\"400\"></p><h3>3.1 Encoder and Decoder Stacks</h3><p>{en_encoder}</p><p>{en_decoder}</p>"
    );
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut r = ReaderView::new(raw, None, None, window, cx);
            r.reading_mode = ReadingMode::Translation;
            let en = r.state.paper.blocks[0].en.clone();
            r.pdf_code_html.insert(
                0,
                (
                    en,
                    Some(easyread_gpui::pdf::CodePresentation {
                        html: original_html,
                        inline_figures: vec![],
                        figures: vec![],
                        math: vec![],
                    }),
                ),
            );
            r
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    settle(cx);
    let encoder = reader.read_with(cx, |r, cx| {
        r.text_views[&(0, false)]
            .1
            .read(cx)
            .paragraph_bounds(1)
            .unwrap()
    });
    cx.simulate_click(
        gpui_kit::point(encoder.left() + px(8.), encoder.top() + px(8.)),
        Modifiers::default(),
    );
    settle(cx);
    let toggle = cx.debug_bounds("selection-language-toggle").unwrap();
    cx.simulate_click(toggle.center(), Modifiers::default());
    settle(cx);
    reader.read_with(cx, |r, cx| {
        let html = &r.text_views[&(0, false)].0;
        assert!(html.contains("Encoder: Each layer"), "The clicked encoder's original must appear directly in the translated page, not after the entire page");
        assert!(html.find("Encoder: Each layer") < html.find("解码器："));
        assert!(!html.contains("Decoder: Each layer"), "Only the clicked paragraph should expand");
        let translation = r.text_views[&(0, false)].1.read(cx);
        let mut index = 0;
        let mut previous_bottom = translation.bounds().top();
        while let Some(paragraph) = translation.paragraph_bounds(index) {
            assert!(paragraph.bottom() <= translation.bounds().bottom() + px(1.), "paragraph {index} overflows: {paragraph:?}, view={:?}", translation.bounds());
            assert!(paragraph.top() >= previous_bottom - px(1.), "paragraphs overlap at {index}");
            previous_bottom = paragraph.bottom();
            index += 1;
        }
        assert!(index >= 4);
    });
    // Clicking the same primary paragraph must remove just its companion.
    cx.simulate_click(
        gpui_kit::point(encoder.left() + px(8.), encoder.top() + px(8.)),
        Modifiers::default(),
    );
    settle(cx);
    let toggle = cx.debug_bounds("selection-language-toggle").unwrap();
    cx.simulate_click(toggle.center(), Modifiers::default());
    settle(cx);
    reader.read_with(cx, |r, cx| {
        let (html, view) = &r.text_views[&(0, false)];
        assert!(!html.contains("Encoder: Each layer"));
        assert!(html.contains("解码器："));
        assert!(
            view.read(cx).paragraph_bounds(3).is_none(),
            "Collapsed companion geometry must not survive as a ghost paragraph"
        );
    });
    // The inverse mode uses the same logical pairing and keeps the source
    // image once, while inserting only the clicked encoder's translation.
    reader.update(cx, |r, cx| r.set_reading_mode(ReadingMode::Original, cx));
    settle(cx);
    reader.update(cx, |r, cx| {
        let encoder = r.text_views[&(0, true)]
            .1
            .read(cx)
            .paragraph_bounds(2)
            .unwrap();
        let offset = (encoder.top() - r.list.viewport_bounds().top() - px(120.)).max(px(0.));
        r.list.scroll_to(gpui_kit::ListOffset {
            item_ix: 0,
            offset_in_item: offset,
        });
        cx.notify();
    });
    settle(cx);
    let source_encoder = reader.read_with(cx, |r, cx| {
        r.text_views[&(0, true)]
            .1
            .read(cx)
            .paragraph_bounds(2)
            .unwrap()
    });
    cx.simulate_click(
        gpui_kit::point(
            source_encoder.left() + px(8.),
            source_encoder.top() + px(8.),
        ),
        Modifiers::default(),
    );
    settle(cx);
    let toggle = cx.debug_bounds("selection-language-toggle").unwrap();
    cx.simulate_click(toggle.center(), Modifiers::default());
    settle(cx);
    reader.read_with(cx, |r, _| {
        let html = &r.text_views[&(0, true)].0;
        assert!(html.find("Encoder: Each layer") < html.find("编码器："));
        assert!(html.find("编码器：") < html.find("Decoder: Each layer"));
        assert!(!html.contains("解码器："));
        assert_eq!(html.matches("<img ").count(), 1);
    });
}

#[gpui_kit::test]
fn bilingual_pdf_pairs_each_paragraph_in_both_orders_without_duplicate_images(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let source_html = "<p><a href=\"easyread-image:figure\"><img src=\"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/l9sAAAAASUVORK5CYII=\" width=\"120\" height=\"30\"></a></p><p style=\"text-align:center\">Figure 1: Diagram.</p><h3>3.1 Stacks</h3><p><strong>Encoder:</strong> Original encoder.</p><p>Encoder continuation.</p><p><strong>Decoder:</strong> Original decoder.</p><p>3</p>";
    let raw = json!({"paper":{"meta":{"title_en":"Bilingual paragraph regression"},"blocks":[
        {"id":"page","type":"para","source_kind":"pdf-text","page":3,
         "en":"Figure 1: Diagram.\n3.1 Stacks\nEncoder: Original encoder.\nEncoder continuation.\nDecoder: Original decoder.\n3",
         "zh":"图 1：示意图。\n3.1 堆栈\n编码器译文。\n解码器译文。\n3"},
        {"id":"figure","type":"figure","source_kind":"pdf-image","page":3,"caption_en":""}
    ]},"reader":{}});
    let paper = raw["paper"].clone();
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut r = ReaderView::new(raw, None, None, window, cx);
            let source = r.state.paper.blocks[0].en.clone();
            r.pdf_code_html.insert(
                0,
                (
                    source,
                    Some(easyread_gpui::pdf::CodePresentation {
                        html: source_html.into(),
                        inline_figures: vec!["figure".into()],
                        figures: vec![easyread_gpui::pdf::FigurePreview {
                            id: "figure".into(),
                            png: vec![],
                            width: 120,
                            height: 30,
                        }],
                        math: vec![],
                    }),
                ),
            );
            r
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    for original_first in [true, false] {
        reader.update(cx, |r, cx| {
            r.original_first = original_first;
            r.set_reading_mode(ReadingMode::Bilingual, cx);
        });
        settle(cx);
        reader.read_with(cx, |r, cx| {
            assert_eq!(
                r.rendered_languages(0),
                vec![original_first],
                "Bilingual paragraphs must share one layout, not append a second page"
            );
            let (html, view) = &r.text_views[&(0, original_first)];
            let english_encoder = html.find("Original encoder.").unwrap();
            let chinese_encoder = html.find("编码器译文。").unwrap();
            let english_decoder = html.find("Original decoder.").unwrap();
            let chinese_decoder = html.find("解码器译文。").unwrap();
            if original_first {
                assert!(
                    english_encoder < chinese_encoder
                        && chinese_encoder < english_decoder
                        && english_decoder < chinese_decoder
                );
            } else {
                assert!(
                    chinese_encoder < english_encoder
                        && english_encoder < chinese_decoder
                        && chinese_decoder < english_decoder
                );
            }
            assert_eq!(html.matches("<img ").count(), 1);
            assert!(r.reader_suppresses_pdf_figure(1));
            let v = view.read(cx);
            assert!((v.paragraph_bounds(5).unwrap().left() - v.paragraph_bounds(6).unwrap().left()).abs() < px(1.),
                "Paired primary and companion paragraphs must share the same left edge in both bilingual orders");
            let mut last = v.bounds().top();
            for index in 0..10 {
                let paragraph = v.paragraph_bounds(index).unwrap();
                assert!(
                    paragraph.top() >= last - px(1.),
                    "Bilingual paragraphs overlap at {index}"
                );
                assert!(paragraph.bottom() <= v.bounds().bottom() + px(1.));
                last = paragraph.bottom();
            }
            assert_eq!(r.raw["paper"], paper);
        });
        // Collapse/reopen only Encoder; Decoder must stay bilingual.
        let encoder = reader.read_with(cx, |r, cx| {
            r.text_views[&(0, original_first)]
                .1
                .read(cx)
                .paragraph_bounds(5)
                .unwrap()
        });
        for expanded in [false, true] {
            cx.simulate_click(
                gpui_kit::point(encoder.left() + px(8.), encoder.top() + px(8.)),
                Modifiers::default(),
            );
            settle(cx);
            let toggle = cx.debug_bounds("selection-language-toggle").unwrap();
            cx.simulate_click(toggle.center(), Modifiers::default());
            settle(cx);
            reader.read_with(cx, |r, _| {
                let html = &r.text_views[&(0, original_first)].0;
                assert_eq!(
                    html.contains(if original_first {
                        "编码器译文。"
                    } else {
                        "Original encoder."
                    }),
                    expanded
                );
                assert!(html.contains("Original decoder.") && html.contains("解码器译文。"));
                assert_eq!(html.matches("<img ").count(), 1);
                assert_eq!(r.reading_mode, ReadingMode::Bilingual);
            });
        }
    }
}

#[gpui_kit::test]
fn display_equations_have_one_native_row_and_no_language_toggle_in_all_modes(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let tex = r"\operatorname{Attention}\left(Q,K,V\right)=\operatorname{softmax}\left(\frac{QK^{T}}{\sqrt{d_{k}}}\right)V";
    let equation = easyread_gpui::formula::layout(tex, true).unwrap();
    easyread_gpui::formula::layout("d_{k}", false).unwrap();
    let inline = easyread_gpui::formula::native_html("d_{k}", false);
    let recovered = format!(
        "<p>Before uses {inline}.</p><p style=\"text-align:center\" data-vellio-equation=\"true\">{} &#8195;(1)</p><p>After uses {inline}.</p>",
        easyread_gpui::formula::native_html(tex, true)
    );
    let en = "Before uses dk.\nAttention(Q,K,V) = softmax(QKT/sqrt(dk))V (1)\nAfter uses dk.";
    for saved in [false, true] {
        let (zh, metadata) = if saved {
            let plan = easyread_gpui::translation_layout::Plan::from_html(&recovered).unwrap();
            let translated = plan
                .translate(
                    &plan
                        .input()
                        .replace("Before uses ", "之前使用 ")
                        .replace("After uses ", "之后使用 "),
                )
                .unwrap();
            let metadata = easyread_gpui::translation_layout::metadata(en, &translated);
            (translated.markdown, metadata)
        } else {
            (
                "之前使用 dk。\nAttention(Q,K,V) = softmax(QKᵀ/√dk)V (1)\n之后使用 dk。".to_owned(),
                Value::Null,
            )
        };
        let raw = json!({"paper":{"meta":{"title_en":"Shared display equation"},"blocks":[
            {"id":"page","type":"para","source_kind":"pdf-text","page":1,"en":en,"zh":zh,"native_translation_layout":metadata}
        ]},"reader":{"notes":{"existing":{"id":"existing","anchor":"page","quote":"After uses","kind":"highlight","color":"yellow"}}}});
        let original = raw.clone();
        let mut reader = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| {
                let mut r = ReaderView::new(raw, None, None, window, cx);
                r.pdf_code_html.insert(
                    0,
                    (
                        en.into(),
                        Some(easyread_gpui::pdf::CodePresentation {
                            html: recovered.clone(),
                            inline_figures: vec![],
                            figures: vec![],
                            math: vec![],
                        }),
                    ),
                );
                r
            });
            reader = Some(view.clone());
            Root::new(view, window, cx)
        });
        let reader = reader.unwrap();
        for (mode, primary) in [
            (ReadingMode::Bilingual, true),
            (ReadingMode::Bilingual, false),
            (ReadingMode::Original, true),
            (ReadingMode::Translation, false),
        ] {
            reader.update(cx, |r, cx| {
                r.original_first = primary;
                r.set_reading_mode(mode, cx);
            });
            settle(cx);
            let eq_index = if mode == ReadingMode::Bilingual { 2 } else { 1 };
            let bounds = reader.read_with(cx, |r, cx| {
                assert_eq!(r.rendered_languages(0), vec![primary]);
                let (html, view) = &r.text_views[&(0, primary)];
                assert_eq!(html.matches("data-display=\"true\"").count(), 1);
                assert_eq!(html.matches("(1)").count(), 1);
                assert_eq!(
                    html.matches("data-display=\"false\"").count(),
                    if mode == ReadingMode::Bilingual { 4 } else { 2 }
                );
                let view = view.read(cx);
                let bounds = view.paragraph_bounds(eq_index).unwrap();
                assert!(bounds.size.height >= px(equation.height * 0.8));
                assert!(
                    view.paragraph_bounds(eq_index + 1).unwrap().top() >= bounds.bottom() - px(1.)
                );
                bounds
            });
            // Formula selection/copy/annotation stay available, but En must
            // neither duplicate it nor fold other paragraphs on the page.
            cx.simulate_click(bounds.center(), Modifiers::default());
            settle(cx);
            reader.read_with(cx, |r, _| {
                assert!(r.selection_is_display_equation(0));
                assert!(!r.selection_can_toggle_language(0));
                assert!(!r.selection_secondary_visible(0));
            });
            let toggle = cx.debug_bounds("selection-language-toggle").unwrap();
            cx.simulate_click(toggle.center(), Modifiers::default());
            settle(cx);
            reader.update(cx, |r, cx| r.toggle_block_language(0, cx));
            settle(cx);
            reader.read_with(cx, |r, _| {
                let html = &r.text_views[&(0, primary)].0;
                assert_eq!(html.matches("data-display=\"true\"").count(), 1);
                assert_eq!(
                    html.matches("<blockquote data-easyread-companion=\"true\">")
                        .count(),
                    if mode == ReadingMode::Bilingual { 2 } else { 0 }
                );
                assert_eq!(
                    r.raw, original,
                    "Shared formulas must not rewrite translations or annotations"
                );
            });
        }
    }
}

#[gpui_kit::test]
fn bilingual_complex_block_fallback_keeps_image_only_in_primary_flow(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let source_html = "<p><img src=\"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/l9sAAAAASUVORK5CYII=\" width=\"80\" height=\"30\"></p><p>Caption.</p><ul><li>English item.</li></ul>";
    let plan = easyread_gpui::translation_layout::Plan::from_html(source_html).unwrap();
    let translated = plan
        .translate(
            &plan
                .input()
                .replace("Caption.", "题注。")
                .replace("English item.", "中文条目。"),
        )
        .unwrap();
    let mut raw = json!({"paper":{"meta":{"title_en":"Complex bilingual fallback"},"blocks":[
        {"id":"page","type":"para","source_kind":"pdf-text","page":1,"en":"Caption.\nEnglish item.","zh":translated.markdown}
    ]},"reader":{}});
    raw["paper"]["blocks"][0]["native_translation_layout"] =
        easyread_gpui::translation_layout::metadata("Caption.\nEnglish item.", &translated);
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut r = ReaderView::new(raw, None, None, window, cx);
            let source = r.state.paper.blocks[0].en.clone();
            r.pdf_code_html.insert(
                0,
                (
                    source,
                    Some(easyread_gpui::pdf::CodePresentation {
                        html: source_html.into(),
                        inline_figures: vec![],
                        figures: vec![],
                        math: vec![],
                    }),
                ),
            );
            r
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    for original_first in [true, false] {
        reader.update(cx, |r, cx| {
            r.original_first = original_first;
            r.set_reading_mode(ReadingMode::Bilingual, cx);
        });
        settle(cx);
        reader.read_with(cx, |r, _| {
            assert!(r.unpaired_paragraphs.contains(&0));
            assert_eq!(
                r.rendered_languages(0),
                vec![original_first, !original_first]
            );
            let primary = &r.text_views[&(0, original_first)].0;
            let secondary = &r.text_views[&(0, !original_first)].0;
            assert_eq!(primary.matches("<img ").count(), 1);
            assert!(!secondary.contains("<img "));
            assert!(primary.contains(if original_first {
                "English item."
            } else {
                "中文条目。"
            }));
            assert!(secondary.contains(if original_first {
                "中文条目。"
            } else {
                "English item."
            }));
        });
    }
}

#[gpui_kit::test]
fn bilingual_pairs_are_built_after_pending_pdf_recovery(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let raw = json!({"paper":{"meta":{"title_en":"Pending bilingual PDF"},"blocks":[
        {"id":"page","type":"para","source_kind":"pdf-text","page":1,
         "en":"First original.\n\nSecond original.","zh":"第一段译文。\n\n第二段译文。"}
    ]},"reader":{}});
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut r = ReaderView::new(raw, None, None, window, cx);
            r.reading_mode = ReadingMode::Bilingual;
            r.original_first = false;
            r.pdf_code_loading.insert(0);
            r
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    settle(cx);
    reader.read_with(cx, |r, _| assert!(!r.paragraph_languages.contains_key(&0)));
    reader.update(cx, |r, cx| {
        let source = r.state.paper.blocks[0].en.clone();
        r.pdf_code_html.insert(
            0,
            (
                source,
                Some(easyread_gpui::pdf::CodePresentation {
                    html: "<p>First original.</p><p>Second original.</p>".into(),
                    inline_figures: vec![],
                    figures: vec![],
                    math: vec![],
                }),
            ),
        );
        r.pdf_code_loading.remove(&0);
        r.warm_pdf_text(0, cx);
        r.reset_rows();
        cx.notify();
    });
    settle(cx);
    reader.read_with(cx, |r, _| {
        assert_eq!(r.rendered_languages(0), vec![false]);
        let html = &r.text_views[&(0, false)].0;
        assert!(html.find("第一段译文") < html.find("First original"));
        assert!(html.find("First original") < html.find("第二段译文"));
        assert!(html.find("第二段译文") < html.find("Second original"));
    });
}

#[gpui_kit::test]
fn bilingual_epub_pairs_share_one_measured_flow_in_both_orders(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let raw = json!({"paper":{"meta":{"format":"epub","title_en":"Bilingual EPUB"},"blocks":[
        {"id":"chapter","type":"para","source_kind":"epub-xhtml","chapter":1,
         "en":"First original.\n\nSecond original.","zh":"第一段译文。\n\n第二段译文。",
         "html":"<p>First original.</p><p>Second original.</p>"}
    ]},"reader":{}});
    let paper = raw["paper"].clone();
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(raw, None, None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    for original_first in [true, false] {
        reader.update(cx, |r, cx| {
            r.original_first = original_first;
            r.set_reading_mode(ReadingMode::Bilingual, cx);
        });
        settle(cx);
        reader.read_with(cx, |r, cx| {
            assert!(r.epub_reflow.complete);
            assert_eq!(r.rendered_languages(0), vec![original_first]);
            let (html, view) = &r.text_views[&(0, original_first)];
            if original_first {
                assert!(html.find("First original") < html.find("第一段译文"));
                assert!(html.find("第一段译文") < html.find("Second original"));
                assert!(html.find("Second original") < html.find("第二段译文"));
            } else {
                assert!(html.find("第一段译文") < html.find("First original"));
                assert!(html.find("First original") < html.find("第二段译文"));
                assert!(html.find("第二段译文") < html.find("Second original"));
            }
            let view = view.read(cx);
            for paragraph in 0..4 {
                assert!(
                    view.paragraph_bounds(paragraph).unwrap().bottom()
                        <= view.bounds().bottom() + px(1.)
                );
            }
            assert_eq!(r.raw["paper"], paper);
        });
        let pages = reader.read_with(cx, |r, _| r.epub_reflow.pages.clone());
        settle(cx);
        reader.read_with(cx, |r, _| {
            assert!(r.epub_reflow.complete);
            assert_eq!(
                r.epub_reflow.pages, pages,
                "Paired pagination must remain stable"
            );
        });
    }
}

#[gpui_kit::test]
fn reading_mode_epub_remeasures_both_languages_and_preserves_source(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let raw = fixture(true);
    let paper = raw["paper"].clone();
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(raw, None, None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    settle(cx);
    let source_height = reader.read_with(cx, |r, cx| {
        assert!(r.epub_reflow.complete);
        r.text_views[&(0, true)].1.read(cx).bounds().size.height
    });
    reader.update(cx, |r, cx| {
        r.toggle_block_language(0, cx);
        assert!(!r.epub_reflow.complete);
    });
    settle(cx);
    reader.read_with(cx, |r, cx| {
        assert!(r.epub_reflow.complete);
        assert!(r.text_views[&(0, false)].1.read(cx).bounds().size.height > px(0.));
        assert!(r.text_views[&(0, true)].1.read(cx).bounds().size.height >= source_height);
        assert_eq!(r.raw["paper"], paper);
    });
    reader.update(cx, |r, cx| {
        r.original_first = false;
        r.set_reading_mode(ReadingMode::Bilingual, cx);
    });
    settle(cx);
    reader.read_with(cx, |r, _| {
        assert!(r.epub_reflow.complete);
        assert_eq!(r.visible_languages(0), vec![false, true]);
        assert_eq!(r.raw["paper"], paper);
    });
}
