//! About / Updates uses a separate background task, never the reader's AI or
//! import worker. Opening a browser download cannot stop or mutate the library.
use super::*;
use easyread_gpui::updates as releases;
use gpui_kit::component::switch::Switch;
use std::path::PathBuf;

pub(super) struct UpdateUi {
    pub cache: releases::Cache,
    pub checking: bool,
    pub error: String,
    pub path: Option<PathBuf>,
    #[cfg(test)]
    pub fetch: fn() -> anyhow::Result<Option<releases::Release>>,
}
impl UpdateUi {
    pub fn load() -> Self {
        // Native UI tests must neither contact GitHub nor touch user preferences.
        if cfg!(test) {
            return Self {
                cache: releases::Cache {
                    enabled: false,
                    ..Default::default()
                },
                checking: false,
                error: String::new(),
                path: None,
                #[cfg(test)]
                fetch: || Ok(None),
            };
        }
        let path = releases::state_path();
        let (cache, error) = match releases::Cache::load(&path) {
            Ok(cache) => (cache, String::new()),
            Err(error) => (
                releases::Cache {
                    enabled: false,
                    ..Default::default()
                },
                format!("Automatic checks paused: {error}. You can still check manually."),
            ),
        };
        Self {
            cache,
            checking: false,
            error,
            path: Some(path),
            #[cfg(test)]
            fetch: || Ok(None),
        }
    }
    fn persist(&mut self) {
        if let Some(path) = &self.path
            && let Err(error) = self.cache.save(path)
        {
            self.error = format!("Could not save update preferences: {error}");
        }
    }
}
impl ReaderView {
    pub(super) fn start_update_checks(&mut self, cx: &mut Context<Self>) {
        if cfg!(test) {
            return;
        }
        self.check_updates(false, cx);
        // Keep the daily policy working even if the app is left open for days.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(3600))
                    .await;
                if this
                    .update(cx, |this, cx| this.check_updates(false, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }
    pub(super) fn check_updates(&mut self, force: bool, cx: &mut Context<Self>) {
        if self.updates.checking
            || (!force && !self.updates.cache.due(chrono::Utc::now().timestamp()))
        {
            return;
        }
        self.updates.checking = true;
        self.updates.error.clear();
        #[cfg(not(test))]
        let fetch = releases::fetch_latest;
        #[cfg(test)]
        let fetch = self.updates.fetch;
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { fetch() }).await;
            let _ = this.update(cx, |this, cx| this.finish_update_check(result, cx));
        })
        .detach();
        cx.notify();
    }
    pub(super) fn finish_update_check(
        &mut self,
        result: anyhow::Result<Option<releases::Release>>,
        cx: &mut Context<Self>,
    ) {
        self.updates.checking = false;
        self.updates.error = result
            .as_ref()
            .err()
            .map(ToString::to_string)
            .unwrap_or_default();
        self.updates
            .cache
            .complete(&result, chrono::Utc::now().timestamp());
        self.updates.persist();
        cx.notify();
    }
    pub(super) fn open_about(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.home.popup = None;
        self.open_settings(window, cx);
        self.settings.show_about();
        // No forced request when simply opening Settings; respect the daily
        // cache and opt-out. The explicit button always bypasses both.
        self.check_updates(false, cx);
        cx.notify();
    }
    pub(super) fn about_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let state = &self.updates;
        let latest = state
            .cache
            .release
            .as_ref()
            .filter(|r| r.is_newer_than(releases::VERSION));
        let download =
            latest.and_then(|r| r.download(std::env::consts::OS, std::env::consts::ARCH));
        let download_url = download.map(|asset| asset.url.clone());
        let status = if state.checking {
            "Checking…".into()
        } else {
            state.cache.status(releases::VERSION)
        };
        div().debug_selector(|| "settings-about".into()).w_full().flex().flex_col().gap(px(18.))
            .child(div().flex().items_center().gap(px(16.))
                .child(img(self.home.brand_icon.clone()).size(px(48.)).object_fit(ObjectFit::Contain))
                .child(div().flex().flex_col().gap(px(5.))
                    .child(div().text_size(px(24.)).font_weight(FontWeight::SEMIBOLD).child("Vellio"))
                    .child(div().text_size(px(13.)).text_color(rgb(0x8c8579)).child("Runs locally. Your books and notes stay on your computer."))))
            .child(div().p(px(18.)).rounded(px(14.)).bg(rgb(0xf0ece3)).flex().flex_col().gap(px(14.))
                .child(div().flex().flex_wrap().items_center().justify_between().gap(px(12.))
                    .child(div().flex().flex_col().gap(px(6.))
                        .child(div().text_size(px(17.)).font_weight(FontWeight::SEMIBOLD).child(format!("Version {}", releases::VERSION)))
                        .child(div().debug_selector(|| "update-status".into()).text_size(px(13.)).text_color(rgb(0x8c8579)).child(status)))
                    .child(Button::new("check-updates").debug_selector(|| "check-updates".into()).ghost().small()
                        .label("Check for updates").disabled(state.checking)
                        .on_click(cx.listener(|this, _, _, cx| this.check_updates(true, cx)))))
                .child(div().debug_selector(|| "automatic-updates".into()).child(Switch::new("automatic-updates")
                    .checked(state.cache.enabled).color(rgb(0x306778))
                    .label("Check for new versions automatically (once a day)")
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        this.updates.cache.enabled = *checked;
                        this.updates.persist();
                        if *checked { this.check_updates(false, cx); }
                        cx.notify();
                    }))))
                .when(state.cache.checked_at > 0, |d| d.child(div().text_size(px(12.)).text_color(rgb(0x8c8579))
                    .child(format!("Last attempt: {}", chrono::DateTime::from_timestamp(state.cache.checked_at, 0)
                        .map(|at| at.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default())))))
            .when(!state.error.is_empty(), |d| d.child(div().debug_selector(|| "update-error".into()).text_size(px(13.)).text_color(rgb(0xaa681c)).child(state.error.clone())))
            .when_some(latest, |d, release| d.child(div().flex().flex_col().gap(px(12.))
                .child(div().text_size(px(16.)).font_weight(FontWeight::SEMIBOLD).child(format!("What's new in {}", release.version)))
                .child(TextView::markdown("update-release-notes", release.notes.clone()).selectable(true).text_size(px(13.)))
                .when_some(download_url, |d, url| d.child(Button::new("download-update").debug_selector(|| "download-update".into()).primary()
                    .label(format!("Download for {}", releases::platform_label(std::env::consts::OS, std::env::consts::ARCH)))
                    .on_click(move |_, _, cx| cx.open_url(&url))))
                .when(download.is_none(), |d| d.child(div().text_size(px(13.)).text_color(rgb(0x8c8579))
                    .child("No package for this CPU/OS in this release. Open Releases for available downloads.")))))
            .child(div().text_size(px(13.)).line_height(relative(1.6)).text_color(rgb(0x8c8579))
                .child("Downloads open in your browser. Quit Vellio before replacing the app. macOS: replace Vellio.app; Windows / Linux: extract the complete package to replace the program, not your library. Updates never delete your books or notes."))
            .child(div().flex().flex_wrap().gap(px(12.))
                .child(Button::new("open-releases").ghost().label("All releases / Download")
                    .on_click(|_, _, cx| cx.open_url(releases::RELEASES)))
                .child(Button::new("report-issue").ghost().label("Report on GitHub")
                    .on_click(|_, _, cx| cx.open_url(releases::ISSUES))))
            .into_any_element()
    }
}
