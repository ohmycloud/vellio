//! Native EPUB pages. Measure the same rich elements we paint, then clip at
//! actual row boundaries. Source HTML, selection offsets and annotation IDs
//! are never rewritten or split into synthetic paragraphs.
use super::*;
use easyread_gpui::epub_pagination::{self, MeasuredBlock, Page};

#[derive(Default)]
pub(super) struct ReflowState {
    pub size: Option<Size<Pixels>>,
    pub bounds: Option<Bounds<Pixels>>,
    wheel_at: Option<Instant>,
    measured: Vec<MeasuredBlock>,
    // Record the text-flow sources consumed by measurement, excluding
    // highlight/navigation decorations. First paint is not a new revision.
    measured_sources: HashMap<(usize, bool), String>,
    preview_pages: Vec<Page>,
    // Published only after the entire layout has been measured.
    pub pages: Vec<Page>,
    pub current: usize,
    pub complete: bool,
    dirty: bool,
    #[cfg(test)]
    pub measured_blocks: usize,
    #[cfg(test)]
    pub page_map_builds: usize,
}
impl ReflowState {
    pub fn invalidate(&mut self) {
        self.measured.clear();
        self.measured_sources.clear();
        self.preview_pages.clear();
        self.pages.clear();
        self.complete = false;
        self.dirty = true;
    }
    pub fn matches_measured_source(&self, index: usize, original: bool, source: &str) -> bool {
        self.measured_sources
            .get(&(index, original))
            .is_some_and(|hash| *hash == text_hash(source))
    }

    pub fn invalidate_from(&mut self, index: usize) {
        if index < self.measured.len() {
            self.measured.truncate(index);
            self.measured_sources.retain(|(block, _), _| *block < index);
            self.preview_pages.clear();
            self.pages.clear();
            self.complete = false;
            self.dirty = true;
        }
    }
}

impl ReaderView {
    pub(super) fn is_epub(&self) -> bool {
        self.raw["paper"]["meta"]["format"] == "epub"
    }

    pub(super) fn turn_epub_page(&mut self, forward: bool, cx: &mut Context<Self>) {
        // Only a complete, published snapshot is navigable. Turning pages must
        // not drive pagination or expose a moving partial total.
        if !self.epub_reflow.complete {
            return;
        }
        let next = if forward {
            self.epub_reflow.current + 1
        } else {
            self.epub_reflow.current.saturating_sub(1)
        };
        let Some(first) = self
            .epub_reflow
            .pages
            .get(next)
            .and_then(|p| p.first())
            .cloned()
        else {
            return;
        };
        self.queue_margin_note();
        self.margin_editor = None;
        self.cancel_annotation_scroll();
        self.selection = None;
        self.annotation_popup = None;
        self.epub_reflow.current = next;
        self.list.scroll_to(ListOffset {
            item_ix: first.block,
            offset_in_item: px(first.start),
        });
        self.capture_position();
        cx.notify();
    }

    pub(super) fn epub_wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        if !self
            .epub_reflow
            .bounds
            .is_some_and(|bounds| bounds.contains(&event.position))
            || self.selection.is_some()
            || self.image_preview.is_some()
        {
            return;
        }
        let delta = event.delta.pixel_delta(px(self.appearance.font_size));
        let movement = if delta.y.abs() > delta.x.abs() {
            delta.y
        } else {
            delta.x
        };
        if movement.abs() < px(2.) {
            return;
        }
        let now = Instant::now();
        let fresh = self
            .epub_reflow
            .wheel_at
            .is_none_or(|last| now.duration_since(last) > Duration::from_millis(220));
        self.epub_reflow.wheel_at = Some(now);
        // A continuous trackpad gesture (including inertia) turns only once.
        if fresh {
            self.turn_epub_page(movement < px(0.), cx);
        }
    }

    pub(super) fn epub_page_label(&self) -> String {
        if !self.epub_reflow.complete || self.epub_reflow.pages.is_empty() {
            "正在排版……".to_owned()
        } else {
            format!(
                "{} / {}",
                self.epub_reflow.current + 1,
                self.epub_reflow.pages.len()
            )
        }
    }

    pub(super) fn epub_pages_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let page = self.epub_reflow.current;
        let count = self.epub_reflow.pages.len();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(NativePages {
                        reader: cx.entity().downgrade(),
                    }),
            )
            .child(
                div()
                    .h(px(44.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap_4()
                    .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        Button::new("epub-previous-page")
                            .ghost()
                            .small()
                            .label("上一页")
                            .disabled(!self.epub_reflow.complete || count == 0 || page == 0)
                            .on_click(cx.listener(|this, _, _, cx| this.turn_epub_page(false, cx))),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgb(0x969084))
                            .child(self.epub_page_label()),
                    )
                    .child(
                        Button::new("epub-next-page")
                            .ghost()
                            .small()
                            .label("下一页")
                            .disabled(!self.epub_reflow.complete || count == 0 || page + 1 >= count)
                            .on_click(cx.listener(|this, _, _, cx| this.turn_epub_page(true, cx))),
                    ),
            )
            .into_any_element()
    }
}

