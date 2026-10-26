use super::cleaned;

#[test]
fn imported_markup_is_cleaned_before_topcoat_unescaped_output() {
    let html = cleaned(
        r##"<script>alert(1)</script><img src="javascript:alert(1)" onerror="alert(2)"><img src="images/picture.png" alt="figure"><span data-easyread-note="note1"><mark color="#ffeeaa">word</mark></span>"##,
    );
    for forbidden in ["<script", "javascript:", "onerror"] {
        assert!(!html.contains(forbidden), "{html}");
    }
    assert!(html.contains("images/picture.png"), "{html}");
    assert!(html.contains("data-easyread-note=\"note1\""), "{html}");
}

#[test]
fn book_html_rebases_images_and_keeps_only_safe_raster_data_urls() {
    let png = "data:image/png;base64,iVBORw0KGgoAAA==";
    let html = super::cleaned_html(
        &format!(
            r##"<p><img src="figures/a b.webp" alt="diagram" onerror="evil()"><img src="{png}"><img src="data:image/svg+xml,bad"><img src="../secret.png"><a href="data:text/html,bad">bad link</a><a href="#chapter">chapter</a></p>"##
        ),
        Some("test-book"),
    );
    assert!(
        html.contains("/read/test-book/figures/a%20b.webp"),
        "{html}"
    );
    assert!(html.contains(png), "{html}");
    assert!(html.contains("href=\"#chapter\""), "{html}");
    for forbidden in ["onerror", "data:image/svg", "../secret", "data:text/html"] {
        assert!(!html.contains(forbidden), "{html}");
    }
}

#[test]
fn figure_blocks_use_book_scoped_urls_and_caption_translations() {
    use serde_json::{Value, json};
    let mut raw: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fixtures/state.json"
    )))
    .unwrap();
    raw["paper"]["blocks"] = json!([
        {"id":"epub-figure","type":"figure","page":1,"src":"images/book picture.png","alt":"A diagram","caption_en":"Figure 1","caption_zh":"图 1"},
        {"id":"pdf-figure","type":"figure","page":1,"src":"figures/native.webp","caption_en":"Figure 2"}
    ]);
    let state = super::ReaderState::from_value(raw.clone()).unwrap();
    let blocks = super::display_blocks(&raw, &state, &[], 1, true, None, "test-book");
    assert_eq!(blocks[0].src, "/read/test-book/images/book%20picture.png");
    assert_eq!(blocks[0].alt, "A diagram");
    assert!(!blocks[0].untranslated);
    assert!(blocks[0].original.contains("Figure 1"));
    assert!(blocks[0].translated.contains("图 1"));
    assert_eq!(blocks[1].src, "/read/test-book/figures/native.webp");
}

#[test]
fn pdf_page_translation_pairs_paragraphs_without_changing_the_persisted_anchor() {
    use super::display_blocks;
    use easyread_gpui::model::{ReaderState, text_hash};
    use serde_json::json;
    let mut raw: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fixtures/state.json"
    )))
    .unwrap();
    let en = "First source paragraph.\n\nSecond source paragraph.";
    let zh = "第一段译文。\n\n第二段译文。";
    raw["paper"]["blocks"].as_array_mut().unwrap().push(json!({"id":"pdf-page-pair","type":"para", "source_kind":"pdf-text","page":2,
        "en":en,"zh":zh,"native_translation_layout":{"version":1,"source_hash":text_hash(en),"translation_hash":text_hash(zh),
            "html":"<p>第一段译文。</p><p>第二段译文。</p>"}}));
    let state = ReaderState::from_value(raw.clone()).unwrap();
    let blocks = display_blocks(&raw, &state, &[], 2, true, None, "test-book");
    let pair = &blocks[0];
    assert!(pair.page_block);
    assert_eq!(pair.paired.matches("data-easyread-companion").count(), 2);
    let positions = [
        "First source paragraph",
        "第一段译文",
        "Second source paragraph",
        "第二段译文",
    ]
    .map(|text| pair.paired.find(text).unwrap());
    assert!(positions.windows(2).all(|window| window[0] < window[1]));
    assert_eq!(pair.id, "pdf-page-pair");
    let note = json!({"id":"pdf-inline-note","anchor":"pdf-page-pair","side":"en","quote":"Second source",
        "prefix":"","suffix":"","kind":"highlight","color":"yellow","style":"highlight"});
    let marked = display_blocks(&raw, &state, &[note], 2, true, None, "test-book");
    assert!(
        marked[0]
            .paired
            .contains("data-easyread-note=\"pdf-inline-note\"")
    );
    raw["paper"]["blocks"]
        .as_array_mut()
        .unwrap()
        .last_mut()
        .unwrap()["zh"] = json!("合并译文。");
    let state = ReaderState::from_value(raw.clone()).unwrap();
    assert!(
        display_blocks(&raw, &state, &[], 2, true, None, "test-book")[0]
            .paired
            .is_empty(),
        "ambiguous legacy translations must not be guessed into paragraph pairs"
    );
}

#[test]
fn only_generated_math_metadata_and_palette_underlines_survive_sanitizing() {
    let html = cleaned(
        r#"<p data-vellio-equation="true"><span data-vellio-math="x^2" data-display="false">$x^2$</span></p><p data-vellio-footnote="true">4Footnote</p><u style="text-decoration-color:#477fc1">blue</u><u style="background:url(javascript:evil)">bad</u>"#,
    );
    assert!(html.contains("data-vellio-math=\"x^2\""), "{html}");
    assert!(html.contains("data-underline=\"blue\""), "{html}");
    assert!(html.contains("data-vellio-equation=\"true\""), "{html}");
    assert!(html.contains("data-vellio-footnote=\"true\""), "{html}");
    assert!(!html.contains("javascript:"), "{html}");
}
