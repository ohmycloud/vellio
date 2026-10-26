//! Native-window regressions for continuous PDF pages, footnotes and margin
//! replies. These inspect painted geometry, not only generated HTML.
use super::{ReaderView, ReadingMode};
use easyread_gpui::{model::text_hash, pdf::CodePresentation};
use gpui_kit::{AppContext, TestAppContext, VisualTestContext, component::Root, px};
use serde_json::{Value, json};

#[path = "desktop_pdf_flow_real_tests.rs"]
mod real;
#[path = "desktop_pdf_math_real_tests.rs"]
mod math_real;

fn fixture(long_note: bool) -> (Value, String) {
    let body = "Instead of one attention function we project the queries, keys and values. On each projected version we perform attention in parallel, yielding dimensional";
    let source = format!(
        "<p>{body}</p><p data-vellio-footnote=\"true\"><sup>4</sup>A separate explanatory footnote.</p>"
    );
    let en = format!("{body}\n4A separate explanatory footnote.\n1");
    let zh = "我们将查询、键和值分别投影，然后并行执行注意力计算，得到相应维度的\n\n4一个独立的解释性脚注。";
    // Simulate a saved translation predating footnote metadata.
    let translated = "<p>我们将查询、键和值分别投影，然后并行执行注意力计算，得到相应维度的</p><p><sup>4</sup>一个独立的解释性脚注。</p>";
    let mut raw = json!({"paper":{"meta":{"title_en":"PDF continuous reading regression"},"blocks":[
        {"id":"page-one","type":"para","source_kind":"pdf-text","page":1,"en":en,"zh":zh,
            "native_translation_layout":{"version":1,"source_hash":text_hash(&en),"translation_hash":text_hash(zh),"html":translated}},
        {"id":"page-two","type":"para","source_kind":"pdf-text","page":2,"en":"output values. These are concatenated to form the final values.","zh":"输出值。将这些值拼接，得到最终值。"}
    ]},"reader":{"notes":{}}});
    if long_note {
        raw["reader"]["notes"]["long-note"] = json!({"id":"long-note","anchor":"page-one","key":"page-one","side":"en",
            "quote":"projected version","kind":"note","body":"Why project these values?","color":"yellow"});
        raw["discussion"] = json!({"entries":[{"id":"long-reply","anchor":"page-one","kind":"reply","reply_to":"long-note",
            "body":"A lengthy explanation of the projection and its output values.\n\n".repeat(60),"by":"Test AI"}]});
    }
    (raw, source)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..12 {
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.draw(cx).clear(cx);
        });
    }
}

#[gpui_kit::test]
fn pdf_footnotes_stay_inline_in_all_reading_modes_without_changing_anchors(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    for mode in [
        ReadingMode::Original,
        ReadingMode::Translation,
        ReadingMode::Bilingual,
    ] {
        for original_first in [true, false] {
            let (raw, source) = fixture(false);
            let original = raw.clone();
            let mut reader = None;
            let (_, cx) = cx.add_window_view(|window, cx| {
                let view = cx.new(|cx| {
                    let mut reader = ReaderView::new(raw, None, None, window, cx);
                    reader.reading_mode = mode;
                    reader.original_first = original_first;
                    reader.pdf_code_html.insert(
                        0,
                        (
                            reader.state.paper.blocks[0].en.clone(),
                            Some(CodePresentation {
                                html: source,
                                inline_figures: vec![],
                                figures: vec![],
                                math: vec![],
                            }),
                        ),
                    );
                    reader
                });
                reader = Some(view.clone());
                Root::new(view, window, cx)
            });
            let reader = reader.unwrap();
            settle(cx);
            assert!(cx.debug_bounds("pdf-footnotes-toggle-0").is_none());
            reader.read_with(cx, |reader, _| {
                let html = reader.reader_layout_html(0, reader.primary_original());
                assert!(html.contains(if reader.primary_original() {
                    "explanatory footnote"
                } else {
                    "解释性脚注"
                }));
                assert_eq!(reader.state.paper.blocks[1].id, "page-two");
                assert_eq!(reader.raw, original);
            });
            cx.update(|window, cx| {
                reader.update(cx, |reader, cx| {
                    let paragraph = if mode == ReadingMode::Bilingual { 2 } else { 1 };
                    reader.active_reading_paragraph =
                        Some((0, reader.primary_original(), paragraph));
                    reader.begin_inline_edit(0, window, cx);
                    let edit = reader
                        .inline_edit
                        .as_ref()
                        .expect("Inline footnote must remain editable");
                    assert!(
                        edit.plan.text.contains("解释性脚注"),
                        "Must edit the translation, not body/source: {}",
                        edit.plan.text
                    );
                    reader.cancel_inline_edit(window, cx);
                });
            });
            reader.read_with(cx, |reader, _| {
                assert_eq!(reader.raw, original);
            });
        }
    }
}

#[gpui_kit::test]
fn long_pdf_margin_reply_does_not_push_next_page_away_from_body(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (raw, source) = fixture(true);
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut reader = ReaderView::new(raw, None, None, window, cx);
            reader.reading_mode = ReadingMode::Bilingual;
            reader.pdf_code_html.insert(
                0,
                (
                    reader.state.paper.blocks[0].en.clone(),
                    Some(CodePresentation {
                        html: source,
                        inline_figures: vec![],
                        figures: vec![],
                        math: vec![],
                    }),
                ),
            );
            reader
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    settle(cx);
    assert!(
        cx.debug_bounds("annotation-ai-reply").is_some(),
        "The long reply must actually render"
    );
    assert!(cx.debug_bounds("pdf-margin-notes-0").is_some());
    assert!(
        cx.debug_bounds("pdf-margin-scroll-0").is_none(),
        "Personal notes must not share the answer's clipped viewport"
    );
    let margin = cx
        .debug_bounds("annotation-ai-body")
        .expect("Only the long AI answer has a scroll viewport");
    reader.read_with(cx, |reader, cx| {
        let text = reader.text_views[&(0, reader.primary_original())]
            .1
            .read(cx)
            .bounds();
        let row = reader.block_bounds[&0];
        assert!(
            margin.size.height <= text.size.height + px(33.),
            "Margin must follow body height: {margin:?} vs {text:?}"
        );
        assert!(
            row.size.height <= text.size.height + px(110.),
            "AI replies must not create a blank page: {row:?} vs {text:?}; answer={margin:?}; budget={:?}", reader.pdf_margin_height(0, cx)
        );
        let next = reader
            .block_bounds
            .get(&1)
            .expect("The next source page must still be painted");
        assert!(
            next.top() - row.bottom() < px(2.),
            "No artificial page gap: {row:?} vs {next:?}"
        );
        assert!(
            reader.raw["discussion"]["entries"][0]["body"]
                .as_str()
                .unwrap()
                .len()
                > 3000
        );
    });
}
