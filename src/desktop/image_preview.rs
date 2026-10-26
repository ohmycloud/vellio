//! Zoomable reader image previews.
use super::*;

impl ReaderView {
    pub(super) fn open_image_preview(
        &mut self,
        image: Arc<Image>,
        aspect: f32,
        width: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selection = None;
        self.annotation_popup = None;
        self.appearance_open = false;
        self.navigation = None;
        TextSelection::clear(window, cx);
        self.reader_focus.focus(window, cx);
        self.image_preview = Some(ImagePreview {
            image,
            aspect,
            width,
            zoom: 1.,
            scroll: ScrollHandle::new(),
        });
        cx.notify();
    }

    pub(super) fn image_preview_view(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let preview = self.image_preview.as_ref()?;
        let available = window.viewport_size();
        let viewport_width = (f32::from(available.width) - 96.).max(1.);
        let viewport_height = (f32::from(available.height) - 144.).max(1.);
        let fit_width = viewport_width
            .min(viewport_height * preview.aspect)
            .min(preview.width as f32);
        let width = fit_width * preview.zoom;
        let height = width / preview.aspect;
        Some(
            div()
                .id("image-preview-overlay")
                .absolute()
                .inset_0()
                .size_full()
                .bg(rgba(0x19171399))
                .p(px(24.))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.image_preview = None;
                    cx.notify();
                }))
                .child(
                    div()
                        .id("image-preview")
                        .size_full()
                        .flex()
                        .flex_col()
                        .min_h_0()
                        .bg(rgb(0xf6f3ec))
                        .rounded(px(10.))
                        .overflow_hidden()
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .p(px(12.))
                                .child(div().flex_1().text_size(px(14.)).child("图片预览"))
                                .child(
                                    Button::new("image-zoom-out")
                                        .ghost()
                                        .label("−")
                                        .accessibility_label("缩小图片")
                                        .disabled(preview.zoom <= 1.)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if let Some(p) = this.image_preview.as_mut() {
                                                p.zoom = (p.zoom / 1.5).max(1.);
                                            }
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("image-zoom-fit")
                                        .ghost()
                                        .label(format!("{:.0}%", preview.zoom * 100.))
                                        .accessibility_label("适合窗口")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if let Some(p) = this.image_preview.as_mut() {
                                                p.zoom = 1.;
                                                p.scroll.set_offset(point(px(0.), px(0.)));
                                            }
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("image-zoom-in")
                                        .ghost()
                                        .label("+")
                                        .accessibility_label("放大图片")
                                        .disabled(preview.zoom >= 4.)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if let Some(p) = this.image_preview.as_mut() {
                                                p.zoom = (p.zoom * 1.5).min(4.);
                                            }
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("image-preview-close")
                                        .ghost()
                                        .label("×")
                                        .accessibility_label("关闭图片预览")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.image_preview = None;
                                            this.reader_focus.focus(window, cx);
                                            cx.notify();
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .id("image-preview-scroll")
                                .flex_1()
                                .min_h_0()
                                .w_full()
                                .overflow_scroll()
                                .track_scroll(&preview.scroll)
                                .child(
                                    div()
                                        .flex()
                                        .justify_center()
                                        .items_center()
                                        .min_w_full()
                                        .min_h_full()
                                        .w(px(width + 48.))
                                        .h(px(height + 48.))
                                        .child(
                                            img(preview.image.clone()).w(px(width)).h(px(height)),
                                        ),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }
}
