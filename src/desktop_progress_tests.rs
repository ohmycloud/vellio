use super::ReaderView;
use gpui_kit::{AppContext, ListOffset, TestAppContext, VisualTestContext, component::Root, px};
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
fn reading_progress_tracks_scroll_offsets_and_document_end(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let text = "Reading progress follows the document, not the selected paragraph. ".repeat(50);
    let blocks: Vec<_> = (0..30)
        .map(|i| json!({"id":format!("b{i}"),"type":"para","en":text,"page":1}))
        .collect();
    let raw =
        json!({"paper":{"meta":{"title_en":"Progress regression"},"blocks":blocks},"reader":{}});
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(raw, None, None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    settle(cx);
    let bar = cx.debug_bounds("reading-progress").unwrap();
    // GPUI snaps layout bounds to the device pixel grid.
    assert!((f32::from(bar.size.height) - 1.854).abs() <= 0.5);
    assert!(bar.size.width > px(0.));
    reader.read_with(cx, |reader, _| assert_eq!(reader.reading_progress(), 0.));
    reader.update(cx, |reader, cx| {
        reader.list.scroll_to(ListOffset {
            item_ix: 15,
            offset_in_item: px(0.),
        });
        cx.notify();
    });
    settle(cx);
    let middle = reader.read_with(cx, |reader, _| reader.reading_progress());
    assert!((middle - 0.5).abs() < 0.01);
    reader.update(cx, |reader, cx| {
        reader.list.scroll_to(ListOffset {
            item_ix: 15,
            offset_in_item: px(100.),
        });
        cx.notify();
    });
    settle(cx);
    reader.read_with(cx, |reader, _| {
        assert!(reader.reading_progress() > middle);
        assert!(reader.reading_progress() < 1.);
    });
    reader.update(cx, |reader, cx| {
        reader.list.scroll_to_end();
        cx.notify();
    });
    settle(cx);
    reader.read_with(cx, |reader, _| assert_eq!(reader.reading_progress(), 1.));
}

#[gpui_kit::test]
fn reading_progress_restores_epub_anchor_and_tracks_pages(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let text = "Real measured pages must advance progress even inside one paragraph. ".repeat(200);
    let raw = json!({"paper":{"meta":{"format":"epub","title_en":"EPUB progress"},"blocks":[
        {"id":"first","type":"para","en":text,"html":format!("<p>{text}</p>")},
        {"id":"restored","type":"para","en":text,"html":format!("<p>{text}</p>")}
    ]},"reader":{"progress":{"block":"restored"}}});
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(raw, None, None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    settle(cx);
    let restored = reader.read_with(cx, |reader, _| {
        assert!(reader.epub_reflow.complete);
        assert!(reader.epub_reflow.pages.len() > 4);
        assert_eq!(reader.list.logical_scroll_top().item_ix, 1);
        reader.reading_progress()
    });
    assert!(restored > 0. && restored < 1.);
    reader.update(cx, |reader, cx| reader.turn_epub_page(true, cx));
    settle(cx);
    reader.read_with(cx, |reader, _| {
        assert!(reader.reading_progress() > restored)
    });
    reader.update(cx, |reader, cx| {
        let turns = reader.epub_reflow.pages.len();
        for _ in 0..turns {
            reader.turn_epub_page(true, cx);
        }
    });
    settle(cx);
    reader.read_with(cx, |reader, _| assert_eq!(reader.reading_progress(), 1.));
    reader.update(cx, |reader, cx| {
        reader.list.scroll_to(ListOffset {
            item_ix: 0,
            offset_in_item: px(0.),
        });
        cx.notify();
    });
    settle(cx);
    reader.read_with(cx, |reader, _| assert_eq!(reader.reading_progress(), 0.));
}

#[gpui_kit::test]
fn reading_progress_is_zero_for_empty_document(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let raw = json!({"paper":{"meta":{"title_en":"Empty"},"blocks":[]},"reader":{}});
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(raw, None, None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    settle(cx);
    reader
        .unwrap()
        .read_with(cx, |reader, _| assert_eq!(reader.reading_progress(), 0.));
    assert!(cx.debug_bounds("reading-progress").is_some());
}
