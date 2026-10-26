//! Home HTTP handlers.
use super::*;

#[page("/")]
async fn home() -> Result<impl View> {
    let books = tokio::task::spawn_blocking(|| library().list())
        .await?
        .map_err(Error::from_anyhow)?;
    let prefs =
        easyread_gpui::store::read_json(&library().root.join(".library-home.json"), json!({}))
            .unwrap_or_else(|_| json!({}));
    let folders: Vec<HomeFolder> = prefs["folders"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            Some(HomeFolder {
                id: item["id"].as_str()?.to_owned(),
                name: item["name"].as_str()?.to_owned(),
            })
        })
        .filter(|folder| {
            !prefs["hidden_folders"][&folder.id]
                .as_bool()
                .unwrap_or(false)
        })
        .collect();
    let mut books = books;
    books.sort_by(|a, b| library_home::compare_for_sort(a, b, 0));
    let rows: Vec<HomeRow> = books
        .iter()
        .filter_map(|b| {
            let id = b["id"].as_str()?.to_owned();
            let authors = b["authors"].as_str().unwrap_or("");
            let year = library_home::published_year(b).unwrap_or(0);
            let venue = b["meta"]["venue"]
                .as_str()
                .or_else(|| b["meta"]["publication"].as_str())
                .unwrap_or("");
            let details = [
                authors.to_owned(),
                if year > 0 {
                    year.to_string()
                } else {
                    String::new()
                },
                venue.to_owned(),
            ]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
            Some(HomeRow {
                title: library_home::title(b).to_owned(),
                status: b["status"].as_str().unwrap_or("unread").into(),
                starred: b["item"]["starred"].as_bool().unwrap_or(false),
                removed: prefs["removed"][&id].as_bool().unwrap_or(false),
                notes: b["notes"].as_u64().unwrap_or(0),
                questions: b["open_questions"].as_u64().unwrap_or(0),
                added: b["item"]["added"].as_str().unwrap_or("").into(),
                ratio: (b["ratio"].as_f64().unwrap_or(0.) * 100.).round() as u64,
                opened: b["opened"].as_str().unwrap_or("").into(),
                cover: b["cover"]
                    .as_str()
                    .filter(|s| {
                        (s.starts_with("pages/") || s.starts_with("images/"))
                            && library()
                                .workspace(&id)
                                .ok()
                                .and_then(|book| book.asset(s).ok())
                                .is_some()
                    })
                    .unwrap_or("")
                    .into(),
                year,
                details,
                folders: prefs["assignments"][&id]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" "),
                citation: library_home::citation(b),
                folder_names: prefs["assignments"][&id]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|folder_id| {
                        prefs["folders"]
                            .as_array()?
                            .iter()
                            .find(|f| f["id"] == *folder_id)?["name"]
                            .as_str()
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
                    .to_lowercase(),
                id,
            })
        })
        .collect();
    Ok(view! {
        <!DOCTYPE html>
        <html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>"Vellio · 书库"</title><link rel="stylesheet" href="/reader.css"><script src="/home.js" defer="defer"></script></head>
        <body class="library-home"><div class="home-layout">
            <aside class="home-sidebar"><div class="home-brand">"▤ "<strong>"Vellio"</strong></div><p class="eyebrow">"F O L D E R S"</p>
                <nav aria-label="书库筛选"><button class="active" data-filter="all">"▤　全部书籍"</button><button data-filter="reading">"◐　在读"</button><button data-filter="unread">"○　未读"</button><button data-filter="done">"✓　已读"</button><button data-filter="starred">"☆　收藏"</button><button data-filter="questions">"?　待解答问题"</button><button data-filter="trash">"⌑　回收站"</button></nav>
                <p class="eyebrow folder-heading">"文件夹"</p><nav class="folder-nav" aria-label="自定义文件夹">
                for folder in folders.clone() { <div class="folder-entry"><button type="button" data-filter=(folder.id.clone()) data-folder-drop=(folder.id.clone())>(folder.name.clone())</button>
                    <details class="folder-more"><summary aria-label=(format!("管理文件夹 {}",folder.name))>"···"</summary><div>
                        <form method="post" action="/library/folders"><input type="hidden" name="action" value="rename"><input type="hidden" name="folder" value=(folder.id.clone())><input name="name" value=(folder.name) maxlength="80" required=(true) aria-label="文件夹新名称"><button>"重命名"</button></form>
                        <form method="post" action="/library/folders"><input type="hidden" name="action" value="delete"><input type="hidden" name="folder" value=(folder.id)><button>"删除文件夹"</button></form>
                    </div></details>
                </div> }
                </nav><details class="add-folder"><summary>"＋ 新建文件夹"</summary><form method="post" action="/library/folders"><input type="hidden" name="action" value="create"><input name="name" maxlength="80" required=(true) placeholder="文件夹名称" aria-label="文件夹名称"><button>"创建"</button></form></details>
                <p class="sidebar-hint">"支持 PDF / EPUB 书籍与论文；可拖动或批量归档。"</p>
            </aside>
            <main class="home-main"><header class="home-top"><h1>"全部书籍"</h1><span id="home-count"></span><span class="spacer"></span>
                <a class="home-model-link" href="/settings?tab=about" aria-label="关于与软件更新">"? 帮助与更新"</a><a class="home-model-link" href="/settings" aria-label="打开模型设置">"⚙ 模型设置"</a>
                <input id="library-search" type="search" placeholder="搜索标题、作者…" aria-label="搜索书籍与论文">
                <label>"排序 "<select id="library-sort"><option value="recent">"最近阅读"</option><option value="added">"最近添加"</option><option value="year">"发表年份"</option><option value="title">"标题"</option></select></label>
                <form class="upload-compact" method="post" action="/import" enctype="multipart/form-data"><input type="file" name="book" accept=".pdf,.epub,application/pdf,application/epub+zip" required=(true) hidden=(true)><button type="button" data-import-book="true" title="导入 PDF / EPUB">"导入"</button></form>
            </header><p class="home-warning">"本机实验书库；不要公开服务或与桌面程序同时修改同一本书。"</p>
            <form id="batch-form" class="batch-bar" method="post" action="/library/batch"><input type="hidden" name="ids" value="">
                <label class="batch-master"><input type="checkbox" id="batch-select-all" aria-label="全选或取消当前书目"></label><strong id="batch-count">"0 项已选"</strong><span class="spacer"></span>
                if !folders.is_empty() { <select name="folder" aria-label="批量选择文件夹"><option value="">"文件夹"</option>for folder in folders.clone() { <option value=(folder.id)>(folder.name)</option> }</select><button type="submit" name="action" value="assign">"加入收藏夹"</button><button type="submit" name="action" value="remove-folder">"移出文件夹"</button> }
                <select name="status" aria-label="批量阅读状态"><option value="reading">"在读"</option><option value="unread">"未读"</option><option value="done">"已读"</option></select><button type="submit" name="action" value="status">"设置状态"</button>
                <button type="button" data-batch-cite="true">"复制引文"</button>
                <details class="batch-confirm"><summary>"移除所选"</summary><button type="submit" name="action" value="trash">"确认移入回收站"</button></details>
                <button type="submit" name="action" value="restore" class="batch-restore" hidden=(true)>"恢复所选"</button>
            </form>
            <div id="home-list" class="home-list">
            for book in rows { <article class="home-row" draggable="true" data-book=(book.id.clone()) data-folders=(book.folders) data-folder-names=(book.folder_names) data-citation=(book.citation) data-title=(book.title.to_lowercase()) data-details=(book.details.to_lowercase()) data-status=(book.status.clone()) data-starred=(book.starred.to_string()) data-removed=(book.removed.to_string()) data-questions=(book.questions.to_string()) data-opened=(book.opened) data-added=(book.added) data-year=(book.year.to_string())>
                <input class="row-select" type="checkbox" value=(book.id.clone()) aria-label=(format!("选择 {}", book.title))>
                <a class="book-cover" href=(format!("/read/{}", book.id)) aria-label=(format!("打开 {}", book.title))>if !book.cover.is_empty() { <img src=(format!("/read/{}/{}", book.id, book.cover)) alt=""> } else { <span>"▤"</span> }</a>
                <div class="home-row-content"><div class="home-row-head"><a class="row-title" href=(format!("/read/{}", book.id))>(book.title.clone())</a><span class="status">(if book.status == "done" { "已读" } else if book.status == "reading" { "在读" } else { "未读" })</span></div>
                    <div class="row-details">(book.details)</div><div class="row-bottom"><span>(format!("进度 {}%",book.ratio))</span><span>(format!("{} 条批注",book.notes))</span></div><div class="row-progress"><span style=(format!("width:{}%",book.ratio.min(100)))></span></div>
                    <div class="row-actions"><form method="post" action=(format!("/library/{}/action",book.id))><input type="hidden" name="action" value="star"><button title="收藏">(if book.starred { "★ 已收藏" } else { "☆ 收藏" })</button></form>
                        <form method="post" action=(format!("/library/{}/action",book.id))><input type="hidden" name="action" value="status"><select name="value" aria-label="阅读状态"><option value="unread" selected=(book.status=="unread")>"未读"</option><option value="reading" selected=(book.status=="reading")>"在读"</option><option value="done" selected=(book.status=="done")>"已读"</option></select><button>"更新"</button></form>
                        <form method="post" action=(format!("/library/{}/action",book.id))><input type="hidden" name="action" value="rename"><input name="value" aria-label="书籍名称" value=(book.title) maxlength="160" required=(true)><button>"改名"</button></form>
                        <form method="post" action=(format!("/library/{}/action",book.id))><input type="hidden" name="action" value="trash"><button>(if book.removed { "移回书库" } else { "移入回收站" })</button></form>
                        if !folders.is_empty() { <form method="post" action="/library/folders" class="row-folder-action"><input type="hidden" name="book" value=(book.id)><select name="folder" required=(true) aria-label="选择文件夹"><option value="" selected=(true) disabled=(true)>"文件夹"</option>for folder in folders.clone() { <option value=(folder.id)>(folder.name)</option> }</select><button type="submit" name="action" value="assign">"加入"</button><button type="submit" name="action" value="remove">"移出"</button></form> }
                    </div>
                </div>
            </article> }
            </div><p id="home-empty" hidden=(true)>"当前筛选没有书籍。"</p>
            </main></div></body></html>
    })
}

