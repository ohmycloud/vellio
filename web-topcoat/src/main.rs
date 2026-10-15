//! Local-only Topcoat experiment: server-side Vellio parser/storage, DOM reader.
//! Not a public deployment: no accounts, cross-device sync, or complete desktop rendering.
mod math_svg;
mod model_settings;
mod paragraphs;
use easyread_gpui::{
    ai, annotations, formats::epub, formula, library_home, model::ReaderState,
    paragraph_translation::PairLayout, pdf, settings, store::Library, translation_layout,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock, atomic::AtomicBool},
};
use topcoat::{
    Error, Result,
    context::Cx,
    router::{
        BodyLimit, RouterBuilderDiscoverExt,
        content::{Css, Form, Html, Js, Json, multipart::Multipart},
        error::{SeeOther, bad_request, not_found, see_other},
        module_router, page, path_param, query_params, route,
    },
    view::{Unescaped, View, view},
};

static LIBRARY: OnceLock<Library> = OnceLock::new();
static HOME_PREFS_LOCK: Mutex<()> = Mutex::new(());
static TRANSLATIONS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
fn translations() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    TRANSLATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}
fn library() -> &'static Library {
    LIBRARY.get().expect("initialized before serving")
}
// Desktop preferences are read only; the Web library remains separate. The override is useful
// when the desktop was started with --library instead of its remembered/default location.
fn desktop_library_path() -> PathBuf {
    std::env::var_os("VELLIO_DESKTOP_LIBRARY")
        .or_else(|| std::env::var_os("EASYREAD_LIBRARY"))
        .map(PathBuf::from)
        .or_else(settings::saved_library)
        .unwrap_or_else(|| {
            let home_dir = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            #[cfg(target_os = "macos")]
            return home_dir.join("Library/Application Support/EasyReadGPUI/library");
            #[cfg(target_os = "windows")]
            return std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or(home_dir)
                .join("EasyReadGPUI/library");
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            return home_dir.join(".local/share/easyread-gpui/library");
        })
}
fn model_preferences() -> anyhow::Result<settings::Preferences> {
    settings::Preferences::load(Some(&desktop_library_path()))
}
path_param!(book_id);
path_param!(*asset_path);
#[query_params(error = bad_request)]
struct ReadQuery {
    page: Option<usize>,
    mode: Option<String>,
}
#[derive(Deserialize)]
struct NoteInput {
    anchor: String,
    side: String,
    quote: String,
    prefix: String,
    suffix: String,
    kind: String,
    body: String,
    color: String,
    style: String,
    page: usize,
    mode: String,
}
#[derive(Deserialize)]
struct MarginNoteInput {
    note_id: String,
    session_id: String,
    revision: u64,
    anchor: String,
    side: String,
    quote: String,
    prefix: String,
    suffix: String,
    kind: String,
    body: String,
    color: String,
    answered: String,
    source_annotation: String,
    page: usize,
    mode: String,
}
#[derive(Deserialize)]
struct NoteEdit {
    note_id: String,
    kind: String,
    body: String,
    color: String,
    style: String,
    answered: String,
    page: usize,
    mode: String,
}
#[derive(Deserialize)]
struct NoteAction {
    note_id: String,
    action: String,
    page: usize,
    mode: String,
}
#[derive(Deserialize)]
struct PaperNoteInput {
    body: String,
    page: usize,
    mode: String,
}
#[derive(Deserialize)]
struct NotePreviewInput {
    body: String,
}
#[derive(Deserialize)]
struct ProgressInput {
    block: String,
    ratio: f64,
}
#[derive(Deserialize)]
struct TranslationInput {
    block: String,
    paragraph: usize,
    text: String,
    expected_text: String,
    expected_edit: String,
    action: String,
    page: usize,
    mode: String,
}
#[derive(Deserialize)]
struct AiPageInput {
    page: usize,
    mode: String,
    confirm: Option<String>,
}
#[derive(Deserialize)]
struct AskInput {
    text: String,
    anchor: Option<String>,
    quote: Option<String>,
    note_id: Option<String>,
    page: usize,
    mode: String,
}
struct ChatMessage {
    role: String,
    body_html: String,
    failed: bool,
}
#[derive(Clone)]
struct NoteReply {
    by: String,
    body_html: String,
}
#[query_params(error = bad_request)]
struct MathQuery {
    tex: String,
    display: Option<bool>,
}
#[derive(Deserialize)]
struct LibraryAction {
    action: String,
    value: Option<String>,
}
#[derive(Deserialize)]
struct FolderAction {
    action: String,
    folder: Option<String>,
    name: Option<String>,
    book: Option<String>,
}
#[derive(Deserialize)]
struct BatchAction {
    action: String,
    ids: String,
    status: Option<String>,
    folder: Option<String>,
}
#[derive(Clone)]
struct HomeFolder {
    id: String,
    name: String,
}
struct HomeRow {
    id: String,
    title: String,
    details: String,
    status: String,
    starred: bool,
    removed: bool,
    notes: u64,
    questions: u64,
    added: String,
    ratio: u64,
    opened: String,
    year: u64,
    cover: String,
    folders: String,
    folder_names: String,
    citation: String,
}
struct RenderBlock {
    id: String,
    kind: String,
    original: String,
    translated: String,
    src: String,
    untranslated: bool,
    page_block: bool,
    paired: String,
    edit_key: String,
    edit_html: String,
    edit_paragraphs: Vec<paragraphs::Paragraph>,
    edit_hash: String,
    edit_previous: String,
    has_edit: bool,
}
#[derive(Clone)]
struct RenderNote {
    id: String,
    anchor: String,
    side: String,
    prefix: String,
    suffix: String,
    source_annotation: String,
    kind: String,
    quote: String,
    body: String,
    body_html: String,
    created: String,
    color: String,
    style: String,
    answered: bool,
    page: usize,
    replies: Vec<NoteReply>,
}

fn cleaned(html: &str) -> String {
    // Keep only the four application-owned underline colors; never allow arbitrary CSS.
    let mut html = html.to_owned();
    for (name, ink) in [
        ("yellow", "dca722"),
        ("green", "58994c"),
        ("blue", "477fc1"),
        ("pink", "cc413a"),
    ] {
        html = html.replace(
            &format!("<u style=\"text-decoration-color:#{ink}\">"),
            &format!("<u data-underline=\"{name}\">"),
        );
    }
    let mut sanitizer = ammonia::Builder::new();
    sanitizer.url_relative(ammonia::UrlRelative::PassThrough);
    sanitizer.add_tags(&["img", "mark", "u", "span"]);
    sanitizer.add_generic_attributes(&[
        "class",
        "lang",
        "data-read-paragraph",
        "data-edit-paragraph",
    ]);
    sanitizer.add_tag_attributes("img", &["src", "alt", "width", "height"]);
    sanitizer.add_tag_attributes("mark", &["color"]);
    sanitizer.add_tag_attributes("blockquote", &["data-easyread-companion"]);
    sanitizer.add_tag_attributes("u", &["data-underline"]);
    sanitizer.add_tag_attributes(
        "span",
        &["data-easyread-note", "data-vellio-math", "data-display"],
    );
    sanitizer.clean(&html).to_string()
}
fn display_blocks(
    raw: &Value,
    state: &ReaderState,
    notes: &[Value],
    page: usize,
    pdf: bool,
    source_pdf: Option<&std::path::Path>,
) -> Vec<RenderBlock> {
    state
        .paper
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(i, b)| {
            let source = &raw["paper"]["blocks"][i];
            let position = source[if pdf { "page" } else { "chapter" }]
                .as_u64()
                .unwrap_or(1) as usize;
            if position != page
                || !matches!(
                    b.kind.as_str(),
                    "heading" | "para" | "note" | "figure" | "table" | "list" | "math"
                )
            {
                return None;
            }
            let src = source["src"]
                .as_str()
                .filter(|path| path.starts_with("images/") || path.starts_with("pages/"))
                .unwrap_or("")
                .to_owned();
            let original = if b.kind == "math" && !b.tex.is_empty() {
                formula::native_html(&b.tex, true)
            } else if b.kind == "figure" && !src.is_empty() {
                annotations::html(&b.caption_en)
            } else if b.source_kind == "pdf-text" {
                // Desktop's native PDF geometry recovery is reusable here as HTML.
                // Only accept it when its extracted text matches the stored block.
                source_pdf
                    .and_then(|path| pdf::code_html(path, b.page, &b.en).ok().flatten())
                    .unwrap_or_else(|| formula::native_markdown_html(&state.original_markdown(b)))
            } else {
                epub::source_html(source, b, state, true)
                    .unwrap_or_else(|| formula::native_markdown_html(&state.original_markdown(b)))
            };
            let untranslated = b.zh.trim().is_empty() && state.text(&b.id, &b.zh, &b.en) == b.en;
            let active = state.text(&b.id, &b.zh, &b.en);
            let translated = if untranslated || (b.kind == "math" && !b.tex.is_empty()) {
                original.clone()
            } else if b.kind == "figure" && !src.is_empty() {
                annotations::html(state.caption(b))
            } else if b.source_kind == "pdf-text" {
                easyread_gpui::paragraph_edit::edited_html(
                    &raw["reader"]["edits"][&b.id],
                    &b.en,
                    active,
                )
                .map(str::to_owned)
                .or_else(|| translation_layout::saved_html(source, active))
                .or_else(|| translation_layout::legacy_html(&b.en, active, &original, b.page))
                .unwrap_or_else(|| formula::native_markdown_html(&state.markdown(b, false)))
            } else {
                easyread_gpui::paragraph_edit::edited_html(
                    &raw["reader"]["edits"][&b.id],
                    &b.en,
                    active,
                )
                .filter(|_| b.kind != "heading")
                .map(str::to_owned)
                .unwrap_or_else(|| formula::native_markdown_html(&state.markdown(b, false)))
            };
            let edit_key = if matches!(b.kind.as_str(), "figure" | "table") {
                format!("{}#caption", b.id)
            } else {
                b.id.clone()
            };
            let edit_text = if matches!(b.kind.as_str(), "figure" | "table") {
                state.caption(b).to_owned()
            } else {
                state.text(&b.id, &b.zh, &b.en).to_owned()
            };
            let edit_previous = raw["reader"]["edits"]
                .get(&edit_key)
                .cloned()
                .unwrap_or(Value::Null)
                .to_string();
            let has_edit = raw["reader"]["edits"][&edit_key]["zh"].is_string()
                && raw["reader"]["edits"][&edit_key]["reverted"] != true;
            let edit_html = if b.kind == "heading" {
                formula::native_markdown_html(&edit_text)
            } else {
                translated.clone()
            };
            let (original, _) = paragraphs::prepare(&original, None);
            let (translated, mut edit_paragraphs) =
                paragraphs::prepare(&translated, Some(&edit_text));
            // Heading Markdown is wrapped for display, but its stored edit is
            // the heading text, not another nested heading marker.
            if b.kind == "heading" && edit_paragraphs.len() == 1 {
                edit_paragraphs[0].text = edit_text.clone();
            }
            let matching = |side| {
                notes
                    .iter()
                    .filter(|n| n["anchor"] == b.id && n["side"].as_str().unwrap_or("zh") == side)
                    .cloned()
                    .collect::<Vec<_>>()
            };
            let original_decorated = annotations::decorate_interactive(&original, &matching("en"));
            let translated_decorated = annotations::decorate_interactive(
                &translated,
                &matching(if untranslated { "en" } else { "zh" }),
            );
            let page_block = b.source_kind == "pdf-text";
            // Pair the recovered structure BEFORE sanitizing away role-bearing layout
            // attributes. Only the already-sanitized result reaches Unescaped.
            let paired = if !untranslated {
                let footer =
                    b.en.lines()
                        .last()
                        .is_some_and(|line| line.trim() == b.page.to_string());
                PairLayout::for_bilingual(
                    &original_decorated,
                    &translated_decorated,
                    true,
                    (page_block && footer).then_some(b.page),
                )
                .map(|layout| cleaned(&layout.render(&layout.all_pairs())))
                .unwrap_or_default()
            } else {
                String::new()
            };
            let original = cleaned(&original_decorated);
            let translated = cleaned(&translated_decorated);
            Some(RenderBlock {
                id: b.id.clone(),
                kind: b.kind.clone(),
                original,
                translated,
                src,
                untranslated,
                page_block,
                paired,
                edit_hash: easyread_gpui::model::text_hash(&edit_text),
                edit_html,
                edit_paragraphs,
                edit_key,
                edit_previous,
                has_edit,
            })
        })
        .collect()
}

