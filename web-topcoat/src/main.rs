//! Local-only Topcoat experiment: server-side Vellio parser/storage, DOM reader.
//! Not a public deployment: no accounts, cross-device sync, or complete desktop rendering.
mod book_images;
mod math_svg;
mod model_settings;
mod note_markdown;
mod paragraphs;
mod reader_controls;
mod table_of_contents;
mod web_updates;
use easyread_gpui::{
    ai, annotations, formats::epub, formula, library_home, model::ReaderState,
    paragraph_translation::PairLayout, pdf, settings, store::Library, translation_layout,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock, atomic::AtomicBool},
};
use topcoat::{
    Error, Result,
    context::Cx,
    router::{
        Body, BodyLimit, Next, RouterBuilderDiscoverExt,
        content::{Css, Form, Html, Js, Json, multipart::Multipart},
        error::{SeeOther, bad_request, not_found, see_other},
        layer, module_router, page, path_param, query_params,
        response::Response,
        route,
    },
    view::{Unescaped, View, element_id, view},
};

// A layout-local context would not reach nested page components. Set the locale
// before rendering, as required by Topcoat 0.11's request-scoped l10n API.
#[layer]
async fn locale_layer(cx: &Cx, body: Body, next: Next<'_>) -> Result<Response> {
    let mut response = next
        .run(&cx.with(reader_controls::request_locale(cx)), body)
        .await?;
    if response
        .headers()
        .get(topcoat::router::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/html"))
    {
        response.headers_mut().append(
            topcoat::router::header::VARY,
            topcoat::router::HeaderValue::from_static("Accept-Language"),
        );
    }
    Ok(response)
}

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
    alt: String,
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

mod document;
mod routes;
use document::*;

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
