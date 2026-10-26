//! Home actions.
use super::*;

impl ReaderView {
    pub(in crate::desktop) fn save_home_prefs(&mut self) {
        if let Some(lib) = &self.library
            && let Err(e) = write_json(&lib.root.join(".library-home.json"), &self.home.prefs)
        {
            self.home.notice = format!("首页设置保存失败：{e}");
        }
    }

    pub(in crate::desktop) fn show_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.queue_margin_note();
        self.capture_inputs(cx);
        self.capture_position();
        self.flush_session();
        self.margin_editor = None;
        self.navigation = None;
        self.appearance_open = false;
        self.reading_order_open = false;
        self.selection = None;
        self.annotation_popup = None;
        self.home.visible = true;
        self.home.popup = None;
        self.home.batch = false;
        self.home.hovering = false;
        self.home.hovered_paper = None;
        self.home.selected.clear();
        self.home.apply_theme(Some(window), cx);
        match self.library.as_ref().map(Library::list) {
            Some(Ok(items)) => {
                self.items = items;
                self.home.load_covers(self.library.as_ref(), &self.items);
            }
            Some(Err(e)) => self.home.notice = format!("文献库读取失败：{e}"),
            None => {}
        }
        window.blur(cx);
        cx.notify();
    }

    pub(in crate::desktop) fn create_home_folder(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = self.home.folder_input.read(cx).value().trim().to_owned();
        if name.is_empty() {
            return;
        }
        if self.home.folders().iter().any(|(_, n)| n == &name) {
            self.home.notice = self
                .home
                .text("A folder with this name already exists", "已有同名文件夹")
                .into();
            cx.notify();
            return;
        }
        if !self.home.prefs["folders"].is_array() {
            self.home.prefs["folders"] = json!([]);
        }
        let folder_id = format!("folder-{}", Uuid::new_v4());
        self.home.prefs["folders"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":folder_id,"name":name}));
        self.save_home_prefs();
        self.home.adding_folder = false;
        if let Some(ids) = self.home.folder_papers.take() {
            self.add_papers_to_folder(&ids, &folder_id, cx);
        }
        self.home
            .folder_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    pub(in crate::desktop) fn open_home_rename(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || self.home.selected.len() != 1 {
            return;
        }
        let id = self.home.selected.iter().next().unwrap().clone();
        let Some(item) = self.items.iter().find(|item| item["id"] == id) else {
            return;
        };
        let name = catalog::title(item).to_owned();
        self.home.rename_id = Some(id);
        self.home.popup = Some("rename");
        self.home
            .rename_input
            .update(cx, |input, cx| input.set_value(name, window, cx));
        self.home
            .rename_input
            .read(cx)
            .focus_handle(cx)
            .focus(window, cx);
        cx.notify();
    }

    pub(in crate::desktop) fn rename_home_file(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = self.home.rename_input.read(cx).value().trim().to_owned();
        if self.busy || name.is_empty() {
            return;
        }
        let (Some(lib), Some(id)) = (self.library.clone(), self.home.rename_id.clone()) else {
            return;
        };
        self.home.popup = None;
        self.home.rename_id = None;
        self.home.notice.clear();
        window.blur(cx);
        self.worker(cx, move |_, tx| {
            match lib.workspace(&id).and_then(|ws| ws.rename(&name)) {
                Ok(_) => {
                    let _ = tx.send(Event::Catalog(lib.list()));
                }
                Err(e) => {
                    let _ = tx.send(Event::Error(format!("重命名失败：{e}")));
                }
            }
        });
        cx.notify();
    }

    pub(in crate::desktop) fn home_item_action(
        &mut self,
        ids: Vec<String>,
        field: &'static str,
        value: Value,
        cx: &mut Context<Self>,
    ) {
        let Some(lib) = self.library.clone() else {
            return;
        };
        self.home.notice.clear();
        self.worker(cx, move |cancel, tx| {
            for id in ids {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let result = lib.workspace(&id).and_then(|ws| {
                    if field == "status" {
                        ws.set_reading_status(value.as_str().unwrap_or(""))
                    } else {
                        ws.update("item", |item| {
                            item[field] = value.clone();
                            Ok(())
                        })
                    }
                });
                if let Err(e) = result {
                    let _ = tx.send(Event::Error(format!("文献更新失败：{e}")));
                    break;
                }
            }
            let _ = tx.send(Event::Catalog(lib.list()));
        });
    }

    pub(in crate::desktop) fn assign_home_folder(&mut self, folder: &str, cx: &mut Context<Self>) {
        let ids = self.home.selected.iter().cloned().collect::<Vec<_>>();
        self.add_papers_to_folder(&ids, folder, cx);
    }

    pub(in crate::desktop) fn add_papers_to_folder(
        &mut self,
        ids: &[String],
        folder: &str,
        cx: &mut Context<Self>,
    ) {
        if !self.home.prefs["assignments"].is_object() {
            self.home.prefs["assignments"] = json!({});
        }
        for id in ids {
            let mut assigned = self.home.prefs["assignments"][id]
                .as_array()
                .cloned()
                .unwrap_or_default();
            if !assigned.iter().any(|f| f == folder) {
                assigned.push(json!(folder));
            }
            self.home.prefs["assignments"][id] = json!(assigned);
        }
        self.save_home_prefs();
        self.home.popup = None;
        cx.notify();
    }

    pub(in crate::desktop) fn remove_selected_from_folder(&mut self, cx: &mut Context<Self>) {
        for id in &self.home.selected {
            if let Some(assigned) = self.home.prefs["assignments"][id].as_array_mut() {
                assigned.retain(|folder| folder != &self.home.filter);
            }
        }
        self.save_home_prefs();
        self.home.selected.clear();
        cx.notify();
    }

    pub(in crate::desktop) fn set_home_trash(&mut self, removed: bool, cx: &mut Context<Self>) {
        if self.busy || self.home.selected.is_empty() {
            return;
        }
        let previous = self.home.prefs.clone();
        if !self.home.prefs["removed"].is_object() {
            self.home.prefs["removed"] = json!({});
        }
        for id in &self.home.selected {
            self.home.prefs["removed"][id] = json!(removed);
        }
        if let Some(lib) = &self.library
            && let Err(e) = write_json(&lib.root.join(".library-home.json"), &self.home.prefs)
        {
            self.home.prefs = previous;
            self.home.notice = format!("首页设置保存失败：{e}");
            cx.notify();
            return;
        }
        self.home.selected.clear();
        self.home.batch = false;
        self.home.popup = None;
        cx.notify();
    }

    pub(in crate::desktop) fn translate_home_selection(&mut self, cx: &mut Context<Self>) {
        if self.busy
            || self.home.selected.is_empty()
            || self
                .home
                .translation_pages
                .as_ref()
                .is_some_and(|pages| pages.is_empty() || self.home.selected.len() != 1)
        {
            return;
        }
        let Some(lib) = self.library.clone() else {
            return;
        };
        let config = match self.settings.committed.connection(true) {
            Ok(config) => config,
            Err(e) => {
                self.home.notice = e.to_string();
                cx.notify();
                return;
            }
        };
        let options = ai::TranslationOptions {
            pages_per_batch: self.settings.committed.pages_per_batch,
            parallel: self.settings.committed.parallel,
        };
        let ids = self
            .items
            .iter()
            .filter_map(|item| item["id"].as_str())
            .filter(|id| self.home.selected.contains(*id))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        self.home.popup = None;
        self.home.notice = self
            .home
            .text("Translating selected papers…", "正在翻译所选文献……")
            .into();
        let selected_pages = self
            .home
            .translation_pages
            .take()
            .map(|pages| pages.into_iter().collect::<Vec<_>>());
        let active_root = self.workspace.as_ref().map(|ws| ws.root.clone());
        self.worker(cx, move |cancel, tx| {
            let mut failures = 0;
            for id in ids {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let outcome = lib.workspace(&id).and_then(|ws| {
                    // A trashed paper must not consume model quota or stop the
                    // rest of the library queue (upstream v1.4.1 / #56).
                    let home = read_json(&lib.root.join(".library-home.json"), json!({}))?;
                    if home["removed"][&id].as_bool().unwrap_or(false)
                        || ws.load("item")?["removed"].as_bool().unwrap_or(false)
                    {
                        return Ok(());
                    }
                    let progress = |done, total| {
                        let _ = tx.send(Event::Status(format!("翻译 {done}/{total}")));
                    };
                    let result = if let Some(pages) = &selected_pages {
                        ai::translate_pages_batched(&ws, &config, options, pages, &cancel, progress)
                    } else {
                        ai::translate_batched(&ws, &config, options, &cancel, progress)
                    };
                    if active_root.as_ref() == Some(&ws.root)
                        && let Ok(raw) = ws.state()
                    {
                        let _ = tx.send(Event::Refresh(raw));
                    }
                    result
                });
                if let Err(e) = outcome {
                    failures += 1;
                    let _ = tx.send(Event::Error(format!("文献 {id} 翻译未完成：{e:#}")));
                }
            }
            let _ = tx.send(Event::Catalog(lib.list()));
            let _ = tx.send(if cancel.load(Ordering::Relaxed) {
                Event::Status("翻译已取消；已完成内容保留".into())
            } else if failures > 0 {
                Event::Error(format!(
                    "{failures} 篇文献翻译未完成；其余内容已保存，可重试未译页"
                ))
            } else {
                Event::Status("翻译已保存".into())
            });
        });
        self.home.cancellable_task = true;
        cx.notify();
    }
}
