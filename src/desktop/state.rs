//! Reader state, session persistence and editing commands.
use super::*;

impl ReaderView {
    /// A note/color change only changes decorations. Keep measured EPUB pages
    /// and the visible reader list until the underlying text-flow source changes.
    pub(super) fn apply_state(&mut self, mut raw: Value) -> bool {
        if raw.get("chat").is_some()
            && let Ok(chat) = ai::normalize_chat_state(raw["chat"].clone())
        {
            raw["chat"] = chat;
        }
        let text_flow_changed = self.raw["paper"] != raw["paper"]
            || self.raw["reader"]["edits"] != raw["reader"]["edits"];
        if !text_flow_changed {
            if self.raw["layout"] != raw["layout"] {
                self.layout = PageLayout::from_state(&raw, &self.state);
            }
            // Notes and color live in raw JSON, not in ReaderState. Avoid
            // deserializing the entire book and replacing its layout for them.
            self.raw = raw;
            return false;
        }
        match ReaderState::from_value(raw.clone()) {
            Ok(state) => {
                self.clear_paragraph_layouts();
                self.epub_code_language = code::preferred_epub_language(&state.paper.blocks);
                self.layout = PageLayout::from_state(&raw, &state);
                self.raw = raw;
                self.state = state;
                self.epub_reflow.invalidate();
                self.pdf_code_needs_plan = true;
                true
            }
            Err(e) => {
                self.status = format!("数据错误：{e}");
                false
            }
        }
    }

    pub(super) fn refresh_state(&mut self, raw: Value) {
        let offset = self.list.logical_scroll_top();
        if self.apply_state(raw) {
            self.list.reset(self.state.paper.blocks.len());
            self.list.scroll_to(offset);
        }
        self.live.clear();
        self.live_question.clear();
    }

    pub(super) fn selected_key(&self) -> Option<String> {
        let b = self.state.paper.blocks.get(self.selected)?;
        Some(if matches!(b.kind.as_str(), "table" | "figure") {
            format!("{}#caption", b.id)
        } else {
            b.id.clone()
        })
    }

    pub(super) fn selected_text(&self) -> String {
        self.text_at(self.selected)
    }

    pub(super) fn text_at(&self, index: usize) -> String {
        let Some(b) = self.state.paper.blocks.get(index) else {
            return String::new();
        };
        let key = if matches!(b.kind.as_str(), "table" | "figure") {
            format!("{}#caption", b.id)
        } else {
            b.id.clone()
        };
        if matches!(b.kind.as_str(), "table" | "figure") {
            self.state.caption(b).into()
        } else {
            self.state.text(&key, &b.zh, &b.en).into()
        }
    }

    pub(super) fn restored_panel(session: Option<&Session>) -> Panel {
        match session.map(|s| s.panel.as_str()) {
            Some("edit") => Panel::Edit,
            Some("chat") => Panel::Chat,
            _ => Panel::Notes,
        }
    }

    pub(super) fn load_session(workspace: Option<&Workspace>) -> (Option<Session>, Option<String>) {
        let Some(ws) = workspace else {
            return (None, None);
        };
        if !ws.root.join(".native-reader.json").is_file() {
            return (None, None);
        }
        match Session::load(&ws.root) {
            Ok(session) => (Some(session), None),
            Err(e) => (None, Some(format!("恢复文件读取失败，已保留原文件：{e:#}"))),
        }
    }

    pub(super) fn restored_position(&self) -> (usize, f32) {
        let ids: Vec<_> = self
            .state
            .paper
            .blocks
            .iter()
            .map(|b| b.id.as_str())
            .collect();
        self.session
            .as_ref()
            .and_then(|s| s.position(&self.raw["reader"]["progress"], &ids))
            .unwrap_or((self.selected.min(ids.len().saturating_sub(1)), 0.0))
    }

