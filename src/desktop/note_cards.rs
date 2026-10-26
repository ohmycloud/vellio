//! Annotation and discussion card presentation.
use super::*;

impl ReaderView {
    pub(super) fn annotation_replies(
        &self,
        id: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let replies: Vec<_> = self.raw["discussion"]["entries"]
            .as_array()?
            .iter()
            .filter(|entry| entry["reply_to"] == id && entry["deleted"] != true)
            .cloned()
            .collect();
        if replies.is_empty() {
            return None;
        }
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .w_full()
                .children(
                    replies
                        .iter()
                        .map(|entry| self.discussion_card(entry, false, cx)),
                )
                .into_any_element(),
        )
    }

    pub(super) fn discussion_card(
        &self,
        entry: &Value,
        drawer: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let body = entry["body"].as_str().unwrap_or("").to_owned();
        let copy = body.clone();
        let id = entry["id"].as_str().unwrap_or("ai-entry").to_owned();
        let weak = cx.entity().downgrade();
        let measure_id = id.clone();
        let scroll = self
            .discussion_scrolls
            .borrow_mut()
            .entry(id.clone())
            .or_default()
            .clone();
        let wheel_scroll = scroll.clone();
        let height = (!drawer).then(|| {
            self.state
                .paper
                .blocks
                .iter()
                .position(|b| entry["anchor"] == b.id)
                .and_then(|i| self.pdf_margin_height(i, cx))
                // Leave room for note metadata/actions, with at least two
                // readable lines rather than a page-sized answer preview.
                .map(|height| (height - px(240.)).max(px(48.)).min(px(420.)))
                .unwrap_or(px(420.))
        });
        div()
            .debug_selector(|| "annotation-ai-reply".into())
            .relative()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .border_l_2()
            .border_color(rgb(0x387487))
            .pl(px(12.))
            .py(px(4.))
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            if this
                                .margin_card_heights
                                .get(&measure_id)
                                .is_none_or(|h| (*h - bounds.size.height).abs() > px(0.5))
                            {
                                this.margin_card_heights
                                    .insert(measure_id.clone(), bounds.size.height);
                                cx.notify();
                            }
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .when(drawer, |d| {
                d.child(self.annotation_jump_bar(entry, false, cx))
            })
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(0x387487))
                    .child(format!("AI · {}", ai::answer_model_label(entry))),
            )
            .child(
                div()
                    .id(SharedString::from(format!("reply-scroll-{id}")))
                    .debug_selector(|| "annotation-ai-body".into())
                    .w_full()
                    .min_w_0()
                    .when_some(height, |d, height| {
                        d.max_h(height)
                            .overflow_y_scroll()
                            .track_scroll(&scroll)
                            .on_scroll_wheel(move |_, _, cx| {
                                if wheel_scroll.max_offset().y > px(0.) {
                                    cx.stop_propagation();
                                }
                            })
                    })
                    .child(
                        div()
                            .debug_selector(|| "annotation-ai-content".into())
                            .w_full()
                            .min_w_0()
                            .child(
                                self.rich_markdown(format!("reply-body-{id}"), &body, 240., cx)
                                    .selectable(true)
                                    .w_full()
                                    .min_w_0()
                                    .text_size(px(13.5))
                                    .line_height(relative(1.72)),
                            ),
                    ),
            )
            .child(
                Button::new(SharedString::from(format!("reply-copy-{id}")))
                    .debug_selector(|| "annotation-ai-copy".into())
                    .ghost()
                    .xsmall()
                    .label(if height.is_some() {
                        "复制全文 · 回答内可滚动"
                    } else {
                        "复制全文"
                    })
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()))
                    }),
            )
            .into_any_element()
    }

    pub(super) fn annotation_header(&self, note: &Value) -> AnyElement {
        let answered = annotation_panel::answered(note);
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(12.))
                    .text_color(rgb(0xaa681c))
                    .child(annotation_panel::label(note))
                    .when(note["kind"] == "question", |d| {
                        d.child(
                            div()
                                .debug_selector(move || {
                                    if answered {
                                        "annotation-answered"
                                    } else {
                                        "annotation-unanswered"
                                    }
                                    .into()
                                })
                                .rounded(px(8.))
                                .px(px(5.))
                                .text_size(px(10.))
                                .text_color(rgb(if answered { 0x578164 } else { 0xaa681c }))
                                .bg(rgba(if answered { 0x57816415 } else { 0xaa681c15 }))
                                .child(if answered { "Answered" } else { "Unanswered" }),
                        )
                    }),
            )
            .when_some(annotation_panel::timestamp(note), |d, timestamp| {
                d.child(
                    div()
                        .debug_selector(|| "annotation-time".into())
                        .flex_shrink_0()
                        .text_size(px(12.))
                        .text_color(rgb(0x969084))
                        .child(timestamp),
                )
            })
            .into_any_element()
    }

    pub(super) fn annotation_card(
        &self,
        note: &Value,
        drawer: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = note["id"].as_str().unwrap_or("").to_owned();
        let weak = cx.entity().downgrade();
        let measure_id = id.clone();
        let personal = annotation_panel::personal(note);
        let quote = note["quote"].as_str().unwrap_or("").to_owned();
        let body = note["body"].as_str().unwrap_or("").to_owned();
        let color = HighlightColor::from_name(note["color"].as_str().unwrap_or("yellow"));
        let edit_id = id.clone();
        let ai_id = id.clone();
        let ai_background: Hsla = rgba(0x38748712).into();
        let question_id = id.clone();
        let answer_id = id.clone();
        let question = note["kind"] == "question";
        let answered = annotation_panel::answered(note);
        let delete_id = id.clone();
        let hover_id = id.clone();
        let move_id = id.clone();
        let show_actions = self.hovered_note_card.as_ref() == Some(&id);
        div()
            .px(px(if drawer { 0. } else { 12. }))
            .min_w_0()
            .child(
                div()
                    .id(SharedString::from(format!("annotation-card-{id}")))
                    .debug_selector({
                        let id = id.clone();
                        move || format!("annotation-card-{id}")
                    })
                    // Reveal only on mouse movement, not when a saved card
                    // replaces the editor underneath a stationary pointer.
                    .on_mouse_move(cx.listener(move |this, _, _, cx| {
                        this.set_hovered_note_card(Some(move_id.clone()), cx);
                    }))
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if !*hovered && this.hovered_note_card.as_ref() == Some(&hover_id) {
                            this.set_hovered_note_card(None, cx);
                        }
                    }))
                    .relative()
                    .border_l_2()
                    .border_color(rgb(0xaa681c))
                    .pl(px(12.))
                    .py(px(4.))
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(8.))
                    // Let the window selection layer begin intentional drags
                    // in the saved body. Keep release out of reader-only tools.
                    .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .when(drawer, |d| {
                        d.child(self.annotation_jump_bar(note, false, cx))
                    })
                    .child(
                        canvas(
                            move |bounds, _, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    if this.margin_card_heights.get(&measure_id).is_none_or(|old| {
                                        (*old - bounds.size.height).abs() > px(0.5)
                                    }) {
                                        this.margin_card_heights
                                            .insert(measure_id.clone(), bounds.size.height);
                                        cx.notify();
                                    }
                                });
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
                    .child(self.annotation_header(note))
                    .when(!quote.is_empty(), |d| {
                        d.child(
                            div()
                                .debug_selector(|| "annotation-card-quote".into())
                                .relative()
                                .w_full()
                                .text_size(px(13.))
                                .font_family(self.fonts.serif_en.clone())
                                .line_height(relative(1.6))
                                .text_color(rgb(0x969084))
                                .border_l_2()
                                .border_color(rgb(color.rgb()))
                                .pl(px(4.))
                                .when(drawer, |d| {
                                    d.child(self.annotation_jump_bar(note, true, cx))
                                })
                                .child(
                                    self.rich_markdown(
                                        format!("annotation-quote-{id}"),
                                        &quote,
                                        240.,
                                        cx,
                                    )
                                    .selectable(true)
                                    .w_full()
                                    .text_size(px(13.))
                                    .font_family(self.fonts.serif_en.clone())
                                    .line_height(relative(1.6))
                                    .text_color(rgb(0x969084)),
                                ),
                        )
                    })
                    .when(!body.is_empty(), |d| {
                        d.child(
                            div()
                                .debug_selector(|| "annotation-card-body".into())
                                .w_full()
                                .child(
                                    self.rich_markdown(
                                        format!("annotation-body-{id}"),
                                        &body,
                                        240.,
                                        cx,
                                    )
                                    .selectable(true)
                                    .w_full()
                                    .text_size(px(14.))
                                    .text_color(rgb(0x625e55)),
                                ),
                        )
                    })
                    .when_some(self.annotation_replies(&id, cx), |d, replies| {
                        d.child(replies)
                    })
                    .when(personal && !body.trim().is_empty(), |d| {
                        d.when(self.settings.committed.features.ask_ai, |d| {
                            d.child(
                                Button::new(SharedString::from(format!("annotation-ai-{id}")))
                                    .debug_selector(move || {
                                        if question {
                                            "annotation-ai-answer"
                                        } else {
                                            "annotation-ai-feedback"
                                        }
                                        .into()
                                    })
                                    .custom(
                                        ButtonCustomVariant::new(cx)
                                            .color(ai_background)
                                            .foreground(rgb(0x387487).into())
                                            .hover(ai_background)
                                            .active(ai_background),
                                    )
                                    // Custom variants soften their idle color; keep
                                    // the same surface in idle, hover and pressed states.
                                    .bg(ai_background)
                                    .xsmall()
                                    .h(px(26.))
                                    .w(px(184.))
                                    .max_w_full()
                                    .px(px(10.))
                                    .rounded(px(13.))
                                    .text_size(px(12.))
                                    .label(if question {
                                        "✳  Ask AI to answer"
                                    } else {
                                        "✳  Ask AI for feedback"
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.ask_annotation_ai(&ai_id, window, cx)
                                    })),
                            )
                        })
                    })
                    .child(
                        // Reserve the toolbar height so hover cannot move the
                        // card (or the following notes) under the pointer.
                        div().when(!show_actions, |d| d.invisible()).child(
                            div()
                                .debug_selector(|| "annotation-card-actions".into())
                                .flex()
                                .flex_wrap()
                                .gap(px(6.))
                                .child(
                                    Button::new(SharedString::from(format!(
                                        "annotation-edit-{id}"
                                    )))
                                    .ghost()
                                    .xsmall()
                                    .h(px(22.))
                                    .px(px(2.))
                                    .text_size(px(11.))
                                    .text_color(rgb(0x969084))
                                    .label("Edit")
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_note(&edit_id, window, cx)
                                    })),
                                )
                                .child(
                                    Button::new(SharedString::from(format!(
                                        "annotation-question-{id}"
                                    )))
                                    .ghost()
                                    .xsmall()
                                    .h(px(22.))
                                    .px(px(2.))
                                    .text_size(px(11.))
                                    .text_color(rgb(0x969084))
                                    .debug_selector(move || {
                                        if question {
                                            "annotation-make-note"
                                        } else {
                                            "annotation-make-question"
                                        }
                                        .into()
                                    })
                                    .label(if question {
                                        "Make it a note"
                                    } else {
                                        "Make it a question"
                                    })
                                    .disabled(self.busy)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.toggle_note_kind(&question_id, cx)
                                    })),
                                )
                                .when(question, |d| {
                                    d.child(
                                        Button::new(SharedString::from(format!(
                                            "annotation-answer-{id}"
                                        )))
                                        .ghost()
                                        .xsmall()
                                        .h(px(22.))
                                        .px(px(2.))
                                        .text_size(px(11.))
                                        .text_color(rgb(0x969084))
                                        .debug_selector(move || {
                                            if answered {
                                                "annotation-mark-unanswered"
                                            } else {
                                                "annotation-mark-answered"
                                            }
                                            .into()
                                        })
                                        .label(if answered {
                                            "Mark as unanswered"
                                        } else {
                                            "Mark as answered"
                                        })
                                        .disabled(self.busy)
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                this.toggle_question_answered(&answer_id, cx)
                                            }),
                                        ),
                                    )
                                })
                                .child(
                                    Button::new(SharedString::from(format!(
                                        "annotation-delete-{id}"
                                    )))
                                    .ghost()
                                    .xsmall()
                                    .h(px(22.))
                                    .px(px(2.))
                                    .text_size(px(11.))
                                    .text_color(rgb(0x969084))
                                    .label("Delete")
                                    .disabled(self.busy)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.delete_annotation(&delete_id, cx)
                                    })),
                                ),
                        ),
                    ),
            )
            .into_any_element()
    }
}
