//! Settings library.
use super::*;

impl ReaderView {
    pub(in crate::desktop) fn cloud_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let path = self.library.as_ref().map(|l| l.root.clone());
        let drive = path
            .as_ref()
            .and_then(|p| self.settings.drives.iter().find(|(_, d)| p.starts_with(d)))
            .map(|(n, _)| n.clone());
        let mut body=div().flex().flex_col().child(div().text_color(rgb(0x625e55)).line_height(relative(1.8)).child("A cloud sync folder lives on your computer, and the cloud client uploads what is inside it. Move your library there and your papers, translations and notes follow you to any computer signed in to the same drive."))
            .child(section("T h i s   c o m p u t e r"))
            .child(div().p(px(22.)).rounded(px(16.)).border_1().border_color(rgb(BORDER)).flex().items_center().gap(px(18.))
                .child(div().size(px(54.)).rounded(px(14.)).bg(rgb(0xecf0ef)).flex().items_center().justify_center().child(Icon::default().path("icons/folder.svg").size(px(25.)).text_color(rgb(ACCENT))))
                .child(div().flex_1().min_w_0().flex().flex_col().gap(px(10.)).child(div().text_size(px(14.)).font_weight(FontWeight::MEDIUM).child(drive.map(|d|format!("In {d} sync folder")).unwrap_or("On this computer, not synced".into()))).child(muted(path.as_ref().map(|p|p.display().to_string()).unwrap_or("Snapshot mode".into()))))
                .child(outline("open-library-folder","Open folder").disabled(path.is_none()).on_click(cx.listener(|this,_,_,cx|{if let Some(l)=&this.library{cx.reveal_path(&l.root)}})))
                .child(primary("move-library","Move…").disabled(self.busy||path.is_none()).on_click(cx.listener(|this,_,window,cx|this.choose_library_folder(window,cx)))))
            .child(div().mt(px(24.)).flex().items_center().child(section("C l o u d   d r i v e s").mt_0().flex_1()).child(Button::new("check-drives").ghost().label("Check again").text_color(rgb(ACCENT)).on_click(cx.listener(|this,_,_,cx|{this.settings.drives=preferences::cloud_drives();cx.notify();}))));
        for (i, (name, drive)) in self.settings.drives.iter().enumerate() {
            body = body.child(
                div()
                    .mb(px(12.))
                    .p(px(20.))
                    .rounded(px(14.))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .flex()
                    .items_center()
                    .gap(px(16.))
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(name.clone()))
                            .child(muted(drive.display().to_string())),
                    )
                    .child(
                        outline(("cloud-drive", i), "Use folder…")
                            .disabled(self.busy)
                            .on_click(cx.listener({
                                let drive = drive.clone();
                                move |this, _, _, cx| {
                                    this.capture_inputs(cx);
                                    this.capture_position();
                                    this.flush_session();
                                    this.queue_margin_note();
                                    this.relocate_library(drive.clone(), cx)
                                }
                            })),
                    ),
            );
        }
        body.child(div().p(px(22.)).rounded(px(16.)).border_1().border_color(rgb(BORDER)).flex().items_center().gap(px(20.))
            .child(div().flex_1().flex().flex_col().gap(px(12.)).child(div().text_size(px(14.)).font_weight(FontWeight::MEDIUM).child("Other cloud drives")).child(muted(if self.settings.drives.is_empty(){"No OneDrive, Dropbox or iCloud found. Nutstore, Google Drive and others: choose their sync folder"}else{"Nutstore, Google Drive and others: choose their sync folder"})))
            .child(outline("choose-cloud-folder","Choose folder…").disabled(self.busy||path.is_none()).on_click(cx.listener(|this,_,window,cx|this.choose_library_folder(window,cx)))))
            .child(section("G o o d   t o   k n o w"))
            .children(["Moving copies the library; the original folder is kept.","Avoid opening Vellio on two computers at once: changes to the same paper may overwrite each other.","Space-saving modes in iCloud and Google Drive may leave files online only. Set the Vellio folder to always stay on this device.","Baidu Netdisk and Xunlei do not provide the live synced folders needed for a library."].map(|s|div().mb(px(10.)).flex().gap(px(10.)).child(muted("•")).child(muted(s).flex_1().line_height(relative(1.8)))))
            .into_any_element()
    }

    pub(in crate::desktop) fn choose_library_folder(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        self.capture_inputs(cx);
        self.capture_position();
        self.flush_session();
        self.queue_margin_note();
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(
                "Choose a sync folder — creates Vellio/library; keeps the current library".into(),
            ),
        });
        cx.spawn_in(window, async move |this, window| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(folder) = paths.first()
            {
                let folder = folder.clone();
                let _ = this.update_in(window, |this, _, cx| this.relocate_library(folder, cx));
            }
        })
        .detach();
    }

    pub(in crate::desktop) fn relocate_library(&mut self, folder: PathBuf, cx: &mut Context<Self>) {
        let Some(library) = self.library.clone() else {
            return;
        };
        if self.busy {
            return;
        }
        let destination = folder.join("Vellio/library");
        let id = self.workspace.as_ref().map(|w| w.id().to_owned());
        self.settings.relocating = true;
        self.settings.notice = "Copying library…".into();
        let writer = std::mem::take(&mut self.margin_writer);
        self.worker(cx, move |cancel, tx| {
            drop(writer);
            let result = (|| -> anyhow::Result<_> {
                let parent = destination.parent().unwrap();
                std::fs::create_dir_all(parent)?;
                preferences::copy_library(&library.root, &destination, &cancel)?;
                let library = Library::open(destination.clone())?;
                let workspace = id.map(|id| library.workspace(&id)).transpose()?;
                let raw = workspace.as_ref().map(Workspace::state).transpose()?;
                preferences::remember_library(&library.root)?;
                Ok((library, workspace, raw))
            })();
            match result {
                Ok((library, workspace, raw)) => {
                    let _ = tx.send(Event::Relocated(library, workspace, raw));
                }
                Err(e) => {
                    let _ = tx.send(Event::SettingsResult(format!(
                        "Move failed; original library retained: {e}"
                    )));
                }
            }
        });
    }
}
