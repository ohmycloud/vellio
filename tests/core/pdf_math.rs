use super::*;

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
