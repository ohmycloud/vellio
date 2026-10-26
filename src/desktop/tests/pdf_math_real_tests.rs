//! Read-only native-window acceptance for the user's mathematical examples.
use super::{ReaderView, ReadingMode};
use gpui_kit::{AppContext, ListOffset, TestAppContext, component::Root, px};
use serde_json::{Value, json};

#[gpui_kit::test]
#[ignore = "requires EASYREAD_QA_MATH_BOOK pointing to Agentic Large Language Models"]
fn agentic_equations_paint_native_typst_and_copy_complete_source_after_reflow(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let root = std::path::PathBuf::from(std::env::var("EASYREAD_QA_MATH_BOOK").unwrap());
    let mut paper: Value =
        serde_json::from_slice(&std::fs::read(root.join("paper.json")).unwrap()).unwrap();
    paper["blocks"].as_array_mut().unwrap().retain(|b| {
        b["source_kind"] == "pdf-text" && [json!(49), json!(50), json!(183)].contains(&b["page"])
    });
    let requests: Vec<_> = paper["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            (
                b["page"].as_u64().unwrap() as usize,
                b["en"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let presentations =
        easyread_gpui::pdf::code_presentations(&root.join("source.pdf"), &requests).unwrap();
    let presentations: Vec<_> = presentations
        .into_iter()
        .map(|p| p.unwrap().unwrap())
        .collect();
    let equations: Vec<_> = presentations
        .iter()
        .zip(&requests)
        .map(|(p, (page, _))| {
            p.math
                .iter()
                .find(|m| {
                    m.display
                        && match page {
                            49 => m.number.as_deref() == Some("2.10"),
                            50 => m.number.as_deref() == Some("2.11"),
                            183 => m.tex.contains("outer loop"),
                            _ => false,
                        }
                })
                .unwrap()
                .tex
                .clone()
        })
        .collect();
    assert_eq!(equations.len(), 3);
    let raw = json!({"paper":paper,"reader":{}});
    let original = raw.clone();
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut reader = ReaderView::new(raw, None, None, window, cx);
            reader.reading_mode = ReadingMode::Original;
            reader.appearance.margin_notes = false;
            for (index, ((_, source), presentation)) in
                requests.into_iter().zip(presentations).enumerate()
            {
                reader
                    .pdf_code_html
                    .insert(index, (source, Some(presentation)));
            }
            reader
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    for (width, font) in [(35., 20.), (25., 24.)] {
        reader.update(cx, |reader, cx| {
            reader.appearance.font_size = font;
            reader.appearance.line_width = width;
            reader.reset_rows();
            cx.notify();
        });
        for (index, equation) in equations.iter().enumerate() {
            reader.update(cx, |reader, cx| {
                reader.list.scroll_to(ListOffset {
                    item_ix: index,
                    offset_in_item: px(0.),
                });
                cx.notify();
            });
            super::settle(cx);
            assert!(cx.debug_bounds("reader-native-typst-math").is_some());
            let view =
                reader.read_with(cx, |reader, _| reader.text_views[&(index, true)].1.clone());
            view.update(cx, |state, cx| {
                state.select_all(cx);
                assert!(
                    state.selected_text().contains(equation),
                    "The whole display object must copy its complete semantic TeX: {equation}"
                );
                state.clear_selection(cx);
            });
            reader.read_with(cx, |reader, cx| {
                let html = reader.reader_layout_html(index, true);
                let escaped = equation
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;")
                    .replace('"', "&quot;");
                assert!(html.contains(&format!("data-vellio-math=\"{escaped}\"")));
                let layout = easyread_gpui::formula::cached_layout(equation, true).unwrap();
                assert!(layout.is_valid());
                let state = view.read(cx);
                for paragraph in 0..200 {
                    let Some(bounds) = state.paragraph_bounds(paragraph) else {
                        break;
                    };
                    assert!(
                        bounds.bottom() <= state.bounds().bottom() + px(1.),
                        "Reflow must not clip formula rows"
                    );
                }
                assert_eq!(reader.raw, original);
            });
        }
    }
}
