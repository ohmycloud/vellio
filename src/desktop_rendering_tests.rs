//! The reader must not paint a provisional PDF AST while formatting is pending.
use super::{ReaderView, ReadingMode};
use easyread_gpui::pdf::CodePresentation;
use gpui_kit::{AppContext, ListOffset, TestAppContext, VisualTestContext, component::Root, px};
use serde_json::{Value, json};

// Existing regressions exercise the translated/active body, not the new default.
fn translated_reader(
    raw: Value,
    library: Option<easyread_gpui::store::Library>,
    workspace: Option<easyread_gpui::store::Workspace>,
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::Context<ReaderView>,
) -> ReaderView {
    let mut reader = ReaderView::new(raw, library, workspace, window, cx);
    reader.reading_mode = ReadingMode::Translation;
    reader
}

#[allow(dead_code)]
#[path = "../tests/support/mod.rs"]
mod pdf_fixtures;

#[path = "../tests/support/math.rs"]
mod math_fixtures;

#[path = "../tests/support/epub.rs"]
mod epub_fixtures;

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
fn epub_imported_semantics_images_links_and_annotations_paint_in_native_reader(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let code_runs = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let observed = code_runs.clone();
    let highlighter = move |block: &gpui_kit::base::text::CodeBlock| {
        let runs = super::code::highlight(block);
        let colored = runs
            .first()
            .is_some_and(|first| runs.iter().any(|(_, style)| style.color != first.1.color));
        observed.lock().unwrap().push((
            block.lang().map(|s| s.to_string()),
            block.code().to_string(),
            colored,
        ));
        runs
    };
    cx.update(|cx| {
        cx.set_global(
            gpui_kit::base::TextViewDefaults::new()
                .with_code_block_highlighter(highlighter.clone()),
        );
    });
    let synthetic = std::env::var_os("EASYREAD_EPUB_QA_SOURCE").is_none();
    let temp = tempfile::tempdir().unwrap();
    let fixture = temp.path().join("native.epub");
    epub_fixtures::fixture(&fixture, false);
    let source = std::env::var_os("EASYREAD_EPUB_QA_SOURCE")
        .map(std::path::PathBuf::from)
        .unwrap_or(fixture);
    let ws = easyread_gpui::store::Library::open(temp.path().join("library"))
        .unwrap()
        .import_file(&source, None)
        .unwrap();
    let mut raw = ws.validated_state().unwrap();
    if synthetic {
        // Reproduce HTML already persisted by the old EPUB importer: token
        // code siblings, no language attribute. Do not migrate user data.
        let legacy = raw["paper"]["blocks"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|block| {
                block["en"]
                    .as_str()
                    .is_some_and(|s| s.contains("def legacy():"))
            })
            .unwrap();
        legacy["html"] = json!(
            "<pre><code>def</code> legacy():\n    <code>return</code> &quot;unlabeled code&quot;\n</pre>"
        );
    }
    let mut reader = None;
    let (_root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| translated_reader(raw, None, Some(ws.clone()), window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    draw(cx);
    cx.run_until_parked();
    draw(cx);
    if std::env::var_os("EASYREAD_EPUB_QA_AUDIT").is_some() {
        reader.read_with(cx, |reader, _| {
            let mut total = 0;
            let mut hinted = 0;
            let mut colored = 0;
            for (index, block) in reader.raw["paper"]["blocks"].as_array().unwrap().iter().enumerate() {
                if !block["html"].as_str().is_some_and(|html| html.contains("<pre")) { continue; }
                let html = format!("<root>{}</root>", reader.reader_body_html(index, false));
                let doc = roxmltree::Document::parse(&html).unwrap();
                for pre in doc.descendants().filter(|n| n.has_tag_name("pre")) {
                    total += 1;
                    let source = pre.descendants().filter(|n| n.is_text()).filter_map(|n| n.text()).collect::<String>();
                    let lang = pre.children().filter(|n| n.has_tag_name("code")).find_map(|n| n.attribute("class").and_then(|classes| classes.split_whitespace().find_map(|class| class.strip_prefix("language-"))));
                    hinted += usize::from(lang == Some("python"));
                    let runs = super::code::highlight(&gpui_kit::base::text::CodeBlock::from_code(source.clone(), lang));
                    assert!(runs.iter().all(|(range, _)| source.get(range.clone()).is_some()));
                    colored += usize::from(runs.first().is_some_and(|first| runs.iter().any(|(_, style)| style.color != first.1.color)));
                }
            }
            eprintln!("EPUB code audit: {total} pre blocks, {hinted} display-only Python hints, {colored} multicolor blocks; document hint {:?}", reader.epub_code_language);
        });
    }
    let indices = reader.read_with(cx, |reader, _| {
        assert!(!reader.contents().is_empty());
        assert!(reader.pdf_code_cache.is_none());
        assert!(!reader.show_pdf);
        // Sample every structural category plus the last chapter, not just a
        // string conversion: each row must acquire native painted geometry.
        let mut indices = std::collections::BTreeSet::new();
        if let Ok(id) = std::env::var("EASYREAD_EPUB_QA_BLOCK") {
            let index = reader
                .state
                .paper
                .blocks
                .iter()
                .position(|b| b.id == id)
                .expect("EASYREAD_EPUB_QA_BLOCK must identify an imported block");
            indices.insert(index);
        }
        for tag in [
            "<h1",
            "<h2",
            "<p>",
            "<ul>",
            "<ol>",
            "<pre>",
            "<table>",
            "<blockquote>",
        ] {
            if let Some(i) = reader.raw["paper"]["blocks"]
                .as_array()
                .unwrap()
                .iter()
                .position(|b| b["html"].as_str().is_some_and(|s| s.contains(tag)))
            {
                indices.insert(i);
            }
        }
        if synthetic {
            for (i, block) in reader.raw["paper"]["blocks"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
            {
                if block["html"]
                    .as_str()
                    .is_some_and(|html| html.contains("<pre>"))
                {
                    indices.insert(i);
                }
            }
        }
        if let Some(i) = reader
            .state
            .paper
            .blocks
            .iter()
            .position(|b| b.kind == "figure")
        {
            indices.insert(i);
        }
        if let Some(i) = reader
            .state
            .paper
            .blocks
            .iter()
            .position(|b| b.kind == "figure" && b.caption_en.starts_with("Figure "))
        {
            indices.insert(i);
        }
        if let Some(i) = reader
            .state
            .paper
            .blocks
            .iter()
            .rposition(|b| b.kind == "heading")
        {
            indices.insert(i);
        }
        indices
    });
    assert!(indices.len() >= 3);
    for index in indices {
        let probe = reader.read_with(cx, |reader, _| {
            std::env::var("EASYREAD_EPUB_QA_BLOCK")
                .ok()
                .is_some_and(|id| reader.state.paper.blocks[index].id == id)
        });
        if probe {
            code_runs.lock().unwrap().clear();
            // A new callback identity defeats syntax caches populated while
            // this row was previously visible, without changing its styles.
            cx.update(|_, cx| {
                let defaults = gpui_kit::base::TextViewDefaults::global(cx)
                    .clone()
                    .with_code_block_highlighter(highlighter.clone());
                cx.set_global(defaults);
            });
        }
        reader.update(cx, |reader, cx| {
            reader.list.scroll_to(ListOffset {
                item_ix: index,
                offset_in_item: px(0.),
            });
            cx.notify();
        });
        draw(cx);
        cx.run_until_parked();
        draw(cx);
        cx.run_until_parked();
        draw(cx);
        reader.update(cx, |reader, cx| {
            let block = &reader.state.paper.blocks[index];
            if probe {
                let first_line = block
                    .en
                    .lines()
                    .skip(1)
                    .find(|line| !line.trim().is_empty())
                    .expect("QA block must contain code");
                assert!(
                    code_runs
                        .lock()
                        .unwrap()
                        .iter()
                        .any(|(_, code, colored)| *colored
                            && code.trim_start().starts_with(first_line.trim())),
                    "the requested code block must actually invoke multicolor native highlighting"
                );
            }
            if block.kind == "figure" {
                assert!(
                    reader
                        .block_images
                        .get(&index)
                        .is_some_and(|image| image.is_ok()),
                    "EPUB figure {index} must load"
                );
                let path = ws
                    .asset(
                        reader.raw["paper"]["blocks"][index]["src"]
                            .as_str()
                            .unwrap(),
                    )
                    .unwrap();
                let original = std::fs::read(&path).unwrap();
                let display =
                    easyread_gpui::figure_image::display_bytes("epub-image", original.clone())
                        .unwrap();
                assert_eq!(
                    reader.block_images[&index].as_ref().unwrap().0.bytes(),
                    display
                );
                assert_eq!(
                    std::fs::read(path).unwrap(),
                    original,
                    "display must not rewrite source images"
                );
                let rgba = image::load_from_memory(&display).unwrap().to_rgba8();
                let source_rgba = image::load_from_memory(&original).unwrap().to_rgba8();
                if synthetic
                    || (block.caption_en.starts_with("Figure ")
                        && source_rgba.get_pixel(0, 0).0 == [255, 255, 255, 255])
                {
                    assert_eq!(
                        rgba.get_pixel(0, 0)[3],
                        0,
                        "reader paper must show through the figure border"
                    );
                }
                if synthetic {
                    assert_eq!(
                        rgba.get_pixel(80, 50)[3],
                        255,
                        "enclosed white must remain white"
                    );
                }
            } else {
                let view = reader.text_views[&(index, false)].1.clone();
                assert!(
                    view.read(cx).is_content_ready(),
                    "EPUB block {index} must parse"
                );
                assert!(
                    view.read(cx).bounds().size.height > px(0.),
                    "EPUB block {index} must paint"
                );
                view.update(cx, |state, cx| state.select_all(cx));
                assert!(
                    !view.read(cx).selected_text().trim().is_empty(),
                    "EPUB text remains selectable"
                );
            }
        });
    }
    if synthetic {
        let calls = code_runs.lock().unwrap();
        assert!(
            calls
                .iter()
                .any(|(lang, code, colored)| lang.as_deref() == Some("python")
                    && code.contains("def today():")
                    && *colored),
            "declared EPUB code must reach the native highlighter"
        );
        assert!(
            calls.iter().any(|(lang, code, colored)| lang.is_none()
                && code.contains("def legacy():")
                && *colored),
            "existing unlabeled EPUB code must also be colored"
        );
    }
    // The link map routes to a block without invoking PDF or opening a URL.
    cx.update(|window, cx| {
        reader.update(cx, |reader, cx| {
            let target = reader.raw["paper"]["link_targets"].as_object().unwrap().keys().next().unwrap().clone();
            let expected = reader.raw["paper"]["link_targets"][&target].as_str().unwrap().to_owned();
            reader.follow_epub_link(&target, window, cx);
            assert_eq!(reader.state.paper.blocks[reader.selected].id, expected);
            let index = reader.state.paper.blocks.iter().position(|b| b.kind == "para" && !b.en.trim().is_empty()).unwrap();
            let id = reader.state.paper.blocks[index].id.clone();
            let source = reader.reader_body_html(index, false);
            let quote = easyread_gpui::annotations::plain_text(&source).chars().take(12).collect::<String>();
            reader.raw["reader"]["notes"] = json!({"epub-native-note":{"id":"epub-native-note","anchor":id,"key":id,"kind":"highlight","color":"yellow","quote":quote}});
            let decorated = reader.reader_body_html(index, false);
            assert!(decorated.contains("epub-native-note"));
            reader.list.scroll_to(ListOffset { item_ix: index, offset_in_item: px(0.) });
            cx.notify();
        });
    });
    draw(cx);
    cx.run_until_parked();
    draw(cx);
    assert!(cx.debug_bounds("reader-text-ready").is_some());
    assert!(!ws.root.join(".native-pdf-layout-cache").exists());
    cx.update(|window, cx| {
        reader.update(cx, |reader, cx| {
            if let Some((image, aspect, width)) = reader
                .block_images
                .values()
                .find_map(|result| result.as_ref().ok())
                .cloned()
            {
                reader.open_image_preview(image.clone(), aspect, width, window, cx);
                assert!(
                    std::sync::Arc::ptr_eq(&reader.image_preview.as_ref().unwrap().image, &image),
                    "preview must reuse the paper-adapted image, not reload the white original"
                );
            }
        });
    });
    draw(cx);
}

#[gpui_kit::test]
fn epub_python_fragments_keep_source_and_restore_missing_reader_highlighter(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    cx.update(|cx| cx.set_global(gpui_kit::base::TextViewDefaults::new()));
    let samples = [
        ("seed-import", "import math\n"),
        ("seed-def", "def twice(value):\n    return value * 2\n"),
        (
            "seed-class",
            "class Attention:\n    def forward(self, x):\n        return x @ x.T\n",
        ),
        (
            "with",
            "with open(\"input.txt\") as f:\n    text = f.read()\n",
        ),
        ("print", "print(len(text))\n"),
        ("assignment", "result = re.split(r'(\\s)', text)\n"),
        ("call", "torch.manual_seed(123)\n"),
        ("output", "tensor([[0.2, 0.3]])\n"),
    ];
    let blocks: Vec<_> = samples.iter().map(|(id, source)| {
        let escaped = source.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
        json!({"id":id,"type":"para","source_kind":"epub-xhtml","en":format!("```\n{source}```"),"html":format!("<pre><code>{escaped}</code></pre>")})
    }).collect();
    let raw = json!({"paper":{"meta":{"title_en":"Unlabeled Python EPUB regression","format":"epub"},"blocks":blocks},"reader":{}});
    let original_blocks = raw["paper"]["blocks"].clone();
    let mut reader = None;
    let (_root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| translated_reader(raw, None, None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    for (index, (id, source)) in samples.iter().enumerate() {
        reader.update(cx, |reader, cx| {
            assert_eq!(reader.epub_code_language, Some("python"));
            reader.list.scroll_to(ListOffset {
                item_ix: index,
                offset_in_item: px(0.),
            });
            cx.notify();
        });
        draw(cx);
        cx.run_until_parked();
        draw(cx);
        reader.update(cx, |reader, cx| {
            assert!(gpui_kit::base::TextViewDefaults::global(cx).has_code_block_highlighter());
            let (html, view) = &reader.text_views[&(index, false)];
            assert!(view.read(cx).is_content_ready());
            assert!(view.read(cx).bounds().size.height > px(0.));
            if matches!(*id, "assignment" | "call") {
                assert!(html.contains("language-python"));
            }
            if *id == "output" {
                assert!(!html.contains("language-python"));
            }
            let view = view.clone();
            view.update(cx, |view, cx| view.select_all(cx));
            assert!(
                view.read(cx).selected_text().contains(source.trim_end()),
                "source and indentation must survive"
            );
            assert_eq!(reader.raw["paper"]["blocks"], original_blocks);
        });
    }
    // Simulate losing outer defaults while a book is already open.
    cx.update(|_, cx| {
        cx.set_global(gpui_kit::base::TextViewDefaults::new());
        reader.update(cx, |reader, cx| {
            reader.reset_rows();
            cx.notify();
        });
    });
    draw(cx);
    assert!(cx.update(|_, cx| gpui_kit::base::TextViewDefaults::global(cx).has_code_block_highlighter()));
}

#[gpui_kit::test]
fn legacy_epub_figure_caption_is_not_rendered_as_a_bold_heading(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    for translated in [false, true] {
        let source = "Figure 7.1 A publisher caption with emphasis.";
        let displayed = if translated {
            "图 7.1 已有的用户译文。"
        } else {
            source
        };
        let raw = json!({"paper":{"meta":{"title_en":"Publisher caption regression","format":"epub"},"blocks":[
            {"id":"image","type":"figure","source_kind":"epub-image","source_location":{"href":"chapter.xhtml","dom":"body/1/6/1"}},
            {"id":"caption","type":"heading","source_kind":"epub-xhtml","level":5,"en":source,
             "html":"<h5>Figure 7.1 A publisher caption with <em>emphasis</em>.</h5>",
             "source_location":{"href":"chapter.xhtml","dom":"body/1/6/3"}},
            {"id":"section","type":"heading","source_kind":"epub-xhtml","level":2,"en":"7.1 A real section",
             "html":"<h2>7.1 A real section</h2>","source_location":{"href":"chapter.xhtml","dom":"body/1/8/1"}}
        ]},"reader":{"edits":if translated {json!({"caption":{"zh":displayed,"base":easyread_gpui::model::text_hash(source)}})} else {json!({})}}});
        let original = raw.clone();
        let mut reader = None;
        let (_root, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| translated_reader(raw, None, None, window, cx));
            reader = Some(view.clone());
            Root::new(view, window, cx)
        });
        let reader = reader.unwrap();
        reader.update(cx, |reader, cx| {
            reader.list.scroll_to(ListOffset {
                item_ix: 1,
                offset_in_item: px(0.),
            });
            cx.notify();
        });
        draw(cx);
        cx.run_until_parked();
        draw(cx);
        reader.update(cx, |reader, cx| {
            let (html, view) = &reader.text_views[&(1, false)];
            assert!(
                html.starts_with("<p style=\"text-align: center\">"),
                "caption must be centered without heading weight: {html}"
            );
            assert!(!html.contains("<h5>"));
            if !translated {
                assert!(
                    html.contains("<em>emphasis</em>"),
                    "explicit inline emphasis must survive"
                );
            }
            let view = view.clone();
            assert!(view.read(cx).is_content_ready());
            assert!(view.read(cx).bounds().size.height > px(0.));
            view.update(cx, |view, cx| view.select_all(cx));
            assert_eq!(view.read(cx).selected_text().trim_end(), displayed);
            assert_eq!(reader.contents().len(), 1);
            assert_eq!(
                reader.contents()[0].0,
                2,
                "actual section must remain in contents"
            );
            assert!(reader.reader_body_html(2, false).starts_with("<h2>"));
            assert!(
                reader
                    .reader_body_html(1, true)
                    .starts_with("<p style=\"text-align: center\">")
            );
            assert_eq!(
                reader.raw, original,
                "display compatibility must not rewrite source or edits"
            );
        });
    }
}

#[gpui_kit::test]
fn epub_figure_caption_rows_are_centered_on_the_image_including_appendices_and_code(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbImage::new(160, 100)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let image = std::sync::Arc::new(gpui_kit::Image::from_bytes(
        gpui_kit::ImageFormat::Png,
        png.into_inner(),
    ));
    let translation = "图 A.6 不同阶数的张量。";
    for (label, body, translated) in [
        ("Figure 7.1", "A short caption.", false),
        ("Figure A.6", "Tensors with different ranks.", false),
        ("Figure A.6", "Tensors with different ranks.", true),
        (
            "Figure D.2",
            "A caption with <code>learning_rate</code> that explains how several warmup steps are followed by decay and includes enough words to wrap over multiple centered lines at narrow reader widths.",
            false,
        ),
        (
            "Figure E.5",
            "A <strong>locally emphasized</strong> caption with <em>italics</em> and <code>training_loss</code> that includes enough explanatory words to occupy more than one centered line and test the last short wrapped line.",
            false,
        ),
    ] {
        let html = format!("<h5>{label} {body}</h5>");
        let plain = easyread_gpui::annotations::plain_text(&html);
        let raw = json!({"paper":{"meta":{"title_en":"Caption centering regression","format":"epub"},"blocks":[
            {"id":"image","type":"figure","source_kind":"epub-image","width":160,"height":100,"source_location":{"href":"chapter.xhtml","dom":"body/1/6/1"}},
            {"id":"caption","type":"heading","source_kind":"epub-xhtml","level":5,"en":plain,"html":html,"zh":if translated {translation} else {""},
             "source_location":{"href":"chapter.xhtml","dom":"body/1/6/3"}}
        ]},"reader":{}});
        let original = raw.clone();
        let mut reader = None;
        let (_root, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| {
                let mut reader = translated_reader(raw, None, None, window, cx);
                reader.block_images.insert(0, Ok((image.clone(), 1.6, 160)));
                if translated {
                    reader.original_blocks.insert(1, true);
                }
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
            for &show_original in if translated {
                &[false, true][..]
            } else {
                &[false][..]
            } {
                reader.update(cx, |reader, cx| {
                    for ((index, _), (_, view)) in &reader.text_views {
                        if *index == 1 {
                            view.update(cx, |view, cx| view.clear_selection(cx));
                        }
                    }
                    let (html, view) = &reader.text_views[&(1, show_original)];
                    assert!(!html.contains("<h5>"), "missed caption: {label}");
                    let view = view.clone();
                    view.update(cx, |view, cx| view.select_all(cx));
                    let expected = if translated && !show_original {
                        translation
                    } else {
                        &plain
                    };
                    assert_eq!(view.read(cx).selected_text().trim_end(), expected);
                    assert_eq!(reader.raw, original);
                });
                draw(cx);
                let image_bounds = cx
                    .debug_bounds("reader-figure-image-0")
                    .expect("image must paint");
                let (mut cells, scale) = cx.update(|window, cx| {
                    let color: gpui_kit::Background = gpui_kit::base::Theme::global(cx)
                        .tokens
                        .colors
                        .selection
                        .into();
                    (
                        window
                            .painted_quads()
                            .into_iter()
                            .filter(|q| q.background == color)
                            .map(|q| q.bounds)
                            .collect::<Vec<_>>(),
                        window.scale_factor(),
                    )
                });
                assert!(!cells.is_empty());
                cells.sort_by(|a, b| {
                    a.top()
                        .partial_cmp(&b.top())
                        .unwrap()
                        .then_with(|| a.left().partial_cmp(&b.left()).unwrap())
                });
                let mut rows: Vec<gpui_kit::Bounds<gpui_kit::ScaledPixels>> = Vec::new();
                for cell in cells {
                    if let Some(row) = rows.last_mut().filter(|row| row.top() == cell.top()) {
                        *row = gpui_kit::Bounds::from_corners(row.origin, cell.bottom_right());
                    } else {
                        rows.push(cell);
                    }
                }
                if body.contains("<code>") {
                    assert!(rows.len() > 1, "must exercise wrapping");
                }
                for row in rows {
                    assert!(
                        (row.center().x.0 / scale - f32::from(image_bounds.center().x)).abs() < 0.6,
                        "caption row must share the image center: {label}, width={width}, original={show_original}, row={row:?}, image={image_bounds:?}"
                    );
                }
            }
        }
    }
}

#[gpui_kit::test]
fn paragraph_annotation_is_continuous_across_bold_inline_code_and_reflow(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let source = "Unmarked lead selected text with **bold words** and *italic words* before `begin_search_query` and `end_search_query` followed by more selected words with a [document link](part.xhtml), then `begin_search_result` and `end_search_result` followed by the final selected words unmarked tail.";
    let plain = easyread_gpui::annotations::plain_text(&easyread_gpui::annotations::html(source));
    let quote = plain
        .strip_prefix("Unmarked lead ")
        .unwrap()
        .strip_suffix(" unmarked tail.")
        .unwrap();
    let raw = json!({"paper":{"meta":{"title_en":"Mixed-font highlight regression"},"blocks":[{"id":"paragraph","type":"para","en":source}]},"reader":{"notes":{"mark":{"id":"mark","anchor":"paragraph","key":"paragraph","kind":"highlight","color":"yellow","quote":quote}}}});
    let original = raw.clone();
    let mut reader = None;
    let (_root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| translated_reader(raw, None, None, window, cx));
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
        let mut rows = cx.update(|window, _| {
            let color: gpui_kit::Background =
                gpui_kit::rgb(easyread_gpui::annotations::HighlightColor::Yellow.rgb()).into();
            let quads = window
                .painted_quads()
                .into_iter()
                .filter(|q| q.background == color)
                .collect::<Vec<_>>();
            assert!(quads.len() > 3);
            assert!(quads.iter().all(|q| q.corner_radii == Default::default()));
            quads.into_iter().map(|q| q.bounds).collect::<Vec<_>>()
        });
        rows.sort_by(|a, b| a.top().partial_cmp(&b.top()).unwrap());
        rows.dedup_by(|a, b| a.top() == b.top());
        for pair in rows.windows(2) {
            assert_eq!(
                pair[0].bottom(),
                pair[1].top(),
                "actual reader highlight must not have mixed-font row gaps: {rows:?}"
            );
        }
        reader.update(cx, |reader, cx| {
            assert_eq!(reader.raw["paper"], original["paper"]);
            assert_eq!(reader.raw["reader"]["notes"], original["reader"]["notes"]);
            let view = reader.text_views[&(0, false)].1.clone();
            assert!(view.read(cx).is_content_ready());
            assert!(view.read(cx).annotation_offset("mark").is_some());
            view.update(cx, |view, cx| view.select_all(cx));
            assert!(view.read(cx).selected_text().contains("begin_search_query"));
        });
    }
}

#[gpui_kit::test]
fn paragraph_selection_is_continuous_across_mixed_fonts_and_reflow(cx: &mut TestAppContext) {
    fn selection_rows(cx: &mut VisualTestContext) -> Vec<gpui_kit::Bounds<gpui_kit::ScaledPixels>> {
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
                    "mixed-font cells must share row height"
                );
                assert!(
                    (row.right() - cell.left()).0.abs() < 0.01,
                    "selection must include inline padding without gaps or overlaps: {row:?}, {cell:?}"
                );
                *row = gpui_kit::Bounds::from_corners(row.origin, cell.bottom_right());
            } else {
                rows.push(cell);
            }
        }
        assert!(rows.len() > 1);
        for pair in rows.windows(2) {
            assert_eq!(
                pair[0].bottom(),
                pair[1].top(),
                "selection must not have mixed-font row gaps: {rows:?}"
            );
        }
        rows
    }

    cx.update(gpui_kit::init);
    let source = "The `SpamDataset` class loads data from the CSV files we created earlier, tokenizes the text using the GPT-2 tokenizer from `tiktoken`, and allows us to *pad* or *truncate* the sequences to a uniform length determined by **the longest sequence**. More **bold words** and ordinary words follow to exercise wrapped selection.";
    let plain = easyread_gpui::annotations::plain_text(&easyread_gpui::annotations::html(source));
    let raw = json!({"paper":{"meta":{"title_en":"Mixed-font selection regression"},"blocks":[{"id":"paragraph","type":"para","en":source}]},"reader":{}});
    let mut reader = None;
    let (_root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| translated_reader(raw, None, None, window, cx));
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
        reader.update(cx, |reader, cx| {
            let view = reader.text_views[&(0, false)].1.clone();
            view.update(cx, |view, cx| view.select_all(cx));
            assert_eq!(view.read(cx).selected_text().trim_end(), plain);
        });
        draw(cx);
        let rows = selection_rows(cx);
        let first = rows[0];
        let last = *rows.last().unwrap();
        let scale = cx.update(|window, _| window.scale_factor());
        let start = gpui_kit::point(
            px(first.left().0 / scale + 20.),
            px(first.center().y.0 / scale),
        );
        let end = gpui_kit::point(px(last.center().x.0 / scale), px(last.center().y.0 / scale));
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
            let selected = selection_rows(cx);
            assert!(
                selected[0].left() > first.left(),
                "partial selection must not fill the leading unselected text"
            );
            assert!(
                selected.last().unwrap().right() < last.right(),
                "partial selection must not fill the trailing unselected text"
            );
            reader.read_with(cx, |reader, cx| {
                let text = reader.text_views[&(0, false)].1.read(cx).selected_text();
                assert!(!text.trim().is_empty());
                assert!(
                    plain.contains(text.trim_end()),
                    "drag must copy a continuous substring: {text:?}"
                );
            });
        }
    }
}

fn fixture() -> Value {
    json!({
        "paper": {
            "meta": {"title_en": "PDF formatting regression"},
            "blocks": [{"id": "code-page", "type": "para", "page": 323,
                "source_kind": "pdf-text", "en": "def run():\n    return 'formatted code'"}]
        },
        "reader": {"notes": {}}
    })
}

fn draw(cx: &mut VisualTestContext) {
    // Deliberately do not run the parser executor before inspecting this frame.
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        window.draw(cx).clear(cx);
    });
}

fn finish_presentation(
    reader: &gpui_kit::Entity<ReaderView>,
    html: Option<String>,
    cx: &mut VisualTestContext,
) {
    reader.update(cx, |reader, cx| {
        let source = reader.state.paper.blocks[0].en.clone();
        reader.pdf_code_loading.remove(&0);
        reader.pdf_code_html.insert(
            0,
            (
                source,
                html.map(|html| CodePresentation {
                    html,
                    math: vec![],
                    inline_figures: vec![],
                    figures: vec![],
                }),
            ),
        );
        reader.list.remeasure_items(0..1);
        cx.notify();
    });
}

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
