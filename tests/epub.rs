use easyread_gpui::{formats::epub, model::ReaderState, store::Library};
use serde_json::json;
use std::{fs, sync::atomic::AtomicBool};
#[path = "support/epub.rs"]
mod fixtures;

#[test]
fn epub2_and_epub3_import_spine_semantics_images_navigation_and_stable_ids() {
    for epub2 in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("fixture.epub");
        fixtures::fixture(&source, epub2);
        let original = fs::read(&source).unwrap();
        let library = Library::open(temp.path().join("library")).unwrap();
        let mut progress = vec![];
        let ws = library
            .import_file_cancellable(&source, None, &AtomicBool::new(false), |d, t| {
                progress.push((d, t))
            })
            .unwrap();
        assert_eq!(progress, [(1, 2), (2, 2)]);
        let raw = ws.validated_state().unwrap();
        let state = ReaderState::from_value(raw.clone()).unwrap();
        assert_eq!(state.title(), "EPUB 可行性验证");
        assert_eq!(raw["paper"]["meta"]["format"], "epub");
        assert_eq!(raw["paper"]["meta"]["language"], "zh-CN");
        assert_eq!(raw["paper"]["meta"]["authors"], json!(["Vellio QA"]));
        assert_eq!(raw["paper"]["meta"]["chapter_count"], 2);
        assert!(state.paper.meta.pages.is_empty());
        assert!(state.paper.blocks.iter().all(|b| b.page == 0));
        assert_eq!(
            fs::read(ws.asset("source.epub").unwrap()).unwrap(),
            original
        );
        assert_eq!(fs::read(&source).unwrap(), original);
        assert!(!ws.root.join("source.pdf").exists());
        assert!(!ws.root.join(".native-pdf-layout-cache").exists());
        let html: String = raw["paper"]["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|b| b["html"].as_str())
            .collect();
        for expected in [
            "<h1>",
            "<h2>",
            "<strong>",
            "<em>",
            "<ul>",
            "<ol>",
            "<blockquote>",
            "<pre>",
            "<table>",
            "language-rust",
            "vellio-epub:",
            "https://example.com/",
        ] {
            assert!(html.contains(expected), "{expected}: {html}");
        }
        for unexpected in [
            "<script",
            "<iframe",
            "onclick",
            "javascript:",
            "font-size",
            "window.bad",
        ] {
            assert!(!html.contains(unexpected), "{unexpected}");
        }
        let first = state
            .paper
            .blocks
            .iter()
            .position(|b| b.en.starts_with("第一章："))
            .unwrap();
        let second = state
            .paper
            .blocks
            .iter()
            .position(|b| b.en.starts_with("第二章："))
            .unwrap();
        assert!(first < second);
        let pre = state
            .paper
            .blocks
            .iter()
            .find(|b| b.en.contains("fn main()"))
            .unwrap();
        assert!(pre.en.contains("\n    println!"));
        let image = raw["paper"]["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["type"] == "figure")
            .unwrap();
        let publisher_caption = raw["paper"]["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["source_role"] == "figure-caption")
            .unwrap();
        assert_eq!(publisher_caption["type"], "para");
        assert!(publisher_caption["level"].is_null());
        assert_eq!(
            publisher_caption["html"],
            "<p><span>Figure 2.1 A caption with an <em>italic</em> word and <strong>explicit emphasis</strong>.</span></p>"
        );
        assert!(
            publisher_caption["en"]
                .as_str()
                .unwrap()
                .contains("*italic*")
        );
        let real_heading = state
            .paper
            .blocks
            .iter()
            .find(|b| b.en.starts_with("Figure 9.1"))
            .unwrap();
        assert_eq!(
            real_heading.kind, "heading",
            "actual h5 headings must retain their semantics"
        );
        assert_eq!(image["caption_en"], "图 1：原生图片");
        assert_eq!(image["width"], 160);
        assert_eq!(image["height"], 100);
        assert!(ws.asset(image["src"].as_str().unwrap()).unwrap().is_file());
        let cover = ws
            .asset(raw["paper"]["meta"]["cover"].as_str().unwrap())
            .unwrap();
        assert_eq!(
            image::guess_format(&fs::read(cover).unwrap()).unwrap(),
            image::ImageFormat::Jpeg
        );
        let toc = raw["paper"]["outline"].as_array().unwrap();
        assert_eq!(toc.len(), 3);
        assert_eq!(toc[1]["depth"], 1);
        assert_eq!(toc[2]["block"], state.paper.blocks[second].id);
        assert!(
            raw["paper"]["link_targets"]
                .as_object()
                .unwrap()
                .values()
                .any(|v| v == &json!(state.paper.blocks[second].id))
        );
        let before = fs::read(ws.root.join("paper.json")).unwrap();
        assert_eq!(library.import_file(&source, None).unwrap().root, ws.root);
        assert_eq!(fs::read(ws.root.join("paper.json")).unwrap(), before);
        let other = Library::open(temp.path().join("other"))
            .unwrap()
            .import_file(&source, None)
            .unwrap();
        assert_eq!(
            other.load("paper").unwrap()["blocks"],
            raw["paper"]["blocks"]
        );
    }
}

#[test]
fn legacy_publisher_captions_display_as_paragraphs_without_rewriting_data() {
    let raw = json!({"paper":{"meta":{"format":"epub"},"blocks":[
        {"id":"image","type":"figure","source_kind":"epub-image","source_location":{"href":"chapter.xhtml","dom":"body/1/6/1"}},
        {"id":"caption","type":"heading","source_kind":"epub-xhtml","level":5,
         "en":"Figure 7.1 A caption with emphasis.","html":"<h5>Figure 7.1 A caption with <em>emphasis</em>.</h5>",
         "source_location":{"href":"chapter.xhtml","dom":"body/1/6/3"}},
        {"id":"heading","type":"heading","source_kind":"epub-xhtml","level":5,
         "en":"Figure 9.1 is discussed here","html":"<h5>Figure 9.1 is discussed here</h5>",
         "source_location":{"href":"chapter.xhtml","dom":"body/1/8/1"}}
    ]},"reader":{}});
    let state = ReaderState::from_value(raw.clone()).unwrap();
    let caption = &state.paper.blocks[1];
    assert_eq!(caption.kind, "para");
    assert_eq!(state.paper.blocks[2].kind, "heading");
    assert_eq!(state.markdown(caption, false), caption.en);
    assert_eq!(state.original_markdown(caption), caption.en);
    assert_eq!(
        epub::source_html(&raw["paper"]["blocks"][1], caption, &state, false).unwrap(),
        "<p><span>Figure 7.1 A caption with <em>emphasis</em>.</span></p>"
    );
    assert_eq!(raw["paper"]["blocks"][1]["type"], "heading");
    assert_eq!(
        raw["paper"]["blocks"][1]["html"],
        "<h5>Figure 7.1 A caption with <em>emphasis</em>.</h5>"
    );
    let mut translated = raw.clone();
    translated["reader"]["edits"] = json!({"caption":{"zh":"图 7.1 用户修订 **保留粗体**","base":easyread_gpui::model::text_hash(&caption.en)}});
    let translated_state = ReaderState::from_value(translated).unwrap();
    let edited = &translated_state.paper.blocks[1];
    assert_eq!(
        translated_state.markdown(edited, false),
        "图 7.1 用户修订 **保留粗体**"
    );
    assert!(
        epub::source_html(&raw["paper"]["blocks"][1], edited, &translated_state, false).is_none()
    );
    assert!(
        epub::source_html(&raw["paper"]["blocks"][1], edited, &translated_state, true)
            .unwrap()
            .starts_with("<p>")
    );

    for label in [
        "Figure A.6",
        "Figure D.2",
        "Figure E.5",
        "Fig. AA.12",
        "Figure 12.3",
    ] {
        let mut appendix = raw.clone();
        appendix["paper"]["blocks"][1]["en"] = json!(format!("{label} A caption."));
        appendix["paper"]["blocks"][1]["html"] = json!(format!("<h5>{label} A caption.</h5>"));
        let state = ReaderState::from_value(appendix.clone()).unwrap();
        assert_eq!(
            state.paper.blocks[1].kind, "para",
            "missed caption: {label}"
        );
        assert!(
            epub::source_html(
                &appendix["paper"]["blocks"][1],
                &state.paper.blocks[1],
                &state,
                false
            )
            .unwrap()
            .starts_with("<p>")
        );
    }

    for (field, value) in [
        ("level", json!(2)),
        ("en", json!("Figure design principles")),
        ("source_kind", json!("pdf-text")),
        (
            "source_location",
            json!({"href":"chapter.xhtml","dom":"body/1/8/1"}),
        ),
        (
            "source_location",
            json!({"href":"other.xhtml","dom":"body/1/6/3"}),
        ),
    ] {
        let mut non_caption = raw.clone();
        non_caption["paper"]["blocks"][1][field] = value;
        assert_eq!(
            ReaderState::from_value(non_caption).unwrap().paper.blocks[1].kind,
            "heading"
        );
    }
}

/// The optional paths audit a real book in a temporary library; source files
/// and the user's persisted paper are never changed.
#[test]
fn publisher_caption_audit_covers_the_whole_epub_and_legacy_paper() {
    use std::io::Read as _;
    let temp = tempfile::tempdir().unwrap();
    let fixture = temp.path().join("captions.epub");
    fixtures::fixture(&fixture, false);
    let source = std::env::var_os("EASYREAD_EPUB_QA_SOURCE")
        .map(std::path::PathBuf::from)
        .unwrap_or(fixture);
    let ws = Library::open(temp.path().join("library"))
        .unwrap()
        .import_file(&source, None)
        .unwrap();
    let imported = ws.validated_state().unwrap();
    let mut papers = vec![("new import", imported)];
    if let Some(path) = std::env::var_os("EASYREAD_EPUB_QA_LEGACY_PAPER") {
        let paper: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        papers.push(("legacy paper", json!({"paper":paper,"reader":{}})));
    }
    let mut archive = zip::ZipArchive::new(fs::File::open(&source).unwrap()).unwrap();
    let mut expected = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        let href = entry.name().to_owned();
        if !href.ends_with(".xhtml") {
            continue;
        }
        let mut xhtml = String::new();
        entry.read_to_string(&mut xhtml).unwrap();
        let document = roxmltree::Document::parse_with_options(
            &xhtml,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        )
        .unwrap();
        for node in document.descendants().filter(|node| {
            node.attribute("class").is_some_and(|classes| {
                classes
                    .split_whitespace()
                    .any(|class| matches!(class, "figure-container-h5" | "figure-container-h6"))
            })
        }) {
            let text: String = node
                .descendants()
                .filter(|n| n.is_text())
                .filter_map(|n| n.text())
                .collect();
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            expected.push((href.clone(), text));
        }
    }
    assert!(
        !expected.is_empty(),
        "source must contain publisher figure captions"
    );
    for (name, raw) in papers {
        let state = ReaderState::from_value(raw.clone()).unwrap();
        let blocks = raw["paper"]["blocks"].as_array().unwrap();
        for (href, text) in &expected {
            let index = blocks
                .iter()
                .position(|block| {
                    block["source_location"]["href"] == *href
                        && block["html"].as_str().is_some_and(|html| {
                            easyread_gpui::annotations::plain_text(html)
                                .split_whitespace()
                                .collect::<Vec<_>>()
                                .join(" ")
                                == *text
                        })
                })
                .expect("every publisher caption must retain its own block");
            assert!(
                epub::is_figure_caption(blocks, index),
                "{name}: missed caption in {href}: {}",
                blocks[index]["id"]
            );
            let caption = &state.paper.blocks[index];
            assert_eq!(caption.kind, "para", "{name}: caption remained a heading");
            let html = epub::source_html(&blocks[index], caption, &state, true).unwrap();
            assert!(
                html.starts_with("<p>"),
                "{name}: caption inherited heading weight: {html}"
            );
            assert!(!state.original_markdown(caption).starts_with("## "));
        }
        let appendix_count = expected
            .iter()
            .filter(|(_, text)| {
                text.split_whitespace()
                    .nth(1)
                    .is_some_and(|number| number.starts_with(|c: char| c.is_ascii_alphabetic()))
            })
            .count();
        eprintln!(
            "EPUB caption audit ({name}): {} publisher captions, {appendix_count} appendix captions; all normal-weight paragraph semantics",
            expected.len()
        );
    }
}

#[test]
fn epub_edits_notes_and_progress_reopen_without_overwriting_source_html() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("fixture.epub");
    fixtures::fixture(&source, false);
    let lib = Library::open(temp.path().join("library")).unwrap();
    let ws = lib.import_file(&source, None).unwrap();
    let mut raw = ws.state().unwrap();
    let state = ReaderState::from_value(raw.clone()).unwrap();
    let (index, block) = state
        .paper
        .blocks
        .iter()
        .enumerate()
        .find(|(_, b)| b.en.contains("English"))
        .unwrap();
    let source_html =
        epub::source_html(&raw["paper"]["blocks"][index], block, &state, false).unwrap();
    let id = block.id.clone();
    ws.reader_ops(&[
        json!({"op":"edit","block":id,"zh":"用户修订 **保留**","base":easyread_gpui::model::text_hash(&block.en)}),
        json!({"op":"note","note":{"id":"epub-note","anchor":id,"key":id,"kind":"highlight","quote":"English","color":"yellow","updated":"2026-10-10T00:00:00Z"}}),
        json!({"op":"progress","block":id,"ratio":0.25}),
    ]).unwrap();
    raw = lib.workspace(&ws.id()).unwrap().validated_state().unwrap();
    let state = ReaderState::from_value(raw.clone()).unwrap();
    let block = &state.paper.blocks[index];
    assert!(epub::source_html(&raw["paper"]["blocks"][index], block, &state, false).is_none());
    assert_eq!(
        epub::source_html(&raw["paper"]["blocks"][index], block, &state, true).unwrap(),
        source_html
    );
    assert_eq!(state.markdown(block, false), "用户修订 **保留**");
    assert_eq!(raw["reader"]["notes"]["epub-note"]["quote"], "English");
    assert_eq!(raw["reader"]["progress"]["block"], id);
    let decorated = easyread_gpui::annotations::decorate_interactive(
        &source_html,
        &[raw["reader"]["notes"]["epub-note"].clone()],
    );
    assert!(decorated.contains("epub-note"));
    assert_eq!(
        easyread_gpui::annotations::plain_text(&source_html),
        easyread_gpui::annotations::plain_text(&decorated)
    );
}

