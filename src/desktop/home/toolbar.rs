//! Home toolbar.
use super::*;

impl ReaderView {
    pub(in crate::desktop) fn home_filter_button(
        &self,
        filter: String,
        label: String,
        path: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let count = self
            .items
            .iter()
            .filter(|item| self.home.accepts(item, &filter))
            .count();
        let active = self.home.filter == filter;
        let drop_folder = filter.clone();
        Button::new(SharedString::from(format!("home-filter-{filter}")))
            .ghost()
            .text_size(px(13.5))
            .w_full()
            .h(px(32.))
            .px(px(11.))
            .rounded(px(8.))
            .text_color(self.home.color(0x625e55, 0xc8c2b8))
            .accessibility_label(format!("{label} ({count})"))
            .when(active, |b| {
                b.bg(self.home.color(0xdce0df, 0x344951))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(self.home.color(0x282b29, 0xf4efe5))
            })
            .when(filter.starts_with("folder-"), |b| {
                b.on_drop(cx.listener(move |this, paper: &PaperDrag, _, cx| {
                    this.add_papers_to_folder(std::slice::from_ref(&paper.id), &drop_folder, cx);
                }))
                .drag_over::<PaperDrag>(|style, _, _, _| style.bg(rgb(0xdce0df)))
            })
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(icon(path, 15.).text_color(if active {
                        rgb(0x326879)
                    } else {
                        self.home.color(0x969084, 0xa39d93)
                    }))
                    .child(div().flex_1().text_left().truncate().child(label))
                    .child(
                        div()
                            .text_color(self.home.color(0x969084, 0xa39d93))
                            .text_size(px(12.))
                            .child(count.to_string()),
                    ),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.home.filter = filter.clone();
                this.home.selected.clear();
                this.home.batch = false;
                this.home.hovered_paper = None;
                this.home.scroll = ScrollHandle::new();
                this.home.popup = None;
                cx.notify();
            }))
            .into_any_element()
    }

    pub(in crate::desktop) fn home_action_anchor(
        &self,
        button: Button,
        popup: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let weak = cx.entity().downgrade();
        div()
            .relative()
            .flex_shrink_0()
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let _ = weak.update(cx, |this, _| {
                            this.home.popup_anchors.insert(popup, bounds);
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(button)
            .into_any_element()
    }

    pub(in crate::desktop) fn home_actions(
        &self,
        visible_ids: &[String],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ids = visible_ids.to_vec();
        let all_selected = !ids.is_empty() && ids.iter().all(|id| self.home.selected.contains(id));
        let disabled = self.home.selected.is_empty() || self.busy;
        if self.home.filter == "trash" {
            return div()
                .h(px(32.))
                .mb(px(12.))
                .flex()
                .items_center()
                .gap(px(8.))
                .child(div().flex_1().child(format!(
                    "{} {}",
                    self.home.selected.len(),
                    self.home.text("selected", "篇已选")
                )))
                .child(
                    Button::new("home-trash-select-all")
                        .small()
                        .label(self.home.text("Select all", "全选"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.home.batch = true;
                            this.home.selected.extend(ids.iter().cloned());
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("home-restore-selected")
                        .small()
                        .label(self.home.text("Restore", "恢复"))
                        .disabled(disabled)
                        .on_click(cx.listener(|this, _, _, cx| this.set_home_trash(false, cx))),
                )
                .into_any_element();
        }
        let action = |id, path, en, zh| {
            Button::new(id)
                .small()
                .h(px(28.))
                .rounded(px(7.))
                .icon(icon(path, 14.))
                .label(self.home.text(en, zh))
                .bg(self.home.color(0xf6f3ec, 0x242929))
                .border_color(self.home.color(0xd9d1c2, 0x444746))
                .text_color(self.home.color(0x625e55, 0xc8c2b8))
                .disabled(disabled)
        };
        let mut bar = div()
            .h(px(32.))
            .mb(px(12.))
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                Button::new("home-select-all")
                    .ghost()
                    .small()
                    .px_0()
                    .accessibility_label(
                        self.home
                            .text("Select all / deselect all", "全选 / 取消全选"),
                    )
                    .child(
                        div()
                            .size(px(17.))
                            .rounded(px(3.))
                            .border_1()
                            .border_color(rgb(0x326879))
                            .when(all_selected, |d| {
                                d.bg(rgb(0x326879))
                                    .child(icon("icons/check.svg", 15.).text_color(rgb(0xffffff)))
                            }),
                    )
                    .disabled(self.busy || ids.is_empty())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.home.batch = true;
                        if all_selected {
                            for id in &ids {
                                this.home.selected.remove(id);
                            }
                        } else {
                            this.home.selected.extend(ids.iter().cloned());
                        }
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!(
                        "{} {}",
                        self.home.selected.len(),
                        self.home.text("selected", "篇已选")
                    )),
            )
            .child(div().flex_1())
            .child(
                self.home_action_anchor(
                    action(
                        "home-batch-folder",
                        "icons/folder.svg",
                        "Add to collection",
                        "加入收藏夹",
                    )
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.home.popup = if this.home.popup == Some("folder") {
                            None
                        } else {
                            Some("folder")
                        };
                        cx.notify();
                    })),
                    "folder",
                    cx,
                ),
            )
            .child(
                self.home_action_anchor(
                    action(
                        "home-batch-status",
                        "icons/book-open.svg",
                        "Set status",
                        "设置状态",
                    )
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.home.popup = if this.home.popup == Some("status") {
                            None
                        } else {
                            Some("status")
                        };
                        cx.notify();
                    })),
                    "status",
                    cx,
                ),
            )
            .when(self.home.selected.len() == 1, |d| {
                d.child(
                    action("home-rename", "icons/pencil.svg", "Rename", "重命名")
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(
                            cx.listener(|this, _, window, cx| this.open_home_rename(window, cx)),
                        ),
                )
            })
            .child(
                action(
                    "home-batch-translate",
                    "icons/asterisk.svg",
                    "Translation",
                    "翻译",
                )
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.home.translation_pages = None;
                    this.home.popup = Some("translation");
                    cx.notify();
                })),
            )
            .child(
                Button::new("home-batch-delete")
                    .small()
                    .h(px(28.))
                    .rounded(px(7.))
                    .icon(icon("icons/trash.svg", 14.))
                    .text_color(rgb(0xc65335))
                    .accessibility_label(self.home.text("Remove selected papers", "移除所选文献"))
                    .disabled(disabled)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.home.popup = Some("remove");
                        cx.notify();
                    })),
            )
            .child(
                Button::new("home-batch-done")
                    .primary()
                    .small()
                    .h(px(28.))
                    .rounded(px(7.))
                    .bg(rgb(0x326879))
                    .label(self.home.text("Done", "完成"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.home.batch = false;
                        this.home.selected.clear();
                        this.home.popup = None;
                        cx.notify();
                    })),
            );
        if self.home.filter.starts_with("folder-") {
            bar = bar.child(
                action(
                    "home-remove-from-folder",
                    "icons/folder.svg",
                    "Remove from folder",
                    "移出文件夹",
                )
                .on_click(cx.listener(|this, _, _, cx| this.remove_selected_from_folder(cx))),
            );
        }
        bar.into_any_element()
    }
}
