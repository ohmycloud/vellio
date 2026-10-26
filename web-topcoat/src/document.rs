//! Sanitize imported markup and build Web reader display blocks.
use super::*;

pub(super) fn cleaned(html: &str) -> String {
    cleaned_html(html, None)
}
pub(super) fn cleaned_html(html: &str, book: Option<&str>) -> String {
    // Keep only the four application-owned underline colors; never allow arbitrary CSS.
    let mut html = html.to_owned();
    for (name, ink) in [
        ("yellow", "dca722"),
        ("green", "58994c"),
        ("blue", "477fc1"),
        ("pink", "cc413a"),
    ] {
        html = html.replace(
            &format!("<u style=\"text-decoration-color:#{ink}\">"),
            &format!("<u data-underline=\"{name}\">"),
        );
    }
    let mut sanitizer = ammonia::Builder::new();
    sanitizer.url_relative(ammonia::UrlRelative::PassThrough);
    sanitizer.add_tags(&["img", "mark", "u", "span"]);
    if let Some(book) = book {
        let book = book.to_owned();
        sanitizer.add_url_schemes(&["data"]);
        sanitizer.attribute_filter(move |tag, attribute, value| {
            use std::borrow::Cow;
            if tag == "img" && attribute == "src" {
                if book_images::inline_png(value) {
                    return Some(Cow::Borrowed(value));
                }
                return book_images::html_url(&book, value).map(Cow::Owned);
            }
            // Enabling raster data URLs must not enable data: links or other uses.
            if value.trim_start().to_ascii_lowercase().starts_with("data:") {
                return None;
            }
            Some(Cow::Borrowed(value))
        });
    }
    sanitizer.add_generic_attributes(&[
        "class",
        "lang",
        "data-read-paragraph",
        "data-edit-paragraph",
    ]);
    sanitizer.add_tag_attributes("img", &["src", "alt", "width", "height"]);
    sanitizer.add_tag_attributes("mark", &["color"]);
    sanitizer.add_tag_attributes("p", &["data-vellio-equation", "data-vellio-footnote"]);
    sanitizer.add_tag_attributes("blockquote", &["data-easyread-companion"]);
    sanitizer.add_tag_attributes("u", &["data-underline"]);
    sanitizer.add_tag_attributes(
        "span",
        &["data-easyread-note", "data-vellio-math", "data-display"],
    );
    sanitizer.clean(&html).to_string()
}
pub(super) fn display_blocks(
    raw: &Value,
    state: &ReaderState,
    notes: &[Value],
    page: usize,
    pdf: bool,
    source_pdf: Option<&std::path::Path>,
    book_id: &str,
) -> Vec<RenderBlock> {
    // Reuse the PDF presentation's inline-image identities, not just its HTML:
    // native crops replace their legacy standalone blocks rather than duplicating them.
    let mut native = HashMap::new();
    if let Some(path) = source_pdf {
        let blocks: Vec<_> = state
            .paper
            .blocks
            .iter()
            .filter(|block| block.source_kind == "pdf-text" && block.page == page)
            .collect();
        let requests: Vec<_> = blocks
            .iter()
            .map(|block| (block.page, block.en.clone()))
            .collect();
        if let Ok(presentations) = pdf::code_presentations(path, &requests) {
            for (block, presentation) in blocks.into_iter().zip(presentations) {
                if let Ok(Some(presentation)) = presentation {
                    native.insert(block.id.clone(), presentation);
                }
            }
        }
    }
    let inline_figures: HashSet<_> = native
        .values()
        .flat_map(|presentation| presentation.inline_figures.iter().map(String::as_str))
        .collect();
    state
        .paper
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(i, b)| {
            let source = &raw["paper"]["blocks"][i];
            let position = source[if pdf { "page" } else { "chapter" }]
                .as_u64()
                .unwrap_or(1) as usize;
            if position != page
                || (b.kind == "figure" && inline_figures.contains(b.id.as_str()))
                || !matches!(
                    b.kind.as_str(),
                    "heading" | "para" | "note" | "figure" | "table" | "list" | "math"
                )
            {
                return None;
            }
            let src = source["src"]
                .as_str()
                .and_then(|path| book_images::url(book_id, path))
                .unwrap_or_default();
            let alt = source["alt"]
                .as_str()
                .filter(|alt| !alt.trim().is_empty())
                .unwrap_or("插图")
                .to_owned();
            let original = if b.kind == "math" && !b.tex.is_empty() {
                formula::native_html(&b.tex, true)
            } else if b.kind == "figure" && !src.is_empty() {
                annotations::html(&b.caption_en)
            } else if b.source_kind == "pdf-text" {
                // Desktop's native PDF geometry recovery is reusable here as HTML.
                // Only accept it when its extracted text matches the stored block.
                native
                    .get(&b.id)
                    .map(|presentation| presentation.html.clone())
                    .unwrap_or_else(|| formula::native_markdown_html(&state.original_markdown(b)))
            } else {
                epub::source_html(source, b, state, true)
                    .unwrap_or_else(|| formula::native_markdown_html(&state.original_markdown(b)))
            };
            let untranslated = if b.kind == "figure" {
                b.caption_zh.trim().is_empty() && state.caption(b) == b.caption_en
            } else {
                b.zh.trim().is_empty() && state.text(&b.id, &b.zh, &b.en) == b.en
            };
            let active = state.text(&b.id, &b.zh, &b.en);
            let translated = if untranslated || (b.kind == "math" && !b.tex.is_empty()) {
                original.clone()
            } else if b.kind == "figure" && !src.is_empty() {
                annotations::html(state.caption(b))
            } else if b.source_kind == "pdf-text" {
                easyread_gpui::paragraph_edit::edited_html(
                    &raw["reader"]["edits"][&b.id],
                    &b.en,
                    active,
                )
                .map(str::to_owned)
                .or_else(|| translation_layout::saved_html(source, active))
                .or_else(|| translation_layout::legacy_html(&b.en, active, &original, b.page))
                .unwrap_or_else(|| formula::native_markdown_html(&state.markdown(b, false)))
            } else {
                easyread_gpui::paragraph_edit::edited_html(
                    &raw["reader"]["edits"][&b.id],
                    &b.en,
                    active,
                )
                .filter(|_| b.kind != "heading")
                .map(str::to_owned)
                .unwrap_or_else(|| formula::native_markdown_html(&state.markdown(b, false)))
            };
            let translated = if b.source_kind == "pdf-text" {
                paragraphs::inherit_footnotes(&original, &translated)
            } else {
                translated
            };
            let edit_key = if matches!(b.kind.as_str(), "figure" | "table") {
                format!("{}#caption", b.id)
            } else {
                b.id.clone()
            };
            let edit_text = if matches!(b.kind.as_str(), "figure" | "table") {
                state.caption(b).to_owned()
            } else {
                state.text(&b.id, &b.zh, &b.en).to_owned()
            };
            let edit_previous = raw["reader"]["edits"]
                .get(&edit_key)
                .cloned()
                .unwrap_or(Value::Null)
                .to_string();
            let has_edit = raw["reader"]["edits"][&edit_key]["zh"].is_string()
                && raw["reader"]["edits"][&edit_key]["reverted"] != true;
            let edit_html = if b.kind == "heading" {
                formula::native_markdown_html(&edit_text)
            } else {
                translated.clone()
            };
            let (original, _) = paragraphs::prepare(&original, None);
            let (translated, mut edit_paragraphs) =
                paragraphs::prepare(&translated, Some(&edit_text));
            // Heading Markdown is wrapped for display, but its stored edit is
            // the heading text, not another nested heading marker.
            if b.kind == "heading" && edit_paragraphs.len() == 1 {
                edit_paragraphs[0].text = edit_text.clone();
            }
            let matching = |side| {
                notes
                    .iter()
                    .filter(|n| n["anchor"] == b.id && n["side"].as_str().unwrap_or("zh") == side)
                    .cloned()
                    .collect::<Vec<_>>()
            };
            let original_decorated = annotations::decorate_interactive(&original, &matching("en"));
            let translated_decorated = annotations::decorate_interactive(
                &translated,
                &matching(if untranslated { "en" } else { "zh" }),
            );
            let page_block = b.source_kind == "pdf-text";
            // Pair the recovered structure BEFORE sanitizing away role-bearing layout
            // attributes. Only the already-sanitized result reaches Unescaped.
            let paired = if !untranslated {
                let footer =
                    b.en.lines()
                        .last()
                        .is_some_and(|line| line.trim() == b.page.to_string());
                PairLayout::for_bilingual(
                    &original_decorated,
                    &translated_decorated,
                    true,
                    (page_block && footer).then_some(b.page),
                )
                .map(|layout| cleaned_html(&layout.render(&layout.all_pairs()), Some(book_id)))
                .unwrap_or_default()
            } else {
                String::new()
            };
            let original = cleaned_html(&original_decorated, Some(book_id));
            let translated = cleaned_html(&translated_decorated, Some(book_id));
            Some(RenderBlock {
                id: b.id.clone(),
                kind: b.kind.clone(),
                original,
                translated,
                src,
                alt,
                untranslated,
                page_block,
                paired,
                edit_hash: easyread_gpui::model::text_hash(&edit_text),
                edit_html,
                edit_paragraphs,
                edit_key,
                edit_previous,
                has_edit,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