#[route(POST "/library/batch")]
async fn library_batch(Form(input): Form<BatchAction>) -> Result<SeeOther> {
    let ids: Vec<_> = input.ids.split(',').filter(|id| !id.is_empty()).collect();
    if ids.is_empty() || ids.len() > 200 {
        return Err(bad_request("请选择 1 至 200 项书目").into());
    }
    let books = ids
        .iter()
        .map(|id| library().workspace(id))
        .collect::<anyhow::Result<Vec<_>>>()
        .map_err(|_| not_found())?;
    match input.action.as_str() {
        "status" => {
            let status = input.status.as_deref().unwrap_or("");
            if !matches!(status, "unread" | "reading" | "done") {
                return Err(bad_request("状态无效").into());
            }
            for book in books {
                book.set_reading_status(status)
                    .map_err(Error::from_anyhow)?;
            }
            Ok(see_other("/"))
        }
        "assign" | "remove-folder" | "trash" | "restore" => {
            let _lock = HOME_PREFS_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let path = library().root.join(".library-home.json");
            let mut prefs =
                easyread_gpui::store::read_json(&path, json!({})).map_err(Error::from_anyhow)?;
            if !prefs.is_object() {
                return Err(bad_request("书库数据无效").into());
            }
            if matches!(input.action.as_str(), "assign" | "remove-folder") {
                let folder = input.folder.as_deref().unwrap_or("");
                if !prefs["folders"]
                    .as_array()
                    .is_some_and(|folders| folders.iter().any(|f| f["id"] == folder))
                {
                    return Err(bad_request("文件夹不存在").into());
                }
                if !prefs["assignments"].is_object() {
                    prefs["assignments"] = json!({});
                }
                for id in ids {
                    let mut current = prefs["assignments"][id]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    if input.action == "assign" {
                        if !current.iter().any(|f| f == folder) {
                            current.push(json!(folder));
                        }
                    } else {
                        current.retain(|f| f != folder);
                    }
                    prefs["assignments"][id] = json!(current);
                }
                easyread_gpui::store::write_json(&path, &prefs).map_err(Error::from_anyhow)?;
                Ok(see_other(format!("/?filter={folder}")))
            } else {
                if !prefs["removed"].is_object() {
                    prefs["removed"] = json!({});
                }
                for id in ids {
                    prefs["removed"][id] = json!(input.action == "trash");
                }
                easyread_gpui::store::write_json(&path, &prefs).map_err(Error::from_anyhow)?;
                Ok(see_other(if input.action == "restore" {
                    "/?filter=trash"
                } else {
                    "/"
                }))
            }
        }
        _ => Err(bad_request("批量操作无效").into()),
    }
}

