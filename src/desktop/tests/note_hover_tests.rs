//! Saved note controls are transient in both the drawer and reader margin.
use super::{HighlightColor, Panel, ReaderView, ReadingMode, Selection};
use easyread_gpui::{
    annotation_panel::{self, AnnotationFilter},
    store::{Workspace, write_json},
};
use gpui_kit::{
    AppContext, Modifiers, MouseButton, MouseMoveEvent, TestAppContext, VisualTestContext,
    component::Root, point, px,
};
use serde_json::{Value, json};

fn fixture() -> Value {
    json!({
        "paper": {"meta": {"title_en": "Note hover regression"}, "blocks": [{
            "id": "paragraph", "type": "para", "en": "An original paragraph to annotate."
        }]},
        "reader": {"notes": {"note": {
            "id": "note", "anchor": "paragraph", "key": "paragraph",
            "quote": "An original paragraph to annotate.", "body": "My saved note.",
            "kind": "note", "color": "yellow", "created": "2020-10-14T07:36:00+08:00",
            "updated": "2020-10-14T07:36:00+08:00", "future_field": {"keep": true}
        }}}
    })
}

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

fn check_hover_controls(cx: &mut TestAppContext, drawer: bool) {
    cx.update(gpui_kit::init);
    let raw = fixture();
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
    assert!(cx.debug_bounds("annotation-card-actions").is_none());
    assert!(cx.debug_bounds("annotation-time").is_some());
    assert_reader_has_no_selection_tint(cx);
    assert!(cx.debug_bounds("annotation-paragraph-preview").is_none());
    let body = cx.debug_bounds("annotation-card-body").unwrap();
    hover(cx, body.center());
    assert!(cx.debug_bounds("annotation-paragraph-preview").is_some());
    let actions = cx
        .debug_bounds("annotation-card-actions")
        .expect("hover reveals the note action row");
    assert_eq!(cx.debug_bounds("annotation-card-body"), Some(body));
    hover(cx, actions.center());
    assert_eq!(cx.debug_bounds("annotation-card-actions"), Some(actions));
    assert!(cx.debug_bounds("annotation-paragraph-preview").is_some());
    hover(cx, point(px(5.), px(5.)));
    assert!(cx.debug_bounds("annotation-paragraph-preview").is_none());
    assert!(cx.debug_bounds("annotation-card-actions").is_none());
    assert_eq!(cx.debug_bounds("annotation-card-body"), Some(body));

    hover(cx, body.center());
    cx.update(|window, cx| {
        reader.update(cx, |reader, cx| reader.open_note("note", window, cx));
    });
    settle(cx);
    let input = cx.debug_bounds("margin-note-input").unwrap();
    hover(cx, input.center());
    cx.update(|window, cx| {
        reader.update(cx, |reader, cx| reader.close_margin_note(window, cx));
    });
    settle(cx);
    assert!(
        cx.debug_bounds("annotation-card-actions").is_none(),
        "saving/closing must not reveal controls under a stationary pointer"
    );
    let body = cx.debug_bounds("annotation-card-body").unwrap();
    hover(cx, body.center());
    assert!(cx.debug_bounds("annotation-card-actions").is_some());
}

#[gpui_kit::test]
fn margin_note_actions_only_appear_on_hover(cx: &mut TestAppContext) {
    check_hover_controls(cx, false);
}

#[gpui_kit::test]
fn drawer_note_actions_only_appear_on_hover(cx: &mut TestAppContext) {
    check_hover_controls(cx, true);
}

fn wait_for_save(cx: &mut VisualTestContext, reader: &gpui_kit::Entity<ReaderView>) {
    settle(cx);
    for _ in 0..100 {
        if !reader.read_with(cx, |reader, _| reader.busy) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        settle(cx);
    }
    panic!("note write did not finish");
}

fn click_action(
    cx: &mut VisualTestContext,
    reader: &gpui_kit::Entity<ReaderView>,
    selector: &'static str,
) {
    let body = cx.debug_bounds("annotation-card-body").unwrap();
    hover(cx, body.center());
    let action = cx
        .debug_bounds(selector)
        .expect("correct action label is rendered");
    cx.simulate_click(action.center(), Modifiers::default());
    wait_for_save(cx, reader);
}

