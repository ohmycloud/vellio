//! Margin-note persistence, placement and editor rendering.
use super::*;

impl ReaderView {
    pub(super) fn queue_margin_note(&mut self) {
        if self.settings.relocating {
            return;
        }
        if let (Some(editor), Some(workspace)) = (&mut self.margin_editor, &self.workspace)
            && let Some(snapshot) = editor.note.pending()
            && self.margin_writer.save(workspace.clone(), snapshot.clone())
        {
            self.margin_records.insert(
                snapshot["id"].as_str().unwrap().to_owned(),
                snapshot.clone(),
            );
            editor.note.queued(snapshot);
        }
    }

    pub(super) fn receive_margin_notes(&mut self, cx: &mut Context<Self>) {
        while let Ok(saved) = self.margin_writer.results.try_recv() {
            if self
                .workspace
                .as_ref()
                .is_none_or(|ws| ws.root != saved.root)
            {
                continue;
            }
            let id = saved.snapshot["id"].as_str().unwrap_or("");
            if saved.result.is_ok() && self.margin_records.get(id) == Some(&saved.snapshot) {
                self.margin_records.remove(id);
            }
            if let Some(editor) = &mut self.margin_editor
                && editor.note.record["id"] == saved.snapshot["id"]
            {
                editor
                    .note
                    .acknowledge(saved.snapshot, saved.result.is_ok());
            }
            match saved.result {
                Ok(reader) => {
                    let mut raw = self.raw.clone();
                    raw["reader"] = reader;
                    for (id, record) in &self.margin_records {
                        if raw["reader"]["notes"][id]["updated"].as_str().unwrap_or("")
                            < record["updated"].as_str().unwrap_or("")
                        {
                            raw["reader"]["notes"][id] = record.clone();
                        }
                    }
                    self.apply_state(raw);
                    self.status = "笔记已自动保存".into();
                }
                Err(error) => self.status = error,
            }
            cx.notify();
        }
    }

    pub(super) fn open_margin_note(
        &mut self,
        selection: Selection,
        kind: &'static str,
        color: HighlightColor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.queue_margin_note();
        let anchor = &self.state.paper.blocks[selection.block].id;
        let existing = self.raw["reader"]["notes"].as_object().and_then(|notes| {
            notes
                .values()
                .chain(self.margin_records.values())
                .filter(|note| {
                    note["anchor"] == *anchor
                        && note["key"] == selection.key
                        && note["quote"] == selection.quote
                        && note["prefix"] == selection.prefix
                        && !note["deleted"].as_bool().unwrap_or(false)
                        && matches!(note["kind"].as_str(), Some("note" | "question"))
                })
                .max_by_key(|note| note["updated"].as_str().unwrap_or(""))
        });
        let stamp = now();
        let mut record = existing.cloned().unwrap_or_else(|| {
            json!({
                "id":format!("n{}",Uuid::new_v4().simple()), "anchor":anchor, "key":selection.key,
                "quote":selection.quote, "prefix":selection.prefix, "suffix":selection.suffix,
                "kind":kind, "color":color.name(), "body":"", "style":"underline",
                "created":stamp, "updated":stamp
            })
        });
        record["kind"] = json!(kind);
        record["updated"] = json!(stamp);
        // Existing marks remain independent of the attached note.
        if let Some((id, _)) = &self.annotation_popup {
            record["source_annotation"] = json!(id);
            record["decoration"] = json!("none");
            record["color"] = json!(color.name());
            record.as_object_mut().unwrap().remove("style");
        }
        self.annotation_popup = None;
        self.panel = Panel::Notes;
        self.panel_open = false;
        self.show_pdf = false;
        self.margin_input.update(cx, |state, cx| {
            state.set_value(record["body"].as_str().unwrap_or("").to_owned(), window, cx);
        });
        let mut note = MarginNote::new(record);
        if let Some(existing) = existing {
            note.queued(existing.clone());
            note.acknowledge(existing.clone(), true);
        }
        self.margin_editor = Some(MarginEditor {
            selection,
            note,
            changed_at: Instant::now(),
            placement_pending: Some(Instant::now()),
        });
        self.reset_rows();
        self.place_margin_editor(window, cx);
        cx.notify();
    }

    pub(super) fn place_margin_editor(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(token) = self
            .margin_editor
            .as_ref()
            .and_then(|editor| editor.placement_pending)
        else {
            return;
        };
        let weak = cx.entity().downgrade();
        // The first render adds the gutter and quote anchor. Inline annotation
        // offsets are published during paint, not while block() builds the row.
        // Rebuild the cached row only after that frame has finished, so its
        // first visible placement uses the actual, newly reflowed quote line.
        window.on_next_frame(move |window, _| {
            window.on_next_frame(move |window, cx| {
                let _ = weak.update(cx, |this, cx| {
                    let Some(editor) = this
                        .margin_editor
                        .as_mut()
                        .filter(|editor| editor.placement_pending == Some(token))
                    else {
                        return;
                    };
                    let index = editor.selection.block;
                    if this.pdf_code_pending(index)
                        || [false, true].into_iter().any(|original| {
                            this.text_views
                                .get(&(index, original))
                                .is_some_and(|(_, view)| !view.read(cx).is_content_ready())
                        })
                    {
                        this.place_margin_editor(window, cx);
                        return;
                    }
                    this.margin_editor.as_mut().unwrap().placement_pending = None;
                    this.reset_rows();
                    this.margin_input
                        .read(cx)
                        .focus_handle(cx)
                        .focus(window, cx);
                    cx.notify();
                });
            });
        });
    }