#[page]
async fn home() -> Result<impl View> {
    let books = tokio::task::spawn_blocking(|| library().list())
        .await?
        .map_err(Error::from_anyhow)?;
    let prefs =
        easyread_gpui::store::read_json(&library().root.join(".library-home.json"), json!({}))
            .unwrap_or_else(|_| json!({}));
    let folders: Vec<HomeFolder> = prefs["folders"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            Some(HomeFolder {
                id: item["id"].as_str()?.to_owned(),
                name: item["name"].as_str()?.to_owned(),
            })
        })
        .filter(|folder| {
            !prefs["hidden_folders"][&folder.id]
                .as_bool()
                .unwrap_or(false)
        })
        .collect();
    let mut books = books;
    books.sort_by(|a, b| library_home::compare_for_sort(a, b, 0));
    let rows: Vec<HomeRow> = books
        .iter()
        .filter_map(|b| {
            let id = b["id"].as_str()?.to_owned();
            let authors = b["authors"].as_str().unwrap_or("");
            let year = library_home::published_year(b).unwrap_or(0);
            let venue = b["meta"]["venue"]
                .as_str()
                .or_else(|| b["meta"]["publication"].as_str())
                .unwrap_or("");
            let details = [
                authors.to_owned(),
                if year > 0 {
                    year.to_string()
                } else {
                    String::new()
                },
                venue.to_owned(),
            ]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
            Some(HomeRow {
                title: library_home::title(b).to_owned(),
                status: b["status"].as_str().unwrap_or("unread").into(),
                starred: b["item"]["starred"].as_bool().unwrap_or(false),
                removed: prefs["removed"][&id].as_bool().unwrap_or(false),
                notes: b["notes"].as_u64().unwrap_or(0),
                questions: b["open_questions"].as_u64().unwrap_or(0),
                added: b["item"]["added"].as_str().unwrap_or("").into(),
                ratio: (b["ratio"].as_f64().unwrap_or(0.) * 100.).round() as u64,
                opened: b["opened"].as_str().unwrap_or("").into(),
                cover: b["cover"]
                    .as_str()
                    .filter(|s| {
                        (s.starts_with("pages/") || s.starts_with("images/"))
                            && library()
                                .workspace(&id)
                                .ok()
                                .and_then(|book| book.asset(s).ok())
                                .is_some()
                    })
                    .unwrap_or("")
                    .into(),
                year,
                details,
                folders: prefs["assignments"][&id]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" "),
                citation: library_home::citation(b),
                folder_names: prefs["assignments"][&id]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|folder_id| {
                        prefs["folders"]
                            .as_array()?
                            .iter()
                            .find(|f| f["id"] == *folder_id)?["name"]
                            .as_str()
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_lowercase(),
                id,
            })
        })
        .collect();
    Ok(view! {
        <!DOCTYPE html>
        <html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>"Vellio · 文献库"</title><link rel="stylesheet" href="/reader.css"><script src="/home.js" defer="defer"></script></head>
        <body class="library-home"><div class="home-layout">
            <aside class="home-sidebar"><div class="home-brand">"▤ "<strong>"Vellio"</strong></div><p class="eyebrow">"F O L D E R S"</p>
                <nav aria-label="书库筛选"><button class="active" data-filter="all">"▤　全部文献"</button><button data-filter="reading">"◐　在读"</button><button data-filter="unread">"○　未读"</button><button data-filter="done">"✓　已读"</button><button data-filter="starred">"☆　收藏"</button><button data-filter="questions">"?　待解答问题"</button><button data-filter="trash">"⌑　回收站"</button></nav>
                <p class="eyebrow folder-heading">"文件夹"</p><nav class="folder-nav" aria-label="自定义文件夹">
                for folder in folders.clone() { <div class="folder-entry"><button type="button" data-filter=(folder.id.clone()) data-folder-drop=(folder.id.clone())>(folder.name.clone())</button>
                    <details class="folder-more"><summary aria-label=(format!("管理文件夹 {}",folder.name))>"···"</summary><div>
                        <form method="post" action="/library/folders"><input type="hidden" name="action" value="rename"><input type="hidden" name="folder" value=(folder.id.clone())><input name="name" value=(folder.name) maxlength="80" required=(true) aria-label="文件夹新名称"><button>"重命名"</button></form>
                        <form method="post" action="/library/folders"><input type="hidden" name="action" value="delete"><input type="hidden" name="folder" value=(folder.id)><button>"删除文件夹"</button></form>
                    </div></details>
                </div> }
                </nav><details class="add-folder"><summary>"＋ 新建文件夹"</summary><form method="post" action="/library/folders"><input type="hidden" name="action" value="create"><input name="name" maxlength="80" required=(true) placeholder="文件夹名称" aria-label="文件夹名称"><button>"创建"</button></form></details>
                <p class="sidebar-hint">"拖动或批量归档文献；模型设置见右上角。"</p>
            </aside>
            <main class="home-main"><header class="home-top"><h1>"全部文献"</h1><span id="home-count"></span><span class="spacer"></span>
                <a class="home-model-link" href="/settings" aria-label="打开模型设置">"⚙ 模型设置"</a>
                <input id="library-search" type="search" placeholder="搜索标题、作者…" aria-label="搜索文献">
                <label>"排序 "<select id="library-sort"><option value="recent">"最近阅读"</option><option value="added">"最近添加"</option><option value="year">"发表年份"</option><option value="title">"标题"</option></select></label>
                <form class="upload-compact" method="post" action="/import" enctype="multipart/form-data"><label class="import-label">"＋ 导入 PDF / EPUB"<input type="file" name="book" accept=".pdf,.epub,application/pdf,application/epub+zip" required=(true)></label><button type="submit">"导入"</button></form>
            </header><p class="home-warning">"本机实验书库；不要公开服务或与桌面程序同时修改同一本书。"</p>
            <form id="batch-form" class="batch-bar" method="post" action="/library/batch"><input type="hidden" name="ids" value="">
                <label class="batch-master"><input type="checkbox" id="batch-select-all" aria-label="全选或取消当前文献"></label><strong id="batch-count">"0 篇已选"</strong><span class="spacer"></span>
                if !folders.is_empty() { <select name="folder" aria-label="批量选择文件夹"><option value="">"文件夹"</option>for folder in folders.clone() { <option value=(folder.id)>(folder.name)</option> }</select><button type="submit" name="action" value="assign">"加入收藏夹"</button><button type="submit" name="action" value="remove-folder">"移出文件夹"</button> }
                <select name="status" aria-label="批量阅读状态"><option value="reading">"在读"</option><option value="unread">"未读"</option><option value="done">"已读"</option></select><button type="submit" name="action" value="status">"设置状态"</button>
                <button type="button" data-batch-cite="true">"复制引文"</button>
                <details class="batch-confirm"><summary>"移除所选"</summary><button type="submit" name="action" value="trash">"确认移入回收站"</button></details>
                <button type="submit" name="action" value="restore" class="batch-restore" hidden=(true)>"恢复所选"</button>
            </form>
            <div id="home-list" class="home-list">
            for book in rows { <article class="home-row" draggable="true" data-book=(book.id.clone()) data-folders=(book.folders) data-folder-names=(book.folder_names) data-citation=(book.citation) data-title=(book.title.to_lowercase()) data-details=(book.details.to_lowercase()) data-status=(book.status.clone()) data-starred=(book.starred.to_string()) data-removed=(book.removed.to_string()) data-questions=(book.questions.to_string()) data-opened=(book.opened) data-added=(book.added) data-year=(book.year.to_string())>
                <input class="row-select" type="checkbox" value=(book.id.clone()) aria-label=(format!("选择 {}", book.title))>
                <a class="book-cover" href=(format!("/read/{}", book.id)) aria-label=(format!("打开 {}", book.title))>if !book.cover.is_empty() { <img src=(format!("/read/{}/{}", book.id, book.cover)) alt=""> } else { <span>"▤"</span> }</a>
                <div class="home-row-content"><div class="home-row-head"><a class="row-title" href=(format!("/read/{}", book.id))>(book.title.clone())</a><span class="status">(if book.status == "done" { "已读" } else if book.status == "reading" { "在读" } else { "未读" })</span></div>
                    <div class="row-details">(book.details)</div><div class="row-bottom"><span>(format!("进度 {}%",book.ratio))</span><span>(format!("{} 条批注",book.notes))</span></div><div class="row-progress"><span style=(format!("width:{}%",book.ratio.min(100)))></span></div>
                    <div class="row-actions"><form method="post" action=(format!("/library/{}/action",book.id))><input type="hidden" name="action" value="star"><button title="收藏">(if book.starred { "★ 已收藏" } else { "☆ 收藏" })</button></form>
                        <form method="post" action=(format!("/library/{}/action",book.id))><input type="hidden" name="action" value="status"><select name="value" aria-label="阅读状态"><option value="unread" selected=(book.status=="unread")>"未读"</option><option value="reading" selected=(book.status=="reading")>"在读"</option><option value="done" selected=(book.status=="done")>"已读"</option></select><button>"更新"</button></form>
                        <form method="post" action=(format!("/library/{}/action",book.id))><input type="hidden" name="action" value="rename"><input name="value" aria-label="书籍名称" value=(book.title) maxlength="160" required=(true)><button>"改名"</button></form>
                        <form method="post" action=(format!("/library/{}/action",book.id))><input type="hidden" name="action" value="trash"><button>(if book.removed { "移回书库" } else { "移入回收站" })</button></form>
                        if !folders.is_empty() { <form method="post" action="/library/folders" class="row-folder-action"><input type="hidden" name="book" value=(book.id)><select name="folder" required=(true) aria-label="选择文件夹"><option value="" selected=(true) disabled=(true)>"文件夹"</option>for folder in folders.clone() { <option value=(folder.id)>(folder.name)</option> }</select><button type="submit" name="action" value="assign">"加入"</button><button type="submit" name="action" value="remove">"移出"</button></form> }
                    </div>
                </div>
            </article> }
            </div><p id="home-empty" hidden=(true)>"当前筛选没有文献。"</p>
            </main></div></body></html>
    })
}

