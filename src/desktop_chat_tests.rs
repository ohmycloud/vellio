//! EasyRead Ask AI: slide geometry, context chips and automatic note replies.
use super::{Panel, ReaderView, chat};
use easyread_gpui::{
    ai,
    store::{Workspace, write_json},
};
use gpui_kit::{
    AppContext, Entity, Focusable, Modifiers, TestAppContext, VisualTestContext, component::Root,
    px, size,
};
use serde_json::{Value, json};
use std::time::Duration;

fn fixture() -> Value {
    json!({"paper":{"meta":{"title_en":"Ask AI reader regression"},"blocks":[
        {"id":"paragraph","type":"para","en":"A first paragraph for reading.\n\nA second paragraph with an explanation and a question.\n\nA final paragraph that remains in the reader."},
        {"id":"other","type":"para","en":"Another reference to discuss."}
    ]},"reader":{"notes":{"note":{"id":"note","anchor":"paragraph","key":"paragraph",
        "quote":"A second paragraph with an explanation and a question.","kind":"question","answered":false,
        "body":"Why does this work?","created":"2020-01-01T00:00:00Z","updated":"2020-01-01T00:00:00Z"}}},
        "chat":{"threads":[]},"discussion":{"entries":[]}})
}
fn frame(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        window.draw(cx).clear(cx);
    });
}
fn settle(cx: &mut VisualTestContext) {
    for _ in 0..8 {
        frame(cx);
    }
}
fn finish_slide(cx: &mut VisualTestContext) {
    std::thread::sleep(Duration::from_millis(240));
    cx.executor().advance_clock(Duration::from_millis(310));
    settle(cx);
}
fn wait_for_worker(cx: &mut VisualTestContext, reader: &Entity<ReaderView>) {
    for _ in 0..150 {
        std::thread::sleep(Duration::from_millis(5));
        cx.executor().advance_clock(Duration::from_millis(80));
        settle(cx);
        if !reader.read_with(cx, |reader, _| reader.busy) {
            return;
        }
    }
    panic!("explicit mock chat request did not finish");
}

