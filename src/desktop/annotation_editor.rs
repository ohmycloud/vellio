//! Editing saved annotations in the floating editor.
use super::*;

impl ReaderView {
    pub(super) fn edit_annotation(
        &mut self,
        action: &'static str,
        color: HighlightColor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((id, position)) = self.annotation_popup.clone() else {
            return;
        };
        let mut note = self.raw["reader"]["notes"][&id].clone();
        if !note.is_object() || note["deleted"].as_bool().unwrap_or(false) {
            return;
        }
        if action == "remove" {
            self.annotation_popup = None;
            self.save_ops(vec![json!({"op":"note_del","id":id,"at":now()})], cx);
        } else if matches!(action, "note" | "question") {
            let Some(block) = self
                .state
                .paper
                .blocks
                .iter()
                .position(|b| b.id == note["anchor"])
            else {
                return;
            };
            self.selection = Some(Selection {
                block,
                key: note["key"]
                    .as_str()
                    .unwrap_or(&self.state.paper.blocks[block].id)
                    .to_owned(),
                quote: note["quote"].as_str().unwrap_or("").to_owned(),
                prefix: note["prefix"].as_str().unwrap_or("").to_owned(),
                suffix: note["suffix"].as_str().unwrap_or("").to_owned(),
                position,
                underline: note["style"] == "underline",
            });
            self.selection_action(action, color, window, cx);
        } else {
            if action == "color" {
                note["color"] = json!(color.name());
            } else if action == "underline" {
                note["style"] = json!("underline");
            } else {
                note.as_object_mut().unwrap().remove("style");
            }
            note["updated"] = json!(now());
            self.save_ops(vec![json!({"op":"note","note":note})], cx);
        }
        cx.notify();
    }

    pub(super) fn annotation_editor(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (id, position) = self.annotation_popup.as_ref()?;
        let note = &self.raw["reader"]["notes"][id];
        if !note.is_object() || note["deleted"].as_bool().unwrap_or(false) {
            return None;
        }
        let underline = note["style"] == "underline";
        let color = HighlightColor::from_name(note["color"].as_str().unwrap_or("yellow"));
        let locked = self.busy || self.workspace.is_none();
        let viewport = window.viewport_size();
        let width = px(360.).min(viewport.width - px(16.));
        let left = (position.x - width / 2.)
            .max(px(8.))
            .min((viewport.width - width - px(8.)).max(px(8.)));
        let top = if position.y > px(166.) {
            position.y - px(156.)
        } else {
            position.y + px(18.)
        };
        let mode_button = |action: &'static str, label: &'static str, icon: &'static str| {
            Button::new(SharedString::from(format!("annotation-{action}")))
                .ghost()
                .w(px(30.))
                .h(px(30.))
                .p_0()
                .rounded(px(8.))
                .accessibility_label(label)
                .tooltip(label)
                .disabled(locked)
                .text_color(rgb(0x928b7d))
                .when((action == "underline") == underline, |b| {
                    b.bg(rgb(0xfbfaf7)).text_color(rgb(0x5f5a51)).shadow(vec![
                        BoxShadow::new(px(0.), px(1.), rgba(0x52452c12).into()).blur_radius(px(3.)),
                    ])
                })
                .child(Icon::default().path(icon).size(px(17.)))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.edit_annotation(action, color, window, cx)
                }))
        };
        Some(
            deferred(
                div()
                    .id("annotation-editor")
                    .absolute()
                    .left(left)
                    .top(
                        top.max(px(8.))
                            .min((viewport.height - px(152.)).max(px(8.))),
                    )
                    .w(width)
                    .p(px(14.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .bg(rgb(0xfbfaf7))
                    .border_1()
                    .border_color(rgb(0xe2dccf))
                    .rounded(px(14.))
                    .shadow(vec![
                        BoxShadow::new(px(0.), px(3.), rgba(0x52452c16).into())
                            .blur_radius(px(12.)),
                    ])
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgb(0x928b7d))
                            .child("My highlight"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex()
                                    .p(px(2.))
                                    .bg(rgb(0xeee9df))
                                    .rounded(px(10.))
                                    .child(mode_button(
                                        "highlight",
                                        "改为高亮",
                                        "icons/highlighter.svg",
                                    ))
                                    .child(mode_button(
                                        "underline",
                                        "改为下划线",
                                        "icons/underline.svg",
                                    )),
                            )
                            .child(div().flex().items_center().gap(px(2.)).children(
                                HighlightColor::ALL.map(|choice| {
                                    let label = format!("改为{}", choice.label());
                                    Button::new(("annotation-color", choice as usize))
                                        .ghost()
                                        .size(px(26.))
                                        .p_0()
                                        .accessibility_label(label.clone())
                                        .tooltip(label)
                                        .disabled(locked)
                                        .child(
                                            div()
                                                .w(px(26.))
                                                .h(px(if underline { 14. } else { 26. }))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded(px(if underline { 6. } else { 20. }))
                                                .border_2()
                                                .border_color(if choice == color {
                                                    rgb(0x928b7d)
                                                } else {
                                                    rgba(0x00000000)
                                                })
                                                .child(
                                                    div()
                                                        .w(px(18.))
                                                        .h(px(if underline { 5. } else { 18. }))
                                                        .rounded(px(if underline {
                                                            2.
                                                        } else {
                                                            20.
                                                        }))
                                                        .bg(rgb(if underline {
                                                            choice.line_rgb()
                                                        } else {
                                                            choice.rgb()
                                                        }))
                                                        .border_1()
                                                        .border_color(rgba(0x6a604b38)),
                                                ),
                                        )
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.edit_annotation("color", choice, window, cx)
                                        }))
                                }),
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .children(
                                [("note", "Add note", 78.), ("question", "Question", 84.)].map(
                                    |(action, label, width)| {
                                        Button::new(SharedString::from(format!(
                                            "annotation-{action}"
                                        )))
                                        .ghost()
                                        .w(px(width))
                                        .h(px(30.))
                                        .p_0()
                                        .border_1()
                                        .border_color(rgb(0xd4cbbb))
                                        .rounded(px(8.))
                                        .text_color(rgb(0x5f5a51))
                                        .text_size(px(14.))
                                        .label(label)
                                        .disabled(locked)
                                        .on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                this.edit_annotation(action, color, window, cx)
                                            }),
                                        )
                                    },
                                ),
                            )
                            .child(
                                Button::new("annotation-remove")
                                    .ghost()
                                    .p_0()
                                    .w(px(140.))
                                    .h(px(30.))
                                    .text_size(px(14.))
                                    .text_color(rgb(0xc54129))
                                    .label("Remove highlight")
                                    .disabled(locked)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.edit_annotation("remove", color, window, cx)
                                    })),
                            ),
                    ),
            )
            .with_priority(10)
            .into_any_element(),
        )
    }
}
