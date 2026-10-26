//! Demand-driven PDF presentation and image loading.
use super::*;

impl ReaderView {
    pub(super) fn request_block_image(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(ws) = self.workspace.clone() else {
            return;
        };
        if self.block_images.contains_key(&index) || !self.image_loading.insert(index) {
            return;
        }
        let source = self.raw["paper"]["blocks"][index]["src"]
            .as_str()
            .unwrap_or("")
            .to_owned();
        let source_kind = self.state.paper.blocks[index].source_kind.clone();
        let root = ws.root.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let load = || -> anyhow::Result<_> {
                        let path = ws.asset(&source)?;
                        let bytes = easyread_gpui::figure_image::display_bytes(
                            &source_kind,
                            std::fs::read(path)?,
                        )?;
                        let format = match image::guess_format(&bytes)? {
                            image::ImageFormat::Png => ImageFormat::Png,
                            image::ImageFormat::WebP => ImageFormat::Webp,
                            image::ImageFormat::Jpeg => ImageFormat::Jpeg,
                            image::ImageFormat::Gif => ImageFormat::Gif,
                            image::ImageFormat::Bmp => ImageFormat::Bmp,
                            image::ImageFormat::Tiff => ImageFormat::Tiff,
                            _ => anyhow::bail!("不支持的图片格式"),
                        };
                        let (w, h) = image::ImageReader::new(std::io::Cursor::new(&bytes))
                            .with_guessed_format()?
                            .into_dimensions()?;
                        anyhow::ensure!(w > 0 && h > 0, "图片尺寸无效");
                        Ok((
                            Arc::new(Image::from_bytes(format, bytes)),
                            w as f32 / h as f32,
                            w,
                        ))
                    };
                    load().map_err(|e| format!("{e:#}"))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.workspace.as_ref().is_some_and(|ws| ws.root == root) {
                    this.block_images.insert(index, result);
                    this.image_loading.remove(&index);
                    this.list.remeasure_items(index..index + 1);
                    this.epub_reflow.invalidate_from(index);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn warm_pdf_text(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.reading_mode == ReadingMode::Bilingual {
            self.ensure_paragraph_layout(index);
        }
        let Some(block) = self.state.paper.blocks.get(index) else {
            return;
        };
        let recovered = self
            .pdf_code_html
            .get(&index)
            .is_some_and(|(source, _)| source == &block.en);
        let primary_pdf = self.state.text(&block.id, &block.zh, &block.en) == block.en;
        // Warm only visible sources. Translations need not wait for PDF recovery.
        for original in self.rendered_languages(index) {
            if recovered || (!original && !primary_pdf) {
                let source = self.reader_body_html(index, original);
                self.reader_html_view(index, original, source, cx);
            }
        }
    }

    pub(super) fn pdf_code_pending(&self, index: usize) -> bool {
        (self.pdf_code_cache.is_some() || self.pdf_code_loading.contains(&index))
            && self.state.paper.blocks.get(index).is_some_and(|block| {
                block.source_kind == "pdf-text"
                    && block.kind == "para"
                    && !self
                        .pdf_code_html
                        .get(&index)
                        .is_some_and(|(source, _)| source == &block.en)
            })
    }

    pub(super) fn update_pdf_focus(&mut self, index: usize) {
        let focus = self
            .state
            .paper
            .blocks
            .get(index)
            .filter(|b| b.source_kind != "pdf-text" || b.kind != "para")
            .and_then(|block| {
                self.state.paper.blocks.iter().position(|b| {
                    b.page == block.page && b.source_kind == "pdf-text" && b.kind == "para"
                })
            })
            .unwrap_or(index);
        if focus != self.pdf_code_focus {
            self.pdf_code_forward = focus > self.pdf_code_focus;
            self.pdf_code_focus = focus;
            self.pdf_code_needs_plan = true;
        }
    }

    pub(super) fn nearby_pdf_indices(&self) -> Vec<usize> {
        let eligible: Vec<_> = self
            .state
            .paper
            .blocks
            .iter()
            .enumerate()
            .filter_map(|(i, b)| (b.source_kind == "pdf-text" && b.kind == "para").then_some(i))
            .collect();
        pdf_layout_cache::lookahead(&eligible, self.pdf_code_focus, self.pdf_code_forward)
    }

    pub(super) fn request_nearby_pdf_code(&mut self, index: usize, cx: &mut Context<Self>) {
        // Do not inspect ListState here: its render callback holds its mutable
        // borrow. main_view updates focus before entering the list renderer.
        if self
            .rendered_languages(index)
            .iter()
            .any(|original| !self.text_views.contains_key(&(index, *original)))
        {
            self.warm_pdf_text(index, cx);
        }
        if self.pdf_code_needs_plan {
            for nearby in self.nearby_pdf_indices() {
                if self
                    .rendered_languages(nearby)
                    .iter()
                    .any(|original| !self.text_views.contains_key(&(nearby, *original)))
                {
                    self.warm_pdf_text(nearby, cx);
                }
            }
        }
        self.pump_pdf_code(cx);
    }

    pub(super) fn pump_pdf_code(&mut self, cx: &mut Context<Self>) {
        if self.pdf_code_active || !self.pdf_code_needs_plan {
            return;
        }
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
        if self.pdf_code_cache.is_none() {
            let Ok(cache) = PresentationCache::new(&pdf, &root.join(".native-pdf-layout-cache"))
            else {
                return;
            };
            self.pdf_code_cache = Some(cache);
        }
        let eligible: Vec<_> = self
            .state
            .paper
            .blocks
            .iter()
            .enumerate()
            .filter_map(|(i, b)| (b.source_kind == "pdf-text" && b.kind == "para").then_some(i))
            .collect();
        let ready = self
            .pdf_code_html
            .iter()
            .filter_map(|(i, (source, _))| {
                self.state
                    .paper
                    .blocks
                    .get(*i)
                    .is_some_and(|b| &b.en == source)
                    .then_some(*i)
            })
            .collect();
        let prepared = self
            .pdf_code_prepared
            .iter()
            .filter_map(|(i, source)| {
                self.state
                    .paper
                    .blocks
                    .get(*i)
                    .is_some_and(|b| &b.en == source)
                    .then_some(*i)
            })
            .collect();
        let batch = pdf_layout_cache::next_batch(
            &eligible,
            self.pdf_code_focus,
            self.pdf_code_forward,
            &ready,
            &prepared,
        );
        self.pdf_code_needs_plan = false;
        if batch.is_empty() {
            return;
        }
        let requests: Vec<_> = batch
            .into_iter()
            .map(|i| {
                let b = &self.state.paper.blocks[i];
                self.pdf_code_loading.insert(i);
                (i, b.id.clone(), b.page, b.en.clone())
            })
            .collect();
        let nearby = self.nearby_pdf_indices();
        let inputs: Vec<_> = requests
            .iter()
            .map(|(index, _, page, source)| {
                (
                    *page,
                    source.clone(),
                    nearby.contains(index) || self.pdf_code_html.contains_key(index),
                )
            })
            .collect();
        let cache = self.pdf_code_cache.clone().unwrap();
        let generation = self.pdf_code_generation;
        self.pdf_code_active = true;
        cx.spawn(async move |this, cx| {
            let results = cx
                .background_executor()
                .spawn(async move { cache.prepare_selective(&pdf, &inputs) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if generation != this.pdf_code_generation
                    || !this.workspace.as_ref().is_some_and(|ws| ws.root == root)
                {
                    return;
                }
                this.pdf_code_active = false;
                let nearby = this.nearby_pdf_indices();
                let mut visible_changed = false;
                for ((index, id, page, source), result) in requests.into_iter().zip(results) {
                    this.pdf_code_loading.remove(&index);
                    if !this
                        .state
                        .paper
                        .blocks
                        .get(index)
                        .is_some_and(|b| b.id == id && b.page == page && b.en == source)
                    {
                        continue;
                    }
                    this.pdf_code_prepared.insert(index, source.clone());
                    let presentation = match result {
                        Ok(pdf_layout_cache::PreparedPage::OnDisk) => continue,
                        Ok(pdf_layout_cache::PreparedPage::Loaded(presentation)) => presentation,
                        Err(_) => None,
                    };
                    // Prepare the whole book on disk, but do not retain every
                    // page's raster previews/AST in memory. Keep the lookahead
                    // window and pages already read, as before.
                    if nearby.contains(&index) || this.pdf_code_html.contains_key(&index) {
                        this.pdf_code_html.insert(index, (source, presentation));
                        this.paragraph_languages.remove(&index);
                        this.unpaired_paragraphs.remove(&index);
                        this.warm_pdf_text(index, cx);
                        let end = this
                            .state
                            .paper
                            .blocks
                            .iter()
                            .rposition(|b| b.page == page)
                            .map_or(index + 1, |i| i + 1);
                        this.list.remeasure_items(index..end);
                        visible_changed = true;
                    }
                }
                if visible_changed {
                    cx.notify();
                }
                // Continue through the book while idle. A jump/scroll changes
                // focus before the next batch, rather than queuing PDF waiters.
                this.pdf_code_needs_plan = true;
                this.pump_pdf_code(cx);
            });
        })
        .detach();
    }
}