    pub(super) fn current_note(&self, cx: &Context<Self>) -> NoteDraft {
        let quote = self.quote.read(cx).value().to_string();
        let context = self
            .note_selection
            .as_ref()
            .filter(|s| s.quote == quote && s.block == self.selected);
        NoteDraft {
            body: self.note.read(cx).value().to_string(),
            key: context
                .map(|s| s.key.clone())
                .or_else(|| self.selected_key())
                .unwrap_or_default(),
            quote,
            kind: self.note_kind.into(),
            prefix: context.map(|s| s.prefix.clone()).unwrap_or_default(),
            suffix: context.map(|s| s.suffix.clone()).unwrap_or_default(),
        }
    }

    pub(super) fn capture_inputs(&mut self, cx: &Context<Self>) {
        if self.restore_inputs {
            return;
        }
        let Some(key) = self.selected_key() else {
            return;
        };
        let text = self.editor.read(cx).value().to_string();
        let existing = self
            .session
            .as_ref()
            .and_then(|s| s.drafts.get(&key))
            .and_then(|d| d.edit.as_ref());
        let edit = (text != self.editor_base).then(|| EditDraft {
            text,
            base: existing
                .map(|d| d.base.clone())
                .unwrap_or_else(|| text_hash(&self.editor_base)),
        });
        let note = self.current_note(cx);
        let note = (!note.body.is_empty() || !note.quote.is_empty()).then_some(note);
        if let Some(session) = &mut self.session {
            session.set_draft(key, Draft { edit, note });
        }
    }

    pub(super) fn capture_position(&mut self) {
        let offset = self.list.logical_scroll_top();
        let Some(block) = self.state.paper.blocks.get(offset.item_ix) else {
            return;
        };
        let selected_key = self.selected_key();
        if let Some(session) = &mut self.session {
            session.selected_key = selected_key;
            session.panel = match self.panel {
                Panel::Edit => "edit",
                Panel::Notes => "notes",
                Panel::Chat => "chat",
            }
            .into();
            if !session.progress.as_ref().is_some_and(|p| {
                p.block == block.id && p.offset == f32::from(offset.offset_in_item)
            }) {
                session.progress = Some(Position {
                    block: block.id.clone(),
                    offset: f32::from(offset.offset_in_item).max(0.0),
                    at: now(),
                });
            }
            session.bilingual = self.reading_mode == ReadingMode::Bilingual;
            session.reading_mode = Some(self.reading_mode);
            session.original_first = self.original_first;
            session.show_pdf = self.show_pdf;
            session.appearance = self.appearance.clone();
        }
    }

    pub(super) fn flush_session(&mut self) -> bool {
        if self.settings.relocating {
            return false;
        }
        if self.session == self.persisted_session {
            return false;
        }
        if let (Some(ws), Some(session)) = (&self.workspace, &self.session) {
            match session.save(&ws.root) {
                Ok(()) => self.persisted_session = self.session.clone(),
                Err(e) => {
                    self.status = format!("草稿/位置保存失败：{e:#}");
                    return true;
                }
            }
        }
        false
    }

