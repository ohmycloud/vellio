use super::*;

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
