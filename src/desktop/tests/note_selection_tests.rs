//! Mouse release must end selection even when a saved note swallows bubbling.
use super::{ReaderView, ReadingMode};
use easyread_gpui::store::{Workspace, write_json};
use gpui_kit::base::TextSelection;
use gpui_kit::{
    AppContext, Modifiers, MouseButton, MouseMoveEvent, TestAppContext, VisualTestContext,
    component::Root, point, px,
};
use serde_json::json;

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..8 {
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.draw(cx).clear(cx);
        });
    }
}

fn hover(cx: &mut VisualTestContext, position: gpui_kit::Point<gpui_kit::Pixels>) {
    cx.simulate_event(MouseMoveEvent {
        position,
        pressed_button: None,
        modifiers: Modifiers::default(),
    });
    settle(cx);
}

#[gpui_kit::test]
fn saved_note_release_and_hover_do_not_leave_a_selection_gesture_active(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace {
        root: temp.path().to_owned(),
    };
    let raw = json!({"paper":{"meta":{"title_en":"Saved note mouse regression"},"blocks":[{"id":"page", "type":"para", "en":"First original paragraph with sufficient text to drag and copy.\n\nSecond original paragraph with another line of selectable text."}]},"reader":{"notes":{"saved-note":{"id":"saved-note", "anchor":"page", "key":"page", "quote":"First original paragraph with sufficient text to drag and copy.", "kind":"note", "style":"underline", "color":"yellow", "body":"A previous note.", "created":"1", "updated":"1"}}}});
    write_json(&workspace.root.join("paper.json"), &raw["paper"]).unwrap();
    write_json(&workspace.root.join("reader.json"), &raw["reader"]).unwrap();
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(raw, None, Some(workspace.clone()), window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    reader.update(cx, |r, cx| r.set_reading_mode(ReadingMode::Original, cx));
    settle(cx);
    // Re-open, edit, persist and close through the actual margin-note lifecycle.
    cx.update(|window, cx| {
        reader.update(cx, |r, cx| {
            r.open_note("saved-note", window, cx);
        })
    });
    settle(cx);
    cx.update(|window, cx| reader.update(cx, |r, cx| {
        r.margin_input.update(cx, |input, cx| {
            input.set_value("Saved note text should select only while the left mouse button is held. 这是一条已保存的笔记。", window, cx);
            cx.emit(super::InputEvent::Change);
        });
    }));
    settle(cx);
    cx.update(|window, cx| reader.update(cx, |r, cx| r.close_margin_note(window, cx)));
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(5));
        reader.update(cx, |r, cx| r.receive_margin_notes(cx));
        settle(cx);
        if workspace.load("reader").unwrap()["notes"]["saved-note"]["body"]
            .as_str()
            .unwrap()
            .starts_with("Saved note text")
        {
            break;
        }
    }
    assert!(
        workspace.load("reader").unwrap()["notes"]["saved-note"]["body"]
            .as_str()
            .unwrap()
            .starts_with("Saved note text")
    );
    assert!(
        cx.debug_bounds("annotation-card-actions").is_none(),
        "persisting a note must leave its action row hidden until hover"
    );
    let note = cx
        .debug_bounds("annotation-card-body")
        .expect("saved card body is visible");
    let start = point(note.left() + px(8.), note.top() + px(10.));
    let end = point(note.left() + px(110.), note.top() + px(10.));
    let source = reader.read_with(cx, |r, cx| {
        r.text_views[&(0, true)]
            .1
            .read(cx)
            .paragraph_bounds(0)
            .unwrap()
    });
    let source_start = point(source.left() + px(8.), source.top() + px(10.));
    let source_end = point(source.left() + px(110.), source.top() + px(10.));
    // A source press released over the card used to miss the window's mouse-up.
    cx.simulate_mouse_down(source_start, MouseButton::Left, Modifiers::default());
    settle(cx);
    cx.simulate_mouse_move(source_end, MouseButton::Left, Modifiers::default());
    settle(cx);
    cx.simulate_mouse_up(start, MouseButton::Left, Modifiers::default());
    settle(cx);
    assert!(
        !reader.read_with(cx, |r, cx| r.text_views[&(0, true)]
            .1
            .read(cx)
            .is_selecting()),
        "A card swallowing mouse-up must not leave a drag active"
    );
    let before = cx.update(TextSelection::selected_text);
    for pos in [end, source_end, start, source_start] {
        hover(cx, pos);
        assert_eq!(
            cx.update(TextSelection::selected_text),
            before,
            "Hovering after release must not change the selection"
        );
    }
    // A simple click and later hover must not start selecting the saved body.
    cx.update(TextSelection::clear);
    cx.simulate_click(start, Modifiers::default());
    settle(cx);
    for pos in [end, source_end, start] {
        hover(cx, pos);
        assert!(
            cx.update(TextSelection::selected_text).trim().is_empty(),
            "Hover alone must not select saved note text"
        );
    }
    // Ordinary drag/copy still works inside the saved card, including its release.
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    settle(cx);
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::default());
    settle(cx);
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
    settle(cx);
    let selected = cx.update(TextSelection::selected_text);
    assert!(
        !selected.trim().is_empty(),
        "A deliberate drag in saved notes must remain selectable"
    );
    for pos in [source_end, start, end] {
        hover(cx, pos);
        assert_eq!(
            cx.update(TextSelection::selected_text),
            selected,
            "The completed drag must stay fixed after release"
        );
    }
    // Losing the release outside the window must also recover on the next
    // button-free move, rather than extending an old selection into the card.
    cx.update(TextSelection::clear);
    cx.simulate_mouse_down(source_start, MouseButton::Left, Modifiers::default());
    settle(cx);
    cx.simulate_mouse_move(source_end, MouseButton::Left, Modifiers::default());
    settle(cx);
    let selected = cx.update(TextSelection::selected_text);
    assert!(!selected.trim().is_empty());
    // Releasing a different button must not prematurely end a left drag.
    cx.simulate_mouse_up(source_end, MouseButton::Right, Modifiers::default());
    settle(cx);
    assert!(reader.read_with(cx, |r, cx| {
        r.text_views[&(0, true)].1.read(cx).is_selecting()
    }));
    hover(cx, end);
    assert!(!reader.read_with(cx, |r, cx| {
        r.text_views[&(0, true)].1.read(cx).is_selecting()
    }));
    assert_eq!(cx.update(TextSelection::selected_text), selected);
    hover(cx, source_start);
    assert_eq!(cx.update(TextSelection::selected_text), selected);
    cx.simulate_mouse_up(source_end, MouseButton::Left, Modifiers::default());
    settle(cx);
}