#[gpui_kit::test]
fn ask_ai_slides_from_right_and_uses_easyread_column_widths(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(fixture(), None, None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    cx.simulate_resize(size(px(1300.), px(800.)));
    settle(cx);
    let before = cx.debug_bounds("reader-column").unwrap();
    let offset = reader.read_with(cx, |reader, _| reader.list.logical_scroll_top());
    cx.update(|window, cx| {
        reader.update(cx, |reader, cx| {
            reader.ask_annotation_ai("note", window, cx)
        })
    });
    frame(cx);
    let first = cx.debug_bounds("chat-panel").unwrap();
    let slot = cx.debug_bounds("chat-column").unwrap();
    assert!((slot.size.width - px(572.)).abs() < px(1.));
    assert!(
        first.left() > slot.left() + px(100.),
        "panel must begin to the right of its final column"
    );
    std::thread::sleep(Duration::from_millis(70));
    frame(cx);
    let middle = cx.debug_bounds("chat-panel").unwrap();
    assert!(middle.left() < first.left());
    assert!(middle.left() > slot.left());
    finish_slide(cx);
    let panel = cx.debug_bounds("chat-panel").unwrap();
    let left = cx.debug_bounds("reader-column").unwrap();
    assert!((panel.left() - slot.left()).abs() < px(1.));
    assert!((left.right() - panel.left()).abs() < px(1.));
    assert!((left.size.width + panel.size.width - before.size.width).abs() < px(1.));
    let header = cx.debug_bounds("chat-header").unwrap();
    assert_eq!(header.size.height, px(50.));
    let composer = cx.debug_bounds("chat-composer").unwrap();
    assert!((panel.bottom() - composer.bottom() - px(12.)).abs() < px(2.));
    assert!(composer.size.width <= panel.size.width - px(24.));
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.list.logical_scroll_top().item_ix, offset.item_ix);
        assert_eq!(reader.chat_ui.refs.len(), 1);
        assert_eq!(
            reader.chat_ui.refs[0].quote,
            "A second paragraph with an explanation and a question."
        );
    });
    assert!(cx.debug_bounds("chat-reference").is_some());
    let grip = cx.debug_bounds("chat-panel-grip").unwrap();
    cx.simulate_mouse_down(
        grip.center(),
        gpui_kit::MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(
        gpui_kit::point(px(650.), grip.center().y),
        gpui_kit::MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_up(
        gpui_kit::point(px(650.), grip.center().y),
        gpui_kit::MouseButton::Left,
        Modifiers::default(),
    );
    settle(cx);
    assert!((cx.debug_bounds("chat-column").unwrap().size.width - px(650.)).abs() < px(1.));
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.settings.committed.extra["side_panel_width"], 650.);
        assert!(!reader.chat_ui.resizing);
    });
    let close = cx.debug_bounds("chat-close").unwrap();
    cx.simulate_click(close.center(), Modifiers::default());
    frame(cx);
    assert!(reader.read_with(cx, |reader, _| reader.chat_ui.closing));
    finish_slide(cx);
    assert!(cx.debug_bounds("chat-column").is_none());
    assert_eq!(
        cx.debug_bounds("reader-column").unwrap().size.width,
        before.size.width
    );

    cx.simulate_resize(size(px(900.), px(800.)));
    settle(cx);
    cx.update(|window, cx| {
        reader.update(cx, |reader, cx| reader.show_panel(Panel::Chat, window, cx))
    });
    finish_slide(cx);
    assert_eq!(cx.debug_bounds("chat-column").unwrap().size.width, px(600.));
    assert_eq!(
        cx.debug_bounds("reader-column").unwrap().size.width,
        px(900.),
        "narrow screens overlay instead of squeezing text"
    );
    assert!(cx.debug_bounds("chat-panel-grip").is_none());
}

