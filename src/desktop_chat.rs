//! Ask AI: EasyRead's slide-in reader column, context chips and streaming chat.
use super::*;
use easyread_gpui::ai::{AnswerStyle, ChatReference, ChatRequest};

pub(super) const SLIDE_MS: u64 = 220;

pub(super) struct ChatUi {
    pub generation: usize,
    pub slide_started: Option<Instant>,
    pub closing: bool,
    pub streaming: bool,
    pub list_open: bool,
    pub model_open: bool,
    pub style_open: bool,
    pub model: String,
    pub reasoning: Option<String>,
    pub model_focus: FocusHandle,
    pub model_cursor: usize,
    pub model_scroll: ScrollHandle,
    pub model_bounds: Bounds<Pixels>,
    pub model_menu_width: f32,
    pub style: AnswerStyle,
    pub style_focus: FocusHandle,
    pub style_cursor: usize,
    pub style_scroll: ScrollHandle,
    pub style_bounds: Bounds<Pixels>,
    pub style_menu_width: f32,
    pub refs: Vec<ChatReference>,
    pub live_refs: Vec<ChatReference>,
    pub no_auto: bool,
    pub pending_note: Option<String>,
    pub error: Option<String>,
    pub rename_id: Option<String>,
    pub delete_id: Option<String>,
    pub rename_input: Entity<InputState>,
    pub resizing: bool,
}
impl ChatUi {
    pub fn new(window: &mut Window, cx: &mut Context<ReaderView>) -> Self {
        Self {
            generation: 0,
            slide_started: None,
            closing: false,
            streaming: false,
            list_open: false,
            model_open: false,
            style_open: false,
            model: String::new(),
            reasoning: None,
            model_focus: cx.focus_handle(),
            model_cursor: 0,
            model_scroll: ScrollHandle::new(),
            model_bounds: Bounds::default(),
            model_menu_width: 370.,
            style: AnswerStyle::default(),
            style_focus: cx.focus_handle(),
            style_cursor: 0,
            style_scroll: ScrollHandle::new(),
            style_bounds: Bounds::default(),
            style_menu_width: 500.,
            refs: vec![],
            live_refs: vec![],
            no_auto: false,
            pending_note: None,
            error: None,
            rename_id: None,
            delete_id: None,
            rename_input: cx.new(|cx| InputState::new(window, cx)),
            resizing: false,
        }
    }
}

/// CSS cubic-bezier(.2,.8,.2,1), including inversion of its x coordinate.
pub(super) fn slide_ease(progress: f32) -> f32 {
    if progress <= 0. {
        return 0.;
    }
    if progress >= 1. {
        return 1.;
    }
    let curve = |t: f32, a: f32, b: f32| {
        3. * (1. - t).powi(2) * t * a + 3. * (1. - t) * t * t * b + t.powi(3)
    };
    let (mut low, mut high) = (0., 1.);
    for _ in 0..18 {
        let mid = (low + high) / 2.;
        if curve(mid, 0.2, 0.2) < progress {
            low = mid;
        } else {
            high = mid;
        }
    }
    curve((low + high) / 2., 0.8, 1.)
}

impl ReaderView {
    pub(super) fn tool_panel_visible(&self) -> bool {
        self.panel_open || self.chat_ui.closing
    }

