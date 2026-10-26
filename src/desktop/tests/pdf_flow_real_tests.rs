//! Optional read-only acceptance against imported PDF books. No writes,
//! model calls, or background import: the reader receives in-memory snapshots.
use super::{ReaderView, ReadingMode};
use gpui_kit::{AppContext, ListOffset, TestAppContext, component::Root, px};
use serde_json::{Value, json};

#[gpui_kit::test]
#[ignore = "requires EASYREAD_QA_TOC_BOOK pointing to an imported PDF book"]
fn printed_contents_entries_paint_on_separate_rows_after_reflow(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let root = std::path::PathBuf::from(std::env::var("EASYREAD_QA_TOC_BOOK").unwrap());
    let mut paper: Value =
        serde_json::from_slice(&std::fs::read(root.join("paper.json")).unwrap()).unwrap();
    paper["blocks"]
        .as_array_mut()
        .unwrap()
        .retain(|b| b["source_kind"] == "pdf-text");
    paper["blocks"].as_array_mut().unwrap().truncate(2);
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
    let raw = json!({"paper":paper,"reader":{}});
    let original = raw.clone();
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut reader = ReaderView::new(raw, None, None, window, cx);
            reader.reading_mode = ReadingMode::Original;
            reader.appearance.margin_notes = false;
            for (index, ((_, en), presentation)) in
                requests.into_iter().zip(presentations).enumerate()
            {
                reader
                    .pdf_code_html
                    .insert(index, (en, presentation.unwrap()));
            }
            reader
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    for (width, font) in [(35., 20.), (25., 20.), (35., 24.)] {
        reader.update(cx, |reader, cx| {
            reader.appearance.line_width = width;
            reader.appearance.font_size = font;
            reader.reset_rows();
            cx.notify();
        });
        for index in 0..2 {
            reader.update(cx, |reader, cx| {
                reader.list.scroll_to(ListOffset {
                    item_ix: index,
                    offset_in_item: px(0.),
                });
                cx.notify();
            });
            super::settle(cx);
            reader.read_with(cx, |reader, cx| {
                let state = reader.text_views[&(index, true)].1.read(cx);
                let mut previous_bottom = None;
                let mut rows = 0;
                for paragraph in 0..200 {
                    let Some(bounds) = state.paragraph_bounds(paragraph) else {
                        break;
                    };
                    let (text, range) = state.paragraph_text_context(paragraph).unwrap();
                    let text = &text[range];
                    if !text.contains(" … ") {
                        continue;
                    }
                    assert_eq!(
                        text.matches(" … ").count(),
                        1,
                        "Each paragraph must contain only one entry: {text}"
                    );
                    if let Some(bottom) = previous_bottom {
                        assert!(bounds.top() >= bottom, "TOC rows must not overlap");
                    }
                    assert!(
                        bounds.size.height <= px(font * reader.appearance.line_spacing * 2. + 1.),
                        "Dot leaders must not fill multiple lines: {text}"
                    );
                    assert!(bounds.bottom() <= state.bounds().bottom() + px(1.));
                    previous_bottom = Some(bounds.bottom());
                    rows += 1;
                }
                let html = reader.reader_layout_html(index, true);
                assert!(rows > 10);
                assert_eq!(rows, html.matches("data-vellio-toc").count());
                assert_eq!(reader.raw, original);
            });
        }
    }
}

