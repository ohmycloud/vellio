use easyread_gpui::{
    ai, formula,
    model::text_hash,
    store::{Library, apply_ops, empty_reader, write_json},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::{Arc, atomic::AtomicBool},
    thread,
};

#[path = "support/math.rs"]
mod math_fixtures;
mod support;
use support::pdf_fixture;

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
fn border_only_continued_notes_and_narrow_captions_stay_text_without_losing_charts() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("border-notes.pdf");
    support::border_callouts_pdf_fixture(&source);
    let pdf_before = fs::read(&source).unwrap();
    let lib = Library::open(temp.path().join("library")).unwrap();
    let ws = lib.import_pdf(&source, None).unwrap();
    let before: Vec<_> = ["paper", "reader", "layout"]
        .iter()
        .map(|n| fs::read(ws.root.join(format!("{n}.json"))).unwrap())
        .collect();
    let raw = ws.state().unwrap();
    let blocks = raw["paper"]["blocks"].as_array().unwrap();
    for page in [1, 2] {
        assert_eq!(
            blocks
                .iter()
                .filter(|b| b["page"] == page && b["source_kind"] == "pdf-image")
                .count(),
            1,
            "Only real artwork on page {page}"
        );
        let text = blocks
            .iter()
            .find(|b| b["page"] == page && b["source_kind"] == "pdf-text")
            .unwrap()["en"]
            .as_str()
            .unwrap();
        let p = easyread_gpui::pdf::code_presentation(&source, page, text)
            .unwrap()
            .unwrap();
        assert_eq!(p.figures.len(), 1);
        assert_eq!(p.html.matches("<img ").count(), 1);
        let visible = easyread_gpui::annotations::plain_text(&p.html);
        let norm = |v: &str| v.split_whitespace().collect::<Vec<_>>().join(" ");
        if page == 1 {
            assert!(
                p.html.matches("<blockquote>").count() == 3,
                "Full, short and clipped callouts"
            );
            assert!(
                visible.contains("A short reminder remains readable text.")
                    && visible.contains("IMPORTANT")
            );
            assert_eq!(norm(&visible), norm(text));
            assert!(p.html.contains("<em>Figure 1. A narrow caption</em>"));
            assert!(
                p.html.contains("NOTE") && p.html.contains("Multiple tools and argument formats")
            );
            assert!(p.html.contains("https://example.org/quality"));
            assert!(
                p.figures[0].height <= 240,
                "Caption must stay outside crop: {}",
                p.figures[0].height
            );
            assert!(
                p.inline_figures.contains(&"pdf-p001-img003".into()),
                "Consume the old note crop"
            );
        } else {
            assert!(p.html.contains("<blockquote>") && p.html.contains("</blockquote><p>"));
            assert!(
                p.html.contains("get_readme()</mark></code>")
                    && p.html.contains("<mark color=\"#ffff00\">"),
                "Native code and source highlight"
            );
            assert!(
                visible.contains("Schemas can change")
                    && visible.contains("Following ordinary body")
            );
            assert!(p.inline_figures.contains(&"pdf-p002-img001".into()));
            assert!(
                p.figures[0].id == "pdf-p002-img011",
                "Keep genuine chart frame: {}",
                p.figures[0].id
            );
        }
        let quote = if page == 1 {
            "Quality data for tool calling"
        } else {
            "Schemas can change"
        };
        let decorated = easyread_gpui::annotations::decorate_interactive(
            &p.html,
            &[json!({"id":"callout-note","quote":quote,"kind":"highlight","color":"blue"})],
        );
        assert!(decorated.contains("callout-note"));
        assert_eq!(easyread_gpui::annotations::plain_text(&decorated), visible);
    }
    assert_eq!(fs::read(&source).unwrap(), pdf_before);
    for (n, bytes) in ["paper", "reader", "layout"].iter().zip(before) {
        assert_eq!(fs::read(ws.root.join(format!("{n}.json"))).unwrap(), bytes);
    }
}

