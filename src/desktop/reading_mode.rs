//! Reading language policy shared by rendering, PDF warmup and EPUB pagination.
use super::*;
use easyread_gpui::paragraph_translation::PairLayout;
use std::collections::HashSet;

pub(super) struct ParagraphExpansion {
    pub layout: PairLayout,
    pub expanded: HashSet<usize>,
}

impl ReaderView {
    pub(super) fn primary_original(&self) -> bool {
        match self.reading_mode {
            ReadingMode::Original => true,
            ReadingMode::Translation => false,
            ReadingMode::Bilingual => self.original_first,
        }
    }

    pub(super) fn show_secondary(&self, index: usize) -> bool {
        self.original_blocks
            .get(&index)
            .copied()
            .unwrap_or(self.reading_mode == ReadingMode::Bilingual)
    }

    pub(super) fn visible_languages(&self, index: usize) -> Vec<bool> {
        // Standalone formulas have one shared rendering, not two languages.
        if self
            .state
            .paper
            .blocks
            .get(index)
            .is_some_and(|b| b.kind == "math")
        {
            return vec![false];
        }
        let primary = self.primary_original();
        if self.show_secondary(index) && self.has_original(index) {
            vec![primary, !primary]
        } else {
            vec![primary]
        }
    }

    pub(super) fn rendered_languages(&self, index: usize) -> Vec<bool> {
        if self.paragraph_languages.contains_key(&index) {
            // Both local and default bilingual companions share the primary
            // flow, including when every companion has been collapsed.
            vec![self.primary_original()]
        } else {
            self.visible_languages(index)
        }
    }

    pub(super) fn clear_paragraph_layouts(&mut self) {
        self.paragraph_languages.clear();
        self.unpaired_paragraphs.clear();
    }

    pub(super) fn ensure_paragraph_layout(&mut self, index: usize) {
        if self.state.paper.blocks.get(index).is_none()
            || self.paragraph_languages.contains_key(&index)
            || self.unpaired_paragraphs.contains(&index)
            || self.pdf_code_pending(index)
            || !self.has_original(index)
        {
            return;
        }
        let source = self.reader_language_layout_html(index, true);
        let translation = self.reader_language_layout_html(index, false);
        let block = &self.state.paper.blocks[index];
        let page = (block.source_kind == "pdf-text"
            && block
                .en
                .lines()
                .last()
                .is_some_and(|line| line.trim() == block.page.to_string()))
        .then_some(block.page);
        let layout = if self.reading_mode == ReadingMode::Bilingual {
            PairLayout::for_bilingual(&source, &translation, self.primary_original(), page)
        } else if let Some(page) = page {
            PairLayout::for_pdf_page(&source, &translation, self.primary_original(), page)
        } else {
            PairLayout::new(&source, &translation, self.primary_original())
        };
        if let Some(layout) = layout {
            let expanded = if self.show_secondary(index) {
                layout.all_pairs()
            } else {
                HashSet::new()
            };
            self.paragraph_languages
                .insert(index, ParagraphExpansion { layout, expanded });
        } else {
            // Complex/ambiguous pages keep the conservative block fallback;
            // don't repeatedly parse their image-heavy HTML on every frame.
            self.unpaired_paragraphs.insert(index);
        }
    }

    pub(super) fn selection_is_display_equation(&self, index: usize) -> bool {
        self.active_reading_paragraph
            .is_some_and(|(block, original, paragraph)| {
                block == index
                    && original == self.primary_original()
                    && self
                        .paragraph_languages
                        .get(&index)
                        .is_some_and(|p| p.layout.is_equation_at(paragraph, &p.expanded))
            })
    }

    pub(super) fn selection_can_toggle_language(&self, index: usize) -> bool {
        self.state.paper.blocks[index].kind != "math"
            && self.has_original(index)
            && !self.selection_is_display_equation(index)
    }

    pub(super) fn selection_secondary_visible(&self, index: usize) -> bool {
        if self.selection_is_display_equation(index) {
            return false;
        }
        if let Some((block, original, paragraph)) = self.active_reading_paragraph
            && block == index
            && original == self.primary_original()
            && let Some(expansion) = self.paragraph_languages.get(&index)
            && let Some(pair) = expansion.layout.pair_at(paragraph, &expansion.expanded)
        {
            return expansion.expanded.contains(&pair);
        }
        self.show_secondary(index)
    }

    pub(super) fn set_reading_mode(&mut self, mode: ReadingMode, cx: &mut Context<Self>) {
        self.reading_mode = mode;
        self.reading_order_open = false;
        self.appearance_open = false;
        self.navigation = None;
        self.original_blocks.clear();
        self.clear_paragraph_layouts();
        self.active_reading_paragraph = None;
        self.appearance_changed(cx);
        self.flush_session();
    }

    pub(super) fn toggle_bilingual(&mut self, cx: &mut Context<Self>) {
        let mode = if self.reading_mode == ReadingMode::Bilingual {
            ReadingMode::Original
        } else {
            ReadingMode::Bilingual
        };
        self.set_reading_mode(mode, cx);
    }

