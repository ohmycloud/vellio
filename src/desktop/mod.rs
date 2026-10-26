//! Native GPUI application boundary. Shared view state stays private here;
//! child modules own construction, commands and focused UI surfaces.
use easyread_gpui::{
    ai,
    annotation_panel::{self, AnnotationFilter},
    annotations,
    annotations::HighlightColor,
    formula,
    layout::PageLayout,
    margin_note::{MarginNote, MarginWriter},
    model::{ReaderState, text_hash},
    navigation::{heading_depth, split_heading_number},
    pdf_layout_cache::{self, PresentationCache},
    reading_motion::{ScrollPath, ScrollPosition},
    session::{
        Appearance, Draft, EditDraft, NoteDraft, Position, ReadingFont, ReadingMode, Session,
    },
    store::{Library, Workspace, now},
};
use gpui_kit::base::{Disableable, TextSelection};
use gpui_kit::{
    component::{
        Icon, Sizable,
        button::{Button, ButtonCustomVariant, ButtonVariants},
        input::{Input, InputEvent, InputState, Textarea, TextareaState},
        slider::{Slider, SliderEvent, SliderState},
        text::{TextView, TextViewState},
    },
    prelude::FluentBuilder,
    *,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use uuid::Uuid;

#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "tests/annotation_scope_tests.rs"]
mod annotation_scope_tests;
mod chat;
mod chat_model;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "tests/chat_model_tests.rs"]
mod chat_model_tests;
mod chat_style;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "tests/chat_tests.rs"]
mod chat_tests;
mod code;
mod epub;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "tests/epub_stability_tests.rs"]
mod epub_stability_tests;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "tests/epub_tests.rs"]
mod epub_tests;
mod home;
mod inline_edit;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "tests/inline_edit_tests.rs"]
mod inline_edit_tests;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "tests/margin_tests.rs"]
mod margin_tests;
mod math;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "tests/note_hover_tests.rs"]
mod note_hover_tests;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "tests/note_selection_tests.rs"]
mod note_selection_tests;
mod pdf_flow;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "tests/pdf_flow_tests.rs"]
mod pdf_flow_tests;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "tests/placement_tests.rs"]
mod placement_tests;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "tests/progress_tests.rs"]
mod progress_tests;
mod reading_mode;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "tests/reading_mode_tests.rs"]
mod reading_mode_tests;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "tests/rendering_tests/mod.rs"]
mod rendering_tests;
mod settings;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "tests/source_navigation_tests.rs"]
mod source_navigation_tests;
mod typography;
mod updates;
#[cfg(all(test, feature = "desktop-ui-tests"))]
#[path = "tests/updates_tests.rs"]
mod updates_tests;

mod annotation_editor;
mod appearance;
mod blocks;
mod bootstrap;
mod formulas;
mod image_preview;
mod init;
mod library_actions;
mod margin_editor;
mod navigation_menu;
mod note_actions;
mod note_cards;
mod note_navigation;
mod notes_panel;
mod pdf_loading;
mod selection;
mod source_navigation;
mod state;
mod tasks;
mod text;
mod view;

pub use bootstrap::run;

const READER_BACKGROUND: u32 = 0xf6f3ec;

#[derive(Clone, Copy, PartialEq)]
enum Navigation {
    Library,
    Contents,
}
#[derive(Clone, Copy, PartialEq)]
enum Panel {
    Notes,
    Edit,
    Chat,
}
enum Event {
    Catalog(anyhow::Result<Vec<Value>>),
    Loaded(Workspace, Value),
    Refresh(Value),
    SavedDraft(String, Option<EditDraft>, Option<NoteDraft>, bool),
    InlineEditSaved(std::path::PathBuf, Result<Value, String>),
    Formula(String, Result<Arc<formula::FormulaLayout>, String>),
    InlineFormula(String, Result<Arc<formula::FormulaLayout>, String>),
    Delta(String),
    Status(String),
    Error(String),
    SettingsResult(String),
    Relocated(Library, Option<Workspace>, Option<Value>),
}
#[derive(Clone)]
struct Selection {
    block: usize,
    key: String,
    quote: String,
    prefix: String,
    suffix: String,
    position: Point<Pixels>,
    underline: bool,
}
type BlockImageResult = Result<(Arc<Image>, f32, u32), String>;

struct ImagePreview {
    image: Arc<Image>,
    aspect: f32,
    width: u32,
    zoom: f32,
    scroll: ScrollHandle,
}

struct MarginEditor {
    selection: Selection,
    note: MarginNote,
    changed_at: Instant,
    // Reserve the gutter while the new quote anchor is being painted. The
    // token also prevents an old frame callback from revealing another editor.
    placement_pending: Option<Instant>,
}

struct AnnotationScroll {
    path: ScrollPath,
    started: Instant,
    duration: Duration,
    generation: usize,
    refine: Option<Value>,
}

