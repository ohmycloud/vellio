//! Home view.
use super::*;

impl ReaderView {
    pub(in crate::desktop) fn home_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !self.settings.open
            && (window.focused(cx).is_none()
                || self.reader_focus.is_focused(window)
                || self.settings.focus.is_focused(window))
        {
            self.home.focus.focus(window, cx);
        }
        window.set_window_title("Vellio");
        let query = self.home.search.read(cx).value().to_string();
        let folders = self.home.folders();
        let mut items: Vec<_> = self
            .items
            .iter()
            .filter(|item| {
                let assigned = folders
                    .iter()
                    .filter(|(id, _)| self.home.in_folder(item["id"].as_str().unwrap_or(""), id))
                    .map(|(_, name)| name.clone())
                    .collect::<Vec<_>>();
                self.home.accepts(item, &self.home.filter)
                    && catalog::matches(item, &query, &assigned)
            })
            .collect();
        items.sort_by(|a, b| catalog::compare_for_sort(a, b, self.home.sort));
        let label = match self.home.filter.as_str() {
            "all" => self.home.text("All books", "全部书籍").to_owned(),
            "reading" => self.home.text("Reading", "在读").to_owned(),
            "unread" => self.home.text("Unread", "未读").to_owned(),
            "done" => self.home.text("Done", "已读").to_owned(),
            "starred" => self.home.text("Starred", "已收藏").to_owned(),
            "trash" => self.home.text("Trash", "回收站").to_owned(),
            "questions" => self.home.text("Open questions", "待解答问题").to_owned(),
            "translating" => self.home.text("Translating", "正在翻译").to_owned(),
            id => folders
                .iter()
                .find(|(fid, _)| fid == id)
                .map(|(_, n)| n.clone())
                .unwrap_or_default(),
        };
        let count = items.len();
        let visible_ids = items
            .iter()
            .map(|i| i["id"].as_str().unwrap_or("").to_owned())
            .collect::<Vec<_>>();
        let mut sidebar = div()
            .id("home-sidebar")
            .text_size(px(13.5))
            .w(px(235.))
            .flex_shrink_0()
            .h_full()
            .border_r_1()
            .border_color(self.home.color(0xe3ddd2, 0x3f4240))
            .px(px(10.))
            .py(px(16.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .track_scroll(&self.home.sidebar_scroll)
            .overflow_y_scroll()
            .child(
                div()
                    .px(px(10.))
                    .mb(px(6.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(11.))
                    .text_color(self.home.color(0x969084, 0xa39d93))
                    .child(self.home.text("F o l d e r s", "文件夹"))
                    .child(
                        Button::new("home-add-folder")
                            .ghost()
                            .xsmall()
                            .icon(icon("icons/plus.svg", 15.))
                            .accessibility_label("New folder")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.home.adding_folder = !this.home.adding_folder;
                                if this.home.adding_folder {
                                    this.home
                                        .folder_input
                                        .read(cx)
                                        .focus_handle(cx)
                                        .focus(window, cx);
                                }
                                cx.notify();
                            })),
                    ),
            );
        let mut sidebar_folders = vec![
            (
                "all".to_string(),
                self.home.text("All", "全部").to_string(),
                "icons/book-open.svg",
            ),
            (
                "reading".into(),
                self.home.text("Reading", "在读").into(),
                "icons/book-open.svg",
            ),
            (
                "unread".into(),
                self.home.text("Unread", "未读").into(),
                "icons/book-open.svg",
            ),
            (
                "done".into(),
                self.home.text("Done", "已读").into(),
                "icons/check.svg",
            ),
            (
                "starred".into(),
                self.home.text("Starred", "收藏").into(),
                "icons/star.svg",
            ),
        ];
        if self
            .items
            .iter()
            .any(|i| i["open_questions"].as_u64().unwrap_or(0) > 0)
        {
            sidebar_folders.push((
                "questions".into(),
                self.home.text("Open questions", "待解答问题").into(),
                "icons/circle-question-mark.svg",
            ));
        }
        if self
            .items
            .iter()
            .any(|i| i["translating"].as_bool().unwrap_or(false))
        {
            sidebar_folders.push((
                "translating".into(),
                self.home.text("Translating", "正在翻译").into(),
                "icons/asterisk.svg",
            ));
        }
        sidebar_folders.push((
            "trash".into(),
            self.home.text("Trash", "回收站").into(),
            "icons/trash.svg",
        ));
        sidebar_folders.extend(
            folders
                .iter()
                .map(|(id, name)| (id.clone(), name.clone(), "icons/book-open.svg")),
        );
        sidebar_folders.retain(|(id, _, _)| {
            id == "all"
                || id == "trash"
                || !self.home.prefs["hidden_folders"][id]
                    .as_bool()
                    .unwrap_or(false)
        });
        sidebar_folders.sort_by_key(|(id, _, _)| {
            if id == "all" {
                0
            } else if self.home.prefs["pinned_folders"][id]
                .as_bool()
                .unwrap_or(false)
            {
                1
            } else {
                2
            }
        });
        for (id, name, path) in sidebar_folders {
            sidebar = sidebar.child(self.home_filter_button(id, name, path, cx));
        }
        sidebar = sidebar
            .when(self.home.adding_folder, |d| {
                d.child(
                    div()
                        .mt(px(8.))
                        .flex()
                        .flex_col()
                        .gap(px(7.))
                        .child(
                            Input::new(&self.home.folder_input)
                                .small()
                                .aria_label("Folder name"),
                        )
                        .child(
                            Button::new("home-create-folder")
                                .small()
                                .label(self.home.text("Create folder", "创建文件夹"))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.create_home_folder(window, cx)
                                })),
                        ),
                )
            })
            .child(
                div()
                    .px(px(10.))
                    .mt(px(8.))
                    .text_size(px(11.))
                    .text_color(self.home.color(0x969084, 0xa39d93))
                    .child(self.home.text(
                        "New folder, then drag papers in",
                        "创建文件夹后，将文献拖入",
                    )),
            )
            .child(
                div()
                    .px(px(10.))
                    .mt(px(24.))
                    .mb(px(8.))
                    .text_size(px(11.))
                    .text_color(self.home.color(0x969084, 0xa39d93))
                    .child(self.home.text("R e c e n t", "最近阅读")),
            );
        let mut recent: Vec<_> = self
            .items
            .iter()
            .filter(|item| {
                self.home.accepts(item, "all") && !item["opened"].as_str().unwrap_or("").is_empty()
            })
            .collect();
        recent.sort_by(|a, b| b["opened"].as_str().cmp(&a["opened"].as_str()));
        for (i, item) in recent.into_iter().take(5).enumerate() {
            let id = item["id"].as_str().unwrap_or("").to_owned();
            sidebar = sidebar.child(
                Button::new(("home-recent", i))
                    .ghost()
                    .w_full()
                    .h(px(33.))
                    .px(px(10.))
                    .disabled(self.busy)
                    .accessibility_label(format!("Continue reading {}", catalog::title(item)))
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_left()
                                    .text_size(px(13.))
                                    .text_color(self.home.color(0x625e55, 0xc8c2b8))
                                    .child(catalog::title(item).to_owned()),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(self.home.color(0x969084, 0xa39d93))
                                    .child(format!(
                                        "{:.0}%",
                                        item["ratio"].as_f64().unwrap_or(0.) * 100.
                                    )),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.open(id.clone(), cx))),
            );
        }
        let mut rows = div()
            .id("home-papers")
            .flex_1()
            .min_h_0()
            .track_scroll(&self.home.scroll)
            .overflow_y_scroll();
        if items.is_empty() {
            rows = rows.child(
                div()
                    .py(px(80.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(12.))
                    .text_color(self.home.color(0x969084, 0xa39d93))
                    .child(icon("icons/book-open.svg", 38.))
                    .child(if self.items.is_empty() {
                        self.home
                            .text("Your library starts here", "从这里开始建立文献库")
                    } else {
                        self.home.text("No matching papers", "没有匹配的文献")
                    })
                    .when(self.items.is_empty(), |d| {
                        d.child(
                            Button::new("home-empty-import")
                                .small()
                                .label(self.home.text("Import PDF / EPUB", "导入 PDF / EPUB"))
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, _, cx| this.pick(false, cx))),
                        )
                    }),
            );
        }
        for item in items {
            rows = rows.child(self.home_row(item, cx));
        }
        let sort_label = self.home.text(
            SORT_OPTIONS[self.home.sort].0,
            SORT_OPTIONS[self.home.sort].1,
        );
        let show_actions = self.home.hovering || self.home.batch || self.home.filter == "trash";
        let mut main = div()
            .id("home-main")
            .flex_1()
            .min_w_0()
            .min_h_0()
            .px(px(28.))
            .pt(px(18.))
            .flex()
            .flex_col()
            .on_hover(cx.listener(|this, hovered, _, cx| {
                if this.home.hovering != *hovered {
                    this.home.hovering = *hovered;
                    cx.notify();
                }
            }))
            .when(show_actions, |d| {
                d.child(self.home_actions(&visible_ids, cx))
            })
            .when(!show_actions, |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .h(px(32.))
                        .mb(px(12.))
                        .child(
                            div()
                                .font_family(self.fonts.title(&label))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_size(px(20.))
                                .line_height(relative(1.3))
                                .child(label),
                        )
                        .child(
                            div()
                                .text_size(px(13.))
                                .text_color(self.home.color(0x969084, 0xa39d93))
                                .child(if self.home.zh() {
                                    format!("{count} 篇")
                                } else {
                                    format!("{count} papers")
                                }),
                        )
                        .child(div().flex_1())
                        .child(
                            Button::new("home-batch")
                                .small()
                                .h(px(27.))
                                .rounded(px(7.))
                                .icon(icon("icons/check.svg", 13.))
                                .label(self.home.text("Batch actions", "批量操作"))
                                .bg(self.home.color(0xf6f3ec, 0x242929))
                                .border_color(self.home.color(0xd9d1c2, 0x444746))
                                .text_color(self.home.color(0x625e55, 0xc8c2b8))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.home.batch = !this.home.batch;
                                    this.home.selected.clear();
                                    this.home.popup = None;
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .text_size(px(12.5))
                                .text_color(self.home.color(0x969084, 0xa39d93))
                                .child(self.home.text("Sort", "排序")),
                        )
                        .child(
                            Button::new("home-sort")
                                .small()
                                .h(px(29.))
                                .w(px(170.))
                                .rounded(px(7.))
                                .justify_start()
                                .text_left()
                                .accessibility_label(sort_label)
                                .child(
                                    div()
                                        .w_full()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.))
                                        .child(div().flex_1().text_left().child(sort_label))
                                        .child(icon("icons/chevron-down.svg", 13.)),
                                )
                                .bg(self.home.color(0xfbfaf7, 0x242929))
                                .border_color(rgb(0x326879))
                                .text_color(self.home.color(0x23211d, 0xece7db))
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.home.popup = if this.home.popup == Some("sort") {
                                        None
                                    } else {
                                        Some("sort")
                                    };
                                    cx.notify();
                                })),
                        ),
                )
            });
        let show_cancel = self.busy && self.home.cancellable_task;
        if !self.home.notice.is_empty() || show_cancel {
            main = main.child(
                div()
                    .mb(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .text_size(px(12.))
                    .text_color(rgb(0xc26d42))
                    .child(div().flex_1().child(self.home.notice.clone()))
                    .when(show_cancel, |d| {
                        d.child(
                            div().debug_selector(|| "home-cancel-task".into()).child(
                                Button::new("home-cancel")
                                    .ghost()
                                    .small()
                                    .label(self.home.text("Cancel", "取消"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.cancel.store(true, Ordering::Relaxed);
                                        cx.notify();
                                    })),
                            ),
                        )
                    }),
            );
        }
        main = main.child(rows);
        let popup = self.home_popup(cx);
        let weak = cx.entity().downgrade();
        div()
            .id("library-home")
            .track_focus(&self.home.focus)
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(self.home.color(0xf6f3ec, 0x202625))
            .text_color(self.home.color(0x23211d, 0xece7db))
            .text_size(px(typography::UI_SIZE))
            .line_height(relative(typography::UI_LINE_HEIGHT))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.home.popup.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    if this.home.popup.take().is_some() {
                        this.home.folder_papers = None;
                        this.home.rename_id = None;
                        window.blur(cx);
                        cx.stop_propagation();
                        cx.notify();
                        return;
                    }
                    this.home.adding_folder = false;
                    this.home.batch = false;
                    this.home.selected.clear();
                    window.blur(cx);
                    cx.notify();
                }
                if event.keystroke.key == "enter"
                    && this.home.popup == Some("rename")
                    && this
                        .home
                        .rename_input
                        .read(cx)
                        .focus_handle(cx)
                        .is_focused(window)
                {
                    this.rename_home_file(window, cx);
                    cx.stop_propagation();
                    return;
                }
                if event.keystroke.key == "enter"
                    && (this.home.adding_folder || this.home.popup == Some("new-folder"))
                    && this
                        .home
                        .folder_input
                        .read(cx)
                        .focus_handle(cx)
                        .is_focused(window)
                {
                    this.create_home_folder(window, cx);
                    cx.stop_propagation();
                }
                if event.keystroke.key == "/"
                    && !this
                        .home
                        .rename_input
                        .read(cx)
                        .focus_handle(cx)
                        .is_focused(window)
                    && !this
                        .home
                        .search
                        .read(cx)
                        .focus_handle(cx)
                        .is_focused(window)
                    && !this
                        .home
                        .folder_input
                        .read(cx)
                        .focus_handle(cx)
                        .is_focused(window)
                {
                    this.home.search.read(cx).focus_handle(cx).focus(window, cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let _ = weak.update(cx, |this, _| this.home.bounds = Some(bounds));
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(
                div()
                    .h(px(49.))
                    .flex_shrink_0()
                    .px(px(14.))
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .border_b_1()
                    .border_color(self.home.color(0xe3ddd2, 0x3f4240))
                    .child(
                        div()
                            .w(px(130.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(
                                img(self.home.brand_icon.clone())
                                    .id("home-brand-icon")
                                    .debug_selector(|| "home-brand-icon".into())
                                    .size(px(28.))
                                    .flex_shrink_0()
                                    .object_fit(ObjectFit::Contain),
                            )
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .text_size(px(17.))
                                    .text_color(rgb(0x326879))
                                    .child("Vellio"),
                            ),
                    )
                    .child(
                        div().flex_1().max_w(px(495.)).min_w(px(160.)).child(
                            Input::new(&self.home.search)
                                .h(px(33.))
                                .rounded(px(9.))
                                .bg(self.home.color(0xfbfaf7, 0x2a3030))
                                .border_color(self.home.color(0xe3ddd2, 0x444746))
                                .cleanable(true)
                                .aria_label("Search library")
                                .prefix(icon("icons/search.svg", 15.).text_color(rgb(0x969084)))
                                .suffix(
                                    div()
                                        .px(px(4.))
                                        .rounded(px(3.))
                                        .border_1()
                                        .border_color(self.home.color(0xe3ddd2, 0x444746))
                                        .text_size(px(11.))
                                        .text_color(rgb(0x969084))
                                        .child("/"),
                                ),
                        ),
                    )
                    .child(div().flex_1().min_w_0())
                    .child(
                        div()
                            .flex_shrink_0()
                            .debug_selector(|| "home-import".into())
                            .child(
                                Button::new("home-import")
                                    .small()
                                    .h(px(32.))
                                    .rounded(px(9.))
                                    .icon(icon("icons/plus.svg", 15.))
                                    .label(self.home.text("Import", "导入"))
                                    .bg(rgb(0x326879))
                                    .border_color(rgb(0x326879))
                                    .text_color(rgb(0xfffaf0))
                                    .disabled(self.busy || self.library.is_none())
                                    .on_click(cx.listener(|this, _, _, cx| this.pick(false, cx))),
                            ),
                    )
                    .child(
                        Button::new("home-theme")
                            .ghost()
                            .small()
                            .icon(icon(
                                if self.home.dark() {
                                    "icons/sun.svg"
                                } else {
                                    "icons/moon.svg"
                                },
                                16.,
                            ))
                            .accessibility_label("Toggle home theme")
                            .text_color(self.home.color(0x625e55, 0xc8c2b8))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.home.prefs["dark"] = json!(!this.home.dark());
                                this.home.apply_theme(Some(window), cx);
                                this.save_home_prefs();
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("home-language")
                            .ghost()
                            .small()
                            .label("中")
                            .accessibility_label("Toggle home language")
                            .text_color(self.home.color(0x625e55, 0xc8c2b8))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.home.prefs["zh"] = json!(!this.home.zh());
                                typography::apply_theme(this.home.zh(), cx);
                                this.save_home_prefs();
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("home-help")
                            .ghost()
                            .small()
                            .icon(icon("icons/circle-question-mark.svg", 16.))
                            .accessibility_label("Help")
                            .text_color(self.home.color(0x625e55, 0xc8c2b8))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.home.popup = if this.home.popup == Some("help") {
                                    None
                                } else {
                                    Some("help")
                                };
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("home-settings")
                            .ghost()
                            .small()
                            .icon(icon("icons/settings.svg", 16.))
                            .accessibility_label("Settings")
                            .text_color(self.home.color(0x625e55, 0xc8c2b8))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_settings(window, cx);
                                cx.notify();
                            })),
                    ),
            )
            .child(div().flex_1().min_h_0().flex().child(sidebar).child(main))
            .when_some(popup, |d, popup| d.child(popup))
            .into_any_element()
    }
}
