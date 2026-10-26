//! Reader block rendering and list row invalidation.
use super::*;

impl ReaderView {
    pub(super) fn reader_suppresses_pdf_figure(&self, index: usize) -> bool {
        let Some(figure) = self
            .state
            .paper
            .blocks
            .get(index)
            .filter(|b| b.source_kind == "pdf-image")
        else {
            return false;
        };
        self.state
            .paper
            .blocks
            .iter()
            .enumerate()
            .any(|(parent, text)| {
                if text.source_kind != "pdf-text" || text.page != figure.page {
                    return false;
                }
                let active = self.state.text(&text.id, &text.zh, &text.en);
                let raw = &self.raw["paper"]["blocks"][parent];
                // Persisted native layout already includes the image, even before
                // original PDF recovery finishes warming after reopening the book.
                // Companion images are omitted even on the conservative
                // block fallback, so only the primary flow can own a figure.
                let languages = [self.primary_original()];
                if languages.contains(&false)
                    && raw["native_translation_layout"]["inline_figures"]
                        .as_array()
                        .is_some_and(|ids| ids.iter().any(|id| id == &figure.id))
                    && easyread_gpui::translation_layout::saved_html(raw, active).is_some()
                {
                    return true;
                }
                self.pdf_code_html
                    .get(&parent)
                    .filter(|(source, _)| source == &text.en)
                    .and_then(|(_, p)| p.as_ref())
                    .is_some_and(|p| {
                        if !p.inline_figures.contains(&figure.id) {
                            return false;
                        }
                        if languages.contains(&true) || active == text.en {
                            return true;
                        }
                        // Legacy repair only embeds source-backed captioned image
                        // groups. Suppress duplicates only when ALL source previews
                        // really occur in the translated layout, not just because
                        // the original page contains them.
                        if p.figures.is_empty() {
                            return false;
                        }
                        if !languages.contains(&false) {
                            return false;
                        }
                        let html = self.reader_layout_html(parent, false);
                        p.figures
                            .iter()
                            .all(|f| html.contains(&format!("easyread-image:{}", f.id)))
                    })
            })
    }

