//! Headless tests for the real reader's first visible margin-editor placement.
use super::{HighlightColor, ReaderView, Selection};
use gpui_kit::{
    AppContext, Focusable, Pixels, TestAppContext, VisualTestContext, component::Root, point, px,
};
use serde_json::{Value, json};

fn fixture() -> Value {
    json!({
        "paper": {
            "meta": {"title_en": "Margin placement regression"},
            "blocks": [{"id": "page-323", "type": "para", "page": 323,
                "en": "First line.\n\nSecond line.\n\ntemplates\n\nFollowing body text."}]
        },
        "reader": {"notes": {}}
    })
}

fn selection() -> Selection {
    Selection {
        block: 0,
        key: "page-323".into(),
        quote: "templates".into(),
        prefix: "Second line.\n".into(),
        suffix: "\nFollowing body text.".into(),
        position: point(px(300.), px(200.)),
        underline: false,
    }
}

fn frame(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        window.draw(cx).clear(cx);
    });
}

#[gpui_kit::test]
fn pdf_underline_recolor_keeps_the_visible_text_ready(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut reader = None;
    let (_root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut reader = ReaderView::new(fixture(), None, None, window, cx);
            reader.reading_mode = super::ReadingMode::Translation;
            reader
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    for _ in 0..3 {
        frame(cx);
    }
    assert!(cx.debug_bounds("reader-text-ready").is_some());
    let before = reader.read_with(cx, |reader, _| reader.text_views[&(0, false)].1.clone());
    let mut colored = fixture();
    colored["reader"]["notes"] = json!({"mark": {
        "id":"mark", "anchor":"page-323", "quote":"templates",
        "kind":"highlight", "style":"underline", "color":"green"
    }});
    reader.update(cx, |reader, _| reader.refresh_state(colored));
    reader.read_with(cx, |reader, cx| {
        let (source, view) = &reader.text_views[&(0, false)];
        assert_eq!(view, &before, "recolor must reuse the existing text view");
        assert!(view.read(cx).is_content_ready());
        assert_eq!(reader.list.item_count(), 1);
        assert!(!source.contains("text-decoration-color"));
    });
    frame(cx);
    assert!(cx.debug_bounds("reader-text-ready").is_some());
    reader.read_with(cx, |reader, cx| {
        let (source, view) = &reader.text_views[&(0, false)];
        assert!(source.contains("text-decoration-color:"));
        assert!(view.read(cx).is_content_ready());
    });
}

#[gpui_kit::test]
fn margin_editor_first_visible_position_is_the_measured_quote_line(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut reader = None;
    let (_root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(fixture(), None, None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    for _ in 0..3 {
        frame(cx);
    }
    cx.update(|window, cx| {
        reader.update(cx, |reader, cx| {
            reader.open_margin_note(selection(), "note", HighlightColor::Yellow, window, cx);
            assert!(
                reader
                    .margin_editor
                    .as_ref()
                    .unwrap()
                    .placement_pending
                    .is_some()
            );
        });
    });

    let mut first_top: Option<Pixels> = None;
    let mut reserved_frame = false;
    for _ in 0..8 {
        frame(cx);
        if reader.read_with(cx, |reader, _| {
            reader
                .margin_editor
                .as_ref()
                .unwrap()
                .placement_pending
                .is_some()
        }) {
            reserved_frame = true;
            assert!(
                cx.debug_bounds("margin-note").is_some(),
                "reserve editor layout"
            );
            assert!(
                cx.debug_bounds("margin-note-input").is_none(),
                "do not paint provisional editor content"
            );
        } else {
            assert!(cx.debug_bounds("margin-note-input").is_some());
            let bounds = cx
                .debug_bounds("margin-note")
                .expect("editor has been laid out");
            let offset = reader.update(cx, |reader, cx| {
                reader.note_offset(&reader.margin_editor.as_ref().unwrap().note.record, 0, cx)
            });
            assert!(
                offset > px(40.),
                "the quote must not be at the paragraph start"
            );
            if let Some(top) = first_top {
                assert!(
                    (bounds.top() - top).abs() < px(1.),
                    "visible editor jumped: {top:?} -> {:?}",
                    bounds.top()
                );
            } else {
                first_top = Some(bounds.top());
            }
        }
    }
    assert!(
        reserved_frame,
        "the initial measurement frame was not exercised"
    );
    assert!(first_top.is_some(), "editor was never revealed");
}

#[gpui_kit::test]
fn reopening_does_not_use_the_old_editors_placement_callback(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut reader = None;
    let (_root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(fixture(), None, None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    frame(cx);
    cx.update(|window, cx| {
        reader.update(cx, |reader, cx| {
            reader.open_margin_note(selection(), "note", HighlightColor::Yellow, window, cx);
        });
    });
    frame(cx); // The old editor now has a pending reveal callback.
    cx.update(|window, cx| {
        reader.update(cx, |reader, cx| {
            reader.close_margin_note(window, cx);
            reader.open_margin_note(selection(), "question", HighlightColor::Blue, window, cx);
        });
    });
    frame(cx); // The old reveal must not skip the new editor's measurement frame.
    reader.read_with(cx, |reader, _| {
        assert!(
            reader
                .margin_editor
                .as_ref()
                .unwrap()
                .placement_pending
                .is_some()
        );
    });
    frame(cx);
    reader.read_with(cx, |reader, _| {
        assert!(
            reader
                .margin_editor
                .as_ref()
                .unwrap()
                .placement_pending
                .is_none()
        );
    });
}

#[gpui_kit::test]
fn closing_before_placement_does_not_restore_editor_or_focus(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut reader = None;
    let (_root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(fixture(), None, None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    frame(cx);
    cx.update(|window, cx| {
        reader.update(cx, |reader, cx| {
            reader.open_margin_note(selection(), "note", HighlightColor::Yellow, window, cx);
            reader.close_margin_note(window, cx);
        });
    });
    for _ in 0..4 {
        frame(cx);
    }
    reader.read_with(cx, |reader, _| assert!(reader.margin_editor.is_none()));
    cx.update(|window, cx| {
        assert!(
            !reader
                .read(cx)
                .margin_input
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
    });
}