    pub(super) fn toggle_block_language(&mut self, index: usize, cx: &mut Context<Self>) {
        if !self.selection_can_toggle_language(index) {
            return;
        }
        if (self.paragraph_languages.contains_key(&index) || !self.show_secondary(index))
            && let Some((block, original, paragraph)) = self.active_reading_paragraph
            && block == index
            && original == self.primary_original()
        {
            self.ensure_paragraph_layout(index);
            if let Some(expansion) = self.paragraph_languages.get_mut(&index)
                && let Some(pair) = expansion.layout.pair_at(paragraph, &expansion.expanded)
            {
                if !expansion.expanded.remove(&pair) {
                    expansion.expanded.insert(pair);
                }
                self.reset_rows();
                cx.notify();
                return;
            }
        }
        self.paragraph_languages.remove(&index);
        self.original_blocks
            .insert(index, !self.show_secondary(index));
        self.reset_rows();
        cx.notify();
    }

    pub(super) fn reading_mode_control(&self, cx: &mut Context<Self>) -> AnyElement {
        let active = self.reading_mode;
        let weak = cx.entity().downgrade();
        let mut control = div()
            .id("reading-mode-control")
            .flex()
            .flex_shrink_0()
            .items_center()
            .p(px(3.))
            .gap(px(2.))
            .rounded(px(12.))
            .bg(rgb(0xeee9df))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        for (id, label, mode) in [
            ("reading-mode-original", "原文", ReadingMode::Original),
            ("reading-mode-translation", "译文", ReadingMode::Translation),
            ("reading-mode-bilingual", "双语", ReadingMode::Bilingual),
        ] {
            control = control.child(
                div().debug_selector(move || id.into()).child(
                    Button::new(id)
                        .ghost()
                        .small()
                        .label(label)
                        .accessibility_label(label)
                        .h(px(30.))
                        .px(px(10.))
                        .rounded(px(9.))
                        .text_color(rgb(if mode == active { 0x23211d } else { 0x969084 }))
                        .when(mode == active, |b| b.bg(rgb(0xfbfaf7)))
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.set_reading_mode(mode, cx)),
                        ),
                ),
            );
        }
        control
            .child(
                div()
                    .relative()
                    .child(
                        canvas(
                            move |bounds, _, cx| {
                                let _ = weak.update(cx, |this, _| {
                                    this.reading_order_button_bounds = Some(bounds)
                                });
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
                    .child(
                        Button::new("reading-order-toggle")
                            .ghost()
                            .small()
                            .h(px(30.))
                            .w(px(26.))
                            .p_0()
                            .rounded(px(9.))
                            .icon(Icon::default().path("icons/chevron-down.svg").size(px(14.)))
                            .accessibility_label("双语显示顺序")
                            .when(self.reading_order_open, |b| b.bg(rgb(0xfbfaf7)))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.reading_order_open = !this.reading_order_open;
                                this.appearance_open = false;
                                this.navigation = None;
                                this.selection = None;
                                this.annotation_popup = None;
                                this.reader_focus.focus(window, cx);
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn reading_order_menu(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.reading_order_open {
            return None;
        }
        let width = px(180.);
        let left = self
            .reading_order_button_bounds
            .map(|b| b.right() - width)
            .unwrap_or(px(12.))
            .clamp(
                px(8.),
                (window.viewport_size().width - width - px(8.)).max(px(8.)),
            );
        let weak = cx.entity().downgrade();
        Some(
            div()
                .id("reading-order-menu")
                .debug_selector(|| "reading-order-menu".into())
                .occlude()
                .absolute()
                .left(left)
                .top(px(54.))
                .w(width)
                .p(px(6.))
                .flex()
                .flex_col()
                .gap(px(4.))
                .bg(rgb(0xfbfaf7))
                .border_1()
                .border_color(rgb(0xe2dccf))
                .rounded(px(12.))
                .shadow(vec![
                    BoxShadow::new(px(0.), px(3.), rgba(0x52452c20).into()).blur_radius(px(12.)),
                ])
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    canvas(
                        move |bounds, _, cx| {
                            let _ = weak.update(cx, |this, _| {
                                this.reading_order_menu_bounds = Some(bounds)
                            });
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                .children(
                    [
                        (true, "原文在前", "reading-order-original-first"),
                        (false, "译文在前", "reading-order-translation-first"),
                    ]
                    .into_iter()
                    .map(|(original_first, label, id)| {
                        let selected = self.original_first == original_first;
                        div().debug_selector(move || id.into()).child(
                            Button::new(id)
                                .ghost()
                                .w_full()
                                .h(px(38.))
                                .rounded(px(8.))
                                .accessibility_label(label)
                                .child(
                                    div()
                                        .w_full()
                                        .flex()
                                        .items_center()
                                        .gap(px(12.))
                                        .child(div().w(px(18.)).text_color(rgb(0x326879)).when(
                                            selected,
                                            |d| {
                                                d.child(
                                                    Icon::default()
                                                        .path("icons/check.svg")
                                                        .size(px(16.)),
                                                )
                                            },
                                        ))
                                        .child(label),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.original_first = original_first;
                                    this.set_reading_mode(ReadingMode::Bilingual, cx);
                                })),
                        )
                    }),
                )
                .into_any_element(),
        )
    }
}
