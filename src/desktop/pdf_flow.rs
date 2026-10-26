//! Budget only the AI answer's scroll viewport, never the personal-note column.
//! Footnotes and personal notes retain their normal flow.
use super::*;

impl ReaderView {
    pub(super) fn pdf_margin_height(&self, index: usize, cx: &App) -> Option<Pixels> {
        let block = self.state.paper.blocks.get(index)?;
        if block.source_kind != "pdf-text"
            || self
                .margin_editor
                .as_ref()
                .is_some_and(|editor| editor.selection.block == index)
        {
            return None;
        }
        let mut height = px(0.);
        for original in self.rendered_languages(index) {
            let (_, view) = self.text_views.get(&(index, original))?;
            if !view.read(cx).is_content_ready() {
                return None;
            }
            // Bilingual paragraph pairing can share one layout between the
            // language views. Do not count that page height twice.
            height = height.max(view.read(cx).bounds().size.height);
        }
        Some((height + px(32.)).max(px(160.)))
    }
}
