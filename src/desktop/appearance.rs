//! Reader appearance controls.
use super::*;

impl ReaderView {
    pub(super) fn appearance_changed(&mut self, cx: &mut Context<Self>) {
        self.selection = None;
        self.annotation_popup = None;
        self.margin_card_heights.clear();
        self.reset_rows();
        self.capture_position();
        cx.notify();
    }

    pub(super) fn appearance_menu(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.appearance_open {
            return None;
        }
        let width = px(320.).min((window.viewport_size().width - px(24.)).max(px(240.)));
        let weak = cx.entity().downgrade();
        let slider_row = |label: &'static str, slider: &Entity<SliderState>, value: String| {
            div()
                .flex()
                .items_center()
                .gap(px(12.))
                .h(px(48.))
                .child(
                    div()
                        .w(px(56.))
                        .flex_shrink_0()
                        .text_color(rgb(0x969084))
                        .child(label),
                )
                .child(
                    div().flex_1().min_w_0().child(
                        Slider::new(slider)
                            .accessibility_label(label)
                            .bg(rgb(0x326879))
                            .text_color(rgb(0x326879)),
                    ),
                )
                .child(div().w(px(60.)).flex_shrink_0().text_right().child(value))
        };
        let choices = div().flex().flex_col().gap(px(8.)).child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .h(px(44.))
                .child(
                    div()
                        .w(px(64.))
                        .text_color(rgb(0x969084))
                        .child("Margin\nnotes"),
                )
                .child(
                    div()
                        .flex()
                        .p(px(2.))
                        .rounded(px(10.))
                        .bg(rgb(0xeee9df))
                        .children(["Show", "Collapse"].into_iter().enumerate().map(
                            |(option, text)| {
                                let active = self.appearance.margin_notes == (option == 0);
                                Button::new(("appearance-choice", 4 + option))
                                    .ghost()
                                    .small()
                                    .h(px(30.))
                                    .px(px(11.))
                                    .rounded(px(8.))
                                    .text_color(rgb(if active { 0x23211d } else { 0x969084 }))
                                    .when(active, |b| {
                                        b.bg(rgb(0xfbfaf7)).shadow(vec![
                                            BoxShadow::new(px(0.), px(1.), rgba(0x52452c16).into())
                                                .blur_radius(px(2.)),
                                        ])
                                    })
                                    .label(text)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.appearance.margin_notes = option == 0;
                                        this.appearance_changed(cx);
                                        this.flush_session();
                                    }))
                            },
                        )),
                ),
        );
        Some(
            div()
                .id("appearance-menu")
                .occlude()
                .relative()
                .track_focus(&self.appearance_focus)
                .absolute()
                .right(px(12.))
                .top(px(62.))
                .w(width)
                .p(px(16.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .bg(rgb(0xfbfaf7))
                .border_1()
                .border_color(rgb(0xe2dccf))
                .rounded(px(14.))
                .text_size(px(13.))
                .text_color(rgb(0x5f5a51))
                .shadow(vec![
                    BoxShadow::new(px(0.), px(3.), rgba(0x52452c20).into()).blur_radius(px(14.)),
                ])
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    canvas(
                        move |bounds, _, cx| {
                            let _ = weak
                                .update(cx, |this, _| this.appearance_menu_bounds = Some(bounds));
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                .child(slider_row(
                    "Font\nsize",
                    &self.font_slider,
                    format!("{:.0} px", self.appearance.font_size),
                ))
                .child(slider_row(
                    "Line\nwidth",
                    &self.width_slider,
                    format!("{:.0} chars", self.appearance.line_width),
                ))
                .child(slider_row(
                    "Line\nspacing",
                    &self.spacing_slider,
                    format!("{:.1}", self.appearance.line_spacing),
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .py(px(6.))
                        .child(div().text_color(rgb(0x969084)).child("字体"))
                        .child(
                            div()
                                .flex()
                                .p(px(2.))
                                .rounded(px(10.))
                                .bg(rgb(0xeee9df))
                                .children(ReadingFont::ALL.into_iter().enumerate().map(
                                    |(i, font)| {
                                        let active = self.appearance.reading_font() == font;
                                        Button::new(("reading-font", i))
                                            .ghost()
                                            .small()
                                            .flex_1()
                                            .h(px(30.))
                                            .px(px(6.))
                                            .rounded(px(8.))
                                            .text_color(rgb(if active {
                                                0x23211d
                                            } else {
                                                0x969084
                                            }))
                                            .when(active, |b| {
                                                b.bg(rgb(0xfbfaf7)).shadow(vec![
                                                    BoxShadow::new(
                                                        px(0.),
                                                        px(1.),
                                                        rgba(0x52452c16).into(),
                                                    )
                                                    .blur_radius(px(2.)),
                                                ])
                                            })
                                            .label(font.label())
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.appearance.set_font(font);
                                                this.appearance_changed(cx);
                                                this.flush_session();
                                            }))
                                    },
                                )),
                        )
                        .child(div().text_size(px(11.)).text_color(rgb(0x969084)).child(
                            match self.appearance.reading_font() {
                                ReadingFont::NotoSerifSc => "Noto Serif SC / 思源宋体 · 默认",
                                ReadingFont::SystemSerif => "系统中文宋体 / 英文衬线字体",
                                ReadingFont::SansSerif => "系统无衬线字体",
                            },
                        )),
                )
                .child(choices)
                .child(
                    div()
                        .mt(px(6.))
                        .pt(px(8.))
                        .border_t_1()
                        .border_color(rgb(0xe3ddd2))
                        .flex()
                        .items_center()
                        .child(
                            Button::new("reset-appearance")
                                .ghost()
                                .small()
                                .px_0()
                                .label("Reset")
                                .text_color(rgb(0x326879))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.appearance = Appearance::default();
                                    this.reading_mode = ReadingMode::Original;
                                    this.original_first = true;
                                    this.original_blocks.clear();
                                    this.clear_paragraph_layouts();
                                    this.appearance_changed(cx);
                                    this.flush_session();
                                })),
                        )
                        .child(div().flex_1())
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(0x969084))
                                .child("Esc to close"),
                        ),
                )
                .into_any_element(),
        )
    }
}
