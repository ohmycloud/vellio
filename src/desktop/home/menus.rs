//! Home menus.
use super::*;

impl ReaderView {
    pub(in crate::desktop) fn home_popup(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let popup = self.home.popup?;
        let mut content = div().flex().flex_col().gap(px(9.));
        let heading = match popup {
            "help" => self.home.text("Reading with Vellio", "使用 Vellio"),
            "settings" => self.home.text("Settings", "设置"),
            "sort" => self.home.text("Sort papers", "文献排序"),
            "status" => self.home.text("Set status", "设置阅读状态"),
            "translation" => self
                .home
                .text("Translate selected papers?", "翻译所选文献？"),
            "remove" => self.home.text("Remove selected papers?", "移除所选文献？"),
            "rename" => self.home.text("Rename", "重命名"),
            _ => self.home.text("Add to folder", "加入文件夹"),
        };
        if !matches!(
            popup,
            "sort" | "status" | "folder" | "remove" | "new-folder"
        ) {
            content = content.child(div().font_weight(FontWeight::SEMIBOLD).child(heading));
        } else {
            content = content.gap_0();
        }
        match popup {
            "help" => {
                content = content.child(self.home.text("Open a paper to read. Select text to highlight, take notes or ask AI. Your reading position and drafts save automatically.", "点击文献开始阅读。选中文字可高亮、写笔记或提问。阅读位置与草稿自动保存。"))
                    .child(self.home.text("Use / to search the library. Batch actions organize papers into folders and update reading status.", "按 / 搜索文献库。批量操作可加入文件夹或设置阅读状态。"))
                    .child(Button::new("home-about-updates").debug_selector(|| "home-about-updates".into()).ghost()
                        .label(self.home.text("About Vellio / Updates", "关于 Vellio / 软件更新"))
                        .on_click(cx.listener(|this, _, window, cx| this.open_about(window, cx))))
                    .child(Button::new("home-report-issue").ghost()
                        .label(self.home.text("Report on GitHub", "在 GitHub 反馈"))
                        .on_click(|_, _, cx| cx.open_url(easyread_gpui::updates::ISSUES)));
            }
            "sort" => {
                for (i, (en, zh)) in SORT_OPTIONS.into_iter().enumerate() {
                    content = content.child(
                        Button::new(("home-sort-option", i))
                            .ghost()
                            .small()
                            .text_color(self.home.color(0x625e55, 0xc8c2b8))
                            .w_full()
                            .h(px(32.))
                            .px(px(8.))
                            .justify_start()
                            .text_left()
                            .accessibility_label(self.home.text(en, zh))
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(
                                        div()
                                            .w(px(18.))
                                            .flex_shrink_0()
                                            .when(self.home.sort == i, |d| {
                                                d.child(icon("icons/check.svg", 16.))
                                            }),
                                    )
                                    .child(
                                        div().flex_1().text_left().child(self.home.text(en, zh)),
                                    ),
                            )
                            .when(self.home.sort == i, |b| {
                                b.bg(self.home.color(0xe5ece9, 0x344951))
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.home.sort = i;
                                this.home.popup = None;
                                cx.notify();
                            })),
                    );
                }
            }
            "status" => {
                for (status, en, zh, path) in [
                    ("reading", "Reading", "在读", "icons/book-open.svg"),
                    ("unread", "Unread", "未读", "icons/book-open.svg"),
                    ("done", "Done", "已读", "icons/check.svg"),
                ] {
                    let checked = !self.home.selected.is_empty()
                        && self
                            .items
                            .iter()
                            .filter(|item| {
                                self.home
                                    .selected
                                    .contains(item["id"].as_str().unwrap_or(""))
                            })
                            .all(|item| item["status"] == status);
                    content = content.child(
                        Button::new(SharedString::from(format!("home-status-{status}")))
                            .ghost()
                            .small()
                            .text_color(self.home.color(0x625e55, 0xc8c2b8))
                            .w_full()
                            .h(px(36.))
                            .px(px(12.))
                            .justify_start()
                            .accessibility_label(self.home.text(en, zh))
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .items_center()
                                    .gap(px(12.))
                                    .child(icon(path, 16.))
                                    .child(div().flex_1().text_left().child(self.home.text(en, zh)))
                                    .child(div().w(px(18.)).when(checked, |d| {
                                        d.child(
                                            icon("icons/check.svg", 13.)
                                                .text_color(self.home.color(0x969084, 0xa39d93)),
                                        )
                                    })),
                            )
                            .disabled(self.busy || self.home.selected.is_empty())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.home.popup = None;
                                this.home_item_action(
                                    this.home.selected.iter().cloned().collect(),
                                    "status",
                                    json!(status),
                                    cx,
                                );
                            })),
                    );
                }
            }
            "translation" => {
                let model = self
                    .settings
                    .committed
                    .models
                    .iter()
                    .find(|m| m.id == self.settings.committed.translation_model)
                    .map(|m| m.name.as_str())
                    .unwrap_or("configured model");
                content = content.child(if self.home.zh() {
                    format!("将 {} 篇文献的未翻译正文发送给 {model}。已有翻译、编辑和笔记会保留。", self.home.selected.len())
                } else {
                    format!("Send untranslated text from {} papers to {model}. Existing translations, edits and notes are retained.", self.home.selected.len())
                });
                if self.home.selected.len() == 1
                    && let Some(item) = self.items.iter().find(|item| {
                        self.home
                            .selected
                            .contains(item["id"].as_str().unwrap_or(""))
                    })
                    && item["meta"]["format"] != "epub"
                {
                    let available: Vec<_> = item["untranslated_pages"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_u64)
                        .map(|p| p as usize)
                        .collect();
                    content = content.child(
                        div()
                            .debug_selector(|| "home-choose-translation-pages".into())
                            .child(
                                Button::new("home-choose-translation-pages")
                                    .ghost()
                                    .small()
                                    .label(
                                        self.home
                                            .text("Select untranslated pages…", "选未译页翻译…"),
                                    )
                                    .disabled(available.is_empty() || self.busy)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.home.translation_pages =
                                            if this.home.translation_pages.is_some() {
                                                None
                                            } else {
                                                Some(HashSet::new())
                                            };
                                        cx.notify();
                                    })),
                            ),
                    );
                    if let Some(chosen) = &self.home.translation_pages {
                        let mut buttons = div()
                            .id("home-translation-page-list")
                            .max_h(px(180.))
                            .overflow_y_scroll()
                            .flex()
                            .flex_wrap()
                            .gap(px(5.));
                        for page in available {
                            let selected = chosen.contains(&page);
                            buttons = buttons.child(
                                div()
                                    .debug_selector(move || format!("home-translation-page-{page}"))
                                    .child(
                                        Button::new(("home-translation-page", page))
                                            .small()
                                            .ghost()
                                            .label(page.to_string())
                                            .accessibility_label(format!("Source page {page}"))
                                            .when(selected, |b| {
                                                b.bg(self.home.color(0xe5ece9, 0x344951))
                                            })
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                if let Some(pages) =
                                                    &mut this.home.translation_pages
                                                    && !pages.insert(page)
                                                {
                                                    pages.remove(&page);
                                                }
                                                cx.notify();
                                            })),
                                    ),
                            );
                        }
                        content = content.child(buttons).child(if self.home.zh() {
                            format!("已选 {} 页；已译页不重复发送。", chosen.len())
                        } else {
                            format!(
                                "{} pages selected; translated pages are not resent.",
                                chosen.len()
                            )
                        });
                    }
                }
                content = content.child(
                    Button::new("home-confirm-translation")
                        .primary()
                        .small()
                        .label(self.home.text("Translate", "开始翻译"))
                        .disabled(
                            self.busy
                                || self.home.selected.is_empty()
                                || self.home.translation_pages.as_ref().is_some_and(|pages| {
                                    pages.is_empty() || self.home.selected.len() != 1
                                }),
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.translate_home_selection(cx))),
                );
            }
            "remove" => {
                content = content
                    .gap(px(12.))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(18.))
                            .child(if self.home.zh() {
                                format!("将 {} 篇文献移到回收站？", self.home.selected.len())
                            } else {
                                format!(
                                    "Move {} {} to Trash?",
                                    self.home.selected.len(),
                                    if self.home.selected.len() == 1 {
                                        "paper"
                                    } else {
                                        "papers"
                                    }
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_color(self.home.color(0x625e55, 0xc8c2b8))
                            .text_size(px(16.))
                            .child(self.home.text(
                                "You can restore them anytime from Trash in the sidebar.",
                                "可以随时从侧栏的回收站恢复这些文献。",
                            )),
                    )
                    .child(
                        div()
                            .mt(px(8.))
                            .flex()
                            .justify_end()
                            .gap(px(12.))
                            .child(
                                Button::new("home-cancel-remove")
                                    .ghost()
                                    .label(self.home.text("Cancel", "取消"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.home.popup = None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("home-confirm-remove")
                                    .primary()
                                    .bg(rgb(0xb4482d))
                                    .rounded(px(9.))
                                    .label(self.home.text("Move to Trash", "移到回收站"))
                                    .disabled(self.busy || self.home.selected.is_empty())
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.set_home_trash(true, cx)),
                                    ),
                            ),
                    );
            }
            "rename" => {
                content = content
                    .gap(px(14.))
                    .child(
                        Input::new(&self.home.rename_input)
                            .aria_label(self.home.text("Name", "名称")),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(10.))
                            .child(
                                Button::new("home-cancel-rename")
                                    .ghost()
                                    .label(self.home.text("Cancel", "取消"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.home.popup = None;
                                        this.home.rename_id = None;
                                        window.blur(cx);
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("home-save-rename")
                                    .primary()
                                    .label(self.home.text("Save", "保存"))
                                    .disabled(
                                        self.busy
                                            || self
                                                .home
                                                .rename_input
                                                .read(cx)
                                                .value()
                                                .trim()
                                                .is_empty(),
                                    )
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.rename_home_file(window, cx)
                                    })),
                            ),
                    );
            }
            "new-folder" => {
                content =
                    content
                        .gap(px(14.))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_size(px(18.))
                                .child(self.home.text(
                                    "New folder with selected papers",
                                    "为所选文献新建文件夹",
                                )),
                        )
                        .child(Input::new(&self.home.folder_input).aria_label("Folder name"))
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap(px(10.))
                                .child(
                                    Button::new("home-cancel-folder")
                                        .ghost()
                                        .label(self.home.text("Cancel", "取消"))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.home.popup = None;
                                            this.home.folder_papers = None;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("home-create-folder-selected")
                                        .primary()
                                        .label(self.home.text("Create folder", "创建文件夹"))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.create_home_folder(window, cx)
                                        })),
                                ),
                        );
            }
            _ => {
                content = content.child(
                    Button::new("home-collection-starred")
                        .ghost()
                        .small()
                        .text_color(self.home.color(0x625e55, 0xc8c2b8))
                        .w_full()
                        .h(px(38.))
                        .px(px(12.))
                        .accessibility_label(self.home.text("Starred", "收藏"))
                        .child(
                            div()
                                .w_full()
                                .flex()
                                .items_center()
                                .gap(px(12.))
                                .child(icon("icons/star.svg", 17.))
                                .child(
                                    div()
                                        .flex_1()
                                        .text_left()
                                        .child(self.home.text("Starred", "收藏")),
                                ),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.home.popup = None;
                            this.home_item_action(
                                this.home.selected.iter().cloned().collect(),
                                "starred",
                                json!(true),
                                cx,
                            );
                        })),
                );
                for (i, (id, name)) in self.home.folders().into_iter().enumerate() {
                    content = content.child(
                        Button::new(("home-assign-folder", i))
                            .ghost()
                            .small()
                            .text_color(self.home.color(0x625e55, 0xc8c2b8))
                            .w_full()
                            .h(px(36.))
                            .px(px(12.))
                            .accessibility_label(name.clone())
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .items_center()
                                    .gap(px(12.))
                                    .child(icon("icons/folder.svg", 17.))
                                    .child(div().flex_1().text_left().truncate().child(name)),
                            )
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.assign_home_folder(&id, cx)),
                            ),
                    );
                }
                content = content
                    .child(
                        div()
                            .h(px(1.))
                            .mx(px(10.))
                            .my(px(6.))
                            .bg(self.home.color(0xe3ddd2, 0x444746)),
                    )
                    .child(
                        Button::new("home-new-folder-selected")
                            .ghost()
                            .small()
                            .text_color(self.home.color(0x625e55, 0xc8c2b8))
                            .w_full()
                            .h(px(40.))
                            .px(px(12.))
                            .accessibility_label(
                                self.home
                                    .text("New folder with this paper…", "为所选文献新建文件夹…"),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .items_center()
                                    .gap(px(12.))
                                    .child(icon("icons/plus.svg", 17.))
                                    .child(div().flex_1().text_left().child(self.home.text(
                                        "New folder with this paper…",
                                        "为所选文献新建文件夹…",
                                    ))),
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.home.folder_papers =
                                    Some(this.home.selected.iter().cloned().collect());
                                this.home.popup = Some("new-folder");
                                this.home
                                    .folder_input
                                    .update(cx, |input, cx| input.set_value("", window, cx));
                                this.home
                                    .folder_input
                                    .read(cx)
                                    .focus_handle(cx)
                                    .focus(window, cx);
                                cx.notify();
                            })),
                    );
            }
        }
        if matches!(popup, "remove" | "new-folder" | "rename") {
            return Some(
                div()
                    .id("home-confirm-overlay")
                    .debug_selector(|| "home-confirm-overlay".into())
                    .absolute()
                    .inset_0()
                    .occlude()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgba(0x00000028))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .w(px(400.))
                            .p(px(24.))
                            .rounded(px(14.))
                            .bg(self.home.color(0xfbfaf7, 0x2a3030))
                            .border_1()
                            .border_color(self.home.color(0xe3ddd2, 0x444746))
                            .shadow(vec![
                                BoxShadow::new(px(0.), px(6.), rgba(0x52452c20).into())
                                    .blur_radius(px(24.)),
                            ])
                            .child(content),
                    )
                    .into_any_element(),
            );
        }
        let width = if popup == "folder" { 280. } else { 220. };
        let anchor = self.home.popup_anchors.get(popup).zip(self.home.bounds);
        Some(
            div()
                .id("home-popup")
                .occlude()
                .absolute()
                .when_some(anchor, |d, (bounds, root)| {
                    d.left((bounds.right() - root.left() - px(width)).max(px(10.)))
                        .top(bounds.bottom() - root.top() + px(4.))
                })
                .when(anchor.is_none(), |d| {
                    d.right(px(28.))
                        .top(px(if popup == "help" { 54. } else { 110. }))
                })
                .w(px(if matches!(popup, "folder" | "status" | "sort") {
                    width
                } else {
                    350.
                }))
                .p(px(if matches!(popup, "sort" | "status" | "folder") {
                    6.
                } else {
                    16.
                }))
                .rounded(px(12.))
                .bg(self.home.color(0xfbfaf7, 0x2a3030))
                .border_1()
                .border_color(self.home.color(0xe3ddd2, 0x444746))
                .shadow(vec![
                    BoxShadow::new(px(0.), px(4.), rgba(0x52452c20).into()).blur_radius(px(16.)),
                ])
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(content)
                .into_any_element(),
        )
    }
}