fn check_question_lifecycle(cx: &mut TestAppContext, drawer: bool) {
    cx.update(gpui_kit::init);
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace {
        root: temp.path().to_owned(),
    };
    let raw = fixture();
    let original = raw["reader"]["notes"]["note"].clone();
    write_json(&workspace.root.join("paper.json"), &raw["paper"]).unwrap();
    write_json(&workspace.root.join("reader.json"), &raw["reader"]).unwrap();
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut reader = ReaderView::new(raw, None, Some(workspace.clone()), window, cx);
            reader.panel = Panel::Notes;
            reader.panel_open = drawer;
            reader
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    settle(cx);
    assert!(cx.debug_bounds("annotation-time").is_some());
    assert!(cx.debug_bounds("annotation-unanswered").is_none());

    click_action(cx, &reader, "annotation-make-question");
    assert_eq!(
        workspace.load("reader").unwrap()["notes"]["note"]["kind"],
        "question"
    );
    assert!(cx.debug_bounds("annotation-unanswered").is_some());
    assert!(cx.debug_bounds("annotation-make-note").is_some());
    click_action(cx, &reader, "annotation-mark-answered");
    assert!(cx.debug_bounds("annotation-answered").is_some());
    assert!(
        workspace.load("reader").unwrap()["notes"]["note"]["answered"]
            .as_bool()
            .unwrap()
    );
    reader.read_with(cx, |reader, _| {
        assert!(!annotation_panel::matches(
            &reader.raw["reader"]["notes"]["note"],
            AnnotationFilter::Unanswered
        ));
    });
    click_action(cx, &reader, "annotation-mark-unanswered");
    assert!(cx.debug_bounds("annotation-unanswered").is_some());
    reader.read_with(cx, |reader, _| {
        assert!(annotation_panel::matches(
            &reader.raw["reader"]["notes"]["note"],
            AnnotationFilter::Unanswered
        ));
    });
    click_action(cx, &reader, "annotation-mark-answered");
    click_action(cx, &reader, "annotation-make-note");
    assert!(cx.debug_bounds("annotation-answered").is_none());
    assert!(cx.debug_bounds("annotation-mark-unanswered").is_none());
    assert_eq!(
        workspace.load("reader").unwrap()["notes"]["note"]["kind"],
        "note"
    );
    click_action(cx, &reader, "annotation-make-question");
    assert!(
        cx.debug_bounds("annotation-answered").is_some(),
        "round-trip preserves question status"
    );

    hover(cx, point(px(5.), px(5.)));
    assert!(cx.debug_bounds("annotation-card-actions").is_none());
    assert!(
        cx.debug_bounds("annotation-answered").is_some(),
        "status is visible without hover"
    );
    assert!(cx.debug_bounds("annotation-time").is_some());
    let saved = workspace.load("reader").unwrap()["notes"]["note"].clone();
    for field in [
        "id",
        "anchor",
        "key",
        "quote",
        "body",
        "color",
        "created",
        "future_field",
    ] {
        assert_eq!(saved[field], original[field], "must preserve {field}");
    }
    assert_ne!(saved["updated"], original["updated"]);
    assert_eq!(
        annotation_panel::timestamp(&saved),
        annotation_panel::timestamp(&original)
    );

    // The same status control works while editing and uses ordered autosave.
    cx.update(|window, cx| {
        reader.update(cx, |reader, cx| reader.open_note("note", window, cx));
    });
    settle(cx);
    assert!(cx.debug_bounds("annotation-answered").is_some());
    let status = cx.debug_bounds("margin-question-status").unwrap();
    cx.simulate_click(status.center(), Modifiers::default());
    settle(cx);
    assert!(cx.debug_bounds("annotation-unanswered").is_some());
    cx.update(|window, cx| {
        reader.update(cx, |reader, cx| reader.close_margin_note(window, cx));
    });
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(5));
        reader.update(cx, |reader, cx| reader.receive_margin_notes(cx));
        settle(cx);
        if workspace.load("reader").unwrap()["notes"]["note"]["answered"] == false {
            break;
        }
    }
    assert_eq!(
        workspace.load("reader").unwrap()["notes"]["note"]["answered"],
        false
    );
    assert!(cx.debug_bounds("annotation-unanswered").is_some());
    assert!(cx.debug_bounds("annotation-card-actions").is_none());
}

#[gpui_kit::test]
fn margin_question_switches_kind_and_answer_status_and_persists(cx: &mut TestAppContext) {
    check_question_lifecycle(cx, false);
}

#[gpui_kit::test]
fn drawer_question_switches_kind_and_answer_status_and_persists(cx: &mut TestAppContext) {
    check_question_lifecycle(cx, true);
}

