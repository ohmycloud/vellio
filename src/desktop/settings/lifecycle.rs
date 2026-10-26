//! Settings lifecycle.
use super::*;

impl ReaderView {
    pub(in crate::desktop) fn open_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.capture_inputs(cx);
        self.capture_position();
        self.flush_session();
        self.queue_margin_note();
        self.appearance_open = false;
        self.reading_order_open = false;
        self.navigation = None;
        self.selection = None;
        self.settings.open = true;
        self.settings.draft = self.settings.committed.clone();
        self.settings.draft.appearance = if self.home.visible {
            self.settings.committed.appearance.clone()
        } else {
            self.appearance.clone()
        };
        self.settings.model_edit = None;
        self.settings.model_menu = None;
        self.settings.recording = None;
        self.settings.log_open = false;
        self.settings.scroll = ScrollHandle::new();
        self.settings.selects.clear();
        gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Light, Some(window), cx);
        typography::apply_theme(false, cx);
        self.settings.focus.focus(window, cx);
        self.init_settings_selects(window, cx);
        cx.notify();
    }

    pub(in crate::desktop) fn commit_model_settings(&mut self) -> bool {
        let mut committed = self.settings.committed.clone();
        committed.models = self.settings.draft.models.clone();
        committed.translation_model = self.settings.draft.translation_model.clone();
        committed.ask_model = self.settings.draft.ask_model.clone();
        committed.pages_per_batch = self.settings.draft.pages_per_batch;
        committed.parallel = self.settings.draft.parallel;
        committed.ask_above = self.settings.draft.ask_above;
        match self
            .library
            .as_ref()
            .map(|l| committed.save(&l.root))
            .unwrap_or(Ok(()))
        {
            Ok(()) => {
                self.settings.committed = committed;
                true
            }
            Err(e) => {
                self.settings.notice = format!("Could not save models: {e}");
                false
            }
        }
    }

    pub(in crate::desktop) fn close_settings(
        &mut self,
        save: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings.relocating {
            return;
        }
        if save {
            self.settings.draft.appearance.normalize();
            let result = self.settings.draft.validate().and_then(|_| {
                self.library
                    .as_ref()
                    .map(|l| self.settings.draft.save(&l.root))
                    .unwrap_or(Ok(()))
            });
            if let Err(e) = result {
                self.settings.notice = e.to_string();
                cx.notify();
                return;
            }
            self.settings.committed = self.settings.draft.clone();
            self.appearance = self.settings.committed.appearance.clone();
            self.reading_mode = if self.settings.committed.bilingual {
                ReadingMode::Bilingual
            } else {
                ReadingMode::Original
            };
            self.original_blocks.clear();
            self.clear_paragraph_layouts();
            self.enforce_features();
            self.appearance_changed(cx);
            self.flush_session();
        }
        self.settings.open = false;
        self.settings.recording = None;
        if self.home.visible {
            self.home.apply_theme(Some(window), cx);
        }
        window.blur(cx);
        cx.notify();
    }

    pub(in crate::desktop) fn enforce_features(&mut self) {
        let f = self.settings.committed.features.clone();
        if !f.original {
            self.reading_mode = ReadingMode::Translation;
            self.reading_order_open = false;
            self.original_blocks.clear();
            self.clear_paragraph_layouts();
        }
        if !f.page {
            self.show_pdf = false;
        }
        if !f.edit {
            self.inline_edit = None;
        }
        if self.panel_open
            && ((self.panel == Panel::Chat && !f.ask_ai) || (self.panel == Panel::Edit && !f.edit))
        {
            self.panel_open = false;
        }
    }
}
