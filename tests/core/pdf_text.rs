use super::*;

#[test]
fn batched_pdf_layout_matches_single_page_and_cache_preserves_book_records() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("terms.pdf");
    support::definitions_pdf_fixture(&source);
    let ws = Library::open(temp.path().join("library"))
        .unwrap()
        .import_pdf(&source, None)
        .unwrap();
    let state = ws.state().unwrap();
    let expected = state["paper"]["blocks"][0]["en"]
        .as_str()
        .unwrap()
        .to_owned();
    let before: Vec<_> = ["paper.json", "reader.json", "layout.json", "source.pdf"]
        .iter()
        .map(|name| fs::read(ws.root.join(name)).unwrap())
        .collect();
    let single = easyread_gpui::pdf::code_presentation(&source, 1, &expected)
        .unwrap()
        .unwrap();
    let batch = easyread_gpui::pdf::code_presentations(
        &source,
        &[
            (1, expected.clone()),
            (999, expected.clone()),
            (1, "edited".into()),
        ],
    )
    .unwrap();
    assert_eq!(
        batch[0].as_ref().unwrap().as_ref().unwrap().html,
        single.html
    );
    assert!(batch[1].is_err());
    assert!(batch[2].as_ref().unwrap().is_none());
    let cache = easyread_gpui::pdf_layout_cache::PresentationCache::new(
        &source,
        &ws.root.join(".native-pdf-layout-cache"),
    )
    .unwrap();
    let requests = [(1, expected), (1, "edited".into())];
    let cold = cache.prepare(&source, &requests);
    assert_eq!(
        cold[0].as_ref().unwrap().as_ref().unwrap().html,
        single.html
    );
    // No PDF open is needed on a hit: even an unavailable file is irrelevant.
    let warm = cache.prepare(&temp.path().join("unavailable.pdf"), &requests);
    assert_eq!(
        warm[0].as_ref().unwrap().as_ref().unwrap().html,
        single.html
    );
    assert!(warm[1].as_ref().unwrap().is_none());
    for (name, bytes) in ["paper.json", "reader.json", "layout.json", "source.pdf"]
        .iter()
        .zip(before)
    {
        assert_eq!(fs::read(ws.root.join(name)).unwrap(), bytes);
    }
}

