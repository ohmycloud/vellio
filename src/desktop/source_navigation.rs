//! Contents, source-page navigation and reading progress.
use super::*;

impl ReaderView {
    pub(super) fn load_outline(&mut self, cx: &mut Context<Self>) {
        self.pdf_outline.clear();
        self.pdf_code_html.clear();
        self.clear_paragraph_layouts();
        self.pdf_code_loading.clear();
        self.pdf_code_prepared.clear();
        self.pdf_code_cache = None;
        self.pdf_code_active = false;
        self.pdf_code_needs_plan = true;
        self.pdf_code_generation = self.pdf_code_generation.wrapping_add(1);
        let Some(ws) = self.workspace.clone() else {
            return;
        };
        let Some(rel) = self.raw["paper"]["meta"]["pdf"].as_str() else {
            return;
        };
        let root = ws.root.clone();
        let pdf = ws.asset(rel);
        let index = self.list.logical_scroll_top().item_ix;
        self.update_pdf_focus(index);
        self.request_nearby_pdf_code(index, cx);
        cx.spawn(async move |this, cx| {
            let outline = cx
                .background_executor()
                .spawn(async move { pdf.and_then(|pdf| easyread_gpui::pdf::outline(&pdf)) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.workspace.as_ref().is_some_and(|ws| ws.root == root) {
                    match outline {
                        Ok(outline) => this.pdf_outline = outline,
                        Err(e) => this.status = format!("目录加载失败：{e:#}"),
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn contents(&self) -> Vec<(usize, String, usize)> {
        let epub_outline: Vec<_> = self.raw["paper"]["outline"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let id = entry["block"].as_str()?;
                let index = self.state.paper.blocks.iter().position(|b| b.id == id)?;
                Some((
                    index,
                    entry["title"].as_str()?.to_owned(),
                    entry["depth"].as_u64().unwrap_or(0).min(6) as usize,
                ))
            })
            .collect();
        if !epub_outline.is_empty() {
            return epub_outline;
        }
        let headings: Vec<_> = self
            .state
            .paper
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| b.kind == "heading")
            .map(|(i, b)| {
                let title = self.state.text(&b.id, &b.zh, &b.en).to_owned();
                let level = self.raw["paper"]["blocks"][i]["level"].as_u64();
                let depth = heading_depth(&title, level);
                (i, title, depth)
            })
            .collect();
        if !headings.is_empty() {
            return headings;
        }
        self.pdf_outline
            .iter()
            .filter_map(|entry| {
                self.state
                    .paper
                    .blocks
                    .iter()
                    .position(|b| b.page >= entry.page)
                    .map(|i| {
                        (
                            i,
                            entry.title.clone(),
                            entry.depth.max(heading_depth(&entry.title, None)).min(6),
                        )
                    })
            })
            .collect()
    }

    pub(super) fn load_page(&mut self) {
        self.page_bounds = None;
        self.page_image = None;
        if let Some(ws) = &self.workspace
            && let Some(rel) = self
                .state
                .paper
                .meta
                .pages
                .get(self.page.saturating_sub(1))
                .and_then(|p| p["img"].as_str())
        {
            match ws.asset(rel).and_then(|p| Ok(std::fs::read(p)?)) {
                Ok(bytes) => {
                    self.page_image = Some(Arc::new(Image::from_bytes(ImageFormat::Webp, bytes)))
                }
                Err(e) => self.status = format!("原页加载失败：{e}"),
            }
        }
    }

    pub(super) fn follow_block(&mut self, index: usize) {
        let Some(block) = self.state.paper.blocks.get(index) else {
            return;
        };
        let page = self.source_page_number(index).unwrap_or(block.page.max(1));
        self.page_block = Some(index);
        if self.page != page {
            self.page = page;
            self.load_page();
        }
    }

    /// PDF uses the source layout's physical page, not the reading row number.
    /// EPUB has no physical pages: its number belongs to the published reflow.
    pub(super) fn source_page_number(&self, index: usize) -> Option<usize> {
        let block = self.state.paper.blocks.get(index)?;
        if self.is_epub() {
            if !self.epub_reflow.complete {
                return None;
            }
            // A long block can span several pages. Prefer its visible fragment
            // before looking up its first page (also avoids a full-book scan).
            if self
                .epub_reflow
                .pages
                .get(self.epub_reflow.current)
                .is_some_and(|page| page.iter().any(|fragment| fragment.block == index))
            {
                return Some(self.epub_reflow.current + 1);
            }
            self.epub_reflow
                .pages
                .iter()
                .position(|page| page.iter().any(|fragment| fragment.block == index))
                .map(|page| page + 1)
        } else {
            Some(
                self.layout
                    .region(index)
                    .map(|r| r.page)
                    .unwrap_or(block.page.max(1)),
            )
        }
    }

    pub(super) fn can_locate_source(&self, index: usize) -> bool {
        self.settings.committed.features.page
            && self.source_page_number(index).is_some_and(|page| {
                self.is_epub() || (page > 0 && page <= self.state.paper.meta.pages.len())
            })
    }

    pub(super) fn locate_source(&mut self, index: usize, cx: &mut Context<Self>) {
        if !self.can_locate_source(index) {
            return;
        }
        self.queue_margin_note();
        self.margin_editor = None;
        self.panel_open = false;
        self.selection = None;
        self.annotation_popup = None;
        self.show_pdf = true;
        self.follow_block(index);
        // Opening the source pane replaces margin notes; keep the reader's
        // logical position while remeasuring the narrower column.
        self.reset_rows();
        cx.notify();
    }

    pub(super) fn source_page_button(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let label = self
            .source_page_number(index)
            .map(|page| format!("p.{page}"))
            .unwrap_or_else(|| "p.…".into());
        div()
            .debug_selector(move || format!("source-page-gutter-{index}"))
            .child(
                Button::new(("locate-page", index))
                    .ghost()
                    .xsmall()
                    .h(px(24.))
                    .px(px(6.))
                    .rounded(px(7.))
                    .text_size(px(12.))
                    .text_color(rgb(0x969084))
                    .label(label)
                    .tooltip(if self.is_epub() {
                        "查看 EPUB 原文（页号随排版变化）"
                    } else {
                        "跳转到原 PDF 对应页面"
                    })
                    .disabled(!self.can_locate_source(index))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, _, cx| this.locate_source(index, cx))),
            )
            .into_any_element()
    }

    pub(super) fn page_view(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.is_epub() {
            let Some(index) = self.page_block else {
                return div().child("请选择要查看的 EPUB 原文").into_any_element();
            };
            return div()
                .debug_selector(|| "epub-source-content".into())
                .text_size(px(16.))
                .line_height(relative(1.7))
                .child(TextView::html(
                    ("epub-source", index),
                    self.reader_body_html(index, true),
                ))
                .into_any_element();
        }
        let Some(image) = self.page_image.clone() else {
            return div().p_3().child("没有可显示的原页").into_any_element();
        };
        let page = self.page;
        let meta = &self.state.paper.meta.pages[page - 1];
        let width = meta["w"]
            .as_f64()
            .filter(|n| n.is_finite() && *n > 0.)
            .unwrap_or(612.);
        let height = meta["h"]
            .as_f64()
            .filter(|n| n.is_finite() && *n > 0.)
            .unwrap_or(792.);
        let boxes = self
            .page_block
            .and_then(|i| self.layout.region(i))
            .filter(|r| r.page == page)
            .map(|r| r.boxes.clone())
            .unwrap_or_default();
        let weak = cx.entity().downgrade();
        div()
            .id("page-image")
            .relative()
            .w_full()
            .aspect_ratio((width / height) as f32)
            .flex_none()
            .cursor_pointer()
            .child(img(image).absolute().size_full())
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let _ = weak.update(cx, |this, _| this.page_bounds = Some((page, bounds)));
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .children(boxes.into_iter().map(|b| {
                let [x0, y0, x1, y1] = b.0;
                div()
                    .absolute()
                    .left(relative(x0))
                    .top(relative(y0))
                    .w(relative(x1 - x0))
                    .h(relative(y1 - y0))
                    .border_2()
                    .border_color(rgb(0x9c5f1c))
                    .bg(rgba(0xecc45c35))
            }))
            .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                let Some((page, bounds)) = this.page_bounds else {
                    return;
                };
                if page != this.page || bounds.size.width <= px(0.) || bounds.size.height <= px(0.)
                {
                    return;
                }
                let point = event.position();
                let x = f32::from(point.x - bounds.origin.x) / f32::from(bounds.size.width);
                let y = f32::from(point.y - bounds.origin.y) / f32::from(bounds.size.height);
                if let Some(index) = this.layout.block_at(page, x, y) {
                    // Offscreen variable-height rows still have estimates.
                    // Anchor directly by logical row rather than estimated pixels.
                    this.list.scroll_to(ListOffset {
                        item_ix: index,
                        offset_in_item: px(0.),
                    });
                    this.select(index, window, cx);
                }
            }))
            .into_any_element()
    }

