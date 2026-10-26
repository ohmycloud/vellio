//! Native paragraph-local editing, persistence and cancel semantics.
use super::{ReaderView, ReadingMode, Workspace};
use easyread_gpui::store::write_json;
use gpui_kit::{AppContext, Modifiers, TestAppContext, VisualTestContext, component::Root};
use serde_json::json;

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..24 {
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.draw(cx).clear(cx);
        });
    }
}

#[gpui_kit::test]
fn paragraph_edit_is_inline_cancel_discards_and_save_changes_only_target(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let temp = tempfile::tempdir().unwrap();
    let ws = Workspace {
        root: temp.path().to_owned(),
    };
    let raw = json!({"paper":{"meta":{"format":"pdf","title_en":"Inline edit QA"},"blocks":[
        {"id":"page","type":"para","page":7,"en":"First paragraph.\n\nRepeated formula.\n\nRepeated formula.\n\nFinal paragraph.",
        "zh":"首段保持。\n\n**重复段落** $d_k$。\n\n**重复段落** $d_k$。\n\n末段保持。"}
    ]},"reader":{"notes":{"saved":{"id":"saved","anchor":"page","quote":"首段保持。","kind":"highlight","color":"yellow"}}}});
    write_json(&ws.root.join("paper.json"), &raw["paper"]).unwrap();
    write_json(&ws.root.join("reader.json"), &raw["reader"]).unwrap();
    let original = std::fs::read(ws.root.join("paper.json")).unwrap();
    let old_reader = std::fs::read(ws.root.join("reader.json")).unwrap();
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(raw, None, Some(ws.clone()), window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    reader.update(cx, |r, cx| r.set_reading_mode(ReadingMode::Translation, cx));
    for _ in 0..200 {
        std::thread::sleep(std::time::Duration::from_millis(10));
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(100));
        settle(cx);
        if reader.read_with(cx, |r, _| !r.busy) {
            break;
        }
    }
    reader.read_with(cx, |r, _| assert!(!r.busy));
    let target = reader.read_with(cx, |r, cx| {
        r.text_views[&(0, false)]
            .1
            .read(cx)
            .paragraph_bounds(2)
            .unwrap()
    });
    cx.simulate_click(target.center(), Modifiers::default());
    settle(cx);
    let edit = cx.debug_bounds("selection-edit-translation").unwrap();
    cx.simulate_click(edit.center(), Modifiers::default());
    settle(cx);
    reader.read_with(cx, |r, _| assert!(r.inline_edit.is_some(), "{}", r.status));
    let bounds = cx
        .debug_bounds("inline-translation-editor")
        .expect("in-place editor");
    assert!((bounds.top() - target.top()).abs() < gpui_kit::px(35.));
    reader.read_with(cx, |r, cx| {
        assert!(!r.panel_open);
        assert_eq!(
            r.inline_edit
                .as_ref()
                .unwrap()
                .input
                .read(cx)
                .value()
                .as_str(),
            "**重复段落** $d_k$。"
        );
    });
    let input = reader.read_with(cx, |r, _| r.inline_edit.as_ref().unwrap().input.clone());
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.set_value("不应保存的草稿。", window, cx)
        })
    });
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert!(cx.debug_bounds("inline-translation-editor").is_none());
    assert_eq!(
        std::fs::read(ws.root.join("reader.json")).unwrap(),
        old_reader
    );
    let target = reader.read_with(cx, |r, cx| {
        r.text_views[&(0, false)]
            .1
            .read(cx)
            .paragraph_bounds(2)
            .unwrap()
    });
    cx.simulate_click(target.center(), Modifiers::default());
    settle(cx);
    let edit = cx.debug_bounds("selection-edit-translation").unwrap();
    cx.simulate_click(edit.center(), Modifiers::default());
    settle(cx);
    let input = reader.read_with(cx, |r, _| r.inline_edit.as_ref().unwrap().input.clone());
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.set_value("**修订段落** $d_v$。", window, cx)
        })
    });
    cx.simulate_keystrokes("ctrl-enter");
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(10));
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(100));
        settle(cx);
        if reader.read_with(cx, |r, _| r.inline_edit.is_none()) {
            break;
        }
    }
    reader.read_with(cx, |r, _| {
        assert!(
            r.inline_edit.is_none(),
            "successful save closes inline editor"
        );
        assert!(!r.panel_open);
        assert_eq!(
            r.text_at(0),
            "首段保持。\n\n**重复段落** $d_k$。\n\n**修订段落** $d_v$。\n\n末段保持。"
        );
        assert!(
            r.reader_language_layout_html(0, false)
                .contains("data-vellio-math=\"d_v\"")
        );
    });
    assert_eq!(std::fs::read(ws.root.join("paper.json")).unwrap(), original);
    let persisted = ws.state().unwrap();
    assert_eq!(persisted["reader"]["notes"]["saved"]["quote"], "首段保持。");
    assert!(
        persisted["reader"]["edits"]["page"]["native_layout"]["html"]
            .as_str()
            .unwrap()
            .contains("修订段落")
    );
}