#[test]
fn pdf_definitions_and_inline_fonts_keep_links_notes_code_and_saved_source() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("terms.pdf");
    support::definitions_pdf_fixture(&source);
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
    let text = state["paper"]["blocks"][0]["en"].as_str().unwrap();
    let p = easyread_gpui::pdf::code_presentation(&source, 1, text)
        .unwrap()
        .unwrap();
    let html = &p.html;
    assert_eq!(html.matches("<dl>").count(), 1, "{html}");
    assert_eq!(html.matches("<dt>").count(), 2, "{html}");
    assert_eq!(
        html.matches("data-easyread-indent=\"1.500\"").count(),
        2,
        "{html}"
    );
    assert!(
        html.contains("<code>Tools</code>")
            && html.contains("<em>carefully</em>")
            && html.contains("<strong>important</strong>"),
        "{html}"
    );
    assert!(html.contains("<dt><strong><code>description"), "{html}");
    assert!(html.contains("<strong><code>Tools</code>"), "{html}");
    assert!(
        html.contains("href=\"https://example.org/tools\""),
        "{html}"
    );
    assert!(
        html.contains("</dl><p><span>Following ordinary prose."),
        "{html}"
    );
    assert_eq!(
        html.matches("<pre>").count(),
        1,
        "Only actual code is a code block: {html}"
    );
    let normalize = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    assert_eq!(
        normalize(text),
        normalize(&easyread_gpui::annotations::plain_text(html))
    );
    let quote = "A description of all registered tools and their parameters.";
    let marked = easyread_gpui::annotations::decorate_interactive(
        html,
        &[json!({"id":"definition-note","quote":quote,"kind":"underline","color":"blue"})],
    );
    assert!(marked.contains("definition-note"), "{marked}");
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
fn vector_lists_preserve_wraps_nesting_links_notes_and_the_original_book() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("lists.pdf");
    support::vector_lists_pdf_fixture(&source);
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
    assert_eq!(
        blocks.len(),
        1,
        "Small list paths do not become image blocks"
    );
    let text = blocks[0]["en"].as_str().unwrap();
    assert!(
        !text.contains('•'),
        "Markers really are absent from PDF text"
    );
    let presentation = easyread_gpui::pdf::code_presentation(&source, 1, text)
        .unwrap()
        .unwrap();
    let html = &presentation.html;
    assert_eq!(
        html.matches("<ul>").count(),
        3,
        "Two groups plus nested list: {html}"
    );
    assert_eq!(html.matches("<li>").count(), 5, "{html}");
    assert_eq!(presentation.inline_figures.len(), 5);
    assert!(
        html.contains("First item continues <em>on the next line. </em><ul><li>"),
        "{html}"
    );
    assert!(
        html.contains("href=\"https://example.org/nested\""),
        "{html}"
    );
    assert!(
        html.contains("</ul><p><span>Following body without a full stop"),
        "{html}"
    );
    assert!(html.contains("<li>Square item"), "{html}");
    assert!(
        html.contains("<pre><code>") && html.contains("* code expression</span>"),
        "{html}"
    );
    assert!(!html.contains("<img "), "{html}");
    let normalize = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    assert_eq!(
        normalize(text),
        normalize(&easyread_gpui::annotations::plain_text(html))
    );
    let marked = easyread_gpui::annotations::decorate_interactive(
        html,
        &[
            json!({"id":"wrapped-list-note","quote":"First item continues on the next line.","kind":"underline","color":"blue"}),
        ],
    );
    assert!(marked.contains("wrapped-list-note"), "{marked}");
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
fn raised_pdf_reference_numbers_keep_entries_separate_without_changing_saved_text() {
    for size in [7, 10] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("references.pdf");
        support::references_pdf_fixture(&source, size);
        let pdf_before = fs::read(&source).unwrap();
        let lib = Library::open(temp.path().join("library")).unwrap();
        let ws = lib.import_pdf(&source, None).unwrap();
        let before: Vec<_> = ["paper", "reader", "layout"]
            .iter()
            .map(|name| fs::read(ws.root.join(format!("{name}.json"))).unwrap())
            .collect();
        let raw = ws.state().unwrap();
        let text = raw["paper"]["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["source_kind"] == "pdf-text")
            .unwrap()["en"]
            .as_str()
            .unwrap();
        let html = easyread_gpui::pdf::code_html(&source, 1, text)
            .unwrap()
            .unwrap();
        assert_eq!(html.matches("<p>").count(), 3, "{html}");
        for number in [20, 21, 22] {
            assert!(html.contains(&format!("<sup>{number}</sup>")), "{html}");
        }
        assert!(html.contains("<sup>21</sup></span></a> Bai"), "{html}");
        assert!(html.contains("2022. OPT"), "{html}");
        assert!(html.contains("2022. Flamingo"), "{html}");
        assert!(html.contains("Transformer Language Models"), "{html}");
        assert!(html.contains("Model for Few-Shot"), "{html}");
        let normalize = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        assert_eq!(
            normalize(&easyread_gpui::annotations::plain_text(&html)),
            normalize(text)
        );
        let annotated = easyread_gpui::annotations::decorate_interactive(
            &html,
            &[json!({
                "id":"ref-note", "quote":"22 Alayrac", "prefix":"Qwen-VL.\n", "kind":"underline", "color":"blue"
            })],
        );
        assert!(annotated.contains("<sup>"));
        assert!(annotated.contains("ref-note"));
        assert_eq!(
            normalize(&easyread_gpui::annotations::plain_text(&annotated)),
            normalize(text)
        );
        assert!(
            easyread_gpui::pdf::code_html(&source, 1, "user edit")
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
}

#[test]
fn wrapped_pdf_links_keep_only_labels_with_and_without_quad_points() {
    for quads in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("links.pdf");
        support::wrapped_link_fixture(&source, quads);
        let before = fs::read(&source).unwrap();
        let lib = Library::open(temp.path().join("library")).unwrap();
        let ws = lib.import_pdf(&source, None).unwrap();
        let raw = ws.state().unwrap();
        let text = raw["paper"]["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["source_kind"] == "pdf-text")
            .unwrap()["en"]
            .as_str()
            .unwrap();
        let html = easyread_gpui::pdf::code_html(&source, 1, text)
            .unwrap()
            .unwrap();
        assert!(html.contains(">GitHub</span></a> <a href="), "{html}");
        assert!(
            html.contains(">repository</span></a>). The pass rate"),
            "{html}"
        );
        assert!(
            html.contains("Key from the <a href=\"https://example.org/site\""),
            "{html}"
        );
        assert!(
            html.contains(">website</span></a>. We then create"),
            "{html}"
        );
        assert_eq!(html.matches("href=\"https://example.org/repo\"").count(), 2);
        assert_eq!(html.matches("color:#8e0012").count(), 3, "{html}");
        assert_eq!(fs::read(&source).unwrap(), before);
    }
}

#[test]
fn unnumbered_pdf_headings_use_bookmark_depth_without_code_or_reimport() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("headings.pdf");
    support::heading_pdf_fixture(&source);
    assert_eq!(
        easyread_gpui::pdf::outline(&source)
            .unwrap()
            .iter()
            .map(|e| e.depth)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    let lib = Library::open(temp.path().join("library")).unwrap();
    let ws = lib.import_pdf(&source, None).unwrap();
    let before = fs::read(ws.root.join("paper.json")).unwrap();
    let pdf_before = fs::read(&source).unwrap();
    let raw = ws.state().unwrap();
    let state = easyread_gpui::model::ReaderState::from_value(raw).unwrap();
    let block = state
        .paper
        .blocks
        .iter()
        .find(|b| b.page == 1 && b.source_kind == "pdf-text")
        .unwrap();
    assert!(
        state
            .markdown(block, false)
            .contains("Building Code Agents The first thing")
    );
    let presentation = easyread_gpui::pdf::code_presentation(&source, 1, &block.en)
        .unwrap()
        .unwrap();
    assert!(
        presentation
            .html
            .contains("<h2>Building Code Agents</h2>\n<p><span>The first thing")
    );
    assert!(
        presentation
            .html
            .contains("<h3>Code Agents to Serve Non-Coders</h3>\n<p><span>Code agents open"),
        "{}\nSource: {:?}",
        presentation.html,
        block.en
    );
    assert_eq!(presentation.html.matches("<h2>").count(), 1);
    assert_eq!(presentation.html.matches("<h3>").count(), 1);
    assert!(
        presentation
            .html
            .contains("We discuss Building Code Agents in this paragraph.")
    );
    assert!(!presentation.html.contains("<pre>"));
    assert!(presentation.inline_figures.is_empty());
    assert!(
        easyread_gpui::pdf::code_presentation(&source, 1, "user edit")
            .unwrap()
            .is_none()
    );
    let other = state
        .paper
        .blocks
        .iter()
        .find(|b| b.page == 2 && b.source_kind == "pdf-text")
        .unwrap();
    let other = easyread_gpui::pdf::code_presentation(&source, 2, &other.en)
        .unwrap()
        .unwrap();
    // Typography can recover an unbookmarked title, without borrowing the
    // depth of a similarly named bookmark on another destination page.
    assert!(other.html.contains("<h3>Building Code Agents</h3>"));
    assert!(!other.html.contains("<h2>"));
    assert_eq!(fs::read(ws.root.join("paper.json")).unwrap(), before);
    assert_eq!(fs::read(&source).unwrap(), pdf_before);
}

#[test]
fn italic_pdf_labels_recover_nested_paragraphs_links_and_notes_without_reimport() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("labels.pdf");
    support::italic_labels_pdf_fixture(&source);
    let pdf_before = fs::read(&source).unwrap();
    let lib = Library::open(temp.path().join("library")).unwrap();
    let ws = lib.import_pdf(&source, None).unwrap();
    let before = fs::read(ws.root.join("paper.json")).unwrap();
    let state = easyread_gpui::model::ReaderState::from_value(ws.state().unwrap()).unwrap();
    let block = state
        .paper
        .blocks
        .iter()
        .find(|b| b.source_kind == "pdf-text")
        .unwrap();
    let presentation = easyread_gpui::pdf::code_presentation(&source, 1, &block.en)
        .unwrap()
        .unwrap();
    assert!(
        presentation
            .inline_figures
            .contains(&"pdf-p001-img001".to_owned()),
        "Legacy background crop is hidden"
    );
    let html = presentation.html;
    assert!(
        !html.contains("<img "),
        "A background frame must not rasterize the text"
    );
    assert!(
        html.contains("</em></p>\n<p><span>The agent retrieves"),
        "{html}"
    );
    assert!(html.contains("<p><em>A) You hand the agent the file (for example, a spreadsheet)</em></p>\n<p><span>A simple tool"), "{html}");
    assert!(html.contains("https://example.org/data"));
    assert!(!html.contains("<em>2."));
    for label in ["The data", "Tool-use data", "Software engineering tasks"] {
        assert!(
            html.contains(&format!("<p><em>{label}</em></p>\n<p>")),
            "{html}"
        );
    }
    assert!(html.contains("begins and wraps without punctuation</span></p><p><span>Second paragraph begins and also wraps"), "{html}");
    assert!(
        html.contains("explanation begins and continues on this line"),
        "{html}"
    );
    let paragraphs: Vec<_> = html
        .split("</p>")
        .map(easyread_gpui::annotations::plain_text)
        .collect();
    for address in ["support@example.org", "https://example.org/contact"] {
        assert!(
            paragraphs.iter().any(|p| p.trim() == address),
            "Separate italic contact line: {html}"
        );
    }
    let marked = easyread_gpui::annotations::decorate_interactive(
        &html,
        &[
            json!({"id":"label-note", "anchor":block.id, "quote":"file (for example", "kind":"highlight", "style":"underline", "color":"pink"}),
        ],
    );
    assert!(marked.contains("label-note"));
    assert!(marked.contains("<em>"));
    assert!(marked.contains("https://example.org/data"));
    let normalize = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    assert_eq!(
        normalize(&easyread_gpui::annotations::plain_text(&marked)),
        normalize(&block.en)
    );
    assert!(
        easyread_gpui::pdf::code_presentation(&source, 1, "user edit")
            .unwrap()
            .is_none()
    );
    assert_eq!(fs::read(ws.root.join("paper.json")).unwrap(), before);
    assert_eq!(fs::read(&source).unwrap(), pdf_before);
}

#[test]
fn code_raster_glyph_stays_inline_while_separate_small_image_remains_a_figure() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("code-image.pdf");
    support::inline_code_image_fixture(&source);
    let lib = Library::open(temp.path().join("library")).unwrap();
    let ws = lib.import_pdf(&source, None).unwrap();
    let state = ws.state().unwrap();
    let blocks = state["paper"]["blocks"].as_array().unwrap();
    let text = blocks
        .iter()
        .find(|b| b["source_kind"] == "pdf-text")
        .unwrap()["en"]
        .as_str()
        .unwrap();
    let presentation = easyread_gpui::pdf::code_presentation(&source, 1, text)
        .unwrap()
        .unwrap();
    assert_eq!(presentation.html.matches("<img ").count(), 1);
    assert_eq!(presentation.inline_figures.len(), 1);
    assert!(presentation.html.contains("alt=\"\"><span"));
    let figures: Vec<_> = blocks
        .iter()
        .filter(|b| b["source_kind"] == "pdf-image")
        .collect();
    assert_eq!(
        figures.len(),
        1,
        "a small raster outside the code line must survive"
    );
    assert!(
        !presentation
            .inline_figures
            .contains(&figures[0]["id"].as_str().unwrap().to_owned())
    );
    assert!(
        easyread_gpui::pdf::code_presentation(&source, 1, "edited source")
            .unwrap()
            .is_none()
    );
    let media =
        easyread_gpui::pdf::media(&source, &ws.root, &AtomicBool::new(false), |_, _| {}).unwrap();
    assert_eq!(media.blocks.len(), 1);
    assert_eq!(media.blocks[0]["id"], figures[0]["id"]);
}