    pub(super) fn block(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        self.request_nearby_pdf_code(index, cx);
        if self.reading_mode == ReadingMode::Bilingual {
            self.ensure_paragraph_layout(index);
        }
        if self.reader_suppresses_pdf_figure(index) {
            return div().into_any_element();
        }
        if self
            .state
            .paper
            .blocks
            .get(index)
            .is_some_and(|b| b.kind == "figure")
        {
            self.request_block_image(index, cx);
        }
        let block = self.state.paper.blocks.get(index).cloned();
        let Some(b) = block.as_ref() else {
            return div().into_any_element();
        };
        let caption =
            matches!(b.kind.as_str(), "figure" | "table") || self.reader_figure_caption(index);
        let english = if matches!(b.kind.as_str(), "figure" | "table") {
            b.caption_zh.is_empty()
        } else {
            b.zh.is_empty()
        };
        let mut content = div().flex().flex_col().gap_3().min_w_0();
        if b.kind == "figure" {
            let max_image_height = if self.is_epub() {
                self.epub_reflow
                    .size
                    .map(|s| f32::from(s.height) - 110.)
                    .unwrap_or(600.)
                    .max(40.)
            } else {
                f32::MAX
            };
            let media = match self.block_images.get(&index) {
                Some(Ok((image, aspect, width))) => div()
                    .relative()
                    .w_full()
                    .max_w(px((*width as f32)
                        .min(self.appearance.text_width())
                        .min(max_image_height * *aspect)))
                    .mx_auto()
                    .aspect_ratio(*aspect)
                    .child(
                        img(image.clone())
                            .id(("figure-image", index))
                            .debug_selector(move || format!("reader-figure-image-{index}"))
                            .absolute()
                            .size_full()
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if let Some(Ok((image, aspect, width))) =
                                    this.block_images.get(&index)
                                {
                                    this.open_image_preview(
                                        image.clone(),
                                        *aspect,
                                        *width,
                                        window,
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }
                            })),
                    )
                    .into_any_element(),
                Some(Err(error)) => div()
                    .p_4()
                    .border_1()
                    .border_color(rgb(0xd3cbbb))
                    .text_size(px(13.))
                    .text_color(rgb(0xb0472f))
                    .child(format!("图片无法显示：{error}"))
                    .into_any_element(),
                None => div()
                    .w_full()
                    .mx_auto()
                    .map(|d| {
                        let raw = &self.raw["paper"]["blocks"][index];
                        match raw["width"]
                            .as_f64()
                            .zip(raw["height"].as_f64())
                            .filter(|(w, h)| *w > 0. && *h > 0.)
                        {
                            Some((w, h)) => d
                                .max_w(px((w as f32)
                                    .min(self.appearance.text_width())
                                    .min(max_image_height * (w / h) as f32)))
                                .aspect_ratio((w / h) as f32),
                            None => d.h(px(180.)),
                        }
                    })
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(13.))
                    .text_color(rgb(0x8c8579))
                    .child("正在加载图片……")
                    .into_any_element(),
            };
            content = content.child(media);
        }
        if b.kind == "math" {
            match self.formulas.get(&b.tex) {
                Some(Ok(_)) => {
                    let source = format!(
                        "<p style=\"text-align:center\">{}</p>",
                        formula::native_html(&b.tex, true)
                    );
                    let view = self.reader_html_view(index, false, source, cx);
                    content =
                        content.child(self.reader_text(&view, false, false, false, index, cx));
                }
                Some(Err(e)) => {
                    content = content.child(TextView::markdown(
                        ("formula-error", index),
                        format!("```latex\n{}\n```\n排版失败：{e}", b.tex),
                    ))
                }
                None => {
                    content = content.child(TextView::markdown(
                        ("formula-pending", index),
                        format!("```latex\n{}\n```", b.tex),
                    ))
                }
            }
        } else if b.kind != "figure" || !self.state.caption(b).trim().is_empty() {
            for (position, original) in self.rendered_languages(index).into_iter().enumerate() {
                let source = self.reader_body_html(index, original);
                let view = self.reader_html_view(index, original, source, cx);
                let text = if let Some(inline) = self.inline_edit_flow(index, original, cx) {
                    inline
                } else {
                    self.reader_text(&view, original, caption, original || english, index, cx)
                };
                if position == 0 {
                    content = content.child(text);
                } else {
                    content = content.child(div().mt_2().text_color(rgb(0x77716a)).child(text));
                }
            }
        }
        let note_count = self.raw["reader"]["notes"]
            .as_object()
            .map(|notes| {
                notes
                    .values()
                    .filter(|n| n["anchor"] == b.id && !n["deleted"].as_bool().unwrap_or(false))
                    .count()
            })
            .unwrap_or(0);
        let margin = if !self.tool_panel_visible()
            && !self.show_pdf
            && (self.appearance.margin_notes || self.margin_editor.is_some())
        {
            let editing_id = self
                .margin_editor
                .as_ref()
                .filter(|editor| editor.selection.block == index)
                .and_then(|editor| editor.note.record["id"].as_str())
                .map(str::to_owned);
            let cards: Vec<_> = self
                .all_notes()
                .into_iter()
                .filter(|note| {
                    note["anchor"] == b.id
                        && self.appearance.margin_notes
                        && annotation_panel::personal(note)
                        && editing_id.as_deref() != note["id"].as_str()
                })
                .collect();
            let mut entries: Vec<_> = cards
                .iter()
                .map(|note| {
                    (
                        note["id"].as_str().unwrap_or("").to_owned(),
                        self.note_offset(note, index, cx),
                        self.annotation_card(note, false, cx),
                    )
                })
                .collect();
            if self.appearance.margin_notes {
                for entry in self.raw["discussion"]["entries"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|entry| {
                        entry["anchor"] == b.id
                            && entry["kind"] != "reply"
                            && entry["deleted"] != true
                    })
                {
                    entries.push((
                        entry["id"].as_str().unwrap_or("ai-entry").to_owned(),
                        self.note_offset(entry, index, cx),
                        div()
                            .px(px(12.))
                            .min_w_0()
                            .child(self.discussion_card(entry, false, cx))
                            .into_any_element(),
                    ));
                }
            }
            if let Some(editor) = self
                .margin_editor
                .as_ref()
                .filter(|editor| editor.selection.block == index)
                && let Some(view) = self.margin_note_view(cx)
            {
                entries.push((
                    editor.note.record["id"].as_str().unwrap_or("").to_owned(),
                    self.note_offset(&editor.note.record, index, cx),
                    view,
                ));
            }
            entries.sort_by(|a, b| {
                a.1.partial_cmp(&b.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.0.cmp(&b.0))
            });
            let mut cursor = px(0.);
            let mut column = div()
                .w(px(300.))
                .flex_shrink_0()
                .pt(px(34.))
                .pb_5()
                .flex()
                .flex_col();
            for (id, offset, element) in entries {
                let gap = (offset - cursor).max(px(0.));
                cursor += gap
                    + self
                        .margin_card_heights
                        .get(&id)
                        .copied()
                        .unwrap_or(px(160.))
                    + px(18.);
                column = column.child(div().flex_shrink_0().mt(gap).mb(px(18.)).child(element));
            }
            // Personal notes must never be clipped to the source page height.
            // Only a long AI answer gets its own persistent scroll viewport;
            // limiting this entire column used to hide subsequent questions.
            Some(
                column
                    .debug_selector(move || format!("pdf-margin-notes-{index}"))
                    .into_any_element(),
            )
        } else {
            None
        };
        let gutter_width = if self.settings.committed.features.page {
            48.
        } else {
            0.
        };
        let row_width = self.appearance.text_width()
            + 48.
            + gutter_width
            + if margin.is_some() { 324. } else { 0. };
        let gutter_top = self
            .active_reading_paragraph
            .filter(|(block, _, _)| *block == index)
            .and_then(|(_, original, paragraph)| {
                let bounds = self
                    .text_views
                    .get(&(index, original))?
                    .1
                    .read(cx)
                    .paragraph_bounds(paragraph)?;
                Some((bounds.top() - self.block_bounds.get(&index)?.top()).max(px(12.)))
            })
            .unwrap_or(px(12.));
        let emphasized = self.panel_open
            && self.panel == Panel::Notes
            && self.hovered_annotation.as_ref().map(|(_, block)| *block) == Some(index);
        let text = div()
            .w_full()
            .max_w(px(self.appearance.text_width() + 48.))
            .mx_auto()
            .px_6()
            .pt_3()
            .pb_5()
            .border_l_2()
            .border_color(if self.show_pdf && self.page_block == Some(index) {
                rgb(0x9c5f1c)
            } else {
                rgb(0xf6f3ec)
            })
            // Selecting a block for note/edit state must not tint the page.
            // Only an explicit annotation hover changes its background.
            .when(emphasized, |d| d.bg(rgb(0xecece6)).rounded(px(10.)))
            .when(
                self.state.stale(&b.id, &b.zh, &b.en) || note_count > 0,
                |d| {
                    let mut metadata = Vec::new();
                    if self.state.stale(&b.id, &b.zh, &b.en) {
                        metadata.push("保留修订，译者稿已更新".to_owned());
                    }
                    if note_count > 0 {
                        metadata.push(format!("{note_count} 条标记"));
                    }
                    d.child(
                        div()
                            .mb_3()
                            .text_size(px(11.))
                            .text_color(rgb(0x9c5f1c))
                            .child(metadata.join(" · ")),
                    )
                },
            )
            .child(content)
            .into_any_element();
        let row = div()
            .w_full()
            .max_w(px(row_width))
            .flex_shrink_0()
            .flex()
            .items_start()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .relative()
                    .pr(px(gutter_width))
                    .child(text)
                    .when(self.settings.committed.features.page, |d| {
                        d.child(
                            div()
                                .absolute()
                                .right(px(0.))
                                .top(gutter_top)
                                .child(self.source_page_button(index, cx)),
                        )
                    }),
            )
            .when_some(margin, |d, margin| d.child(margin));
        // Distribute spare reading width 2:1 so the left margin is wider.
        // The body and its notes remain one column group when tools are open.
        let weak = cx.entity().downgrade();
        div()
            .id(("reader-block", index))
            .relative()
            .w_full()
            .overflow_hidden()
            .flex()
            .items_start()
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let _ = weak.update(cx, |this, _| {
                            this.block_bounds.insert(index, bounds);
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(div().flex_1().flex_grow(2.))
            .child(row)
            .child(div().flex_1())
            .into_any_element()
    }

    pub(super) fn reset_rows(&mut self) {
        self.epub_reflow.invalidate();
        self.epub_reflow_displayed = (usize::MAX, usize::MAX, usize::MAX);
        self.cancel_annotation_scroll();
        self.hovered_annotation = None;
        let offset = self.list.logical_scroll_top();
        self.list.reset(self.state.paper.blocks.len());
        self.list.scroll_to(offset);
    }
}
