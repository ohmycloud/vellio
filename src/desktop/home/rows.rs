//! Home rows.
use super::*;

impl ReaderView {
    pub(in crate::desktop) fn home_row(&self, item: &Value, cx: &mut Context<Self>) -> AnyElement {
        let id = item["id"].as_str().unwrap_or("").to_owned();
        let open_id = id.clone();
        let star_id = id.clone();
        let select_id = id.clone();
        let hover_id = id.clone();
        let starred = item["item"]["starred"].as_bool().unwrap_or(false);
        let selected = self.home.selected.contains(&id);
        let show_checkbox =
            self.home.batch || selected || self.home.hovered_paper.as_ref() == Some(&id);
        let ratio = item["ratio"].as_f64().unwrap_or(0.) as f32;
        let status = match item["status"].as_str() {
            Some("done") => self.home.text("Done", "已读"),
            Some("reading") => self.home.text("Reading", "在读"),
            _ => self.home.text("Unread", "未读"),
        };
        let authors = item["authors"].as_str().unwrap_or("");
        let year = item["meta"]["year"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| item["meta"]["year"].as_u64().map(|v| v.to_string()))
            .unwrap_or_default();
        let venue = item["meta"]["venue"]
            .as_str()
            .or_else(|| item["meta"]["publication"].as_str())
            .unwrap_or("");
        let details = [authors, year.as_str(), venue]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("   ·   ");
        let failed = item["translation"]["failed_pages"]
            .as_array()
            .map(Vec::len)
            .or_else(|| {
                item["translation"]["failed_pages"]
                    .as_u64()
                    .map(|n| n as usize)
            })
            .unwrap_or(0);
        let done = item["translation"]["done_pages"]
            .as_array()
            .map(Vec::len)
            .unwrap_or(0);
        let translation = if failed > 0 {
            if self.home.zh() {
                format!("{failed} 页处理失败")
            } else {
                format!("{failed} pages failed to process")
            }
        } else if done > 0 {
            if self.home.zh() {
                format!("已翻译 {done} 页")
            } else {
                format!("{done} pages translated")
            }
        } else {
            self.home.text("English original", "原文").into()
        };
        let cover = div()
            .w(px(52.))
            .h(px(68.))
            .flex_shrink_0()
            .bg(self.home.color(0xfbfaf7, 0x353737))
            .border_1()
            .border_color(self.home.color(0xe3ddd2, 0x444746))
            .rounded(px(3.))
            .overflow_hidden()
            .shadow(vec![
                BoxShadow::new(px(0.), px(1.), rgba(0x52452c12).into()).blur_radius(px(2.)),
            ])
            .when_some(self.home.covers.get(&id), |d, image| {
                d.child(
                    img(image.clone())
                        .size_full()
                        .object_fit(ObjectFit::Contain),
                )
            })
            .when(!self.home.covers.contains_key(&id), |d| {
                d.flex()
                    .items_center()
                    .justify_center()
                    .child(icon("icons/file.svg", 26.).text_color(rgb(0x969084)))
            });
        div()
            .id(SharedString::from(format!("home-paper-{id}")))
            .relative()
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                if *hovered {
                    this.home.hovered_paper = Some(hover_id.clone());
                } else if this.home.hovered_paper.as_ref() == Some(&hover_id) {
                    this.home.hovered_paper = None;
                }
                cx.notify();
            }))
            .role(Role::Button)
            .aria_label(format!("Open {}", catalog::title(item)))
            .on_drag(
                PaperDrag {
                    id: id.clone(),
                    title: catalog::title(item).to_owned(),
                },
                |paper, _, _, cx| cx.new(|_| paper.clone()),
            )
            .w_full()
            .flex()
            .items_start()
            .gap(px(16.))
            .py(px(15.))
            .px(px(10.))
            .rounded(px(10.))
            .hover(|style| style.bg(self.home.color(0xeeeee8, 0x293333)))
            .cursor_pointer()
            .when(selected, |d| d.bg(self.home.color(0xe5ece9, 0x293d42)))
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.busy {
                    return;
                }
                if this.home.batch {
                    if !this.home.selected.remove(&open_id) {
                        this.home.selected.insert(open_id.clone());
                    }
                    cx.notify();
                } else {
                    this.open(open_id.clone(), cx);
                }
            }))
            .child(
                div()
                    .relative()
                    .flex_shrink_0()
                    .child(cover)
                    .when(show_checkbox, |d| {
                        d.child(
                            div()
                                .id(SharedString::from(format!("home-select-{id}")))
                                .role(Role::CheckBox)
                                .aria_label(format!(
                                    "{} {}",
                                    if selected { "Deselect" } else { "Select" },
                                    catalog::title(item)
                                ))
                                .absolute()
                                .left(px(-6.))
                                .top(px(-6.))
                                .size(px(23.))
                                .rounded(px(7.))
                                .border_1()
                                .border_color(rgb(0x326879))
                                .bg(self.home.color(0xfbfaf7, 0x353737))
                                .flex()
                                .items_center()
                                .justify_center()
                                .when(selected, |d| {
                                    d.bg(rgb(0x326879)).child(
                                        icon("icons/check.svg", 17.).text_color(rgb(0xffffff)),
                                    )
                                })
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    if this.busy {
                                        return;
                                    }
                                    this.home.batch = true;
                                    this.home.popup = None;
                                    if !this.home.selected.remove(&select_id) {
                                        this.home.selected.insert(select_id.clone());
                                    }
                                    cx.notify();
                                })),
                        )
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .pb(px(24.))
                    .border_b_1()
                    .border_color(self.home.color(0xe3ddd2, 0x3f4240))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .font_family(self.fonts.title(catalog::title(item)))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_size(px(16.))
                                    .line_height(relative(1.5))
                                    .child(catalog::title(item).to_owned()),
                            )
                            .child(
                                Button::new(SharedString::from(format!("home-star-{id}")))
                                    .ghost()
                                    .xsmall()
                                    .icon(icon("icons/star.svg", 15.))
                                    .text_color(if starred {
                                        rgb(0xab761f)
                                    } else {
                                        self.home.color(0xc6c0b5, 0x74766f)
                                    })
                                    .accessibility_label(if starred {
                                        "Unstar paper"
                                    } else {
                                        "Star paper"
                                    })
                                    .disabled(self.busy)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.home_item_action(
                                            vec![star_id.clone()],
                                            "starred",
                                            json!(!starred),
                                            cx,
                                        );
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(7.))
                            .text_size(px(12.5))
                            .text_color(self.home.color(0x969084, 0xa39d93))
                            .child(details),
                    ),
            )
            .child(
                div()
                    .w(px(190.))
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .items_end()
                    .gap(px(7.))
                    .child(
                        div()
                            .px(px(8.))
                            .py(px(2.))
                            .rounded(px(12.))
                            .text_size(px(11.5))
                            .bg(self.home.color(0xe8e8e2, 0x34434a))
                            .text_color(rgb(0x326879))
                            .child(status),
                    )
                    .child(
                        div()
                            .w(px(94.))
                            .h(px(4.))
                            .rounded(px(3.))
                            .bg(self.home.color(0xe3ddd2, 0x454942))
                            .child(
                                div()
                                    .w(px(94. * ratio))
                                    .h_full()
                                    .rounded(px(3.))
                                    .bg(rgb(0x326879)),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(if failed > 0 {
                                rgb(0xd05c3d)
                            } else {
                                self.home.color(0x969084, 0xa39d93)
                            })
                            .child(translation),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(12.))
                            .text_color(self.home.color(0x969084, 0xa39d93))
                            .child(icon("icons/message-square.svg", 13.))
                            .child(item["notes"].as_u64().unwrap_or(0).to_string()),
                    ),
            )
            .into_any_element()
    }
}
