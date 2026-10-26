//! Notes drawer filtering and layout.
use super::*;

impl ReaderView {
    pub(super) fn notes(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut notes = self.all_notes();
        let discussion = self.raw["discussion"]["entries"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for (i, mut entry) in discussion.into_iter().enumerate() {
            if entry.is_object()
                && !entry["deleted"].as_bool().unwrap_or(false)
                && entry["kind"] != "reply"
            {
                if !entry["id"].is_string() {
                    entry["id"] = json!(format!("ai-entry-{i}"));
                }
                entry["kind"] = json!("ai");
                notes.push(entry);
            }
        }
        notes.sort_by_key(|note| {
            (
                self.state
                    .paper
                    .blocks
                    .iter()
                    .position(|block| block.id == note["anchor"])
                    .unwrap_or(usize::MAX),
                note["created"].as_str().unwrap_or("").to_owned(),
                note["id"].as_str().unwrap_or("").to_owned(),
            )
        });
        let count = notes.len();
        let mut groups: Vec<(String, Vec<Value>)> = Vec::new();
        for note in notes
            .into_iter()
            .filter(|note| annotation_panel::matches(note, self.annotation_filter))
        {
            let index = self
                .state
                .paper
                .blocks
                .iter()
                .position(|block| block.id == note["anchor"]);
            let heading = index
                .and_then(|index| {
                    self.state.paper.blocks[..=index]
                        .iter()
                        .rev()
                        .find(|block| block.kind == "heading")
                })
                .map(|block| {
                    annotations::plain_text(&annotations::html(&self.state.markdown(block, false)))
                })
                .unwrap_or_default();
            let page = index
                .map(|index| self.state.paper.blocks[index].page)
                .unwrap_or(0);
            let title = if page > 0 {
                format!(
                    "Page {page}{}{}",
                    if heading.is_empty() { "" } else { " · " },
                    heading
                )
            } else {
                "Other annotations".into()
            };
            if groups.last().is_none_or(|(last, _)| *last != title) {
                groups.push((title, Vec::new()));
            }
            groups.last_mut().unwrap().1.push(note);
        }
        div()
            .flex()
            .flex_col()
            .gap(px(18.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex()
                            .p(px(2.))
                            .rounded(px(9.))
                            .bg(rgb(0xeee9df))
                            .children(
                                [
                                    (false, format!("Annotations {count}")),
                                    (true, "Paper notes".to_owned()),
                                ]
                                .map(|(paper, label)| {
                                    Button::new(SharedString::from(format!(
                                        "annotation-tab-{paper}"
                                    )))
                                    .ghost()
                                    .xsmall()
                                    .h(px(28.))
                                    .px(px(10.))
                                    .text_size(px(12.))
                                    .text_color(rgb(if paper == self.paper_notes_tab {
                                        0x5f5a51
                                    } else {
                                        0x969084
                                    }))
                                    .when(paper == self.paper_notes_tab, |b| {
                                        b.bg(rgb(0xfbfaf7)).shadow(vec![
                                            BoxShadow::new(px(0.), px(1.), rgba(0x52452c12).into())
                                                .blur_radius(px(2.)),
                                        ])
                                    })
                                    .label(label)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.paper_notes_tab = paper;
                                        cx.notify();
                                    }))
                                }),
                            ),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("export-annotations")
                            .ghost()
                            .xsmall()
                            .h(px(28.))
                            .px(px(4.))
                            .text_size(px(12.))
                            .label("Export")
                            .on_click(cx.listener(|this, _, _, cx| this.export_annotations(cx))),
                    )
                    .child(
                        Button::new("close-notes-panel")
                            .ghost()
                            .xsmall()
                            .h(px(28.))
                            .w(px(22.))
                            .p_0()
                            .label("×")
                            .accessibility_label("Close Notes")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.show_panel(Panel::Notes, window, cx)
                            })),
                    ),
            )
            .when(!self.paper_notes_tab, |d| {
                d.child(
                    div().flex().flex_wrap().gap(px(5.)).children(
                        [
                            (AnnotationFilter::All, "All"),
                            (AnnotationFilter::MyNote, "My note"),
                            (AnnotationFilter::Highlights, "Highlights"),
                            (AnnotationFilter::Ai, "AI"),
                            (AnnotationFilter::Unanswered, "Unanswered"),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(i, (filter, label))| {
                            Button::new(("annotation-filter", i))
                                .ghost()
                                .xsmall()
                                .h(px(24.))
                                .px(px(9.))
                                .rounded(px(12.))
                                .border_1()
                                .border_color(rgb(if self.annotation_filter == filter {
                                    0x77716a
                                } else {
                                    0xe3ddd2
                                }))
                                .text_size(px(11.))
                                .text_color(rgb(if self.annotation_filter == filter {
                                    0x5f5a51
                                } else {
                                    0x969084
                                }))
                                .label(label)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.annotation_filter = filter;
                                    cx.notify();
                                }))
                        }),
                    ),
                )
                .child(
                    Button::new("add-paragraph-note")
                        .ghost()
                        .xsmall()
                        .h(px(28.))
                        .px(px(10.))
                        .border_1()
                        .border_color(rgb(0xd4cbbb))
                        .rounded(px(7.))
                        .text_size(px(12.))
                        .w(px(300.))
                        .max_w_full()
                        .label("+  Add a note to the current paragraph")
                        .on_click(
                            cx.listener(|this, _, window, cx| this.add_paragraph_note(window, cx)),
                        ),
                )
                .when(groups.is_empty(), |d| {
                    d.child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgb(0x969084))
                            .child("No annotations in this filter."),
                    )
                })
                .children(groups.into_iter().map(|(title, notes)| {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(12.))
                        .child(
                            div()
                                .pb(px(7.))
                                .border_b_1()
                                .border_color(rgb(0xe3ddd2))
                                .text_size(px(11.))
                                .text_color(rgb(0x969084))
                                .child(title),
                        )
                        .children(notes.iter().map(|note| {
                            if note["kind"] == "ai" {
                                self.discussion_card(note, true, cx)
                            } else {
                                self.annotation_card(note, true, cx)
                            }
                        }))
                }))
            })
            .when(self.paper_notes_tab, |d| {
                d.child(
                    div()
                        .text_size(px(13.))
                        .text_color(rgb(0x969084))
                        .child("Notes about the whole book"),
                )
                .child(Textarea::new(&self.paper_note_input).h(px(300.)))
                .child(
                    Button::new("save-paper-note")
                        .ghost()
                        .xsmall()
                        .border_1()
                        .border_color(rgb(0xd4cbbb))
                        .label("Save paper notes")
                        .disabled(self.busy || self.workspace.is_none())
                        .on_click(cx.listener(|this, _, _, cx| {
                            let body = this.paper_note_input.read(cx).value().to_string();
                            this.save_ops(
                                vec![json!({"op":"paper_note","body":body,"at":now()})],
                                cx,
                            );
                        })),
                )
            })
            .into_any_element()
    }
}