fn assert_ai_surface(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx.debug_bounds(selector).unwrap();
    cx.update(|window, _| {
        let scale = window.scale_factor();
        let surfaces: Vec<_> = window
            .painted_quads()
            .into_iter()
            .filter(|quad| {
                (quad.bounds.left().0 / scale - f32::from(bounds.left())).abs() < 1.
                    && (quad.bounds.top().0 / scale - f32::from(bounds.top())).abs() < 1.
                    && (quad.bounds.size.width.0 / scale - f32::from(bounds.size.width)).abs() < 1.
                    && (quad.bounds.size.height.0 / scale - f32::from(bounds.size.height)).abs()
                        < 1.
            })
            .collect();
        let teal = gpui_kit::Background::from(gpui_kit::rgba(0x38748712));
        let white = gpui_kit::Background::from(gpui_kit::rgb(0xffffff));
        assert!(
            surfaces.iter().any(|quad| quad.background == teal),
            "AI button keeps its teal surface: expected {teal:?}, bounds {bounds:?}, surfaces {surfaces:?}"
        );
        assert!(
            !surfaces.iter().any(|quad| quad.background == white),
            "AI button must not turn white"
        );
    });
}

fn check_ai_and_paragraph_preview(cx: &mut TestAppContext, drawer: bool, question: bool) {
    cx.update(gpui_kit::init);
    let mut raw = fixture();
    raw["paper"]["blocks"][0]["en"] = json!(
        "First paragraph should not be highlighted.\n\nAn original paragraph to annotate.\n\nLast paragraph should not be highlighted."
    );
    raw["reader"]["notes"]["note"]["kind"] = json!(if question { "question" } else { "note" });
    let selector = if question {
        "annotation-ai-answer"
    } else {
        "annotation-ai-feedback"
    };
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut reader = ReaderView::new(raw, None, None, window, cx);
            reader.reading_mode = ReadingMode::Original;
            reader.panel = Panel::Notes;
            reader.panel_open = drawer;
            reader
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    settle(cx);
    assert_ai_surface(cx, selector);
    let (text_view, paragraphs) = reader.read_with(cx, |reader, cx| {
        let view = reader.text_views[&(0, true)].1.clone();
        let bounds = (0..3)
            .map(|index| view.read(cx).paragraph_bounds(index).unwrap())
            .collect::<Vec<_>>();
        (view, bounds)
    });
    let body = cx.debug_bounds("annotation-card-body").unwrap();
    hover(cx, body.center());
    let preview = cx
        .debug_bounds("annotation-paragraph-preview")
        .expect("card hover highlights the quoted paragraph");
    assert!((preview.top() - paragraphs[1].top() + px(4.)).abs() < px(1.));
    assert!((preview.bottom() - paragraphs[1].bottom() - px(4.)).abs() < px(1.));
    assert!(preview.top() >= paragraphs[0].bottom());
    assert!(preview.bottom() <= paragraphs[2].top());
    assert_eq!(cx.debug_bounds("annotation-card-body"), Some(body));
    reader.read_with(cx, |reader, cx| {
        assert_eq!(reader.text_views[&(0, true)].1, text_view);
        assert!(text_view.read(cx).is_content_ready());
    });
    cx.update(|window, _| {
        let scale = window.scale_factor();
        assert!(
            window.painted_quads().iter().any(|quad| {
                quad.background == gpui_kit::Background::from(gpui_kit::rgb(0xecece6))
                    && (quad.bounds.top().0 / scale - f32::from(preview.top())).abs() < 1.
                    && (quad.bounds.bottom().0 / scale - f32::from(preview.bottom())).abs() < 1.
            }),
            "paragraph preview must paint exactly #ECECE6"
        );
    });
    hover(cx, point(px(5.), px(5.)));
    assert!(cx.debug_bounds("annotation-paragraph-preview").is_none());
    let button = cx.debug_bounds(selector).unwrap();
    hover(cx, button.center());
    assert_ai_surface(cx, selector);
    assert!(cx.debug_bounds("annotation-paragraph-preview").is_some());
    cx.simulate_mouse_down(button.center(), MouseButton::Left, Modifiers::default());
    settle(cx);
    assert_ai_surface(cx, selector);
    cx.simulate_mouse_up(button.center(), MouseButton::Left, Modifiers::default());
    settle(cx);
    reader.read_with(cx, |reader, cx| {
        assert!(reader.panel == Panel::Chat && reader.panel_open);
        let prompt = reader.question.read(cx).value().to_string();
        assert_eq!(reader.chat_ui.refs.len(), 1);
        assert_eq!(
            reader.chat_ui.refs[0].quote,
            "An original paragraph to annotate."
        );
        assert!(prompt.contains("My saved note."));
        if question {
            assert_eq!(prompt, "My saved note.");
        } else {
            assert!(prompt.contains("请点评"));
            assert!(prompt.contains("我的笔记："));
        }
        // No workspace/model in this fixture: keep the draft for retry.
        // A configured note-card action auto-sends (covered by chat_tests).
        assert!(reader.hovered_note_card.is_none());
    });
    assert!(cx.debug_bounds("annotation-paragraph-preview").is_none());
}

