//! Text selection capture and the floating selection toolbar.
use super::*;

impl ReaderView {
    pub(super) fn capture_selection(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        if self.inline_edit.is_some() {
            return;
        }
        let selected: Vec<_> = self
            .text_views
            .iter()
            .filter_map(|((index, original), (_, view))| {
                if !self.rendered_languages(*index).contains(original) {
                    return None;
                }
                let state = view.read(cx);
                // The HTML renderer appends a paragraph separator even for
                // a partial selection; it isn't part of the user's quote.
                let quote = state.selected_text().trim_end_matches('\n').to_owned();
                (!state.is_selecting() && !quote.trim().is_empty())
                    .then_some((*index, *original, quote))
            })
            .collect();
        self.selection = None;
        // Like the Python reader, annotations belong to one paragraph/item.
        if let [(index, original, quote)] = selected.as_slice() {
            let b = &self.state.paper.blocks[*index];
            let text = |key: &str, zh: &str, en: &str| {
                if *original {
                    if b.source_kind == "pdf-text" {
                        easyread_gpui::model::pdf_prose(en)
                    } else {
                        en.to_owned()
                    }
                } else {
                    let text = self.state.text(key, zh, en);
                    if b.source_kind == "pdf-text" && text == en {
                        easyread_gpui::model::pdf_prose(text)
                    } else {
                        text.to_owned()
                    }
                }
            };
            let sources: Vec<(String, String)> = match b.kind.as_str() {
                "para" | "heading" | "note" => vec![(b.id.clone(), text(&b.id, &b.zh, &b.en))],
                "figure" | "table" => {
                    let key = format!("{}#caption", b.id);
                    vec![(key.clone(), text(&key, &b.caption_zh, &b.caption_en))]
                }
                "list" => b
                    .items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| {
                        let key = format!("{}#{i}", b.id);
                        (key.clone(), text(&key, &item.zh, &item.en))
                    })
                    .collect(),
                _ => vec![],
            };
            for (key, source) in sources {
                // Recovered PDF formulas are atomic TeX nodes in the rendered
                // text, not the separate glyphs in the stored extraction. Use
                // the same undecorated HTML as the selectable TextView, so
                // quotes and annotation contexts share its representation.
                // Lists still anchor each item to its own persisted key.
                let html = if b.kind == "list" {
                    formula::native_markdown_html(&source)
                } else {
                    self.reader_layout_html(*index, *original)
                };
                let text = annotations::plain_text(&html);
                let native = self
                    .active_reading_paragraph
                    .filter(|(block, language, _)| block == index && language == original)
                    .and_then(|(_, _, paragraph)| {
                        self.text_views
                            .get(&(*index, *original))?
                            .1
                            .read(cx)
                            .paragraph_text_context(paragraph)
                    });
                let range = if let Some((native_text, paragraph)) = native {
                    annotations::rendered_paragraph_quote(
                        &text,
                        &native_text,
                        paragraph,
                        Some(quote),
                    )
                    .map(|(_, range)| range)
                } else {
                    annotations::quote_range(&text, quote, "", "")
                };
                if let Some(range) = range {
                    let (prefix, suffix) = annotations::context(&text, range);
                    self.selection = Some(Selection {
                        block: *index,
                        key,
                        quote: quote.clone(),
                        prefix,
                        suffix,
                        position,
                        underline: false,
                    });
                    break;
                }
            }
        }
        // A single click also opens the paragraph toolbar. Text-range
        // selections take precedence, and stale/hidden views never hit-test.
        if selected.is_empty()
            && self.selection.is_none()
            && let Some((index, original, paragraph)) = self.active_reading_paragraph
        {
            let hit = self.rendered_languages(index).contains(&original)
                && self
                    .text_views
                    .get(&(index, original))
                    .is_some_and(|(_, view)| {
                        view.read(cx).paragraph_at(position) == Some(paragraph)
                    });
            let block = &self.state.paper.blocks[index];
            if hit
                && matches!(
                    block.kind.as_str(),
                    "para" | "heading" | "note" | "figure" | "table"
                )
            {
                let text = annotations::plain_text(&self.reader_layout_html(index, original));
                let scoped = self
                    .text_views
                    .get(&(index, original))
                    .and_then(|(_, view)| view.read(cx).paragraph_text_context(paragraph))
                    .and_then(|(native_text, range)| {
                        annotations::rendered_paragraph_quote(&text, &native_text, range, None)
                    });
                if let Some((quote, range)) = scoped {
                    let (prefix, suffix) = annotations::context(&text, range);
                    self.selection = Some(Selection {
                        block: index,
                        key: if matches!(block.kind.as_str(), "figure" | "table") {
                            format!("{}#caption", block.id)
                        } else {
                            block.id.clone()
                        },
                        quote,
                        prefix,
                        suffix,
                        position,
                        underline: false,
                    });
                }
            }
        }
        if let Some(selection) = &self.selection {
            self.annotation_popup = None;
            self.follow_block(selection.block);
        }
        cx.notify();
    }

    pub(super) fn selection_action(
        &mut self,
        action: &'static str,
        color: HighlightColor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if (action == "chat" && !self.settings.committed.features.ask_ai)
            || (action == "original" && !self.settings.committed.features.original)
            || (action == "edit" && !self.settings.committed.features.edit)
            || (action == "page" && !self.settings.committed.features.page)
        {
            return;
        }
        let Some(selection) = self.selection.take() else {
            return;
        };
        TextSelection::clear(window, cx);
        if action == "page" {
            self.locate_source(selection.block, cx);
        } else if action == "edit" {
            self.begin_inline_edit(selection.block, window, cx);
        } else if action == "copy" {
            cx.write_to_clipboard(ClipboardItem::new_string(selection.quote));
            self.status = "已复制".into();
        } else if action == "original" {
            self.toggle_block_language(selection.block, cx);
        } else {
            let annotation = self.annotation_popup.clone();
            self.select(selection.block, window, cx);
            if action == "highlight" || action == "underline" {
                let b = &self.state.paper.blocks[selection.block];
                let stamp = now();
                let mut note = json!({"id":format!("n{}",Uuid::new_v4().simple()),"anchor":b.id,"key":selection.key,
                    "quote":selection.quote,"prefix":selection.prefix,"suffix":selection.suffix,
                    "kind":"highlight","color":color.name(),"body":"","created":stamp,"updated":stamp});
                if action == "underline" {
                    note["style"] = json!("underline");
                }
                self.save_ops(vec![json!({"op":"note","note":note})], cx);
            } else if action == "chat" {
                self.add_chat_reference(ai::ChatReference {
                    anchor: self.state.paper.blocks[selection.block].id.clone(),
                    quote: selection.quote,
                });
                self.open_chat(window, cx);
            } else {
                self.annotation_popup = annotation;
                self.open_margin_note(selection, action, color, window, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn selection_toolbar(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let selection = self.selection.as_ref()?;
        let underline = selection.underline;
        let can_toggle_original = self.selection_can_toggle_language(selection.block);
        let showing_original = self.selection_secondary_visible(selection.block);
        let secondary_original = !self.primary_original();
        let viewport = window.viewport_size();
        let source_label = format!(
            "{} p.{}",
            if self.is_epub() { "EPUB" } else { "PDF" },
            self.source_page_number(selection.block)
                .map(|page| page.to_string())
                .unwrap_or_else(|| "…".into()),
        );
        let features = &self.settings.committed.features;
        // Match the sum of the button groups, separators and outer padding.
        // Hidden actions must not leave empty space behind in either layout.
        let toolbar_width = |compact: bool| {
            let button = if compact { 24. } else { 28. };
            let gap = if compact { 0. } else { 2. };
            let group_width = |widths: &[f32]| {
                let count = widths.iter().filter(|&&width| width > 0.).count();
                widths.iter().sum::<f32>() + gap * count.saturating_sub(1) as f32
            };
            let notes = group_width(&[
                if compact { button } else { 60. },
                if compact { button } else { 82. },
                if features.ask_ai {
                    if compact { button } else { 70. }
                } else {
                    0.
                },
            ]);
            let tools = group_width(&[
                if features.original {
                    if compact { button } else { 80. }
                } else {
                    0.
                },
                if features.edit {
                    if compact { button } else { 76. }
                } else {
                    0.
                },
                if features.page {
                    if compact { button } else { 90. }
                } else {
                    0.
                },
                button,
            ]);
            14. // Outer padding and border.
                + button * 2. + 4. // Mark style group.
                + 4. + 4. * if compact { 20. } else { 22. } // Color group.
                + 2. * if compact { 7. } else { 9. } // Separators.
                + notes + tools
        };
        let full_width = toolbar_width(false);
        let compact = viewport.width < px(full_width + 16.);
        let width = px(toolbar_width(compact)).min((viewport.width - px(16.)).max(px(1.)));
        let left = (selection.position.x - width / 2.)
            .max(px(8.))
            .min((viewport.width - width - px(8.)).max(px(8.)));
        let top = if selection.position.y > px(76.) {
            selection.position.y - px(54.)
        } else {
            selection.position.y + px(16.)
        };
        let locked = self.busy || self.workspace.is_none();
        let icon_size = if compact { 15. } else { 16. };
        let button_size = if compact { 24. } else { 28. };
        let action_button =
            |action: &'static str, label: &'static str, icon: &'static str, full_width: f32| {
                let disabled = match action {
                    "highlight" | "underline" | "note" | "question" => locked,
                    "original" => !can_toggle_original,
                    "edit" => self.busy,
                    _ => false,
                };
                Button::new(SharedString::from(format!("selection-{action}")))
                    .ghost()
                    .accessibility_label(label)
                    .rounded(px(8.))
                    .w(px(if compact { button_size } else { full_width }))
                    .h(px(button_size))
                    .p_0()
                    .text_color(rgb(0x5f5a51))
                    .when(matches!(action, "highlight" | "underline"), |b| {
                        if (action == "underline") == underline {
                            b.bg(rgb(0xfbfaf7)).shadow(vec![
                                BoxShadow::new(px(0.), px(1.), rgba(0x52452c12).into())
                                    .blur_radius(px(3.)),
                            ])
                        } else {
                            b.text_color(rgb(0x928b7d))
                        }
                    })
                    .when(disabled, |b| b.opacity(0.4))
                    .disabled(disabled)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .text_size(px(13.))
                            .child(Icon::default().path(icon).size(px(icon_size)))
                            .when(
                                !compact && !matches!(action, "highlight" | "underline" | "copy"),
                                |d| d.child(label),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if matches!(action, "highlight" | "underline") {
                            if let Some(selection) = this.selection.as_mut() {
                                selection.underline = action == "underline";
                            }
                            cx.notify();
                        } else {
                            this.selection_action(action, HighlightColor::Yellow, window, cx)
                        }
                    }))
            };
        let separator = || {
            div()
                .flex_shrink_0()
                .w(px(1.))
                .h(px(if compact { 18. } else { 22. }))
                .mx(px(if compact { 3. } else { 4. }))
                .bg(rgb(0xe3ddcf))
        };
        Some(
            deferred(
                div()
                    .id("selection-toolbar")
                    .debug_selector(|| "selection-toolbar".into())
                    .absolute()
                    .left(left)
                    .top(top.max(px(8.)).min((viewport.height - px(56.)).max(px(8.))))
                    .w(width)
                    .h(px(if compact { 40. } else { 42. }))
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .bg(rgb(0xfbfaf7))
                    .border_1()
                    .border_color(rgb(0xe2dccf))
                    .rounded(px(14.))
                    .shadow(vec![
                        BoxShadow::new(px(0.), px(3.), rgba(0x52452c16).into())
                            .blur_radius(px(12.)),
                    ])
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .p(px(2.))
                            .rounded(px(10.))
                            .bg(rgb(0xeee9df))
                            .child(action_button(
                                "highlight",
                                "高亮",
                                "icons/highlighter.svg",
                                button_size,
                            ))
                            .child(action_button(
                                "underline",
                                "下划线",
                                "icons/underline.svg",
                                button_size,
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .ml(px(4.))
                            .children(HighlightColor::ALL.map(|color| {
                                let label = format!(
                                    "{}{}",
                                    color.label(),
                                    if underline { "下划线" } else { "高亮" }
                                );
                                Button::new(("selection-color", color as usize))
                                    .ghost()
                                    .accessibility_label(label)
                                    .w(px(if compact { 20. } else { 22. }))
                                    .h(px(button_size))
                                    .p_0()
                                    .rounded(px(8.))
                                    .disabled(locked)
                                    .when(locked, |b| b.opacity(0.4))
                                    .child(
                                        div()
                                            .size(px(if compact { 15. } else { 18. }))
                                            .rounded(px(if underline { 2. } else { 20. }))
                                            .bg(rgb(if underline {
                                                color.line_rgb()
                                            } else {
                                                match color {
                                                    HighlightColor::Yellow => 0xf7e6bd,
                                                    HighlightColor::Green => 0xd4e4cd,
                                                    HighlightColor::Blue => 0xd2e0ef,
                                                    HighlightColor::Red => 0xf6d4d1,
                                                }
                                            }))
                                            .border_1()
                                            .border_color(rgba(0x6a604b38)),
                                    )
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.selection_action(
                                            if underline { "underline" } else { "highlight" },
                                            color,
                                            window,
                                            cx,
                                        )
                                    }))
                            })),
                    )
                    .child(separator())
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap(px(if compact { 0. } else { 2. }))
                            .child(action_button(
                                "note",
                                "Notes",
                                "icons/message-square.svg",
                                60.,
                            ))
                            .child(action_button(
                                "question",
                                "Question",
                                "icons/circle-question-mark.svg",
                                82.,
                            ))
                            .when(self.settings.committed.features.ask_ai, |d| {
                                d.child(action_button("chat", "Ask AI", "icons/asterisk.svg", 70.))
                            }),
                    )
                    .child(separator())
                    .child(
                        div()
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .gap(px(if compact { 0. } else { 2. }))
                            .when(self.settings.committed.features.original, |d| {
                                d.child(
                                    div()
                                        .debug_selector(|| "selection-language-toggle".into())
                                        .child(
                                            Button::new("selection-original")
                                                .ghost()
                                                .accessibility_label(
                                                    if self.selection_is_display_equation(
                                                        selection.block,
                                                    ) {
                                                        "独立公式无需翻译"
                                                    } else {
                                                        match (secondary_original, showing_original)
                                                        {
                                                            (true, true) => "收起原文",
                                                            (true, false) => "显示原文",
                                                            (false, true) => "收起译文",
                                                            (false, false) => "显示译文",
                                                        }
                                                    },
                                                )
                                                .rounded(px(8.))
                                                .w(px(if compact { button_size } else { 80. }))
                                                .h(px(button_size))
                                                .p_0()
                                                .text_color(rgb(0x5f5a51))
                                                .disabled(!can_toggle_original)
                                                .when(!can_toggle_original, |b| b.opacity(0.4))
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap(px(4.))
                                                        .text_size(px(13.))
                                                        .child(div().text_size(px(12.)).child("En"))
                                                        .when(!compact, |d| d.child("/ 译文")),
                                                )
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.selection_action(
                                                        "original",
                                                        HighlightColor::Yellow,
                                                        window,
                                                        cx,
                                                    )
                                                })),
                                        ),
                                )
                            })
                            .when(self.settings.committed.features.edit, |d| {
                                d.child(
                                    div()
                                        .debug_selector(|| "selection-edit-translation".into())
                                        .child(action_button(
                                            "edit",
                                            "改译文",
                                            "icons/pencil.svg",
                                            76.,
                                        )),
                                )
                            })
                            .when(self.settings.committed.features.page, |d| {
                                d.child(
                                    div()
                                        .debug_selector(|| "selection-source-page".into())
                                        .child(
                                            Button::new("selection-page")
                                                .ghost()
                                                .accessibility_label(source_label.clone())
                                                .rounded(px(8.))
                                                .w(px(if compact { button_size } else { 90. }))
                                                .h(px(button_size))
                                                .p_0()
                                                .text_color(rgb(0x5f5a51))
                                                .disabled(!self.can_locate_source(selection.block))
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap(px(4.))
                                                        .text_size(px(13.))
                                                        .child(
                                                            Icon::default()
                                                                .path("icons/file.svg")
                                                                .size(px(icon_size)),
                                                        )
                                                        .when(!compact, |d| d.child(source_label)),
                                                )
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.selection_action(
                                                        "page",
                                                        HighlightColor::Yellow,
                                                        window,
                                                        cx,
                                                    )
                                                })),
                                        ),
                                )
                            })
                            .child(action_button("copy", "复制", "icons/copy.svg", button_size)),
                    ),
            )
            .with_priority(10)
            .into_any_element(),
        )
    }
}