#[route(POST "/library/batch")]
async fn library_batch(Form(input): Form<BatchAction>) -> Result<SeeOther> {
    let ids: Vec<_> = input.ids.split(',').filter(|id| !id.is_empty()).collect();
    if ids.is_empty() || ids.len() > 200 {
        return Err(bad_request("请选择 1 至 200 篇文献").into());
    }
    let books = ids
        .iter()
        .map(|id| library().workspace(id))
        .collect::<anyhow::Result<Vec<_>>>()
        .map_err(|_| not_found())?;
    match input.action.as_str() {
        "status" => {
            let status = input.status.as_deref().unwrap_or("");
            if !matches!(status, "unread" | "reading" | "done") {
                return Err(bad_request("状态无效").into());
            }
            for book in books {
                book.update("item", |item| {
                    item["status"] = json!(status);
                    Ok(())
                })
                .map_err(Error::from_anyhow)?;
            }
            Ok(see_other("/"))
        }
        "assign" | "remove-folder" | "trash" | "restore" => {
            let _lock = HOME_PREFS_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let path = library().root.join(".library-home.json");
            let mut prefs =
                easyread_gpui::store::read_json(&path, json!({})).map_err(Error::from_anyhow)?;
            if !prefs.is_object() {
                return Err(bad_request("书库数据无效").into());
            }
            if matches!(input.action.as_str(), "assign" | "remove-folder") {
                let folder = input.folder.as_deref().unwrap_or("");
                if !prefs["folders"]
                    .as_array()
                    .is_some_and(|folders| folders.iter().any(|f| f["id"] == folder))
                {
                    return Err(bad_request("文件夹不存在").into());
                }
                if !prefs["assignments"].is_object() {
                    prefs["assignments"] = json!({});
                }
                for id in ids {
                    let mut current = prefs["assignments"][id]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    if input.action == "assign" {
                        if !current.iter().any(|f| f == folder) {
                            current.push(json!(folder));
                        }
                    } else {
                        current.retain(|f| f != folder);
                    }
                    prefs["assignments"][id] = json!(current);
                }
                easyread_gpui::store::write_json(&path, &prefs).map_err(Error::from_anyhow)?;
                Ok(see_other(format!("/?filter={folder}")))
            } else {
                if !prefs["removed"].is_object() {
                    prefs["removed"] = json!({});
                }
                for id in ids {
                    prefs["removed"][id] = json!(input.action == "trash");
                }
                easyread_gpui::store::write_json(&path, &prefs).map_err(Error::from_anyhow)?;
                Ok(see_other(if input.action == "restore" {
                    "/?filter=trash"
                } else {
                    "/"
                }))
            }
        }
        _ => Err(bad_request("批量操作无效").into()),
    }
}

#[route(POST "/library/folders")]
async fn folder_action(Form(input): Form<FolderAction>) -> Result<SeeOther> {
    let _lock = HOME_PREFS_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let path = library().root.join(".library-home.json");
    let mut prefs =
        easyread_gpui::store::read_json(&path, json!({})).map_err(Error::from_anyhow)?;
    if !prefs.is_object() {
        return Err(bad_request("文件夹数据无效").into());
    }
    if !prefs["folders"].is_array() {
        prefs["folders"] = json!([]);
    }
    if !prefs["assignments"].is_object() {
        prefs["assignments"] = json!({});
    }
    let name = input.name.as_deref().unwrap_or("").trim();
    let folder = input.folder.as_deref().unwrap_or("");
    let has_folder = prefs["folders"]
        .as_array()
        .is_some_and(|folders| folders.iter().any(|f| f["id"] == folder));
    let redirect_folder = match input.action.as_str() {
        "create" => {
            if name.is_empty()
                || name.len() > 80
                || name.chars().any(char::is_control)
                || prefs["folders"]
                    .as_array()
                    .is_some_and(|folders| folders.iter().any(|f| f["name"] == name))
            {
                return Err(bad_request("文件夹名称无效或已存在").into());
            }
            let id = format!("folder-{}", uuid::Uuid::new_v4());
            prefs["folders"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":id,"name":name}));
            Some(id)
        }
        "rename" => {
            if !has_folder
                || name.is_empty()
                || name.len() > 80
                || name.chars().any(char::is_control)
                || prefs["folders"].as_array().is_some_and(|folders| {
                    folders
                        .iter()
                        .any(|f| f["name"] == name && f["id"] != folder)
                })
            {
                return Err(bad_request("文件夹名称无效或已存在").into());
            }
            for item in prefs["folders"].as_array_mut().unwrap() {
                if item["id"] == folder {
                    item["name"] = json!(name);
                }
            }
            Some(folder.to_owned())
        }
        "delete" => {
            if !has_folder {
                return Err(bad_request("文件夹不存在").into());
            }
            prefs["folders"]
                .as_array_mut()
                .unwrap()
                .retain(|f| f["id"] != folder);
            if let Some(assigned) = prefs["assignments"].as_object_mut() {
                for ids in assigned.values_mut() {
                    if let Some(ids) = ids.as_array_mut() {
                        ids.retain(|id| id != folder);
                    }
                }
            }
            if let Some(pinned) = prefs["pinned_folders"].as_object_mut() {
                pinned.remove(folder);
            }
            if let Some(hidden) = prefs["hidden_folders"].as_object_mut() {
                hidden.remove(folder);
            }
            None
        }
        "assign" | "remove" => {
            if !has_folder {
                return Err(bad_request("文件夹不存在").into());
            }
            let id = input.book.as_deref().unwrap_or("");
            library().workspace(id).map_err(|_| not_found())?;
            let current = prefs["assignments"][id]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut assigned = current;
            if input.action == "assign" {
                if !assigned.iter().any(|f| f == folder) {
                    assigned.push(json!(folder));
                }
            } else {
                assigned.retain(|f| f != folder);
            }
            prefs["assignments"][id] = json!(assigned);
            Some(folder.to_owned())
        }
        _ => return Err(bad_request("文件夹操作无效").into()),
    };
    easyread_gpui::store::write_json(&path, &prefs).map_err(Error::from_anyhow)?;
    Ok(see_other(redirect_folder.map_or_else(
        || "/".to_owned(),
        |id| format!("/?filter={id}"),
    )))
}

#[route(POST "/library/{book_id}/action")]
async fn library_action(cx: &Cx, Form(input): Form<LibraryAction>) -> Result<SeeOther> {
    let id = path_param::<BookId>(cx).to_string();
    let book = library().workspace(&id).map_err(|_| not_found())?;
    match input.action.as_str() {
        "star" => {
            book.update("item", |item| {
                item["starred"] = json!(!item["starred"].as_bool().unwrap_or(false));
                Ok(())
            })
            .map_err(Error::from_anyhow)?;
        }
        "status" => {
            let status = input.value.as_deref().unwrap_or("");
            if !matches!(status, "unread" | "reading" | "done") {
                return Err(bad_request("状态无效").into());
            }
            book.update("item", |item| {
                item["status"] = json!(status);
                Ok(())
            })
            .map_err(Error::from_anyhow)?;
        }
        "rename" => {
            let name = input.value.as_deref().unwrap_or("").trim();
            if name.len() > 160 {
                return Err(bad_request("书名过长").into());
            }
            book.rename(name).map_err(Error::from_anyhow)?;
        }
        "trash" => {
            let _lock = HOME_PREFS_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let prefs_path = library().root.join(".library-home.json");
            let mut prefs = easyread_gpui::store::read_json(&prefs_path, json!({}))
                .map_err(Error::from_anyhow)?;
            if !prefs["removed"].is_object() {
                prefs["removed"] = json!({});
            }
            prefs["removed"][id.as_str()] =
                json!(!prefs["removed"][id.as_str()].as_bool().unwrap_or(false));
            easyread_gpui::store::write_json(&prefs_path, &prefs).map_err(Error::from_anyhow)?;
        }
        _ => return Err(bad_request("操作无效").into()),
    }
    Ok(see_other("/"))
}

#[route(POST "/import")]
async fn import(mut multipart: Multipart) -> Result<SeeOther> {
    let mut upload = None;
    while let Some(field) = multipart.next_field().await? {
        if field.name() != Some("book") {
            continue;
        }
        // Only use a short, harmless stem for the display name; never use a
        // browser-provided filename as a filesystem path.
        let stem = field
            .file_name()
            .unwrap_or("document")
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("document")
            .rsplit_once('.')
            .map(|(name, _)| name)
            .unwrap_or("document")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == ' ')
            .take(100)
            .collect::<String>();
        upload = Some((
            field.bytes().await?,
            if stem.is_empty() {
                "document".into()
            } else {
                stem
            },
        ));
        break;
    }
    let (bytes, stem) = upload.ok_or_else(|| bad_request("请选择 PDF 或 EPUB 文件"))?;
    let extension = if bytes.starts_with(b"%PDF") {
        "pdf"
    } else if bytes.starts_with(b"PK\x03\x04") {
        "epub"
    } else {
        return Err(bad_request("文件不是 PDF 或 EPUB").into());
    };
    let id = tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join(format!("{stem}.{extension}"));
        fs::write(&path, bytes)?;
        Ok(library().import_file(&path, None)?.id())
    })
    .await?
    .map_err(Error::from_anyhow)?;
    Ok(see_other(format!("/read/{id}")))
}

