//! Chat resize.
use super::*;

impl ReaderView {
    pub(in crate::desktop) fn finish_chat_resize(&mut self, cx: &mut Context<Self>) {
        self.chat_ui.resizing = false;
        if let Some(library) = &self.library
            && let Err(error) = self.settings.committed.save(&library.root)
        {
            self.status = format!("面板宽度保存失败：{error:#}");
        }
        self.reset_rows();
        cx.notify();
    }

    pub(in crate::desktop) fn chat_column(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let viewport = f32::from(window.viewport_size().width);
        let width = self.chat_panel_width(viewport);
        self.chat_ui.model_menu_width = (width - 24.).clamp(180., 370.);
        self.chat_ui.style_menu_width = (width - 24.).clamp(180., 500.);
        let closing = self.chat_ui.closing;
        let panel = self.chat(cx);
        let resize_view = cx.entity().downgrade();
        // Keep the transition clock in ReaderView: remounting popup / message
        // children must not restart the whole panel's slide animation.
        let progress = self
            .chat_ui
            .slide_started
            .map(|start| (start.elapsed().as_secs_f32() / (SLIDE_MS as f32 / 1000.)).clamp(0., 1.))
            .unwrap_or(1.);
        if progress < 1. && !cx.reduce_motion() {
            window.request_animation_frame();
        }
        let eased = if cx.reduce_motion() {
            1.
        } else {
            slide_ease(progress)
        };
        let offset = width * 1.02 * if closing { eased } else { 1. - eased };
        div()
            .id("chat-column")
            .debug_selector(|| "chat-column".into())
            .h_full()
            .flex_shrink_0()
            .relative()
            .when(viewport >= 1180., |d| d.w(px(width)).overflow_hidden())
            .when(viewport < 1180., |d| {
                d.absolute()
                    .right_0()
                    .top_0()
                    .bottom_0()
                    .w(px(width))
                    .overflow_hidden()
            })
            .child(
                div()
                    .id("chat-panel")
                    .debug_selector(|| "chat-panel".into())
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .w(px(width))
                    .border_l_1()
                    .border_color(rgb(0xe3ddd2))
                    .left(px(offset))
                    .child(panel),
            )
            .when(viewport >= 1180. && !closing, |d| {
                d.child(
                    div()
                        .id("chat-panel-grip")
                        .debug_selector(|| "chat-panel-grip".into())
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(px(6.))
                        .cursor(CursorStyle::ResizeLeftRight)
                        .hover(|d| d.bg(rgba(0x38748722)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, event: &MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                if event.click_count == 2 {
                                    this.settings.committed.extra.remove("side_panel_width");
                                    this.finish_chat_resize(cx);
                                } else {
                                    this.chat_ui.resizing = true;
                                }
                            }),
                        ),
                )
            })
            .child(
                canvas(
                    |_, _, _| {},
                    move |_, _, window, _cx| {
                        let moving = resize_view.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase == DispatchPhase::Capture {
                                let _ = moving.update(cx, |this, cx| {
                                    if this.chat_ui.resizing
                                        && event.pressed_button == Some(MouseButton::Left)
                                    {
                                        let viewport = f32::from(window.viewport_size().width);
                                        let width = (viewport - f32::from(event.position.x))
                                            .clamp(320., (viewport - 420.).max(320.));
                                        this.settings
                                            .committed
                                            .extra
                                            .insert("side_panel_width".into(), json!(width));
                                        cx.notify();
                                    }
                                });
                            }
                        });
                        let release = resize_view.clone();
                        window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture {
                                let _ = release.update(cx, |this, cx| {
                                    if this.chat_ui.resizing {
                                        this.finish_chat_resize(cx);
                                    }
                                });
                            }
                        });
                    },
                )
                .absolute()
                .size_0(),
            )
            .into_any_element()
    }
}
