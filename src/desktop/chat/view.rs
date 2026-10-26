//! Chat view.
use super::*;

impl ReaderView {
    pub(in crate::desktop) fn chat_header(&self, cx: &mut Context<Self>) -> AnyElement {
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

    pub(in crate::desktop) fn chat_thread_menu(&self, cx: &mut Context<Self>) -> AnyElement {
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

    pub(in crate::desktop) fn chat_composer(&self, cx: &mut Context<Self>) -> AnyElement {
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

    pub(in crate::desktop) fn chat(&mut self, cx: &mut Context<Self>) -> AnyElement {
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
}
