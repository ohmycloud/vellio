//! Annotation and tool-panel commands.
use super::*;

impl ReaderView {
    pub(super) fn show_panel(&mut self, panel: Panel, window: &mut Window, cx: &mut Context<Self>) {
        if panel == Panel::Edit {
            self.begin_inline_edit(self.selected, window, cx);
            return;
        }
        if (panel == Panel::Chat && !self.settings.committed.features.ask_ai)
            || (panel == Panel::Edit && !self.settings.committed.features.edit)
        {
            return;
        }
        if panel == Panel::Chat {
            if self.panel_open && self.panel == Panel::Chat {
                self.close_chat(window, cx);
            } else {
                self.open_chat(window, cx);
            }
            return;
        }
        self.chat_ui.closing = false;
        self.chat_ui.generation = self.chat_ui.generation.wrapping_add(1);
        self.appearance_open = false;
        self.navigation = None;
        self.queue_margin_note();
        self.margin_editor = None;
        self.panel_open = !self.panel_open || self.panel != panel;
        self.panel = panel;
        self.selection = None;
        self.annotation_popup = None;
        window.blur(cx);
        self.reset_rows();
        cx.notify();
    }

    pub(super) fn open_note(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.queue_margin_note();
        self.hovered_note_card = None;
        let Some(record) = self.all_notes().into_iter().find(|note| note["id"] == id) else {
            return;
        };
        let Some(block) = self
            .state
            .paper
            .blocks
            .iter()
            .position(|block| block.id == record["anchor"])
        else {
            return;
        };
        let keep_position =
            !self.panel_open && !self.show_pdf && self.list.logical_scroll_top().item_ix == block;
        let quote_offset = self.note_offset(&record, block, cx);
        self.select(block, window, cx);
        self.panel_open = false;
        self.show_pdf = false;
        if !keep_position {
            self.list.scroll_to(ListOffset {
                item_ix: block,
                offset_in_item: (quote_offset - px(80.)).max(px(0.)),
            });
        }
        if !annotation_panel::personal(&record) {
            self.annotation_popup = Some((id.to_owned(), point(px(400.), px(280.))));
        } else {
            let selection = Selection {
                block,
                key: record["key"]
                    .as_str()
                    .unwrap_or(&self.state.paper.blocks[block].id)
                    .to_owned(),
                quote: record["quote"].as_str().unwrap_or("").to_owned(),
                prefix: record["prefix"].as_str().unwrap_or("").to_owned(),
                suffix: record["suffix"].as_str().unwrap_or("").to_owned(),
                position: point(px(0.), px(0.)),
                underline: record["style"] == "underline",
            };
            self.margin_input.update(cx, |state, cx| {
                state.set_value(record["body"].as_str().unwrap_or("").to_owned(), window, cx)
            });
            let mut note = MarginNote::new(record.clone());
            note.queued(record.clone());
            note.acknowledge(record, true);
            self.margin_editor = Some(MarginEditor {
                selection,
                note,
                changed_at: Instant::now(),
                placement_pending: Some(Instant::now()),
            });
            self.place_margin_editor(window, cx);
        }
        self.reset_rows();
        cx.notify();
    }

