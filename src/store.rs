//! Local, schema-2 compatible library. All mutation paths preserve unknown JSON fields.
use anyhow::{Context, Result, bail, ensure};
use chrono::Utc;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;

pub fn now() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
pub fn read_json(path: &Path, default: Value) -> Result<Value> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .with_context(|| format!("Invalid JSON: {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(default),
        Err(e) => Err(e.into()),
    }
}
pub fn write_json(path: &Path, value: &Value) -> Result<()> {
    let parent = path.parent().context("JSON path has no parent")?;
    fs::create_dir_all(parent)?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut tmp, value)?;
    tmp.write_all(b"\n")?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| e.error)?;
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}
/// The legacy app uses O_EXCL on .write.lock. Use the same protocol, never steal a live lock.
struct Lock {
    path: PathBuf,
}
impl Lock {
    fn acquire(folder: &Path) -> Result<Self> {
        let path = folder.join(".write.lock");
        let start = Instant::now();
        loop {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut f) => {
                    writeln!(f, "{}", std::process::id())?;
                    return Ok(Self { path });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    ensure!(
                        start.elapsed() < Duration::from_secs(15),
                        "Workspace locked: {}. A crashed writer may require lock recovery.",
                        folder.display()
                    );
                    thread::sleep(Duration::from_millis(25));
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[derive(Clone, Debug)]
pub struct Library {
    pub root: PathBuf,
}
#[derive(Clone, Debug)]
pub struct Workspace {
    pub root: PathBuf,
}
impl Library {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        fs::create_dir_all(root.as_ref())?;
        Ok(Self {
            root: root.as_ref().canonicalize()?,
        })
    }
    pub fn workspace(&self, id: &str) -> Result<Workspace> {
        ensure!(
            (4..=64).contains(&id.len())
                && id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-'),
            "Invalid paper id"
        );
        let root = self.root.join(id).canonicalize()?;
        ensure!(
            root.starts_with(&self.root) && root.join("paper.json").is_file(),
            "Invalid workspace"
        );
        Ok(Workspace { root })
    }
    pub fn list(&self) -> Result<Vec<Value>> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let path = entry?.path();
            let id = path.file_name().unwrap_or_default().to_string_lossy();
            if id.starts_with('.') || !path.join("paper.json").is_file() {
                continue;
            }
            let ws = self.workspace(&id)?;
            let state = ws.state()?;
            let native = read_json(&ws.root.join(".native-reader.json"), json!({}))
                .unwrap_or_else(|_| json!({}));
            entries.push(crate::library_home::summary(&id, &state, &native));
        }
        entries.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        Ok(entries)
    }
    pub fn import_pdf(&self, path: &Path, pdfium: Option<&Path>) -> Result<Workspace> {
        self.import_pdf_cancellable(
            path,
            pdfium,
            &std::sync::atomic::AtomicBool::new(false),
            |_, _| {},
        )
    }
    pub fn import_pdf_cancellable(
        &self,
        path: &Path,
        pdfium: Option<&Path>,
        cancel: &std::sync::atomic::AtomicBool,
        progress: impl FnMut(usize, usize),
    ) -> Result<Workspace> {
        let data = fs::read(path)?;
        ensure!(data.starts_with(b"%PDF"), "Not a PDF file");
        let digest = format!("{:x}", Sha256::digest(&data));
        let id = &digest[..12];
        if self.root.join(id).join("paper.json").is_file() {
            return self.workspace(id);
        }
        // Parse before publishing the workspace; cancellation/error leaves no partial library entry.
        let staging = tempfile::Builder::new()
            .prefix(".import-")
            .tempdir_in(&self.root)?;
        fs::write(staging.path().join("source.pdf"), data)?;
        let extracted = crate::pdf::prepare(
            &staging.path().join("source.pdf"),
            staging.path(),
            pdfium,
            cancel,
            progress,
        )?;
        let title = if extracted.title.trim().is_empty() {
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        } else {
            extracted.title
        };
        let paper = json!({"schema":2, "meta":{"title_en":title,"title_zh":"", "authors":extracted.author,"source":path.file_name().unwrap_or_default().to_string_lossy(),"source_sha256":digest,"pdf":"source.pdf","page_count":extracted.pages.len(),"pages":extracted.pages},"translation":{"scope":"未开始","done_pages":[],"en_pages":[],"note":"Rust PDFium extraction; source text is not semantic paper reflow"},"glossary":[],"references":[],"blocks":extracted.blocks});
        write_json(&staging.path().join("paper.json"), &paper)?;
        write_json(&staging.path().join("layout.json"), &extracted.layout)?;
        write_json(&staging.path().join("reader.json"), &empty_reader())?;
        write_json(
            &staging.path().join("discussion.json"),
            &json!({"schema":2,"entries":[]}),
        )?;
        write_json(
            &staging.path().join("item.json"),
            &json!({"added":now(),"tags":[],"status":"unread","starred":false}),
        )?;
        let _lock = Lock::acquire(&self.root)?;
        ensure!(
            !cancel.load(std::sync::atomic::Ordering::Relaxed),
            "PDF import cancelled"
        );
        if !self.root.join(id).exists() {
            fs::rename(staging.path(), self.root.join(id))?;
        }
        self.workspace(id)
    }
    /// Format-neutral entry point; inspect the file signature, not its extension.
    pub fn import_file(&self, path: &Path, pdfium: Option<&Path>) -> Result<Workspace> {
        self.import_file_cancellable(
            path,
            pdfium,
            &std::sync::atomic::AtomicBool::new(false),
            |_, _| {},
        )
    }
    pub fn import_file_cancellable(
        &self,
        path: &Path,
        pdfium: Option<&Path>,
        cancel: &std::sync::atomic::AtomicBool,
        progress: impl FnMut(usize, usize),
    ) -> Result<Workspace> {
        use std::io::Read;
        let mut signature = [0u8; 4];
        File::open(path)?.read_exact(&mut signature)?;
        match &signature {
            b"%PDF" => self.import_pdf_cancellable(path, pdfium, cancel, progress),
            b"PK\x03\x04" => self.import_epub_cancellable(path, cancel, progress),
            _ => bail!("Unsupported document; choose a PDF or EPUB file"),
        }
    }
    fn import_epub_cancellable(
        &self,
        path: &Path,
        cancel: &std::sync::atomic::AtomicBool,
        progress: impl FnMut(usize, usize),
    ) -> Result<Workspace> {
        ensure!(
            !cancel.load(std::sync::atomic::Ordering::Relaxed),
            "EPUB import cancelled"
        );
        ensure!(
            fs::metadata(path)?.len() <= 512 * 1024 * 1024,
            "EPUB exceeds file size limit"
        );
        let data = fs::read(path)?;
        let digest = format!("{:x}", Sha256::digest(&data));
        let id = &digest[..12];
        if self.root.join(id).join("paper.json").is_file() {
            return self.workspace(id);
        }
        let staging = tempfile::Builder::new()
            .prefix(".import-")
            .tempdir_in(&self.root)?;
        fs::write(staging.path().join("source.epub"), data)?;
        let mut paper = crate::formats::epub::prepare(
            &staging.path().join("source.epub"),
            staging.path(),
            cancel,
            progress,
        )?;
        paper["meta"]["source"] = json!(path.file_name().unwrap_or_default().to_string_lossy());
        paper["meta"]["source_sha256"] = json!(digest);
        if paper["meta"]["title_en"]
            .as_str()
            .is_none_or(|s| s.trim().is_empty())
        {
            paper["meta"]["title_en"] =
                json!(path.file_stem().unwrap_or_default().to_string_lossy());
        }
        crate::model::ReaderState::from_value(json!({"paper":paper,"reader":empty_reader()}))?;
        write_json(&staging.path().join("paper.json"), &paper)?;
        write_json(&staging.path().join("layout.json"), &json!({}))?;
        write_json(&staging.path().join("reader.json"), &empty_reader())?;
        write_json(
            &staging.path().join("discussion.json"),
            &json!({"schema":2,"entries":[]}),
        )?;
        write_json(
            &staging.path().join("item.json"),
            &json!({"added":now(),"tags":[],"status":"unread","starred":false}),
        )?;
        let _lock = Lock::acquire(&self.root)?;
        ensure!(
            !cancel.load(std::sync::atomic::Ordering::Relaxed),
            "EPUB import cancelled"
        );
        if !self.root.join(id).exists() {
            fs::rename(staging.path(), self.root.join(id))?;
        }
        self.workspace(id)
    }
    /// Copy a legacy workspace into this library; never write back to the source.
    pub fn migrate_copy(&self, source: &Path) -> Result<Workspace> {
        let source = source.canonicalize()?;
        ensure!(
            !source.starts_with(&self.root),
            "Migration source must be outside the destination library"
        );
        let data = fs::read(source.join("source.pdf"))?;
        ensure!(data.starts_with(b"%PDF"), "Source PDF is invalid");
        let digest = format!("{:x}", Sha256::digest(data));
        let id = &digest[..12];
        let paper = read_json(&source.join("paper.json"), Value::Null)?;
        let reader = read_json(&source.join("reader.json"), empty_reader())?;
        crate::model::ReaderState::from_value(json!({"paper":paper,"reader":reader}))?;
        let stage = tempfile::Builder::new()
            .prefix(".migrate-")
            .tempdir_in(&self.root)?;
        copy_tree(&source, stage.path())?;
        if !stage.path().join("reader.json").exists() {
            write_json(&stage.path().join("reader.json"), &empty_reader())?;
        }
        if !stage.path().join("item.json").exists() {
            write_json(
                &stage.path().join("item.json"),
                &json!({"added":now(),"status":"reading","tags":[]}),
            )?;
        }
        let _lock = Lock::acquire(&self.root)?;
        ensure!(
            !self.root.join(id).exists(),
            "Destination already exists; existing data was preserved"
        );
        fs::rename(stage.path(), self.root.join(id))?;
        self.workspace(id)
    }
}
fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from)? {
        let e = e?;
        let name = e.file_name();
        if name == ".write.lock" || name == "server.json" {
            continue;
        }
        ensure!(
            !e.file_type()?.is_symlink(),
            "Migration refuses symlinks: {}",
            e.path().display()
        );
        if e.file_type()?.is_dir() {
            copy_tree(&e.path(), &to.join(name))?;
        } else {
            fs::copy(e.path(), to.join(name))?;
        }
    }
    Ok(())
}
pub fn empty_reader() -> Value {
    json!({"schema":2,"rev":0,"edits":{},"notes":{},"paper_note":{},"progress":{}})
}
impl Workspace {
    pub fn complete_pdf_images(
        &self,
        cancel: &std::sync::atomic::AtomicBool,
        progress: impl FnMut(usize, usize),
    ) -> Result<usize> {
        let initial = self.load("paper")?;
        ensure!(
            initial["blocks"]
                .as_array()
                .is_some_and(|bs| bs.iter().any(|b| b["source_kind"] == "pdf-text")),
            "Image completion is for Rust text-only imports; semantic figure blocks are already preserved"
        );
        let media = crate::pdf::media(&self.asset("source.pdf")?, &self.root, cancel, progress)?;
        let _lock = Lock::acquire(&self.root)?;
        ensure!(
            !cancel.load(std::sync::atomic::Ordering::Relaxed),
            "Image extraction cancelled"
        );
        let mut paper = self.load("paper")?;
        let mut layout = self.load("layout")?;
        let layout = layout.as_object_mut().context("Layout must be an object")?;
        let blocks = paper["blocks"]
            .as_array_mut()
            .context("Paper blocks must be an array")?;
        let mut added = 0;
        for block in media.blocks {
            let id = block["id"].as_str().unwrap();
            if blocks.iter().any(|b| b["id"] == id) {
                continue;
            }
            // Append after the last existing block on that page. Existing
            // semantic reading order and stable user-owned keys stay intact.
            let position = blocks
                .iter()
                .rposition(|b| b["page"] == block["page"])
                .map(|i| i + 1)
                .unwrap_or(blocks.len());
            layout.insert(id.into(), media.layout[id].clone());
            blocks.insert(position, block);
            added += 1;
        }
        if added > 0 {
            let history = self.root.join("history");
            fs::create_dir_all(&history)?;
            let backup = Uuid::new_v4();
            for part in ["paper", "layout"] {
                let path = self.root.join(format!("{part}.json"));
                if path.exists() {
                    fs::copy(
                        path,
                        history.join(format!("{part}-before-images-{backup}.json")),
                    )?;
                }
            }
            write_json(&self.root.join("layout.json"), &json!(layout))?;
            write_json(&self.root.join("paper.json"), &paper)?;
        }
        Ok(added)
    }
    pub fn id(&self) -> String {
        self.root
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    }
    pub fn load(&self, part: &str) -> Result<Value> {
        ensure!(
            [
                "paper",
                "reader",
                "discussion",
                "layout",
                "item",
                "chat",
                "job"
            ]
            .contains(&part),
            "Unknown workspace part"
        );
        read_json(
            &self.root.join(format!("{part}.json")),
            match part {
                "reader" => empty_reader(),
                "discussion" => json!({"schema":2,"entries":[]}),
                "chat" => json!({"threads":[]}),
                _ => json!({}),
            },
        )
    }
    pub fn state(&self) -> Result<Value> {
        let mut state = json!({});
        for part in [
            "paper",
            "reader",
            "discussion",
            "layout",
            "item",
            "chat",
            "job",
        ] {
            state[part] = self.load(part)?;
        }
        Ok(state)
    }
    pub fn validated_state(&self) -> Result<Value> {
        let state = self.state()?;
        crate::model::ReaderState::from_value(state.clone())?;
        Ok(state)
    }
    pub fn asset(&self, relative: &str) -> Result<PathBuf> {
        let rel = Path::new(relative);
        ensure!(
            !rel.is_absolute() && rel.components().all(|c| matches!(c, Component::Normal(_))),
            "Unsafe asset path"
        );
        let path = self.root.join(rel).canonicalize()?;
        ensure!(
            path.starts_with(self.root.canonicalize()?),
            "Asset escapes workspace"
        );
        Ok(path)
    }
    pub fn update(
        &self,
        part: &str,
        change: impl FnOnce(&mut Value) -> Result<()>,
    ) -> Result<Value> {
        let _lock = Lock::acquire(&self.root)?;
        let mut value = self.load(part)?;
        change(&mut value)?;
        write_json(&self.root.join(format!("{part}.json")), &value)?;
        Ok(value)
    }
    /// Rename the library entry without moving assets or replacing bibliographic titles.
    pub fn rename(&self, name: &str) -> Result<Value> {
        let name = name.trim();
        ensure!(!name.is_empty(), "Name cannot be empty");
        ensure!(
            !name.chars().any(char::is_control),
            "Name must be a single line"
        );
        self.update("paper", |paper| {
            let meta = object(paper, "meta")?;
            meta.insert("display_name".into(), json!(name));
            Ok(())
        })
    }
    pub fn reader_ops(&self, ops: &[Value]) -> Result<Value> {
        let _lock = Lock::acquire(&self.root)?;
        let mut reader = self.load("reader")?;
        // Inline paragraph edits merge into a captured block. Check that the
        // underlying translation has not changed while the editor was open.
        if ops.iter().any(|op| op.get("expected_text").is_some()) {
            let state = crate::model::ReaderState::from_value(
                json!({"paper":self.load("paper")?, "reader":reader}),
            )?;
            for op in ops {
                if let Some(expected) = op["expected_text"].as_str() {
                    let key = required(op, "block")?;
                    let caption = key.ends_with("#caption");
                    let block = state
                        .paper
                        .blocks
                        .iter()
                        .find(|block| block.id == key.trim_end_matches("#caption"))
                        .context("修订段落已不存在")?;
                    let current = if caption {
                        state.caption(block)
                    } else {
                        state.text(key, &block.zh, &block.en)
                    };
                    ensure!(
                        crate::model::text_hash(current) == expected,
                        "译文已更新，请取消后重新编辑"
                    );
                }
            }
        }
        let applied = apply_ops(&mut reader, ops)?;
        if !applied.is_empty() {
            let history = self.root.join("history");
            fs::create_dir_all(&history)?;
            let old = self.root.join("reader.json");
            if old.exists() {
                fs::copy(old, history.join(format!("reader-{}.json", Uuid::new_v4())))?;
            }
            // Persist the snapshot first; journal is an audit record, not a replay commit log.
            write_json(&self.root.join("reader.json"), &reader)?;
            let mut journal = OpenOptions::new()
                .create(true)
                .append(true)
                .open(history.join("reader.log.jsonl"))?;
            for op in ops {
                let mut entry = op.clone();
                entry["t"] = json!(now());
                entry["client"] = json!("rust-native");
                writeln!(journal, "{entry}")?;
            }
            journal.sync_all()?;
        }
        Ok(json!({"rev":reader["rev"],"applied":applied}))
    }
}
fn object<'a>(v: &'a mut Value, key: &str) -> Result<&'a mut serde_json::Map<String, Value>> {
    if v.get(key).is_none() {
        v[key] = json!({});
    }
    v[key]
        .as_object_mut()
        .with_context(|| format!("{key} must be an object"))
}
fn required<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v[key]
        .as_str()
        .with_context(|| format!("Missing string {key}"))
}
fn newer(at: &str, old: &Value, key: &str) -> bool {
    let previous = old[key].as_str().unwrap_or("");
    match (
        chrono::DateTime::parse_from_rfc3339(at),
        chrono::DateTime::parse_from_rfc3339(previous),
    ) {
        (Ok(a), Ok(b)) => a >= b,
        _ => at >= previous,
    }
}
/// Batch is transactional: malformed operations cannot partially mutate a reader.
pub fn apply_ops(reader: &mut Value, ops: &[Value]) -> Result<Vec<String>> {
    let mut next = reader.clone();
    let mut applied = Vec::new();
    for op in ops {
        let kind = required(op, "op")?;
        let stamp = op["at"].as_str().map(str::to_owned).unwrap_or_else(now);
        let before = next.clone();
        let label = match kind {
            "edit" => {
                let key = required(op, "block")?;
                ensure!(!key.is_empty(), "Empty edit key");
                let edits = object(&mut next, "edits")?;
                let cur = edits.get(key).cloned().unwrap_or(Value::Null);
                if let Some(expected) = op.get("expected_edit") {
                    ensure!(expected == &cur, "译文已被其他修订更新，请取消后重新编辑");
                }
                if !newer(&stamp, &cur, "at") {
                    continue;
                }
                if op["zh"].is_null() {
                    if !cur.is_null() && !cur["reverted"].as_bool().unwrap_or(false) {
                        edits.insert(
                            key.into(),
                            json!({"reverted":true,"prev":cur["zh"],"at":stamp}),
                        );
                    }
                } else {
                    let zh = required(op, "zh")?;
                    let mut entry =
                        json!({"zh":zh,"base":op["base"].as_str().unwrap_or(""),"at":stamp});
                    if let Some(layout) = op.get("native_layout") {
                        ensure!(
                            layout["translation_hash"] == crate::model::text_hash(zh)
                                && layout["html"].is_string()
                                && layout["source_hash"].is_string(),
                            "Invalid native edit layout"
                        );
                        entry["native_layout"] = layout.clone();
                    }
                    if cur["zh"].as_str().is_some_and(|s| !s.is_empty() && s != zh) {
                        entry["prev"] = cur["zh"].clone();
                    } else if !cur["prev"].is_null() {
                        entry["prev"] = cur["prev"].clone();
                    }
                    edits.insert(key.into(), entry);
                }
                format!("edit:{key}")
            }
            "note" => {
                let note = &op["note"];
                let id = required(note, "id")?;
                let updated = required(note, "updated")?;
                ensure!(!id.is_empty(), "Empty note id");
                let notes = object(&mut next, "notes")?;
                let cur = notes.get(id).cloned().unwrap_or(Value::Null);
                if !newer(updated, &cur, "updated") {
                    continue;
                }
                notes.insert(id.into(), note.clone());
                format!("note:{id}")
            }
            "note_del" => {
                let id = required(op, "id")?;
                let notes = object(&mut next, "notes")?;
                let mut cur = notes.get(id).cloned().unwrap_or_else(|| json!({"id":id}));
                if !newer(&stamp, &cur, "updated") {
                    continue;
                }
                cur["deleted"] = json!(true);
                cur["updated"] = json!(stamp);
                notes.insert(id.into(), cur);
                format!("note_del:{id}")
            }
            "paper_note" => {
                if !newer(&stamp, &next["paper_note"], "at") {
                    continue;
                }
                next["paper_note"] = json!({"body":op["body"].as_str().unwrap_or(""),"at":stamp});
                "paper_note".into()
            }
            "progress" => {
                if !newer(&stamp, &next["progress"], "at") {
                    continue;
                }
                let progress = object(&mut next, "progress")?;
                progress.insert("block".into(), op["block"].clone());
                progress.insert("at".into(), json!(stamp));
                if let Some(ratio) = op["ratio"].as_f64() {
                    ensure!((0.0..=1.0).contains(&ratio), "Invalid progress ratio");
                    progress.insert("ratio".into(), json!(ratio));
                }
                "progress".into()
            }
            _ => bail!("Unknown reader operation: {kind}"),
        };
        if next != before {
            applied.push(label);
        }
    }
    if !applied.is_empty() {
        next["rev"] = json!(next["rev"].as_u64().unwrap_or(0) + 1);
    }
    *reader = next;
    Ok(applied)
}
