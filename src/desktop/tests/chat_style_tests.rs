use super::{click, settle, setup};
use easyread_gpui::ai::AnswerStyle;
use gpui_kit::{TestAppContext, px, size};
use serde_json::json;

#[gpui_kit::test]
fn answer_style_menu_has_wrapped_explanations_and_preserves_draft_and_preferences(
    cx: &mut TestAppContext,
) {
    let (reader, cx) = setup(cx);
    let preferences = reader.read_with(cx, |reader, _| reader.settings.committed.clone());
    click(cx, "chat-style");
    let menu = cx.debug_bounds("chat-style-menu").unwrap();
    let trigger = cx.debug_bounds("chat-style").unwrap();
    assert!(
        menu.bottom() <= trigger.top(),
        "menu {menu:?}, trigger {trigger:?}"
    );
    assert!(menu.size.width >= px(490.));
    for (row, title, description, check) in [
        (
            "chat-style-option-0",
            "chat-style-title-0",
            "chat-style-description-0",
            "chat-style-check-0",
        ),
        (
            "chat-style-option-1",
            "chat-style-title-1",
            "chat-style-description-1",
            "chat-style-check-1",
        ),
        (
            "chat-style-option-2",
            "chat-style-title-2",
            "chat-style-description-2",
            "chat-style-check-2",
        ),
    ] {
        let row = cx.debug_bounds(row).unwrap();
        let title = cx.debug_bounds(title).unwrap();
        let description = cx.debug_bounds(description).unwrap();
        let check = cx.debug_bounds(check).unwrap();
        assert!((title.left() - row.left() - px(10.)).abs() < px(3.));
        assert_eq!(title.left(), description.left());
        assert!(description.top() >= title.bottom());
        assert!(description.right() <= row.right());
        assert!(
            description.bottom() <= row.bottom(),
            "description must not be clipped by a fixed-height button"
        );
        assert!(check.left() > title.right());
    }
    let standard = cx.debug_bounds("chat-style-description-0").unwrap();
    let detailed = cx.debug_bounds("chat-style-description-2").unwrap();
    assert!(
        detailed.size.height > standard.size.height,
        "long descriptions must wrap"
    );
    click(cx, "chat-style-option-1");
    assert!(cx.debug_bounds("chat-style-menu").is_none());
    reader.read_with(cx, |reader, cx| {
        assert_eq!(reader.chat_ui.style, AnswerStyle::Simple);
        assert_eq!(reader.question.read(cx).value(), "Keep this unsent draft");
        assert_eq!(reader.settings.committed, preferences);
        assert!(!reader.chat_ui.streaming);
    });
    click(cx, "chat-style");
    click(cx, "chat-style-option-2");
    assert_eq!(
        reader.read_with(cx, |reader, _| reader.chat_ui.style),
        AnswerStyle::CloseReading
    );
    cx.update(|_, cx| {
        reader.update(cx, |reader, cx| {
            reader.raw["chat"]["threads"][0]["answer_style"] = json!("ste100");
            reader.select_chat_thread("saved", cx);
        })
    });
    assert_eq!(
        reader.read_with(cx, |reader, _| reader.chat_ui.style),
        AnswerStyle::Simple
    );
}

#[gpui_kit::test]
fn answer_style_menu_keyboard_toggle_outside_click_and_narrow_bounds(cx: &mut TestAppContext) {
    let (reader, cx) = setup(cx);
    click(cx, "chat-style");
    cx.simulate_keystrokes("down enter");
    settle(cx);
    assert_eq!(
        reader.read_with(cx, |reader, _| reader.chat_ui.style),
        AnswerStyle::Simple
    );
    assert!(cx.debug_bounds("chat-style-menu").is_none());
    click(cx, "chat-style");
    cx.simulate_keystrokes("escape");
    settle(cx);
    reader.read_with(cx, |reader, _| {
        assert!(!reader.chat_ui.style_open);
        assert!(reader.panel_open && !reader.chat_ui.closing);
    });
    click(cx, "chat-style");
    click(cx, "chat-style");
    assert!(cx.debug_bounds("chat-style-menu").is_none());
    click(cx, "chat-style");
    click(cx, "chat-model");
    assert!(cx.debug_bounds("chat-style-menu").is_none());
    assert!(cx.debug_bounds("chat-model-menu").is_some());
    click(cx, "chat-style");
    assert!(cx.debug_bounds("chat-model-menu").is_none());
    assert!(cx.debug_bounds("chat-style-menu").is_some());
    click(cx, "chat-style");
    cx.simulate_resize(size(px(360.), px(600.)));
    settle(cx);
    click(cx, "chat-style");
    let menu = cx.debug_bounds("chat-style-menu").unwrap();
    assert!(menu.left() >= px(8.) && menu.right() <= px(352.) && menu.top() >= px(8.));
    click(cx, "chat-input");
    assert!(cx.debug_bounds("chat-style-menu").is_none());
    reader.read_with(cx, |reader, cx| {
        assert_eq!(reader.question.read(cx).value(), "Keep this unsent draft");
        assert!(reader.panel_open && !reader.chat_ui.closing);
    });
}
