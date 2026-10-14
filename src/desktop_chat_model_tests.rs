//! Model menu geometry, layering, keyboard navigation and local effort scope.
use super::{Panel, ReaderView};
use easyread_gpui::ai::{ModelProfile, Provider};
use gpui_kit::{
    AppContext, Entity, Modifiers, TestAppContext, VisualTestContext, component::Root, px, size,
};
use serde_json::json;
use std::time::Duration;

#[path = "desktop_chat_style_tests.rs"]
mod style_menu_tests;

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..8 {
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.draw(cx).clear(cx);
        });
    }
}
fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("missing {selector}"));
    cx.simulate_event(gpui_kit::MouseMoveEvent {
        position: bounds.center(),
        pressed_button: None,
        modifiers: Modifiers::default(),
    });
    settle(cx);
    cx.simulate_click(bounds.center(), Modifiers::default());
    settle(cx);
}
fn models() -> Vec<ModelProfile> {
    [
        (
            "opus",
            "Claude Opus 5.5",
            Provider::ClaudeCode,
            "claude-opus-5-5",
        ),
        ("sonnet", "Claude Sonnet 5", Provider::ClaudeCode, "sonnet"),
        ("codex", "Codex default", Provider::CodexCli, "default"),
        ("haiku", "Claude Haiku", Provider::ClaudeCode, "haiku"),
    ]
    .into_iter()
    .map(|(id, name, provider, model)| ModelProfile {
        id: id.into(),
        name: name.into(),
        provider,
        model: model.into(),
        ..Default::default()
    })
    .collect()
}
fn setup(cx: &mut TestAppContext) -> (Entity<ReaderView>, &mut VisualTestContext) {
    cx.update(gpui_kit::init);
    let mut reader = None;
    let (_,cx)=cx.add_window_view(|window,cx| {
        let view=cx.new(|cx| {
            let mut reader=ReaderView::new(json!({"paper":{"meta":{"title_en":"Model menu"},"blocks":[
                {"id":"p","type":"para","en":"A paragraph for the Ask AI model picker."}]},"reader":{},
                "chat":{"threads":[{"id":"saved","model":"sonnet","chat_options":{"reasoning_effort":"max"},"messages":[]},
                    {"id":"plain","model":"codex","messages":[]}]}}),None,None,window,cx);
            reader.settings.committed.models=models();
            reader.settings.committed.ask_model="opus".into();
            reader.chat_ui.model.clear();
            reader
        });
        reader=Some(view.clone());Root::new(view,window,cx)
    });
    let reader = reader.unwrap();
    cx.simulate_resize(size(px(1300.), px(800.)));
    cx.update(|window, cx| {
        reader.update(cx, |reader, cx| {
            reader.show_panel(Panel::Chat, window, cx);
            reader.question.update(cx, |input, cx| {
                input.set_value("Keep this unsent draft", window, cx)
            });
        })
    });
    settle(cx);
    std::thread::sleep(Duration::from_millis(240));
    cx.executor().advance_clock(Duration::from_millis(310));
    settle(cx);
    (reader, cx)
}

