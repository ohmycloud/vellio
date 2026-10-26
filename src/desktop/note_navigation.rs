//! Annotation anchors and animated scrolling.
use super::*;

impl ReaderView {
    pub(super) fn all_notes(&self) -> Vec<Value> {
        let mut notes = annotation_panel::notes(&self.raw["reader"], &self.margin_records);
        // Legacy EasyRead questions derive their status from attached replies.
        // Preserve an explicit manual Unanswered status in the native reader.
        for note in &mut notes {
            if note["kind"] == "question"
                && !note["answered"].is_boolean()
                && self.raw["discussion"]["entries"]
                    .as_array()
                    .is_some_and(|entries| {
                        entries.iter().any(|entry| {
                            entry["reply_to"] == note["id"]
                                && entry["deleted"] != true
                                && !entry["body"].as_str().unwrap_or("").trim().is_empty()
                        })
                    })
            {
                note["answered"] = json!(true);
            }
        }
        notes
    }

    pub(super) fn note_offset(&self, note: &Value, index: usize, cx: &Context<Self>) -> Pixels {
        let origin = self
            .text_views
            .get(&(index, self.primary_original()))
            .map(|(_, view)| view.read(cx).bounds().origin.y)
            .unwrap_or(px(0.));
        self.annotation_y(note, index, cx)
            .map(|y| (y - origin).max(px(0.)))
            .unwrap_or(px(0.))
    }

    pub(super) fn annotation_y(
        &self,
        note: &Value,
        index: usize,
        cx: &Context<Self>,
    ) -> Option<Pixels> {
        for original in self.rendered_languages(index) {
            if let Some((_, view)) = self.text_views.get(&(index, original)) {
                let view = view.read(cx);
                let editor_anchor = self
                    .margin_editor
                    .as_ref()
                    .filter(|editor| editor.note.record["id"] == note["id"])
                    .map(|_| annotations::MARGIN_EDITOR_ANCHOR);
                for id in [
                    editor_anchor,
                    note["id"].as_str(),
                    note["source_annotation"].as_str(),
                ]
                .into_iter()
                .flatten()
                {
                    if let Some(offset) = view.annotation_offset(id) {
                        let y = view.bounds().origin.y + offset;
                        if note["pdf_figure"] == true {
                            let position = point(view.bounds().origin.x + px(4.), y + px(2.));
                            if let Some(previous) = view
                                .paragraph_at(position)
                                .and_then(|i| i.checked_sub(1))
                                .and_then(|i| view.paragraph_bounds(i))
                            {
                                return Some(previous.origin.y);
                            }
                        }
                        return Some(y);
                    }
                }
            }
        }
        None
    }

    pub(super) fn jump_to_annotation(
        &mut self,
        note: Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self
            .state
            .paper
            .blocks
            .iter()
            .position(|block| block.id == note["anchor"])
        else {
            return;
        };
        let offset = self
            .annotation_y(&note, index, cx)
            .zip(self.block_bounds.get(&index))
            .map(|(y, bounds)| (y - bounds.origin.y - px(80.)).max(px(0.)))
            .unwrap_or(px(0.));
        let hovered = self
            .hovered_annotation
            .clone()
            .filter(|(_, block)| *block == index);
        self.select(index, window, cx);
        self.appearance_open = false;
        self.navigation = None;
        // Reveal an original-language quote when it is absent from the translated text.
        let block = &self.state.paper.blocks[index];
        let quote = note["quote"].as_str().unwrap_or("");
        let matches = |markdown: String| {
            annotations::quote_range(
                &annotations::plain_text(&annotations::html(&markdown)),
                quote,
                note["prefix"].as_str().unwrap_or(""),
                note["suffix"].as_str().unwrap_or(""),
            )
            .is_some()
        };
        if !quote.is_empty() {
            let in_original = matches(self.state.original_markdown(block));
            let in_translation = matches(self.state.markdown(block, false));
            if (self.primary_original() && !in_original && in_translation)
                || (!self.primary_original() && !in_translation && in_original)
            {
                self.paragraph_languages.remove(&index);
                self.original_blocks.insert(index, true);
            }
        }
        self.reset_rows();
        // The jump rebuilds rows but does not end the bar's actual hover.
        // Keep its preview until the pointer leaves; never retain click focus.
        self.hovered_annotation = hovered;
        self.navigation_note = Some(note.clone());
        window.blur(cx);
        self.start_annotation_scroll(
            index,
            offset,
            Duration::from_millis(380),
            Some(note),
            window,
            cx,
        );
    }

    pub(super) fn cancel_annotation_scroll(&mut self) {
        self.annotation_jump_generation = self.annotation_jump_generation.wrapping_add(1);
        self.annotation_scroll = None;
        self.navigation_note = None;
    }

