//! Full settings surface with fixed header/footer and independently scrolling tabs.
use super::*;
use easyread_gpui::settings::{self as preferences, Preferences, SHORTCUTS};
use gpui_kit::base::Dialog as Modal;
use gpui_kit::component::{
    IndexPath,
    select::{Select, SelectEvent, SelectState},
    switch::Switch,
};
use std::path::PathBuf;

const INK: u32 = 0x26231f;
const MUTED: u32 = 0x8c8579;
const BORDER: u32 = 0xd8cfbf;
const ACCENT: u32 = 0x306778;
const PAPER: u32 = 0xfbfaf7;
const TABS: [&str; 6] = [
    "Model",
    "Reading",
    "Sidebar",
    "Cloud library",
    "Keyboard shortcuts",
    "About / Updates",
];
pub(super) struct SettingsState {
    pub open: bool,
    pub committed: Preferences,
    draft: Preferences,
    tab: usize,
    scroll: ScrollHandle,
    selects: HashMap<&'static str, Entity<SelectState<Vec<String>>>>,
    pub focus: FocusHandle,
    recording: Option<String>,
    model_edit: Option<ai::ModelProfile>,
    model_menu: Option<String>,
    inputs: HashMap<&'static str, Entity<InputState>>,
    drives: Vec<(String, PathBuf)>,
    pub notice: String,
    log_open: bool,
    pub logs: Vec<String>,
    pub relocating: bool,
    pub translation_confirm: bool,
}
impl SettingsState {
    pub(super) fn show_about(&mut self) {
        self.tab = 5;
        self.scroll = ScrollHandle::new();
        self.log_open = false;
    }
    pub fn new(lib: Option<&Library>, window: &mut Window, cx: &mut Context<ReaderView>) -> Self {
        let (mut committed, notice) = match Preferences::load(lib.map(|l| l.root.as_path())) {
            Ok(p) => (p, String::new()),
            Err(e) => (
                Preferences::default(),
                format!("Settings could not be read: {e}"),
            ),
        };
        if committed.models.is_empty() {
            for (provider, id, name, model) in [
                (
                    ai::Provider::ClaudeCode,
                    "claude-opus",
                    "Claude Opus",
                    "opus",
                ),
                (
                    ai::Provider::ClaudeCode,
                    "claude-sonnet",
                    "Claude Sonnet",
                    "sonnet",
                ),
                (
                    ai::Provider::CodexCli,
                    "codex-default",
                    "Codex default",
                    "default",
                ),
            ] {
                if ai::cli_executable(provider, "").is_ok() {
                    committed.models.push(ai::ModelProfile {
                        id: id.into(),
                        name: name.into(),
                        provider,
                        model: model.into(),
                        ..Default::default()
                    });
                }
            }
            if let Some(first) = committed.models.first() {
                committed.translation_model = first.id.clone();
                committed.ask_model = first.id.clone();
            }
        }
        let inputs = [
            ("name", "Display name"),
            ("model", "Model ID or CLI alias"),
            ("base", "https://api.openai.com/v1"),
            ("key", "API key environment variable"),
            ("executable", "Auto-detect (or absolute path)"),
            ("folder", "New folder name"),
        ]
        .map(|(id, placeholder)| {
            (
                id,
                cx.new(|cx| InputState::new(window, cx).placeholder(placeholder)),
            )
        })
        .into_iter()
        .collect();
        Self {
            open: false,
            draft: committed.clone(),
            committed,
            tab: 0,
            scroll: ScrollHandle::new(),
            selects: HashMap::new(),
            focus: cx.focus_handle(),
            recording: None,
            model_edit: None,
            model_menu: None,
            inputs,
            drives: preferences::cloud_drives(),
            notice,
            log_open: false,
            logs: vec![],
            relocating: false,
            translation_confirm: false,
        }
    }
}
fn section(title: impl Into<SharedString>) -> Div {
    div()
        .mt(px(26.))
        .mb(px(18.))
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(MUTED))
        .child(title.into())
}
fn muted(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(12.5))
        .line_height(relative(1.6))
        .text_color(rgb(MUTED))
        .child(text.into())
}
fn outline(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id)
        .label(label)
        .text_size(px(13.5))
        .h(px(36.))
        .px(px(14.))
        .rounded(px(9.))
        .bg(rgb(PAPER))
        .border_color(rgb(BORDER))
        .text_color(rgb(0x625e55))
}
fn primary(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id)
        .primary()
        .label(label)
        .text_size(px(13.5))
        .h(px(38.))
        .px(px(22.))
        .rounded(px(9.))
        .bg(rgb(ACCENT))
        .text_color(rgb(0xffffff))
}

fn font_label(font: ReadingFont) -> &'static str {
    match font {
        ReadingFont::NotoSerifSc => "宋体 · Noto Serif SC",
        ReadingFont::SystemSerif => "System serif",
        ReadingFont::SansSerif => "Sans serif",
    }
}
fn effort_label(effort: &str) -> &'static str {
    match effort {
        "low" => "Low",
        "medium" => "Medium",
        "high" => "High",
        _ => "Default",
    }
}

mod library;
mod lifecycle;
mod models;
mod reading;
mod selects;
mod shortcuts;
mod sidebar;
mod view;