#[page("/read/{book_id}")]
async fn read(cx: &Cx) -> Result<impl View> {
    let id = path_param::<BookId>(cx).to_owned();
    let query = query_params::<ReadQuery>(cx)?;
    let mode = match query.mode.as_deref() {
        Some("original") => "original",
        Some("translation") => "translation",
        _ => "bilingual",
    }
    .to_owned();
    let book = library().workspace(&id).map_err(|_| not_found())?;
    let raw = tokio::task::spawn_blocking({
        let book = book.clone();
        move || book.validated_state()
    })
    .await?
    .map_err(Error::from_anyhow)?;
    let state = ReaderState::from_value(raw.clone()).map_err(Error::from_anyhow)?;
    let pdf = raw["paper"]["meta"]["format"] != "epub";
    let wanted = query.page.unwrap_or_else(|| {
        let saved = raw["reader"]["progress"]["block"].as_str().unwrap_or("");
        raw["paper"]["blocks"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|b| b["id"] == saved)
            .and_then(|b| b[if pdf { "page" } else { "chapter" }].as_u64())
            .unwrap_or(1) as usize
    });
    let total = if pdf {
        raw["paper"]["meta"]["page_count"].as_u64().unwrap_or(1)
    } else {
        raw["paper"]["meta"]["chapter_count"].as_u64().unwrap_or(1)
    }
    .max(1) as usize;
    let number = wanted.clamp(1, total);
    let title = state.title().to_owned();
    let model_prefs = model_preferences();
    let ask_available = model_prefs
        .as_ref()
        .is_ok_and(|prefs| prefs.features.ask_ai && prefs.connection(false).is_ok());
    let web_appearance = model_prefs
        .as_ref()
        .map(|prefs| prefs.appearance.clone())
        .unwrap_or_default();
    let web_font = match web_appearance.reading_font() {
        easyread_gpui::session::ReadingFont::NotoSerifSc => "noto",
        easyread_gpui::session::ReadingFont::SystemSerif => "system",
        easyread_gpui::session::ReadingFont::SansSerif => "sans",
    };
    let translating = translations()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .contains_key(&id.to_string());
    let job_state = raw["job"]["state"].as_str().unwrap_or("idle").to_owned();
    let job_progress = format!(
        "翻译 {}/{}",
        raw["job"]["done"].as_u64().unwrap_or(0),
        raw["job"]["total"].as_u64().unwrap_or(0)
    );
    let chat_messages: Vec<ChatMessage> = ai::normalize_chat_state(raw["chat"].clone())
        .ok()
        .and_then(|chat| {
            chat["threads"]
                .as_array()?
                .iter()
                .find(|thread| thread["id"] == "web-reader")
                .cloned()
        })
        .and_then(|thread| thread["messages"].as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|message| {
            let role = message["role"].as_str()?.to_owned();
            if role != "assistant" && role != "user" {
                return None;
            }
            let failed = message["status"] == "error";
            let body = message["content"].as_str().unwrap_or("");
            Some(ChatMessage {
                role,
                body_html: if failed && body.trim().is_empty() {
                    "<p>模型请求失败，请检查本机服务日志和模型配置后重试。</p>".to_owned()
                } else {
                    cleaned(&formula::native_markdown_html(body))
                },
                failed,
            })
        })
        .collect();
    let notes: Vec<_> = raw["reader"]["notes"]
        .as_object()
        .into_iter()
        .flat_map(|o| o.values())
        .filter(|n| !n["deleted"].as_bool().unwrap_or(false))
        .cloned()
        .collect();
    let source_pdf = if pdf {
        book.asset("source.pdf").ok()
    } else {
        None
    };
    let blocks = tokio::task::spawn_blocking({
        let raw = raw.clone();
        let state = state.clone();
        let notes = notes.clone();
        move || display_blocks(&raw, &state, &notes, number, pdf, source_pdf.as_deref())
    })
    .await?;
    let paper_blocks = raw["paper"]["blocks"].as_array();
    let mut all_notes: Vec<_> = notes
        .iter()
        .filter_map(|n| {
            let anchor = n["anchor"].as_str()?;
            let index = paper_blocks?
                .iter()
                .position(|block| block["id"] == anchor)?;
            let page = paper_blocks?[index][if pdf { "page" } else { "chapter" }]
                .as_u64()
                .unwrap_or(1) as usize;
            Some((
                index,
                RenderNote {
                    id: n["id"].as_str().unwrap_or("").into(),
                    anchor: anchor.into(),
                    side: n["side"].as_str().unwrap_or("zh").into(),
                    prefix: n["prefix"].as_str().unwrap_or("").into(),
                    suffix: n["suffix"].as_str().unwrap_or("").into(),
                    source_annotation: n["source_annotation"].as_str().unwrap_or("").into(),
                    kind: n["kind"].as_str().unwrap_or("note").into(),
                    quote: n["quote"].as_str().unwrap_or("").into(),
                    body: n["body"].as_str().unwrap_or("").into(),
                    body_html: cleaned(&formula::native_markdown_html(
                        n["body"].as_str().unwrap_or(""),
                    )),
                    created: n["created"].as_str().unwrap_or("").into(),
                    color: n["color"].as_str().unwrap_or("yellow").into(),
                    style: n["style"].as_str().unwrap_or("highlight").into(),
                    answered: n["answered"].as_bool().unwrap_or(false),
                    replies: raw["discussion"]["entries"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|entry| {
                            entry["reply_to"] == n["id"]
                                && entry["kind"] == "reply"
                                && entry["live"] == true
                                && entry["deleted"] != true
                        })
                        .map(|entry| NoteReply {
                            by: entry["by"].as_str().unwrap_or("AI").to_owned(),
                            body_html: cleaned(&formula::native_markdown_html(
                                entry["body"].as_str().unwrap_or(""),
                            )),
                        })
                        .collect(),
                    page,
                },
            ))
        })
        .collect();
    all_notes.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.created.cmp(&b.1.created)));
    let visible_notes: Vec<_> = all_notes
        .iter()
        .filter(|(_, n)| blocks.iter().any(|b| b.id == n.anchor))
        .map(|(_, n)| n.clone())
        .collect();
    let drawer_notes: Vec<_> = all_notes.into_iter().map(|(_, n)| n).collect();
    let paper_note = raw["reader"]["paper_note"]["body"]
        .as_str()
        .unwrap_or("")
        .to_owned();
    let position = format!("{} {} / {}", if pdf { "第" } else { "章节" }, number, total);
    let prev = format!(
        "/read/{id}?page={}&mode={mode}",
        number.saturating_sub(1).max(1)
    );
    let next = format!("/read/{id}?page={}&mode={mode}", (number + 1).min(total));
    let notes_action = format!("/read/{id}/notes");
    let pdf_page = format!("/read/{id}/pages/page-{number:03}.webp");
    let mode_class = format!("paper mode-{mode} original-first");
    let contents = (number.saturating_sub(5).max(1)..=(number + 5).min(total)).collect::<Vec<_>>();
    let headings: Vec<_> = state
        .paper
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.kind == "heading")
        .filter_map(|(index, b)| {
            let page = raw["paper"]["blocks"][index][if pdf { "page" } else { "chapter" }]
                .as_u64()? as usize;
            Some((
                b.id.clone(),
                state
                    .text(&b.id, &b.zh, &b.en)
                    .chars()
                    .take(120)
                    .collect::<String>(),
                page,
            ))
        })
        .take(500)
        .collect();
    Ok(view! {
        <!DOCTYPE html>
        <html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
            <title>(title.clone())" · Vellio"</title><link rel="stylesheet" href="/reader.css"><link rel="stylesheet" href="/margin-editor.css"><script src="/reader-updates.js" defer="defer"></script><script src="/margin-editor.js" defer="defer"></script><script src="/reader.js" defer="defer"></script>
        </head><body>
            <nav class="top" aria-label="阅读工具栏">
                <a class="library-button icon-button" href="/" title="文献库" aria-label="文献库">"▤"</a>
                <details class="contents-menu"><summary class="icon-button" title="目录与页码" aria-label="目录与页码">"☷"</summary>
                    <div class="contents-popover"><strong>(if pdf { "页码" } else { "章节" })</strong>
                        <div class="contents-pages">
                            if number > 6 { <a href=(format!("/read/{id}?page=1&mode={mode}"))>"1"</a><span>"…"</span> }
                            for p in contents { <a class=(if p == number { "current" } else { "" }) href=(format!("/read/{id}?page={p}&mode={mode}"))>(p)</a> }
                            if number + 5 < total { <span>"…"</span><a href=(format!("/read/{id}?page={total}&mode={mode}"))>(total)</a> }
                        </div>
                        if !headings.is_empty() { <div class="contents-sections">for (anchor, label, p) in headings { <a href=(format!("/read/{id}?page={p}&mode={mode}#block-{anchor}"))><small>(if pdf { "p." } else { "章节" })(p)</small>(label)</a> }</div> }
                        <form method="get" action=(format!("/read/{id}"))><input type="hidden" name="mode" value=(mode.clone())><label>"跳转到 "<input type="number" name="page" min="1" max=(total.to_string()) value=(number.to_string())></label><button type="submit">"前往"</button></form>
                    </div>
                </details>
                <span class="book-title" title=(title.clone())>(title)</span><span class="location-label">(position)</span><span class="spacer"></span>
                <div class="page-nav"><a class="icon-button" href=(prev) aria-label="上一页或章节" aria-disabled=(number <= 1)>"‹"</a><a class="icon-button" href=(next) aria-label="下一页或章节" aria-disabled=(number >= total)>"›"</a></div>
                <div class="mode-control" role="group" aria-label="阅读模式">
                    for (name, label) in [("original", "原文"), ("translation", "译文"), ("bilingual", "双语")] {
                        <a class=(if mode == name { "selected" } else { "" }) aria-current=(if mode == name { "page" } else { "false" }) href=(format!("/read/{id}?page={number}&mode={name}"))>(label)</a>
                    }
                    <details class="order-menu"><summary aria-label="双语显示顺序" title="双语显示顺序">"⌄"</summary><div class="order-options"><button type="button" data-reading-order="source" aria-pressed="true">"原文在前"</button><button type="button" data-reading-order="translation" aria-pressed="false">"译文在前"</button></div></details>
                </div>
                if pdf { <button type="button" class="tool-button" id="source-toggle" aria-expanded="false" aria-controls="source-panel">"▣ PDF"</button> }
                <button type="button" class="tool-button" id="notes-toggle" aria-expanded="false" aria-controls="notes-drawer">"▤ Notes"</button>
                if ask_available { <button class="tool-button" type="button" id="chat-toggle" aria-expanded="false" aria-controls="chat-panel">"✳ Ask AI"</button> }
                if translating { <form class="translate-action" method="post" action=(format!("/read/{id}/translate/cancel"))><input type="hidden" name="page" value=(number)><input type="hidden" name="mode" value=(mode.clone())><button class="tool-button">"取消翻译"</button></form> }
                <span id="job-status" class="job-status" data-active=(translating.to_string()) data-state=(job_state.clone())>(if translating { job_progress } else if job_state == "partial" { "翻译中断，可重试".to_owned() } else { String::new() })</span>
                <details class="appearance-menu"><summary class="tool-button" title="阅读外观">"Aa"</summary>
                    <div class="appearance-popover"><strong>"阅读外观"</strong>
                        <label>"字体"<select id="reader-font"><option value="noto">"宋体"</option><option value="system">"系统衬线"</option><option value="sans">"无衬线"</option></select></label>
                        <label>"字号"<input id="reader-size" type="range" min="14" max="32" value="21"><output for="reader-size">"21"</output></label>
                        <label>"行宽"<input id="reader-width" type="range" min="25" max="60" value="35"><output for="reader-width">"35"</output></label>
                        <label>"行距"<input id="reader-leading" type="range" min="12" max="26" value="19"><output for="reader-leading">"1.9"</output></label>
                        <label class="check"><input id="reader-margin" type="checkbox" checked=(true)>"显示页边笔记"</label>
                    </div>
                </details>
            </nav>
            <div class="reading-progress" role="progressbar" aria-label="阅读进度" aria-valuemin="0" aria-valuemax="100" aria-valuenow="0"><span></span></div>
            <div class="reader-body" data-book-id=(id.to_string()) data-page=(number.to_string()) data-total=(total.to_string()) data-default-font=(web_font) data-default-size=(web_appearance.font_size.to_string()) data-default-width=(web_appearance.line_width.to_string()) data-default-leading=((web_appearance.line_spacing*10.).round().to_string()) data-default-margin=(web_appearance.margin_notes.to_string())><div class="stage"><div class="left-space"></div><article class=(mode_class)>
                for b in blocks {
                    <section class=(format!("blk blk-{}{}{}", b.kind, if b.untranslated { " untranslated" } else { "" }, if b.paired.is_empty() { "" }else{ " paired" })) id=(format!("block-{}", b.id)) data-id=(b.id) data-page-block=(b.page_block.to_string())>
                        if !b.src.is_empty() { <img src=(b.src) alt="文献插图"> }
                        <div class="translated">(Unescaped::new_unchecked(b.translated))</div>
                        <div class="source" lang="en">(Unescaped::new_unchecked(b.original))</div>
                        if b.page_block && !b.untranslated && b.paired.is_empty() { <small class="alignment-warning">"本页原文与译文结构不匹配，无法可靠地逐段对应；保留整页对照。"</small> }
                        if !b.paired.is_empty() { <div class="paired-flow" hidden=(true)>(Unescaped::new_unchecked(b.paired))</div> }
                        if matches!(b.kind.as_str(), "para" | "heading" | "note" | "figure" | "table") {
                            for paragraph in b.edit_paragraphs {
                                <form class="translation-editor" data-paragraph=(paragraph.index.to_string()) hidden=(true) method="post" action=(format!("/read/{id}/translation"))>
                                    <input type="hidden" name="block" value=(b.edit_key.clone())><input type="hidden" name="paragraph" value=(paragraph.index.to_string())><input type="hidden" name="expected_text" value=(b.edit_hash.clone())><input type="hidden" name="expected_edit" value=(b.edit_previous.clone())>
                                    <input type="hidden" name="page" value=(number)><input type="hidden" name="mode" value=(mode.clone())>
                                    <label>"修订该段译文"<textarea name="text" maxlength="64000" spellcheck="false">(paragraph.text)</textarea></label>
                                    <div class="translation-actions"><button type="button" data-translation-cancel="true">"取消"</button><button type="submit" name="action" value="revert" disabled=(!b.has_edit)>"还原该段"</button><button type="submit" name="action" value="save" class="save">"保存译文"</button></div>
                                </form>
                            }
                        }
                    </section>
                }
            </article><div class="source-gutter"></div><aside class="margin" aria-label="页边笔记">
                for n in visible_notes.clone() {
                    <div class=(format!("note-card {}", n.kind)) data-note=(n.id.clone()) data-anchor=(n.anchor) data-side=(n.side) data-prefix=(n.prefix) data-suffix=(n.suffix) data-created=(n.created.clone()) data-source-annotation=(n.source_annotation)>
                        <div class="tag">(if n.kind == "question" { "问题" } else if n.kind == "highlight" { "划线" } else { "笔记" })</div>
                        if !n.quote.is_empty() { <div class="quote">(n.quote)</div> }
                        <div class="body">(Unescaped::new_unchecked(n.body_html.clone()))</div>
                        for reply in n.replies.clone() { <div class="ai-reply"><small>(reply.by)</small><div class="body">(Unescaped::new_unchecked(reply.body_html))</div></div> }
                        <div class="note-ai-slot">if matches!(n.kind.as_str(), "note" | "question") && !n.body.trim().is_empty() {
                            <button class="note-ai-action" type="button" data-ask-note=(n.id.clone()) disabled=(!ask_available) title=(if ask_available { "基于引文与笔记直接提问" } else { "请先在模型设置中启用并配置 Ask AI" })>(if n.kind == "question" { "✳ Ask AI to answer" } else { "✳ Ask AI for feedback" })</button>
                        }</div>
                        <details class="note-edit" hidden=(true)><summary>"笔记数据"</summary><form method="post" action=(format!("/read/{id}/notes/edit"))>
                            <input type="hidden" name="note_id" value=(n.id.clone())><input type="hidden" name="page" value=(number)><input type="hidden" name="mode" value=(mode.clone())>
                            <label>"类型"<select name="kind"><option value="note" selected=(n.kind == "note")>"笔记"</option><option value="question" selected=(n.kind == "question")>"问题"</option><option value="highlight" selected=(n.kind == "highlight")>"划线"</option></select></label>
                            <label>"标记"<select name="style"><option value="highlight" selected=(n.style != "underline")>"高亮"</option><option value="underline" selected=(n.style == "underline")>"下划线"</option></select></label>
                            <label>"颜色"<select name="color"><option value="yellow" selected=(n.color == "yellow")>"黄"</option><option value="green" selected=(n.color == "green")>"绿"</option><option value="blue" selected=(n.color == "blue")>"蓝"</option><option value="pink" selected=(n.color == "pink")>"红"</option></select></label>
                            <label>"问题状态"<select name="answered"><option value="no" selected=(!n.answered)>"未解答"</option><option value="yes" selected=(n.answered)>"已解答"</option></select></label>
                            <label class="full">"内容"<textarea name="body" maxlength="16000">(n.body.clone())</textarea></label><button type="submit">"保存修改"</button><span class="autosave-status" aria-live="polite"></span>
                        </form></details>
                        <div class="card-actions" aria-label="笔记操作">
                            <button type="button" data-edit-note=(n.id.clone()) aria-label="编辑笔记">"Edit"</button>
                            if matches!(n.kind.as_str(), "note" | "question") { <form method="post" action=(format!("/read/{id}/notes/action"))><input type="hidden" name="note_id" value=(n.id.clone())><input type="hidden" name="page" value=(number)><input type="hidden" name="mode" value=(mode.clone())><button type="submit" name="action" value="kind" data-toggle-note=(n.id.clone())>(if n.kind == "question" { "Make it a note" } else { "Make it a question" })</button></form> }
                            <form method="post" action=(format!("/read/{id}/notes/delete"))><input type="hidden" name="note_id" value=(n.id.clone())><input type="hidden" name="page" value=(number)><input type="hidden" name="mode" value=(mode.clone())><button type="submit" aria-label="删除笔记">"Delete"</button></form>
                        </div>
                    </div>
                }
            </aside><div class="right-space"></div></div>
            if pdf { <aside id="source-panel" class="tool-panel source-panel" aria-label="PDF 原页" hidden=(true)>
                <div class="panel-heading"><strong>"PDF 原页 · "(number)" / "(total)</strong><button class="panel-close" type="button" data-close-panel="true" aria-label="关闭原页">"×"</button></div>
                <a href=(pdf_page.clone()) target="_blank" rel="noopener">"在新标签页查看原页 ↗"</a><img src=(pdf_page) alt=(format!("PDF 第 {number} 页"))>
            </aside> }
            <aside id="notes-drawer" class="tool-panel notes-drawer" aria-label="笔记列表" hidden=(true)>
                <div class="panel-heading"><div><span class="eyebrow">"Annotations"</span><strong>"Paper notes"</strong></div><button class="panel-close" type="button" data-close-panel="true" aria-label="关闭笔记列表">"×"</button></div>
                <div class="drawer-filters" role="group" aria-label="笔记筛选"><button type="button" data-note-filter="all" class="active">"全部"</button><button type="button" data-note-filter="note">"笔记"</button><button type="button" data-note-filter="highlight">"划线"</button><button type="button" data-note-filter="unanswered">"未解答"</button></div>
                <div class="paper-note"><strong>"Paper notes"</strong><form method="post" action=(format!("/read/{id}/paper-note"))><input type="hidden" name="page" value=(number)><input type="hidden" name="mode" value=(mode.clone())><textarea name="body" placeholder="记录全文要点…" maxlength="32000">(paper_note)</textarea><button type="submit">"保存全文笔记"</button></form></div>
                <p class="drawer-hint">"全文批注 · 点击引文跳转到对应页。"</p>
                for n in drawer_notes { <div class=(format!("drawer-note {}", n.kind)) data-note=(n.id.clone()) data-anchor=(n.anchor.clone()) data-kind=(n.kind.clone()) data-answered=(n.answered.to_string()) data-page=(n.page.to_string()) data-side=(n.side) data-prefix=(n.prefix) data-suffix=(n.suffix) data-created=(n.created.clone()) data-source-annotation=(n.source_annotation)>
                    <span class="tag">(format!("{} · {} {}", if n.kind == "question" { "My question" } else if n.kind == "highlight" { "My highlight" } else { "My note" }, if pdf { "p." } else { "章节" }, n.page))</span>
                    if !n.quote.is_empty() { <a class="quote" href=(format!("/read/{id}?page={}&mode={mode}#block-{}", n.page, n.anchor))>(n.quote)</a> }
                    <div class="body">(Unescaped::new_unchecked(n.body_html.clone()))</div>
                    for reply in n.replies.clone() { <div class="ai-reply"><small>(reply.by)</small><div class="body">(Unescaped::new_unchecked(reply.body_html))</div></div> }
                    <div class="note-ai-slot">if matches!(n.kind.as_str(), "note" | "question") && !n.body.trim().is_empty() {
                        <button class="note-ai-action" type="button" data-ask-note=(n.id.clone()) disabled=(!ask_available) title=(if ask_available { "基于引文与笔记直接提问" } else { "请先在模型设置中启用并配置 Ask AI" })>(if n.kind == "question" { "✳ Ask AI to answer" } else { "✳ Ask AI for feedback" })</button>
                    }</div>
                    <div class="card-actions" aria-label="笔记操作"><button type="button" data-edit-note=(n.id.clone()) aria-label="编辑笔记">"Edit"</button>
                        if matches!(n.kind.as_str(), "note" | "question") { <form method="post" action=(format!("/read/{id}/notes/action"))><input type="hidden" name="note_id" value=(n.id.clone())><input type="hidden" name="page" value=(number)><input type="hidden" name="mode" value=(mode.clone())><button type="submit" name="action" value="kind" data-toggle-note=(n.id.clone())>(if n.kind == "question" { "Make it a note" } else { "Make it a question" })</button></form> }
                        <form method="post" action=(format!("/read/{id}/notes/delete"))><input type="hidden" name="note_id" value=(n.id.clone())><input type="hidden" name="page" value=(number)><input type="hidden" name="mode" value=(mode.clone())><button type="submit" aria-label="删除笔记">"Delete"</button></form>
                    </div>
                    <details class="note-edit" hidden=(true)><summary>"笔记数据"</summary><form method="post" action=(format!("/read/{id}/notes/edit"))>
                        <input type="hidden" name="note_id" value=(n.id)><input type="hidden" name="page" value=(number)><input type="hidden" name="mode" value=(mode.clone())>
                        <label>"类型"<select name="kind"><option value="note" selected=(n.kind == "note")>"笔记"</option><option value="question" selected=(n.kind == "question")>"问题"</option><option value="highlight" selected=(n.kind == "highlight")>"划线"</option></select></label>
                        <label>"标记"<select name="style"><option value="highlight" selected=(n.style != "underline")>"高亮"</option><option value="underline" selected=(n.style == "underline")>"下划线"</option></select></label>
                        <label>"颜色"<select name="color"><option value="yellow" selected=(n.color == "yellow")>"黄"</option><option value="green" selected=(n.color == "green")>"绿"</option><option value="blue" selected=(n.color == "blue")>"蓝"</option><option value="pink" selected=(n.color == "pink")>"红"</option></select></label>
                        <label>"问题状态"<select name="answered"><option value="no" selected=(!n.answered)>"未解答"</option><option value="yes" selected=(n.answered)>"已解答"</option></select></label>
                        <label class="full">"内容"<textarea name="body" maxlength="16000">(n.body)</textarea></label><button type="submit">"保存修改"</button><span class="autosave-status" aria-live="polite"></span>
                    </form></details>
                </div> }
            </aside>
            if ask_available { <aside id="chat-panel" class="tool-panel chat-panel" aria-label="Ask AI 对话" hidden=(true)>
                <div class="panel-heading"><strong>"✳ Ask AI"</strong><button class="panel-close" type="button" data-close-panel="true" aria-label="关闭 Ask AI">"×"</button></div>
                <p class="drawer-hint">"基于当前文献及选定引文提问；请求经本机 Rust 服务器发送，密钥不会进入浏览器。"</p>
                <div class="chat-messages">for message in chat_messages { <div class=(format!("chat-message {}",message.role))><span>(if message.failed { "AI · 请求失败" } else if message.role == "assistant" { "AI" } else { "你" })</span><div class="body">(Unescaped::new_unchecked(message.body_html))</div></div> }</div>
                <form id="chat-form" method="post" action=(format!("/read/{id}/ask"))>
                    <input type="hidden" name="anchor"><input type="hidden" name="quote"><input type="hidden" name="note_id"><input type="hidden" name="page" value=(number)><input type="hidden" name="mode" value=(mode.clone())>
                    <label>"提问"<textarea name="text" maxlength="4000" required=(true) placeholder="对当前论文有什么疑问？"></textarea></label><button type="submit">"发送问题"</button>
                </form>
            </aside> }</div>
            <div id="floating" class="floating" role="toolbar" aria-label="选文操作" data-mark-style="highlight">
                <span class="mark-styles" role="group" aria-label="标记样式">
                    <button type="button" data-style="highlight" class="selected" aria-label="高亮" aria-pressed="true" title="选择高亮，再点击颜色应用">
                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false"><path d="m9 11-6 6v3h9l3-3"/><path d="m22 12-4.6 4.6a2 2 0 0 1-2.8 0l-5.2-5.2a2 2 0 0 1 0-2.8L14 4"/></svg>
                    </button>
                    <button type="button" data-style="underline" aria-label="下划线" aria-pressed="false" title="选择下划线，再点击颜色应用">
                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false"><path d="M6 4v6a6 6 0 0 0 12 0V4"/><line x1="4" x2="20" y1="20" y2="20"/></svg>
                    </button>
                </span><span class="swatches" role="group" aria-label="选择颜色并应用标记"><button type="button" data-color="yellow" aria-label="黄色高亮"></button><button type="button" data-color="green" aria-label="绿色高亮"></button><button type="button" data-color="blue" aria-label="蓝色高亮"></button><button type="button" data-color="pink" aria-label="红色高亮"></button></span><button type="button" data-action="original" aria-expanded="false">"En / 译文"</button><button type="button" data-action="edit">"修订译文"</button>if ask_available { <button type="button" data-action="ask">"Ask AI"</button> }<button type="button" data-action="note">"笔记"</button><button type="button" data-action="question">"问题"</button><button type="button" data-action="copy">"复制"</button><button type="button" data-action="close">"×"</button>
            </div>
            <div id="margin-editor" class="margin-editor" aria-label="页边笔记编辑器" hidden=(true)></div>
            <div id="margin-drafts" hidden=(true)></div>
            <template id="margin-note-template">
                <form class="margin-note-form" method="post" action=(format!("/read/{id}/notes/margin"))>
                    <input type="hidden" name="note_id"><input type="hidden" name="session_id"><input type="hidden" name="revision" value="0">
                    <input type="hidden" name="anchor"><input type="hidden" name="side"><input type="hidden" name="quote"><input type="hidden" name="prefix"><input type="hidden" name="suffix"><input type="hidden" name="source_annotation">
                    <input type="hidden" name="kind" value="note"><input type="hidden" name="color" value="yellow"><input type="hidden" name="answered" value="no">
                    <input type="hidden" name="page" value=(number)><input type="hidden" name="mode" value=(mode.clone())>
                    <div class="margin-note-header"><span class="margin-note-title">"My note"</span><span class="margin-question-badge" hidden=(true)>"Unanswered"</span><time></time><button type="button" data-margin-close="true" aria-label="关闭笔记" title="关闭 · Esc">"×"</button></div>
                    <blockquote class="margin-note-quote"></blockquote>
                    <textarea name="body" aria-label="My note text" maxlength="16000" placeholder="Write your understanding, questions or ideas… (supports $math$, **bold**)" spellcheck="false"></textarea>
                    <div class="margin-note-options"><span role="group" aria-label="笔记类型"><button type="button" data-note-kind="note" aria-pressed="true">"Notes"</button><button type="button" data-note-kind="question" aria-pressed="false">"Question"</button></span><span class="note-colors" role="group" aria-label="引文颜色"><button type="button" data-note-color="yellow" aria-label="黄色"></button><button type="button" data-note-color="green" aria-label="绿色"></button><button type="button" data-note-color="blue" aria-label="蓝色"></button><button type="button" data-note-color="pink" aria-label="红色"></button></span></div>
                    <button class="margin-answer" type="button" data-note-answered="true" hidden=(true)>"Mark as answered"</button>
                    <div class="margin-note-footer"><button type="button" data-note-retry="true">"Retry"</button><span class="autosave-status" aria-live="polite">"Autosave"</span><span>" · Esc to close"</span></div>
                </form>
            </template>
            <form id="mark-form" hidden=(true) method="post" action=(notes_action)>
                    <input type="hidden" name="anchor"><input type="hidden" name="side"><input type="hidden" name="quote"><input type="hidden" name="prefix"><input type="hidden" name="suffix"><input type="hidden" name="kind"><input type="hidden" name="style" value="highlight"><input type="hidden" name="color" value="yellow">
                    <input type="hidden" name="page" value=(number)><input type="hidden" name="mode" value=(mode.clone())>
                    <input type="hidden" name="body">
            </form>
        </body></html>
    })
}

