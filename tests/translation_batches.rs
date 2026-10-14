//! Upstream v1.4.1 omission/selected-page regressions. No network or paid model.
use easyread_gpui::{
    ai::{self, TranslationOptions, Transport},
    store::{Library, Workspace, write_json},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

fn workspace() -> (tempfile::TempDir, Workspace) {
    let temp = tempfile::tempdir().unwrap();
    let library = Library::open(temp.path()).unwrap();
    let root = library.root.join("batch-book");
    std::fs::create_dir(&root).unwrap();
    write_json(&root.join("paper.json"), &json!({"schema":2,"meta":{"title_en":"Regression","pages":[{},{},{},{}]},"future":42,"blocks":[
        {"id":"a","type":"heading","page":1,"en":"Title"},
        {"id":"b","type":"para","page":1,"en":"First 🦀"},
        {"id":"c","type":"para","page":2,"en":"Second"},
        {"id":"d","type":"para","page":3,"en":"Third"},
        {"id":"e","type":"para","page":4,"en":"Done","zh":"已有译文"}
    ]})).unwrap();
    write_json(&root.join("reader.json"), &json!({"schema":2,"edits":{"b":{"zh":"用户修订"}},"notes":{"n":{"anchor":"b","quote":"First","body":"笔记"}},"future":true})).unwrap();
    (temp, library.workspace("batch-book").unwrap())
}

struct Model<F> {
    calls: Mutex<Vec<Vec<String>>>,
    respond: F,
}
impl<F> Model<F> {
    fn new(respond: F) -> Self {
        Self {
            calls: Mutex::new(vec![]),
            respond,
        }
    }
}
impl<F> Transport for Model<F>
where
    F: Fn(&[String], usize, &AtomicBool) -> anyhow::Result<String> + Sync,
{
    fn model(&self) -> &str {
        "explicit-test-stub"
    }
    fn request(
        &self,
        messages: Value,
        cancel: &AtomicBool,
        _: &mut dyn FnMut(&str),
    ) -> anyhow::Result<String> {
        let input: Vec<Value> = serde_json::from_str(messages[1]["content"].as_str().unwrap())?;
        let ids: Vec<_> = input
            .iter()
            .map(|b| b["id"].as_str().unwrap().to_owned())
            .collect();
        let n = {
            let mut calls = self.calls.lock().unwrap();
            calls.push(ids.clone());
            calls.len()
        };
        (self.respond)(&ids, n, cancel)
    }
}
fn response(ids: &[String], omit: &[&str]) -> anyhow::Result<String> {
    let values: serde_json::Map<_, _> = ids
        .iter()
        .filter(|id| !omit.contains(&id.as_str()))
        .map(|id| (id.clone(), json!(format!("译文 {id} 🦀"))))
        .collect();
    Ok(serde_json::to_string(&values)?)
}
fn options(parallel: usize) -> TranslationOptions {
    TranslationOptions {
        pages_per_batch: 2,
        parallel,
    }
}

#[test]
fn omitted_page_is_retried_once_without_resending_completed_blocks() {
    let (_temp, ws) = workspace();
    let reader = std::fs::read(ws.root.join("reader.json")).unwrap();
    let model = Model::new(|ids: &[String], call, _: &AtomicBool| {
        response(ids, if call == 1 { &["c"] } else { &[] })
    });
    let mut updates = vec![];
    ai::translate_batched(&ws, &model, options(1), &AtomicBool::new(false), |d, t| {
        updates.push((d, t))
    })
    .unwrap();
    assert_eq!(
        *model.calls.lock().unwrap(),
        vec![vec!["a", "b", "c"], vec!["c"], vec!["d"]]
    );
    assert_eq!(updates.last(), Some(&(4, 4)));
    let job = ws.load("job").unwrap();
    assert_eq!(job["state"], "done");
    assert_eq!(job["failed_pages"], json!([]));
    assert_eq!(job["pending_block_ids"], json!([]));
    assert_eq!(std::fs::read(ws.root.join("reader.json")).unwrap(), reader);
    let paper = ws.load("paper").unwrap();
    assert_eq!(paper["future"], 42);
    assert_eq!(paper["blocks"][4]["zh"], "已有译文");
}

#[test]
fn persistent_omission_marks_only_failed_page_and_other_batches_continue() {
    for parallel in [1, 3] {
        let (_temp, ws) = workspace();
        let model = Model::new(|ids: &[String], _, _: &AtomicBool| response(ids, &["c"]));
        assert!(
            ai::translate_batched(
                &ws,
                &model,
                options(parallel),
                &AtomicBool::new(false),
                |_, _| {}
            )
            .is_err()
        );
        let paper = ws.load("paper").unwrap();
        assert!(paper["blocks"][2]["zh"].is_null());
        assert_eq!(paper["blocks"][3]["zh"], "译文 d 🦀");
        let job = ws.load("job").unwrap();
        assert_eq!(job["state"], "partial");
        assert_eq!(job["done"], 3);
        assert_eq!(job["failed_pages"], json!([2]));
        assert_eq!(job["failed_block_ids"], json!(["c"]));
        assert_eq!(job["pending_block_ids"], json!(["c"]));
        let retry = Model::new(|ids: &[String], _, _: &AtomicBool| response(ids, &[]));
        ai::translate_batched(
            &ws,
            &retry,
            options(parallel),
            &AtomicBool::new(false),
            |_, _| {},
        )
        .unwrap();
        assert_eq!(*retry.calls.lock().unwrap(), vec![vec!["c"]]);
        assert_eq!(ws.load("job").unwrap()["failed_pages"], json!([]));
    }
}

#[test]
fn empty_translation_is_an_omission_not_a_completed_page() {
    let (_temp, ws) = workspace();
    let model = Model::new(|ids: &[String], _, _: &AtomicBool| {
        let mut value: Value = serde_json::from_str(&response(ids, &[])?)?;
        if ids.iter().any(|id| id == "c") {
            value["c"] = json!(" \n\t");
        }
        Ok(value.to_string())
    });
    assert!(
        ai::translate_batched(&ws, &model, options(1), &AtomicBool::new(false), |_, _| {}).is_err()
    );
    assert_eq!(ws.load("job").unwrap()["failed_pages"], json!([2]));
    assert!(ws.load("paper").unwrap()["blocks"][2]["zh"].is_null());
}

#[test]
fn malformed_or_unknown_id_batch_fails_atomically_but_does_not_stop_queue() {
    for answer in [
        "not JSON",
        "{\"a\":\"译文\",\"unknown\":\"bad\"}",
        "{\"a\":123}",
    ] {
        let (_temp, ws) = workspace();
        let model = Model::new(|ids: &[String], call, _: &AtomicBool| {
            if call == 1 {
                Ok(answer.to_owned())
            } else {
                response(ids, &[])
            }
        });
        assert!(
            ai::translate_batched(&ws, &model, options(1), &AtomicBool::new(false), |_, _| {})
                .is_err()
        );
        let paper = ws.load("paper").unwrap();
        for i in 0..3 {
            assert!(paper["blocks"][i]["zh"].is_null());
        }
        assert_eq!(paper["blocks"][3]["zh"], "译文 d 🦀");
        assert_eq!(model.calls.lock().unwrap().len(), 2);
        let job = ws.load("job").unwrap();
        assert_eq!(job["failed_pages"], json!([1, 2]));
        assert_eq!(job["done"], 1);
    }
}

#[test]
fn selected_pages_skip_completed_blocks_and_reject_invalid_selection_before_work() {
    let (_temp, ws) = workspace();
    let model = Model::new(|ids: &[String], _, _: &AtomicBool| response(ids, &[]));
    for pages in [&[][..], &[0][..], &[2, 99][..]] {
        assert!(
            ai::translate_pages_batched(
                &ws,
                &model,
                options(1),
                pages,
                &AtomicBool::new(false),
                |_, _| {}
            )
            .is_err()
        );
        assert!(!ws.root.join("job.json").exists());
        assert!(model.calls.lock().unwrap().is_empty());
    }
    ai::translate_pages_batched(
        &ws,
        &model,
        options(1),
        &[2, 2, 4],
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    assert_eq!(*model.calls.lock().unwrap(), vec![vec!["c"]]);
    let paper = ws.load("paper").unwrap();
    assert!(paper["blocks"][0]["zh"].is_null());
    assert!(paper["blocks"][3]["zh"].is_null());
    assert_eq!(paper["blocks"][4]["zh"], "已有译文");
    assert_eq!(ws.load("job").unwrap()["requested_pages"], json!([2, 4]));
    ai::translate_pages_batched(
        &ws,
        &model,
        options(1),
        &[2, 4],
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    assert_eq!(model.calls.lock().unwrap().len(), 1);
}

#[test]
fn cancellation_keeps_committed_work_and_never_publishes_inflight_answer() {
    let (_temp, ws) = workspace();
    let model = Model::new(|ids: &[String], call, cancel: &AtomicBool| {
        if call == 2 {
            cancel.store(true, Ordering::Relaxed);
        }
        response(ids, &[])
    });
    let cancel = AtomicBool::new(false);
    assert!(ai::translate_batched(&ws, &model, options(1), &cancel, |_, _| {}).is_err());
    let paper = ws.load("paper").unwrap();
    assert_eq!(paper["blocks"][0]["zh"], "译文 a 🦀");
    assert!(paper["blocks"][3]["zh"].is_null());
    let job = ws.load("job").unwrap();
    assert_eq!(job["state"], "cancelled");
    assert_eq!(job["done"], 3);
    assert_eq!(job["pending_block_ids"], json!(["d"]));
    let before = std::fs::read(ws.root.join("job.json")).unwrap();
    assert!(ai::translate_batched(&ws, &model, options(1), &cancel, |_, _| {}).is_err());
    assert_eq!(std::fs::read(ws.root.join("job.json")).unwrap(), before);
}

#[test]
fn source_change_during_page_retry_keeps_earlier_success_and_never_overwrites_changes() {
    let (_temp, ws) = workspace();
    let model = Model::new(|ids: &[String], call, _: &AtomicBool| {
        if call == 2 {
            ws.update("paper", |p| {
                p["blocks"][2]["en"] = json!("Changed");
                Ok(())
            })?;
        }
        response(ids, if call == 1 { &["c"] } else { &[] })
    });
    assert!(
        ai::translate_batched(&ws, &model, options(1), &AtomicBool::new(false), |_, _| {}).is_err()
    );
    let paper = ws.load("paper").unwrap();
    assert_eq!(paper["blocks"][0]["zh"], "译文 a 🦀");
    assert_eq!(paper["blocks"][2]["en"], "Changed");
    assert!(paper["blocks"][2]["zh"].is_null());
    assert_eq!(paper["blocks"][3]["zh"], "译文 d 🦀");
}

#[test]
fn catalog_corrects_false_completed_pages_without_rewriting_old_data() {
    let (_temp, ws) = workspace();
    ws.update("paper", |p| {
        p["translation"] = json!({"done_pages":[1,2,3,4],"failed_pages":[2],"future":"keep"});
        Ok(())
    })
    .unwrap();
    let before = std::fs::read(ws.root.join("paper.json")).unwrap();
    let summary =
        easyread_gpui::library_home::summary("batch-book", &ws.state().unwrap(), &json!({}));
    assert_eq!(summary["untranslated_pages"], json!([1, 2, 3]));
    assert_eq!(summary["translation"]["done_pages"], json!([4]));
    assert_eq!(summary["translation"]["future"], "keep");
    assert_eq!(std::fs::read(ws.root.join("paper.json")).unwrap(), before);
    let model = Model::new(|ids: &[String], _, _: &AtomicBool| response(ids, &["c"]));
    assert!(
        ai::translate_batched(&ws, &model, options(1), &AtomicBool::new(false), |_, _| {}).is_err()
    );
    let summary =
        easyread_gpui::library_home::summary("batch-book", &ws.state().unwrap(), &json!({}));
    assert_eq!(summary["translation"]["done_pages"], json!([1, 3, 4]));
    assert_eq!(summary["translation"]["failed_pages"], json!([2]));
    let retry = Model::new(|ids: &[String], _, _: &AtomicBool| response(ids, &[]));
    ai::translate_pages_batched(
        &ws,
        &retry,
        options(1),
        &[2],
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    let summary =
        easyread_gpui::library_home::summary("batch-book", &ws.state().unwrap(), &json!({}));
    assert_eq!(summary["translation"]["done_pages"], json!([1, 2, 3, 4]));
    assert_eq!(summary["translation"]["failed_pages"], json!([]));
}

#[test]
fn epub_can_translate_all_blocks_but_has_no_source_pdf_page_selection() {
    let (_temp, ws) = workspace();
    ws.update("paper", |p| {
        p["meta"]["format"] = json!("epub");
        p["meta"]["pages"] = json!([]);
        for b in p["blocks"].as_array_mut().unwrap() {
            b["page"] = json!(0);
        }
        Ok(())
    })
    .unwrap();
    let model = Model::new(|ids: &[String], _, _: &AtomicBool| response(ids, &[]));
    assert!(
        ai::translate_pages_batched(
            &ws,
            &model,
            options(1),
            &[1],
            &AtomicBool::new(false),
            |_, _| {}
        )
        .is_err()
    );
    ai::translate_batched(&ws, &model, options(2), &AtomicBool::new(false), |_, _| {}).unwrap();
    let completed: BTreeSet<_> = model
        .calls
        .lock()
        .unwrap()
        .iter()
        .flatten()
        .cloned()
        .collect();
    assert_eq!(
        completed,
        BTreeSet::from(["a".into(), "b".into(), "c".into(), "d".into()])
    );
}
