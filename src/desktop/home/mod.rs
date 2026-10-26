//! Native library home state and shared styling. Reader composition lives in `desktop::view`.
use super::*;
use easyread_gpui::{
    library_home as catalog,
    store::{read_json, write_json},
};
use std::collections::HashSet;

#[cfg(all(test, feature = "desktop-ui-tests"))]
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
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/assets/icons/Vellio-small.png"
                ))
                .to_vec(),
            )),
            search: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Search titles, authors, folders")
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

mod actions;
mod menus;
mod rows;
mod toolbar;
mod view;