#[gpui_kit::test]
fn stored_chat_display_and_inline_math_paint_as_native_paths(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let display =
        r"\text{Attention}(Q,K,V)=\text{softmax}\!\left(\frac{QK^{\top}}{\sqrt{d_k}}\right)V";
    let inline = r"\sqrt{d_{837}}";
    let body = format!(
        "原文写成矩阵形式：\n$${display}$$\n每个分数除以 \\({inline}\\)。\n\n代码 `$$example$$` 应保留。"
    );
    let mut raw = fixture();
    raw["chat"]["threads"] = json!([{"id":"math-thread","messages":[{"id":"math-answer","role":"assistant","content":body,"status":"done"}]}]);
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(raw, None, None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    cx.simulate_resize(size(px(1300.), px(900.)));
    cx.update(|window, cx| {
        reader.update(cx, |reader, cx| reader.show_panel(Panel::Chat, window, cx))
    });
    finish_slide(cx);
    for _ in 0..150 {
        std::thread::sleep(Duration::from_millis(5));
        settle(cx);
        if easyread_gpui::formula::cached_layout(display, true).is_some()
            && easyread_gpui::formula::cached_layout(inline, false).is_some()
            && cx.debug_bounds("reader-native-typst-math").is_some()
        {
            break;
        }
    }
    assert!(easyread_gpui::formula::cached_layout(display, true).is_some());
    assert!(easyread_gpui::formula::cached_layout(inline, false).is_some());
    let bounds = cx
        .debug_bounds("reader-native-typst-math")
        .expect("AI math must paint vector paths, not literal TeX");
    assert!(bounds.size.width <= cx.debug_bounds("chat-panel").unwrap().size.width - px(60.));
    reader.read_with(cx, |reader, _| {
        assert_eq!(
            reader.raw["chat"]["threads"][0]["messages"][0]["content"],
            body
        )
    });
}

#[gpui_kit::test]
fn streamed_display_math_waits_for_closing_delimiter_then_renders(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let tex = "x^{947}+y^{947}";
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(fixture(), None, None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    cx.update(|window, cx| {
        reader.update(cx, |reader, cx| {
            reader.live_question = "解释此公式".into();
            reader.live = format!("矩阵：\n$${tex}");
            reader.show_panel(Panel::Chat, window, cx);
        })
    });
    finish_slide(cx);
    assert!(easyread_gpui::formula::cached_layout(tex, true).is_none());
    assert!(cx.debug_bounds("reader-native-typst-math").is_none());
    reader.update(cx, |reader, cx| {
        reader.live = format!("矩阵：\n$${tex}$$\n后文仍在。");
        cx.notify();
    });
    for _ in 0..150 {
        std::thread::sleep(Duration::from_millis(5));
        settle(cx);
        if cx.debug_bounds("reader-native-typst-math").is_some() {
            break;
        }
    }
    assert!(easyread_gpui::formula::cached_layout(tex, true).is_some());
    assert!(cx.debug_bounds("reader-native-typst-math").is_some());
    reader.read_with(cx, |reader, _| {
        assert_eq!(
            reader.raw["chat"]["threads"],
            json!([]),
            "rendering live math must not rewrite stored chat"
        )
    });
}

#[gpui_kit::test]
fn pinned_ai_reply_uses_the_same_native_math_renderer(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let tex = r"\frac{a_{579}}{b_{579}}";
    let body = format!("解答：\\[{tex}\\]。");
    let mut raw = fixture();
    raw["discussion"]["entries"] = json!([{"id":"math-reply","reply_to":"note","anchor":"paragraph","body":body,"by":"test-model","live":true}]);
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(raw, None, None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    cx.update(|window, cx| {
        reader.update(cx, |reader, cx| reader.show_panel(Panel::Notes, window, cx))
    });
    for _ in 0..150 {
        std::thread::sleep(Duration::from_millis(5));
        settle(cx);
        if cx.debug_bounds("reader-native-typst-math").is_some() {
            break;
        }
    }
    assert!(easyread_gpui::formula::cached_layout(tex, true).is_some());
    assert!(cx.debug_bounds("annotation-ai-reply").is_some());
    assert!(cx.debug_bounds("reader-native-typst-math").is_some());
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.raw["discussion"]["entries"][0]["body"], body)
    });
}

fn check_annotation_math(cx: &mut TestAppContext, drawer: bool) {
    cx.update(gpui_kit::init);
    for kind in ["note", "question"] {
        for in_quote in [true, false] {
            let tex = if in_quote {
                r"\frac{1}{\sqrt{d_{k}}}"
            } else {
                r"\frac{QK^{\top}}{\sqrt{d_k}}"
            };
            let mut raw = fixture();
            raw["reader"]["notes"]["note"]["kind"] = json!(kind);
            raw["reader"]["notes"]["note"]["quote"] = json!(if in_quote {
                format!("${tex}$")
            } else {
                "A second paragraph with an explanation and a question.".into()
            });
            raw["reader"]["notes"]["note"]["body"] = json!(if in_quote {
                "笔记中的公式应正确渲染。".into()
            } else {
                format!("**公式**：\n$${tex}$$\n代码 `$not_math$` 应保持原样。")
            });
            let saved_note = raw["reader"]["notes"]["note"].clone();
            let mut reader = None;
            let (_, cx) = cx.add_window_view(|window, cx| {
                let view = cx.new(|cx| {
                    let mut reader = ReaderView::new(raw, None, None, window, cx);
                    reader.panel = Panel::Notes;
                    reader.panel_open = drawer;
                    reader
                });
                reader = Some(view.clone());
                Root::new(view, window, cx)
            });
            let reader = reader.unwrap();
            for _ in 0..150 {
                std::thread::sleep(Duration::from_millis(5));
                settle(cx);
                if cx.debug_bounds("reader-native-typst-math").is_some() {
                    break;
                }
            }
            let math = cx
                .debug_bounds("reader-native-typst-math")
                .expect("saved notes and questions must paint native math");
            let container = cx
                .debug_bounds(if in_quote {
                    "annotation-card-quote"
                } else {
                    "annotation-card-body"
                })
                .unwrap();
            assert!(container.contains(&math.center()));
            assert!(math.size.width > px(0.) && math.size.width <= container.size.width);
            assert!(easyread_gpui::formula::cached_layout(tex, !in_quote).is_some());
            assert!(easyread_gpui::formula::cached_layout("not_math", false).is_none());
            reader.read_with(cx, |reader, _| {
                assert_eq!(reader.raw["reader"]["notes"]["note"], saved_note);
            });
        }
    }
}

