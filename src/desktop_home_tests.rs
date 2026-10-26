use super::{ReaderView, catalog};
use easyread_gpui::store::{Library, write_json};
use gpui_kit::{AppContext, Focusable, TestAppContext, VisualTestContext, component::Root};
use serde_json::json;

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        window.draw(cx).clear(cx);
    });
}

#[cfg(unix)]
#[gpui_kit::test]
fn home_translation_queue_continues_after_failure_and_skips_trash(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt;
    cx.update(gpui_kit::init);
    let temp = tempfile::tempdir().unwrap();
    let lib = Library::open(temp.path().join("library")).unwrap();
    for (id, block) in [("book-aaaa", "a"), ("book-bbbb", "b"), ("book-cccc", "c")] {
        let root = lib.root.join(id);
        std::fs::create_dir(&root).unwrap();
        write_json(
            &root.join("paper.json"),
            &json!({"schema":2,"meta":{"title_en":id},"blocks":[
                {"id":block,"type":"para","page":1,"en":"Original"}
            ]}),
        )
        .unwrap();
    }
    write_json(
        &lib.root.join(".library-home.json"),
        &json!({"removed":{"book-cccc":true}}),
    )
    .unwrap();
    let executable = temp.path().join("fake-cli");
    std::fs::write(
        &executable,
        r#"#!/bin/sh
input=$(cat)
case "$input" in
  *'\"id\":\"a\"'*) printf '%s' '{"is_error":false,"result":"not JSON"}' ;;
  *) printf '%s' '{"is_error":false,"result":"{\"b\":\"译文\"}"}' ;;
