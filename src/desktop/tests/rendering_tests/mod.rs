//! The reader must not paint a provisional PDF AST while formatting is pending.
use super::{ReaderView, ReadingMode};
use easyread_gpui::pdf::CodePresentation;
use gpui_kit::{AppContext, ListOffset, TestAppContext, VisualTestContext, component::Root, px};
use serde_json::{Value, json};

mod epub;
mod figures;
mod loading;
mod math;
mod selection;
mod tables;

// Existing regressions exercise the translated/active body, not the new default.
fn translated_reader(
    raw: Value,
    library: Option<easyread_gpui::store::Library>,
    workspace: Option<easyread_gpui::store::Workspace>,
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::Context<ReaderView>,
) -> ReaderView {
    let mut reader = ReaderView::new(raw, library, workspace, window, cx);
    reader.reading_mode = ReadingMode::Translation;
    reader
}

#[allow(dead_code)]
#[path = "../../../../tests/support/mod.rs"]
mod pdf_fixtures;

#[path = "../../../../tests/support/math.rs"]
mod math_fixtures;

#[path = "../../../../tests/support/epub.rs"]
mod epub_fixtures;

fn fixture() -> Value {
    json!({
        "paper": {
            "meta": {"title_en": "PDF formatting regression"},
            "blocks": [{"id": "code-page", "type": "para", "page": 323,
                "source_kind": "pdf-text", "en": "def run():\n    return 'formatted code'"}]
        },
        "reader": {"notes": {}}
    })
}

fn draw(cx: &mut VisualTestContext) {
    // Deliberately do not run the parser executor before inspecting this frame.
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        window.draw(cx).clear(cx);
    });
}

fn finish_presentation(
    reader: &gpui_kit::Entity<ReaderView>,
    html: Option<String>,
    cx: &mut VisualTestContext,
) {
    reader.update(cx, |reader, cx| {
        let source = reader.state.paper.blocks[0].en.clone();
        reader.pdf_code_loading.remove(&0);
        reader.pdf_code_html.insert(
            0,
            (
                source,
                html.map(|html| CodePresentation {
                    html,
                    math: vec![],
                    inline_figures: vec![],
                    figures: vec![],
                }),
            ),
        );
        reader.list.remeasure_items(0..1);
        cx.notify();
    });
}