    pub(super) fn close_margin_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.queue_margin_note();
        self.margin_editor = None;
        self.hovered_note_card = None;
        self.reset_rows();
        window.blur(cx);
        cx.notify();
    }

    pub(super) fn change_margin_note(
        &mut self,
        kind: Option<&'static str>,
        color: Option<HighlightColor>,
        cx: &mut Context<Self>,
    ) {
        if let Some(editor) = &mut self.margin_editor {
            if let Some(kind) = kind {
                editor.note.record["kind"] = json!(kind);
            }
            if let Some(color) = color {
                editor.note.record["color"] = json!(color.name());
            }
            editor.note.record["updated"] = json!(now());
            editor.changed_at = Instant::now();
        }
        // Margin editors live inside cached list rows; refresh their swatch
        // and quote border immediately, keeping the current reading offset.
        self.reset_rows();
        cx.notify();
    }

    pub(super) fn margin_note_view(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let editor = self.margin_editor.as_ref()?;
        let color =
            HighlightColor::from_name(editor.note.record["color"].as_str().unwrap_or("yellow"));
        let question = editor.note.record["kind"] == "question";
        Some(
            div()
                .w(px(300.))
                .flex_shrink_0()
                .px(px(12.))
                .child(
                    div()
                        .id("margin-note")
                        .debug_selector(|| "margin-note".into())
                        .when(editor.placement_pending.is_some(), |d| d.invisible())
                        .border_l_2()
                        .border_color(rgb(0xaa681c))
                        .pl(px(12.))
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(self.annotation_header(&editor.note.record))
                        .child(
                            div()
                                .border_l_2()
                                .border_color(rgb(color.rgb()))
                                .pl(px(4.))
                                .text_size(px(13.))
                                .text_color(rgb(0x969084))
                                .max_h(px(78.))
                                .overflow_hidden()
                                .child(format!("“{}”", editor.selection.quote)),
                        )
                        .child(
                            div()
                                .debug_selector(|| "margin-note-input".into())
                                .border_1()
                                .border_color(rgb(0xaa681c))
                                .rounded(px(8.))
                                .bg(rgb(0xfbfaf7))
                                .p(px(6.))
                                .child(
                                    Textarea::new(&self.margin_input)
                                        .appearance(false)
                                        .bordered(false)
                                        .h(px(110.))
                                        .text_size(px(14.))
                                        .aria_label("My note text"),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(3.))
                                .children([("note", "Notes"), ("question", "Question")].map(
                                    |(kind, label)| {
                                        Button::new(SharedString::from(format!("margin-{kind}")))
                                            .ghost()
                                            .h(px(24.))
                                            .px(px(6.))
                                            .rounded(px(12.))
                                            .text_size(px(11.))
                                            .text_color(rgb(if (kind == "question") == question {
                                                0xaa681c
                                            } else {
                                                0x969084
                                            }))
                                            .when((kind == "question") == question, |b| {
                                                b.bg(rgb(0xeee6da))
                                            })
                                            .label(label)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.change_margin_note(Some(kind), None, cx);
                                            }))
                                    },
                                ))
                                .children(HighlightColor::ALL.map(|swatch| {
                                    Button::new(("margin-color", swatch as usize))
                                        .ghost()
                                        .p_0()
                                        .size(px(20.))
                                        .rounded(px(10.))
                                        .accessibility_label(swatch.label())
                                        .tooltip(swatch.label())
                                        .child(
                                            div()
                                                .size(px(20.))
                                                .rounded(px(10.))
                                                .border_1()
                                                .border_color(
                                                    rgb(if swatch == color {
                                                        0x928b7d
                                                    } else {
                                                        0x000000
                                                    })
                                                    .opacity(if swatch == color { 1. } else { 0. }),
                                                )
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .child(
                                                    div()
                                                        .size(px(14.))
                                                        .rounded(px(7.))
                                                        .border_1()
                                                        .border_color(rgba(0x928b7d55))
                                                        .bg(rgb(swatch.rgb())),
                                                ),
                                        )
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.change_margin_note(None, Some(swatch), cx);
                                        }))
                                })),
                        )
                        .when(question, |d| {
                            let id = editor.note.record["id"].as_str().unwrap_or("").to_owned();
                            d.child(
                                Button::new("margin-question-status")
                                    .debug_selector(|| "margin-question-status".into())
                                    .ghost()
                                    .xsmall()
                                    .label(if annotation_panel::answered(&editor.note.record) {
                                        "Mark as unanswered"
                                    } else {
                                        "Mark as answered"
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.toggle_question_answered(&id, cx);
                                    })),
                            )
                        })
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(0x969084))
                                .text_right()
                                .child(if editor.note.is_saved() {
                                    "Autosaved · Esc to close"
                                } else if editor.note.record["body"]
                                    .as_str()
                                    .unwrap_or("")
                                    .is_empty()
                                {
                                    "Autosave · Esc to close"
                                } else {
                                    "Autosaving… · Esc to close"
                                }),
                        ),
                )
                .into_any_element(),
        )
    }
}