esac
"#,
    )
    .unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let raw = json!({"paper":{"meta":{},"blocks":[]},"reader":{}});
        let view = cx.new(|cx| ReaderView::new(raw, Some(lib.clone()), None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    reader.update(cx, |view, cx| {
        view.settings.committed.models = vec![easyread_gpui::ai::ModelProfile {
            id: "stub".into(),
            name: "Explicit fixture".into(),
            model: "test".into(),
            provider: easyread_gpui::ai::Provider::ClaudeCode,
            executable: executable.display().to_string(),
            ..Default::default()
        }];
        view.settings.committed.translation_model = "stub".into();
        view.settings.committed.parallel = 1;
        view.home
            .selected
            .extend(["book-aaaa".into(), "book-bbbb".into(), "book-cccc".into()]);
        view.translate_home_selection(cx);
    });
    for _ in 0..200 {
        std::thread::sleep(std::time::Duration::from_millis(10));
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        draw(cx);
        if !reader.read_with(cx, |view, _| view.busy) {
            break;
        }
    }
    assert!(!reader.read_with(cx, |view, _| view.busy));
    let first = lib.workspace("book-aaaa").unwrap();
    assert_eq!(first.load("job").unwrap()["state"], "partial");
    assert!(first.load("paper").unwrap()["blocks"][0]["zh"].is_null());
    let second = lib.workspace("book-bbbb").unwrap();
    assert_eq!(second.load("job").unwrap()["state"], "done");
    assert_eq!(second.load("paper").unwrap()["blocks"][0]["zh"], "译文");
    assert!(
        !lib.workspace("book-cccc")
            .unwrap()
            .root
            .join("job.json")
            .exists()
    );
}

#[gpui_kit::test]
fn home_page_picker_only_offers_untranslated_source_pages(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let temp = tempfile::tempdir().unwrap();
    let lib = Library::open(temp.path()).unwrap();
    let root = lib.root.join("book-1234");
    std::fs::create_dir(&root).unwrap();
    let raw = json!({"paper":{"meta":{"title_en":"Page fixture"},"blocks":[
        {"id":"a","type":"para","page":1,"en":"First"},
        {"id":"b","type":"para","page":2,"en":"Done","zh":"已译"},
        {"id":"c","type":"para","page":3,"en":"Third"}
    ]},"reader":{}});
    write_json(&root.join("paper.json"), &raw["paper"]).unwrap();
    let before = std::fs::read(root.join("paper.json")).unwrap();
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(raw, Some(lib.clone()), None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    reader.update(cx, |view, cx| {
        view.home.selected.insert("book-1234".into());
        view.home.popup = Some("translation");
        cx.notify();
    });
    draw(cx);
    let toggle = cx.debug_bounds("home-choose-translation-pages").unwrap();
    cx.simulate_click(toggle.center(), gpui_kit::Modifiers::default());
    draw(cx);
    reader.read_with(cx, |view, _| {
        assert!(view.home.translation_pages.as_ref().unwrap().is_empty())
    });
    assert!(cx.debug_bounds("home-translation-page-2").is_none());
    let page = cx.debug_bounds("home-translation-page-3").unwrap();
    cx.simulate_click(page.center(), gpui_kit::Modifiers::default());
    draw(cx);
    reader.read_with(cx, |view, _| {
        assert_eq!(
            view.home.translation_pages,
            Some(std::collections::HashSet::from([3]))
        );
    });
    let page = cx.debug_bounds("home-translation-page-3").unwrap();
    cx.simulate_click(page.center(), gpui_kit::Modifiers::default());
    draw(cx);
    reader.read_with(cx, |view, _| {
        assert!(view.home.translation_pages.as_ref().unwrap().is_empty())
    });
    // Opening/cancelling the selector does not spend quota or mutate a book.
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert_eq!(std::fs::read(root.join("paper.json")).unwrap(), before);
    assert!(!root.join("job.json").exists());
}

#[gpui_kit::test]
fn home_rename_requires_one_selection_and_supports_cancel_and_enter(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let temp = tempfile::tempdir().unwrap();
    let lib = Library::open(temp.path()).unwrap();
    let root = lib.root.join("book-1234");
    std::fs::create_dir(&root).unwrap();
    let raw = json!({"paper":{"meta":{"title_en":"Original title"},"blocks":[]},"reader":{}});
    write_json(&root.join("paper.json"), &raw["paper"]).unwrap();
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(raw, Some(lib.clone()), None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    draw(cx);
    cx.update(|window, cx| {
        reader.update(cx, |view, cx| {
            view.open_home_rename(window, cx);
            assert_ne!(view.home.popup, Some("rename"));
            view.home.selected.insert("book-1234".into());
            view.home.selected.insert("other-book".into());
            view.open_home_rename(window, cx);
            assert_ne!(view.home.popup, Some("rename"));
            view.home.selected.remove("other-book");
            view.open_home_rename(window, cx);
            assert_eq!(
                view.home.rename_input.read(cx).value().as_ref(),
                "Original title"
            );
            assert!(
                view.home
                    .rename_input
                    .read(cx)
                    .focus_handle(cx)
                    .is_focused(window)
            );
        })
    });
    draw(cx);
    assert!(cx.debug_bounds("home-confirm-overlay").is_some());
    cx.simulate_keystrokes("escape");
    draw(cx);
    reader.read_with(cx, |view, _| {
        assert_eq!(view.home.popup, None);
        assert!(view.home.rename_id.is_none());
    });
    assert!(
        lib.workspace("book-1234").unwrap().load("paper").unwrap()["meta"]["display_name"]
            .is_null()
    );
    cx.update(|window, cx| {
        reader.update(cx, |view, cx| {
            view.open_home_rename(window, cx);
            view.home
                .rename_input
                .update(cx, |input, cx| input.set_value("  ", window, cx));
            view.rename_home_file(window, cx);
            assert_eq!(view.home.popup, Some("rename"));
            view.home
                .rename_input
                .update(cx, |input, cx| input.set_value("New name", window, cx));
        })
    });
    draw(cx);
    cx.simulate_keystrokes("enter");
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(10));
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        draw(cx);
        if !reader.read_with(cx, |view, _| view.busy) {
            break;
        }
    }
    reader.read_with(cx, |view, _| {
        assert!(!view.busy);
        assert_eq!(view.home.popup, None);
        assert_eq!(catalog::title(&view.items[0]), "New name");
    });
    assert_eq!(
        lib.workspace("book-1234").unwrap().load("paper").unwrap()["meta"]["display_name"],
        "New name"
    );
}

#[gpui_kit::test]
fn home_import_stays_in_place_during_tasks_and_opening_books(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let temp = tempfile::tempdir().unwrap();
    let lib = Library::open(temp.path()).unwrap();
    let root = lib.root.join("book-1234");
    std::fs::create_dir(&root).unwrap();
    let raw = json!({"paper":{"meta":{"title_en":"A book"},"blocks":[]},"reader":{}});
    write_json(&root.join("paper.json"), &raw["paper"]).unwrap();
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(raw, Some(lib.clone()), None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    draw(cx);
    let import_bounds = cx.debug_bounds("home-import").unwrap();
    assert!(cx.debug_bounds("home-cancel-task").is_none());

    // A cancellable import/translation has a cancel control below the top bar,
    // without changing Import's bounds. Gate the worker so the busy frame is deterministic.
    let (release, gate) = std::sync::mpsc::channel();
    reader.update(cx, |view, cx| {
        view.worker(cx, move |_, _| {
            gate.recv().unwrap();
        });
        view.home.cancellable_task = true;
    });
    draw(cx);
    assert_eq!(cx.debug_bounds("home-import").unwrap(), import_bounds);
    let cancel_bounds = cx.debug_bounds("home-cancel-task").unwrap();
    assert!(cancel_bounds.top() > import_bounds.bottom());
    release.send(()).unwrap();
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(10));
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        draw(cx);
        if !reader.read_with(cx, |view, _| view.busy) {
            break;
        }
    }
    reader.read_with(cx, |view, _| {
        assert!(!view.busy);
        assert!(!view.home.cancellable_task);
    });
    assert!(cx.debug_bounds("home-cancel-task").is_none());

    // Hold the workspace's write lock to inspect the actual book-open busy frame.
    let lock = root.join(".write.lock");
    std::fs::write(&lock, "test").unwrap();
    reader.update(cx, |view, cx| view.open("book-1234".into(), cx));
    draw(cx);
    reader.read_with(cx, |view, _| {
        assert!(view.busy);
        assert!(view.home.visible);
        assert!(!view.home.cancellable_task);
    });
    assert_eq!(cx.debug_bounds("home-import").unwrap(), import_bounds);
    assert!(cx.debug_bounds("home-cancel-task").is_none());
    std::fs::remove_file(lock).unwrap();
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(10));
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        draw(cx);
        if !reader.read_with(cx, |view, _| view.busy) {
            break;
        }
    }
    reader.read_with(cx, |view, _| {
        assert!(!view.busy);
        assert!(!view.home.visible);
        assert!(view.status.is_empty());
    });
    assert!(cx.debug_bounds("reader-status").is_none());
    assert!(cx.debug_bounds("reader-cancel-task").is_none());
}