    pub(super) fn start_annotation_scroll(
        &mut self,
        index: usize,
        offset: Pixels,
        duration: Duration,
        refine: Option<Value>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = self.state.paper.blocks.len();
        if count == 0 {
            return;
        }
        let current = self.list.logical_scroll_top();
        let heights = self
            .state
            .paper
            .blocks
            .iter()
            .enumerate()
            .map(|(i, block)| {
                self.block_bounds
                    .get(&i)
                    .map(|bounds| f32::from(bounds.size.height))
                    .unwrap_or_else(|| {
                        // Unpainted rows have no geometry. Use prose estimates until
                        // they enter the viewport, then refine the destination below.
                        let text = self.state.markdown(block, false);
                        let lines: f32 = text
                            .lines()
                            .map(|line| {
                                let chars: f32 = line
                                    .chars()
                                    .map(|c| if c.is_ascii() { 0.5 } else { 1. })
                                    .sum();
                                (chars / self.appearance.line_width).ceil().max(1.)
                            })
                            .sum();
                        56. + lines * self.appearance.font_size * self.appearance.line_spacing
                            + if block.kind == "figure" {
                                self.appearance.text_width() * 0.65
                            } else {
                                0.
                            }
                    })
                    .max(1.)
            })
            .collect();
        self.annotation_scroll = Some(AnnotationScroll {
            path: ScrollPath::new(
                heights,
                ScrollPosition {
                    block: current.item_ix.min(count - 1),
                    offset: f32::from(current.offset_in_item).max(0.),
                },
                ScrollPosition {
                    block: index,
                    offset: f32::from(offset).max(0.),
                },
            ),
            started: Instant::now(),
            duration,
            generation: self.annotation_jump_generation,
            refine,
        });
        self.schedule_annotation_frame(window, cx);
        cx.notify();
    }

    pub(super) fn schedule_annotation_frame(&self, window: &mut Window, cx: &Context<Self>) {
        let weak = cx.entity().downgrade();
        let generation = self.annotation_jump_generation;
        window.on_next_frame(move |window, cx| {
            let _ = weak.update(cx, |this, cx| {
                if this.annotation_jump_generation == generation {
                    this.tick_annotation_scroll(window, cx);
                }
            });
        });
    }

    pub(super) fn tick_annotation_scroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(motion) = self.annotation_scroll.take() else {
            return;
        };
        let progress = motion.started.elapsed().as_secs_f32() / motion.duration.as_secs_f32();
        let position = motion.path.sample(progress);
        self.list.scroll_to(ListOffset {
            item_ix: position.block,
            offset_in_item: px(position.offset),
        });
        if progress < 1. {
            self.annotation_scroll = Some(motion);
            self.schedule_annotation_frame(window, cx);
        } else if let Some(note) = motion.refine {
            self.refine_annotation_scroll(note, motion.generation, window, cx);
        } else {
            self.capture_position();
        }
        cx.notify();
    }

    pub(super) fn refine_annotation_scroll(
        &self,
        note: Value,
        generation: usize,
        window: &mut Window,
        cx: &Context<Self>,
    ) {
        let weak = cx.entity().downgrade();
        // Virtualized rows have no current quote geometry until they have painted.
        window.on_next_frame(move |window, _| {
            window.on_next_frame(move |window, cx| {
                let _ = weak.update(cx, |this, cx| {
                    if this.annotation_jump_generation != generation {
                        return;
                    }
                    let index = this.selected;
                    let target =
                        json!({"id":"reader-navigation-target", "pdf_figure":note["pdf_figure"]});
                    let offset = this
                        .annotation_y(&target, index, cx)
                        .or_else(|| this.annotation_y(&note, index, cx))
                        .zip(this.block_bounds.get(&index))
                        .map(|(y, bounds)| (y - bounds.origin.y - px(80.)).max(px(0.)))
                        .unwrap_or(px(0.));
                    // The geometry anchor is only needed for this jump; restore
                    // all of the source annotation's normal click behavior.
                    this.navigation_note = None;
                    this.start_annotation_scroll(
                        index,
                        offset,
                        Duration::from_millis(160),
                        None,
                        window,
                        cx,
                    );
                });
            });
        });
    }

    pub(super) fn annotation_jump_bar(
        &self,
        note: &Value,
        short: bool,
        cx: &mut Context<Self>,
    ) -> Button {
        let record = note.clone();
        let bar_id = format!(
            "annotation-jump-{}-{}",
            if short { "quote" } else { "card" },
            note["id"].as_str().unwrap_or("")
        );
        let hover_id = bar_id.clone();
        let index = self
            .state
            .paper
            .blocks
            .iter()
            .position(|block| block.id == note["anchor"]);
        Button::new(SharedString::from(bar_id))
            // Text buttons keep normal, hover and pressed backgrounds transparent.
            .text()
            .absolute()
            .left(px(-5.))
            .top_0()
            .h_full()
            .w(px(12.))
            .p_0()
            .rounded_none()
            .accessibility_label(if short {
                "Go to highlighted passage"
            } else {
                "Go to annotation passage"
            })
            .disabled(index.is_none())
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                if *hovered {
                    this.hovered_annotation = index.map(|index| (hover_id.clone(), index));
                } else if this
                    .hovered_annotation
                    .as_ref()
                    .is_some_and(|(id, _)| *id == hover_id)
                {
                    this.hovered_annotation = None;
                }
                cx.notify();
            }))
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.jump_to_annotation(record.clone(), window, cx);
            }))
    }
}
