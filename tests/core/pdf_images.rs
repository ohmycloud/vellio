use super::*;

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