    pub(super) fn chat_panel_width(&self, viewport: f32) -> f32 {
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

    pub(super) fn open_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    pub(super) fn close_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    pub(super) fn add_chat_reference(&mut self, reference: ChatReference) {
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

    fn chat_references(&self) -> Vec<ChatReference> {
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

    fn chat_reference_label(&self, reference: &ChatReference) -> String {
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

    pub(super) fn ask(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    fn new_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    pub(super) fn select_chat_thread(&mut self, id: &str, cx: &mut Context<Self>) {
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

    fn change_chat_record(
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

    fn chat_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let title = self.raw["chat"]["threads"]
            .as_array()
            .and_then(|ts| ts.iter().find(|t| t["id"] == self.tid))
            .and_then(|t| t["title"].as_str())
            .unwrap_or("新对话")
            .to_owned();
        div()
            .id("chat-header")
            .debug_selector(|| "chat-header".into())
            .h(px(50.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(4.))
            .px(px(10.))
            .border_b_1()
            .border_color(rgb(0xe3ddd2))
            .bg(rgb(0xfbfaf7))
            .child(
                Button::new("chat-title")
                    .debug_selector(|| "chat-title".into())
                    .ghost()
                    .max_w(relative(0.72))
                    .h(px(34.))
                    .label(format!("☰  {title}  ⌄"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.chat_ui.list_open = !this.chat_ui.list_open;
                        this.chat_ui.model_open = false;
                        this.chat_ui.style_open = false;
                        cx.notify();
                    })),
            )
            .child(div().flex_1())
            .child(
                Button::new("new-chat")
                    .debug_selector(|| "chat-new".into())
                    .ghost()
                    .size(px(32.))
                    .label("+")
                    .accessibility_label("新对话")
                    .disabled(self.chat_ui.streaming)
                    .on_click(cx.listener(|this, _, window, cx| this.new_chat(window, cx))),
            )
            .child(
                Button::new("close-chat")
                    .debug_selector(|| "chat-close".into())
                    .ghost()
                    .size(px(32.))
                    .label("×")
                    .accessibility_label("关闭 Ask AI")
                    .on_click(cx.listener(|this, _, window, cx| this.close_chat(window, cx))),
            )
            .into_any_element()
    }

    fn chat_thread_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut threads = self.raw["chat"]["threads"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        threads.sort_by(|a, b| b["updated"].as_str().cmp(&a["updated"].as_str()));
        div()
            .id("chat-threads")
            .debug_selector(|| "chat-threads".into())
            .absolute()
            .top(px(48.))
            .left(px(8.))
            .right(px(8.))
            .max_h(px(360.))
            .overflow_y_scroll()
            .p(px(6.))
            .rounded(px(12.))
            .bg(rgb(0xfbfaf7))
            .border_1()
            .border_color(rgb(0xe3ddd2))
            .child(
                Button::new("chat-list-new")
                    .ghost()
                    .w_full()
                    .label("+  新对话")
                    .disabled(self.chat_ui.streaming)
                    .on_click(cx.listener(|this, _, window, cx| this.new_chat(window, cx))),
            )
            .children(threads.into_iter().enumerate().map(|(index, thread)| {
                let id = thread["id"].as_str().unwrap_or("").to_owned();
                let choose = id.clone();
                let rename = id.clone();
                let delete = id.clone();
                let title = thread["title"].as_str().unwrap_or("对话").to_owned();
                let rename_title = title.clone();
                let count = thread["messages"]
                    .as_array()
                    .map(|m| m.len() / 2)
                    .unwrap_or(0);
                div()
                    .flex()
                    .items_center()
                    .rounded(px(8.))
                    .when(id == self.tid, |d| d.bg(rgb(0xe5eeed)))
                    .child(
                        Button::new(("chat-thread", index))
                            .ghost()
                            .flex_1()
                            .min_w_0()
                            .label(format!("{title} · {count} 问"))
                            .disabled(self.chat_ui.streaming)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.select_chat_thread(&choose, cx)
                            })),
                    )
                    .child(
                        Button::new(("chat-rename", index))
                            .ghost()
                            .xsmall()
                            .label("改名")
                            .disabled(self.busy)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.chat_ui.rename_id = Some(rename.clone());
                                this.chat_ui.rename_input.update(cx, |input, cx| {
                                    input.set_value(rename_title.clone(), window, cx)
                                });
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new(("chat-delete", index))
                            .ghost()
                            .xsmall()
                            .label("删除")
                            .disabled(self.busy)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.chat_ui.delete_id = Some(delete.clone());
                                cx.notify();
                            })),
                    )
            }))
            .when(self.chat_ui.rename_id.is_some(), |d| {
                d.child(
                    div()
                        .p_2()
                        .flex()
                        .gap_2()
                        .child(Input::new(&self.chat_ui.rename_input).flex_1())
                        .child(
                            Button::new("chat-rename-save")
                                .ghost()
                                .label("保存")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(id) = this.chat_ui.rename_id.clone() {
                                        let title =
                                            this.chat_ui.rename_input.read(cx).value().to_string();
                                        if !title.trim().is_empty() {
                                            this.change_chat_record("rename", id, title, cx);
                                        }
                                    }
                                })),
                        )
                        .child(
                            Button::new("chat-rename-cancel")
                                .ghost()
                                .label("取消")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.chat_ui.rename_id = None;
                                    cx.notify();
                                })),
                        ),
                )
            })
            .when(self.chat_ui.delete_id.is_some(), |d| {
                d.child(
                    div()
                        .p_2()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child("删除这个对话？已放到页边的讨论不受影响。")
                        .child(
                            Button::new("chat-delete-confirm")
                                .danger()
                                .label("确认删除")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(id) = this.chat_ui.delete_id.clone() {
                                        this.change_chat_record("delete", id, String::new(), cx);
                                    }
                                })),
                        )
                        .child(
                            Button::new("chat-delete-cancel")
                                .ghost()
                                .label("取消")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.chat_ui.delete_id = None;
                                    cx.notify();
                                })),
                        ),
                )
            })
            .into_any_element()
    }

    fn chat_composer(&self, cx: &mut Context<Self>) -> AnyElement {
        let references = self.chat_references();
        let automatic = self.chat_ui.refs.is_empty();
        let send_color = rgb(if self.chat_ui.streaming {
            0x27231d
        } else {
            0x387487
        });
        div().debug_selector(|| "chat-composer".into()).relative().flex_shrink_0().w_full().max_w(px(780.)).mx_auto()
            .border_1().border_color(rgb(0xd4cbbb)).rounded(px(16.)).bg(rgb(0xfbfaf7)).p(px(8.))
            .child(div().flex().flex_wrap().gap(px(6.)).children(references.into_iter().enumerate().map(|(index, reference)| {
                let label = self.chat_reference_label(&reference);
                let target = json!({"id":"reader-navigation-target","anchor":reference.anchor,"quote":reference.quote});
                div().debug_selector(|| "chat-reference".into()).flex().items_center().max_w_full().rounded(px(12.)).bg(rgb(READER_BACKGROUND))
                    .child(Button::new(("chat-ref",index)).ghost().xsmall().min_w_0().label(format!("{} {label}",if automatic {"▤"} else {"↗"}))
                        .on_click(cx.listener(move |this,_,window,cx| this.jump_to_annotation(target.clone(),window,cx))))
                    .child(Button::new(("chat-unref",index)).ghost().xsmall().label("×").accessibility_label("不带这段")
                        .on_click(cx.listener(move |this,_,_,cx| {
                            if automatic {this.chat_ui.no_auto=true;} else if index < this.chat_ui.refs.len() {this.chat_ui.refs.remove(index);}
                            this.chat_ui.pending_note=None;cx.notify();
                        })))
            })))
            .child(div().debug_selector(|| "chat-input".into()).py(px(6.)).child(Textarea::new(&self.question).appearance(false).bordered(false).text_size(px(14.5)).aria_label("问点什么…")))
            .child(div().flex().items_center().gap(px(6.))
                .child(self.chat_model_picker(cx))
                .child(self.chat_style_picker(cx))
                .child(div().flex_1())
                .child(Button::new("chat-send").debug_selector(|| "chat-send".into()).custom(ButtonCustomVariant::new(cx)
                        .color(send_color.into()).foreground(rgb(0xffffff).into()).hover(send_color.into()).active(send_color.into()))
                    .bg(send_color).size(px(32.)).rounded(px(16.))
                    .label(if self.chat_ui.streaming {"■"} else {"↑"}).accessibility_label(if self.chat_ui.streaming {"停止回答"} else {"发送（Enter）；Shift+Enter 换行"})
                    .disabled(!self.chat_ui.streaming && (self.busy || self.workspace.is_none() || self.question.read(cx).value().trim().is_empty()))
                    .on_click(cx.listener(|this,_,window,cx| {
                        if this.chat_ui.streaming {this.cancel.store(true,Ordering::Relaxed);} else {this.ask(window,cx);}
                    }))))
            .into_any_element()
    }

    pub(super) fn chat(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let mut messages = self.raw["chat"]["threads"]
            .as_array()
            .and_then(|ts| ts.iter().find(|t| t["id"] == self.tid))
            .and_then(|t| t["messages"].as_array())
            .cloned()
            .unwrap_or_default();
        if !self.live_question.is_empty() {
            messages.push(
                json!({"role":"user","content":self.live_question,"refs":self.chat_ui.live_refs}),
            );
            messages.push(json!({"role":"assistant","content":self.live,"live":true,"answer_style":self.chat_ui.style}));
        }
        let count = messages.len();
        let old = self.chat_list.item_count();
        if count > old {
            self.chat_list.splice(old..old, count - old);
        } else if count < old {
            self.chat_list.splice(count..old, 0);
        }
        let weak = cx.entity().downgrade();
        let serif = self.fonts.serif_zh.clone();
        let header = self.chat_header(cx);
        let composer = self.chat_composer(cx);
        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .min_h_0()
            .bg(rgb(READER_BACKGROUND))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(header)
            .when(count == 0, |d| {
                d.child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .justify_center()
                        .items_center()
                        .p(px(24.))
                        .gap(px(12.))
                        .child(
                            div()
                                .text_size(px(32.))
                                .text_color(rgb(0x387487))
                                .child("✳"),
                        )
                        .child(div().text_size(px(18.)).child("边读边问"))
                        .child(div().text_size(px(13.)).text_color(rgb(0x969084)).child(
                            "默认带上正在读的段落；选中文字后点 Ask AI，可以引用多段一起问。",
                        ))
                        .children(
                            [
                                "这段在说什么？用大白话讲一遍",
                                "这个公式每一项是什么意思？怎么推出来的？",
                                "这里的结论靠得住吗？有什么前提？",
                            ]
                            .into_iter()
                            .enumerate()
                            .map(|(index, text)| {
                                Button::new(("chat-suggest", index))
                                    .ghost()
                                    .w_full()
                                    .max_w(px(520.))
                                    .border_1()
                                    .border_color(rgb(0xe3ddd2))
                                    .rounded(px(10.))
                                    .bg(rgb(0xfbfaf7))
                                    .label(text)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.question.update(cx, |input, cx| {
                                            input.set_value(text, window, cx)
                                        });
                                        this.ask(window, cx);
                                    }))
                            }),
                        ),
                )
            })
            .when(count > 0, |d| {
                d.child(
                    list(self.chat_list.clone(), move |index, window, cx| {
                        let message = &messages[index];
                        let user = message["role"] == "user";
                        let body = message["content"].as_str().unwrap_or("").to_owned();
                        let live = message["live"] == true;
                        let error =
                            matches!(message["status"].as_str(), Some("error" | "cancelled"));
                        let style = AnswerStyle::from_value(&message["answer_style"]);
                        let model = ai::answer_model_label(message);
                        let copy = body.clone();
                        let copy_weak = weak.clone();
                        let pin_weak = weak.clone();
                        let mid = message["id"].as_str().unwrap_or("").to_owned();
                        let refs = message["refs"].as_array().cloned().unwrap_or_else(|| {
                            if message["anchor"].is_string() { vec![message.clone()] } else { vec![] }
                        });
                        let rendered_body = if body.is_empty() && error { "没能回答，请重试。" }
                            else if body.is_empty() { "思考中…" } else { &body };
                        let rich_text = weak.upgrade().map(|reader| reader.update(cx, |reader, cx| {
                            let width=reader.chat_panel_width(f32::from(window.viewport_size().width));
                            let width=if user { (width-36.)*0.88-28. } else { width-65. };
                            reader.rich_markdown(format!("chat-{}-{index}",reader.tid),rendered_body,width.max(32.),cx)
                        })).unwrap_or_else(|| gpui_kit::base::TextView::markdown(("chat-fallback",index),rendered_body.to_owned()));
                        let reference_buttons = refs.into_iter().enumerate().map(|(r_index, reference)| {
                            let label = weak.upgrade().map(|reader| reader.read(cx).chat_reference_label(&ChatReference {
                                anchor: reference["anchor"].as_str().unwrap_or("").to_owned(),
                                quote: reference["quote"].as_str().unwrap_or("").to_owned(),
                            })).unwrap_or_default();
                            let reference_weak = weak.clone();
                            let target = json!({"id":"reader-navigation-target","anchor":reference["anchor"],"quote":reference["quote"]});
                            Button::new(SharedString::from(format!("message-reference-{index}-{r_index}"))).ghost().xsmall()
                                .text_color(rgb(0x969084)).label(format!("↗ {label}"))
                                .on_click(move |_,window,cx| {
                                    let _ = reference_weak.update(cx, |this,cx| this.jump_to_annotation(target.clone(),window,cx));
                                })
                        }).collect::<Vec<_>>();
                        div()
                            .w_full()
                            .px(px(18.))
                            .py(px(9.))
                            .child(
                                div()
                                    .max_w(px(760.))
                                    .mx_auto()
                                    .w_full()
                                    .flex()
                                    .flex_col()
                                    .when(user, |d| {
                                        d.items_end().child(
                                            div()
                                                .max_w(relative(0.88))
                                                .rounded(px(14.))
                                                .bg(rgb(0xfbfaf7))
                                                .border_1()
                                                .border_color(rgb(0xe3ddd2))
                                                .px(px(13.))
                                                .py(px(9.))
                                                .child(
                                                    rich_text.clone()
                                                    .selectable(true)
                                                    .text_size(px(14.))
                                                    .line_height(relative(1.7)),
                                                ),
                                        ).children(reference_buttons)
                                    })
                                    .when(!user, |d| {
                                        d.child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(7.))
                                                .text_size(px(12.5))
                                                .text_color(rgb(0x625e55))
                                                .child(
                                                    div()
                                                        .size(px(22.))
                                                        .rounded(px(11.))
                                                        .bg(rgb(0x387487))
                                                        .text_color(rgb(0xffffff))
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .child("✳"),
                                                )
                                                .child(model)
                                                .when(style != AnswerStyle::Standard, |d| {
                                                    d.child(style.label())
                                                })
                                                .when(live, |d| d.child("思考中…")),
                                        )
                                        .child(
                                            div()
                                                .pl(px(29.))
                                                .when(error, |d| d.text_color(rgb(0xa34f21)))
                                                .child(
                                                    rich_text.clone()
                                                    .selectable(true)
                                                    .font_family(serif.clone())
                                                    .text_size(px(15.))
                                                    .line_height(relative(1.9)),
                                                ),
                                        )
                                        .when(error, |d| {
                                            d.child(
                                                div()
                                                    .pl(px(29.))
                                                    .text_size(px(12.))
                                                    .text_color(rgb(0xa34f21))
                                                    .child(if message["status"] == "cancelled" {
                                                        "（已停止）"
                                                    } else {
                                                        "回答中断，已保留部分内容，可重试。"
                                                    }),
                                            )
                                        })
                                        .when(
                                            !live && !error && !mid.is_empty(),
                                            |d| {
                                                d.child(
                                                    div()
                                                        .pl(px(29.))
                                                        .flex()
                                                        .gap(px(14.))
                                                        .child(
                                                            Button::new(("chat-copy", index))
                                                                .ghost()
                                                                .xsmall()
                                                                .label("复制")
                                                                .on_click(move |_, _, cx| {
                                                                    cx.write_to_clipboard(
                                                                        ClipboardItem::new_string(
                                                                            copy.clone(),
                                                                        ),
                                                                    );
                                                                    let _ = copy_weak.update(
                                                                        cx,
                                                                        |this, cx| {
                                                                            this.status =
                                                                                "已复制".into();
                                                                            cx.notify();
                                                                        },
                                                                    );
                                                                }),
                                                        )
                                                        .child(
                                                            Button::new(("chat-pin", index))
                                                                .ghost()
                                                                .xsmall()
                                                                .label("放到页边")
                                                                .on_click(move |_, _, cx| {
                                                                    let _ = pin_weak.update(
                                                                        cx,
                                                                        |this, cx| {
                                                                            this.change_chat_record(
                                                                                "pin",
                                                                                mid.clone(),
                                                                                String::new(),
                                                                                cx,
                                                                            )
                                                                        },
                                                                    );
                                                                }),
                                                        ),
                                                )
                                            },
                                        )
                                    }),
                            )
                            .into_any_element()
                    })
                    .flex_1()
                    .min_h_0(),
                )
            })
            .when(!self.chat_following, |d| {
                d.child(
                    Button::new("chat-bottom")
                        .ghost()
                        .label("回到底部查看回答")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.chat_list.scroll_to_end();
                            this.chat_following = true;
                            cx.notify();
                        })),
                )
            })
            .when_some(self.chat_ui.error.clone(), |d, error| {
                d.child(
                    div()
                        .px(px(18.))
                        .py(px(6.))
                        .text_size(px(12.))
                        .text_color(rgb(0xa34f21))
                        .child(error),
                )
            })
            .child(
                div()
                    .w_full()
                    .px(px(12.))
                    .pt(px(8.))
                    .pb(px(12.))
                    .child(composer),
            )
            .when(self.chat_ui.list_open, |d| {
                d.child(self.chat_thread_menu(cx))
            })
            .into_any_element()
    }

    fn finish_chat_resize(&mut self, cx: &mut Context<Self>) {
        self.chat_ui.resizing = false;
        if let Some(library) = &self.library
            && let Err(error) = self.settings.committed.save(&library.root)
        {
            self.status = format!("面板宽度保存失败：{error:#}");
        }
        self.reset_rows();
        cx.notify();
    }

    pub(super) fn chat_column(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let viewport = f32::from(window.viewport_size().width);
        let width = self.chat_panel_width(viewport);
        self.chat_ui.model_menu_width = (width - 24.).clamp(180., 370.);
        self.chat_ui.style_menu_width = (width - 24.).clamp(180., 500.);
        let closing = self.chat_ui.closing;
        let panel = self.chat(cx);
        let resize_view = cx.entity().downgrade();
        // Keep the transition clock in ReaderView: remounting popup / message
        // children must not restart the whole panel's slide animation.
        let progress = self
            .chat_ui
            .slide_started
            .map(|start| (start.elapsed().as_secs_f32() / (SLIDE_MS as f32 / 1000.)).clamp(0., 1.))
            .unwrap_or(1.);
        if progress < 1. && !cx.reduce_motion() {
            window.request_animation_frame();
        }
        let eased = if cx.reduce_motion() {
            1.
        } else {
            slide_ease(progress)
        };
        let offset = width * 1.02 * if closing { eased } else { 1. - eased };
        div()
            .id("chat-column")
            .debug_selector(|| "chat-column".into())
            .h_full()
            .flex_shrink_0()
            .relative()
            .when(viewport >= 1180., |d| d.w(px(width)).overflow_hidden())
            .when(viewport < 1180., |d| {
                d.absolute()
                    .right_0()
                    .top_0()
                    .bottom_0()
                    .w(px(width))
                    .overflow_hidden()
            })
            .child(
                div()
                    .id("chat-panel")
                    .debug_selector(|| "chat-panel".into())
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .w(px(width))
                    .border_l_1()
                    .border_color(rgb(0xe3ddd2))
                    .left(px(offset))
                    .child(panel),
            )
            .when(viewport >= 1180. && !closing, |d| {
                d.child(
                    div()
                        .id("chat-panel-grip")
                        .debug_selector(|| "chat-panel-grip".into())
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(px(6.))
                        .cursor(CursorStyle::ResizeLeftRight)
                        .hover(|d| d.bg(rgba(0x38748722)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, event: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                if event.click_count == 2 {
                                    this.settings.committed.extra.remove("side_panel_width");
                                    this.finish_chat_resize(cx);
                                } else {
                                    this.chat_ui.resizing = true;
                                }
                            }),
                        ),
                )
            })
            .child(
                canvas(
                    |_, _, _| {},
                    move |_, _, window, _cx| {
                        let moving = resize_view.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase == DispatchPhase::Capture {
                                let _ = moving.update(cx, |this, cx| {
                                    if this.chat_ui.resizing
                                        && event.pressed_button == Some(MouseButton::Left)
                                    {
                                        let viewport = f32::from(window.viewport_size().width);
                                        let width = (viewport - f32::from(event.position.x))
                                            .clamp(320., (viewport - 420.).max(320.));
                                        this.settings
                                            .committed
                                            .extra
                                            .insert("side_panel_width".into(), json!(width));
                                        cx.notify();
                                    }
                                });
                            }
                        });
                        let release = resize_view.clone();
                        window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture {
                                let _ = release.update(cx, |this, cx| {
                                    if this.chat_ui.resizing {
                                        this.finish_chat_resize(cx);
                                    }
                                });
                            }
                        });
                    },
                )
                .absolute()
                .size_0(),
            )
            .into_any_element()
    }
}