#[gpui_kit::test]
fn margin_notes_and_questions_render_quote_and_body_math(cx: &mut TestAppContext) {
    check_annotation_math(cx, false);
}

#[gpui_kit::test]
fn drawer_notes_and_questions_render_quote_and_body_math(cx: &mut TestAppContext) {
    check_annotation_math(cx, true);
}

fn check_annotation_code_highlighting(cx: &mut TestAppContext, drawer: bool) {
    cx.update(gpui_kit::init);
    for (language, source) in [
        ("python", "# 注释\ndef multiply(a, b):\n    return a * b\n"),
        ("js", "function hello() { return \"中文\"; }\n"),
        ("rust", "fn main() { let count = 42; }\n"),
        ("json", "{\"name\": \"中文\", \"count\": 42}\n"),
        ("", "# Register tool\ntools = Tools()\ntools.add_tool(\n    name=\"multiply\",\n    func=multiply,\n    description=\"两个数相乘\"\n)\n"),
    ] {
        let mut raw = fixture();
        let body = format!("代码示例：\n\n```{language}\n{source}```\n\n保留正文与 `inline_code`。");
        raw["reader"]["notes"]["note"]["body"] = json!(body);
        let saved_note = raw["reader"]["notes"]["note"].clone();
        let mut reader = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| {
                let mut reader = ReaderView::new(raw, None, None, window, cx);
                reader.panel = Panel::Notes;
                reader.panel_open = drawer;
                reader
            });
            reader = Some(view.clone());
            Root::new(view, window, cx)
        });
        let reader = reader.unwrap();
        let observed = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = observed.clone();
        cx.update(|_, cx| {
            // Simulate defaults being replaced by a theme change. The note's
            // rich-text entry point must restore highlighting on its own.
            cx.set_global(gpui_kit::base::TextViewDefaults::new());
            reader.update(cx, |reader, cx| {
                let _ = reader.rich_markdown("note-code-probe".into(), &body, 240., cx);
            });
            assert!(gpui_kit::base::TextViewDefaults::global(cx).has_code_block_highlighter());
            let defaults = gpui_kit::base::TextViewDefaults::global(cx)
                .clone()
                .with_code_block_highlighter(move |block| {
                    let runs = super::code::highlight(block);
                    let multicolor = runs.first().is_some_and(|first| {
                        runs.iter().any(|(_, style)| style.color != first.1.color)
                    });
                    captured.lock().unwrap().push((
                        block.lang().map(|lang| lang.to_string()),
                        block.code().to_string(),
                        multicolor,
                    ));
                    runs
                });
            cx.set_global(defaults);
        });
        settle(cx);
        assert!(cx.debug_bounds("annotation-card-body").is_some());
        assert!(
            observed.lock().unwrap().iter().any(|(lang, code, colored)| {
                lang.as_deref() == (!language.is_empty()).then_some(language)
                    && code == source
                    && *colored
            }),
            "saved note code must reach the native multicolor highlighter: {language:?}"
        );
        reader.read_with(cx, |reader, _| {
            assert_eq!(reader.raw["reader"]["notes"]["note"], saved_note);
        });
    }
}