#[gpui_kit::test]
fn margin_note_ai_keeps_background_and_previews_only_its_paragraph(cx: &mut TestAppContext) {
    check_ai_and_paragraph_preview(cx, false, false);
}

#[gpui_kit::test]
fn margin_question_ai_answers_and_keeps_background(cx: &mut TestAppContext) {
    check_ai_and_paragraph_preview(cx, false, true);
}

#[gpui_kit::test]
fn drawer_note_ai_keeps_background_and_previews_only_its_paragraph(cx: &mut TestAppContext) {
    check_ai_and_paragraph_preview(cx, true, false);
}

#[gpui_kit::test]
fn drawer_question_ai_answers_and_keeps_background(cx: &mut TestAppContext) {
    check_ai_and_paragraph_preview(cx, true, true);
}

fn assert_reader_has_no_selection_tint(cx: &mut VisualTestContext) {
    cx.update(|window, _| {
        let tint = gpui_kit::Background::from(gpui_kit::rgba(0x9c5f1c09));
        assert!(
            !window
                .painted_quads()
                .iter()
                .any(|quad| quad.background == tint),
            "selecting a block for a note must not tint the original page"
        );
    });
}

#[gpui_kit::test]
fn adding_and_saving_a_note_does_not_change_the_original_page_background(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let temp = tempfile::tempdir().unwrap();
    let workspace = Workspace {
        root: temp.path().to_owned(),
    };
    let mut raw = fixture();
    raw["reader"]["notes"] = json!({});
    raw["paper"]["blocks"].as_array_mut().unwrap().insert(
        0,
        json!({
            "id":"earlier", "type":"para", "en":"An earlier paragraph on another reader block."
        }),
    );
    let paper = raw["paper"].clone();
    write_json(&workspace.root.join("paper.json"), &paper).unwrap();
    write_json(&workspace.root.join("reader.json"), &raw["reader"]).unwrap();
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut reader = ReaderView::new(raw, None, Some(workspace.clone()), window, cx);
            reader.reading_mode = ReadingMode::Original;
            reader
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    settle(cx);
    assert_reader_has_no_selection_tint(cx);
    cx.update(|window, cx| {
        reader.update(cx, |reader, cx| {
            reader.selection = Some(Selection {
                block: 1,
                key: "paragraph".into(),
                quote: "An original paragraph to annotate.".into(),
                prefix: String::new(),
                suffix: String::new(),
                position: point(px(300.), px(200.)),
                underline: false,
            });
            reader.selection_action("note", HighlightColor::Yellow, window, cx);
            assert_eq!(reader.selected, 1, "exercise a newly selected note anchor");
        });
    });
    settle(cx);
    assert!(cx.debug_bounds("margin-note-input").is_some());
    assert_reader_has_no_selection_tint(cx);
    cx.update(|window, cx| {
        reader.update(cx, |reader, cx| {
            reader.margin_input.update(cx, |input, cx| {
                input.set_value("A newly saved note.", window, cx);
                cx.emit(super::InputEvent::Change);
            });
        });
    });
    settle(cx);
    assert_reader_has_no_selection_tint(cx);
    cx.update(|window, cx| {
        reader.update(cx, |reader, cx| reader.close_margin_note(window, cx));
    });
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(5));
        reader.update(cx, |reader, cx| reader.receive_margin_notes(cx));
        settle(cx);
        if reader.read_with(cx, |reader, _| {
            reader.raw["reader"]["notes"]
                .as_object()
                .unwrap()
                .values()
                .any(|note| note["body"] == "A newly saved note.")
        }) {
            break;
        }
    }
    assert!(
        workspace.load("reader").unwrap()["notes"]
            .as_object()
            .unwrap()
            .values()
            .any(|note| note["body"] == "A newly saved note.")
    );
    assert!(cx.debug_bounds("annotation-card-body").is_some());
    assert!(cx.debug_bounds("annotation-paragraph-preview").is_none());
    assert_reader_has_no_selection_tint(cx);
    assert_eq!(workspace.load("paper").unwrap(), paper);
    let body = cx.debug_bounds("annotation-card-body").unwrap();
    hover(cx, body.center());
    assert!(cx.debug_bounds("annotation-paragraph-preview").is_some());
    hover(cx, point(px(5.), px(5.)));
    assert!(cx.debug_bounds("annotation-paragraph-preview").is_none());
    assert_reader_has_no_selection_tint(cx);
}
