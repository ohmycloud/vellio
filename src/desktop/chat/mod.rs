//! Ask AI: EasyRead's slide-in reader column, context chips and streaming chat.
use super::*;
use easyread_gpui::ai::{AnswerStyle, ChatReference, ChatRequest};

pub(super) const SLIDE_MS: u64 = 220;

pub(super) struct ChatUi {
    pub generation: usize,
    pub slide_started: Option<Instant>,
    pub closing: bool,
    pub streaming: bool,
    pub list_open: bool,
    pub model_open: bool,
    pub style_open: bool,
    pub model: String,
    pub reasoning: Option<String>,
    pub model_focus: FocusHandle,
    pub model_cursor: usize,
    pub model_scroll: ScrollHandle,
    pub model_bounds: Bounds<Pixels>,
    pub model_menu_width: f32,
    pub style: AnswerStyle,
    pub style_focus: FocusHandle,
    pub style_cursor: usize,
    pub style_scroll: ScrollHandle,
    pub style_bounds: Bounds<Pixels>,
    pub style_menu_width: f32,
    pub refs: Vec<ChatReference>,
    pub live_refs: Vec<ChatReference>,
    pub no_auto: bool,
    pub pending_note: Option<String>,
    pub error: Option<String>,
    pub rename_id: Option<String>,
    pub delete_id: Option<String>,
    pub rename_input: Entity<InputState>,
    pub resizing: bool,
}
impl ChatUi {
    pub fn new(window: &mut Window, cx: &mut Context<ReaderView>) -> Self {
        Self {
            generation: 0,
            slide_started: None,
            closing: false,
            streaming: false,
            list_open: false,
            model_open: false,
            style_open: false,
            model: String::new(),
            reasoning: None,
            model_focus: cx.focus_handle(),
            model_cursor: 0,
            model_scroll: ScrollHandle::new(),
            model_bounds: Bounds::default(),
            model_menu_width: 370.,
            style: AnswerStyle::default(),
            style_focus: cx.focus_handle(),
            style_cursor: 0,
            style_scroll: ScrollHandle::new(),
            style_bounds: Bounds::default(),
            style_menu_width: 500.,
            refs: vec![],
            live_refs: vec![],
            no_auto: false,
            pending_note: None,
            error: None,
            rename_id: None,
            delete_id: None,
            rename_input: cx.new(|cx| InputState::new(window, cx)),
            resizing: false,
        }
    }
}

/// CSS cubic-bezier(.2,.8,.2,1), including inversion of its x coordinate.
pub(super) fn slide_ease(progress: f32) -> f32 {
    if progress <= 0. {
        return 0.;
    }
    if progress >= 1. {
        return 1.;
    }
    let curve = |t: f32, a: f32, b: f32| {
        3. * (1. - t).powi(2) * t * a + 3. * (1. - t) * t * t * b + t.powi(3)
    };
    let (mut low, mut high) = (0., 1.);
    for _ in 0..18 {
        let mid = (low + high) / 2.;
        if curve(mid, 0.2, 0.2) < progress {
            low = mid;
        } else {
            high = mid;
        }
    }
    curve((low + high) / 2., 0.8, 1.)
}

mod actions;
mod resize;
mod view;
