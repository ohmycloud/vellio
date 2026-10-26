//! Chat actions.
use super::*;

impl ReaderView {
    pub(in crate::desktop) fn tool_panel_visible(&self) -> bool {
        self.panel_open || self.chat_ui.closing
    }

    pub(in crate::desktop) fn chat_panel_width(&self, viewport: f32) -> f32 {
        if viewport < 1180. {
            return viewport.min(600.);
        }
        self.settings
            .committed
            .extra
            .get("side_panel_width")
            .and_then(Value::as_f64)
            .filter(|w| w.is_finite())
            .map(|w| (w as f32).clamp(320., viewport - 420.))
            .unwrap_or((viewport * 0.44).min(720.))
    }

    pub(in crate::desktop) fn open_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.settings.committed.features.ask_ai {
            return;
        }
        self.queue_margin_note();
        self.margin_editor = None;
        self.appearance_open = false;
        self.navigation = None;
        self.selection = None;
        self.annotation_popup = None;
        self.hovered_note_card = None;
        let opening = !self.panel_open || self.panel != Panel::Chat || self.chat_ui.closing;
        self.panel = Panel::Chat;
        self.panel_open = true;
        self.show_pdf = false;
        self.chat_ui.closing = false;
        if opening {
            self.chat_ui.generation = self.chat_ui.generation.wrapping_add(1);
            self.chat_ui.slide_started = Some(Instant::now());
        }
        self.reset_rows();
        // EasyRead focuses the composer after the 220ms slide, not during reflow.
        let generation = self.chat_ui.generation;
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(300))
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.panel_open
                    && this.panel == Panel::Chat
                    && this.chat_ui.generation == generation
                    && !this.chat_ui.model_open
                    && !this.chat_ui.style_open
                    && !this.chat_ui.list_open
                {
                    this.question.read(cx).focus_handle(cx).focus(window, cx);
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::desktop) fn close_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.panel_open = false;
        self.chat_ui.closing = true;
        self.chat_ui.slide_started = Some(Instant::now());
        self.chat_ui.generation = self.chat_ui.generation.wrapping_add(1);
        self.chat_ui.list_open = false;
        self.chat_ui.model_open = false;
        self.chat_ui.style_open = false;
        self.reader_focus.focus(window, cx);
        let generation = self.chat_ui.generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(SLIDE_MS))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.chat_ui.generation == generation && this.chat_ui.closing {
                    this.chat_ui.closing = false;
                    this.reset_rows();
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::desktop) fn add_chat_reference(&mut self, reference: ChatReference) {
        if !self
            .state
            .paper
            .blocks
            .iter()
            .any(|b| b.id == reference.anchor)
        {
            return;
        }
        if self.chat_ui.refs.contains(&reference) {
            return;
        }
        if !reference.quote.is_empty() {
            self.chat_ui
                .refs
                .retain(|r| r.anchor != reference.anchor || !r.quote.is_empty());
        }
        self.chat_ui.refs.push(reference);
        self.chat_ui.pending_note = None;
    }

    pub(in crate::desktop) fn chat_references(&self) -> Vec<ChatReference> {
        if !self.chat_ui.refs.is_empty() {
            return self.chat_ui.refs.clone();
        }
        if self.chat_ui.no_auto {
            return vec![];
        }
        self.state
            .paper
            .blocks
            .get(self.list.logical_scroll_top().item_ix)
            .map(|block| {
                vec![ChatReference {
                    anchor: block.id.clone(),
                    quote: String::new(),
                }]
            })
            .unwrap_or_default()
    }

    pub(in crate::desktop) fn chat_reference_label(&self, reference: &ChatReference) -> String {
        let text = if reference.quote.is_empty() {
            self.state
                .paper
                .blocks
                .iter()
                .find(|b| b.id == reference.anchor)
                .map(|b| {
                    annotations::plain_text(&annotations::html(
                        &self.state.markdown(b, self.primary_original()),
                    ))
                })
                .unwrap_or_default()
        } else {
            reference.quote.clone()
        };
        let text = text.replace('\n', " ");
        format!(
            "「{}{}」",
            text.chars().take(18).collect::<String>(),
            if text.chars().count() > 18 { "…" } else { "" }
        )
    }

    pub(in crate::desktop) fn ask(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.settings.committed.features.ask_ai || self.busy || self.chat_ui.streaming {
            return;
        }
        let text = self.question.read(cx).value().to_string();
        if text.trim().is_empty() {
            return;
        }
        let Some(ws) = self.workspace.clone() else {
            return;
        };
        let config = if let Some(profile) = self.chat_model_profile() {
            let mut profile = profile.clone();
            if let Some(effort) = &self.chat_ui.reasoning {
                profile.reasoning = effort.clone();
            }
            ai::Connection::from_profile(profile)
        } else {
            self.settings.committed.connection(false)
        };
        let config = match config {
            Ok(config) => config,
            Err(error) => {
                self.chat_ui.error = Some(error.to_string());
                self.status = error.to_string();
                cx.notify();
                return;
            }
        };
        let request = ChatRequest {
            text: text.trim().to_owned(),
            refs: self.chat_references(),
            note: self.chat_ui.pending_note.take(),
            answer_style: self.chat_ui.style,
            reasoning_effort: self.chat_ui.reasoning.clone(),
            model_id: if self.chat_ui.model.is_empty() {
                self.settings.committed.ask_model.clone()
            } else {
                self.chat_ui.model.clone()
            },
        };
        self.chat_ui.live_refs = request.refs.clone();
        self.chat_ui.refs.clear();
        self.chat_ui.no_auto = false;
        self.chat_ui.error = None;
        self.chat_ui.streaming = true;
        self.chat_ui.list_open = false;
        self.chat_ui.model_open = false;
        self.chat_ui.style_open = false;
        self.live.clear();
        self.live_question = request.text.clone();
        self.question
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.chat_following = true;
        self.chat_list.scroll_to_end();
        self.status = "AI 正在回答……".into();
        let tid = self.tid.clone();
        self.worker(cx, move |cancel, tx| {
            let result = ai::ask_with_context(&ws, &tid, &request, &config, &cancel, |s| {
                let _ = tx.send(Event::Delta(s.into()));
            });
            if let Ok(raw) = ws.state() {
                let _ = tx.send(Event::Refresh(raw));
            }
            let _ = tx.send(match result {
                Ok(_) => Event::Status("回答已保存".into()),
                Err(error) => Event::Error(format!("回答中断：{error:#}")),
            });
        });
        self.home.cancellable_task = true;
    }

    pub(in crate::desktop) fn new_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.chat_ui.streaming {
            return;
        }
        self.tid = format!("t{}", Uuid::new_v4().simple());
        self.chat_ui.refs.clear();
        self.chat_ui.no_auto = false;
        self.chat_ui.pending_note = None;
        self.chat_ui.model = self.settings.committed.ask_model.clone();
        self.chat_ui.reasoning = None;
        self.chat_ui.model_open = false;
        self.chat_ui.style_open = false;
        self.chat_ui.style = AnswerStyle::Standard;
        self.chat_ui.list_open = false;
        self.chat_ui.error = None;
        self.question
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.chat_list.reset(0);
        self.question.read(cx).focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    pub(in crate::desktop) fn select_chat_thread(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.chat_ui.streaming {
            return;
        }
        self.tid = id.to_owned();
        // No explicit thread model means follow the current Ask AI default.
        self.chat_ui.model.clear();
        self.chat_ui.reasoning = None;
        self.chat_ui.model_open = false;
        self.chat_ui.style_open = false;
        self.chat_ui.style = AnswerStyle::Standard;
        if let Some(thread) = self.raw["chat"]["threads"]
            .as_array()
            .and_then(|threads| threads.iter().find(|t| t["id"] == id))
        {
            self.chat_ui.style = AnswerStyle::from_value(&thread["answer_style"]);
            let model = thread["model"].as_str().unwrap_or("");
            if let Some(profile) = self
                .settings
                .committed
                .models
                .iter()
                .find(|m| m.id == model || m.model == model)
            {
                self.chat_ui.model = profile.id.clone();
                self.chat_ui.reasoning = thread["chat_options"]["reasoning_effort"]
                    .as_str()
                    .filter(|effort| profile.supports_reasoning(effort))
                    .map(str::to_owned);
            }
        }
        self.chat_ui.list_open = false;
        self.chat_ui.pending_note = None;
        self.chat_ui.error = None;
        self.chat_list.reset(0);
        self.chat_following = true;
        cx.notify();
    }

    pub(in crate::desktop) fn change_chat_record(
        &mut self,
        op: &'static str,
        id: String,
        value: String,
        cx: &mut Context<Self>,
    ) {
        if self.busy || self.chat_ui.streaming {
            return;
        }
        let Some(ws) = self.workspace.clone() else {
            return;
        };
        let tid = self.tid.clone();
        if op == "delete" && self.tid == id {
            self.tid = format!("t{}", Uuid::new_v4().simple());
            self.chat_list.reset(0);
        }
        self.chat_ui.rename_id = None;
        self.chat_ui.delete_id = None;
        self.worker(cx, move |_, tx| {
            let result = match op {
                "rename" => ai::rename_chat_thread(&ws, &id, &value),
                "delete" => ai::delete_chat_thread(&ws, &id),
                _ => ai::pin_chat_message(&ws, &tid, &id),
            };
            match result.and_then(|_| ws.state()) {
                Ok(raw) => {
                    let _ = tx.send(Event::Refresh(raw));
                }
                Err(error) => {
                    let _ = tx.send(Event::Error(format!("对话更新失败：{error:#}")));
                }
            }
        });
    }
}
