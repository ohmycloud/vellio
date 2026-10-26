//! Settings shortcuts.
use super::*;

impl ReaderView {
    pub(in crate::desktop) fn shortcut_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = &self.settings.draft;
        let mut body=div().flex().flex_col().child(div().flex().items_center().gap(px(24.)).mb(px(12.)).child(div().flex_1().flex().flex_col().gap(px(10.)).child(div().text_size(px(14.)).font_weight(FontWeight::MEDIUM).child("Enable keyboard shortcuts")).child(muted("When off, only Esc works. Single-letter shortcuts never fire while you’re typing, but do fire when the page has focus.").line_height(relative(1.7))))
            .child(Switch::new("shortcuts-enabled").checked(p.shortcuts_enabled).color(rgb(ACCENT)).accessibility_label("Enable keyboard shortcuts").on_click(cx.listener(|this,checked:&bool,_,cx|{this.settings.draft.shortcuts_enabled = *checked;cx.notify()}))));
        let mut group = "";
        for (id, label, _, category) in SHORTCUTS {
            if category != group {
                group = category;
                body = body.child(section(category));
            }
            let key = p.shortcuts.get(id).cloned().unwrap_or_default();
            let recording = self.settings.recording.as_deref() == Some(id);
            body = body.child(
                div()
                    .h(px(46.))
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .child(
                        div()
                            .flex_1()
                            .child(label)
                            .when(!p.action_enabled(id), |d| d.child(muted(" (feature off)"))),
                    )
                    .child(
                        outline(
                            SharedString::from(format!("shortcut-{id}")),
                            if recording {
                                "Press a key…".into()
                            } else if key.is_empty() {
                                "Not set".into()
                            } else {
                                key.to_uppercase()
                            },
                        )
                        .w(px(96.))
                        .bg(rgb(0xf6f3ec))
                        .disabled(id == "retranslate")
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.settings.recording = Some(id.into());
                                this.settings.focus.focus(window, cx);
                                cx.notify();
                            },
                        )),
                    )
                    .child(
                        Button::new(SharedString::from(format!("clear-shortcut-{id}")))
                            .ghost()
                            .label("Clear")
                            .text_color(rgb(MUTED))
                            .disabled(id == "retranslate")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.settings
                                    .draft
                                    .shortcuts
                                    .insert(id.into(), String::new());
                                cx.notify()
                            })),
                    ),
            );
        }
        body.child(div().mt(px(22.)).flex().items_center().gap(px(20.)).child(muted("After selecting text: 1–4 highlight in four colors, N adds a note, Q asks a question. Follows the main switch.").line_height(relative(1.7)).flex_1())
            .child(Button::new("reset-shortcuts").ghost().label("Reset shortcuts").on_click(cx.listener(|this,_,_,cx|{this.settings.draft.shortcuts=preferences::default_shortcuts();this.settings.recording=None;cx.notify();})))).into_any_element()
    }

    pub(in crate::desktop) fn reader_shortcut(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let modifiers = event.keystroke.modifiers;
        if modifiers.control
            || modifiers.platform
            || modifiers.alt
            || modifiers.function
            || self.busy
            || self.settings.open
            || self.navigation.is_some()
            || self.appearance_open
            || self.reading_order_open
        {
            return;
        }
        let typing = [
            self.editor.read(cx).focus_handle(cx),
            self.note.read(cx).focus_handle(cx),
            self.quote.read(cx).focus_handle(cx),
            self.question.read(cx).focus_handle(cx),
            self.search.read(cx).focus_handle(cx),
            self.margin_input.read(cx).focus_handle(cx),
            self.paper_note_input.read(cx).focus_handle(cx),
        ]
        .iter()
        .any(|h| h.is_focused(window));
        if typing {
            return;
        }
        let key = event.keystroke.key.to_lowercase();
        if self.settings.committed.shortcuts_enabled
            && self.selection.is_some()
            && let Ok(number) = key.parse::<usize>()
            && (1..=4).contains(&number)
        {
            self.selection_action("highlight", HighlightColor::ALL[number - 1], window, cx);
            cx.stop_propagation();
            return;
        }
        if self.is_epub()
            && self.settings.committed.shortcuts_enabled
            && self.settings.committed.action(&key).is_none()
            && matches!(key.as_str(), "left" | "right" | "pageup" | "pagedown")
        {
            self.turn_epub_page(matches!(key.as_str(), "right" | "pagedown"), cx);
            cx.stop_propagation();
            return;
        }
        let Some(action) = self.settings.committed.action(&key).map(str::to_owned) else {
            return;
        };
        let index = self
            .list
            .logical_scroll_top()
            .item_ix
            .min(self.state.paper.blocks.len().saturating_sub(1));
        if self.state.paper.blocks.is_empty() {
            return;
        }
        match action.as_str() {
            "next" | "previous" => {
                let next = if action == "next" {
                    (index + 1).min(self.state.paper.blocks.len() - 1)
                } else {
                    index.saturating_sub(1)
                };
                self.select(next, window, cx);
                self.list.scroll_to(ListOffset {
                    item_ix: next,
                    offset_in_item: px(0.),
                });
            }
            "contents" => {
                self.navigation = if self.navigation == Some(Navigation::Contents) {
                    None
                } else {
                    Some(Navigation::Contents)
                };
                if self.navigation.is_some() {
                    self.navigation_focus.focus(window, cx);
                }
            }
            "bilingual" => {
                self.toggle_bilingual(cx);
            }
            "larger" | "smaller" | "reset_size" => {
                self.appearance.font_size = if action == "reset_size" {
                    Appearance::default().font_size
                } else {
                    self.appearance.font_size + if action == "larger" { 1. } else { -1. }
                };
                self.appearance.normalize();
                self.appearance_changed(cx);
            }
            "notes" => self.show_panel(Panel::Notes, window, cx),
            "ask" => {
                if self.selection.is_some() {
                    self.selection_action("chat", HighlightColor::Yellow, window, cx)
                } else {
                    self.select(index, window, cx);
                    self.show_panel(Panel::Chat, window, cx);
                    self.question.update(cx, |input, cx| {
                        input.set_value(format!("> {}\n\n", self.text_at(index)), window, cx)
                    });
                    self.question.read(cx).focus_handle(cx).focus(window, cx);
                }
            }
            "edit" => {
                let index = self
                    .active_reading_paragraph
                    .map(|(block, _, _)| block)
                    .unwrap_or(index);
                self.begin_inline_edit(index, window, cx);
            }
            "page" | "locate_page" => {
                if action == "page" && self.show_pdf {
                    self.show_pdf = false;
                    self.reset_rows();
                } else {
                    self.locate_source(index, cx);
                }
            }
            "previous_page" | "next_page" if self.is_epub() => {
                self.turn_epub_page(action == "next_page", cx);
            }
            "previous_page" | "next_page" => {
                self.show_pdf = true;
                self.panel_open = false;
                self.page_block = None;
                let pages = self.state.paper.meta.pages.len().max(1);
                self.page = if action == "previous_page" {
                    self.page.saturating_sub(1).max(1)
                } else {
                    (self.page + 1).min(pages)
                };
                self.load_page();
            }
            "note" | "question" => {
                let kind = if action == "note" { "note" } else { "question" };
                if self.selection.is_some() {
                    self.selection_action(kind, HighlightColor::Yellow, window, cx)
                } else {
                    self.select(index, window, cx);
                    let block = &self.state.paper.blocks[index];
                    let selection = Selection {
                        block: index,
                        key: block.id.clone(),
                        quote: String::new(),
                        prefix: String::new(),
                        suffix: String::new(),
                        position: point(px(0.), px(0.)),
                        underline: false,
                    };
                    self.open_margin_note(selection, kind, HighlightColor::Yellow, window, cx);
                }
            }
            "original" => {
                if self.selection.is_some() {
                    self.selection_action("original", HighlightColor::Yellow, window, cx)
                } else {
                    self.toggle_block_language(index, cx);
                }
            }
            "copy" => {
                if self.selection.is_some() {
                    self.selection_action("copy", HighlightColor::Yellow, window, cx)
                } else {
                    cx.write_to_clipboard(ClipboardItem::new_string(self.text_at(index)));
                }
            }
            _ => {}
        }
        self.capture_position();
        self.flush_session();
        cx.stop_propagation();
        cx.notify();
    }
}