#[route(POST "/read/{book_id}/notes")]
async fn save_note(cx: &Cx, Form(input): Form<NoteInput>) -> Result<SeeOther> {
    let book_id = path_param::<BookId>(cx);
    let book = library().workspace(book_id).map_err(|_| not_found())?;
    if !matches!(input.kind.as_str(), "note" | "question" | "highlight")
        || !matches!(input.color.as_str(), "yellow" | "green" | "blue" | "pink")
        || !matches!(input.style.as_str(), "highlight" | "underline")
        || input.body.len() > 16_000
        || input.quote.len() > 4_000
        || input.prefix.len() > 128
        || input.suffix.len() > 128
        || !matches!(input.side.as_str(), "en" | "zh")
    {
        return Err(bad_request("笔记内容无效").into());
    }
    let raw = book.load("paper").map_err(Error::from_anyhow)?;
    if !raw["blocks"]
        .as_array()
        .is_some_and(|blocks| blocks.iter().any(|b| b["id"] == input.anchor))
    {
        return Err(bad_request("笔记段落不存在").into());
    }
    let note = json!({"id": uuid::Uuid::new_v4().to_string(), "anchor": input.anchor, "side": input.side, "quote":input.quote,
        "prefix":input.prefix, "suffix":input.suffix, "kind":input.kind, "body":input.body, "color":input.color, "style":input.style, "created":easyread_gpui::store::now(), "updated":easyread_gpui::store::now()});
    book.reader_ops(&[json!({"op":"note","note":note})])
        .map_err(Error::from_anyhow)?;
    let mode = if matches!(
        input.mode.as_str(),
        "original" | "translation" | "bilingual"
    ) {
        input.mode
    } else {
        "bilingual".into()
    };
    Ok(see_other(format!(
        "/read/{book_id}?page={}&mode={mode}",
        input.page
    )))
}