    pub(super) fn add_paragraph_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(block) = self.state.paper.blocks.get(self.selected) else {
            return;
        };
        let selection = Selection {
            block: self.selected,
            key: block.id.clone(),
            quote: String::new(),
            prefix: String::new(),
            suffix: String::new(),
            position: point(px(0.), px(0.)),
            underline: false,
        };
        self.open_margin_note(selection, "note", HighlightColor::Yellow, window, cx);
    }

    pub(super) fn ask_annotation_ai(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || self.chat_ui.streaming {
            return;
        }
        self.queue_margin_note();
        let Some(note) = self.all_notes().into_iter().find(|note| note["id"] == id) else {
            return;
        };
        self.chat_ui.refs.clear();
        self.add_chat_reference(ai::ChatReference {
            anchor: note["anchor"].as_str().unwrap_or("").to_owned(),
            quote: note["quote"].as_str().unwrap_or("").to_owned(),
        });
        self.open_chat(window, cx);
        let question = note["kind"] == "question";
        if question && annotation_panel::answered(&note) {
            // Already answered: open a follow-up with the original context,
            // without re-sending the same question or erasing its reply.
            self.chat_ui.pending_note = None;
            self.question
                .update(cx, |input, cx| input.set_value("", window, cx));
            cx.notify();
            return;
        }
        let text = if question {
            note["body"].as_str().unwrap_or("").to_owned()
        } else {
            format!(
                "这是我读这里时写的笔记，请点评：我理解得对不对、有没有漏掉或想错的地方、还可以往哪想。\n\n我的笔记：{}",
                note["body"].as_str().unwrap_or("")
            )
        };
        self.chat_ui.pending_note = Some(id.to_owned());
        self.question
            .update(cx, |input, cx| input.set_value(text, window, cx));
        // Note-card actions send immediately, as in EasyRead's askModel.
        // Missing model credentials leave the draft and reference available to retry.
        self.ask(window, cx);
        cx.notify();
    }

    pub(super) fn toggle_note_kind(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(mut note) = self.all_notes().into_iter().find(|note| note["id"] == id) {
            note["kind"] = json!(if note["kind"] == "question" {
                "note"
            } else {
                "question"
            });
            // Preserve the answer/status when switching back and forth.
            if note["kind"] == "question" && !note["answered"].is_boolean() {
                note["answered"] = json!(annotation_panel::answered(&note));
            }
            note["updated"] = json!(now());
            self.save_ops(vec![json!({"op":"note","note":note})], cx);
        }
    }

    pub(super) fn toggle_question_answered(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(editor) = &mut self.margin_editor
            && editor.note.record["id"] == id
            && editor.note.record["kind"] == "question"
        {
            editor.note.record["answered"] =
                json!(!annotation_panel::answered(&editor.note.record));
            editor.note.record["updated"] = json!(now());
            editor.changed_at = Instant::now();
            self.reset_rows();
            cx.notify();
            return;
        }
        if let Some(mut note) = self
            .all_notes()
            .into_iter()
            .find(|note| note["id"] == id && note["kind"] == "question")
        {
            note["answered"] = json!(!annotation_panel::answered(&note));
            note["updated"] = json!(now());
            self.save_ops(vec![json!({"op":"note","note":note})], cx);
        }
    }

    pub(super) fn delete_annotation(&mut self, id: &str, cx: &mut Context<Self>) {
        if self
            .margin_editor
            .as_ref()
            .is_some_and(|editor| editor.note.record["id"] == id)
        {
            self.queue_margin_note();
            self.margin_editor = None;
        }
        self.margin_records.remove(id);
        self.save_ops(vec![json!({"op":"note_del","id":id,"at":now()})], cx);
    }

    pub(super) fn set_hovered_note_card(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        if self.hovered_note_card != id {
            self.hovered_note_card = id;
            // Rebuild the cached reader rows in both layouts, without changing
            // text measurements, EPUB pagination or the current scroll offset.
            let offset = self.list.logical_scroll_top();
            self.list.reset(self.state.paper.blocks.len());
            self.list.scroll_to(offset);
            cx.notify();
        }
    }

    pub(super) fn export_annotations(&mut self, cx: &mut Context<Self>) {
        let mut text = format!("# {}\n\n", self.state.title());
        for note in self.all_notes() {
            text.push_str(&format!(
                "## {}\n\n> {}\n\n{}\n\n",
                annotation_panel::label(&note),
                note["quote"].as_str().unwrap_or(""),
                note["body"].as_str().unwrap_or("")
            ));
        }
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.status = "已将全部批注导出为 Markdown 并复制到剪贴板".into();
        cx.notify();
    }
}
