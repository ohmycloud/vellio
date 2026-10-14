//! Anchored model menu and conversation-local thinking effort.
use super::*;
use gpui_kit::base::{ElementExt as _, Popup};

pub(super) fn effort_label(value: &str) -> &'static str {
    match value {
        "low" => "低",
        "medium" => "中",
        "high" => "高",
        "xhigh" => "很高",
        "max" => "最高",
        _ => "默认",
    }
}

impl ReaderView {
    pub(super) fn chat_model_profile(&self) -> Option<&ai::ModelProfile> {
        let id = if self.chat_ui.model.is_empty() {
            &self.settings.committed.ask_model
        } else {
            &self.chat_ui.model
        };
        self.settings.committed.models.iter().find(|m| m.id == *id)
    }

    fn chat_effective_effort<'a>(&'a self, model: &'a ai::ModelProfile) -> &'a str {
        let value = self
            .chat_ui
            .reasoning
            .as_deref()
            .unwrap_or(&model.reasoning);
        if value.is_empty() {
            model.reasoning_fallback()
        } else {
            value
        }
    }

    fn choose_chat_model(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.chat_ui.streaming {
            return;
        }
        let Some(model) = self.settings.committed.models.iter().find(|m| m.id == id) else {
            return;
        };
        if self
            .chat_ui
            .reasoning
            .as_deref()
            .is_some_and(|v| !model.supports_reasoning(v))
        {
            self.chat_ui.reasoning = None;
        }
        self.chat_ui.model = id;
        self.chat_ui.model_open = false;
        self.question.read(cx).focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    pub(super) fn chat_model_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let profile = self.chat_model_profile();
        let name = profile.map(|m| m.name.as_str()).unwrap_or("选择模型");
        let effort = profile
            .filter(|m| !m.reasoning_levels().is_empty())
            .map(|m| self.chat_effective_effort(m))
            .unwrap_or("");
        let trigger = Button::new("chat-model")
            .debug_selector(|| "chat-model".into())
            .ghost()
            .small()
            .h(px(30.))
            .px(px(9.))
            .max_w_full()
            .rounded(px(9.))
            .disabled(self.chat_ui.streaming)
            .accessibility_label(format!(
                "选择模型：{name}，思考强度{}",
                effort_label(effort)
            ))
            .when(self.chat_ui.model_open, |b| b.bg(rgba(0x38748712)))
            .child(
                div()
                    .flex()
                    .min_w_0()
                    .gap(px(4.))
                    .items_center()
                    .text_size(px(13.))
                    .child(div().min_w_0().truncate().child(name.to_owned()))
                    .when(!effort.is_empty(), |d| {
                        d.child(
                            div()
                                .flex_shrink_0()
                                .text_color(rgb(0x8e8679))
                                .child(format!("· {}", effort_label(effort))),
                        )
                    })
                    .child(div().flex_shrink_0().child("⌄")),
            )
            .on_click(cx.listener(|this, _, window, cx| {
                this.chat_ui.model_open = !this.chat_ui.model_open;
                this.chat_ui.style_open = false;
                this.chat_ui.list_open = false;
                if this.chat_ui.model_open {
                    this.chat_ui.model_cursor = this
                        .settings
                        .committed
                        .models
                        .iter()
                        .position(|m| Some(&m.id) == this.chat_model_profile().map(|m| &m.id))
                        .unwrap_or(0);
                    this.chat_ui
                        .model_scroll
                        .scroll_to_item(this.chat_ui.model_cursor);
                    this.chat_ui.model_focus.focus(window, cx);
                } else {
                    this.question.read(cx).focus_handle(cx).focus(window, cx);
                }
                cx.notify();
            }));
        // Popup uses deferred painting, occludes the text field underneath and
        // clamps to the window. A normal absolute child can paint below Textarea.
        div()
            .min_w_0()
            .max_w(relative(0.56))
            .on_prepaint({
                let view = cx.entity().downgrade();
                move |bounds, _, cx| {
                    let _ = view.update(cx, |this, _| this.chat_ui.model_bounds = bounds);
                }
            })
            .child(
                Popup::new("chat-model-popup", trigger)
                    .anchor(Anchor::BottomLeft)
                    .when(self.chat_ui.model_open, |p| {
                        p.content(self.chat_model_menu(cx))
                    }),
            )
            .into_any_element()
    }