/// The margin editor owns a stable ID before its first save. Replaying a
/// committed request (or closing while a save is pending) must not add a note.
#[route(POST "/read/{book_id}/notes/margin")]
async fn save_margin_note(cx: &Cx, Form(input): Form<MarginNoteInput>) -> Result<SeeOther> {
    if !matches!(input.kind.as_str(), "note" | "question")
        || !matches!(input.color.as_str(), "yellow" | "green" | "blue" | "pink")
        || !matches!(input.answered.as_str(), "yes" | "no")
        || !matches!(input.side.as_str(), "en" | "zh")
        || input.body.len() > 16_000
        || input.quote.len() > 4_000
        || input.prefix.len() > 128
        || input.suffix.len() > 128
        || uuid::Uuid::parse_str(&input.session_id).is_err()
    {
        return Err(bad_request("页边笔记内容无效").into());
    }
    // Serialize the revision check and write, including the final pagehide
    // beacon: an older in-flight autosave cannot overwrite the latest draft.
    static WRITES: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = WRITES
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let id = path_param::<BookId>(cx);
    let book = library().workspace(id).map_err(|_| not_found())?;
    let raw = book.validated_state().map_err(Error::from_anyhow)?;
    if !raw["paper"]["blocks"]
        .as_array()
        .is_some_and(|blocks| blocks.iter().any(|b| b["id"] == input.anchor))
    {
        return Err(bad_request("笔记段落不存在").into());
    }
    let stored = &raw["reader"]["notes"][&input.note_id];
    if stored.is_null() {
        if uuid::Uuid::parse_str(&input.note_id).is_err() || input.body.trim().is_empty() {
            return Err(bad_request("新笔记不能为空").into());
        }
    } else if stored["deleted"] == true
        || stored["anchor"] != input.anchor
        || stored["side"].as_str().unwrap_or("zh") != input.side
        || !matches!(stored["kind"].as_str(), Some("note" | "question"))
    {
        return Err(bad_request("该笔记已删除或锚点不匹配").into());
    }
    let mode = if matches!(
        input.mode.as_str(),
        "original" | "translation" | "bilingual"
    ) {
        &input.mode
    } else {
        "bilingual"
    };
    let destination = format!("/read/{id}?page={}&mode={mode}", input.page);
    if stored["web_margin_version"]["session"] == input.session_id
        && stored["web_margin_version"]["revision"]
            .as_u64()
            .is_some_and(|revision| revision >= input.revision)
    {
        return Ok(see_other(destination));
    }
    let stamp = easyread_gpui::store::now();
    let mut note = if stored.is_null() {
        json!({"id":input.note_id,"anchor":input.anchor,"side":input.side,"quote":input.quote,
            "prefix":input.prefix,"suffix":input.suffix,"created":stamp,"style":"underline"})
    } else {
        stored.clone()
    };
    note["kind"] = json!(input.kind);
    note["body"] = json!(input.body);
    note["color"] = json!(input.color);
    note["answered"] = json!(input.answered == "yes");
    note["updated"] = json!(stamp);
    note["web_margin_version"] = json!({"session":input.session_id,"revision":input.revision});
    let mut source_update = None;
    if !input.source_annotation.is_empty() {
        let source = &raw["reader"]["notes"][&input.source_annotation];
        if source["deleted"] != true {
            if source["kind"] != "highlight"
                || source["anchor"] != input.anchor
                || source["side"].as_str().unwrap_or("zh") != input.side
            {
                return Err(bad_request("关联标记无效").into());
            }
            if source["color"] != input.color {
                let mut source = source.clone();
                source["color"] = json!(input.color);
                source["updated"] = json!(stamp);
                source_update = Some(json!({"op":"note","note":source}));
            }
        }
        // Attach writing to the mark without turning that independent mark
        // into a question or adding a second visible decoration over it.
        note["source_annotation"] = json!(input.source_annotation);
        note["decoration"] = json!("none");
        note.as_object_mut().unwrap().remove("style");
    }
    let mut ops = vec![json!({"op":"note","note":note})];
    if let Some(source) = source_update {
        ops.push(source);
    }
    book.reader_ops(&ops).map_err(Error::from_anyhow)?;
    Ok(see_other(destination))
}

#[route(POST "/notes/preview")]
async fn preview_note(Form(input): Form<NotePreviewInput>) -> Result<Html<String>> {
    if input.body.len() > 16_000 {
        return Err(bad_request("笔记过长").into());
    }
    Ok(Html(cleaned(&formula::native_markdown_html(&input.body))))
}

#[route(POST "/read/{book_id}/notes/edit")]
async fn edit_note(cx: &Cx, Form(input): Form<NoteEdit>) -> Result<SeeOther> {
    if !matches!(input.kind.as_str(), "note" | "question" | "highlight")
        || !matches!(input.color.as_str(), "yellow" | "green" | "blue" | "pink")
        || !matches!(input.style.as_str(), "highlight" | "underline")
        || !matches!(input.answered.as_str(), "yes" | "no")
        || input.body.len() > 16_000
    {
        return Err(bad_request("笔记修改无效").into());
    }
    let id = path_param::<BookId>(cx);
    let book = library().workspace(id).map_err(|_| not_found())?;
    let current = book.load("reader").map_err(Error::from_anyhow)?;
    let Some(mut note) = current["notes"]
        .get(&input.note_id)
        .cloned()
        .filter(|n| !n["deleted"].as_bool().unwrap_or(false))
    else {
        return Err(not_found().into());
    };
    note["kind"] = json!(input.kind);
    note["body"] = json!(input.body);
    note["color"] = json!(input.color);
    note["style"] = json!(input.style);
    note["answered"] = json!(input.answered == "yes");
    note["updated"] = json!(easyread_gpui::store::now());
    book.reader_ops(&[json!({"op":"note","note":note})])
        .map_err(Error::from_anyhow)?;
    let mode = if matches!(
        input.mode.as_str(),
        "original" | "translation" | "bilingual"
    ) {
        input.mode
    } else {
        "bilingual".into()
    };
    Ok(see_other(format!(
        "/read/{id}?page={}&mode={mode}",
        input.page
    )))
}

