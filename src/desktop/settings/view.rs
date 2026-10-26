//! Settings view.
use super::*;

impl ReaderView {
    pub(in crate::desktop) fn settings_modal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let panel = self.settings_view(window, cx);
        Modal::new(cx)
            .focus_handle(self.settings.focus.clone())
            .close_on_escape(false)
            .close_on_backdrop_press(false)
            .flex()
            .items_center()
            .justify_center()
            .backdrop(
                div()
                    .size_full()
                    .occlude()
                    .bg(rgba(0x00000044))
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation()),
            )
            .popup(panel)
            .into_any_element()
    }

    pub(in crate::desktop) fn settings_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let viewport = window.viewport_size();
        let modal_side = (viewport.width - px(64.))
            .min(viewport.height - px(96.))
            .max(px(0.))
            .min(px(840.));
        let modal_width = modal_side;
        let modal_height = modal_side;
        let compact = modal_width < px(960.);
        let body = if self.settings.log_open {
            div()
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(section("Log"))
                .child(muted(self.status.clone()))
                .children(self.settings.logs.iter().rev().map(|s| muted(s.clone())))
                .into_any_element()
        } else {
            match self.settings.tab {
                0 => self.model_settings(cx),
                1 => self.reading_settings(cx),
                2 => self.sidebar_settings(cx),
                3 => self.cloud_settings(cx),
                4 => self.shortcut_settings(cx),
                _ => self.about_settings(cx),
            }
        };
        div()
            .id("settings-surface")
            .occlude()
            .w(modal_width)
            .h(modal_height)
            .flex_shrink_0()
            .rounded(px(18.))
            .overflow_hidden()
            .shadow(vec![
                BoxShadow::new(px(0.), px(18.), rgba(0x403a2d40).into()).blur_radius(px(44.)),
            ])
            .flex()
            .flex_col()
            .bg(rgb(PAPER))
            .text_color(rgb(INK))
            .text_size(px(typography::UI_SIZE))
            .line_height(relative(typography::UI_LINE_HEIGHT))
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = &event.keystroke.key;
                if let Some(id) = this.settings.recording.take() {
                    if key != "escape" {
                        let normalized = key.to_lowercase();
                        let conflict =
                            this.settings.draft.shortcuts.iter().any(|(other, k)| {
                                other != &id && !k.is_empty() && k == &normalized
                            });
                        if normalized.chars().count() != 1
                            || conflict
                            || matches!(normalized.as_str(), "1" | "2" | "3" | "4")
                        {
                            this.settings.notice =
                                "Choose one unused key; 1–4 are reserved for highlights.".into();
                        } else {
                            this.settings.draft.shortcuts.insert(id, normalized);
                            this.settings.notice.clear();
                        }
                    }
                    cx.stop_propagation();
                    cx.notify();
                } else if key == "escape" {
                    if this.settings.model_edit.is_some() {
                        this.settings.model_edit = None;
                        cx.notify();
                    } else {
                        this.close_settings(false, window, cx)
                    }
                    cx.stop_propagation();
                }
            }))
            .child(
                div()
                    .id("settings-tabs")
                    .overflow_x_scroll()
                    .h(px(72.))
                    .flex_shrink_0()
                    .px(px(28.))
                    .flex()
                    .items_center()
                    .gap(px(if compact { 8. } else { 22. }))
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .font_family(gpui_kit::component::Theme::global(cx).font_family.clone())
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(20.))
                            .mr(px(6.))
                            .flex_shrink_0()
                            .child("Settings"),
                    )
                    .children(TABS.into_iter().enumerate().map(|(i, title)| {
                        Button::new(("settings-tab", i))
                            .debug_selector(move || format!("settings-tab-{i}"))
                            .ghost()
                            .h_full()
                            .px(px(if compact { 8. } else { 14. }))
                            .flex_shrink_0()
                            .rounded_none()
                            .text_size(px(14.))
                            .border_b_2()
                            .border_color(if i == self.settings.tab && !self.settings.log_open {
                                rgb(ACCENT)
                            } else {
                                rgba(0x00000000)
                            })
                            .text_color(rgb(if i == self.settings.tab && !self.settings.log_open {
                                INK
                            } else {
                                MUTED
                            }))
                            .label(title)
                            .disabled(self.settings.relocating)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.settings.tab = i;
                                this.settings.scroll = ScrollHandle::new();
                                this.settings.log_open = false;
                                this.settings.model_edit = None;
                                this.settings.recording = None;
                                this.settings.focus.focus(window, cx);
                                if i == 5 {
                                    this.check_updates(false, cx);
                                }
                                cx.notify();
                            }))
                    })),
            )
            .child(
                div()
                    .id("settings-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.settings.scroll)
                    .px(px(28.))
                    .py(px(26.))
                    .child(body),
            )
            .when(!self.settings.notice.is_empty(), |d| {
                d.child(
                    div()
                        .flex_shrink_0()
                        .px(px(28.))
                        .py(px(10.))
                        .bg(rgb(0xf0eee6))
                        .text_size(px(13.))
                        .text_color(rgb(0x625e55))
                        .child(self.settings.notice.clone())
                        .when(self.busy, |d| {
                            d.child(
                                Button::new("cancel-settings-task")
                                    .ghost()
                                    .small()
                                    .label("Cancel task")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.cancel.store(true, Ordering::Relaxed);
                                        cx.notify()
                                    })),
                            )
                        }),
                )
            })
            .child(
                div()
                    .h(px(76.))
                    .flex_shrink_0()
                    .px(px(28.))
                    .flex()
                    .items_center()
                    .gap(px(18.))
                    .border_t_1()
                    .border_color(rgb(BORDER))
                    .child(
                        Button::new("settings-log")
                            .ghost()
                            .label("Log")
                            .text_color(rgb(ACCENT))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.settings.log_open = !this.settings.log_open;
                                this.settings.scroll = ScrollHandle::new();
                                cx.notify()
                            })),
                    )
                    .child(
                        Button::new("settings-about-link")
                            .ghost()
                            .label("About / Updates")
                            .disabled(self.settings.relocating)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.settings.show_about();
                                this.check_updates(false, cx);
                                cx.notify();
                            })),
                    )
                    .child(div().flex_1())
                    .when(matches!(self.settings.tab, 0 | 5), |d| {
                        d.child(
                            Button::new("settings-close")
                                .ghost()
                                .label("Close")
                                .disabled(self.settings.relocating)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_settings(false, window, cx)
                                })),
                        )
                    })
                    .when(!matches!(self.settings.tab, 0 | 5), |d| {
                        d.child(
                            Button::new("settings-cancel")
                                .ghost()
                                .label("Cancel")
                                .disabled(self.settings.relocating)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_settings(false, window, cx)
                                })),
                        )
                        .child(
                            primary("settings-save", "Save")
                                .bg(rgb(INK))
                                .disabled(self.settings.relocating)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_settings(true, window, cx)
                                })),
                        )
                    }),
            )
            .into_any_element()
    }

    pub(in crate::desktop) fn translation_confirmation_view(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pages = self.state.paper.meta.pages.len().max(
            self.state
                .paper
                .blocks
                .iter()
                .map(|b| b.page)
                .max()
                .unwrap_or(0),
        );
        let model = self
            .settings
            .committed
            .models
            .iter()
            .find(|m| m.id == self.settings.committed.translation_model)
            .map(|m| m.name.as_str())
            .unwrap_or("configured model");
        div().size_full().flex().items_center().justify_center().bg(rgb(PAPER)).text_color(rgb(INK))
            .child(div().w(px(480.)).p(px(28.)).border_1().border_color(rgb(BORDER)).rounded(px(16.)).flex().flex_col().gap(px(20.))
                .child(div().text_size(px(22.)).font_weight(FontWeight::SEMIBOLD).child("Translate this paper?"))
                .child(muted(format!("This paper has {pages} pages. Untranslated text will be sent to {model}. Completed translations, edits and notes are retained.")))
                .child(div().flex().justify_end().gap(px(12.))
                    .child(outline("cancel-translation","Cancel").on_click(cx.listener(|this,_,_,cx|{this.settings.translation_confirm=false;cx.notify()})))
                    .child(primary("confirm-translation","Translate").on_click(cx.listener(|this,_,_,cx|{this.settings.translation_confirm=false;this.start_translate(cx);cx.notify()})))))
            .capture_key_down(cx.listener(|this,event:&KeyDownEvent,_,cx|{if event.keystroke.key=="escape"{this.settings.translation_confirm=false;cx.stop_propagation();cx.notify();}})).into_any_element()
    }
}
