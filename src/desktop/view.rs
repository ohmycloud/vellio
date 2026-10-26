//! Top-level reader composition.
use super::*;

impl ReaderView {
    pub(super) fn main_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if self.home.visible {
            return self.home_view(window, cx);
        }
        if !self.settings.open
            && (window.focused(cx).is_none()
                || self.home.focus.is_focused(window)
                || self.settings.focus.is_focused(window))
        {
            self.reader_focus.focus(window, cx);
        }
        for (slider, value) in [
            (&self.font_slider, self.appearance.font_size),
            (&self.width_slider, self.appearance.line_width),
            (&self.spacing_slider, self.appearance.line_spacing),
        ] {
            if (slider.read(cx).value().end() - value).abs() > 0.001 {
                slider.update(cx, |slider, cx| slider.set_value(value, window, cx));
            }
        }
        let reading_order_menu = self.reading_order_menu(window, cx);
        let appearance_menu = self.appearance_menu(window, cx);
        let appearance_will_open = !self.appearance_open;
        if self.restore_inputs {
            self.fill_editor(window, cx);
        }
        let toolbar = self.selection_toolbar(window, cx);
        let annotation_popup = self.annotation_editor(window, cx);
        if self.restore_paper_note {
            self.fill_paper_note(window, cx);
            self.restore_paper_note = false;
        }
        let margin_open = self.margin_editor.is_some();
        let weak = cx.entity().downgrade();
        let navigation_menu = self.navigation_menu(window, cx);
        let reading_index = self.list.logical_scroll_top().item_ix;
        self.update_pdf_focus(reading_index);
        let chapter = self
            .contents()
            .into_iter()
            .rfind(|(i, _, _)| *i <= reading_index)
            .map(|(_, title, _)| title)
            .unwrap_or_else(|| {
                self.state
                    .paper
                    .blocks
                    .get(reading_index)
                    .map(|b| format!("第 {} 页", b.page.max(1)))
                    .unwrap_or_default()
            });
        let title = self.state.title().to_owned();
        let show_status = !matches!(
            self.status.as_str(),
            "" | "就绪 · 草稿与阅读位置自动保存"
                | "论文已打开 · 已恢复草稿与阅读位置"
                | "公式已编译"
                | "笔记已自动保存"
        );
        window.set_window_title(&if title.is_empty() {
            "Vellio".to_owned()
        } else {
            format!("{title} · Vellio")
        });
        let panel = match self.panel {
            Panel::Notes => self.notes(cx),
            Panel::Edit => div()
                .flex()
                .flex_col()
                .gap_3()
                .child("修订译文")
                .when(self.draft_conflict, |d| {
                    d.child(
                        div()
                            .text_color(rgb(0xa34f21))
                            .child("正文已更新；已保留旧草稿，请核对后保存。"),
                    )
                })
                .child(Textarea::new(&self.editor).h(px(280.)))
                .child(
                    Button::new("save-edit")
                        .primary()
                        .label("保存修订")
                        .disabled(self.busy || self.workspace.is_none())
                        .on_click(cx.listener(|this, _, _, cx| this.save_edit(false, cx))),
                )
                .child(
                    Button::new("revert-edit")
                        .label("恢复译者稿")
                        .disabled(self.busy || self.workspace.is_none())
                        .on_click(cx.listener(|this, _, _, cx| this.save_edit(true, cx))),
                )
                .into_any_element(),
            Panel::Chat => div().into_any_element(),
        };
        let page_view = self.page_view(cx);
        let image_preview = self.image_preview_view(window, cx);
        div()
            .id("reader")
            .text_size(px(15.))
            .line_height(relative(typography::UI_LINE_HEIGHT))
            .track_focus(&self.reader_focus)
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(READER_BACKGROUND))
            .text_color(rgb(typography::READER_INK))
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.inline_edit.is_some() {
                    if event.keystroke.key == "escape" {
                        this.cancel_inline_edit(window, cx);
                        cx.stop_propagation();
                    } else if event.keystroke.key == "enter"
                        && (event.keystroke.modifiers.control || event.keystroke.modifiers.platform)
                    {
                        this.save_inline_edit(cx);
                        cx.stop_propagation();
                    }
                    // Other keys belong to the textarea, never reader shortcuts.
                    return;
                }
                if this.image_preview.is_some() {
                    if event.keystroke.key == "escape" {
                        this.image_preview = None;
                        this.reader_focus.focus(window, cx);
                        cx.notify();
                    }
                    cx.stop_propagation();
                } else if event.keystroke.key == "escape" && this.reading_order_open {
                    this.reading_order_open = false;
                    this.reader_focus.focus(window, cx);
                    cx.notify();
                    cx.stop_propagation();
                } else if event.keystroke.key == "escape" && this.appearance_open {
                    this.appearance_open = false;
                    window.blur(cx);
                    this.flush_session();
                    cx.notify();
                    cx.stop_propagation();
                } else if event.keystroke.key == "escape" && this.navigation.is_some() {
                    this.navigation = None;
                    window.blur(cx);
                    cx.notify();
                    cx.stop_propagation();
                } else if event.keystroke.key == "escape" && this.margin_editor.is_some() {
                    this.close_margin_note(window, cx);
                    cx.stop_propagation();
                } else if event.keystroke.key == "escape"
                    && this.panel == Panel::Chat
                    && (this.chat_ui.model_open
                        || this.chat_ui.style_open
                        || this.chat_ui.list_open)
                {
                    this.chat_ui.model_open = false;
                    this.chat_ui.style_open = false;
                    this.chat_ui.list_open = false;
                    this.question.read(cx).focus_handle(cx).focus(window, cx);
                    cx.notify();
                    cx.stop_propagation();
                } else if event.keystroke.key == "escape"
                    && this.panel == Panel::Chat
                    && this.question.read(cx).focus_handle(cx).is_focused(window)
                {
                    window.blur(cx);
                    cx.stop_propagation();
                } else if event.keystroke.key == "escape" && this.panel_open {
                    this.show_panel(this.panel, window, cx);
                    cx.stop_propagation();
                } else if event.keystroke.key == "escape" && this.selection.is_some() {
                    this.selection = None;
                    TextSelection::clear(window, cx);
                    cx.stop_propagation();
                    cx.notify();
                } else if !this.chat_ui.model_open && !this.chat_ui.style_open {
                    this.reader_shortcut(event, window, cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    let in_reading_order = this
                        .reading_order_button_bounds
                        .is_some_and(|bounds| bounds.contains(&event.position))
                        || (this.reading_order_open
                            && this
                                .reading_order_menu_bounds
                                .is_some_and(|bounds| bounds.contains(&event.position)));
                    if !in_reading_order {
                        this.reading_order_open = false;
                    }
                    let in_appearance = this
                        .appearance_button_bounds
                        .is_some_and(|bounds| bounds.contains(&event.position))
                        || (this.appearance_open
                            && this
                                .appearance_menu_bounds
                                .is_some_and(|bounds| bounds.contains(&event.position)));
                    if !in_appearance {
                        this.appearance_open = false;
                    }
                    let in_navigation = this
                        .navigation_button_bounds
                        .iter()
                        .flatten()
                        .any(|bounds| bounds.contains(&event.position))
                        || (this.navigation.is_some()
                            && this
                                .navigation_menu_bounds
                                .is_some_and(|bounds| bounds.contains(&event.position)));
                    if !in_navigation {
                        this.navigation = None;
                    }
                    if this.margin_editor.is_some() {
                        this.close_margin_note(window, cx);
                    }
                    this.selection = None;
                    this.annotation_popup = None;
                    cx.notify();
                }),
            )
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                if this.is_epub()
                    && !this.appearance_open
                    && !this.reading_order_open
                    && this.navigation.is_none()
                {
                    this.epub_wheel(event, cx);
                }
                if this.list.viewport_bounds().contains(&event.position) {
                    this.cancel_annotation_scroll();
                }
                this.selection = None;
                this.annotation_popup = None;
                cx.notify();
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|_, event: &MouseUpEvent, window, cx| {
                    let position = event.position;
                    let weak = cx.entity().downgrade();
                    // Selection geometry is projected into text ranges during
                    // prepaint. Read it after the release frame has painted.
                    window.on_next_frame(move |window, _| {
                        window.on_next_frame(move |_, cx| {
                            let _ =
                                weak.update(cx, |this, cx| this.capture_selection(position, cx));
                        });
                    });
                }),
            )
            .child(
                div()
                    .h(px(56.))
                    .flex_shrink_0()
                    .px_4()
                    .flex()
                    .items_center()
                    .gap_2()
                    // Toolbar releases must not schedule body text selection
                    // capture while a panel button is changing the layout.
                    .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .border_b_1()
                    .border_color(rgb(0xe3ddd2))
                    .child(self.navigation_button(Navigation::Library, cx))
                    .child(self.navigation_button(Navigation::Contents, cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(
                                div()
                                    .min_w_0()
                                    .max_w(px(420.))
                                    .truncate()
                                    .text_size(px(14.))
                                    .line_height(relative(1.2))
                                    .font_weight(FontWeight::MEDIUM)
                                    .font_family(self.fonts.title(&title))
                                    .child(if title.is_empty() {
                                        "Vellio".to_owned()
                                    } else {
                                        title
                                    }),
                            )
                            .when(!chapter.is_empty(), |d| {
                                d.child(div().text_color(rgb(0x969084)).child("·")).child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .text_size(px(12.5))
                                        .text_color(rgb(0x969084))
                                        .child(chapter),
                                )
                            }),
                    )
                    .when(show_status, |d| {
                        d.child(
                            div()
                                .debug_selector(|| "reader-status".into())
                                .max_w(px(180.))
                                .truncate()
                                .text_size(px(12.))
                                .text_color(rgb(0x969084))
                                .child(self.status.clone()),
                        )
                    })
                    .when(self.settings.committed.features.original, |d| {
                        d.child(self.reading_mode_control(cx))
                    })
                    .when(self.busy && self.home.cancellable_task, |d| {
                        d.child(
                            div().debug_selector(|| "reader-cancel-task".into()).child(
                                Button::new("cancel")
                                    .ghost()
                                    .small()
                                    .label("取消")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.cancel.store(true, Ordering::Relaxed);
                                        this.status = "正在取消……".into();
                                        cx.notify();
                                    })),
                            ),
                        )
                    })
                    .when(self.settings.committed.features.page, |d| {
                        d.child(
                            Button::new("pdf-toggle")
                                .ghost()
                                .small()
                                .icon(Icon::default().path("icons/file.svg").size(px(17.)))
                                .label(if self.is_epub() { "EPUB" } else { "PDF" })
                                .rounded(px(9.))
                                .text_color(rgb(0x625e55))
                                .when(self.show_pdf, |b| {
                                    b.bg(rgba(0x38748720)).text_color(rgb(0x387487))
                                })
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if this.show_pdf {
                                        this.show_pdf = false;
                                        this.reset_rows();
                                        cx.notify();
                                    } else {
                                        let index = this.list.logical_scroll_top().item_ix;
                                        this.locate_source(index, cx);
                                    }
                                })),
                        )
                    })
                    .child(
                        Button::new("notes-tab")
                            .ghost()
                            .small()
                            .icon(
                                Icon::default()
                                    .path("icons/message-square.svg")
                                    .size(px(17.)),
                            )
                            .label("Notes")
                            .rounded(px(9.))
                            .text_color(rgb(0x625e55))
                            .when(self.panel_open && self.panel == Panel::Notes, |b| {
                                b.bg(rgba(0x38748720)).text_color(rgb(0x387487))
                            })
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.show_panel(Panel::Notes, window, cx)
                            })),
                    )
                    .when(self.settings.committed.features.ask_ai, |d| {
                        d.child(
                            Button::new("chat-tab")
                                .ghost()
                                .small()
                                .icon(Icon::default().path("icons/asterisk.svg").size(px(17.)))
                                .label("Ask AI")
                                .rounded(px(9.))
                                .text_color(rgb(0x625e55))
                                .when(self.panel_open && self.panel == Panel::Chat, |b| {
                                    b.bg(rgba(0x38748720)).text_color(rgb(0x387487))
                                })
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.show_panel(Panel::Chat, window, cx)
                                })),
                        )
                    })
                    .child(
                        div()
                            .relative()
                            .child({
                                let weak = cx.entity().downgrade();
                                canvas(
                                    move |bounds, _, cx| {
                                        let _ = weak.update(cx, |this, _| {
                                            this.appearance_button_bounds = Some(bounds)
                                        });
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full()
                            })
                            .child(
                                Button::new("appearance-toggle")
                                    .ghost()
                                    .small()
                                    .label("Aa")
                                    .accessibility_label("Reading appearance")
                                    .rounded(px(9.))
                                    .text_color(rgb(0x625e55))
                                    .when(self.appearance_open, |b| {
                                        b.bg(rgba(0x38748720)).text_color(rgb(0x387487))
                                    })
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.navigation = None;
                                        this.reading_order_open = false;
                                        this.appearance_open = appearance_will_open;
                                        this.selection = None;
                                        this.annotation_popup = None;
                                        if this.appearance_open {
                                            this.appearance_focus.focus(window, cx);
                                        } else {
                                            window.blur(cx);
                                            this.flush_session();
                                        }
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(self.reading_progress_bar(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .relative()
                    .overflow_hidden()
                    .child(
                        div()
                            .debug_selector(|| "reader-column".into())
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .when(self.state.paper.blocks.is_empty(), |d| {
                                d.child(div().p_8().child(if self.workspace.is_some() {
                                    "该 PDF 没有可提取文字，可在原页查看；扫描件的 OCR 尚未接入。"
                                } else {
                                    "导入 PDF / EPUB 开始阅读，或迁入已有论文。"
                                }))
                            })
                            .child(if self.is_epub() {
                                self.epub_pages_view(cx)
                            } else {
                                list(self.list.clone(), move |i, _, cx| {
                                    weak.update(cx, |this, cx| this.block(i, cx))
                                        .unwrap_or_else(|_| div().into_any_element())
                                })
                                .w_full()
                                .flex_1()
                                .min_h_0()
                                .into_any_element()
                            }),
                    )
                    .when(self.show_pdf && !margin_open && !self.panel_open, |d| {
                        d.child(
                            div()
                                .id("pdf-panel")
                                .debug_selector(|| "source-panel".into())
                                .w(px(260.))
                                .min_h_0()
                                .overflow_y_scroll()
                                .border_l_1()
                                .border_color(rgb(0xe3ddd2))
                                .p_3()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .when(self.is_epub(), |d| {
                                            d.child(
                                                div()
                                                    .flex_1()
                                                    .text_size(px(13.))
                                                    .child(format!("EPUB p.{} · 原文", self.page)),
                                            )
                                        })
                                        .when(!self.is_epub(), |d| {
                                            d.child(
                                                Button::new("prev-page")
                                                    .label("上一页")
                                                    .disabled(self.page <= 1)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.page_block = None;
                                                        this.page -= 1;
                                                        this.load_page();
                                                        cx.notify();
                                                    })),
                                            )
                                            .child(format!(
                                                "{} / {}",
                                                self.page,
                                                self.state.paper.meta.pages.len()
                                            ))
                                            .child(
                                                Button::new("next-page")
                                                    .label("下一页")
                                                    .disabled(
                                                        self.page
                                                            >= self.state.paper.meta.pages.len(),
                                                    )
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.page_block = None;
                                                        this.page += 1;
                                                        this.load_page();
                                                        cx.notify();
                                                    })),
                                            )
                                        })
                                        .child(
                                            Button::new("close-source-panel")
                                                .ghost()
                                                .xsmall()
                                                .label("×")
                                                .accessibility_label("Close source")
                                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                                    cx.stop_propagation()
                                                })
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.show_pdf = false;
                                                    this.reset_rows();
                                                    cx.notify();
                                                })),
                                        ),
                                )
                                .child(page_view),
                        )
                    })
                    .when(
                        self.tool_panel_visible() && self.panel == Panel::Chat,
                        |d| d.child(self.chat_column(window, cx)),
                    )
                    .when(self.panel_open && self.panel != Panel::Chat, |d| {
                        d.child(
                            div()
                                .w(px(420.))
                                .flex_shrink_0()
                                .min_h_0()
                                .flex()
                                .flex_col()
                                .border_l_1()
                                .border_color(rgb(0xe3ddd2))
                                .bg(rgb(0xfbfaf7))
                                .p(px(16.))
                                .gap(px(12.))
                                .when(self.panel != Panel::Notes, |d| {
                                    d.child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.))
                                            .child(div().flex_1().text_size(px(13.)).child(
                                                if self.panel == Panel::Chat {
                                                    "Ask AI"
                                                } else {
                                                    "Edit translation"
                                                },
                                            ))
                                            .child(
                                                Button::new("close-tool-panel")
                                                    .ghost()
                                                    .w(px(24.))
                                                    .h(px(28.))
                                                    .p_0()
                                                    .label("×")
                                                    .accessibility_label("Close panel")
                                                    .on_click(cx.listener(
                                                        |this, _, window, cx| {
                                                            this.show_panel(this.panel, window, cx)
                                                        },
                                                    )),
                                            ),
                                    )
                                })
                                .child(
                                    div()
                                        .id("notes-scroll")
                                        .flex_1()
                                        .min_h_0()
                                        .when(self.panel != Panel::Chat, |d| d.overflow_y_scroll())
                                        .child(panel),
                                ),
                        )
                    }),
            )
            .when_some(toolbar, |d, toolbar| d.child(toolbar))
            .when_some(annotation_popup, |d, popup| d.child(popup))
            .when_some(appearance_menu, |d, menu| d.child(menu))
            .when_some(reading_order_menu, |d, menu| d.child(menu))
            .when_some(navigation_menu, |d, menu| d.child(menu))
            .when_some(image_preview, |d, preview| d.child(preview))
            .into_any_element()
    }
}
impl Render for ReaderView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.settings.translation_confirm {
            return self.translation_confirmation_view(cx);
        }
        let background = self.main_view(window, cx);
        if !self.settings.open {
            return background;
        }
        let settings = self.settings_modal(window, cx);
        div()
            .id("reader-with-settings")
            .relative()
            .size_full()
            .child(background)
            .child(settings)
            .into_any_element()
    }
}
