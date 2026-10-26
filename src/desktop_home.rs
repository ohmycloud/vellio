//! Native library home screen. Reader layout stays in desktop.rs.
use super::*;
use easyread_gpui::{
    library_home as catalog,
    store::{read_json, write_json},
};
use std::collections::HashSet;

#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "desktop_home_tests.rs"]
mod tests;

const SORT_OPTIONS: [(&str, &str); 4] = [
    ("Recently opened", "最近阅读"),
    ("Recently added", "最近添加"),
    ("Year published", "出版年份"),
    ("Title", "标题"),
];

#[derive(Clone)]
struct PaperDrag {
    id: String,
    title: String,
}
impl Render for PaperDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(12.))
            .py(px(8.))
            .rounded(px(8.))
            .bg(rgb(0xfbfaf7))
            .text_color(rgb(0x326879))
            .border_1()
            .border_color(rgb(0xd9d1c2))
            .child(self.title.clone())
    }
}

pub(super) struct HomeState {
    pub visible: bool,
    pub(super) cancellable_task: bool,
    pub focus: FocusHandle,
    pub search: Entity<InputState>,
    pub(super) brand_icon: Arc<Image>,
    pub(super) folder_input: Entity<InputState>,
    pub(super) rename_input: Entity<InputState>,
    rename_id: Option<String>,
    pub scroll: ScrollHandle,
    sidebar_scroll: ScrollHandle,
    pub(super) prefs: Value,
    pub(super) filter: String,
    sort: usize,
    pub(super) popup: Option<&'static str>,
    popup_anchors: HashMap<&'static str, Bounds<Pixels>>,
    bounds: Option<Bounds<Pixels>>,
    folder_papers: Option<Vec<String>>,
    adding_folder: bool,
    batch: bool,
    hovering: bool,
    hovered_paper: Option<String>,
    selected: HashSet<String>,
    translation_pages: Option<HashSet<usize>>,
    covers: HashMap<String, Arc<Image>>,
    pub notice: String,
}
impl HomeState {
    pub fn new(
        library: Option<&Library>,
        items: &[Value],
        visible: bool,
        window: &mut Window,
        cx: &mut Context<ReaderView>,
    ) -> Self {
        let mut notice = String::new();
        let mut prefs = library
            .map(|lib| {
                read_json(&lib.root.join(".library-home.json"), json!({})).unwrap_or_else(|e| {
                    notice = format!("首页设置读取失败：{e}");
                    json!({})
                })
            })
            .unwrap_or_else(|| json!({}));
        if !prefs.is_object() {
            prefs = json!({});
        }
        let mut home = Self {
            visible,
            cancellable_task: false,
            focus: cx.focus_handle(),
            prefs,
            notice,
            // Cache the full-colour image once per view; Icon would tint it monochrome.
            brand_icon: Arc::new(Image::from_bytes(
                ImageFormat::Png,
                include_bytes!("../assets/icons/Vellio-small.png").to_vec(),
            )),
            search: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Search titles, authors, folders")
            }),
            folder_input: cx.new(|cx| InputState::new(window, cx).placeholder("Folder name")),
            rename_input: cx.new(|cx| InputState::new(window, cx).placeholder("Name")),
            rename_id: None,
            scroll: ScrollHandle::new(),
            sidebar_scroll: ScrollHandle::new(),
            filter: "all".into(),
            sort: 0,
            popup: None,
            popup_anchors: HashMap::new(),
            bounds: None,
            folder_papers: None,
            adding_folder: false,
            batch: false,
            hovering: false,
            hovered_paper: None,
            selected: HashSet::new(),
            translation_pages: None,
            covers: HashMap::new(),
        };
        home.load_covers(library, items);
        if visible {
            home.apply_theme(Some(window), cx);
        }
        home
    }
    pub fn load_covers(&mut self, library: Option<&Library>, items: &[Value]) {
        let Some(lib) = library else {
            return;
        };
        for item in items {
            let id = item["id"].as_str().unwrap_or("");
            if self.covers.contains_key(id) {
                continue;
            }
            let image = (|| {
                let relative = item["cover"].as_str()?;
                let path = lib.workspace(id).ok()?.asset(relative).ok()?;
                if std::fs::metadata(&path).ok()?.len() > 16 * 1024 * 1024 {
                    return None;
                }
                let format = match path.extension()?.to_str()?.to_lowercase().as_str() {
                    "png" => ImageFormat::Png,
                    "jpg" | "jpeg" => ImageFormat::Jpeg,
                    "webp" => ImageFormat::Webp,
                    _ => return None,
                };
                Some(Arc::new(Image::from_bytes(
                    format,
                    std::fs::read(path).ok()?,
                )))
            })();
            if let Some(image) = image {
                self.covers.insert(id.into(), image);
            }
        }
    }
    fn dark(&self) -> bool {
        self.prefs["dark"].as_bool().unwrap_or(false)
    }
    pub(super) fn apply_theme(&self, window: Option<&mut Window>, cx: &mut App) {
        gpui_kit::component::Theme::change(
            if self.dark() {
                gpui_kit::component::ThemeMode::Dark
            } else {
                gpui_kit::component::ThemeMode::Light
            },
            window,
            cx,
        );
        typography::apply_theme(self.zh(), cx);
    }
    fn zh(&self) -> bool {
        self.prefs["zh"].as_bool().unwrap_or(false)
    }
    pub(super) fn text(&self, en: &'static str, zh: &'static str) -> &'static str {
        if self.zh() { zh } else { en }
    }
    pub(super) fn folders(&self) -> Vec<(String, String)> {
        self.prefs["folders"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|f| Some((f["id"].as_str()?.into(), f["name"].as_str()?.into())))
            .collect()
    }
    pub(super) fn in_folder(&self, id: &str, folder: &str) -> bool {
        self.prefs["assignments"][id]
            .as_array()
            .is_some_and(|fs| fs.iter().any(|f| f == folder))
    }
    fn accepts(&self, item: &Value, filter: &str) -> bool {
        let removed = self.prefs["removed"][item["id"].as_str().unwrap_or("")]
            .as_bool()
            .unwrap_or(false);
        if filter == "trash" {
            return removed;
        }
        if removed {
            return false;
        }
        match filter {
            "all" => true,
            "reading" | "unread" | "done" => item["status"] == filter,
            "starred" => item["item"]["starred"].as_bool().unwrap_or(false),
            "questions" => item["open_questions"].as_u64().unwrap_or(0) > 0,
            "translating" => item["translating"].as_bool().unwrap_or(false),
            folder => self.in_folder(item["id"].as_str().unwrap_or(""), folder),
        }
    }
    fn color(&self, light: u32, dark: u32) -> Rgba {
        rgb(if self.dark() { dark } else { light })
    }
}