#[test]
fn failed_unsafe_and_cancelled_epub_imports_leave_no_library_entries() {
    let temp = tempfile::tempdir().unwrap();
    let lib = Library::open(temp.path().join("library")).unwrap();
    let source = temp.path().join("bad.epub");
    for entries in [
        vec![
            ("mimetype", b"application/epub+zip".as_slice()),
            ("../escape", b"bad".as_slice()),
        ],
        vec![("mimetype", b"not-an-epub".as_slice())],
        vec![
            ("mimetype", b"application/epub+zip".as_slice()),
            (
                "META-INF/container.xml",
                br#"<container><rootfile full-path="../../outside"/></container>"#.as_slice(),
            ),
        ],
    ] {
        fixtures::archive(&source, &entries);
        assert!(lib.import_file(&source, None).is_err());
        assert!(lib.list().unwrap().is_empty());
        assert_eq!(fs::read_dir(&lib.root).unwrap().count(), 0);
    }
    fixtures::fixture(&source, false);
    assert!(
        lib.import_file_cancellable(&source, None, &AtomicBool::new(true), |_, _| {})
            .is_err()
    );
    let cancel = AtomicBool::new(false);
    assert!(
        lib.import_file_cancellable(&source, None, &cancel, |_, _| cancel
            .store(true, std::sync::atomic::Ordering::Relaxed))
            .is_err()
    );
    assert_eq!(fs::read_dir(&lib.root).unwrap().count(), 0);
}