#[gpui_kit::test]
fn margin_note_code_blocks_have_syntax_highlighting(cx: &mut TestAppContext) {
    check_annotation_code_highlighting(cx, false);
}

#[gpui_kit::test]
fn drawer_note_code_blocks_have_syntax_highlighting(cx: &mut TestAppContext) {
    check_annotation_code_highlighting(cx, true);
}

fn check_note_code_horizontal_scroll(cx: &mut TestAppContext, drawer: bool) {
    cx.update(gpui_kit::init);
    let mut raw = fixture();
    raw["reader"]["notes"]["note"]["body"] = json!("```python\nprint(\"short\")\n```\n");
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut reader = ReaderView::new(raw, None, None, window, cx);
            reader.panel = Panel::Notes;
            reader.panel_open = drawer;
            reader
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    settle(cx);
    let short_height = cx.debug_bounds("text-code-viewport").unwrap().size.height;
    let long_body = format!("```python\nprint(\"{}\")\n```\n", "long 中文 code ".repeat(80));
    reader.update(cx, |reader, cx| {
        reader.raw["reader"]["notes"]["note"]["body"] = json!(long_body);
        reader.state = easyread_gpui::model::ReaderState::from_value(reader.raw.clone()).unwrap();
        reader.reset_rows();
        cx.notify();
    });
    settle(cx);
    let viewport = cx.debug_bounds("text-code-viewport").unwrap();
    let content = cx.debug_bounds("text-code-content").unwrap();
    let body = cx.debug_bounds("annotation-card-body").unwrap();
    assert!((viewport.size.height - short_height).abs() < px(1.), "long source lines must not wrap");
    assert!(viewport.size.width <= body.size.width);
    assert!(content.size.width > viewport.size.width * 2.);
    cx.update(|window, cx| {
        let old_background: gpui_kit::Background = gpui_kit::base::TextViewDefaults::global(cx)
            .style().cloned().unwrap_or_default().code_background().into();
        assert!(!window.painted_quads().iter().any(|quad| quad.background == old_background),
            "note code must not paint the theme's contrasting code background");
    });
    cx.simulate_event(gpui_kit::ScrollWheelEvent {
        position: viewport.center(),
        delta: gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(-120.), px(0.))),
        ..Default::default()
    });
    settle(cx);
    let scrolled = cx.debug_bounds("text-code-content").unwrap();
    assert!(scrolled.left() < content.left() - px(50.), "horizontal gestures must reveal the clipped code");
    assert_eq!(cx.debug_bounds("text-code-viewport").unwrap(), viewport);
    assert_eq!(scrolled.size.height, content.size.height);
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.raw["reader"]["notes"]["note"]["body"], long_body);
    });
}

#[gpui_kit::test]
fn margin_note_code_uses_transparent_background_and_invisible_horizontal_scroll(cx: &mut TestAppContext) {
    check_note_code_horizontal_scroll(cx, false);
}

#[gpui_kit::test]
fn drawer_note_code_uses_transparent_background_and_invisible_horizontal_scroll(cx: &mut TestAppContext) {
    check_note_code_horizontal_scroll(cx, true);
}