#[gpui_kit::test]
#[ignore = "requires EASYREAD_QA_BOOK pointing to an imported Attention book"]
fn attention_encoder_last_line_stays_inside_its_measured_row(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let root = std::path::PathBuf::from(std::env::var("EASYREAD_QA_BOOK").unwrap());
    let read = |name: &str| -> Value {
        serde_json::from_slice(&std::fs::read(root.join(format!("{name}.json"))).unwrap()).unwrap()
    };
    let mut paper = read("paper");
    paper["blocks"]
        .as_array_mut()
        .unwrap()
        .retain(|b| b["source_kind"] == "pdf-text" && (b["page"] == 3 || b["page"] == 4));
    let requests = paper["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            (
                b["page"].as_u64().unwrap() as usize,
                b["en"].as_str().unwrap().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    let presentations =
        easyread_gpui::pdf::code_presentations(&root.join("source.pdf"), &requests).unwrap();
    let raw = json!({"paper":paper,"reader":read("reader"),"discussion":read("discussion")});
    let original = raw.clone();
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut reader = ReaderView::new(raw, None, None, window, cx);
            reader.reading_mode = ReadingMode::Translation;
            reader.appearance.font_size = 20.;
            reader.appearance.line_width = 35.;
            reader.appearance.margin_notes = false;
            for (index, ((_, en), presentation)) in
                requests.into_iter().zip(presentations).enumerate()
            {
                reader
                    .pdf_code_html
                    .insert(index, (en, presentation.unwrap()));
            }
            reader
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    for (width, font) in [(35., 20.), (25., 20.), (35., 24.)] {
        reader.update(cx, |reader, cx| {
            reader.appearance.line_width = width;
            reader.appearance.font_size = font;
            reader.reset_rows();
            cx.notify();
        });
        super::settle(cx);
        reader.read_with(cx, |reader, cx| {
            let state = reader.text_views[&(0, false)].1.read(cx);
            let text = state.bounds();
            let row = reader.block_bounds[&0];
            assert!(
                state.paragraph_bounds(7).is_some(),
                "All source-page paragraphs must be laid out"
            );
            for i in 0..100 {
                let Some(paragraph) = state.paragraph_bounds(i) else {
                    break;
                };
                assert!(
                    paragraph.bottom() <= text.bottom() + px(1.),
                    "paragraph {i} extends beyond TextView: {paragraph:?} vs {text:?}"
                );
                assert!(
                    paragraph.bottom() <= row.bottom() + px(1.),
                    "paragraph {i} clipped by row: {paragraph:?} vs {row:?}"
                );
            }
            assert_eq!(reader.raw, original);
        });
        reader.update(cx, |reader, _| {
            let row = reader.block_bounds[&0];
            reader.list.scroll_to(ListOffset {
                item_ix: 0,
                offset_in_item: row.size.height - px(300.),
            });
        });
        super::settle(cx);
        reader.read_with(cx, |reader, cx| {
            let last = reader.text_views[&(0, false)]
                .1
                .read(cx)
                .paragraph_bounds(7)
                .unwrap();
            assert!(last.bottom() <= reader.block_bounds[&0].bottom());
            assert!(
                last.bottom() < reader.block_bounds[&1].top(),
                "The next page must not cover the final line"
            );
        });
    }
}

#[gpui_kit::test]
#[ignore = "requires EASYREAD_QA_BOOK pointing to an imported Attention book"]
fn attention_cross_page_keeps_inline_footnote_without_margin_sized_gap(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let root = std::path::PathBuf::from(std::env::var("EASYREAD_QA_BOOK").unwrap());
    let read = |name: &str| -> Value {
        serde_json::from_slice(&std::fs::read(root.join(format!("{name}.json"))).unwrap()).unwrap()
    };
    let mut paper = read("paper");
    paper["blocks"]
        .as_array_mut()
        .unwrap()
        .retain(|b| b["source_kind"] == "pdf-text" && (b["page"] == 4 || b["page"] == 5));
    assert_eq!(paper["blocks"].as_array().unwrap().len(), 2);
    let requests = paper["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            (
                b["page"].as_u64().unwrap() as usize,
                b["en"].as_str().unwrap().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    let presentations =
        easyread_gpui::pdf::code_presentations(&root.join("source.pdf"), &requests).unwrap();
    let raw = json!({"paper":paper,"reader":read("reader"),"discussion":read("discussion")});
    let original = raw.clone();
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut reader = ReaderView::new(raw, None, None, window, cx);
            reader.reading_mode = ReadingMode::Bilingual;
            for (index, ((_, en), presentation)) in
                requests.into_iter().zip(presentations).enumerate()
            {
                reader
                    .pdf_code_html
                    .insert(index, (en, presentation.unwrap()));
            }
            reader
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    for phase in 0..2 {
        for _ in 0..16 {
            cx.run_until_parked();
            cx.update(|window, cx| {
                window.simulate_next_frame(cx);
                window.draw(cx).clear(cx);
            });
        }
        if phase == 0 {
            reader.update(cx, |reader, _| {
                let row = reader.block_bounds[&0];
                reader.list.scroll_to(ListOffset {
                    item_ix: 0,
                    offset_in_item: (row.size.height - px(300.)).max(px(0.)),
                });
            });
        }
    }
    reader.read_with(cx, |reader, cx| {
        let html = reader.reader_layout_html(0, reader.primary_original());
        assert!(html.contains("To illustrate why the dot products"));
        assert!(html.contains("为了说明"));
        assert!(html.contains("yielding") && html.contains("维的"));
        let next_html = reader.reader_layout_html(1, reader.primary_original());
        assert!(next_html.contains("output values"));
        let text = reader.text_views[&(0, reader.primary_original())]
            .1
            .read(cx)
            .bounds();
        let row = reader.block_bounds[&0];
        assert!(
            row.size.height <= text.size.height + px(110.),
            "Actual paper has a margin-sized gap: {row:?} vs {text:?}"
        );
        let next = reader.block_bounds[&1];
        assert!((next.top() - row.bottom()).abs() < px(2.));
        assert_eq!(reader.raw, original);
    });
}
