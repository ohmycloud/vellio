use super::*;

#[gpui_kit::test]
fn epub_imported_semantics_images_links_and_annotations_paint_in_native_reader(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let code_runs = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let observed = code_runs.clone();
    let highlighter = move |block: &gpui_kit::base::text::CodeBlock| {
        let runs = crate::desktop::code::highlight(block);
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
                    let runs = crate::desktop::code::highlight(&gpui_kit::base::text::CodeBlock::from_code(source.clone(), lang));
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