#[test]
fn wrapped_pdf_captions_keep_their_last_word_and_following_body_separate() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("captions.pdf");
    support::wrapped_captions_fixture(&source);
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
    let presentation = easyread_gpui::pdf::code_presentation(&source, 1, text)
        .unwrap()
        .unwrap();
    let html = presentation.html;
    assert!(
        html.contains("<p style=\"text-align:center\"><em>Figure 4-1. Coding agents solve complex problems</em></p><p>"),
        "{html}"
    );
    assert!(
        html.contains("<p><em>Table 4-2. Small &amp; large examples.</em></p><p>"),
        "{html}"
    );
    assert_eq!(presentation.inline_figures.len(), 2);
    let marked = easyread_gpui::annotations::decorate_interactive(
        &html,
        &[json!({
            "id":"wrapped-caption", "quote":"complex problems", "kind":"highlight", "style":"underline", "color":"pink"
        })],
    );
    assert!(marked.contains("wrapped-caption"));
    assert!(marked.contains("text-decoration-color:#cc413a"));
    let normalize = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    assert_eq!(
        normalize(text),
        normalize(&easyread_gpui::annotations::plain_text(&marked))
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

#[test]
fn pdf_figures_follow_source_paragraphs_and_links_keep_local_destinations() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("linked-figures.pdf");
    support::linked_figures_fixture(&source);
    let lib = Library::open(temp.path().join("library")).unwrap();
    let ws = lib.import_pdf(&source, None).unwrap();
    let before: Vec<_> = ["paper", "reader", "layout"]
        .iter()
        .map(|name| fs::read(ws.root.join(format!("{name}.json"))).unwrap())
        .collect();
    let raw = ws.state().unwrap();
    let blocks = raw["paper"]["blocks"].as_array().unwrap();
    let source_text = blocks
        .iter()
        .find(|b| b["source_kind"] == "pdf-text")
        .unwrap()["en"]
        .as_str()
        .unwrap();
    let p = easyread_gpui::pdf::code_presentation(&source, 1, source_text)
        .unwrap()
        .unwrap();
    let h = &p.html;
    assert_eq!(h.matches("<img ").count(), 2);
    assert_eq!(h.matches("style=\"aspect-ratio:").count(), 2);
    assert_eq!(p.inline_figures.len(), 2);
    let first = h.find("<img ").unwrap();
    let second = h[first + 1..].find("<img ").unwrap() + first + 1;
    assert!(h.find("across calls.").unwrap() < first);
    assert!(
        first
            < h.find("<p style=\"text-align:center\"><em>Figure 4-1.")
                .unwrap()
    );
    assert!(h.find("Without memory.").unwrap() < h.find("Over the years").unwrap());
    assert!(h.find("follows this paragraph.").unwrap() < second);
    assert!(
        second
            < h.find("<p style=\"text-align:center\"><em>Figure 4-2.")
                .unwrap()
    );
    assert!(h.find("With memory.").unwrap() < h.rfind("<p>").unwrap());
    assert!(h.contains("href=\"easyread-pdf:1:680\""));
    assert!(h.contains("href=\"easyread-pdf:1:450\""));
    assert!(h.contains("https://example.invalid/docs?a=1&amp;b=2"));
    assert!(h.contains("color:#8e0012"));
    let normalize = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    assert_eq!(
        normalize(source_text),
        normalize(&easyread_gpui::annotations::plain_text(h))
    );
    for (url, quote) in [
        ("easyread-pdf:1:680", "Figure 4-1. Without memory."),
        ("easyread-pdf:1:450", "Figure 4-2. With memory."),
    ] {
        assert_eq!(
            easyread_gpui::pdf::link_destination(&source, url).unwrap(),
            (1, quote.into())
        );
    }
    assert!(easyread_gpui::pdf::link_destination(&source, "easyread-pdf:0:0").is_err());
    assert!(easyread_gpui::pdf::link_destination(&source, "easyread-pdf:1:NaN").is_err());
    let marked = easyread_gpui::annotations::decorate_interactive(
        h,
        &[
            json!({"id":"link-note", "anchor":"pdf-p001", "quote":"Figure 4-1", "kind":"underline", "color":"pink"}),
        ],
    );
    assert!(marked.contains("easyread-pdf:1:680"));
    assert_eq!(
        normalize(&easyread_gpui::annotations::plain_text(&marked)),
        normalize(source_text)
    );
    for (name, before) in ["paper", "reader", "layout"].iter().zip(before) {
        assert_eq!(
            fs::read(ws.root.join(format!("{name}.json"))).unwrap(),
            before
        );
    }
    // Also works for an older text-only import without completing stored media.
    fs::remove_dir_all(ws.root.join("figures")).unwrap();
    let restored = easyread_gpui::pdf::code_presentation(&source, 1, source_text)
        .unwrap()
        .unwrap();
    assert_eq!(restored.html, p.html);
    assert!(
        easyread_gpui::pdf::code_presentation(&source, 1, "edited text")
            .unwrap()
            .is_none()
    );
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
fn fragmented_pdf_caption_uses_physical_lines_and_word_gaps_without_reimport() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("fragmented-caption.pdf");
    fs::copy("fixtures/pdf-fragmented-caption.pdf", &source).unwrap();
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
    assert!(
        text.contains("F\ni\ng"),
        "The fixture must reproduce per-letter extraction: {text}"
    );
    let p = easyread_gpui::pdf::code_presentation(&source, 1, text)
        .unwrap()
        .unwrap();
    let caption = "Figure 2-19. Each token flows through its own track in the stack, with only the final token’s track passed to the LM head to predict the next token";
    assert!(
        p.html.contains(&format!(
            "<p style=\"text-align:center\"><em>{caption}</em></p>"
        )),
        "{}",
        p.html
    );
    assert_eq!(p.html.matches("<img ").count(), 0);
    let letters = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    assert_eq!(
        letters(text),
        letters(&easyread_gpui::annotations::plain_text(&p.html))
    );
    let decorated = easyread_gpui::annotations::decorate_interactive(
        &p.html,
        &[
            json!({"id":"caption-note", "quote":"final token’s track", "style":"underline", "color":"pink"}),
        ],
    );
    assert!(decorated.contains("caption-note"));
    assert!(decorated.contains("text-decoration-color:#cc413a"));
    assert_eq!(
        easyread_gpui::annotations::plain_text(&decorated),
        easyread_gpui::annotations::plain_text(&p.html)
    );
    assert!(
        easyread_gpui::pdf::code_presentation(&source, 1, "user edit")
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
fn home_catalog_reads_native_progress_and_preserves_source_files() {
    let temp = tempfile::tempdir().unwrap();
    let lib = Library::open(temp.path().join("library")).unwrap();
    let root = lib.root.join("book-home");
    fs::create_dir_all(&root).unwrap();
    let paper = json!({"schema":2,"meta":{"title_en":"Authorization", "authors":"Ada", "abstract":"Access control"},
        "blocks":[{"id":"a","type":"para","en":"First"},{"id":"b","type":"para","en":"Second"}],"unknown":42});
    write_json(&root.join("paper.json"), &paper).unwrap();
    let reader = json!({"schema":2,"progress":{"block":"a","at":"2026-10-01T00:00:00Z"},"edits":{"a":{"zh":"保留"}},"notes":{"n1":{},"n2":{"deleted":true}}});
    write_json(&root.join("reader.json"), &reader).unwrap();
    write_json(
        &root.join("item.json"),
        &json!({"status":"reading","custom":"keep"}),
    )
    .unwrap();
    write_json(
        &root.join(".native-reader.json"),
        &json!({"progress":{"block":"b","offset":25,"at":"2026-10-07T00:00:00Z"}}),
    )
    .unwrap();
    let before = fs::read(root.join("paper.json")).unwrap();
    let listed = lib.list().unwrap();
    assert_eq!(listed[0]["ratio"], 1.0);
    assert_eq!(listed[0]["notes"], 1);
    assert!(easyread_gpui::library_home::matches(
        &listed[0],
        "Ada access",
        &[]
    ));
    let ws = lib.workspace("book-home").unwrap();
    ws.set_reading_status("unread").unwrap();
    ws.update("item", |item| {
        item["starred"] = json!(true);
        Ok(())
    })
    .unwrap();
    let listed = lib.list().unwrap();
    assert_eq!(listed[0]["status"], "unread");
    assert_eq!(listed[0]["item"]["starred"], true);
    assert_eq!(listed[0]["item"]["custom"], "keep");
    assert_eq!(fs::read(root.join("paper.json")).unwrap(), before);
    assert_eq!(ws.load("reader").unwrap(), reader);
}
#[test]
fn import_render_extract_rotation_deduplicate_and_reject_broken() {
    let temp = tempfile::tempdir().unwrap();
    let lib = Library::open(temp.path().join("library")).unwrap();
    for rotation in [0, 90, 180, 270] {
        let pdf = temp.path().join(format!("test-{rotation}.pdf"));
        pdf_fixture(&pdf, rotation, false);
        let ws = lib.import_pdf(&pdf, None).unwrap();
        let state = ws.state().unwrap();
        assert_eq!(state["paper"]["meta"]["page_count"], 1);
        assert!(
            state["paper"]["blocks"][0]["en"]
                .as_str()
                .unwrap()
                .contains("Rust PDF integration")
        );
        let page = image::open(ws.root.join("pages/page-001.webp")).unwrap();
        assert_eq!(page.width(), 1200);
        let chars: Vec<Value> =
            serde_json::from_slice(&fs::read(ws.root.join("extract/page-001.chars.json")).unwrap())
                .unwrap();
        assert!(chars.len() > 30);
        for c in &chars {
            for coordinate in c.as_array().unwrap().iter().skip(1) {
                assert!((0.0..=1.0).contains(&coordinate.as_f64().unwrap()));
            }
            assert!(c[1].as_f64() <= c[3].as_f64());
            assert!(c[2].as_f64() <= c[4].as_f64());
        }
        let duplicate = lib.import_pdf(&pdf, None).unwrap();
        assert_eq!(duplicate.id(), ws.id());
        let range = chars.iter().find(|c| c[0] == "R").unwrap();
        // First glyph must be inside the raster content area at all rotations.
        let (x0, y0, x1, y1) = (
            range[1].as_f64().unwrap(),
            range[2].as_f64().unwrap(),
            range[3].as_f64().unwrap(),
            range[4].as_f64().unwrap(),
        );
        let rgba = page.to_rgb8();
        let mut dark = 0;
        for y in (y0 * page.height() as f64) as u32
            ..((y1 * page.height() as f64).ceil() as u32).min(page.height())
        {
            for x in (x0 * page.width() as f64) as u32
                ..((x1 * page.width() as f64).ceil() as u32).min(page.width())
            {
                if rgba.get_pixel(x, y).0.iter().all(|v| *v < 100) {
                    dark += 1;
                }
            }
        }
        assert!(
            dark > 5,
            "glyph box does not cover rendered glyph at rotation {rotation}"
        );
    }
    let scan = temp.path().join("scan.pdf");
    pdf_fixture(&scan, 0, true);
    let ws = lib.import_pdf(&scan, None).unwrap();
    let scanned = ws.state().unwrap();
    assert_eq!(scanned["paper"]["blocks"][0]["source_kind"], "pdf-page");
    assert_eq!(scanned["paper"]["blocks"][0]["src"], "pages/page-001.webp");
    assert_eq!(scanned["paper"]["blocks"][0]["caption_zh"], "");
    assert_eq!(scanned["paper"]["blocks"][0]["caption_en"], "");
    let broken = temp.path().join("broken.pdf");
    fs::write(&broken, b"%PDF-not-valid").unwrap();
    assert!(lib.import_pdf(&broken, None).is_err());
    assert_eq!(lib.list().unwrap().len(), 5);
}
#[test]
fn operations_are_transactional_monotonic_and_replay_safe() {
    let mut reader = empty_reader();
    reader["future_field"] = json!({"keep":true});
    let ops = json!([{"op":"edit","block":"b","zh":"用户🦀","base":text_hash("原稿"),"at":"2026-10-03T00:00:00.000Z"},{"op":"note","note":{"id":"n","anchor":"b","quote":"用户","updated":"2026-10-03T00:00:00.000Z","future":5}},{"op":"progress","block":"b","ratio":0.5,"at":"2026-10-03T00:00:00.000Z"}]);
    let ops = ops.as_array().unwrap();
    assert_eq!(apply_ops(&mut reader, ops).unwrap().len(), 3);
    let expected = reader.clone();
    assert!(apply_ops(&mut reader, ops).unwrap().is_empty());
    assert_eq!(reader, expected);
    assert!(
        apply_ops(
            &mut reader,
            &[json!({"op":"edit","block":"b","zh":"older","at":"2020"})]
        )
        .unwrap()
        .is_empty()
    );
    assert!(
        apply_ops(
            &mut reader,
            &[
                json!({"op":"edit","block":"b","zh":"next","at":"2030"}),
                json!({"op":"bad"})
            ]
        )
        .is_err()
    );
    assert_eq!(reader, expected);
    apply_ops(
        &mut reader,
        &[
            json!({"op":"edit","block":"b","zh":null,"at":"2031"}),
            json!({"op":"note_del","id":"n","at":"2031"}),
        ],
    )
    .unwrap();
    assert_eq!(reader["edits"]["b"]["prev"], "用户🦀");
    assert_eq!(reader["notes"]["n"]["deleted"], true);
    assert_eq!(reader["future_field"]["keep"], true);
}
#[test]
fn legacy_copy_preserves_unknown_fields_and_reader_ownership() {
    let temp = tempfile::tempdir().unwrap();
    let old = temp.path().join("old");
    fs::create_dir(&old).unwrap();
    pdf_fixture(&old.join("source.pdf"), 0, false);
    let envelope: Value = serde_json::from_str(include_str!("../fixtures/state.json")).unwrap();
    let mut paper = envelope["paper"].clone();
    paper["future"] = json!({"preserve":42});
    write_json(&old.join("paper.json"), &paper).unwrap();
    write_json(&old.join("reader.json"), &envelope["reader"]).unwrap();
    let before = fs::read(old.join("reader.json")).unwrap();
    let lib = Library::open(temp.path().join("new")).unwrap();
    let ws = lib.migrate_copy(&old).unwrap();
    ws.reader_ops(&[json!({"op":"paper_note","body":"Rust note","at":"2030"})])
        .unwrap();
    assert_eq!(fs::read(old.join("reader.json")).unwrap(), before);
    assert_eq!(ws.load("paper").unwrap()["future"], json!({"preserve":42}));
    assert!(lib.migrate_copy(&old).is_err());
    let invalid = temp.path().join("invalid-old");
    fs::create_dir(&invalid).unwrap();
    pdf_fixture(&invalid.join("source.pdf"), 90, false);
    write_json(&invalid.join("paper.json"), &paper).unwrap();
    write_json(&invalid.join("reader.json"), &json!({"edits":[]})).unwrap();
    assert!(lib.migrate_copy(&invalid).is_err());
    assert_eq!(lib.list().unwrap().len(), 1);
    assert!(ws.asset("../old/source.pdf").is_err());
    assert!(lib.workspace("../../old").is_err());
}
#[test]
fn pdf_math_numbers_and_adjacent_section_titles_keep_complete_native_objects() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("math-numbers.pdf");
    math_fixtures::numbers_fixture(&source);
    let ws = Library::open(temp.path().join("library"))
        .unwrap()
        .import_pdf(&source, None)
        .unwrap();
    let names = ["source.pdf", "paper.json", "reader.json", "layout.json"];
    let before: Vec<_> = names
        .iter()
        .map(|name| fs::read(ws.root.join(name)).unwrap())
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
    let tex: Vec<_> = p.math.iter().map(|m| m.tex.replace(' ', "")).collect();
    assert_eq!(
        tex,
        [
            r"\epsilon_{ls}=0.1",
            "28.4",
            "3.5",
            "1/4",
            r"1.0\cdot10^{20}"
        ],
        "{}",
        p.html
    );
    assert!(p.math.iter().all(|m| !m.display && m.layout.is_valid()));
    assert_eq!(p.html.matches("data-vellio-math").count(), 5);
    assert!(!p.html.contains("<img") && !p.html.contains("data:image"));
    let heading = p.html.find("<h3>6 Results</h3>").expect(&p.html);
    assert!(p.html[..heading].ends_with("</p>"), "{}", p.html);
    assert!(
        p.html[heading..].starts_with("<h3>6 Results</h3>\n<h3>6.1 Machine Translation</h3>\n<p>")
    );
    let plain = easyread_gpui::annotations::plain_text(&p.html);
    assert!(plain.contains("Ordinary prose numbers 0.1 and 28.4 remain ordinary text."));
    let marked = easyread_gpui::annotations::decorate_interactive(
        &p.html,
        &[
            json!({"id":"numeric-note","quote":"improves accuracy and BLEU score.","kind":"underline","color":"blue"}),
        ],
    );
    assert!(marked.contains("numeric-note"));
    assert_eq!(easyread_gpui::annotations::plain_text(&marked), plain);
    let cache = easyread_gpui::pdf_layout_cache::PresentationCache::new(
        &source,
        &temp.path().join("cache"),
    )
    .unwrap();
    for _ in 0..2 {
        let cached = cache
            .prepare(&source, &[(1, text.to_owned())])
            .remove(0)
            .unwrap()
            .unwrap();
        assert_eq!(cached.html, p.html);
        assert_eq!(cached.math.len(), p.math.len());
        assert!(
            cached
                .math
                .iter()
                .all(|m| easyread_gpui::formula::cached_layout(&m.tex, m.display).is_some())
        );
    }
    assert!(
        easyread_gpui::pdf::code_presentation(&source, 1, "user-edited source")
            .unwrap()
            .is_none()
    );
    for (name, bytes) in names.iter().zip(before) {
        assert_eq!(fs::read(ws.root.join(name)).unwrap(), bytes, "{name}");
    }
}

#[test]
fn pdf_math_reconstructs_tex_for_native_typst_and_preserves_surrounding_text() {
    let temp = tempfile::tempdir().unwrap();
    for mode in [
        "display",
        "only",
        "unnumbered",
        "inline",
        "inline-scripts",
        "code",
        "reference",
        "adjacent",
    ] {
        let source = temp.path().join(format!("math-{mode}.pdf"));
        math_fixtures::fixture(&source, mode);
        let ws = Library::open(temp.path().join(format!("library-{mode}")))
            .unwrap()
            .import_pdf(&source, None)
            .unwrap();
        let before: Vec<_> = ["source.pdf", "paper.json", "reader.json", "layout.json"]
            .iter()
            .map(|name| fs::read(ws.root.join(name)).unwrap())
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
        let recovered = matches!(mode, "display" | "only" | "unnumbered" | "adjacent");
        assert_eq!(
            p.math.iter().filter(|m| m.display).count(),
            usize::from(recovered),
            "{mode}: {:?}",
            p.math.iter().map(|m| &m.tex).collect::<Vec<_>>()
        );
        assert!(
            !p.html.contains("<img")
                && !p.html.contains("data:image")
                && !p.html.contains("easyread-image:"),
            "Formulas are not images"
        );
        if recovered {
            let equation = p.math.iter().find(|m| m.display).unwrap();
            assert!(
                equation.tex.contains("\\frac{QK^{T}}{\\sqrt{d_{k}}}"),
                "{}",
                equation.tex
            );
            assert_eq!(
                equation.number.as_deref(),
                if mode == "unnumbered" {
                    None
                } else {
                    Some("1")
                }
            );
            assert!(p.html.contains("data-vellio-equation") && p.html.contains("data-vellio-math"));
            assert_eq!(p.html.matches("data-vellio-equation").count(), 1);
            assert!(!p.html.contains("<span>f(x)"), "no flattened duplicate");
            let layout = &equation.layout;
            assert!(layout.height > 20. && !layout.paths.is_empty());
            let bar = layout
                .paths
                .iter()
                .find(|p| p.stroke.is_some())
                .expect("Typst fraction rule");
            assert!(
                bar.commands
                    .iter()
                    .any(|c| matches!(c, easyread_gpui::formula::PathCommand::Line(_)))
            );
            assert!(layout.baseline > 0. && layout.baseline <= layout.height);
            assert!(
                easyread_gpui::annotations::plain_text(&p.html).contains("\\frac"),
                "select/copy the TeX, not flattened glyphs"
            );
            let cache = easyread_gpui::pdf_layout_cache::PresentationCache::new(
                &source,
                &temp.path().join(format!("cache-{mode}")),
            )
            .unwrap();
            for _ in 0..2 {
                let cached = cache
                    .prepare(&source, &[(1, text.to_owned())])
                    .remove(0)
                    .unwrap()
                    .unwrap();
                assert_eq!(cached.html, p.html);
                let cached = cached.math.iter().find(|m| m.display).unwrap();
                assert_eq!(cached.tex, equation.tex);
                assert_eq!(cached.layout.paths.len(), equation.layout.paths.len());
            }
        }
        if matches!(mode, "code" | "reference") {
            assert!(p.math.is_empty(), "{mode} must remain literal");
        }
        if mode == "inline" {
            assert!(
                p.math
                    .iter()
                    .any(|m| !m.display && m.tex == "\\frac{QK^{T}}{\\sqrt{d_{k}}}"),
                "{:?}",
                p.math.iter().map(|m| &m.tex).collect::<Vec<_>>()
            );
            assert!(
                easyread_gpui::annotations::plain_text(&p.html).contains("within ordinary prose.")
            );
        }
        if mode == "inline-scripts" {
            assert_eq!(p.math.len(), 2);
            assert!(p.math.iter().any(|m| m.tex == "Q^{T}"));
            assert!(p.math.iter().any(|m| m.tex == "d_{k}"));
            assert_eq!(p.html.matches("data-vellio-math").count(), 2);
            assert!(
                easyread_gpui::annotations::plain_text(&p.html).contains("ordinary d and k"),
                "never replace unrelated letters"
            );
        }
        if mode != "only" {
            let plain = easyread_gpui::annotations::plain_text(&p.html);
            assert!(plain.contains("Body before the formula."));
            assert!(plain.contains("Body after the formula."));
            let marked = easyread_gpui::annotations::decorate_interactive(
                &p.html,
                &[
                    json!({"id":"after-math", "quote":"Body after the formula.", "kind":"underline", "color":"blue"}),
                ],
            );
            assert!(marked.contains("after-math"));
            assert_eq!(easyread_gpui::annotations::plain_text(&marked), plain);
        }
        if mode == "adjacent" {
            assert!(
                easyread_gpui::annotations::plain_text(&p.html).contains("Nearby body stays text.")
            );
        }
        assert!(
            easyread_gpui::pdf::code_presentation(&source, 1, "edited source")
                .unwrap()
                .is_none()
        );
        for (name, bytes) in ["source.pdf", "paper.json", "reader.json", "layout.json"]
            .iter()
            .zip(before)
        {
            assert_eq!(
                fs::read(ws.root.join(name)).unwrap(),
                bytes,
                "{mode}: {name}"
            );
        }
    }
}

#[test]
fn native_formula_corpus_and_invalid_input() {
    for tex in [
        r"e^{i\pi}+1=0",
        r"\mathcal{L}(\theta)=\sum_{i=1}^{n}\log p_\theta(x_i)",
        r"\frac{\partial}{\partial x}\sum_{i=1}^n x_i^2",
        r"\begin{pmatrix}a&b\\c&d\end{pmatrix}",
        r"f(x)=\begin{cases}x^2&x\ge 0\\-x&x<0\end{cases}",
        r"\begin{aligned}a&=b+c\\d&=e+f\end{aligned}",
    ] {
        let svg = formula::svg(tex, true).unwrap_or_else(|e| panic!("{tex}: {e:#}"));
        assert!(svg.contains("<svg"));
        assert!(svg.len() > 500);
        for display in [false, true] {
            let layout =
                formula::layout(tex, display).unwrap_or_else(|e| panic!("native {tex}: {e:#}"));
            assert!(layout.is_valid());
            assert!(layout.width > 10. && layout.height > 10.);
            assert!(
                layout
                    .paths
                    .iter()
                    .any(|p| p.commands.iter().any(|c| matches!(
                        c,
                        formula::PathCommand::Quad(..) | formula::PathCommand::Cubic(..)
                    )))
            );
            assert!(std::sync::Arc::ptr_eq(
                &layout,
                &formula::cached_layout(tex, display).unwrap()
            ));
            let json = serde_json::to_string(&*layout).unwrap();
            assert!(
                !json.contains("<svg") && !json.contains("data:image") && !json.contains("png")
            );
            let restored: formula::FormulaLayout = serde_json::from_str(&json).unwrap();
            assert!(restored.is_valid());
            assert_eq!(restored.paths.len(), layout.paths.len());
        }
    }
    assert!(formula::layout("", true).is_err());
    assert!(formula::layout(r"\nonexistentcommand{x}", false).is_err());
    assert!(formula::svg("", true).is_err());
    assert!(formula::svg(r"\nonexistentcommand{x}", true).is_err());
}
#[test]
fn sse_unicode_multiline_done_error_and_cancel() {
    let input = "data: {\"choices\":[{\"delta\":{\"content\":\"中文🦀\"}}]}\r\n\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"继续\"},\n data-ignored\ndata: \"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
    let mut deltas = String::new();
    assert_eq!(
        ai::decode_sse(input.as_bytes(), &AtomicBool::new(false), |s| deltas
            .push_str(s))
        .unwrap(),
        "中文🦀继续"
    );
    assert_eq!(deltas, "中文🦀继续");
    assert!(
        ai::decode_sse(
            b"data: {\"error\":{}}\n\n".as_slice(),
            &AtomicBool::new(false),
            |_| {}
        )
        .is_err()
    );
    assert!(
        ai::decode_sse(
            b"data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n".as_slice(),
            &AtomicBool::new(false),
            |_| {}
        )
        .is_err()
    );
    assert!(
        ai::decode_sse(
            b"data: [DONE]\n\n".as_slice(),
            &AtomicBool::new(true),
            |_| {}
        )
        .is_err()
    );
}
#[test]
fn actual_http_ai_transport_and_thread_persistence() {
    let temp = tempfile::tempdir().unwrap();
    let pdf = temp.path().join("test.pdf");
    pdf_fixture(&pdf, 0, false);
    let ws = Library::open(temp.path().join("lib"))
        .unwrap()
        .import_pdf(&pdf, None)
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        let mut reader = BufReader::new(socket.try_clone().unwrap());
        let mut line = String::new();
        let mut length = 0;
        loop {
            line.clear();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
            if line.to_ascii_lowercase().starts_with("content-length:") {
                length = line
                    .split(':')
                    .nth(1)
                    .unwrap()
                    .trim()
                    .parse::<usize>()
                    .unwrap();
            }
        }
        let mut body = vec![0; length];
        std::io::Read::read_exact(&mut reader, &mut body).unwrap();
        let request: Value = serde_json::from_slice(&body).unwrap();
        assert!(request["stream"].as_bool().unwrap());
        assert!(
            request["messages"][0]["content"]
                .as_str()
                .unwrap()
                .contains("Rust PDF integration")
        );
        let data =
            "data: {\"choices\":[{\"delta\":{\"content\":\"验证回答🦀\"}}]}\n\ndata: [DONE]\n\n";
        write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",data.len(),data).unwrap();
    });
    let config = ai::Config {
        base_url: format!("http://127.0.0.1:{port}/v1"),
        model: "explicit-mock".into(),
        api_key: "test-only".into(),
    };
    let answer = ai::ask(
        &ws,
        "t-test",
        "解释论文",
        Some("pdf-p001"),
        &config,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    server.join().unwrap();
    assert_eq!(answer, "验证回答🦀");
    let chat = ws.load("chat").unwrap();
    assert_eq!(chat["threads"][0]["messages"].as_array().unwrap().len(), 2);
    assert_eq!(chat["threads"][0]["messages"][1]["status"], "done");
}
#[test]
fn concurrent_reader_updates_do_not_lose_notes() {
    let temp = tempfile::tempdir().unwrap();
    let pdf = temp.path().join("test.pdf");
    pdf_fixture(&pdf, 0, false);
    let ws = Arc::new(
        Library::open(temp.path().join("lib"))
            .unwrap()
            .import_pdf(&pdf, None)
            .unwrap(),
    );
    let writers:Vec<_>=(0..12).map(|i|{let ws=ws.clone();thread::spawn(move||ws.reader_ops(&[json!({"op":"note","note":{"id":format!("n{i}"),"updated":"2030","body":format!("note{i}")}})]).unwrap())}).collect();
    for w in writers {
        w.join().unwrap();
    }
    assert_eq!(
        ws.load("reader").unwrap()["notes"]
            .as_object()
            .unwrap()
            .len(),
        12
    );
}

