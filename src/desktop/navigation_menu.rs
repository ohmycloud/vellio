//! Library and contents navigation popovers.
use super::*;

impl ReaderView {
    pub(super) fn navigation_button(
        &self,
        navigation: Navigation,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let slot = if navigation == Navigation::Library {
            0
        } else {
            1
        };
        let next = if self.navigation == Some(navigation) {
            None
        } else {
            Some(navigation)
        };
        let weak = cx.entity().downgrade();
        div()
            .relative()
            .flex_shrink_0()
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let _ = weak.update(cx, |this, _| {
                            this.navigation_button_bounds[slot] = Some(bounds)
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(
                Button::new(if slot == 0 {
                    "library-toggle"
                } else {
                    "contents-toggle"
                })
                .ghost()
                .small()
                .w(px(32.))
                .h(px(32.))
                .rounded(px(8.))
                .when(slot == 0, |b| {
                    b.p_0().child(
                        img(self.home.brand_icon.clone())
                            .id("reader-brand-icon")
                            .debug_selector(|| "reader-brand-icon".into())
                            .size(px(28.))
                            .flex_shrink_0()
                            .object_fit(ObjectFit::Contain),
                    )
                })
                .when(slot == 1, |b| {
                    b.icon(Icon::default().path("icons/list.svg").size(px(19.)))
                })
                .accessibility_label(if slot == 0 { "文献库" } else { "目录" })
                .when(slot == 1 && self.navigation == Some(navigation), |b| {
                    b.bg(rgba(0x38748720)).text_color(rgb(0x387487))
                })
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.navigation = next;
                    this.reading_order_open = false;
                    if slot == 0 && this.library.is_some() {
                        this.show_home(window, cx);
                        return;
                    }
                    this.appearance_open = false;
                    this.selection = None;
                    this.annotation_popup = None;
                    if next.is_some() {
                        this.navigation_focus.focus(window, cx);
                    } else {
                        window.blur(cx);
                    }
                    cx.notify();
                })),
            )
    }

    pub(super) fn navigation_menu(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let navigation = self.navigation?;
        let weak = cx.entity().downgrade();
        let mut entries = div()
            .id("navigation-entries")
            .h(px(480.).min((window.viewport_size().height - px(150.)).max(px(120.))))
            .track_scroll(&self.navigation_scroll)
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(4.));
        if navigation == Navigation::Contents {
            let mut headings = self.contents();
            let page_only = headings.is_empty();
            if page_only {
                let mut last_page = None;
                for (i, b) in self.state.paper.blocks.iter().enumerate() {
                    if last_page != Some(b.page) {
                        headings.push((i, format!("第 {} 页", b.page.max(1)), 0));
                        last_page = Some(b.page);
                    }
                }
                entries = entries.child(
                    div()
                        .p_2()
                        .text_size(px(12.))
                        .text_color(rgb(0x969084))
                        .child("这本书尚无结构化章节，按页导航"),
                );
            }
            let current = self.list.logical_scroll_top().item_ix;
            let active = headings
                .iter()
                .enumerate()
                .rfind(|(_, (i, _, _))| *i <= current)
                .map(|(entry, _)| entry);
            if headings.is_empty() {
                entries = entries.child(div().p_2().child("暂无目录"));
            }
            for (entry, (index, title, depth)) in headings.into_iter().enumerate() {
                let (number, body) = split_heading_number(&title);
                let label = div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .pl(px(depth as f32 * 16.))
                    .when_some(number, |d, number| {
                        d.child(
                            div()
                                .flex_shrink_0()
                                .text_color(rgb(0x969084))
                                .child(number.to_owned()),
                        )
                    })
                    .child(div().flex_1().min_w_0().truncate().child(body.to_owned()));
                entries = entries.child(
                    Button::new(("contents-entry", entry))
                        .ghost()
                        .small()
                        .w_full()
                        .justify_start()
                        .accessibility_label(title.clone())
                        .child(label)
                        .when(active == Some(entry), |b| {
                            b.bg(rgba(0x38748720)).text_color(rgb(0x387487))
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.navigation = None;
                            this.list.scroll_to(ListOffset {
                                item_ix: index,
                                offset_in_item: px(0.),
                            });
                            this.select(index, window, cx);
                            window.blur(cx);
                        })),
                );
            }
        } else {
            for (i, item) in self.items.iter().enumerate() {
                let id = item["id"].as_str().unwrap_or("").to_owned();
                let title = item["meta"]["title_zh"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .or_else(|| item["meta"]["title_en"].as_str())
                    .unwrap_or("未命名")
                    .to_owned();
                entries = entries.child(
                    Button::new(("paper", i))
                        .ghost()
                        .small()
                        .w_full()
                        .justify_start()
                        .accessibility_label(title.clone())
                        .child(div().w_full().truncate().child(title))
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.navigation = None;
                            this.open(id.clone(), cx);
                        })),
                );
            }
            entries = entries.child(
                div()
                    .mt_2()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        Button::new("import")
                            .label("导入 PDF / EPUB")
                            .disabled(self.busy || self.library.is_none())
                            .on_click(cx.listener(|this, _, _, cx| this.pick(false, cx))),
                    )
                    .child(
                        Button::new("migrate")
                            .label("迁入旧论文")
                            .disabled(self.busy || self.library.is_none())
                            .on_click(cx.listener(|this, _, _, cx| this.pick(true, cx))),
                    )
                    .when(self.settings.committed.features.original, |d| {
                        d.child(
                            Button::new("bilingual")
                                .label(if self.reading_mode == ReadingMode::Bilingual {
                                    "仅原文"
                                } else {
                                    "中英对照"
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.toggle_bilingual(cx);
                                })),
                        )
                    })
                    .child(
                        Button::new("complete-images")
                            .label("补全图片")
                            .disabled(
                                self.busy
                                    || self.workspace.is_none()
                                    || !self
                                        .state
                                        .paper
                                        .blocks
                                        .iter()
                                        .any(|b| b.source_kind == "pdf-text"),
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.complete_images(cx))),
                    )
                    .child(
                        Button::new("translate")
                            .label("翻译")
                            .disabled(self.busy || self.workspace.is_none())
                            .on_click(cx.listener(|this, _, _, cx| this.translate(cx))),
                    ),
            );
        }
        let search = div()
            .mt_2()
            .flex()
            .gap_2()
            .child(Input::new(&self.search))
            .child(Button::new("find").label("查找").on_click(cx.listener(
                |this, _, window, cx| {
                    let q = this.search.read(cx).value().to_string();
                    if q.is_empty() {
                        return;
                    }
                    if let Some(i) = this
                        .state
                        .paper
                        .blocks
                        .iter()
                        .position(|b| this.state.markdown(b, true).contains(&q))
                    {
                        this.navigation = None;
                        this.list.scroll_to(ListOffset {
                            item_ix: i,
                            offset_in_item: px(0.),
                        });
                        this.select(i, window, cx);
                        window.blur(cx);
                    } else {
                        this.status = "未找到".into();
                        cx.notify();
                    }
                },
            )));
        Some(
            div()
                .id("navigation-menu")
                .occlude()
                .absolute()
                .left(px(16.))
                .top(px(62.))
                .w(px(360.).min((window.viewport_size().width - px(32.)).max(px(220.))))
                .track_focus(&self.navigation_focus)
                .p(px(12.))
                .flex()
                .flex_col()
                .gap_2()
                .bg(rgb(0xfbfaf7))
                .border_1()
                .border_color(rgb(0xe2dccf))
                .rounded(px(12.))
                .shadow(vec![
                    BoxShadow::new(px(0.), px(3.), rgba(0x52452c20).into()).blur_radius(px(14.)),
                ])
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    canvas(
                        move |bounds, _, cx| {
                            let _ = weak
                                .update(cx, |this, _| this.navigation_menu_bounds = Some(bounds));
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_2()
                        .child(if navigation == Navigation::Contents {
                            "目录"
                        } else {
                            "文献库"
                        })
                        .child(
                            Button::new("close-navigation")
                                .ghost()
                                .small()
                                .label("×")
                                .accessibility_label("关闭导航")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.navigation = None;
                                    window.blur(cx);
                                    cx.notify();
                                })),
                        ),
                )
                .child(search)
                .child(entries)
                .into_any_element(),
        )
    }
}