#[route(POST "/library/folders")]
async fn folder_action(Form(input): Form<FolderAction>) -> Result<SeeOther> {
    let _lock = HOME_PREFS_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let path = library().root.join(".library-home.json");
    let mut prefs =
        easyread_gpui::store::read_json(&path, json!({})).map_err(Error::from_anyhow)?;
    if !prefs.is_object() {
        return Err(bad_request("文件夹数据无效").into());
    }
    if !prefs["folders"].is_array() {
        prefs["folders"] = json!([]);
    }
    if !prefs["assignments"].is_object() {
        prefs["assignments"] = json!({});
    }
    let name = input.name.as_deref().unwrap_or("").trim();
    let folder = input.folder.as_deref().unwrap_or("");
    let has_folder = prefs["folders"]
        .as_array()
        .is_some_and(|folders| folders.iter().any(|f| f["id"] == folder));
    let redirect_folder = match input.action.as_str() {
        "create" => {
            if name.is_empty()
                || name.len() > 80
                || name.chars().any(char::is_control)
                || prefs["folders"]
                    .as_array()
                    .is_some_and(|folders| folders.iter().any(|f| f["name"] == name))
            {
                return Err(bad_request("文件夹名称无效或已存在").into());
            }
            let id = format!("folder-{}", uuid::Uuid::new_v4());
            prefs["folders"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id":id,"name":name}));
            Some(id)
        }
        "rename" => {
            if !has_folder
                || name.is_empty()
                || name.len() > 80
                || name.chars().any(char::is_control)
                || prefs["folders"].as_array().is_some_and(|folders| {
                    folders
                        .iter()
                        .any(|f| f["name"] == name && f["id"] != folder)
                })
            {
                return Err(bad_request("文件夹名称无效或已存在").into());
            }
            for item in prefs["folders"].as_array_mut().unwrap() {
                if item["id"] == folder {
                    item["name"] = json!(name);
                }
            }
            Some(folder.to_owned())
        }
        "delete" => {
            if !has_folder {
                return Err(bad_request("文件夹不存在").into());
            }
            prefs["folders"]
                .as_array_mut()
                .unwrap()
                .retain(|f| f["id"] != folder);
            if let Some(assigned) = prefs["assignments"].as_object_mut() {
                for ids in assigned.values_mut() {
                    if let Some(ids) = ids.as_array_mut() {
                        ids.retain(|id| id != folder);
                    }
                }
            }
            if let Some(pinned) = prefs["pinned_folders"].as_object_mut() {
                pinned.remove(folder);
            }
            if let Some(hidden) = prefs["hidden_folders"].as_object_mut() {
                hidden.remove(folder);
            }
            None
        }
        "assign" | "remove" => {
            if !has_folder {
                return Err(bad_request("文件夹不存在").into());
            }
            let id = input.book.as_deref().unwrap_or("");
            library().workspace(id).map_err(|_| not_found())?;
            let current = prefs["assignments"][id]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut assigned = current;
            if input.action == "assign" {
                if !assigned.iter().any(|f| f == folder) {
                    assigned.push(json!(folder));
                }
            } else {
                assigned.retain(|f| f != folder);
            }
            prefs["assignments"][id] = json!(assigned);
            Some(folder.to_owned())
        }
        _ => return Err(bad_request("文件夹操作无效").into()),
    };
    easyread_gpui::store::write_json(&path, &prefs).map_err(Error::from_anyhow)?;
    Ok(see_other(redirect_folder.map_or_else(
        || "/".to_owned(),
        |id| format!("/?filter={id}"),
    )))
}

#[route(POST "/library/{book_id}/action")]
async fn library_action(cx: &Cx, Form(input): Form<LibraryAction>) -> Result<SeeOther> {
    let id = path_param::<BookId>(cx).to_string();
    let book = library().workspace(&id).map_err(|_| not_found())?;
    match input.action.as_str() {
        "star" => {
            book.update("item", |item| {
                item["starred"] = json!(!item["starred"].as_bool().unwrap_or(false));
                Ok(())
            })
            .map_err(Error::from_anyhow)?;
        }
        "status" => {
            let status = input.value.as_deref().unwrap_or("");
            if !matches!(status, "unread" | "reading" | "done") {
                return Err(bad_request("状态无效").into());
            }
            book.set_reading_status(status)
                .map_err(Error::from_anyhow)?;
        }
        "rename" => {
            let name = input.value.as_deref().unwrap_or("").trim();
            if name.len() > 160 {
                return Err(bad_request("书名过长").into());
            }
            book.rename(name).map_err(Error::from_anyhow)?;
        }
        "trash" => {
            let _lock = HOME_PREFS_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let prefs_path = library().root.join(".library-home.json");
            let mut prefs = easyread_gpui::store::read_json(&prefs_path, json!({}))
                .map_err(Error::from_anyhow)?;
            if !prefs["removed"].is_object() {
                prefs["removed"] = json!({});
            }
            prefs["removed"][id.as_str()] =
                json!(!prefs["removed"][id.as_str()].as_bool().unwrap_or(false));
            easyread_gpui::store::write_json(&prefs_path, &prefs).map_err(Error::from_anyhow)?;
        }
        _ => return Err(bad_request("操作无效").into()),
    }
    Ok(see_other("/"))
}

#[route(POST "/import")]
async fn import(mut multipart: Multipart) -> Result<SeeOther> {
    let mut upload = None;
    while let Some(field) = multipart.next_field().await? {
        if field.name() != Some("book") {
            continue;
        }
        // Only use a short, harmless stem for the display name; never use a
        // browser-provided filename as a filesystem path.
        let stem = field
            .file_name()
            .unwrap_or("document")
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("document")
            .rsplit_once('.')
            .map(|(name, _)| name)
            .unwrap_or("document")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == ' ')
            .take(100)
            .collect::<String>();
        upload = Some((
            field.bytes().await?,
            if stem.is_empty() {
                "document".into()
            } else {
                stem
            },
        ));
        break;
    }
    let (bytes, stem) = upload.ok_or_else(|| bad_request("请选择 PDF 或 EPUB 文件"))?;
    let extension = if bytes.starts_with(b"%PDF") {
        "pdf"
    } else if bytes.starts_with(b"PK\x03\x04") {
        "epub"
    } else {
        return Err(bad_request("文件不是 PDF 或 EPUB").into());
    };
    let id = tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join(format!("{stem}.{extension}"));
        fs::write(&path, bytes)?;
        Ok(library().import_file(&path, None)?.id())
    })
    .await?
    .map_err(Error::from_anyhow)?;
    Ok(see_other(format!("/read/{id}")))
}