    pub(super) fn reading_progress(&self) -> f32 {
        let count = self.state.paper.blocks.len();
        if count == 0 {
            return 0.;
        }
        if self.is_epub() && self.epub_reflow.complete {
            let pages = self.epub_reflow.pages.len();
            return match pages {
                0 => 0.,
                1 => 1.,
                _ => (self.epub_reflow.current as f32 / (pages - 1) as f32).clamp(0., 1.),
            };
        }
        let anchor = self.list.logical_scroll_top();
        if !self.is_epub() {
            let viewport = self.list.viewport_bounds();
            if viewport.size.height > px(0.)
                && self
                    .list
                    .bounds_for_item(count - 1)
                    .is_some_and(|last| last.bottom() <= viewport.bottom() + px(1.))
            {
                return 1.;
            }
        }
        // Unmeasured virtual-list rows have zero pixel height, so a ratio of
        // total scroll pixels would report completion long before the end.
        // Use the logical block anchor plus its measured intra-block offset.
        let fraction = self
            .list
            .bounds_for_item(anchor.item_ix)
            .filter(|bounds| bounds.size.height > px(0.))
            .map(|bounds| {
                (f32::from(anchor.offset_in_item) / f32::from(bounds.size.height)).clamp(0., 1.)
            })
            .unwrap_or(0.);
        ((anchor.item_ix as f32 + fraction) / count as f32).clamp(0., 1.)
    }

