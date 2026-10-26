//! Real GPUI-window regressions for saved margin questions and AI answers.
use super::{ReaderView, ReadingMode};
use gpui_kit::{AppContext, TestAppContext, VisualTestContext, component::Root, point, px};
use serde_json::json;
fn settle(cx: &mut VisualTestContext) {
    for _ in 0..20 {
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.draw(cx).clear(cx);
        });
    }
}
fn wheel(cx: &mut VisualTestContext, position: gpui_kit::Point<gpui_kit::Pixels>, delta: f32) {
    cx.simulate_event(gpui_kit::ScrollWheelEvent {
        position,
        delta: gpui_kit::ScrollDelta::Pixels(point(px(0.), px(delta))),
        ..Default::default()
    });
    settle(cx);
}
fn fixture() -> serde_json::Value {
    let mut blocks = vec![
        json!({"id":"p","type":"para","source_kind":"pdf-text","page":1,"en":"We explain attention and keep questions accessible.","zh":"我们解释注意力，并保留问题与笔记。"}),
    ];
    for i in 1..10 {
        blocks.push(json!({"id":format!("p{i}"),"type":"para","page":i+1,"en":"A following source paragraph remains continuous. ".repeat(25),"zh":"后续段落正常连续显示。".repeat(25)}));
    }
    json!({"paper":{"meta":{"title_en":"Margin regression"},"blocks":blocks},"reader":{"notes":{
        "a":{"id":"a","anchor":"p","side":"zh","kind":"question","quote":"注意力","body":"为什么要对注意力进行缩放？","created":"2026-10-15T23:40:00Z"},
        "b":{"id":"b","anchor":"p","side":"zh","kind":"note","quote":"笔记","body":"笔记中的公式没有正确渲染。","created":"2026-10-15T23:41:00Z"}
    }},"discussion":{"entries":[{"id":"answer","anchor":"p","kind":"qa","by":"default","model":"gpt-actual-test","provider":"codex-cli","body":format!("**Additive attention（加性注意力）用一个小型神经网络**，给查询 $q$ 与每个键 $k_i$ 的匹配程度打分。分数经过 softmax 变成权重，再对值加权求和。\n\n{}\n\n最后的完整回答。", "中英混排的回答 **保留完整神经网络**，不应在行尾丢字。\n\n".repeat(30))}]}})
}
#[gpui_kit::test]
fn scrolling_long_answer_never_scrolls_away_questions_and_survives_row_rebuild(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let raw = fixture();
    let saved = raw.clone();
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut r = ReaderView::new(raw, None, None, window, cx);
            r.reading_mode = ReadingMode::Translation;
            r
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    settle(cx);
    let question = cx.debug_bounds("annotation-card-a").unwrap();
    let note = cx.debug_bounds("annotation-card-b").unwrap();
    let viewport = cx.debug_bounds("annotation-ai-body").unwrap();
    let content = cx.debug_bounds("annotation-ai-content").unwrap();
    assert!(content.size.height > viewport.size.height * 2.);
    assert!(content.size.width <= viewport.size.width + px(0.5));
    assert_eq!(
        question.left(),
        cx.debug_bounds("annotation-ai-reply").unwrap().left(),
        "Standalone AI and personal-note rules must align"
    );
    assert!(note.top() >= question.bottom());
    assert!(cx.debug_bounds("pdf-margin-scroll-0").is_none());
    let position = reader.read_with(cx, |r, _| r.list.logical_scroll_top());
    wheel(cx, viewport.center(), -100000.);
    let last = cx.debug_bounds("annotation-ai-content").unwrap();
    assert!(last.top() < content.top() - px(30.));
    assert!(
        last.bottom() <= viewport.bottom() + px(1.),
        "The final paragraph must be reachable: {last:?} vs {viewport:?}"
    );
    assert_eq!(cx.debug_bounds("annotation-card-a").unwrap(), question);
    assert_eq!(cx.debug_bounds("annotation-card-b").unwrap(), note);
    reader.read_with(cx, |r, _| {
        assert_eq!(r.list.logical_scroll_top().item_ix, position.item_ix)
    });
    reader.update(cx, |r, cx| {
        r.reset_rows();
        cx.notify();
    });
    settle(cx);
    assert!(
        (cx.debug_bounds("annotation-ai-content").unwrap().top() - last.top()).abs() < px(1.),
        "Virtualized-row rebuild must keep answer scroll offset"
    );
    wheel(cx, viewport.center(), 100000.);
    assert!(
        (cx.debug_bounds("annotation-ai-content").unwrap().top() - content.top()).abs() < px(1.)
    );
    // A real document-wheel path, away from the answer viewport, and return.
    wheel(cx, point(px(100.), px(300.)), -700.);
    wheel(cx, point(px(100.), px(300.)), 100000.);
    assert!(cx.debug_bounds("annotation-card-a").is_some());
    assert!(cx.debug_bounds("annotation-card-b").is_some());
    reader.read_with(cx, |r, _| assert_eq!(r.raw, saved));
    // Short, non-overflowing answers must not create a dead wheel zone.
    reader.update(cx, |r, cx| {
        r.raw["discussion"]["entries"][0]["body"] = json!("简短回答。");
        r.reset_rows();
        cx.notify();
    });
    settle(cx);
    assert_eq!(
        reader.read_with(cx, |r, _| r.discussion_scrolls.borrow()["answer"]
            .max_offset()
            .y),
        px(0.)
    );
    let before = reader.read_with(cx, |r, _| r.list.logical_scroll_top());
    let short_viewport = cx.debug_bounds("annotation-ai-body").unwrap();
    wheel(cx, short_viewport.center(), -150.);
    let after = reader.read_with(cx, |r, _| r.list.logical_scroll_top());
    assert!(
        after.item_ix != before.item_ix || after.offset_in_item != before.offset_in_item,
        "A fitting answer must pass the document-wheel gesture through"
    );
}
#[gpui_kit::test]
fn mixed_bold_cjk_answer_wraps_to_column_and_keeps_last_paragraph_selectable(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    for drawer in [false, true] {
        let raw = fixture();
        let mut reader = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| {
                let mut r = ReaderView::new(raw, None, None, window, cx);
                r.panel = super::Panel::Notes;
                r.panel_open = drawer;
                r
            });
            reader = Some(view.clone());
            Root::new(view, window, cx)
        });
        let reader = reader.unwrap();
        for _ in 0..100 {
            settle(cx);
            if reader.read_with(cx, |r, cx| {
                r.rich_markdown_cache
                    .borrow()
                    .reply_views
                    .get("reply-body-answer")
                    .is_some_and(|v| v.read(cx).is_content_ready())
            }) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let viewport = cx.debug_bounds("annotation-ai-body").unwrap();
        if !drawer {
            let copy = cx.debug_bounds("annotation-ai-copy").unwrap();
            cx.simulate_click(copy.center(), gpui_kit::Modifiers::default());
            cx.update(|_, cx| {
                let copied = cx.read_from_clipboard().unwrap().text().unwrap();
                reader.read_with(cx, |r, _| {
                    assert_eq!(
                        copied,
                        r.raw["discussion"]["entries"][0]["body"].as_str().unwrap()
                    )
                });
            });
        }
        cx.update(|_, cx| {
            reader.update(cx, |r, cx| {
                let cache = r.rich_markdown_cache.borrow();
                let state = &cache.reply_views["reply-body-answer"];
                state.update(cx, |v, cx| {
                    assert!(v.bounds().size.width <= viewport.size.width + px(0.5));
                    v.select_all(cx);
                    let text = v.selected_text();
                    assert!(text.contains("完整神经网络"));
                    assert!(text.contains("最后的完整回答"));
                });
            })
        });
    }
}