    fn chat_model_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = self
            .chat_model_profile()
            .map(|m| m.id.as_str())
            .unwrap_or("");
        let profile = self.chat_model_profile();
        let list_height = (self.chat_ui.model_bounds.top() - px(200.)).clamp(px(56.), px(240.));
        div()
            .id("chat-model-menu")
            .debug_selector(|| "chat-model-menu".into())
            .w(px(self.chat_ui.model_menu_width))
            .p(px(6.))
            .flex()
            .flex_col()
            .bg(rgb(0xfbfaf7))
            .border_1()
            .border_color(rgb(0xe3ddd2))
            .rounded(px(16.))
            .shadow_md()
            .text_color(rgb(0x27231d))
            .track_focus(&self.chat_ui.model_focus)
            .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                // The trigger owns toggling; closing on its mouse-down would
                // cause its subsequent click to reopen the menu.
                if this.chat_ui.model_open && !this.chat_ui.model_bounds.contains(&event.position) {
                    this.chat_ui.model_open = false;
                    cx.notify();
                }
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if !this.chat_ui.model_open {
                    return;
                }
                let count = this.settings.committed.models.len();
                match event.keystroke.key.as_str() {
                    "escape" => {
                        this.chat_ui.model_open = false;
                        this.question.read(cx).focus_handle(cx).focus(window, cx);
                    }
                    "down" | "up" if count > 0 => {
                        let delta = if event.keystroke.key == "down" {
                            1
                        } else {
                            count - 1
                        };
                        this.chat_ui.model_cursor = (this.chat_ui.model_cursor + delta) % count;
                        this.chat_ui
                            .model_scroll
                            .scroll_to_item(this.chat_ui.model_cursor);
                    }
                    "enter" if count > 0 => {
                        let id = this.settings.committed.models
                            [this.chat_ui.model_cursor.min(count - 1)]
                        .id
                        .clone();
                        this.choose_chat_model(id, window, cx);
                    }
                    _ => return,
                }
                cx.stop_propagation();
                cx.notify();
            }))
            .child(
                div()
                    .id("chat-model-list")
                    .track_scroll(&self.chat_ui.model_scroll)
                    .max_h(list_height)
                    .overflow_y_scroll()
                    .when(self.settings.committed.models.is_empty(), |d| {
                        d.child(
                            div()
                                .p(px(12.))
                                .text_size(px(13.))
                                .text_color(rgb(0x8e8679))
                                .child("还没有可用模型，请先添加模型。"),
                        )
                    })
                    .children(self.settings.committed.models.iter().enumerate().map(
                        |(index, model)| {
                            let id = model.id.clone();
                            let selected = model.id == current;
                            let subtitle = format!(
                                "{}{}",
                                model.provider.label(),
                                if model.id == self.settings.committed.ask_model {
                                    " · 默认"
                                } else {
                                    ""
                                }
                            );
                            Button::new(("chat-model-option", index))
                                .debug_selector(move || format!("chat-model-option-{index}"))
                                .ghost()
                                .w_full()
                                .h(px(54.))
                                .px(px(10.))
                                .rounded(px(8.))
                                .text_left()
                                .accessibility_label(format!(
                                    "{}，{}{}",
                                    model.name,
                                    subtitle,
                                    if selected { "，已选择" } else { "" }
                                ))
                                .when(self.chat_ui.model_cursor == index, |b| {
                                    b.bg(rgba(0x38748709))
                                })
                                .child(
                                    div()
                                        .flex()
                                        .w_full()
                                        .min_w_0()
                                        .items_center()
                                        .justify_between()
                                        .gap(px(12.))
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .min_w_0()
                                                .flex_1()
                                                .gap(px(3.))
                                                .child(
                                                    div()
                                                        .debug_selector(move || {
                                                            format!("chat-model-name-{index}")
                                                        })
                                                        .text_size(px(14.))
                                                        .truncate()
                                                        .text_color(rgb(if selected {
                                                            0x387487
                                                        } else {
                                                            0x27231d
                                                        }))
                                                        .child(model.name.clone()),
                                                )
                                                .child(
                                                    div()
                                                        .debug_selector(move || {
                                                            format!("chat-model-provider-{index}")
                                                        })
                                                        .text_size(px(11.5))
                                                        .text_color(rgb(0x8e8679))
                                                        .truncate()
                                                        .child(subtitle),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .debug_selector(move || {
                                                    format!("chat-model-check-{index}")
                                                })
                                                .w(px(16.))
                                                .flex_shrink_0()
                                                .text_color(rgb(0x387487))
                                                .child(if selected { "✓" } else { "" }),
                                        ),
                                )
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.choose_chat_model(id.clone(), window, cx)
                                }))
                        },
                    )),
            )
            .when_some(
                profile.filter(|m| !m.reasoning_levels().is_empty()),
                |d, model| d.child(self.chat_effort_section(model, cx)),
            )
            .child(
                div()
                    .mx(px(4.))
                    .mt(px(4.))
                    .pt(px(6.))
                    .border_t_1()
                    .border_color(rgb(0xe3ddd2))
                    .child(
                        Button::new("chat-manage-models")
                            .debug_selector(|| "chat-manage-models".into())
                            .ghost()
                            .w_full()
                            .h(px(38.))
                            .px(px(10.))
                            .text_left()
                            .child(
                                div()
                                    .flex()
                                    .w_full()
                                    .gap(px(9.))
                                    .items_center()
                                    .text_size(px(13.))
                                    .text_color(rgb(0x655e52))
                                    .child("⚙")
                                    .child("管理模型…"),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.chat_ui.model_open = false;
                                this.open_settings(window, cx);
                            })),
                    ),
            )
            .into_any_element()
    }

    fn chat_effort_section(&self, model: &ai::ModelProfile, cx: &mut Context<Self>) -> AnyElement {
        let effort = self.chat_effective_effort(model);
        div()
            .debug_selector(|| "chat-thinking-effort".into())
            .mx(px(4.))
            .mt(px(6.))
            .p(px(10.))
            .border_t_1()
            .border_color(rgb(0xe3ddd2))
            .flex()
            .flex_col()
            .gap(px(7.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(12.))
                    .text_color(rgb(0x655e52))
                    .child("思考强度")
                    .when(self.chat_ui.reasoning.is_some(), |d| {
                        d.child(
                            Button::new("chat-effort-reset")
                                .debug_selector(|| "chat-effort-reset".into())
                                .ghost()
                                .xsmall()
                                .label("恢复默认")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.chat_ui.reasoning = None;
                                    cx.notify();
                                })),
                        )
                    }),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(0x8e8679))
                    .child("越高想得越久，仅对当前对话生效。"),
            )
            .child(
                div()
                    .debug_selector(|| "chat-effort-options".into())
                    .flex()
                    .w_full()
                    .gap(px(2.))
                    .p(px(2.))
                    .rounded(px(9.))
                    .bg(rgb(0xf5f2ec))
                    .children(model.reasoning_levels().iter().map(|&value| {
                        let active = value == effort;
                        Button::new(SharedString::from(format!("chat-effort-{value}")))
                            .debug_selector(move || format!("chat-effort-{value}"))
                            .ghost()
                            .flex_1()
                            .min_w_0()
                            .h(px(26.))
                            .px(px(4.))
                            .rounded(px(7.))
                            .role(Role::RadioButton)
                            .toggled(active)
                            .accessibility_label(format!("思考强度：{}", effort_label(value)))
                            .label(effort_label(value))
                            .text_color(rgb(if active { 0x387487 } else { 0x8e8679 }))
                            .when(active, |b| {
                                b.bg(rgb(0xfbfaf7))
                                    .border_1()
                                    .border_color(rgb(0xd4cbbb))
                                    .shadow_sm()
                            })
                            .disabled(self.chat_ui.streaming)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(model) = this.chat_model_profile() {
                                    this.chat_ui.reasoning = Some(
                                        if model.reasoning.is_empty()
                                            && value == model.reasoning_fallback()
                                        {
                                            String::new()
                                        } else {
                                            value.to_owned()
                                        },
                                    );
                                }
                                // Keep the menu open, matching EasyRead's segmented selector.
                                cx.notify();
                            }))
                    })),
            )
            .into_any_element()
    }
}