    pub(super) fn fill_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.restore_inputs = false;
        self.editor_base = self.selected_text();
        let draft = self
            .selected_key()
            .and_then(|key| self.session.as_ref()?.drafts.get(&key))
            .cloned()
            .unwrap_or_default();
        self.draft_conflict = draft
            .edit
            .as_ref()
            .is_some_and(|d| d.base != text_hash(&self.editor_base));
        let text = draft
            .edit
            .map(|d| d.text)
            .unwrap_or_else(|| self.editor_base.clone());
        let note = draft.note.unwrap_or_default();
        self.note_kind = if note.kind == "question" {
            "question"
        } else {
            "note"
        };
        self.note_selection = if note.quote.is_empty() {
            None
        } else {
            self.selected_key().map(|key| Selection {
                key: if note.key.is_empty() {
                    key
                } else {
                    note.key.clone()
                },
                block: self.selected,
                quote: note.quote.clone(),
                prefix: note.prefix.clone(),
                suffix: note.suffix.clone(),
                position: point(px(0.), px(0.)),
                underline: false,
            })
        };
        self.editor
            .update(cx, |s, cx| s.set_value(text, window, cx));
        self.quote
            .update(cx, |s, cx| s.set_value(note.quote, window, cx));
        self.note
            .update(cx, |s, cx| s.set_value(note.body, window, cx));
    }

    pub(super) fn fill_paper_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let body = self.raw["reader"]["paper_note"]["body"]
            .as_str()
            .unwrap_or("")
            .to_owned();
        self.paper_note_input
            .update(cx, |state, cx| state.set_value(body, window, cx));
    }

    pub(super) fn select(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_annotation_scroll();
        self.queue_margin_note();
        self.margin_editor = None;
        self.capture_inputs(cx);
        self.flush_session();
        self.selection = None;
        self.annotation_popup = None;
        self.note_selection = None;
        self.note_kind = "note";
        self.selected = index;
        self.follow_block(index);
        self.fill_editor(window, cx);
        self.panel = Panel::Notes;
        cx.notify();
    }

    pub(super) fn save_ops(&mut self, ops: Vec<Value>, cx: &mut Context<Self>) {
        self.save_ops_with_draft(ops, None, cx);
    }

    pub(super) fn save_ops_with_draft(
        &mut self,
        ops: Vec<Value>,
        saved: Option<(String, Option<EditDraft>, Option<NoteDraft>, bool)>,
        cx: &mut Context<Self>,
    ) {
        let Some(ws) = self.workspace.clone() else {
            return;
        };
        self.status = "正在保存……".into();
        self.worker(cx, move |_, tx| {
            match ws.reader_ops(&ops).and_then(|_| ws.state()) {
                Ok(raw) => {
                    let committed = saved.is_none()
                        || easyread_gpui::session::draft_ops_committed(&raw["reader"], &ops);
                    let _ = tx.send(Event::Refresh(raw));
                    if !committed {
                        let _ = tx.send(Event::Error(
                            "正文已有更新，保存未生效；草稿已保留，请核对后重试".into(),
                        ));
                        return;
                    }
                    if let Some((key, edit, note, revert)) = saved {
                        let _ = tx.send(Event::SavedDraft(key, edit, note, revert));
                    }
                    let _ = tx.send(Event::Status("已保存".into()));
                }
                Err(e) => {
                    let _ = tx.send(Event::Error(format!("保存失败：{e:#}")));
                }
            }
        });
    }

    pub(super) fn save_edit(&mut self, revert: bool, cx: &mut Context<Self>) {
        let Some(b) = self.state.paper.blocks.get(self.selected) else {
            return;
        };
        if !matches!(
            b.kind.as_str(),
            "para" | "heading" | "note" | "figure" | "table"
        ) {
            self.status = "当前可修订段落、标题或图表题注".into();
            return;
        }
        let key = self.selected_key().unwrap();
        let (zh, en) = if matches!(b.kind.as_str(), "figure" | "table") {
            (&b.caption_zh, &b.caption_en)
        } else {
            (&b.zh, &b.en)
        };
        let base = text_hash(if zh.is_empty() { en } else { zh });
        let text = self.editor.read(cx).value().to_string();
        self.capture_inputs(cx);
        let draft = EditDraft {
            text: text.clone(),
            base: self
                .session
                .as_ref()
                .and_then(|s| s.drafts.get(&key))
                .and_then(|d| d.edit.as_ref())
                .map(|d| d.base.clone())
                .unwrap_or_else(|| text_hash(&self.editor_base)),
        };
        self.save_ops_with_draft(vec![json!({"op":"edit","block":key,"zh":if revert{Value::Null}else{json!(text)},"base":base,"at":now()})], Some((key, Some(draft), None, revert)), cx);
    }
}