fn icon(path: &'static str, size: f32) -> Icon {
    Icon::default().path(path).size(px(size))
}

impl ReaderView {
    pub(super) fn save_home_prefs(&mut self) {
        if let Some(lib) = &self.library
            && let Err(e) = write_json(&lib.root.join(".library-home.json"), &self.home.prefs)
        {
            self.home.notice = format!("首页设置保存失败：{e}");
        }
    }
    pub(super) fn show_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.queue_margin_note();
        self.capture_inputs(cx);
        self.capture_position();
        self.flush_session();
        self.margin_editor = None;
        self.navigation = None;
        self.appearance_open = false;
        self.reading_order_open = false;
        self.selection = None;
        self.annotation_popup = None;
        self.home.visible = true;
        self.home.popup = None;
        self.home.batch = false;
        self.home.hovering = false;
        self.home.hovered_paper = None;
        self.home.selected.clear();
        self.home.apply_theme(Some(window), cx);
        match self.library.as_ref().map(Library::list) {
            Some(Ok(items)) => {
                self.items = items;
                self.home.load_covers(self.library.as_ref(), &self.items);
            }
            Some(Err(e)) => self.home.notice = format!("文献库读取失败：{e}"),
            None => {}
        }
        window.blur(cx);
        cx.notify();
    }
    pub(super) fn create_home_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.home.folder_input.read(cx).value().trim().to_owned();
        if name.is_empty() {
            return;
        }
        if self.home.folders().iter().any(|(_, n)| n == &name) {
            self.home.notice = self
                .home
                .text("A folder with this name already exists", "已有同名文件夹")
                .into();
            cx.notify();
            return;
        }
        if !self.home.prefs["folders"].is_array() {
            self.home.prefs["folders"] = json!([]);
        }
        let folder_id = format!("folder-{}", Uuid::new_v4());
        self.home.prefs["folders"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":folder_id,"name":name}));
        self.save_home_prefs();
        self.home.adding_folder = false;
        if let Some(ids) = self.home.folder_papers.take() {
            self.add_papers_to_folder(&ids, &folder_id, cx);
        }
        self.home
            .folder_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }
    fn open_home_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.home.selected.len() != 1 {
            return;
        }
        let id = self.home.selected.iter().next().unwrap().clone();
        let Some(item) = self.items.iter().find(|item| item["id"] == id) else {
            return;
        };
        let name = catalog::title(item).to_owned();
        self.home.rename_id = Some(id);
        self.home.popup = Some("rename");
        self.home
            .rename_input
            .update(cx, |input, cx| input.set_value(name, window, cx));
        self.home
            .rename_input
            .read(cx)
            .focus_handle(cx)
            .focus(window, cx);
        cx.notify();
    }
    fn rename_home_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.home.rename_input.read(cx).value().trim().to_owned();
        if self.busy || name.is_empty() {
            return;
        }
        let (Some(lib), Some(id)) = (self.library.clone(), self.home.rename_id.clone()) else {
            return;
        };
        self.home.popup = None;
        self.home.rename_id = None;
        self.home.notice.clear();
        window.blur(cx);
        self.worker(cx, move |_, tx| {
            match lib.workspace(&id).and_then(|ws| ws.rename(&name)) {
                Ok(_) => {
                    let _ = tx.send(Event::Catalog(lib.list()));
                }
                Err(e) => {
                    let _ = tx.send(Event::Error(format!("重命名失败：{e}")));
                }
            }
        });
        cx.notify();
    }
    fn home_item_action(
        &mut self,
        ids: Vec<String>,
        field: &'static str,
        value: Value,
        cx: &mut Context<Self>,
    ) {
        let Some(lib) = self.library.clone() else {
            return;
        };
        self.home.notice.clear();
        self.worker(cx, move |cancel, tx| {
            for id in ids {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let result = lib.workspace(&id).and_then(|ws| {
                    if field == "status" {
                        ws.set_reading_status(value.as_str().unwrap_or(""))
                    } else {
                        ws.update("item", |item| {
                            item[field] = value.clone();
                            Ok(())
                        })
                    }
                });
                if let Err(e) = result {
                    let _ = tx.send(Event::Error(format!("文献更新失败：{e}")));
                    break;
                }
            }
            let _ = tx.send(Event::Catalog(lib.list()));
        });
    }
    fn assign_home_folder(&mut self, folder: &str, cx: &mut Context<Self>) {
        let ids = self.home.selected.iter().cloned().collect::<Vec<_>>();
        self.add_papers_to_folder(&ids, folder, cx);
    }
    fn add_papers_to_folder(&mut self, ids: &[String], folder: &str, cx: &mut Context<Self>) {
        if !self.home.prefs["assignments"].is_object() {
            self.home.prefs["assignments"] = json!({});
        }
        for id in ids {
            let mut assigned = self.home.prefs["assignments"][id]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if !assigned.iter().any(|f| f == folder) {
                assigned.push(json!(folder));
            }
            self.home.prefs["assignments"][id] = json!(assigned);
        }
        self.save_home_prefs();
        self.home.popup = None;
        cx.notify();
    }
    fn remove_selected_from_folder(&mut self, cx: &mut Context<Self>) {
        for id in &self.home.selected {
            if let Some(assigned) = self.home.prefs["assignments"][id].as_array_mut() {
                assigned.retain(|folder| folder != &self.home.filter);
            }
        }
        self.save_home_prefs();
        self.home.selected.clear();
        cx.notify();
    }
    fn home_filter_button(
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
    fn home_row(&self, item: &Value, cx: &mut Context<Self>) -> AnyElement {
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
    fn home_action_anchor(
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
    fn set_home_trash(&mut self, removed: bool, cx: &mut Context<Self>) {
        if self.busy || self.home.selected.is_empty() {
            return;
        }
        let previous = self.home.prefs.clone();
        if !self.home.prefs["removed"].is_object() {
            self.home.prefs["removed"] = json!({});
        }
        for id in &self.home.selected {
            self.home.prefs["removed"][id] = json!(removed);
        }
        if let Some(lib) = &self.library
            && let Err(e) = write_json(&lib.root.join(".library-home.json"), &self.home.prefs)
        {
            self.home.prefs = previous;
            self.home.notice = format!("首页设置保存失败：{e}");
            cx.notify();
            return;
        }
        self.home.selected.clear();
        self.home.batch = false;
        self.home.popup = None;
        cx.notify();
    }
    fn home_actions(&self, visible_ids: &[String], cx: &mut Context<Self>) -> AnyElement {
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
    fn translate_home_selection(&mut self, cx: &mut Context<Self>) {
        if self.busy
            || self.home.selected.is_empty()
            || self
                .home
                .translation_pages
                .as_ref()
                .is_some_and(|pages| pages.is_empty() || self.home.selected.len() != 1)
        {
            return;
        }
        let Some(lib) = self.library.clone() else {
            return;
        };
        let config = match self.settings.committed.connection(true) {
            Ok(config) => config,
            Err(e) => {
                self.home.notice = e.to_string();
                cx.notify();
                return;
            }
        };
        let options = ai::TranslationOptions {
            pages_per_batch: self.settings.committed.pages_per_batch,
            parallel: self.settings.committed.parallel,
        };
        let ids = self
            .items
            .iter()
            .filter_map(|item| item["id"].as_str())
            .filter(|id| self.home.selected.contains(*id))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        self.home.popup = None;
        self.home.notice = self
            .home
            .text("Translating selected papers…", "正在翻译所选文献……")
            .into();
        let selected_pages = self
            .home
            .translation_pages
            .take()
            .map(|pages| pages.into_iter().collect::<Vec<_>>());
        let active_root = self.workspace.as_ref().map(|ws| ws.root.clone());
        self.worker(cx, move |cancel, tx| {
            let mut failures = 0;
            for id in ids {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let outcome = lib.workspace(&id).and_then(|ws| {
                    // A trashed paper must not consume model quota or stop the
                    // rest of the library queue (upstream v1.4.1 / #56).
                    let home = read_json(&lib.root.join(".library-home.json"), json!({}))?;
                    if home["removed"][&id].as_bool().unwrap_or(false)
                        || ws.load("item")?["removed"].as_bool().unwrap_or(false)
                    {
                        return Ok(());
                    }
                    let progress = |done, total| {
                        let _ = tx.send(Event::Status(format!("翻译 {done}/{total}")));
                    };
                    let result = if let Some(pages) = &selected_pages {
                        ai::translate_pages_batched(&ws, &config, options, pages, &cancel, progress)
                    } else {
                        ai::translate_batched(&ws, &config, options, &cancel, progress)
                    };
                    if active_root.as_ref() == Some(&ws.root)
                        && let Ok(raw) = ws.state()
                    {
                        let _ = tx.send(Event::Refresh(raw));
                    }
                    result
                });
                if let Err(e) = outcome {
                    failures += 1;
                    let _ = tx.send(Event::Error(format!("文献 {id} 翻译未完成：{e:#}")));
                }
            }
            let _ = tx.send(Event::Catalog(lib.list()));
            let _ = tx.send(if cancel.load(Ordering::Relaxed) {
                Event::Status("翻译已取消；已完成内容保留".into())
            } else if failures > 0 {
                Event::Error(format!(
                    "{failures} 篇文献翻译未完成；其余内容已保存，可重试未译页"
                ))
            } else {
                Event::Status("翻译已保存".into())
            });
        });
        self.home.cancellable_task = true;
        cx.notify();
    }
    fn home_popup(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
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
    pub(super) fn home_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
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