#[route(POST "/read/{book_id}/notes/action")]
async fn note_action(cx: &Cx, Form(input): Form<NoteAction>) -> Result<SeeOther> {
    if input.action != "kind" {
        return Err(bad_request("笔记操作无效").into());
    }
    let id = path_param::<BookId>(cx);
    let book = library().workspace(id).map_err(|_| not_found())?;
    let reader = book.load("reader").map_err(Error::from_anyhow)?;
    let Some(mut note) = reader["notes"].get(&input.note_id).cloned().filter(|note| {
        note["deleted"] != true && matches!(note["kind"].as_str(), Some("note" | "question"))
    }) else {
        return Err(bad_request("笔记已删除或不能转换类型").into());
    };
    note["kind"] = json!(if note["kind"] == "question" {
        "note"
    } else {
        "question"
    });
    // Only change kind: retain body, answer/status, color, decoration and links.
    note["updated"] = json!(easyread_gpui::store::now());
    book.reader_ops(&[json!({"op":"note","note":note})])
        .map_err(Error::from_anyhow)?;
    let mode = if matches!(
        input.mode.as_str(),
        "original" | "translation" | "bilingual"
    ) {
        &input.mode
    } else {
        "bilingual"
    };
    Ok(see_other(format!(
        "/read/{id}?page={}&mode={mode}",
        input.page
    )))
}

#[route(POST "/read/{book_id}/translate")]
async fn translate_book(cx: &Cx, Form(input): Form<AiPageInput>) -> Result<SeeOther> {
    let id = path_param::<BookId>(cx).to_string();
    let book = library().workspace(&id).map_err(|_| not_found())?;
    let prefs = model_preferences().map_err(Error::from_anyhow)?;
    let config = prefs
        .connection(true)
        .map_err(|_| bad_request("翻译模型尚未就绪，请检查首页的模型设置"))?;
    let options = ai::TranslationOptions {
        pages_per_batch: prefs.pages_per_batch,
        parallel: prefs.parallel,
    };
    let concurrent_papers = prefs
        .extra
        .get("concurrent_papers")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .clamp(1, 4) as usize;
    let raw = book.validated_state().map_err(Error::from_anyhow)?;
    let pages = raw["paper"]["meta"][if raw["paper"]["meta"]["format"] == "epub" {
        "chapter_count"
    } else {
        "page_count"
    }]
    .as_u64()
    .unwrap_or(1);
    if (prefs.ask_above == 0 || pages > prefs.ask_above as u64)
        && input.confirm.as_deref() != Some("yes")
    {
        return Err(bad_request("长文献需要确认模型调用费用").into());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut tasks = translations().lock().unwrap_or_else(|p| p.into_inner());
        if tasks.contains_key(&id) {
            return Err(bad_request("该书正在翻译").into());
        }
        if tasks.len() >= concurrent_papers {
            return Err(bad_request("已达到模型设置中的同时翻译文献上限").into());
        }
        tasks.insert(id.clone(), cancel.clone());
    }
    tokio::task::spawn_blocking(move || {
        if let Err(error) = ai::translate_batched(&book, &config, options, &cancel, |_, _| {}) {
            eprintln!("Web translation interrupted: {error:#}");
        }
        translations()
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&id);
    });
    let mode = if matches!(
        input.mode.as_str(),
        "original" | "translation" | "bilingual"
    ) {
        input.mode
    } else {
        "bilingual".into()
    };
    Ok(see_other(format!(
        "/read/{}?page={}&mode={mode}",
        path_param::<BookId>(cx),
        input.page
    )))
}