#[test]
fn inline_formula_scanner_preserves_code_and_escaped_dollars() {
    let text = r"中文 $e^{i\pi}+1=0$ and `code $x$` and \$cash and $$x^2$$ and $\frac{1}{2}$";
    let ranges = formula::inline_math(text);
    assert_eq!(
        ranges.iter().map(|(_, t)| *t).collect::<Vec<_>>(),
        vec![r"e^{i\pi}+1=0", r"\frac{1}{2}"]
    );
    let transformed = formula::native_markdown_html(text);
    assert_eq!(transformed.matches("data-vellio-math").count(), 3);
    assert!(transformed.contains("data-display=\"true\""));
    assert!(!transformed.contains("<img") && !transformed.contains("data:image"));
    assert!(transformed.contains("<code>code $x$</code>"));
    let plain = easyread_gpui::annotations::plain_text(&transformed);
    assert!(plain.contains(r"$\frac{1}{2}$") && plain.contains("$cash"));
    let malicious = formula::native_markdown_html(
        r#"Before $x<y$ <script>alert(1)</script> $\text{\"quoted\"}_k$"#,
    );
    assert!(!malicious.contains("<script>"));
    assert!(malicious.contains("x&lt;y") && malicious.contains("&quot;quoted"));
    assert!(
        formula::inline_math("```latex\n$x$\n```\n中文 $y$")
            .iter()
            .all(|(_, t)| *t == "y")
    );
}
#[test]
fn cancelled_import_never_publishes_partial_workspace() {
    let temp = tempfile::tempdir().unwrap();
    let pdf = temp.path().join("test.pdf");
    pdf_fixture(&pdf, 0, false);
    let lib = Library::open(temp.path().join("lib")).unwrap();
    assert!(
        lib.import_pdf_cancellable(&pdf, None, &AtomicBool::new(true), |_, _| {})
            .is_err()
    );
    assert!(lib.list().unwrap().is_empty());
}
#[test]
fn timestamp_comparison_handles_legacy_timezone_offsets() {
    let mut reader = empty_reader();
    apply_ops(
        &mut reader,
        &[json!({"op":"edit","block":"b","zh":"old","at":"2026-10-03T10:00:00.000+08:00"})],
    )
    .unwrap();
    apply_ops(
        &mut reader,
        &[json!({"op":"edit","block":"b","zh":"new","at":"2026-10-03T04:00:00.000Z"})],
    )
    .unwrap();
    assert_eq!(reader["edits"]["b"]["zh"], "new");
}