    pub(super) fn reading_progress_bar(&self, cx: &Context<Self>) -> AnyElement {
        let reader = cx.entity().downgrade();
        div()
            .id("reading-progress")
            .debug_selector(|| "reading-progress".into())
            .w_full()
            .h(px(1.854))
            .flex_shrink_0()
            .bg(rgb(0xe3ddd2))
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, cx| {
                        // Paint after the body has been measured, including
                        // EPUB reflow and restored variable-height anchors.
                        let progress = reader
                            .read_with(cx, |this, _| this.reading_progress())
                            .unwrap_or(0.);
                        if progress > 0. {
                            window.paint_quad(fill(
                                Bounds::new(
                                    bounds.origin,
                                    size(bounds.size.width * progress, bounds.size.height),
                                ),
                                rgb(0x387487),
                            ));
                        }
                    },
                )
                .size_full(),
            )
            .into_any_element()
    }

    pub(super) fn follow_epub_link(
        &mut self,
        target: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.raw["paper"]["link_targets"][target].as_str() else {
            self.status = "EPUB 链接目标未包含在阅读内容中".into();
            cx.notify();
            return;
        };
        let Some(index) = self.state.paper.blocks.iter().position(|b| b.id == id) else {
            return;
        };
        self.list.scroll_to(ListOffset {
            item_ix: index,
            offset_in_item: px(0.),
        });
        self.select(index, window, cx);
        cx.notify();
    }

    pub(super) fn follow_pdf_link(&mut self, url: String, window: &Window, cx: &mut Context<Self>) {
        let Some(ws) = self.workspace.as_ref() else {
            return;
        };
        let Some(rel) = self.raw["paper"]["meta"]["pdf"].as_str() else {
            return;
        };
        let Ok(pdf) = ws.asset(rel) else {
            return;
        };
        let root = ws.root.clone();
        cx.spawn_in(window, async move |this, cx| {
            let target = cx.background_executor().spawn(async move {
                easyread_gpui::pdf::link_destination(&pdf, &url)
            }).await;
            let _ = cx.update(|window, cx| {
                let _ = this.update(cx, |this, cx| {
                    if !this.workspace.as_ref().is_some_and(|ws| ws.root == root) { return; }
                    let Ok((page, quote)) = target else { this.status = "无法读取 PDF 链接目标".into(); cx.notify(); return; };
                    let Some(index) = this.state.paper.blocks.iter().position(|b| b.page == page && b.source_kind == "pdf-text") else { return; };
                    if quote.is_empty() {
                        this.list.scroll_to(ListOffset { item_ix: index, offset_in_item: px(0.) });
                        this.select(index, window, cx);
                    } else {
                        let anchor = this.state.paper.blocks[index].id.clone();
                        this.jump_to_annotation(json!({"id":"reader-navigation-target", "anchor":anchor, "quote":quote, "pdf_figure":quote.starts_with("Figure ") || quote.starts_with("Table ")}), window, cx);
                    }
                });
            });
        }).detach();
    }
}
