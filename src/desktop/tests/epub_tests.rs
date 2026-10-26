use super::ReaderView;
use gpui_kit::{
    AppContext, ListOffset, ScrollDelta, ScrollWheelEvent, TestAppContext, VisualTestContext,
    component::Root, point, px, size,
};
use serde_json::json;

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..24 {
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.draw(cx).clear(cx);
        });
    }
}

#[gpui_kit::test]
fn native_epub_pages_split_measured_rows_turn_reflow_and_preserve_source(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let text = "中文排版与 English mixed text should wrap by actual font metrics, not by character counts. ".repeat(150);
    let raw = json!({"paper":{"meta":{"format":"epub","title_en":"Pagination regression"},"blocks":[
        {"id":"title","type":"heading","source_kind":"epub-xhtml","chapter":1,"en":"Chapter One","html":"<h1>Chapter One</h1>"},
        {"id":"long","type":"para","source_kind":"epub-xhtml","chapter":1,"en":text,"html":format!("<p><em>{}</em></p>",text)},
        {"id":"second","type":"heading","source_kind":"epub-xhtml","chapter":2,"en":"Chapter Two","html":"<h1>Chapter Two</h1>"}
    ]},"reader":{}});
    let source = raw["paper"].clone();
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut reader = ReaderView::new(raw, None, None, window, cx);
            reader.reading_mode = super::ReadingMode::Translation;
            reader
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    settle(cx);
    let (before, second_start) = reader.read_with(cx, |reader, cx| {
        assert!(reader.epub_reflow.complete);
        let pages = &reader.epub_reflow.pages;
        assert!(
            pages.len() > 4,
            "a long paragraph must span many real pages"
        );
        let height = f32::from(reader.epub_reflow.size.unwrap().height);
        for page in pages {
            assert!(page.iter().map(|f| f.end - f.start).sum::<f32>() <= height + 0.1);
        }
        let lines = reader.text_views[&(1, false)]
            .1
            .read(cx)
            .page_break_positions();
        assert!(
            lines.len() > 50,
            "collect native rows rather than estimated text lengths"
        );
        let origin = reader.block_bounds[&1].top();
        let last = pages.iter().flatten().rfind(|f| f.block == 1).unwrap().end;
        for fragment in pages
            .iter()
            .flatten()
            .filter(|f| f.block == 1 && f.end < last)
        {
            assert!(
                lines
                    .iter()
                    .any(|line| (f32::from(*line - origin) - fragment.end).abs() < 1.0),
                "page cut {} must coincide with a native glyph-row boundary",
                fragment.end
            );
        }
        assert_eq!(reader.raw["paper"], source);
        (pages.len(), pages[1][0].clone())
    });
    reader.update(cx, |reader, cx| reader.turn_epub_page(true, cx));
    settle(cx);
    reader.read_with(cx, |reader, cx| {
        assert_eq!(reader.epub_reflow.current, 1);
        let anchor = reader.list.logical_scroll_top();
        assert_eq!(anchor.item_ix, second_start.block);
        assert_eq!(f32::from(anchor.offset_in_item), second_start.start);
        assert_eq!(
            reader.state.paper.blocks.len(),
            3,
            "pagination must not create synthetic content blocks"
        );
        let view = reader.text_views[&(1, false)].1.read(cx);
        assert!(view.bounds().size.height > px(0.));
    });
    reader.update(cx, |reader, cx| {
        reader.appearance.font_size = 32.;
        reader.appearance_changed(cx);
    });
    settle(cx);
    reader.read_with(cx, |reader, _| {
        assert!(reader.epub_reflow.complete);
        assert!(
            reader.epub_reflow.pages.len() > before,
            "font changes must repaginate real rows"
        );
        assert_eq!(reader.list.logical_scroll_top().item_ix, second_start.block);
        assert_eq!(reader.raw["paper"], source);
    });
    let (larger_font_pages, viewport) = reader.read_with(cx, |reader, _| {
        (
            reader.epub_reflow.pages.len(),
            reader.epub_reflow.size.unwrap(),
        )
    });
    cx.simulate_resize(size(viewport.width, viewport.height * 2. + px(100.)));
    settle(cx);
    reader.read_with(cx, |reader, _| {
        assert!(
            reader.epub_reflow.pages.len() < larger_font_pages,
            "window height changes must repaginate"
        );
        assert_eq!(reader.list.logical_scroll_top().item_ix, second_start.block);
    });
    reader.update(cx, |reader, cx| {
        reader.list.scroll_to(ListOffset {
            item_ix: 0,
            offset_in_item: px(0.),
        });
        cx.notify();
    });
    settle(cx);
    cx.simulate_keystrokes("right");
    settle(cx);
    reader.read_with(cx, |reader, _| assert_eq!(reader.epub_reflow.current, 1));
    cx.simulate_keystrokes("[");
    settle(cx);
    reader.read_with(cx, |reader, _| assert_eq!(reader.epub_reflow.current, 0));
    let position = reader.read_with(cx, |reader, _| reader.epub_reflow.bounds.unwrap().center());
    cx.simulate_event(ScrollWheelEvent {
        position,
        delta: ScrollDelta::Pixels(point(px(0.), px(-40.))),
        ..Default::default()
    });
    settle(cx);
    reader.read_with(cx, |reader, _| assert_eq!(reader.epub_reflow.current, 1));
    reader.update(cx, |reader, cx| {
        reader.list.scroll_to(ListOffset {
            item_ix: 2,
            offset_in_item: px(0.),
        });
        cx.notify();
    });
    settle(cx);
    reader.read_with(cx, |reader, _| {
        let page = &reader.epub_reflow.pages[reader.epub_reflow.current];
        assert_eq!(
            page[0].block, 2,
            "TOC anchors must resolve to the new chapter's page"
        );
        assert!(!reader.show_pdf);
    });
}