fn mock_responses(responses: Vec<(u16, &'static str)>) -> (ai::Config, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = thread::spawn(move || {
        for (status, body) in responses {
            let (mut socket, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            let mut line = String::new();
            let mut length = 0;
            loop {
                line.clear();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                if line.to_ascii_lowercase().starts_with("content-length:") {
                    length = line
                        .split(':')
                        .nth(1)
                        .unwrap()
                        .trim()
                        .parse::<usize>()
                        .unwrap();
                }
            }
            let mut request = vec![0; length];
            std::io::Read::read_exact(&mut reader, &mut request).unwrap();
            write!(socket,"HTTP/1.1 {status} Test\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
    });
    (
        ai::Config {
            base_url: format!("http://127.0.0.1:{port}/v1"),
            model: "explicit-mock".into(),
            api_key: "test-only".into(),
        },
        handle,
    )
}
#[test]
fn translation_partial_failure_retry_preserves_ids_and_user_edits() {
    let temp = tempfile::tempdir().unwrap();
    let pdf = temp.path().join("test.pdf");
    pdf_fixture(&pdf, 0, false);
    let ws = Library::open(temp.path().join("lib"))
        .unwrap()
        .import_pdf(&pdf, None)
        .unwrap();
    ws.update("paper",|p|{p["blocks"]=json!([{"id":"a","type":"para","page":1,"en":"First","zh":""},{"id":"b","type":"para","page":1,"en":"Second","zh":""}]);Ok(())}).unwrap();
    ws.reader_ops(&[json!({"op":"edit","block":"a","zh":"用户版本","base":"base","at":"2030"})])
        .unwrap();
    let reader = ws.load("reader").unwrap();
    let (config, server) = mock_responses(vec![
        (
            200,
            "data: {\"choices\":[{\"delta\":{\"content\":\"第一段\"}}]}\n\ndata: [DONE]\n\n",
        ),
        (500, "deliberate error"),
    ]);
    assert!(ai::translate(&ws, &config, &AtomicBool::new(false), |_, _| {}).is_err());
    server.join().unwrap();
    assert_eq!(ws.load("paper").unwrap()["blocks"][0]["zh"], "第一段");
    assert_eq!(ws.load("job").unwrap()["state"], "partial");
    assert_eq!(ws.load("reader").unwrap(), reader);
    let (config, server) = mock_responses(vec![(
        200,
        "data: {\"choices\":[{\"delta\":{\"content\":\"第二段\"}}]}\n\ndata: [DONE]\n\n",
    )]);
    ai::translate(&ws, &config, &AtomicBool::new(false), |_, _| {}).unwrap();
    server.join().unwrap();
    assert_eq!(ws.load("paper").unwrap()["blocks"][1]["id"], "b");
    assert_eq!(ws.load("paper").unwrap()["blocks"][1]["zh"], "第二段");
    assert_eq!(ws.load("reader").unwrap(), reader);
    assert_eq!(ws.load("job").unwrap()["state"], "done");
}
#[test]
fn incomplete_ai_stream_preserves_partial_answer_with_error_status() {
    let temp = tempfile::tempdir().unwrap();
    let pdf = temp.path().join("test.pdf");
    pdf_fixture(&pdf, 0, false);
    let ws = Library::open(temp.path().join("lib"))
        .unwrap()
        .import_pdf(&pdf, None)
        .unwrap();
    let (config, server) = mock_responses(vec![(
        200,
        "data: {\"choices\":[{\"delta\":{\"content\":\"部分回答\"}}]}\n\n",
    )]);
    assert!(
        ai::ask(
            &ws,
            "t-partial",
            "问题",
            None,
            &config,
            &AtomicBool::new(false),
            |_| {}
        )
        .is_err()
    );
    server.join().unwrap();
    let answer = ws.load("chat").unwrap()["threads"][0]["messages"][1].clone();
    assert_eq!(answer["content"], "部分回答");
    assert_eq!(answer["status"], "error");
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

#[test]
fn annotated_pdf_figures_have_transparent_paper_and_clickable_original_pixels() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("annotated-figure.pdf");
    support::annotated_figure_fixture(&source);
    let source_before = fs::read(&source).unwrap();
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
    assert_eq!(p.figures.len(), 2);
    for f in &p.figures {
        assert!(
            p.html
                .contains(&format!("href=\"easyread-image:{}\"", f.id))
        );
        assert!(p.inline_figures.contains(&f.id));
        let image = image::load_from_memory(&f.png).unwrap().to_rgba8();
        assert_eq!(image.dimensions(), (f.width, f.height));
    }
    let diagram = image::load_from_memory(&p.figures[0].png)
        .unwrap()
        .to_rgba8();
    let count = (diagram.width() * diagram.height()) as usize;
    assert!(
        diagram.pixels().filter(|p| p[3] == 0).count() > count / 2,
        "Unpainted page background must let the reader paper show through"
    );
    assert!(
        diagram.pixels().any(|p| p.0 == [255, 255, 255, 255]),
        "Deliberately painted white regions must remain opaque"
    );
    assert!(
        diagram
            .pixels()
            .filter(|p| p[3] > 128 && p[0] < 50 && p[1] < 50 && p[2] < 50)
            .count()
            > 100,
        "The diagram's text labels must remain readable"
    );
    let raster = image::load_from_memory(&p.figures[1].png)
        .unwrap()
        .to_rgba8();
    assert!(raster.pixels().any(|p| p.0 == [255, 0, 0, 255]));
    assert!(raster.pixels().any(|p| p.0 == [0, 255, 0, 255]));
    let marked = easyread_gpui::annotations::decorate_interactive(
        &p.html,
        &[
            json!({"id":"figure-note", "quote":"Body after the annotated diagram.", "kind":"highlight", "color":"pink"}),
        ],
    );
    assert!(marked.contains("figure-note"));
    assert_eq!(
        easyread_gpui::annotations::plain_text(&marked),
        easyread_gpui::annotations::plain_text(&p.html)
    );
    for f in &p.figures {
        assert!(marked.contains(&format!("href=\"easyread-image:{}\"", f.id)));
    }
    assert!(
        easyread_gpui::pdf::code_presentation(&source, 1, "edited source")
            .unwrap()
            .is_none()
    );
    assert_eq!(fs::read(source).unwrap(), source_before);
    for (name, bytes) in ["paper", "reader", "layout"].iter().zip(before) {
        assert_eq!(
            fs::read(ws.root.join(format!("{name}.json"))).unwrap(),
            bytes
        );
    }
}

#[test]
fn nested_clipped_forms_do_not_bake_captions_or_hidden_titles_into_figures() {
    let temp = tempfile::tempdir().unwrap();
    for rotated in [false, true] {
        let source = temp.path().join(format!("clipped-{rotated}.pdf"));
        support::clipped_caption_form_fixture(&source, rotated);
        let ws = Library::open(temp.path().join(format!("library-{rotated}")))
            .unwrap()
            .import_pdf(&source, None)
            .unwrap();
        let before: Vec<_> = ["source.pdf", "paper.json", "reader.json", "layout.json"]
            .iter()
            .map(|name| fs::read(ws.root.join(name)).unwrap())
            .collect();
        let state = ws.state().unwrap();
        let blocks = state["paper"]["blocks"].as_array().unwrap();
        let text = blocks
            .iter()
            .find(|b| b["source_kind"] == "pdf-text")
            .unwrap()["en"]
            .as_str()
            .unwrap();
        assert!(
            text.contains("Hidden diagram title"),
            "Keep original extraction intact"
        );
        let p = easyread_gpui::pdf::code_presentation(&source, 1, text)
            .unwrap()
            .unwrap();
        assert_eq!(p.figures.len(), 1);
        assert!(p.html.contains("<span>Figure 4: A selectable caption outside the clipped diagram. The final caption line remains text.</span>"), "{}", p.html);
        assert!(p.html.contains("Visible body before the diagram."));
        assert!(p.html.contains("Body after the caption."));
        assert!(!p.html.contains("Hidden diagram title"));
        assert!(
            !p.html.contains("Visible diagram label"),
            "Visible label stays inside the crop"
        );
        let f = &p.figures[0];
        assert!(
            f.height < 600,
            "No unpainted/hidden form bounds: {}",
            f.height
        );
        assert!(p.inline_figures.contains(&f.id));
        let image = blocks.iter().find(|b| b["type"] == "figure").unwrap();
        assert!(image["height"].as_u64().unwrap() < 610);
        let layout: Value =
            serde_json::from_slice(&fs::read(ws.root.join("layout.json")).unwrap()).unwrap();
        let bounds = &layout[image["id"].as_str().unwrap()]["box"];
        assert!(
            bounds[3].as_f64().unwrap() < (792. - 390.) / 792.,
            "Import crop ends above the caption: {bounds}"
        );
        let marked = easyread_gpui::annotations::decorate_interactive(
            &p.html,
            &[
                json!({"id":"clipped-caption-note", "quote":"final caption line", "kind":"highlight", "color":"pink"}),
            ],
        );
        assert!(marked.contains("clipped-caption-note"));
        for (name, original) in ["source.pdf", "paper.json", "reader.json", "layout.json"]
            .iter()
            .zip(&before)
        {
            assert_eq!(&fs::read(ws.root.join(name)).unwrap(), original);
        }
    }
}

#[test]
fn shared_figure_captions_keep_panels_titles_scale_and_legacy_suppression_together() {
    let temp = tempfile::tempdir().unwrap();
    for mode in [
        "raster",
        "vector",
        "italic",
        "separate",
        "none",
        "reference",
    ] {
        let source = temp.path().join(format!("panels-{mode}.pdf"));
        support::shared_caption_panels_fixture(&source, mode);
        let ws = Library::open(temp.path().join(format!("library-{mode}")))
            .unwrap()
            .import_pdf(&source, None)
            .unwrap();
        let before: Vec<_> = ["source.pdf", "paper.json", "reader.json", "layout.json"]
            .iter()
            .map(|name| fs::read(ws.root.join(name)).unwrap())
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
        let shared = matches!(mode, "raster" | "vector" | "italic");
        assert_eq!(p.figures.len(), if shared { 1 } else { 2 }, "{mode}");
        assert_eq!(p.html.matches("<img ").count(), p.figures.len(), "{mode}");
        assert_eq!(
            blocks.iter().filter(|b| b["type"] == "figure").count(),
            p.figures.len(),
            "import and display agree: {mode}"
        );
        // The previous importer used each disconnected component's minimum
        // object index. Both legacy blocks must hide, not just the new key.
        assert!(
            p.inline_figures.contains(&"pdf-p001-img003".into()),
            "{mode}"
        );
        assert!(
            p.inline_figures.contains(&"pdf-p001-img005".into()),
            "{mode}"
        );
        if shared {
            let f = &p.figures[0];
            let image = image::load_from_memory(&f.png).unwrap().to_rgba8();
            assert_eq!(image.dimensions(), (f.width, f.height));
            // Source union is roughly 400pt wide and 240pt tall with titles.
            assert!(
                (1.5..1.8).contains(&(f.width as f32 / f.height as f32)),
                "{mode}: {}x{}",
                f.width,
                f.height
            );
            assert!(
                p.html
                    .contains(&format!("aspect-ratio:{}/{}", f.width, f.height))
            );
            assert!(
                p.html
                    .contains(&format!("href=\"easyread-image:{}\"", f.id))
            );
            assert!(
                !p.html.contains("Scaled Dot-Product Attention")
                    && !p.html.contains("Right attention"),
                "Titles belong inside the crop: {mode}"
            );
            let first_painted = |left: u32, right: u32| {
                (0..image.height())
                    .find(|&y| {
                        (left..right).any(|x| {
                            let p = image.get_pixel(x, y).0;
                            p[3] > 128 && p[0] < 80 && p[1] < 80 && p[2] < 80
                        })
                    })
                    .unwrap()
            };
            assert!(
                first_painted(0, f.width / 3) < 35,
                "Left title retained: {mode}"
            );
            assert!(
                first_painted(f.width * 2 / 3, f.width) < 35,
                "Right title retained: {mode}"
            );
            let gap = image.get_pixel(f.width / 2, f.height / 2);
            assert_eq!(gap[3], 0, "Original inter-panel whitespace: {mode}");
            let tag = if mode == "italic" { "em" } else { "span" };
            assert!(p.html.contains(&format!("<p style=\"text-align:center\"><{tag}>Figure 2: Left and right attention panels are shown side by side. Both panels retain their source scale.</{tag}></p>")), "{mode}: caption wrapping/style");
            assert!(p.html.find("Body before").unwrap() < p.html.find("<img ").unwrap());
            assert!(p.html.find("<img ").unwrap() < p.html.find("Figure 2:").unwrap());
            assert!(p.html.find("source scale.").unwrap() < p.html.find("Body after").unwrap());
            let marked = easyread_gpui::annotations::decorate_interactive(
                &p.html,
                &[
                    json!({"id":"panel-caption-note", "quote":"side by side. Both panels", "kind":"underline", "color":"blue"}),
                ],
            );
            assert!(marked.contains("panel-caption-note"));
            assert_eq!(
                easyread_gpui::annotations::plain_text(&marked),
                easyread_gpui::annotations::plain_text(&p.html)
            );
        }
        assert!(
            easyread_gpui::pdf::code_presentation(&source, 1, "edited source")
                .unwrap()
                .is_none()
        );
        for (name, bytes) in ["source.pdf", "paper.json", "reader.json", "layout.json"]
            .iter()
            .zip(before)
        {
            assert_eq!(
                fs::read(ws.root.join(name)).unwrap(),
                bytes,
                "{mode}: {name}"
            );
        }
    }
}

#[test]
fn stretched_standalone_bitmaps_use_intrinsic_ratio_but_composites_keep_page_geometry() {
    let temp = tempfile::tempdir().unwrap();
    for mode in [
        "standalone",
        "normal",
        "overlay",
        "vector",
        "clip",
        "reflect",
        "rotate",
        "nested",
    ] {
        let source = temp.path().join(format!("{mode}.pdf"));
        support::stretched_bitmap_fixture(&source, mode);
        let ws = Library::open(temp.path().join(format!("library-{mode}")))
            .unwrap()
            .import_pdf(&source, None)
            .unwrap();
        let before: Vec<_> = ["source.pdf", "paper.json", "reader.json", "layout.json"]
            .iter()
            .map(|name| fs::read(ws.root.join(name)).unwrap())
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
        assert_eq!(p.figures.len(), 1, "{mode}");
        let f = &p.figures[0];
        let decoded = image::load_from_memory(&f.png).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (f.width, f.height));
        assert!(
            p.html
                .contains(&format!("aspect-ratio:{}/{}", f.width, f.height))
        );
        assert!(p.html.contains(&format!("easyread-image:{}", f.id)));
        assert!(p.inline_figures.contains(&f.id));
        if mode == "standalone" {
            assert_eq!((f.width, f.height), (3, 2));
            assert_eq!(decoded.get_pixel(0, 0).0, [255, 0, 0, 255]);
            assert_eq!(decoded.get_pixel(1, 0).0, [0, 255, 0, 255]);
        } else {
            assert!(
                f.width > 100 && f.height > 100,
                "{mode}: retain page rendering"
            );
            if ["vector", "reflect", "rotate"].contains(&mode) {
                assert!(
                    (f.width as f32 / f.height as f32 - 1.).abs() < 0.04,
                    "{mode}"
                );
            }
        }
        let normalize = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
        // Overlay text is intentionally baked into the composite preview.
        if mode != "overlay" {
            assert_eq!(
                normalize(text),
                normalize(&easyread_gpui::annotations::plain_text(&p.html))
            );
        }
        for (name, bytes) in ["source.pdf", "paper.json", "reader.json", "layout.json"]
            .iter()
            .zip(before)
        {
            assert_eq!(
                fs::read(ws.root.join(name)).unwrap(),
                bytes,
                "{mode}: {name}"
            );
        }
    }
}

#[test]
fn pdf_images_keep_colors_rotation_and_nested_form_regions() {
    let temp = tempfile::tempdir().unwrap();
    let lib = Library::open(temp.path().join("library")).unwrap();
    for rotation in [0, 90, 180, 270] {
        for nested in [false, true] {
            let path = temp.path().join(format!("image-{rotation}-{nested}.pdf"));
            support::image_pdf_fixture(&path, rotation, nested);
            let ws = lib.import_pdf(&path, None).unwrap();
            let raw = ws.state().unwrap();
            let figures: Vec<_> = raw["paper"]["blocks"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|b| b["type"] == "figure")
                .collect();
            assert_eq!(figures.len(), 1, "rotation {rotation}, nested {nested}");
            let figure = figures[0];
            let pixels = image::open(ws.asset(figure["src"].as_str().unwrap()).unwrap())
                .unwrap()
                .to_rgb8();
            let aspect = pixels.width() as f64 / pixels.height() as f64;
            let expected = if rotation == 90 || rotation == 270 {
                2.0 / 3.0
            } else {
                1.5
            };
            assert!((aspect - expected).abs() < 0.06);
            let colorful = pixels
                .pixels()
                .filter(|p| p.0.iter().min().unwrap() < &50 && p.0.iter().max().unwrap() > &200)
                .count();
            assert!(
                colorful > (pixels.width() * pixels.height() / 2) as usize,
                "crop must show actual colored pixels"
            );
            let id = figure["id"].as_str().unwrap();
            assert!(
                raw["layout"][id]["box"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|v| (0.0..=1.0).contains(&v.as_f64().unwrap()))
            );
        }
    }
}
#[test]
fn completing_old_pdf_images_is_additive_idempotent_and_preserves_reader() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("illustrated.pdf");
    support::image_pdf_fixture(&source, 0, true);
    let lib = Library::open(temp.path().join("library")).unwrap();
    let ws = lib.import_pdf(&source, None).unwrap();
    ws.update("paper", |paper| {
        paper["blocks"]
            .as_array_mut()
            .unwrap()
            .retain(|b| b["type"] != "figure");
        paper["future"] = json!({"keep":true});
        Ok(())
    })
    .unwrap();
    ws.reader_ops(&[json!({"op":"edit","block":"pdf-p001","zh":"已有修订","base":"original"})])
        .unwrap();
    let before = fs::read(ws.root.join("reader.json")).unwrap();
    let original = ws.load("paper").unwrap()["blocks"][0].clone();
    assert_eq!(
        ws.complete_pdf_images(&AtomicBool::new(false), |_, _| {})
            .unwrap(),
        1
    );
    assert_eq!(ws.load("paper").unwrap()["blocks"][0], original);
    assert_eq!(ws.load("paper").unwrap()["future"]["keep"], true);
    assert_eq!(before, fs::read(ws.root.join("reader.json")).unwrap());
    assert_eq!(
        ws.complete_pdf_images(&AtomicBool::new(false), |_, _| {})
            .unwrap(),
        0
    );
    let paper_before = fs::read(ws.root.join("paper.json")).unwrap();
    assert!(
        ws.complete_pdf_images(&AtomicBool::new(true), |_, _| {})
            .is_err()
    );
    assert_eq!(paper_before, fs::read(ws.root.join("paper.json")).unwrap());
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