#[gpui_kit::test]
fn model_menu_has_left_aligned_details_local_effort_and_no_draft_side_effects(
    cx: &mut TestAppContext,
) {
    let (reader, cx) = setup(cx);
    let preferences = reader.read_with(cx, |reader, _| reader.settings.committed.clone());
    click(cx, "chat-model");
    let menu = cx.debug_bounds("chat-model-menu").unwrap();
    let trigger = cx.debug_bounds("chat-model").unwrap();
    assert!(menu.bottom() < trigger.top());
    assert!(menu.left() >= px(8.) && menu.right() <= px(1292.));
    assert!(menu.size.width >= px(350.));
    for (row, name, provider, check) in [
        (
            "chat-model-option-0",
            "chat-model-name-0",
            "chat-model-provider-0",
            "chat-model-check-0",
        ),
        (
            "chat-model-option-1",
            "chat-model-name-1",
            "chat-model-provider-1",
            "chat-model-check-1",
        ),
        (
            "chat-model-option-2",
            "chat-model-name-2",
            "chat-model-provider-2",
            "chat-model-check-2",
        ),
        (
            "chat-model-option-3",
            "chat-model-name-3",
            "chat-model-provider-3",
            "chat-model-check-3",
        ),
    ] {
        let row = cx.debug_bounds(row).unwrap();
        let name = cx.debug_bounds(name).unwrap();
        let provider = cx.debug_bounds(provider).unwrap();
        let check = cx.debug_bounds(check).unwrap();
        assert!((name.left() - row.left() - px(10.)).abs() < px(3.));
        assert_eq!(name.left(), provider.left());
        assert!(provider.top() >= name.bottom());
        assert!(check.left() > name.right());
    }
    assert!(cx.debug_bounds("chat-effort-max").is_some());
    click(cx, "chat-effort-xhigh");
    assert!(
        cx.debug_bounds("chat-model-menu").is_some(),
        "effort keeps menu open"
    );
    reader.read_with(cx, |reader, cx| {
        assert_eq!(reader.chat_ui.reasoning.as_deref(), Some("xhigh"));
        assert_eq!(reader.question.read(cx).value(), "Keep this unsent draft");
        assert_eq!(reader.settings.committed, preferences);
    });
    click(cx, "chat-effort-reset");
    assert!(reader.read_with(cx, |reader, _| reader.chat_ui.reasoning.is_none()));
    click(cx, "chat-effort-medium");
    assert_eq!(
        reader.read_with(cx, |reader, _| reader.chat_ui.reasoning.clone()),
        Some(String::new()),
        "the documented default is displayed without forcing an override"
    );
    click(cx, "chat-effort-max");
    click(cx, "chat-model-option-2");
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.chat_ui.model, "codex");
        assert!(
            reader.chat_ui.reasoning.is_none(),
            "unsupported effort must not leak to another provider"
        );
    });
    click(cx, "chat-model");
    assert!(cx.debug_bounds("chat-effort-xhigh").is_some());
    assert!(cx.debug_bounds("chat-effort-max").is_none());
    click(cx, "chat-model-option-3");
    click(cx, "chat-model");
    assert!(
        cx.debug_bounds("chat-thinking-effort").is_none(),
        "Haiku has no effort control"
    );
    click(cx, "chat-model");
    assert!(
        cx.debug_bounds("chat-model-menu").is_none(),
        "trigger toggles closed, rather than reopening on mouse-up"
    );
    click(cx, "chat-model");
    click(cx, "chat-input");
    assert!(cx.debug_bounds("chat-model-menu").is_none());
    reader.read_with(cx, |reader, cx| {
        assert!(reader.panel_open && !reader.chat_ui.closing);
        assert_eq!(reader.question.read(cx).value(), "Keep this unsent draft");
    });
}

#[gpui_kit::test]
fn model_menu_closes_outside_supports_keyboard_and_restores_only_thread_options(
    cx: &mut TestAppContext,
) {
    let (reader, cx) = setup(cx);
    click(cx, "chat-model");
    cx.simulate_keystrokes("down enter");
    settle(cx);
    assert_eq!(
        reader.read_with(cx, |reader, _| reader.chat_ui.model.clone()),
        "sonnet"
    );
    assert!(cx.debug_bounds("chat-model-menu").is_none());
    click(cx, "chat-model");
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert!(cx.debug_bounds("chat-model-menu").is_none());
    reader.read_with(cx, |reader, _| {
        assert!(
            reader.panel_open && !reader.chat_ui.closing,
            "Escape must close only the model menu"
        );
    });
    click(cx, "chat-model");
    click(cx, "chat-style");
    assert!(cx.debug_bounds("chat-model-menu").is_none());
    reader.read_with(cx, |reader, _| {
        assert!(
            reader.panel_open && !reader.chat_ui.closing,
            "outside click must close only the model menu"
        );
    });
    cx.update(|_, cx| reader.update(cx, |reader, cx| reader.select_chat_thread("saved", cx)));
    reader.read_with(cx, |reader, _| {
        assert_eq!(reader.chat_ui.model, "sonnet");
        assert_eq!(reader.chat_ui.reasoning.as_deref(), Some("max"));
    });
    cx.update(|_, cx| reader.update(cx, |reader, cx| reader.select_chat_thread("plain", cx)));
    assert!(reader.read_with(cx, |reader, _| reader.chat_ui.reasoning.is_none()));
    cx.update(|_, cx| reader.update(cx, |reader, cx| reader.select_chat_thread("saved", cx)));
    settle(cx);
    cx.simulate_resize(size(px(360.), px(600.)));
    settle(cx);
    click(cx, "chat-model");
    let menu = cx.debug_bounds("chat-model-menu").unwrap();
    assert!(
        menu.left() >= px(8.) && menu.right() <= px(352.),
        "menu must stay inside a narrow viewport: {menu:?}"
    );
    click(cx, "chat-manage-models");
    assert!(reader.read_with(cx, |reader, _| reader.settings.open));
}
