//! Library loading, import and translation commands.
use super::*;

impl ReaderView {
    pub(super) fn pick(&mut self, migrate: bool, cx: &mut Context<Self>) {
        let Some(lib) = self.library.clone() else {
            return;
        };
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: !migrate,
            directories: migrate,
            multiple: false,
            prompt: Some(
                if migrate {
                    "复制旧论文目录"
                } else {
                    "导入 PDF / EPUB"
                }
                .into(),
            ),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(path) = paths.first()
            {
                let path = path.clone();
                let _ = this.update(cx, |view, cx| {
                    view.status = "正在导入……".into();
                    view.worker(cx, move |cancel, tx| {
                        let result = if migrate {
                            lib.migrate_copy(&path)
                        } else {
                            lib.import_file_cancellable(&path, None, &cancel, |done, total| {
                                let _ = tx.send(Event::Status(format!("导入 {done}/{total}")));
                            })
                        };
                        match result.and_then(Self::opened_workspace) {
                            Ok((ws, raw)) => {
                                let _ = tx.send(Event::Loaded(ws, raw));
                            }
                            Err(e) => {
                                let _ = tx.send(Event::Error(format!("导入失败：{e:#}")));
                            }
                        }
                    });
                    view.home.cancellable_task = !migrate;
                });
            }
        })
        .detach();
    }

    pub(super) fn open(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(lib) = self.library.clone() else {
            return;
        };
        self.worker(cx, move |_, tx| {
            match lib.workspace(&id).and_then(Self::opened_workspace) {
                Ok((ws, raw)) => {
                    let _ = tx.send(Event::Loaded(ws, raw));
                }
                Err(e) => {
                    let _ = tx.send(Event::Error(format!("打开失败：{e:#}")));
                }
            }
        });
    }

    pub(super) fn opened_workspace(ws: Workspace) -> anyhow::Result<(Workspace, Value)> {
        let mut raw = ws.validated_state()?;
        raw["item"] = ws.update("item", |item| {
            item["opened"] = json!(now());
            if item["status"] != "done" {
                item["status"] = json!("reading");
            }
            Ok(())
        })?;
        Ok((ws, raw))
    }

    pub(super) fn translate(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.workspace.is_none() {
            return;
        }
        let pages = self.state.paper.meta.pages.len().max(
            self.state
                .paper
                .blocks
                .iter()
                .map(|b| b.page)
                .max()
                .unwrap_or(0),
        );
        if pages > self.settings.committed.ask_above || self.settings.committed.ask_above == 0 {
            self.settings.translation_confirm = true;
            cx.notify();
            return;
        }
        self.start_translate(cx);
    }

    pub(super) fn start_translate(&mut self, cx: &mut Context<Self>) {
        let Some(ws) = self.workspace.clone() else {
            return;
        };
        let config = match self.settings.committed.connection(true) {
            Ok(c) => c,
            Err(e) => {
                self.status = e.to_string();
                return;
            }
        };
        let options = ai::TranslationOptions {
            pages_per_batch: self.settings.committed.pages_per_batch,
            parallel: self.settings.committed.parallel,
        };
        self.status = "正在翻译……".into();
        self.worker(cx, move |cancel, tx| {
            let outcome = ai::translate_batched(&ws, &config, options, &cancel, |done, total| {
                let _ = tx.send(Event::Status(format!("翻译 {done}/{total}")));
            });
            if let Ok(raw) = ws.state() {
                let _ = tx.send(Event::Refresh(raw));
            }
            let _ = tx.send(match outcome {
                Ok(_) => Event::Status("翻译已保存".into()),
                Err(e) => Event::Error(format!("翻译中断：{e:#}")),
            });
        });
        self.home.cancellable_task = true;
    }

    pub(super) fn complete_images(&mut self, cx: &mut Context<Self>) {
        let Some(ws) = self.workspace.clone() else {
            return;
        };
        self.status = "正在补全 PDF 图片……".into();
        self.worker(cx, move |cancel, tx| {
            let result = ws.complete_pdf_images(&cancel, |done, total| {
                let _ = tx.send(Event::Status(format!("补全图片 {done}/{total}")));
            });
            match result.and_then(|count| Ok((count, ws.state()?))) {
                Ok((count, raw)) => {
                    let _ = tx.send(Event::Refresh(raw));
                    let _ = tx.send(Event::Status(format!(
                        "已补全 {count} 个图片区域，修订与笔记已保留"
                    )));
                }
                Err(e) => {
                    let _ = tx.send(Event::Error(format!("图片补全失败：{e:#}")));
                }
            }
        });
        self.home.cancellable_task = true;
    }
}