struct ReaderView {
    reader_focus: FocusHandle,
    home: home::HomeState,
    settings: settings::SettingsState,
    updates: updates::UpdateUi,
    raw: Value,
    state: ReaderState,
    epub_code_language: Option<&'static str>,
    epub_reflow: epub::ReflowState,
    epub_reflow_displayed: (usize, usize, usize),
    layout: PageLayout,
    library: Option<Library>,
    workspace: Option<Workspace>,
    items: Vec<Value>,
    session: Option<Session>,
    persisted_session: Option<Session>,
    editor_base: String,
    inline_edit: Option<inline_edit::InlineTranslationEdit>,
    restore_inputs: bool,
    draft_conflict: bool,
    reading_mode: ReadingMode,
    original_first: bool,
    reading_order_open: bool,
    reading_order_button_bounds: Option<Bounds<Pixels>>,
    reading_order_menu_bounds: Option<Bounds<Pixels>>,
    appearance: Appearance,
    appearance_open: bool,
    navigation: Option<Navigation>,
    pdf_outline: Vec<easyread_gpui::pdf::OutlineEntry>,
    navigation_focus: FocusHandle,
    navigation_scroll: ScrollHandle,
    navigation_button_bounds: [Option<Bounds<Pixels>>; 2],
    navigation_menu_bounds: Option<Bounds<Pixels>>,
    appearance_focus: FocusHandle,
    appearance_button_bounds: Option<Bounds<Pixels>>,
    appearance_menu_bounds: Option<Bounds<Pixels>>,
    font_slider: Entity<SliderState>,
    width_slider: Entity<SliderState>,
    spacing_slider: Entity<SliderState>,
    show_pdf: bool,
    panel: Panel,
    panel_open: bool,
    annotation_filter: AnnotationFilter,
    paper_notes_tab: bool,
    paper_note_input: Entity<TextareaState>,
    restore_paper_note: bool,
    selected: usize,
    active_reading_paragraph: Option<(usize, bool, usize)>,
    selection: Option<Selection>,
    annotation_popup: Option<(String, Point<Pixels>)>,
    margin_editor: Option<MarginEditor>,
    margin_input: Entity<TextareaState>,
    margin_writer: MarginWriter,
    margin_records: HashMap<String, Value>,
    margin_card_heights: HashMap<String, Pixels>,
    discussion_scrolls: std::cell::RefCell<HashMap<String, ScrollHandle>>,
    block_bounds: HashMap<usize, Bounds<Pixels>>,
    annotation_jump_generation: usize,
    navigation_note: Option<Value>,
    annotation_scroll: Option<AnnotationScroll>,
    hovered_annotation: Option<(String, usize)>,
    hovered_note_card: Option<String>,
    note_selection: Option<Selection>,
    note_kind: &'static str,
    text_views: HashMap<(usize, bool), (String, Entity<TextViewState>)>,
    pdf_code_html: HashMap<usize, (String, Option<easyread_gpui::pdf::CodePresentation>)>,
    pdf_code_loading: std::collections::HashSet<usize>,
    pdf_code_prepared: HashMap<usize, String>,
    pdf_code_cache: Option<PresentationCache>,
    pdf_code_active: bool,
    pdf_code_generation: usize,
    pdf_code_focus: usize,
    pdf_code_forward: bool,
    pdf_code_needs_plan: bool,
    // Per-block overrides for the secondary language (mode-relative).
    original_blocks: HashMap<usize, bool>,
    paragraph_languages: HashMap<usize, reading_mode::ParagraphExpansion>,
    unpaired_paragraphs: std::collections::HashSet<usize>,
    page: usize,
    page_block: Option<usize>,
    page_bounds: Option<(usize, Bounds<Pixels>)>,
    page_image: Option<Arc<Image>>,
    block_images: HashMap<usize, BlockImageResult>,
    image_loading: std::collections::HashSet<usize>,
    image_preview: Option<ImagePreview>,
    fonts: typography::Fonts,
    formulas: HashMap<String, Result<Arc<formula::FormulaLayout>, String>>,
    inline_formulas: HashMap<String, Result<Arc<formula::FormulaLayout>, String>>,
    rich_markdown_cache: std::cell::RefCell<math::MarkdownCache>,
    list: ListState,
    chat_list: ListState,
    chat_following: bool,
    chat_ui: chat::ChatUi,
    editor: Entity<TextareaState>,
    note: Entity<TextareaState>,
    quote: Entity<InputState>,
    question: Entity<TextareaState>,
    search: Entity<InputState>,
    tid: String,
    live: String,
    live_question: String,
    status: String,
    busy: bool,
    cancel: Arc<AtomicBool>,
}
impl Drop for ReaderView {
    fn drop(&mut self) {
        self.queue_margin_note();
        self.cancel.store(true, Ordering::Relaxed);
        self.flush_session();
    }
}