#[test]
fn prose_raster_glyphs_reflow_inline_at_line_edges_without_rewriting_anchors() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("prose-images.pdf");
    support::inline_prose_image_fixture(&source);
    let pdf_before = fs::read(&source).unwrap();
    let lib = Library::open(temp.path().join("library")).unwrap();
    let ws = lib.import_pdf(&source, None).unwrap();
    let before: Vec<_> = ["paper", "reader", "layout"]
        .iter()
        .map(|name| fs::read(ws.root.join(format!("{name}.json"))).unwrap())
        .collect();
    let state = ws.state().unwrap();
    let blocks = state["paper"]["blocks"].as_array().unwrap();
    let text = blocks
        .iter()
        .find(|b| b["source_kind"] == "pdf-text")
        .unwrap()["en"]
        .as_str()
        .unwrap();
    let p = easyread_gpui::pdf::code_presentation(&source, 1, text)
        .unwrap()
        .unwrap();
    assert_eq!(p.inline_figures.len(), 3);
    assert_eq!(p.html.matches("<img ").count(), 3);
    assert!(!p.html.contains("<pre>"));
    assert!(
        !p.html.contains("aspect-ratio:"),
        "No standalone image in prose"
    );
    assert!(p.html.find("A ").unwrap() < p.html.find("<img ").unwrap());
    assert!(p.html.find("<img ").unwrap() < p.html.find("B celebrates.").unwrap());
    assert!(p.html.contains("</span>Start of a line."), "{}", p.html);
    assert!(p.html.contains("End!<span><img"), "{}", p.html);
    let normalize = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    assert_eq!(
        normalize(text),
        normalize(&easyread_gpui::annotations::plain_text(&p.html))
    );
    let marked = easyread_gpui::annotations::decorate_interactive(
        &p.html,
        &[
            json!({"id":"cross-image", "quote":"A B celebrates.", "kind":"underline", "color":"pink"}),
        ],
    );
    assert!(marked.contains("cross-image"));
    assert_eq!(marked.matches("<img ").count(), 3);
    assert_eq!(
        normalize(text),
        normalize(&easyread_gpui::annotations::plain_text(&marked))
    );
    let figures: Vec<_> = blocks
        .iter()
        .filter(|b| b["source_kind"] == "pdf-image")
        .collect();
    assert_eq!(figures.len(), 1, "Keep an isolated small raster");
    let media =
        easyread_gpui::pdf::media(&source, &ws.root, &AtomicBool::new(false), |_, _| {}).unwrap();
    assert_eq!(media.blocks.len(), 1);
    assert_eq!(media.blocks[0]["id"], figures[0]["id"]);
    assert!(
        easyread_gpui::pdf::code_presentation(&source, 1, "edited source")
            .unwrap()
            .is_none()
    );
    for (name, bytes) in ["paper", "reader", "layout"].iter().zip(before) {
        assert_eq!(
            fs::read(ws.root.join(format!("{name}.json"))).unwrap(),
            bytes
        );
    }
    assert_eq!(fs::read(&source).unwrap(), pdf_before);
}

#[test]
fn pdf_outline_accepts_local_destinations_and_ignores_external_actions_without_writing() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("outline.pdf");
    support::pdf_fixture_with_outline(&path, 0, false, true);
    let before = fs::read(&path).unwrap();
    assert_eq!(
        easyread_gpui::pdf::outline(&path).unwrap(),
        vec![
            easyread_gpui::pdf::OutlineEntry {
                page: 1,
                title: "First chapter".to_owned(),
                depth: 0
            },
            easyread_gpui::pdf::OutlineEntry {
                page: 1,
                title: "Second chapter".to_owned(),
                depth: 1
            }
        ]
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    pdf_fixture(&path, 0, false);
    assert!(easyread_gpui::pdf::outline(&path).unwrap().is_empty());
    assert!(easyread_gpui::pdf::outline(&temp.path().join("missing.pdf")).is_err());
}
