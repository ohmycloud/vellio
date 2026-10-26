//! Reader text composition and retained rich-text views.
use super::*;

impl ReaderView {
    pub(super) fn decorate_reader_html(&self, index: usize, source: String) -> String {
        let Some(block) = self.state.paper.blocks.get(index) else {
            return source;
        };
        let notes: Vec<_> = self.raw["reader"]["notes"]
            .as_object()
            .map(|notes| {
                notes
                    .values()
                    .filter(|note| note["anchor"] == block.id)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let source = annotations::decorate_interactive(&source, &notes);
        let source = self
            .margin_editor
            .as_ref()
            .filter(|e| e.selection.block == index)
            .map(|e| annotations::margin_editor_anchor(&source, &e.note.record))
            .unwrap_or(source);
        self.navigation_note
            .as_ref()
            .filter(|note| note["anchor"] == block.id)
            .map(|note| annotations::navigation_anchor(&source, note))
            .unwrap_or(source)
    }

    pub(super) fn reader_html_view(
        &mut self,
        index: usize,
        original: bool,
        source: String,
        cx: &mut Context<Self>,
    ) -> Entity<TextViewState> {
        let source = easyread_gpui::typst_table::prepare_html(&source).unwrap_or(source);
        if let Some((old, view)) = self.text_views.get_mut(&(index, original)) {
            if *old != source {
                // Decoration-only updates keep selection; actual replacements
                // remain asynchronous, including while warming an offscreen page.
                view.update(cx, |state, cx| state.set_html_decorations(&source, cx));
                *old = source;
            }
            return view.clone();
        }
        let view = cx.new(|cx| {
            let mut state = TextViewState::html(&source, cx);
            state.set_markdown_extensions(
                Arc::new(
                    gpui_kit::base::text::MarkdownExtensions::default()
                        .plugin(math::MathPlugin {
                            max_width: self.appearance.text_width(),
                        })
                        .plugin(math::TablePlugin {
                            max_width: self.appearance.text_width(),
                        }),
                ),
                cx,
            );
            state
        });
        self.observe_reader_view(&view, index, original, cx);
        self.text_views
            .insert((index, original), (source, view.clone()));
        view
    }

    pub(super) fn reader_figure_caption(&self, index: usize) -> bool {
        self.state
            .paper
            .blocks
            .get(index)
            .is_some_and(|block| block.kind == "figure")
            || self.raw["paper"]["blocks"]
                .as_array()
                .is_some_and(|blocks| {
                    easyread_gpui::formats::epub::is_figure_caption(blocks, index)
                })
    }

    // Layout source excludes selection/navigation/highlight decorations. Those
    // change hit targets and colors, not the book's text-flow revision.
    pub(super) fn reader_layout_html(&self, index: usize, original: bool) -> String {
        if original == self.primary_original()
            && let Some(expansion) = self.paragraph_languages.get(&index)
        {
            return expansion.layout.render(&expansion.expanded);
        }
        let source = self.reader_language_layout_html(index, original);
        if original != self.primary_original() && self.show_secondary(index) {
            easyread_gpui::paragraph_translation::without_shared_content(&source)
        } else {
            source
        }
    }

    pub(super) fn reader_language_layout_html(&self, index: usize, original: bool) -> String {
        let Some(block) = self.state.paper.blocks.get(index) else {
            return String::new();
        };
        // Resolve the source language independently: no reader edit or saved
        // translation layout may enter the original-language rendering path.
        let source = if original {
            self.pdf_code_html
                .get(&index)
                .filter(|(source, _)| source == &block.en)
                .and_then(|(_, presentation)| presentation.as_ref())
                .map(|presentation| presentation.html.clone())
                .or_else(|| {
                    easyread_gpui::formats::epub::source_html(
                        &self.raw["paper"]["blocks"][index],
                        block,
                        &self.state,
                        true,
                    )
                    .map(|html| code::epub_html(html, self.epub_code_language))
                })
                .unwrap_or_else(|| {
                    formula::native_markdown_html(&self.state.original_markdown(block))
                })
        } else {
            let active = self.state.text(&block.id, &block.zh, &block.en);
            let key = if matches!(block.kind.as_str(), "figure" | "table") {
                format!("{}#caption", block.id)
            } else {
                block.id.clone()
            };
            let active = if matches!(block.kind.as_str(), "figure" | "table") {
                self.state.caption(block)
            } else {
                active
            };
            let source_text = if matches!(block.kind.as_str(), "figure" | "table") {
                &block.caption_en
            } else {
                &block.en
            };
            easyread_gpui::paragraph_edit::edited_html(
                &self.raw["reader"]["edits"][&key],
                source_text,
                active,
            )
            .map(str::to_owned)
            .or_else(|| {
                easyread_gpui::translation_layout::saved_html(
                    &self.raw["paper"]["blocks"][index],
                    active,
                )
            })
            .or_else(|| {
                self.pdf_code_html
                    .get(&index)
                    .filter(|(source, _)| source == &block.en)
                    .and_then(|(_, presentation)| presentation.as_ref())
                    .and_then(|presentation| {
                        if active == block.en {
                            Some(presentation.html.clone())
                        } else if block.source_kind == "pdf-text" && active == block.zh {
                            easyread_gpui::translation_layout::legacy_html(
                                &block.en,
                                active,
                                &presentation.html,
                                block.page,
                            )
                        } else {
                            None
                        }
                    })
            })
            .or_else(|| {
                easyread_gpui::formats::epub::source_html(
                    &self.raw["paper"]["blocks"][index],
                    block,
                    &self.state,
                    false,
                )
                .map(|html| code::epub_html(html, self.epub_code_language))
            })
            .unwrap_or_else(|| formula::native_markdown_html(&self.state.markdown(block, false)))
        };
        // Keep alignment on the paragraph itself, including translated/edited
        // captions. The native inline flow then uses centered row geometry for
        // glyphs, selection, links and annotations, not just centered painting.
        if self.reader_figure_caption(index) {
            source.replace("<p>", "<p style=\"text-align: center\">")
        } else {
            source
        }
    }

    pub(super) fn reader_body_html(&self, index: usize, original: bool) -> String {
        self.decorate_reader_html(index, self.reader_layout_html(index, original))
    }

    pub(super) fn observe_reader_view(
        &self,
        view: &Entity<TextViewState>,
        index: usize,
        original: bool,
        cx: &mut Context<Self>,
    ) {
        let mut measured_height = None;
        let mut ready_source = None;
        cx.observe(view, move |this, view, cx| {
            let Some((source, current)) = this.text_views.get(&(index, original)) else {
                return;
            };
            if current != &view {
                return;
            }
            let state = view.read(cx);
            let height = state.bounds().size.height;
            let height_changed = height > px(0.) && measured_height != Some(height);
            // A parser result can be ready without having painted yet. Dirty
            // the cached row then, not only after its height changes, otherwise
            // a loading row may never render the newly prepared TextView.
            let content_changed = state.is_content_ready() && ready_source.as_ref() != Some(source);
            if height_changed {
                measured_height = Some(height);
            }
            if content_changed {
                ready_source = Some(source.clone());
            }
            if height_changed || content_changed {
                this.list.remeasure_items(index..index + 1);
                // A parser's first ready/paint notification can arrive AFTER
                // offscreen pagination already consumed that exact source.
                // Only a genuinely different measured revision invalidates it;
                // first display and page-position changes must reuse the map.
                if content_changed && this.is_epub() {
                    let layout_source = this.reader_layout_html(index, original);
                    if !this
                        .epub_reflow
                        .matches_measured_source(index, original, &layout_source)
                    {
                        this.epub_reflow.invalidate_from(index);
                    }
                }
            }
            cx.notify();
        })
        .detach();
    }

    pub(super) fn reader_text(
        &self,
        view: &Entity<TextViewState>,
        original: bool,
        caption: bool,
        english: bool,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let secondary = original != self.primary_original() && self.has_original(index);
        let source_pending = self.pdf_code_pending(index)
            && (original
                || self.state.paper.blocks.get(index).is_some_and(|block| {
                    self.state.text(&block.id, &block.zh, &block.en) == block.en
                }));
        if source_pending || !view.read(cx).is_content_ready() {
            return self.reader_text_loading(index, original, cx);
        }
        // ReaderViews can also be opened through import/recovery/test paths.
        // Do not rely solely on the outer application's one-time setup.
        code::ensure_highlighter(cx);
        let weak = cx.entity().downgrade();
        let text_size = self.appearance.font_size * typography::text_scale(secondary, caption);
        let family = self
            .fonts
            .reading(self.appearance.reading_font(), original || english);
        view.update(cx, |state, cx| {
            state.set_track_paragraphs(true, cx);
            state.set_track_page_breaks(self.is_epub());
        });
        let measured_width = f32::from(view.read(cx).bounds().size.width);
        let math_width = self.appearance.text_width().min(if measured_width > 0. {
            measured_width
        } else {
            self.appearance.text_width()
        });
        let text = gpui_kit::base::TextView::new(view)
            .markdown_extensions(gpui_kit::base::text::MarkdownExtensions::default()
                .plugin(math::MathPlugin { max_width: math_width })
                .plugin(math::TablePlugin { max_width: math_width }))
            .selectable(true)
            .style(typography::reader_rich_style(self, index, original, family.clone(), text_size, cx))
            .font_family(family)
            .font_weight(FontWeight::NORMAL)
            .text_color(rgb(if secondary || caption { typography::READER_MUTED_INK } else { typography::READER_INK }))
            .text_size(px(text_size))
            .line_height(relative(if secondary {
                typography::SECONDARY_LINE_HEIGHT
            } else if caption {
                typography::CAPTION_LINE_HEIGHT
            } else if self.state.paper.blocks.get(index).is_some_and(|b| b.kind == "heading") {
                1.5
            } else {
                self.appearance.line_spacing
            }))
            .when(self.reader_figure_caption(index), |text| {
                text.text_center()
            })
            .on_link_click(move |url, event, window, cx| {
                if let Some(id) = url.strip_prefix("easyread-annotation:") {
                    if id == "reader-navigation-target" || id == annotations::MARGIN_EDITOR_ANCHOR {
                        return;
                    }
                    if let ClickEvent::Mouse(click) = event
                        && click.up.button == MouseButton::Left
                    {
                        let _ = weak.update(cx, |this, cx| {
                            if !this.raw["reader"]["notes"][id]["deleted"]
                                .as_bool()
                                .unwrap_or(false)
                            {
                                this.selection = None;
                                this.annotation_popup = Some((id.to_owned(), click.up.position));
                                this.follow_block(index);
                                cx.notify();
                            }
                        });
                    }
                } else if let Some(id) = url.strip_prefix("easyread-image:") {
                    if matches!(event, ClickEvent::Keyboard(_)) || matches!(event, ClickEvent::Mouse(click) if click.up.button == MouseButton::Left) {
                        let _ = weak.update(cx, |this, cx| {
                            let figure = this.pdf_code_html.get(&index)
                                .and_then(|(_, p)| p.as_ref())
                                .and_then(|p| p.figures.iter().find(|f| f.id == id));
                            if let Some(figure) = figure {
                                let image = Arc::new(Image::from_bytes(ImageFormat::Png, figure.png.clone()));
                                let aspect = figure.width as f32 / figure.height as f32;
                                this.open_image_preview(image, aspect, figure.width, window, cx);
                            }
                        });
                    }
                } else if let Some(target) = url.strip_prefix("vellio-epub:") {
                    if matches!(event, ClickEvent::Keyboard(_)) || matches!(event, ClickEvent::Mouse(click) if click.up.button == MouseButton::Left) {
                        let _ = weak.update(cx, |this, cx| {
                            this.follow_epub_link(target, window, cx);
                        });
                    }
                } else if url.starts_with("easyread-pdf:") {
                    if matches!(event, ClickEvent::Keyboard(_)) || matches!(event, ClickEvent::Mouse(click) if click.up.button == MouseButton::Left) {
                        let _ = weak.update(cx, |this, cx| this.follow_pdf_link(url.to_string(), window, cx));
                    }
                } else if let ClickEvent::Mouse(click) = event
                    && matches!(click.up.button, MouseButton::Left | MouseButton::Middle)
                {
                    cx.open_url(url);
                } else if matches!(event, ClickEvent::Keyboard(_)) {
                    cx.open_url(url);
                }
            });
        let indicator_view = view.clone();
        let indicator_app = cx.entity().downgrade();
        let state = view.read(cx);
        let preview = self.hovered_note_card.as_ref().and_then(|id| {
            let note = self
                .margin_records
                .get(id)
                .or_else(|| self.raw["reader"]["notes"].get(id))?;
            if note["anchor"] != self.state.paper.blocks[index].id {
                return None;
            }
            let bounds = if note["quote"].as_str().unwrap_or("").is_empty()
                && original == self.primary_original()
            {
                state.bounds()
            } else {
                let offset = state
                    .annotation_offset(id)
                    .or_else(|| state.annotation_offset(note["source_annotation"].as_str()?))?;
                let paragraph = state.paragraph_at(point(
                    state.bounds().left() + px(4.),
                    state.bounds().top() + offset + px(2.),
                ))?;
                state.paragraph_bounds(paragraph)?
            };
            Some((
                (bounds.top() - state.bounds().top()).max(px(0.)),
                bounds.size.height,
            ))
        });
        let active = self
            .active_reading_paragraph
            .filter(|(block, is_original, _)| *block == index && *is_original == original)
            .and_then(|(_, _, paragraph)| state.paragraph_bounds(paragraph))
            .map(|bounds| {
                (
                    (bounds.top() - state.bounds().top()).max(px(0.)),
                    bounds.size.height,
                )
            });
        div()
            .id(("reading-paragraph", view.entity_id()))
            .flex_shrink_0()
            .debug_selector(|| "reader-text-ready".into())
            .on_mouse_down(MouseButton::Left, move |event, _, cx| {
                if let Some(paragraph) = indicator_view.read(cx).paragraph_at(event.position) {
                    let _ = indicator_app.update(cx, |this, cx| {
                        this.active_reading_paragraph = Some((index, original, paragraph));
                        if original == this.primary_original() && !this.show_secondary(index) {
                            // Single-language clicks need the same shared-math
                            // policy as default bilingual layouts, before the
                            // floating toolbar decides whether En is enabled.
                            this.ensure_paragraph_layout(index);
                        }
                        cx.notify();
                    });
                }
            })
            .relative()
            .w_full()
            .when_some(preview, |d, (top, height)| {
                d.child(
                    div()
                        .debug_selector(|| "annotation-paragraph-preview".into())
                        .absolute()
                        .left(px(-8.))
                        .right(px(-8.))
                        .top(top - px(4.))
                        .h(height + px(8.))
                        .rounded(px(5.))
                        .bg(rgb(0xecece6)),
                )
            })
            .child(text)
            .when_some(active, |d, (top, height)| {
                d.child(
                    div()
                        .absolute()
                        .left(px(-16.))
                        .top(top)
                        .h(height)
                        .w(px(3.))
                        .rounded(px(2.))
                        .bg(rgb(0x88a3ab)),
                )
            })
            .into_any_element()
    }

    pub(super) fn has_original(&self, index: usize) -> bool {
        let block = &self.state.paper.blocks[index];
        match block.kind.as_str() {
            "figure" | "table" => {
                !block.caption_en.trim().is_empty()
                    && self.state.text(
                        &format!("{}#caption", block.id),
                        &block.caption_zh,
                        &block.caption_en,
                    ) != block.caption_en
            }
            "list" => block.items.iter().enumerate().any(|(i, item)| {
                !item.en.trim().is_empty()
                    && self
                        .state
                        .text(&format!("{}#{i}", block.id), &item.zh, &item.en)
                        != item.en
            }),
            _ => {
                !block.en.trim().is_empty()
                    && self.state.text(&block.id, &block.zh, &block.en) != block.en
            }
        }
    }

    pub(super) fn reader_text_loading(
        &self,
        index: usize,
        original: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let font = self.appearance.font_size;
        let chars_per_line = (self.appearance.text_width() / (font * 0.52)).max(1.);
        let characters = self
            .state
            .paper
            .blocks
            .get(index)
            .map_or(0, |b| b.en.chars().count());
        let estimate =
            ((characters as f32 / chars_per_line).ceil() * font * self.appearance.line_spacing
                + 64.)
                .clamp(160., 3000.);
        let height = self
            .text_views
            .get(&(index, original))
            .map(|(_, view)| view.read(cx).bounds().size.height)
            .filter(|height| *height > px(0.))
            .unwrap_or(px(estimate));
        div()
            .id(("reader-text-loading", index * 2 + usize::from(original)))
            .debug_selector(|| "reader-text-loading".into())
            .w_full()
            .h(height)
            .text_size(px(13.))
            .text_color(rgb(0x969084))
            .child("正在准备本页排版……")
            .into_any_element()
    }
}
