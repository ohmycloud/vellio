//! Paragraph-local translation editing. No tool panel is opened.
use super::*;
use easyread_gpui::paragraph_edit::{ParagraphEdit, split_flow};

pub(super) struct InlineTranslationEdit {
    pub block: usize,
    pub key: String,
    pub input: Entity<TextareaState>,
    pub plan: ParagraphEdit,
    before: String,
    after: String,
    language: bool,
    previous_expanded: Option<std::collections::HashSet<usize>>,
    previously_unpaired: bool,
    expected_edit: Value,
    source: String,
    base: String,
    pub saving: bool,
    pub error: Option<String>,
}

impl ReaderView {
    pub(super) fn begin_inline_edit(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || !self.settings.committed.features.edit || self.inline_edit.is_some() {
            return;
        }
        let Some(block) = self.state.paper.blocks.get(index).cloned() else {
            return;
        };
        if !matches!(
            block.kind.as_str(),
            "para" | "heading" | "note" | "figure" | "table"
        ) {
            return;
        }
        let (language, paragraph) = self
            .active_reading_paragraph
            .filter(|(block, _, _)| *block == index)
            .map(|(_, language, paragraph)| (language, paragraph))
            .unwrap_or((self.primary_original(), 0));
        let paragraph = if let Some((native_text, native_range)) = self
            .text_views
            .get(&(index, language))
            .and_then(|(_, view)| view.read(cx).paragraph_text_context(paragraph))
        {
            let flow = self.reader_layout_html(index, language);
            let plain = annotations::plain_text(&flow);
            let mapped =
                annotations::rendered_paragraph_quote(&plain, &native_text, native_range, None)
                    .and_then(|(_, range)| {
                        easyread_gpui::paragraph_edit::paragraph_at_plain_range(&flow, &range)
                    });
            let Some(paragraph) = mapped else {
                self.status = "无法可靠定位该段译文，请重新选择正文段落".into();
                cx.notify();
                return;
            };
            paragraph
        } else {
            paragraph
        };
        let previous_expanded = self
            .paragraph_languages
            .get(&index)
            .map(|expansion| expansion.expanded.clone());
        let previously_unpaired = self.unpaired_paragraphs.contains(&index);
        self.ensure_paragraph_layout(index);
        // A single-paragraph source still needs a local translated companion
        // when it is opened for editing in original-language mode.
        if language && self.primary_original() && !self.paragraph_languages.contains_key(&index) {
            let source = self.reader_language_layout_html(index, true);
            let translation = self.reader_language_layout_html(index, false);
            if let Some(layout) = easyread_gpui::paragraph_translation::PairLayout::for_bilingual(
                &source,
                &translation,
                true,
                (block.source_kind == "pdf-text").then_some(block.page),
            ) {
                self.paragraph_languages.insert(
                    index,
                    reading_mode::ParagraphExpansion {
                        layout,
                        expanded: std::collections::HashSet::new(),
                    },
                );
            }
        }
        let primary = self.primary_original();
        let target = if let Some(expansion) = self.paragraph_languages.get_mut(&index) {
            expansion
                .layout
                .pair_at(paragraph, &expansion.expanded)
                .and_then(|pair| {
                    if primary {
                        expansion.expanded.insert(pair);
                    }
                    expansion
                        .layout
                        .translation_target(pair, &expansion.expanded, primary)
                        .map(|(translation, display)| (translation, display, primary))
                })
        } else if !language {
            Some((paragraph, paragraph, false))
        } else {
            None
        };
        let Some((translation, display, language)) = target else {
            self.status = "无法可靠对应该段译文，请在译文模式中选择要修订的段落".into();
            cx.notify();
            return;
        };
        let key = if matches!(block.kind.as_str(), "figure" | "table") {
            format!("{}#caption", block.id)
        } else {
            block.id.clone()
        };
        let markdown = self.text_at(index);
        let translation_html = self.reader_language_layout_html(index, false);
        let flow = self.reader_body_html(index, language);
        let prepared =
            ParagraphEdit::new(&markdown, &translation_html, translation).and_then(|plan| {
                split_flow(&flow, display).map(|(before, after)| (plan, before, after))
            });
        let (plan, before, after) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                self.status = format!("无法编辑该段落：{error}");
                cx.notify();
                return;
            }
        };
        self.queue_margin_note();
        self.margin_editor = None;
        self.selection = None;
        self.annotation_popup = None;
        self.panel_open = false;
        TextSelection::clear(window, cx);
        let input = cx.new(|cx| {
            let mut input = TextareaState::new(window, cx).auto_grow(2, 12);
            input.set_value(plan.text.clone(), window, cx);
            input
        });
        cx.subscribe(&input, move |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.list.remeasure_items(index..index + 1);
                this.epub_reflow.invalidate_from(index);
                cx.notify();
            }
        })
        .detach();
        let (zh, en) = if matches!(block.kind.as_str(), "figure" | "table") {
            (&block.caption_zh, &block.caption_en)
        } else {
            (&block.zh, &block.en)
        };
        self.inline_edit = Some(InlineTranslationEdit {
            block: index,
            key: key.clone(),
            input: input.clone(),
            plan,
            before,
            after,
            language,
            previous_expanded,
            previously_unpaired,
            expected_edit: self.raw["reader"]["edits"][&key].clone(),
            source: en.clone(),
            base: text_hash(if zh.is_empty() { en } else { zh }),
            saving: false,
            error: None,
        });
        self.reset_rows();
        input.read(cx).focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    pub(super) fn cancel_inline_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.inline_edit.as_ref().is_some_and(|edit| edit.saving) {
            return;
        }
        if let Some(edit) = self.inline_edit.take() {
            if let Some(expanded) = edit.previous_expanded {
                if let Some(expansion) = self.paragraph_languages.get_mut(&edit.block) {
                    expansion.expanded = expanded;
                }
            } else {
                self.paragraph_languages.remove(&edit.block);
            }
            if !edit.previously_unpaired {
                self.unpaired_paragraphs.remove(&edit.block);
            }
        }
        self.reset_rows();
        self.reader_focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn save_inline_edit(&mut self, cx: &mut Context<Self>) {
        let Some(workspace) = self.workspace.clone() else {
            return;
        };
        if self.busy {
            return;
        }
        let Some(edit) = self.inline_edit.as_mut() else {
            return;
        };
        let text = edit.input.read(cx).value().to_string();
        let (markdown, html) = match edit.plan.merge(&text) {
            Ok(result) => result,
            Err(error) => {
                edit.error = Some(format!("保存失败：{error}"));
                cx.notify();
                return;
            }
        };
        let op = json!({"op":"edit", "block":edit.key, "zh":markdown,
            "base":edit.base, "at":now(), "expected_edit":edit.expected_edit,
            "expected_text":text_hash(&edit.plan.markdown),
            "native_layout":{"html":html, "source_hash":text_hash(&edit.source), "translation_hash":text_hash(&markdown)}});
        edit.saving = true;
        edit.error = None;
        let root = workspace.root.clone();
        self.worker(cx, move |_, tx| {
            let result = (|| -> anyhow::Result<Value> {
                workspace.reader_ops(std::slice::from_ref(&op))?;
                let raw = workspace.state()?;
                anyhow::ensure!(
                    easyread_gpui::session::draft_ops_committed(
                        &raw["reader"],
                        std::slice::from_ref(&op)
                    ),
                    "保存未生效，草稿已保留"
                );
                Ok(raw)
            })()
            .map_err(|error| format!("保存失败：{error:#}"));
            let _ = tx.send(Event::InlineEditSaved(root, result));
        });
    }

    pub(super) fn inline_edit_flow(
        &self,
        index: usize,
        original: bool,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let edit = self
            .inline_edit
            .as_ref()
            .filter(|edit| edit.block == index && edit.language == original)?;
        let caption = self.reader_figure_caption(index);
        let secondary = original != self.primary_original() && self.has_original(index);
        let text_size = self.appearance.font_size * typography::text_scale(secondary, caption);
        let family = self.fonts.reading(
            self.appearance.reading_font(),
            original || self.state.paper.blocks[index].zh.is_empty(),
        );
        let fragment = |part: &'static str, html: String| {
            gpui_kit::base::TextView::html(
                (SharedString::from(format!("inline-{part}")), index),
                html,
            )
            .markdown_extensions(
                gpui_kit::base::text::MarkdownExtensions::default()
                    .plugin(math::MathPlugin {
                        max_width: self.appearance.text_width(),
                    })
                    .plugin(math::TablePlugin {
                        max_width: self.appearance.text_width(),
                    }),
            )
            .selectable(false)
            .style(typography::reader_rich_style(
                self,
                index,
                original,
                family.clone(),
                text_size,
                cx,
            ))
            .font_family(family.clone())
            .text_color(rgb(if secondary || caption {
                typography::READER_MUTED_INK
            } else {
                typography::READER_INK
            }))
            .text_size(px(text_size))
            .line_height(relative(if secondary {
                typography::SECONDARY_LINE_HEIGHT
            } else if caption {
                typography::CAPTION_LINE_HEIGHT
            } else {
                self.appearance.line_spacing
            }))
            .when(caption, |text| text.text_center())
        };
        Some(
            div()
                .flex()
                .flex_col()
                .w_full()
                .child(fragment("before", edit.before.clone()))
                .child(
                    div()
                        .id("inline-translation-editor")
                        .debug_selector(|| "inline-translation-editor".into())
                        .relative()
                        .w_full()
                        .my_2()
                        .p_3()
                        .rounded(px(10.))
                        .border_1()
                        .border_color(rgb(0xd5cbbb))
                        .bg(rgb(0xfbfaf7))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .absolute()
                                .left(px(-6.))
                                .top(px(16.))
                                .bottom(px(16.))
                                .w(px(3.))
                                .rounded(px(2.))
                                .bg(rgb(0xd9af54)),
                        )
                        .child(
                            Textarea::new(&edit.input)
                                .appearance(false)
                                .bordered(false)
                                .aria_label("原地修订译文")
                                .disabled(edit.saving)
                                .text_size(px(text_size))
                                .font_family(
                                    self.fonts.reading(self.appearance.reading_font(), false),
                                )
                                .line_height(relative(self.appearance.line_spacing)),
                        )
                        .when_some(edit.error.clone(), |d, error| {
                            d.child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(0xb0472f))
                                    .child(error),
                            )
                        })
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .mt_2()
                                .child(
                                    div()
                                        .flex_1()
                                        .text_size(px(12.))
                                        .text_color(rgb(0x969084))
                                        .child(
                                            "Ctrl+Enter 保存 · Esc 取消 · 支持 $公式$、**粗体**",
                                        ),
                                )
                                .child(
                                    div().debug_selector(|| "inline-edit-cancel".into()).child(
                                        Button::new("inline-edit-cancel")
                                            .ghost()
                                            .label("取消")
                                            .disabled(edit.saving)
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.cancel_inline_edit(window, cx)
                                            })),
                                    ),
                                )
                                .child(
                                    div().debug_selector(|| "inline-edit-save".into()).child(
                                        Button::new("inline-edit-save")
                                            .primary()
                                            .label(if edit.saving {
                                                "保存中…"
                                            } else {
                                                "保存"
                                            })
                                            .disabled(self.busy || self.workspace.is_none())
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.save_inline_edit(cx)
                                            })),
                                    ),
                                ),
                        ),
                )
                .child(fragment("after", edit.after.clone()))
                .into_any_element(),
        )
    }
}
