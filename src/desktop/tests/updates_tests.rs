//! Real native About surface, with an explicit in-process release fixture.
//! No network, browser launch or user configuration writes.
use super::ReaderView;
use easyread_gpui::updates::{self, Asset, Release};
use gpui_kit::{AppContext, Modifiers, TestAppContext, VisualTestContext, component::Root};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..8 {
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.draw(cx).clear(cx);
        });
    }
}
fn fixture() -> Release {
    let name = format!(
        "Vellio-99.0.0-{}-{}.zip",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    Release {
        version: "99.0.0".into(),
        url: format!("{}/tag/v99.0.0", updates::RELEASES),
        notes: "**New version**\n\nYour books and notes stay local.".into(),
        published: String::new(),
        assets: vec![Asset {
            url: format!("{}/download/v99.0.0/{name}", updates::RELEASES),
            name,
        }],
    }
}

#[gpui_kit::test]
fn home_help_opens_about_and_renders_update_and_error_states(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut reader = ReaderView::new(
                json!({"paper":{"meta":{},"blocks":[]},"reader":{}}),
                None,
                None,
                window,
                cx,
            );
            reader.home.visible = true;
            reader.home.popup = Some("help");
            reader
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    settle(cx);
    let shortcut = cx
        .debug_bounds("home-about-updates")
        .expect("help menu has update shortcut");
    cx.simulate_click(shortcut.center(), Modifiers::default());
    settle(cx);
    assert!(cx.debug_bounds("settings-about").is_some());
    assert!(cx.debug_bounds("check-updates").is_some());
    assert!(cx.debug_bounds("automatic-updates").is_some());
    assert!(cx.debug_bounds("download-update").is_none());
    let raw = reader.read_with(cx, |r, _| r.raw.clone());
    reader.update(cx, |r, cx| r.finish_update_check(Ok(Some(fixture())), cx));
    settle(cx);
    if std::env::consts::OS != "linux" {
        assert!(cx.debug_bounds("download-update").is_some());
    }
    assert!(cx.debug_bounds("update-error").is_none());
    reader.update(cx, |r, cx| {
        r.finish_update_check(Err(anyhow::anyhow!("GitHub rate limit reached")), cx)
    });
    settle(cx);
    assert!(cx.debug_bounds("update-error").is_some());
    reader.read_with(cx, |r, _| {
        assert!(r.updates.cache.failed);
        assert!(
            !r.updates
                .cache
                .status(updates::VERSION)
                .contains("up to date")
        );
        assert_eq!(r.raw, raw);
        assert!(!r.busy, "updates must not reuse the AI/import worker");
    });
}

#[gpui_kit::test]
fn manual_update_check_bypasses_opt_out_and_deduplicates_requests(cx: &mut TestAppContext) {
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    fn mock() -> anyhow::Result<Option<Release>> {
        CALLS.fetch_add(1, Ordering::Relaxed);
        Ok(Some(fixture()))
    }
    cx.update(gpui_kit::init);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("updates.json");
    let mut reader = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            ReaderView::new(
                json!({"paper":{"meta":{},"blocks":[]},"reader":{}}),
                None,
                None,
                window,
                cx,
            )
        });
        reader = Some(view.clone());
        Root::new(view, window, cx)
    });
    let reader = reader.unwrap();
    reader.update(cx, |r, cx| {
        r.updates.path = Some(path.clone());
        r.updates.fetch = mock;
        r.check_updates(false, cx);
        assert!(!r.updates.checking);
        r.check_updates(true, cx);
        assert!(r.updates.checking);
        r.check_updates(true, cx);
    });
    for _ in 0..100 {
        std::thread::sleep(std::time::Duration::from_millis(5));
        settle(cx);
        if !reader.read_with(cx, |r, _| r.updates.checking) {
            break;
        }
    }
    assert_eq!(CALLS.load(Ordering::Relaxed), 1);
    let saved = updates::Cache::load(&path).unwrap();
    assert!(
        !saved.enabled,
        "a forced check must not re-enable automatic checks"
    );
    assert_eq!(saved.release.unwrap().version, "99.0.0");
}