#[gpui_kit::test]
fn bilingual_and_original_mode_edit_the_matching_translation_not_original(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    for (mode, original_first, paragraph) in [
        (ReadingMode::Bilingual, true, 3),
        (ReadingMode::Bilingual, false, 2),
        (ReadingMode::Original, true, 1),
    ] {
        let raw = json!({"paper":{"meta":{"format":"pdf"},"blocks":[{"id":"page","type":"para","en":"First source.\n\nSecond source.","zh":"第一段译文。\n\n第二段译文。"}]},"reader":{}});
        let mut reader = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| ReaderView::new(raw, None, None, window, cx));
            reader = Some(view.clone());
            Root::new(view, window, cx)
        });
        let reader = reader.unwrap();
        reader.update(cx, |r, cx| {
            r.original_first = original_first;
            r.set_reading_mode(mode, cx);
        });
        settle(cx);
        let original = mode == ReadingMode::Original || original_first;
        let target = reader.read_with(cx, |r, cx| {
            r.text_views[&(0, original)]
                .1
                .read(cx)
                .paragraph_bounds(paragraph)
                .unwrap()
        });
        cx.simulate_click(target.center(), Modifiers::default());
        settle(cx);
        let edit = cx.debug_bounds("selection-edit-translation").unwrap();
        cx.simulate_click(edit.center(), Modifiers::default());
        settle(cx);
        assert!(cx.debug_bounds("inline-translation-editor").is_some());
        reader.read_with(cx, |r, cx| {
            assert!(!r.panel_open);
            let edit = r.inline_edit.as_ref().unwrap();
            assert_eq!(edit.input.read(cx).value().as_str(), "第二段译文。");
            assert_eq!(
                edit.plan.merge("新的第二段。").unwrap().0,
                "第一段译文。\n\n新的第二段。"
            );
        });
        let cancel = cx.debug_bounds("inline-edit-cancel").unwrap();
        cx.simulate_click(cancel.center(), Modifiers::default());
        settle(cx);
        reader.read_with(cx, |r, _| {
            assert!(r.inline_edit.is_none());
            assert_eq!(
                r.state.paper.blocks[0].en,
                "First source.\n\nSecond source."
            );
            if mode == ReadingMode::Original {
                assert_eq!(r.visible_languages(0), vec![true]);
            }
        });
    }
}

#[gpui_kit::test]
fn failed_inline_save_keeps_draft_and_epub_editor_is_in_reading_flow(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let temp = tempfile::tempdir().unwrap();
    let ws = Workspace {
        root: temp.path().to_owned(),
    };
    let raw = json!({"paper":{"meta":{"format":"epub"},"blocks":[{"id":"paragraph","type":"para","source_kind":"epub-xhtml","page":0,
        "en":"Original EPUB text.","html":"<p>Original EPUB text.</p>","zh":"EPUB 原有译文。"}]},"reader":{}});
    write_json(&ws.root.join("paper.json"), &raw["paper"]).unwrap();
    write_json(&ws.root.join("reader.json"), &raw["reader"]).unwrap();
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(raw, None, Some(ws.clone()), window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    reader.update(cx, |r, cx| r.set_reading_mode(ReadingMode::Translation, cx));
    settle(cx);
    let target = reader.read_with(cx, |r, cx| {
        r.text_views[&(0, false)]
            .1
            .read(cx)
            .paragraph_bounds(0)
            .unwrap()
    });
    cx.simulate_click(target.center(), Modifiers::default());
    settle(cx);
    let edit = cx.debug_bounds("selection-edit-translation").unwrap();
    cx.simulate_click(edit.center(), Modifiers::default());
    settle(cx);
    assert!(cx.debug_bounds("inline-translation-editor").is_some());
    let input = reader.read_with(cx, |r, _| r.inline_edit.as_ref().unwrap().input.clone());
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.set_value("必须保留的草稿。", window, cx)
        })
    });
    ws.reader_ops(&[json!({"op":"edit", "block":"paragraph", "zh":"其他窗口的修订。"})])
        .unwrap();
    let save = cx.debug_bounds("inline-edit-save").unwrap();
    cx.simulate_click(save.center(), Modifiers::default());
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(10));
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(100));
        settle(cx);
        if reader.read_with(cx, |r, _| !r.busy) {
            break;
        }
    }
    reader.read_with(cx, |r, cx| {
        let edit = r
            .inline_edit
            .as_ref()
            .expect("failed save must not close editor");
        assert!(!edit.saving);
        assert!(edit.error.is_some());
        assert_eq!(edit.input.read(cx).value().as_str(), "必须保留的草稿。");
        assert!(!r.panel_open);
        assert!(r.epub_reflow.complete);
    });
    assert_eq!(
        ws.load("reader").unwrap()["edits"]["paragraph"]["zh"],
        "其他窗口的修订。"
    );
}
