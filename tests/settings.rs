use easyread_gpui::{
    ai::{self, Transport},
    store::{Library, Workspace, write_json},
};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

fn workspace() -> (tempfile::TempDir, Workspace) {
    let temp = tempfile::tempdir().unwrap();
    let library = Library::open(temp.path().join("library")).unwrap();
    let root = library.root.join("book-settings");
    std::fs::create_dir(&root).unwrap();
    write_json(&root.join("paper.json"),&json!({"schema":2,"meta":{"title_en":"Batch fixture"},"future":42,"blocks":[
        {"id":"a","type":"heading","page":1,"en":"Title"},{"id":"b","type":"para","page":1,"en":"First"},
        {"id":"c","type":"para","page":2,"en":"Second"},{"id":"d","type":"para","page":3,"en":"Third"},
        {"id":"e","type":"para","page":4,"en":"Fourth"},{"id":"skip","type":"para","page":5,"en":"Done","zh":"已有译文"}]})).unwrap();
    write_json(&root.join("reader.json"),&json!({"schema":2,"edits":{"a":{"zh":"我的修订"}},"notes":{"n":{"kind":"note","body":"保留"}}})).unwrap();
    (temp, library.workspace("book-settings").unwrap())
}
struct Model {
    calls: AtomicUsize,
    invalid: bool,
    change: Option<Workspace>,
}
impl Transport for Model {
    fn model(&self) -> &str {
        "fixture"
    }
    fn request(
        &self,
        messages: Value,
        _: &AtomicBool,
        _: &mut dyn FnMut(&str),
    ) -> anyhow::Result<String> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let blocks: Vec<Value> = serde_json::from_str(messages[1]["content"].as_str().unwrap())?;
        if let Some(ws) = &self.change {
            ws.update("paper", |p| {
                p["blocks"][0]["en"] = json!("Changed while translating");
                Ok(())
            })?;
        }
        let mut response = serde_json::Map::new();
        for block in blocks {
            response.insert(
                block["id"].as_str().unwrap().into(),
                json!(format!("译文 {}", block["text"].as_str().unwrap())),
            );
        }
        if self.invalid {
            response.insert("unexpected".into(), json!("bad"));
        }
        Ok(serde_json::to_string(&response)?)
    }
}
#[test]
fn page_batches_resume_and_preserve_edits_notes_and_unknown_fields() {
    let (_temp, ws) = workspace();
    let reader = ws.load("reader").unwrap();
    let model = Model {
        calls: AtomicUsize::new(0),
        invalid: false,
        change: None,
    };
    let options = ai::TranslationOptions {
        pages_per_batch: 2,
        parallel: 2,
    };
    let mut updates = vec![];
    ai::translate_batched(
        &ws,
        &model,
        options,
        &AtomicBool::new(false),
        |done, total| updates.push((done, total)),
    )
    .unwrap();
    assert_eq!(model.calls.load(Ordering::Relaxed), 2);
    assert_eq!(updates.last(), Some(&(5, 5)));
    assert_eq!(ws.load("reader").unwrap(), reader);
    let paper = ws.load("paper").unwrap();
    assert_eq!(paper["future"], 42);
    assert_eq!(paper["blocks"][5]["zh"], "已有译文");
    assert_eq!(ws.load("job").unwrap()["state"], "done");
    ai::translate_batched(&ws, &model, options, &AtomicBool::new(false), |_, _| {}).unwrap();
    assert_eq!(model.calls.load(Ordering::Relaxed), 2);
}
#[test]
fn malformed_batch_and_concurrent_source_change_do_not_write_translations() {
    for change in [false, true] {
        let (_temp, ws) = workspace();
        let model = Model {
            calls: AtomicUsize::new(0),
            invalid: !change,
            change: change.then(|| ws.clone()),
        };
        assert!(
            ai::translate_batched(
                &ws,
                &model,
                ai::TranslationOptions {
                    pages_per_batch: 2,
                    parallel: 1
                },
                &AtomicBool::new(false),
                |_, _| {}
            )
            .is_err()
        );
        let paper = ws.load("paper").unwrap();
        assert!(paper["blocks"][0]["zh"].is_null());
        assert!(paper["blocks"][1]["zh"].is_null());
        assert_eq!(ws.load("job").unwrap()["state"], "partial");
    }
}
#[cfg(unix)]
mod cli {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn fixture(temp: &tempfile::TempDir, body: &str) -> String {
        let path = temp.path().join("fake-cli");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path.display().to_string()
    }
    fn connection(provider: ai::Provider, path: String) -> ai::Connection {
        ai::Connection::from_profile(ai::ModelProfile {
            id: "test".into(),
            name: "Test".into(),
            model: "test-model".into(),
            provider,
            executable: path,
            ..Default::default()
        })
        .unwrap()
    }
    #[test]
    fn claude_json_and_codex_final_message_are_the_answer() {
        let temp = tempfile::tempdir().unwrap();
        let executable = fixture(
            &temp,
            "cat >/dev/null\nprintf '%s' '{\"type\":\"result\",\"is_error\":false,\"result\":\"中文回答\"}'",
        );
        let c = connection(ai::Provider::ClaudeCode, executable);
        let mut streamed = String::new();
        assert_eq!(
            c.request(json!([]), &AtomicBool::new(false), &mut |s| streamed
                .push_str(s))
                .unwrap(),
            "中文回答"
        );
        assert_eq!(streamed, "中文回答");
        let executable = fixture(
            &temp,
            "while [ \"$#\" -gt 0 ]; do\n if [ \"$1\" = '--output-last-message' ]; then shift; output_path=\"$1\"; fi\n shift\ndone\ncat >/dev/null\nprintf '%s' 'Codex answer' >\"$output_path\"",
        );
        let c = connection(ai::Provider::CodexCli, executable);
        assert_eq!(
            c.request(json!([]), &AtomicBool::new(false), &mut |_| {})
                .unwrap(),
            "Codex answer"
        );
    }
    #[test]
    fn cancellation_reaps_cli_and_error_result_does_not_appear_as_success() {
        let temp = tempfile::tempdir().unwrap();
        let c = connection(
            ai::Provider::ClaudeCode,
            fixture(
                &temp,
                "cat >/dev/null\nprintf '%s' '{\"is_error\":true,\"result\":\"secret error details\"}'",
            ),
        );
        let e = c
            .request(json!([]), &AtomicBool::new(false), &mut |_| {})
            .unwrap_err()
            .to_string();
        assert!(!e.contains("secret"));
        let c = connection(ai::Provider::ClaudeCode, fixture(&temp, "exec sleep 20"));
        let cancel = AtomicBool::new(false);
        let start = std::time::Instant::now();
        std::thread::scope(|s| {
            s.spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(100));
                cancel.store(true, Ordering::Relaxed);
            });
            assert!(
                c.request(json!([]), &cancel, &mut |_| {})
                    .unwrap_err()
                    .to_string()
                    .contains("cancelled")
            );
        });
        assert!(start.elapsed() < std::time::Duration::from_secs(3));
    }
}
#[test]
fn library_copy_includes_assets_native_recovery_and_preferences() {
    let (temp, ws) = workspace();
    let source = ws.root.parent().unwrap();
    std::fs::create_dir(ws.root.join("figures")).unwrap();
    std::fs::write(ws.root.join("figures/diagram.png"), b"image bytes").unwrap();
    std::fs::write(ws.root.join("paper.pdf"), b"pdf bytes").unwrap();
    write_json(&ws.root.join(".native-reader.json"),&json!({"progress":{"block":"c","offset":22},"drafts":{"c":{"edit":{"text":"未保存修订"}}}})).unwrap();
    write_json(
        &source.join(".library-home.json"),
        &json!({"folders":[{"id":"f1","name":"Research"}],"assignments":{"book-settings":["f1"]}}),
    )
    .unwrap();
    let destination = temp.path().join("cloud-library");
    easyread_gpui::settings::copy_library(source, &destination, &AtomicBool::new(false)).unwrap();
    let copy = Library::open(&destination)
        .unwrap()
        .workspace("book-settings")
        .unwrap();
    for file in [
        "paper.json",
        "reader.json",
        "paper.pdf",
        "figures/diagram.png",
        ".native-reader.json",
    ] {
        assert_eq!(
            std::fs::read(ws.root.join(file)).unwrap(),
            std::fs::read(copy.root.join(file)).unwrap()
        );
    }
    assert_eq!(
        std::fs::read(source.join(".library-home.json")).unwrap(),
        std::fs::read(destination.join(".library-home.json")).unwrap()
    );
    assert!(
        easyread_gpui::settings::copy_library(source, &destination, &AtomicBool::new(false))
            .is_err()
    );
}