#[route(POST "/read/{book_id}/translate/cancel")]
async fn cancel_translate(cx: &Cx, Form(input): Form<AiPageInput>) -> Result<SeeOther> {
    let id = path_param::<BookId>(cx);
    library().workspace(id).map_err(|_| not_found())?;
    if let Some(cancel) = translations()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(id)
    {
        cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    let mode = if matches!(
        input.mode.as_str(),
        "original" | "translation" | "bilingual"
    ) {
        input.mode
    } else {
        "bilingual".into()
    };
    Ok(see_other(format!(
        "/read/{id}?page={}&mode={mode}",
        input.page
    )))
}

#[route(GET "/read/{book_id}/job")]
async fn translation_status(cx: &Cx) -> Result<Json<Value>> {
    let id = path_param::<BookId>(cx);
    let book = library().workspace(id).map_err(|_| not_found())?;
    let job = book.load("job").map_err(Error::from_anyhow)?;
    let active = translations()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .contains_key(id);
    Ok(Json(
        json!({"state": if active && !job["state"].is_string() { "starting" } else { job["state"].as_str().unwrap_or("idle") },
        "active":active,"done":job["done"],"total":job["total"]}),
    ))
}

#[route(POST "/read/{book_id}/ask")]
async fn ask_ai(cx: &Cx, Form(input): Form<AskInput>) -> Result<SeeOther> {
    if input.text.trim().is_empty()
        || input.text.len() > 4_000
        || input.quote.as_deref().is_some_and(|q| q.len() > 1_000)
    {
        return Err(bad_request("提问内容无效").into());
    }
    let id = path_param::<BookId>(cx);
    let book = library().workspace(id).map_err(|_| not_found())?;
    let prefs = model_preferences().map_err(Error::from_anyhow)?;
    if !prefs.features.ask_ai {
        return Err(bad_request("桌面设置已关闭 Ask AI").into());
    }
    let config = prefs
        .connection(false)
        .map_err(|_| bad_request("Ask AI 模型尚未就绪，请检查首页的模型设置"))?;
    let state = book.validated_state().map_err(Error::from_anyhow)?;
    if input.anchor.as_deref().is_some_and(|anchor| {
        !state["paper"]["blocks"]
            .as_array()
            .is_some_and(|bs| bs.iter().any(|b| b["id"] == anchor))
    }) {
        return Err(bad_request("提问定位已失效").into());
    }
    let note = input.note_id.as_deref().filter(|id| !id.is_empty());
    if note.is_some_and(|note_id| {
        !state["reader"]["notes"][note_id].is_object()
            || state["reader"]["notes"][note_id]["anchor"].as_str() != input.anchor.as_deref()
            || state["reader"]["notes"][note_id]["deleted"] == true
    }) {
        return Err(bad_request("笔记已改变，请刷新后重试").into());
    }
    let request = ai::ChatRequest {
        text: input.text,
        refs: input
            .anchor
            .map(|anchor| ai::ChatReference {
                anchor,
                quote: input.quote.unwrap_or_default(),
            })
            .into_iter()
            .collect(),
        note: note.map(str::to_owned),
        ..Default::default()
    };
    let (outcome, recorded_error) = tokio::task::spawn_blocking(move || {
        let outcome = ai::ask_with_context(
            &book,
            "web-reader",
            &request,
            &config,
            &AtomicBool::new(false),
            |_| {},
        );
        let recorded_error = outcome.is_err()
            && book.load("chat").ok().is_some_and(|chat| {
                chat["threads"].as_array().is_some_and(|threads| {
                    threads.iter().any(|thread| {
                        thread["id"] == "web-reader"
                            && thread["messages"].as_array().is_some_and(|messages| {
                                let len = messages.len();
                                len >= 2
                                    && messages[len - 1]["status"] == "error"
                                    && messages[len - 2]["content"] == request.text
                            })
                    })
                })
            });
        (outcome, recorded_error)
    })
    .await?;
    if let Err(error) = outcome {
        eprintln!("Web Ask AI interrupted: {error:#}");
        if !recorded_error {
            return Err(Error::from_anyhow(error));
        }
    }
    let mode = if matches!(
        input.mode.as_str(),
        "original" | "translation" | "bilingual"
    ) {
        input.mode
    } else {
        "bilingual".into()
    };
    Ok(see_other(format!(
        "/read/{id}?page={}&mode={mode}#chat-panel",
        input.page
    )))
}

#[route(POST "/read/{book_id}/translation")]
async fn save_translation(cx: &Cx, Form(input): Form<TranslationInput>) -> Result<SeeOther> {
    if input.text.len() > 64_000 || !matches!(input.action.as_str(), "save" | "revert") {
        return Err(bad_request("译文修订无效").into());
    }
    let expected_edit: Value =
        serde_json::from_str(&input.expected_edit).map_err(|_| bad_request("修订版本无效"))?;
    let id = path_param::<BookId>(cx);
    let book = library().workspace(id).map_err(|_| not_found())?;
    let raw = book.validated_state().map_err(Error::from_anyhow)?;
    let state = ReaderState::from_value(raw.clone()).map_err(Error::from_anyhow)?;
    let caption = input.block.ends_with("#caption");
    let block_id = input.block.strip_suffix("#caption").unwrap_or(&input.block);
    let Some(block) = state
        .paper
        .blocks
        .iter()
        .find(|b| b.id == block_id)
        .filter(|b| {
            if caption {
                matches!(b.kind.as_str(), "figure" | "table")
            } else {
                matches!(b.kind.as_str(), "para" | "heading" | "note")
            }
        })
    else {
        return Err(bad_request("不可修订该内容块").into());
    };
    let current = if caption {
        state.caption(block)
    } else {
        state.text(&block.id, &block.zh, &block.en)
    };
    let stored = raw["reader"]["edits"]
        .get(&input.block)
        .cloned()
        .unwrap_or(Value::Null);
    if easyread_gpui::model::text_hash(current) != input.expected_text || stored != expected_edit {
        return Err(bad_request("译文已改变，请刷新后重试").into());
    }
    let base = if caption {
        if block.caption_zh.is_empty() {
            &block.caption_en
        } else {
            &block.caption_zh
        }
    } else if block.zh.is_empty() {
        &block.en
    } else {
        &block.zh
    };
    // Reconstruct the server-owned native HTML, never accept a whole-page
    // buffer or client-supplied HTML. ParagraphEdit merges only this paragraph.
    let pdf = raw["paper"]["meta"]["format"] != "epub";
    let position = raw["paper"]["blocks"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|b| b["id"] == block_id)
        .and_then(|b| b[if pdf { "page" } else { "chapter" }].as_u64())
        .unwrap_or(1) as usize;
    let source_pdf = book.asset("source.pdf").ok();
    let rendered = display_blocks(&raw, &state, &[], position, pdf, source_pdf.as_deref())
        .into_iter()
        .find(|b| b.id == block_id)
        .ok_or_else(|| bad_request("段落已不存在"))?;
    let plan = easyread_gpui::paragraph_edit::ParagraphEdit::new(
        current,
        &rendered.edit_html,
        input.paragraph,
    )
    .map_err(|_| bad_request("无法可靠定位该段译文，请重新选择正文段落"))?;
    let replacement = if input.action == "revert" {
        let mut baseline = raw.clone();
        if let Some(edits) = baseline["reader"]["edits"].as_object_mut() {
            edits.remove(&input.block);
        }
        let baseline_state =
            ReaderState::from_value(baseline.clone()).map_err(Error::from_anyhow)?;
        let baseline = display_blocks(
            &baseline,
            &baseline_state,
            &[],
            position,
            pdf,
            source_pdf.as_deref(),
        )
        .into_iter()
        .find(|b| b.id == block_id)
        .ok_or_else(|| bad_request("原译文已不存在"))?;
        if baseline.edit_paragraphs.len() != rendered.edit_paragraphs.len() {
            return Err(bad_request("段落结构已改变，无法安全还原该段").into());
        }
        easyread_gpui::paragraph_edit::ParagraphEdit::new(
            base,
            &baseline.edit_html,
            input.paragraph,
        )
        .map_err(|_| bad_request("无法安全还原该段"))?
        .text
    } else {
        input.text
    };
    let (markdown, html) = plan.merge(&replacement).map_err(Error::from_anyhow)?;
    let reset = input.action == "revert" && markdown == *base;
    let op = json!({"op":"edit","block":input.block,"zh":if reset { Value::Null } else { json!(markdown) },
        "native_layout":{"html":html,"source_hash":easyread_gpui::model::text_hash(if caption { &block.caption_en } else { &block.en }),
            "translation_hash":easyread_gpui::model::text_hash(&markdown)},
        "base":easyread_gpui::model::text_hash(base), "expected_text":input.expected_text,"expected_edit":expected_edit});
    book.reader_ops(&[op]).map_err(Error::from_anyhow)?;
    let mode = if matches!(
        input.mode.as_str(),
        "original" | "translation" | "bilingual"
    ) {
        input.mode
    } else {
        "bilingual".into()
    };
    Ok(see_other(format!(
        "/read/{id}?page={}&mode={mode}#block-{block_id}",
        input.page
    )))
}

#[route(POST "/read/{book_id}/progress")]
async fn save_progress(cx: &Cx, Form(input): Form<ProgressInput>) -> Result<SeeOther> {
    if !input.ratio.is_finite() || !(0.0..=1.0).contains(&input.ratio) {
        return Err(bad_request("进度无效").into());
    }
    let id = path_param::<BookId>(cx);
    let book = library().workspace(id).map_err(|_| not_found())?;
    let paper = book.load("paper").map_err(Error::from_anyhow)?;
    if !paper["blocks"]
        .as_array()
        .is_some_and(|blocks| blocks.iter().any(|block| block["id"] == input.block))
    {
        return Err(bad_request("进度段落无效").into());
    }
    book.reader_ops(&[json!({"op":"progress","block":input.block,"ratio":input.ratio})])
        .map_err(Error::from_anyhow)?;
    Ok(see_other(format!("/read/{id}")))
}

#[route(POST "/read/{book_id}/paper-note")]
async fn save_paper_note(cx: &Cx, Form(input): Form<PaperNoteInput>) -> Result<SeeOther> {
    if input.body.len() > 32_000 {
        return Err(bad_request("全文笔记过长").into());
    }
    let id = path_param::<BookId>(cx);
    let book = library().workspace(id).map_err(|_| not_found())?;
    book.reader_ops(&[json!({"op":"paper_note","body":input.body})])
        .map_err(Error::from_anyhow)?;
    let mode = if matches!(
        input.mode.as_str(),
        "original" | "translation" | "bilingual"
    ) {
        input.mode
    } else {
        "bilingual".into()
    };
    Ok(see_other(format!(
        "/read/{id}?page={}&mode={mode}",
        input.page
    )))
}

#[derive(Deserialize)]
struct DeleteNote {
    note_id: String,
    page: usize,
    mode: String,
}
#[route(POST "/read/{book_id}/notes/delete")]
async fn delete_note(cx: &Cx, Form(input): Form<DeleteNote>) -> Result<SeeOther> {
    let id = path_param::<BookId>(cx);
    let book = library().workspace(id).map_err(|_| not_found())?;
    // Refuse blind deletion of arbitrary note IDs.
    let current = book.load("reader").map_err(Error::from_anyhow)?;
    if current["notes"].get(&input.note_id).is_none() {
        return Err(not_found().into());
    }
    book.reader_ops(&[json!({"op":"note_del","id":input.note_id})])
        .map_err(Error::from_anyhow)?;
    let mode = if matches!(
        input.mode.as_str(),
        "original" | "translation" | "bilingual"
    ) {
        input.mode
    } else {
        "bilingual".into()
    };
    Ok(see_other(format!(
        "/read/{id}?page={}&mode={mode}",
        input.page
    )))
}

#[route(GET "/math.svg")]
async fn serve_math_svg(
    cx: &Cx,
) -> Result<([(topcoat::router::HeaderName, &'static str); 1], Vec<u8>)> {
    let query = query_params::<MathQuery>(cx)?;
    if query.tex.len() > 4096 || query.tex.trim().is_empty() {
        return Err(bad_request("公式长度无效").into());
    }
    let tex = query.tex.clone();
    let display = query.display.unwrap_or(false);
    let svg = tokio::task::spawn_blocking(move || math_svg::render(&tex, display))
        .await?
        .map_err(Error::from_anyhow)?;
    Ok((
        [(topcoat::router::header::CONTENT_TYPE, "image/svg+xml")],
        svg.into_bytes(),
    ))
}

#[route(GET "/home.js")]
async fn home_script() -> Result<Js<&'static str>> {
    Ok(Js(include_str!("../assets/home.js")))
}
#[route(GET "/settings.css")]
async fn settings_styles() -> Result<Css<&'static str>> {
    Ok(Css(include_str!("../assets/settings.css")))
}
#[route(GET "/settings.js")]
async fn settings_script() -> Result<Js<&'static str>> {
    Ok(Js(include_str!("../assets/settings.js")))
}
#[route(GET "/reader.css")]
async fn styles() -> Result<Css<&'static str>> {
    Ok(Css(include_str!("../assets/reader.css")))
}
#[route(GET "/reader-updates.js")]
async fn reader_updates() -> Result<Js<&'static str>> {
    Ok(Js(include_str!("../assets/reader-updates.js")))
}
#[route(GET "/margin-editor.css")]
async fn margin_editor_css() -> Result<Css<&'static str>> {
    Ok(Css(include_str!("../assets/margin-editor.css")))
}
#[route(GET "/margin-editor.js")]
async fn margin_editor_js() -> Result<Js<&'static str>> {
    Ok(Js(include_str!("../assets/margin-editor.js")))
}
#[route(GET "/reader.js")]
async fn scripts() -> Result<Js<&'static str>> {
    Ok(Js(include_str!("../assets/reader.js")))
}
#[route(GET "/font/serif.ttf")]
async fn font() -> Result<([(topcoat::router::HeaderName, &'static str); 1], Vec<u8>)> {
    Ok((
        [(topcoat::router::header::CONTENT_TYPE, "font/ttf")],
        include_bytes!("../../assets/fonts/NotoSerifSC-Regular.ttf").to_vec(),
    ))
}
#[route(GET "/read/{book_id}/{*asset_path}")]
async fn media(cx: &Cx) -> Result<([(topcoat::router::HeaderName, &'static str); 1], Vec<u8>)> {
    let id = path_param::<BookId>(cx);
    let segments = path_param::<AssetPath>(cx);
    let path = segments.collect::<Vec<_>>().join("/");
    if !(path.starts_with("images/") || path.starts_with("pages/")) {
        return Err(not_found().into());
    }
    let content_type = if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".jpg") || path.ends_with(".jpeg") {
        "image/jpeg"
    } else if path.ends_with(".webp") {
        "image/webp"
    } else if path.ends_with(".gif") {
        "image/gif"
    } else {
        return Err(not_found().into());
    };
    let workspace = library().workspace(id).map_err(|_| not_found())?;
    let file = workspace.asset(&path).map_err(|_| not_found())?;
    let bytes = tokio::fs::read(file).await.map_err(|_| not_found())?;
    Ok((
        [(topcoat::router::header::CONTENT_TYPE, content_type)],
        bytes,
    ))
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // This experiment is intentionally loopback-only. Topcoat's default is 127.0.0.1:3000.
    if let Ok(host) = std::env::var("HOST")
        && host != "127.0.0.1"
        && host != "localhost"
    {
        eprintln!("Refusing non-loopback HOST: the experiment has no user authentication");
        std::process::exit(2);
    }
    let root = std::env::var_os("VELLIO_WEB_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("./vellio-web-library"));
    LIBRARY
        .set(Library::open(&root).map_err(std::io::Error::other)?)
        .expect("one library");
    topcoat::start(
        module_router!()
            .discover()
            .layer(BodyLimit::max(32 * 1024 * 1024).at("/import"))
            .build(),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::cleaned;

    #[test]
    fn imported_markup_is_cleaned_before_topcoat_unescaped_output() {
        let html = cleaned(
            r##"<script>alert(1)</script><img src="javascript:alert(1)" onerror="alert(2)"><img src="images/picture.png" alt="figure"><span data-easyread-note="note1"><mark color="#ffeeaa">word</mark></span>"##,
        );
        for forbidden in ["<script", "javascript:", "onerror"] {
            assert!(!html.contains(forbidden), "{html}");
        }
        assert!(html.contains("images/picture.png"), "{html}");
        assert!(html.contains("data-easyread-note=\"note1\""), "{html}");
    }

    #[test]
    fn pdf_page_translation_pairs_paragraphs_without_changing_the_persisted_anchor() {
        use super::display_blocks;
        use easyread_gpui::model::{ReaderState, text_hash};
        use serde_json::json;
        let mut raw: serde_json::Value =
            serde_json::from_str(include_str!("../../fixtures/state.json")).unwrap();
        let en = "First source paragraph.\n\nSecond source paragraph.";
        let zh = "第一段译文。\n\n第二段译文。";
        raw["paper"]["blocks"].as_array_mut().unwrap().push(json!({"id":"pdf-page-pair","type":"para", "source_kind":"pdf-text","page":2,
            "en":en,"zh":zh,"native_translation_layout":{"version":1,"source_hash":text_hash(en),"translation_hash":text_hash(zh),
                "html":"<p>第一段译文。</p><p>第二段译文。</p>"}}));
        let state = ReaderState::from_value(raw.clone()).unwrap();
        let blocks = display_blocks(&raw, &state, &[], 2, true, None);
        let pair = &blocks[0];
        assert!(pair.page_block);
        assert_eq!(pair.paired.matches("data-easyread-companion").count(), 2);
        let positions = [
            "First source paragraph",
            "第一段译文",
            "Second source paragraph",
            "第二段译文",
        ]
        .map(|text| pair.paired.find(text).unwrap());
        assert!(positions.windows(2).all(|window| window[0] < window[1]));
        assert_eq!(pair.id, "pdf-page-pair");
        let note = json!({"id":"pdf-inline-note","anchor":"pdf-page-pair","side":"en","quote":"Second source",
            "prefix":"","suffix":"","kind":"highlight","color":"yellow","style":"highlight"});
        let marked = display_blocks(&raw, &state, &[note], 2, true, None);
        assert!(
            marked[0]
                .paired
                .contains("data-easyread-note=\"pdf-inline-note\"")
        );
        raw["paper"]["blocks"]
            .as_array_mut()
            .unwrap()
            .last_mut()
            .unwrap()["zh"] = json!("合并译文。");
        let state = ReaderState::from_value(raw.clone()).unwrap();
        assert!(
            display_blocks(&raw, &state, &[], 2, true, None)[0]
                .paired
                .is_empty(),
            "ambiguous legacy translations must not be guessed into paragraph pairs"
        );
    }

    #[test]
    fn only_generated_math_metadata_and_palette_underlines_survive_sanitizing() {
        let html = cleaned(
            r#"<span data-vellio-math="x^2" data-display="false">$x^2$</span><u style="text-decoration-color:#477fc1">blue</u><u style="background:url(javascript:evil)">bad</u>"#,
        );
        assert!(html.contains("data-vellio-math=\"x^2\""), "{html}");
        assert!(html.contains("data-underline=\"blue\""), "{html}");
        assert!(!html.contains("javascript:"), "{html}");
    }
}
