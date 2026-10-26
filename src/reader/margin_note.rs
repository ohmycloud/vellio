//! A stable note identity and ordered persistence for the page-margin editor.
use crate::store::Workspace;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::mpsc, thread};

pub struct MarginNote {
    pub record: Value,
    queued: Option<Value>,
    saved: Option<Value>,
}

impl MarginNote {
    pub fn new(record: Value) -> Self {
        Self {
            record,
            queued: None,
            saved: None,
        }
    }

    pub fn pending(&self) -> Option<Value> {
        // Opening an editor alone must not create an empty note.
        if self.queued.as_ref() == Some(&self.record)
            || (self.queued.is_none()
                && self.record["body"].as_str().unwrap_or("").trim().is_empty())
        {
            None
        } else {
            Some(self.record.clone())
        }
    }

    pub fn queued(&mut self, snapshot: Value) {
        self.queued = Some(snapshot);
    }

    pub fn acknowledge(&mut self, snapshot: Value, success: bool) {
        if success {
            self.saved = Some(snapshot);
        } else if self.queued.as_ref() == Some(&snapshot) {
            self.queued = self.saved.clone();
        }
    }

    pub fn is_saved(&self) -> bool {
        self.saved.as_ref() == Some(&self.record)
    }
}

pub struct SavedNote {
    pub root: PathBuf,
    pub snapshot: Value,
    pub result: Result<Value, String>,
}

pub struct MarginWriter {
    sender: Option<mpsc::Sender<(Workspace, Value)>>,
    pub results: mpsc::Receiver<SavedNote>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Default for MarginWriter {
    fn default() -> Self {
        let (sender, requests) = mpsc::channel::<(Workspace, Value)>();
        let (results, receiver) = mpsc::channel();
        let worker = thread::spawn(move || {
            for (workspace, snapshot) in requests {
                let result = (|| {
                    let mut ops = vec![json!({"op":"note", "note":snapshot})];
                    if let Some(id) = snapshot["source_annotation"].as_str() {
                        let reader = workspace.load("reader")?;
                        let mut source = reader["notes"][id].clone();
                        if source.is_object()
                            && !source["deleted"].as_bool().unwrap_or(false)
                            && source["color"] != snapshot["color"]
                        {
                            source["color"] = snapshot["color"].clone();
                            source["updated"] = snapshot["updated"].clone();
                            ops.push(json!({"op":"note", "note":source}));
                        }
                    }
                    workspace.reader_ops(&ops)
                })()
                .and_then(|_| workspace.load("reader"))
                .map_err(|error| format!("笔记保存失败：{error:#}"));
                let _ = results.send(SavedNote {
                    root: workspace.root,
                    snapshot,
                    result,
                });
            }
        });
        Self {
            sender: Some(sender),
            results: receiver,
            worker: Some(worker),
        }
    }
}

impl MarginWriter {
    pub fn save(&self, workspace: Workspace, snapshot: Value) -> bool {
        self.sender
            .as_ref()
            .is_some_and(|sender| sender.send((workspace, snapshot)).is_ok())
    }
}

impl Drop for MarginWriter {
    fn drop(&mut self) {
        // Finish queued writes on window close, including the last keystroke.
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_is_empty_and_edits_keep_identity_and_anchor() {
        let mut note = MarginNote::new(json!({"id":"n1","anchor":"b1","quote":"same words",
            "prefix":"before","suffix":"after","kind":"note","body":""}));
        assert!(note.pending().is_none());
        note.record["body"] = json!("first");
        let first = note.pending().unwrap();
        note.queued(first.clone());
        assert!(note.pending().is_none());
        note.record["body"] = json!("second");
        note.record["kind"] = json!("question");
        note.record["color"] = json!("blue");
        note.acknowledge(first, true);
        assert!(!note.is_saved());
        let second = note.pending().unwrap();
        assert_eq!(second["id"], "n1");
        assert_eq!(second["anchor"], "b1");
        assert_eq!(second["prefix"], "before");
        note.queued(second.clone());
        note.acknowledge(second, true);
        assert!(note.is_saved());
        note.record["body"] = json!("");
        assert!(
            note.pending().is_some(),
            "clearing a saved body must persist"
        );
    }

    #[test]
    fn failed_write_can_retry_without_losing_newer_input() {
        let mut note = MarginNote::new(json!({"id":"n1","body":"first"}));
        let first = note.pending().unwrap();
        note.queued(first.clone());
        note.record["body"] = json!("newer");
        note.acknowledge(first, false);
        assert_eq!(note.pending().unwrap()["body"], "newer");
    }

    #[test]
    fn closing_drains_ordered_writes_and_preserves_the_source_mark() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Workspace {
            root: temp.path().to_owned(),
        };
        let mut reader = crate::store::empty_reader();
        reader["notes"]["mark"] = json!({"id":"mark","anchor":"b1","quote":"words",
            "kind":"highlight","style":"underline","color":"yellow",
            "updated":"2026-10-03T10:00:00.000Z"});
        crate::store::write_json(&workspace.root.join("reader.json"), &reader).unwrap();
        let writer = MarginWriter::default();
        for (i, body) in ["first", "latest"].into_iter().enumerate() {
            assert!(writer.save(
                workspace.clone(),
                json!({"id":"note1","anchor":"b1",
                "quote":"words","body":body,"kind":"question","color":"blue",
                "source_annotation":"mark","decoration":"none",
                "updated":format!("2026-10-03T10:00:0{}.000Z", i+1)})
            ));
        }
        drop(writer);
        let reader = workspace.load("reader").unwrap();
        assert_eq!(reader["notes"].as_object().unwrap().len(), 2);
        assert_eq!(reader["notes"]["note1"]["body"], "latest");
        assert_eq!(reader["notes"]["mark"]["style"], "underline");
        assert_eq!(reader["notes"]["mark"]["color"], "blue");
    }
}