#[gpui_kit::test]
fn chat_references_are_deduplicated_removable_and_do_not_pollute_the_draft(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_kit::init);
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(fixture(), None, None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    settle(cx);
    cx.update(|window, cx| {
        reader.update(cx, |reader, cx| {
            reader.open_chat(window, cx);
            reader.add_chat_reference(ai::ChatReference {
                anchor: "paragraph".into(),
                quote: String::new(),
            });
            reader.add_chat_reference(ai::ChatReference {
                anchor: "paragraph".into(),
                quote: "A second paragraph".into(),
            });
            reader.add_chat_reference(ai::ChatReference {
                anchor: "paragraph".into(),
                quote: "A second paragraph".into(),
            });
            reader.add_chat_reference(ai::ChatReference {
                anchor: "other".into(),
                quote: "Another reference".into(),
            });
            reader.question.update(cx, |input, cx| {
                input.set_value("My unsent question", window, cx)
            });
        })
    });
    finish_slide(cx);
    reader.read_with(cx, |reader, cx| {
        assert_eq!(reader.chat_ui.refs.len(), 2);
        assert_eq!(
            reader.question.read(cx).value().as_str(),
            "My unsent question"
        );
    });
    let model = cx.debug_bounds("chat-model").unwrap();
    cx.simulate_click(model.center(), Modifiers::default());
    settle(cx);
    cx.update(|_, cx| {
        reader.update(cx, |reader, _| {
            reader.chat_ui.reasoning = Some("low".into())
        })
    });
    let style = cx.debug_bounds("chat-style").unwrap();
    cx.simulate_click(style.center(), Modifiers::default());
    settle(cx);
    reader.read_with(cx, |reader, _| {
        assert!(reader.chat_ui.style_open);
        assert!(!reader.chat_ui.model_open);
    });
    let new = cx.debug_bounds("chat-new").unwrap();
    cx.simulate_click(new.center(), Modifiers::default());
    settle(cx);
    reader.read_with(cx, |reader, cx| {
        assert!(reader.chat_ui.refs.is_empty());
        assert!(
            reader.chat_ui.reasoning.is_none(),
            "new conversations must not inherit a previous effort override"
        );
        assert!(reader.question.read(cx).value().is_empty());
    });
}