struct NativePages {
    reader: WeakEntity<ReaderView>,
}
struct PaintedFragment {
    element: AnyElement,
    mask: ContentMask<Pixels>,
}
impl IntoElement for NativePages {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for NativePages {
    type RequestLayoutState = ();
    type PrepaintState = Vec<PaintedFragment>;
    fn id(&self) -> Option<ElementId> {
        Some("epub-native-pages".into())
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (
            window.request_layout(
                Style {
                    size: size(relative(1.).into(), relative(1.).into()),
                    ..Default::default()
                },
                [],
                cx,
            ),
            (),
        )
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<PaintedFragment> {
        if bounds.size.width <= px(0.) || bounds.size.height <= px(0.) {
            return Vec::new();
        }
        let _ = self.reader.update(cx, |this, _| {
            this.epub_reflow.bounds = Some(bounds);
            if this.epub_reflow.size != Some(bounds.size) {
                this.epub_reflow.size = Some(bounds.size);
                this.epub_reflow.invalidate();
                this.epub_reflow_displayed = (usize::MAX, usize::MAX, usize::MAX);
            }
        });
        let available = size(
            AvailableSpace::Definite(bounds.size.width),
            AvailableSpace::MinContent,
        );
        // Empty mask makes measurement noninteractive. Use a nearby origin
        // rather than a huge negative coordinate to retain subpixel precision.
        let origin = bounds.origin;
        let empty_mask = ContentMask {
            bounds: Bounds::new(origin, size(px(0.), px(0.))),
        };
        // Release ReaderView's entity borrow before layout/prepaint: descendants
        // report their geometry back to that entity via canvas callbacks.
        for _ in 0..12 {
            let prepared = self
                .reader
                .update(cx, |this, cx| {
                    let index = this.epub_reflow.measured.len();
                    if index >= this.state.paper.blocks.len() {
                        return None;
                    }
                    let block = this.block(index, cx);
                    let originals = this.rendered_languages(index);
                    let pending_text = originals.iter().any(|original| {
                        this.text_views
                            .get(&(index, *original))
                            .is_some_and(|(_, view)| !view.read(cx).is_content_ready())
                    });
                    let pending_image = this.image_loading.contains(&index);
                    if pending_text || pending_image {
                        return None;
                    }
                    Some((
                        index,
                        originals,
                        div()
                            .id(("epub-measure", index))
                            .w_full()
                            .child(block)
                            .into_any_element(),
                    ))
                })
                .ok()
                .flatten();
            let Some((index, originals, mut element)) = prepared else {
                break;
            };
            let measured = element.layout_as_root(available, window, cx);
            window.with_content_mask(Some(empty_mask), |window| {
                element.prepaint_at(origin, window, cx);
            });
            let _ = self.reader.update(cx, |this, cx| {
                let mut breaks = Vec::new();
                for original in originals {
                    if let Some((_, view)) = this.text_views.get(&(index, original)) {
                        let layout_source = this.reader_layout_html(index, original);
                        this.epub_reflow
                            .measured_sources
                            .insert((index, original), text_hash(&layout_source));
                        breaks.extend(
                            view.read(cx)
                                .page_break_positions()
                                .into_iter()
                                .map(|cut| f32::from(cut - origin.y)),
                        );
                    }
                }
                let block = &this.state.paper.blocks[index];
                let atomic = matches!(block.kind.as_str(), "figure" | "math");
                if atomic {
                    breaks.clear();
                }
                this.epub_reflow.dirty = true;
                #[cfg(test)]
                {
                    this.epub_reflow.measured_blocks += 1;
                }
                this.epub_reflow.measured.push(MeasuredBlock {
                    height: f32::from(measured.height),
                    breaks,
                    chapter: this.raw["paper"]["blocks"][index]["chapter"]
                        .as_u64()
                        .unwrap_or(1) as usize,
                    heading: block.kind == "heading" && !this.reader_figure_caption(index),
                    atomic,
                });
            });
        }
        let page = self
            .reader
            .update(cx, |this, cx| {
                this.epub_reflow.complete =
                    this.epub_reflow.measured.len() == this.state.paper.blocks.len();
                if this.epub_reflow.dirty {
                    #[cfg(test)]
                    {
                        this.epub_reflow.page_map_builds += 1;
                    }
                    let pages = epub_pagination::paginate(
                        &this.epub_reflow.measured,
                        f32::from(bounds.size.height),
                    );
                    if this.epub_reflow.complete {
                        this.epub_reflow.pages = pages;
                        this.epub_reflow.preview_pages.clear();
                    } else {
                        this.epub_reflow.preview_pages = pages;
                    }
                    this.epub_reflow.dirty = false;
                }
                let anchor = this.list.logical_scroll_top();
                let active_pages = if this.epub_reflow.complete {
                    &this.epub_reflow.pages
                } else {
                    &this.epub_reflow.preview_pages
                };
                this.epub_reflow.current = epub_pagination::page_for_anchor(
                    active_pages,
                    anchor.item_ix,
                    f32::from(anchor.offset_in_item),
                )
                .unwrap_or(0);
                this.block_bounds.clear();
                let count = this.epub_reflow.measured.len();
                let displayed = (
                    count,
                    this.epub_reflow.current,
                    this.epub_reflow.pages.len(),
                );
                if this.epub_reflow_displayed != displayed {
                    // Parser/image completions already notify their observers.
                    // Don't spin the frame loop while waiting for background work.
                    this.epub_reflow_displayed = displayed;
                    let weak = cx.entity().downgrade();
                    window.on_next_frame(move |_, cx| {
                        let _ = weak.update(cx, |_, cx| cx.notify());
                    });
                }
                // Never flash the beginning while a restored/TOC anchor is pending.
                if count > anchor.item_ix {
                    active_pages
                        .get(this.epub_reflow.current)
                        .cloned()
                        .unwrap_or_default()
                } else {
                    Vec::new()
                }
            })
            .unwrap_or_default();
        let mut painted = Vec::new();
        let mut y = bounds.top();
        for fragment in page {
            let height = px(fragment.end - fragment.start);
            let mask = ContentMask {
                bounds: Bounds::new(point(bounds.left(), y), size(bounds.size.width, height)),
            };
            let Ok(block) = self
                .reader
                .update(cx, |this, cx| this.block(fragment.block, cx))
            else {
                break;
            };
            let mut element = div()
                .id(("epub-fragment", fragment.block))
                .w_full()
                .child(block)
                .into_any_element();
            element.layout_as_root(available, window, cx);
            window.with_content_mask(Some(mask), |window| {
                element.prepaint_at(point(bounds.left(), y - px(fragment.start)), window, cx);
            });
            painted.push(PaintedFragment { element, mask });
            y += height;
        }
        painted
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        fragments: &mut Vec<PaintedFragment>,
        window: &mut Window,
        cx: &mut App,
    ) {
        for fragment in fragments {
            window.with_content_mask(Some(fragment.mask), |window| {
                fragment.element.paint(window, cx)
            });
        }
    }
}
