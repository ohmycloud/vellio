//! Answer-style menu: wrapped explanations and a right-aligned selection mark.
use super::*;
use easyread_gpui::ai::AnswerStyle;
use gpui_kit::base::{ElementExt as _, Popup};

impl ReaderView {
    fn choose_chat_style(
        &mut self,
        style: AnswerStyle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.chat_ui.streaming {
            return;
        }
        self.chat_ui.style = style;
        self.chat_ui.style_open = false;
        self.question.read(cx).focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    pub(super) fn chat_style_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let nonstandard = self.chat_ui.style != AnswerStyle::Standard;
        let trigger = Button::new("chat-style")
            .debug_selector(|| "chat-style".into())
            .ghost()
            .xsmall()
            .h(px(24.))
            .px(px(10.))
            .rounded(px(12.))
            .border_1()
            .border_color(rgb(if nonstandard { 0x387487 } else { 0xd4cbbb }))
            .text_color(rgb(if nonstandard { 0x387487 } else { 0x8e8679 }))
            .when(nonstandard, |b| b.bg(rgba(0x38748712)))
            .label(self.chat_ui.style.label())
            .accessibility_label(format!("回答方式：{}", self.chat_ui.style.label()))
            .disabled(self.chat_ui.streaming)
            .on_click(cx.listener(|this, _, window, cx| {
                this.chat_ui.style_open = !this.chat_ui.style_open;
                this.chat_ui.model_open = false;
                this.chat_ui.list_open = false;
                if this.chat_ui.style_open {
                    this.chat_ui.style_cursor = AnswerStyle::ALL
                        .iter()
                        .position(|s| *s == this.chat_ui.style)
                        .unwrap_or(0);
                    this.chat_ui
                        .style_scroll
                        .scroll_to_item(this.chat_ui.style_cursor);
                    this.chat_ui.style_focus.focus(window, cx);
                } else {
                    this.question.read(cx).focus_handle(cx).focus(window, cx);
                }
                cx.notify();
            }));
        div()
            .flex_shrink_0()
            .on_prepaint({
                let view = cx.entity().downgrade();
                move |bounds, _, cx| {
                    let _ = view.update(cx, |this, _| this.chat_ui.style_bounds = bounds);
                }
            })
            .child(
                Popup::new("chat-style-popup", trigger)
                    .anchor(Anchor::BottomLeft)
                    .when(self.chat_ui.style_open, |p| {
                        p.content(self.chat_style_menu(cx))
                    }),
            )
            .into_any_element()
    }

    fn chat_style_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("chat-style-menu")
            .debug_selector(|| "chat-style-menu".into())
            .w(px(self.chat_ui.style_menu_width))
            .min_h_0()
            .max_h((self.chat_ui.style_bounds.top() - px(72.)).clamp(px(100.), px(420.)))
            .track_scroll(&self.chat_ui.style_scroll)
            .overflow_y_scroll()
            .p(px(5.))
            .rounded(px(12.))
            .bg(rgb(0xfbfaf7))
            .border_1()
            .border_color(rgb(0xe3ddd2))
            .shadow_md()
            .flex()
            .flex_col()
            .text_left()
            .track_focus(&self.chat_ui.style_focus)
            .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                if this.chat_ui.style_open && !this.chat_ui.style_bounds.contains(&event.position) {
                    this.chat_ui.style_open = false;
                    cx.notify();
                }
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if !this.chat_ui.style_open {
                    return;
                }
                match event.keystroke.key.as_str() {
                    "up" | "down" => {
                        let delta = if event.keystroke.key == "down" { 1 } else { 2 };
                        this.chat_ui.style_cursor = (this.chat_ui.style_cursor + delta) % 3;
                        this.chat_ui
                            .style_scroll
                            .scroll_to_item(this.chat_ui.style_cursor);
                    }
                    "enter" => this.choose_chat_style(
                        AnswerStyle::ALL[this.chat_ui.style_cursor],
                        window,
                        cx,
                    ),
                    "escape" => {
                        this.chat_ui.style_open = false;
                        this.question.read(cx).focus_handle(cx).focus(window, cx);
                    }
                    _ => return,
                }
                cx.stop_propagation();
                cx.notify();
            }))
            .children(
                AnswerStyle::ALL
                    .into_iter()
                    .enumerate()
                    .map(|(index, style)| {
                        let selected = self.chat_ui.style == style;
                        Button::new(("chat-style-option", index))
                            .debug_selector(move || format!("chat-style-option-{index}"))
                            .ghost()
                            .w_full()
                            .h_auto()
                            .flex_shrink_0()
                            .px(px(10.))
                            .py(px(10.))
                            .rounded(px(8.))
                            .text_left()
                            .role(Role::RadioButton)
                            .toggled(selected)
                            .accessibility_label(format!(
                                "{}，{}",
                                style.label(),
                                style.description()
                            ))
                            .disabled(self.chat_ui.streaming)
                            .when(self.chat_ui.style_cursor == index && !selected, |b| {
                                b.bg(rgba(0x38748709))
                            })
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .w_full()
                                    .min_w_0()
                                    .gap(px(4.))
                                    .whitespace_normal()
                                    .child(
                                        div()
                                            .flex()
                                            .w_full()
                                            .items_center()
                                            .justify_between()
                                            .gap(px(12.))
                                            .child(
                                                div()
                                                    .debug_selector(move || {
                                                        format!("chat-style-title-{index}")
                                                    })
                                                    .text_size(px(14.))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(rgb(if selected {
                                                        0x387487
                                                    } else {
                                                        0x27231d
                                                    }))
                                                    .child(style.label()),
                                            )
                                            .child(
                                                div()
                                                    .debug_selector(move || {
                                                        format!("chat-style-check-{index}")
                                                    })
                                                    .flex_shrink_0()
                                                    .w(px(16.))
                                                    .text_color(rgb(0x387487))
                                                    .child(if selected { "✓" } else { "" }),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .debug_selector(move || {
                                                format!("chat-style-description-{index}")
                                            })
                                            .w_full()
                                            .min_w_0()
                                            .whitespace_normal()
                                            .text_size(px(12.))
                                            .line_height(relative(1.6))
                                            .text_color(rgb(0x8e8679))
                                            .child(style.description()),
                                    ),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.choose_chat_style(style, window, cx)
                            }))
                    }),
            )
            .into_any_element()
    }
}