#[cfg(unix)]
#[gpui_kit::test]
fn note_button_sends_immediately_and_reply_persists_then_answered_question_opens_followup(
    cx: &mut TestAppContext,
) {
    use std::os::unix::fs::PermissionsExt;
    cx.update(gpui_kit::init);
    let temp = tempfile::tempdir().unwrap();
    let ws = Workspace {
        root: temp.path().to_owned(),
    };
    let raw = fixture();
    write_json(&ws.root.join("paper.json"), &raw["paper"]).unwrap();
    write_json(&ws.root.join("reader.json"), &raw["reader"]).unwrap();
    let executable = ws.root.join("explicit-chat-mock.sh");
    let args_file = ws.root.join("mock-args.txt");
    std::fs::write(&executable,format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\ndd of=/dev/null bs=4096 2>/dev/null\nsleep 0.08\nprintf '%s' '{{\"result\":\"A native test answer.\"}}'\n",args_file.display())).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut reader = ReaderView::new(raw, None, Some(ws.clone()), window, cx);
            reader.settings.committed.models = vec![ai::ModelProfile {
                id: "explicit-chat-mock".into(),
                name: "Explicit mock".into(),
                model: "test-model".into(),
                provider: ai::Provider::ClaudeCode,
                executable: executable.to_string_lossy().into_owned(),
                ..Default::default()
            }];
            reader.settings.committed.ask_model = "explicit-chat-mock".into();
            reader
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    settle(cx);
    let ai = cx.debug_bounds("annotation-ai-answer").unwrap();
    cx.simulate_click(ai.center(), Modifiers::default());
    reader.read_with(cx, |reader, cx| {
        assert!(reader.chat_ui.streaming);
        assert!(reader.busy);
        assert_eq!(reader.live_question, "Why does this work?");
        assert!(reader.question.read(cx).value().is_empty());
    });
    wait_for_worker(cx, &reader);
    finish_slide(cx);
    let chat = ws.load("chat").unwrap();
    assert_eq!(chat["threads"][0]["messages"][0]["note"], "note");
    assert_eq!(
        chat["threads"][0]["messages"][1]["content"],
        "A native test answer."
    );
    assert_eq!(
        ws.load("reader").unwrap()["notes"]["note"]["answered"],
        true
    );
    assert_eq!(
        ws.load("discussion").unwrap()["entries"][0]["reply_to"],
        "note"
    );
    assert_eq!(
        ws.load("discussion").unwrap()["entries"][0]["body"],
        "A native test answer."
    );
    cx.update(|window, cx| reader.update(cx, |reader, cx| reader.close_chat(window, cx)));
    finish_slide(cx);
    assert!(cx.debug_bounds("annotation-answered").is_some());
    assert!(cx.debug_bounds("annotation-ai-reply").is_some());
    cx.update(|window, cx| {
        reader.update(cx, |reader, cx| {
            reader.ask_annotation_ai("note", window, cx)
        })
    });
    finish_slide(cx);
    reader.read_with(cx, |reader, cx| {
        assert!(!reader.chat_ui.streaming);
        assert!(!reader.busy);
        assert!(reader.question.read(cx).value().is_empty());
        assert_eq!(reader.chat_ui.refs.len(), 1);
    });
    assert_eq!(
        ws.load("chat").unwrap(),
        chat,
        "followup must not repeat the completed question"
    );
    let model = cx.debug_bounds("chat-model").unwrap();
    cx.simulate_click(model.center(), Modifiers::default());
    settle(cx);
    let effort = cx.debug_bounds("chat-effort-xhigh").unwrap();
    cx.simulate_click(effort.center(), Modifiers::default());
    settle(cx);
    cx.simulate_keystrokes("escape");
    settle(cx);
    let style = cx.debug_bounds("chat-style").unwrap();
    cx.simulate_click(style.center(), Modifiers::default());
    settle(cx);
    let close_reading = cx.debug_bounds("chat-style-option-2").unwrap();
    cx.simulate_click(close_reading.center(), Modifiers::default());
    settle(cx);
    cx.update(|window, cx| {
        reader.update(cx, |reader, cx| {
            reader.question.update(cx, |input, cx| {
                input.set_value("A follow-up question", window, cx)
            });
            reader.question.read(cx).focus_handle(cx).focus(window, cx);
        })
    });
    cx.simulate_keystrokes("shift-enter");
    settle(cx);
    reader.read_with(cx, |reader, cx| {
        assert!(!reader.chat_ui.streaming);
        assert!(reader.question.read(cx).value().contains('\n'));
    });
    cx.simulate_keystrokes("enter");
    settle(cx);
    wait_for_worker(cx, &reader);
    let chat = ws.load("chat").unwrap();
    let messages = chat["threads"][0]["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 4);
    assert_eq!(messages[2]["content"], "A follow-up question");
    assert_eq!(chat["threads"][0]["answer_style"], "xray");
    assert_eq!(messages[2]["answer_style"], "xray");
    assert!(messages[2]["note"].is_null());
    assert_eq!(
        chat["threads"][0]["chat_options"]["reasoning_effort"],
        "xhigh"
    );
    let args = std::fs::read_to_string(args_file).unwrap();
    assert!(
        args.contains("--effort\nxhigh\n"),
        "the conversation override must reach the CLI"
    );
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.settings.committed.models[0].reasoning, "");
    });
    let tid = reader.read_with(cx, |reader, _| reader.tid.clone());
    cx.update(|_, cx| {
        reader.update(cx, |reader, cx| {
            reader.chat_ui.reasoning = None;
            reader.select_chat_thread(&tid, cx);
        })
    });
    assert_eq!(
        reader.read_with(cx, |reader, _| reader.chat_ui.reasoning.clone()),
        Some("xhigh".into())
    );
    assert_eq!(
        ws.load("discussion").unwrap()["entries"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn chat_slide_matches_easyread_curve() {
    assert_eq!(chat::slide_ease(0.), 0.);
    assert_eq!(chat::slide_ease(1.), 1.);
    assert!(chat::slide_ease(0.5) > 0.9);
    let mut previous = 0.;
    for i in 0..=100 {
        let value = chat::slide_ease(i as f32 / 100.);
        assert!(value >= previous);
        previous = value;
    }
}
