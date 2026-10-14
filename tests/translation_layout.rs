//! PDF -> recovered layout -> model stub -> durable translation, without paid AI.
use easyread_gpui::{
    ai::{self, TranslationOptions, Transport},
    store::Library,
    translation_layout,
};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[allow(dead_code)]
#[path = "support/math.rs"]
mod math;
#[allow(dead_code)]
mod support;

struct Model {
    broken: bool,
    calls: AtomicUsize,
}
impl Transport for Model {
    fn model(&self) -> &str {
        "structured-translation-fixture"
    }
    fn request(
        &self,
        messages: Value,
        _: &AtomicBool,
        _: &mut dyn FnMut(&str),
    ) -> anyhow::Result<String> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let blocks: Vec<Value> = serde_json::from_str(messages[1]["content"].as_str().unwrap())?;
        let mut output = serde_json::Map::new();
        for block in blocks {
            assert!(
                block["layout"].is_array(),
                "Native PDF translation must carry its layout"
            );
            assert!(
                block["layout"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|u| u["kind"] != "equation"),
                "Standalone equations must not be sent as translation units"
            );
            let text = block["text"].as_str().unwrap();
            assert!(text.contains("[[VELLIO_UNIT_0]]"));
            assert!(
                !text.contains("data:image"),
                "Do not send raster bytes to a text model"
            );
            let mut translated = text
                .replace("Building Code Agents", "构建代码智能体")
                .replace(
                    "Code Agents to Serve Non-Coders",
                    "服务非程序员的代码智能体",
                )
                .replace("Previous paragraph.", "前面的段落。")
                .replace("Body before the formula.", "公式前面的正文。")
                .replace("Body after the formula.", "公式后面的正文。")
                .replace("Figure 2:", "图 2：");
            if self.broken {
                translated = translated.replace("[[VELLIO_UNIT_0]]", "");
            }
            output.insert(block["id"].as_str().unwrap().to_owned(), json!(translated));
        }
        Ok(Value::Object(output).to_string())
    }
}
#[test]
fn native_pdf_translation_preserves_recovered_headings_and_reader_data() {
    let temp = tempfile::tempdir().unwrap();
    let pdf = temp.path().join("heading.pdf");
    support::heading_pdf_fixture(&pdf);
    let ws = Library::open(temp.path().join("library"))
        .unwrap()
        .import_pdf(&pdf, None)
        .unwrap();
    ws.reader_ops(&[
        json!({"op":"edit","block":"pdf-p001","zh":"用户版本","base":"base","at":"2030"}),
    ])
    .unwrap();
    let reader = std::fs::read(ws.root.join("reader.json")).unwrap();
    let original = ws.load("paper").unwrap();
    let model = Model {
        broken: false,
        calls: AtomicUsize::new(0),
    };
    ai::translate_pages_batched(
        &ws,
        &model,
        TranslationOptions::default(),
        &[1],
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    let paper = ws.load("paper").unwrap();
    let block = &paper["blocks"][0];
    let zh = block["zh"].as_str().unwrap();
    assert!(zh.contains("## 构建代码智能体"), "{zh}");
    assert!(zh.contains("### 服务非程序员的代码智能体"), "{zh}");
    let html = translation_layout::saved_html(block, zh).unwrap();
    assert!(html.contains("<h2>构建代码智能体</h2>"));
    assert!(html.contains("<h3>服务非程序员的代码智能体</h3>"));
    assert_eq!(block["en"], original["blocks"][0]["en"]);
    assert_eq!(block["id"], original["blocks"][0]["id"]);
    assert_eq!(std::fs::read(ws.root.join("reader.json")).unwrap(), reader);
    assert_eq!(paper["blocks"][1], original["blocks"][1]);
    assert_eq!(model.calls.load(Ordering::Relaxed), 1);
}
#[test]
fn native_pdf_translation_preserves_exact_tex_and_equation_block() {
    let temp = tempfile::tempdir().unwrap();
    let pdf = temp.path().join("math.pdf");
    math::fixture(&pdf, "display");
    let ws = Library::open(temp.path().join("library"))
        .unwrap()
        .import_pdf(&pdf, None)
        .unwrap();
    let original = ws.load("paper").unwrap();
    let recovered = easyread_gpui::pdf::code_presentation(
        &pdf,
        1,
        original["blocks"][0]["en"].as_str().unwrap(),
    )
    .unwrap()
    .unwrap();
    let tex = &recovered.math.iter().find(|m| m.display).unwrap().tex;
    let model = Model {
        broken: false,
        calls: AtomicUsize::new(0),
    };
    // The older public entry point must also use the structured native path.
    ai::translate(&ws, &model, &AtomicBool::new(false), |_, _| {}).unwrap();
    let paper = ws.load("paper").unwrap();
    let block = &paper["blocks"][0];
    let zh = block["zh"].as_str().unwrap();
    assert!(zh.contains(&format!("$${tex}$$")), "{zh}");
    assert!(zh.contains("公式前面的正文。\n\n"));
    assert!(zh.contains("公式后面的正文。"));
    let html = translation_layout::saved_html(block, zh).unwrap();
    assert!(html.contains("data-display=\"true\""));
    assert!(html.contains(tex));
    assert_eq!(block["en"], original["blocks"][0]["en"]);
}
#[test]
fn formula_only_pdf_is_preserved_without_calling_the_model() {
    let temp = tempfile::tempdir().unwrap();
    let pdf = temp.path().join("only-math.pdf");
    math::fixture(&pdf, "only");
    let ws = Library::open(temp.path().join("library"))
        .unwrap()
        .import_pdf(&pdf, None)
        .unwrap();
    let original = ws.load("paper").unwrap();
    let reader = ws.load("reader").unwrap();
    let model = Model {
        broken: false,
        calls: AtomicUsize::new(0),
    };
    ai::translate_batched(
        &ws,
        &model,
        TranslationOptions::default(),
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    assert_eq!(model.calls.load(Ordering::Relaxed), 0);
    let paper = ws.load("paper").unwrap();
    let block = &paper["blocks"][0];
    let html = translation_layout::saved_html(block, block["zh"].as_str().unwrap()).unwrap();
    assert_eq!(html.matches("data-display=\"true\"").count(), 1);
    assert_eq!(html.matches("(1)").count(), 1);
    assert_eq!(block["en"], original["blocks"][0]["en"]);
    assert_eq!(block["id"], original["blocks"][0]["id"]);
    assert_eq!(ws.load("reader").unwrap(), reader);
}

#[test]
fn native_translation_keeps_shared_panels_before_their_caption() {
    let temp = tempfile::tempdir().unwrap();
    let pdf = temp.path().join("panels.pdf");
    support::shared_caption_panels_fixture(&pdf, "roman");
    let ws = Library::open(temp.path().join("library"))
        .unwrap()
        .import_pdf(&pdf, None)
        .unwrap();
    let model = Model {
        broken: false,
        calls: AtomicUsize::new(0),
    };
    ai::translate_batched(
        &ws,
        &model,
        TranslationOptions::default(),
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    let paper = ws.load("paper").unwrap();
    let block = &paper["blocks"][0];
    let html = translation_layout::saved_html(block, block["zh"].as_str().unwrap()).unwrap();
    assert!(html.contains("<img "));
    assert!(html.find("<img ") < html.find("图 2："));
    assert!(html.contains("text-align:center"));
    assert_eq!(
        block["native_translation_layout"]["inline_figures"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn mismatched_pdf_source_is_rejected_before_sending_a_model_request() {
    let temp = tempfile::tempdir().unwrap();
    let pdf = temp.path().join("heading.pdf");
    support::heading_pdf_fixture(&pdf);
    let ws = Library::open(temp.path().join("library"))
        .unwrap()
        .import_pdf(&pdf, None)
        .unwrap();
    ws.update("paper", |paper| {
        paper["blocks"][0]["en"] = json!("Different source text");
        Ok(())
    })
    .unwrap();
    let before = std::fs::read(ws.root.join("paper.json")).unwrap();
    let model = Model {
        broken: false,
        calls: AtomicUsize::new(0),
    };
    let error = ai::translate_pages_batched(
        &ws,
        &model,
        TranslationOptions::default(),
        &[1],
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("PDF source text no longer matches"));
    assert_eq!(model.calls.load(Ordering::Relaxed), 0);
    assert_eq!(std::fs::read(ws.root.join("paper.json")).unwrap(), before);
}

#[test]
fn losing_layout_tokens_is_a_failure_not_a_flattened_success() {
    let temp = tempfile::tempdir().unwrap();
    let pdf = temp.path().join("heading.pdf");
    support::heading_pdf_fixture(&pdf);
    let ws = Library::open(temp.path().join("library"))
        .unwrap()
        .import_pdf(&pdf, None)
        .unwrap();
    let before = std::fs::read(ws.root.join("paper.json")).unwrap();
    let model = Model {
        broken: true,
        calls: AtomicUsize::new(0),
    };
    let error = ai::translate_pages_batched(
        &ws,
        &model,
        TranslationOptions::default(),
        &[1],
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("document structure"));
    assert_eq!(std::fs::read(ws.root.join("paper.json")).unwrap(), before);
    assert_eq!(ws.load("job").unwrap()["state"], "partial");
    assert_eq!(ws.load("job").unwrap()["failed_pages"], json!([1]));
}