#[gpui_kit::test]
fn reader_background_loading_does_not_flash_status_or_cancel(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let raw = json!({"paper":{"meta":{"title_en":"A book"},"blocks":[]},"reader":{}});
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| ReaderView::new(raw, None, None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    let (release, gate) = std::sync::mpsc::channel();
    reader.update(cx, |view, cx| {
        // Reproduce the old loaded/compiling frame: a routine recovery status
        // remains while a non-user-facing background worker is still busy.
        view.status = "论文已打开 · 已恢复草稿与阅读位置".into();
        view.worker(cx, move |_, _| {
            gate.recv().unwrap();
        });
    });
    draw(cx);
    reader.read_with(cx, |view, _| assert!(view.busy));
    assert!(cx.debug_bounds("reader-status").is_none());
    assert!(cx.debug_bounds("reader-cancel-task").is_none());
    reader.update(cx, |view, cx| {
        view.status.clear();
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("reader-status").is_none());
    assert!(cx.debug_bounds("reader-cancel-task").is_none());
    // Actual user-started cancellable work still has its controls.
    reader.update(cx, |view, cx| {
        view.home.cancellable_task = true;
        view.status = "正在翻译……".into();
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("reader-status").is_some());
    assert!(cx.debug_bounds("reader-cancel-task").is_some());
    release.send(()).unwrap();
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(10));
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        draw(cx);
        if !reader.read_with(cx, |view, _| view.busy) {
            break;
        }
    }
    reader.read_with(cx, |view, _| assert!(!view.busy));
    assert!(cx.debug_bounds("reader-cancel-task").is_none());
    // Genuine recovery/save warnings must remain visible.
    reader.update(cx, |view, cx| {
        view.status = "草稿恢复失败：请检查保存文件".into();
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("reader-status").is_some());
}

#[test]
fn selected_pelican_artwork_and_embedded_header_image_keep_brand_colours() {
    let master = include_str!("../assets/icons/Vellio.svg");
    let selected = include_str!("../assets/branding/vellio/pelican-concepts/a-orange-pelican.svg");
    assert_eq!(
        master.replace(
            "Vellio — orange-billed pelican</title>",
            "Vellio — orange-billed pelican concept</title>"
        ),
        selected,
        "production artwork must match the selected A, apart from its title"
    );
    let icon = image::load_from_memory(include_bytes!("../assets/icons/Vellio-small.png"))
        .unwrap()
        .to_rgba8();
    assert_eq!(icon.dimensions(), (64, 64));
    assert_eq!(icon.get_pixel(0, 0).0[3], 0, "transparent canvas corners");
    assert_eq!(icon.get_pixel(32, 8).0, [0x51, 0x6d, 0x86, 255]);
    assert_eq!(icon.get_pixel(20, 26).0, [0xff, 0xf8, 0xe8, 255]);
    assert_eq!(icon.get_pixel(39, 32).0, [0xff, 0x85, 0x3d, 255]);
    let preview = image::load_from_memory(include_bytes!("../assets/icons/Vellio.png")).unwrap();
    assert_eq!((preview.width(), preview.height()), (1024, 1024));
    let icns = include_bytes!("../assets/icons/Vellio.icns");
    assert_eq!(&icns[..4], b"icns");
    assert_eq!(
        u32::from_be_bytes(icns[4..8].try_into().unwrap()) as usize,
        icns.len()
    );
}

#[gpui_kit::test]
fn home_brand_icon_keeps_full_colour_artwork_across_theme_changes(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let temp = tempfile::tempdir().unwrap();
    let library = Library::open(temp.path()).unwrap();
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let raw = json!({"paper":{"meta":{},"blocks":[]},"reader":{}});
        let view = cx.new(|cx| ReaderView::new(raw, Some(library), None, window, cx));
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    for _ in 0..3 {
        draw(cx);
    }
    let bounds = cx.debug_bounds("home-brand-icon").unwrap();
    assert_eq!(
        bounds.size,
        gpui_kit::size(gpui_kit::px(28.), gpui_kit::px(28.))
    );
    let artwork = reader.read_with(cx, |view, _| view.home.brand_icon.clone());
    cx.update(|window, cx| {
        reader.update(cx, |view, cx| {
            view.home.prefs["dark"] = json!(true);
            view.home.apply_theme(Some(window), cx);
            cx.notify();
        });
    });
    for _ in 0..3 {
        draw(cx);
    }
    assert_eq!(cx.debug_bounds("home-brand-icon").unwrap(), bounds);
    reader.read_with(cx, |view, _| {
        assert!(std::sync::Arc::ptr_eq(&view.home.brand_icon, &artwork));
    });
}

#[gpui_kit::test]
fn reader_brand_icon_uses_shared_pelican_and_returns_to_library(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let temp = tempfile::tempdir().unwrap();
    let library = Library::open(temp.path()).unwrap();
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let raw = json!({"paper":{"meta":{"title_en":"Logo test"},"blocks":[]},"reader":{}});
        let view = cx.new(|cx| {
            let mut view = ReaderView::new(raw, Some(library), None, window, cx);
            view.home.visible = false;
            view
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    for _ in 0..3 {
        draw(cx);
    }
    let bounds = cx.debug_bounds("reader-brand-icon").unwrap();
    assert_eq!(
        bounds.size,
        gpui_kit::size(gpui_kit::px(28.), gpui_kit::px(28.))
    );
    let artwork = reader.read_with(cx, |view, _| view.home.brand_icon.clone());
    cx.simulate_click(bounds.center(), gpui_kit::Modifiers::default());
    for _ in 0..3 {
        draw(cx);
    }
    reader.read_with(cx, |view, _| {
        assert!(view.home.visible);
        assert!(view.navigation.is_none());
        assert!(std::sync::Arc::ptr_eq(&view.home.brand_icon, &artwork));
    });
    assert!(cx.debug_bounds("home-brand-icon").is_some());
    assert!(cx.debug_bounds("reader-brand-icon").is_none());
}
