//! Browser settings surface for the desktop library's model profiles. Secrets remain in env.
use super::*;
use easyread_gpui::ai::Transport;
use sha2::{Digest, Sha256};
use std::path::Path;

static SETTINGS_WRITE_LOCK: Mutex<()> = Mutex::new(());

#[query_params(error = bad_request)]
struct SettingsQuery {
    tab: Option<String>,
}

#[derive(Deserialize)]
struct ReadingInput {
    revision: String,
    font: String,
    size: f32,
    width: f32,
    leading: f32,
    margin: Option<String>,
    bilingual: Option<String>,
    ask_ai: Option<String>,
    edit: Option<String>,
    original: Option<String>,
    page: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct ModelAction {
    action: String,
    revision: String,
    id: String,
    name: String,
    provider: String,
    model: String,
    base_url: String,
    key_env: String,
    executable: String,
    reasoning: String,
    pages_per_batch: String,
    parallel: String,
    ask_above: String,
    concurrent_papers: String,
}

fn revision(path: &Path) -> String {
    format!("{:x}", Sha256::digest(fs::read(path).unwrap_or_default()))
}
fn validate_length(value: &str, max: usize) -> Result<()> {
    if value.len() > max {
        return Err(bad_request("模型字段过长").into());
    }
    Ok(())
}
fn merge_models(raw: &mut Value, profiles: &[ai::ModelProfile]) -> Result<()> {
    let old = raw["models"].as_array().cloned().unwrap_or_default();
    let mut models = vec![];
    for profile in profiles {
        let mut entry = old
            .iter()
            .find(|m| m["id"] == profile.id)
            .cloned()
            .unwrap_or_else(|| json!({}));
        let fields = serde_json::to_value(profile).map_err(|e| Error::from_anyhow(e.into()))?;
        for (key, value) in fields.as_object().expect("model profile object") {
            entry[key] = value.clone();
        }
        models.push(entry);
    }
    raw["models"] = json!(models);
    Ok(())
}

#[page("/settings")]
async fn settings_page(cx: &Cx) -> Result<impl View> {
    let tab = query_params::<SettingsQuery>(cx)?
        .tab
        .clone()
        .unwrap_or_else(|| "model".into());
    let root = desktop_library_path();
    let file = root.join(".settings.json");
    let prefs = model_preferences().map_err(Error::from_anyhow)?;
    let rev = revision(&file);
    let selected = prefs
        .models
        .iter()
        .find(|m| m.id == prefs.translation_model);
    let selected_name = selected
        .map(|m| m.name.clone())
        .unwrap_or_else(|| "Not configured".into());
    let selected_effort = selected.map(|m| m.reasoning.clone()).unwrap_or_default();
    let effort_levels = selected
        .map(|m| m.reasoning_levels().to_vec())
        .unwrap_or_default();
    let effort_fallback = selected.map(|m| m.reasoning_fallback()).unwrap_or("");
    let concurrent = prefs
        .extra
        .get("concurrent_papers")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .clamp(1, 4);
    let models = prefs.models.clone();
    let source = if file.is_file() {
        "Desktop library · changes saved to .settings.json"
    } else {
        "Environment fallback · no desktop .settings.json yet"
    };
    Ok(view! {
        <!DOCTYPE html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
            <title>"Settings · Vellio"</title><link rel="stylesheet" href="/settings.css"><script src="/settings.js" defer="defer"></script>
        </head><body class="settings-page"><div class="settings-shell">
            <header class="settings-header"><a href="/" class="settings-back" title="Back to library">"‹"</a><h1>"Settings"</h1>
                <nav aria-label="Settings tabs"><a class=(if tab == "model" {"active"}else{""}) href="/settings?tab=model">"Model"</a><a class=(if tab == "reading" {"active"}else{""}) href="/settings?tab=reading">"Reading"</a><a class=(if tab == "sidebar" {"active"}else{""}) href="/settings?tab=sidebar">"Sidebar"</a><a class=(if tab == "cloud" {"active"}else{""}) href="/settings?tab=cloud">"Cloud library"</a><a class=(if tab == "shortcuts" {"active"}else{""}) href="/settings?tab=shortcuts">"Keyboard shortcuts"</a></nav>
            </header>
            <main class="settings-content">
                if tab == "model" {
                    <div class="model-grid">
                    for model in models {
                        <article class=(format!("model-card{}",if model.id == prefs.translation_model || model.id == prefs.ask_model {" active"} else {""})) data-id=(model.id.clone()) data-name=(model.name.clone()) data-model=(model.model.clone()) data-provider=(serde_json::to_value(model.provider).unwrap_or(Value::Null).as_str().unwrap_or("api").to_owned()) data-base=(model.base_url.clone()) data-key-env=(model.key_env.clone()) data-executable=(model.executable.clone()) data-reasoning=(model.reasoning.clone())>
                            <form method="post" action="/settings/model" class="model-select"><input type="hidden" name="revision" value=(rev.clone())><input type="hidden" name="id" value=(model.id.clone())><button type="submit" name="action" value="translation" title="Use for translation"><strong>(model.name.clone())</strong><span>(model.provider.label())</span></button></form>
                            <details class="model-more"><summary aria-label=(format!("Manage {}",model.name))>"···"</summary><div class="model-actions"><button type="button" data-edit-model=(model.id.clone())>"Edit connection"</button>
                                <form method="post" action="/settings/model"><input type="hidden" name="revision" value=(rev.clone())><input type="hidden" name="id" value=(model.id.clone())><button name="action" value="ask" type="submit">"Use for Ask AI"</button></form>
                                <form method="post" action="/settings/model"><input type="hidden" name="revision" value=(rev.clone())><input type="hidden" name="id" value=(model.id.clone())><button name="action" value="remove" type="submit" data-confirm-remove=(model.name.clone())>"Remove"</button></form>
                                <small class="model-readiness">(if model.provider == ai::Provider::Api { if ai::Connection::from_profile(model.clone()).is_ok() { "Key present · connection not tested" }else{ "API key missing or profile invalid" } } else if ai::cli_executable(model.provider,&model.executable).is_ok() { "CLI found · login not verified" }else{ "CLI not installed" })</small>
                            </div></details>
                            <div class="model-badges">if model.id == prefs.ask_model { <span>"Ask AI default"</span> }if model.id == prefs.translation_model { <span>"Translation"</span> }</div>
                        </article>
                    }
                    <button type="button" class="model-add" id="add-model"><span>"+"</span><strong>"Add model"</strong></button>
                    </div>
                    <div class="settings-divider"></div><h2 class="settings-caption">"Translation: "(selected_name)</h2>
                    <p class="settings-subcaption">"Reasoning effort: "(if selected_effort.is_empty() { if effort_fallback.is_empty() { "Default (provider dependent)".to_owned() } else { format!("Default ({effort_fallback})") } } else { selected_effort.clone() })" · Fast: not supported by this Vellio core"</p>
                    <form id="translation-options" method="post" action="/settings/model"><input type="hidden" name="action" value="options"><input type="hidden" name="revision" value=(rev.clone())>
                        <details class="settings-effort"><summary>"Change translation reasoning effort"</summary><label>"Reasoning effort"<select name="reasoning" aria-label="Reasoning effort"><option value="" selected=(selected_effort.is_empty())>"Default"</option>
                            for effort in effort_levels { <option value=(effort) selected=(selected_effort == effort)>(effort)</option> }
                        </select></label></details>
                        <div class="settings-options"><label>"Pages per batch"<select name="pages_per_batch" aria-label="Pages per batch">for n in 1..=10 { <option value=(n.to_string()) selected=(prefs.pages_per_batch == n)>(format!("{n} pages"))</option> }</select></label>
                            <label>"Parallel segments per paper"<select name="parallel" aria-label="Parallel segments"><option value="0" selected=(prefs.parallel == 0)>"Auto"</option>for n in 1..=8 { <option value=(n.to_string()) selected=(prefs.parallel == n)>(n)</option> }</select></label>
                            <label>"Concurrent papers"<select name="concurrent_papers" aria-label="Concurrent papers">for n in 1..=4 { <option value=(n.to_string()) selected=(concurrent == n)>(if n == 1 { "1 paper (default, sequential)".to_owned() } else { format!("{n} papers") })</option> }</select></label>
                            <label>"Ask first if a paper has more than"<select name="ask_above" aria-label="Ask first if a paper has more than">for n in [0,20,30,40,60,100,200,500] { <option value=(n.to_string()) selected=(prefs.ask_above == n)>(if n == 0 { "Always ask".into() }else{ format!("{n} pages") })</option> }</select></label>
                        </div><p class="settings-caution">"Running multiple papers multiplies each paper's segment concurrency, increases memory use, and may trigger model rate limits; keep the default of 1 paper when possible (maximum 4). This Web limit takes effect on the next job. The desktop build in this repository does not yet enforce concurrent papers or a Fast toggle."</p>
                        <div class="settings-footer"><button type="submit" class="settings-primary">"Save translation settings"</button></div>
                    </form>
                    <form method="post" action="/settings/test" class="settings-test"><button type="submit">"✳  Test with one sentence"</button></form>
                    <p class="settings-source">(source)" · "(file.display().to_string())</p>
                } else if tab == "reading" { <h2>"Reading"</h2><p class="settings-source">"Changes are saved to the desktop settings and become the Web reader's default when a book has no local Aa override. Existing browser-specific Aa settings still take precedence."</p>
                    <form id="reader-settings-form" method="post" action="/settings/reading"><input type="hidden" name="revision" value=(rev.clone())>
                        <div class="settings-options">
                            <label>"Font"<select name="font"><option value="noto" selected=(prefs.appearance.reading_font()==easyread_gpui::session::ReadingFont::NotoSerifSc)>"宋体"</option><option value="system" selected=(prefs.appearance.reading_font()==easyread_gpui::session::ReadingFont::SystemSerif)>"系统衬线"</option><option value="sans" selected=(prefs.appearance.reading_font()==easyread_gpui::session::ReadingFont::SansSerif)>"无衬线"</option></select></label>
                            <label>"Font size"<select name="size">for n in 14..=32 { <option value=(n.to_string()) selected=(prefs.appearance.font_size==n as f32)>(format!("{n} px"))</option> }</select></label>
                            <label>"Line width"<select name="width">for n in 25..=60 { <option value=(n.to_string()) selected=(prefs.appearance.line_width==n as f32)>(format!("{n} chars"))</option> }</select></label>
                            <label>"Line spacing"<select name="leading">for n in 12..=26 { <option value=(format!("{:.1}",n as f32/10.)) selected=((prefs.appearance.line_spacing*10.).round() == n as f32)>(format!("{:.1}",n as f32/10.))</option> }</select></label>
                        </div>
                        <div class="settings-checks"><label><input type="checkbox" name="margin" value="yes" checked=(prefs.appearance.margin_notes)>"Show margin notes"</label><label><input type="checkbox" name="bilingual" value="yes" checked=(prefs.bilingual)>"Open bilingual by default (desktop)"</label><label><input type="checkbox" name="ask_ai" value="yes" checked=(prefs.features.ask_ai)>"Ask AI"</label><label><input type="checkbox" name="edit" value="yes" checked=(prefs.features.edit)>"Edit translations"</label><label><input type="checkbox" name="original" value="yes" checked=(prefs.features.original)>"Show original"</label><label><input type="checkbox" name="page" value="yes" checked=(prefs.features.page)>"Original page"</label></div>
                        <div class="settings-footer"><button type="submit" class="settings-primary">"Save reading settings"</button></div>
                    </form>
                }
                else if tab == "sidebar" { <h2>"Sidebar"</h2><p>"Use the desktop app to pin or hide built-in folders. Web folder creation and drag-to-file are available in the library."</p> }
                else if tab == "cloud" { <h2>"Cloud library"</h2><p>"Cloud library relocation is desktop-only. This local Web service does not move or publish the desktop library."</p> }
                else if tab == "shortcuts" { <h2>"Keyboard shortcuts"</h2><p>"The desktop app owns keyboard shortcuts; this browser reader has a different event system. Edit shortcuts in the desktop app."</p> }
                else { <p>"Unknown settings tab."</p> }
            </main>
        </div>
        <dialog id="model-dialog" class="model-dialog"><form method="post" action="/settings/model" id="model-editor"><input type="hidden" name="revision" value=(rev.clone())><input type="hidden" name="action" value="save"><input type="hidden" name="id" value=""><header><h2>"Model connection"</h2><button type="button" data-close-model="true" aria-label="Close">"×"</button></header>
            <label>"Display name"<input name="name" maxlength="80" required=(true)></label><label>"Provider"<select name="provider"><option value="api">"API (OpenAI compatible)"</option><option value="claude-code">"Claude Code"</option><option value="codex-cli">"Codex CLI"</option></select></label>
            <label>"Model ID or CLI alias"<input name="model" maxlength="160" required=(true)></label>
            <div data-api-fields="true"><label>"API base URL"<input name="base_url" type="url" maxlength="320" placeholder="https://api.openai.com/v1"></label><label>"API key environment variable"<input name="key_env" maxlength="80" placeholder="EASYREAD_API_KEY"></label></div>
            <div data-cli-fields="true"><label>"CLI executable (optional)"<input name="executable" maxlength="320" placeholder="Auto-detect"></label></div>
            <label>"Reasoning effort"<select name="reasoning"><option value="">"Default"</option>for effort in ["low","medium","high","xhigh","max"] { <option value=(effort)>(effort)</option> }</select></label>
            <p class="settings-source">"API keys are never stored in this file. Export the named variable before starting desktop/Web."</p>
            <footer><button type="button" data-close-model="true">"Cancel"</button><button type="submit" class="settings-primary">"Save model"</button></footer>
        </form></dialog>
        </body></html>
    })
}

#[route(POST "/settings/model")]
async fn save_model(Form(input): Form<ModelAction>) -> Result<SeeOther> {
    let root = desktop_library_path();
    if !root.is_dir() {
        return Err(bad_request("桌面书库不存在；先检查首页显示的设置文件路径").into());
    }
    let file = root.join(".settings.json");
    let _lock = SETTINGS_WRITE_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    if revision(&file) != input.revision {
        return Err(bad_request("桌面设置已更改，请刷新页面后重试").into());
    }
    let mut raw = easyread_gpui::store::read_json(&file, json!({})).map_err(Error::from_anyhow)?;
    let mut prefs = model_preferences().map_err(Error::from_anyhow)?;
    match input.action.as_str() {
        "translation" | "ask" | "remove" => {
            if !prefs.models.iter().any(|m| m.id == input.id) {
                return Err(bad_request("模型已不存在").into());
            }
            match input.action.as_str() {
                "translation" => prefs.translation_model = input.id,
                "ask" => prefs.ask_model = input.id,
                _ => {
                    prefs.models.retain(|m| m.id != input.id);
                    let next = prefs
                        .models
                        .first()
                        .map(|m| m.id.clone())
                        .unwrap_or_default();
                    if prefs.translation_model == input.id {
                        prefs.translation_model = next.clone();
                    }
                    if prefs.ask_model == input.id {
                        prefs.ask_model = next;
                    }
                }
            }
        }
        "save" => {
            for field in [
                &input.name,
                &input.model,
                &input.base_url,
                &input.key_env,
                &input.executable,
            ] {
                validate_length(field, 320)?;
            }
            let provider = match input.provider.as_str() {
                "api" => ai::Provider::Api,
                "claude-code" => ai::Provider::ClaudeCode,
                "codex-cli" => ai::Provider::CodexCli,
                _ => return Err(bad_request("未知模型提供商").into()),
            };
            let existing = prefs.models.iter().position(|m| m.id == input.id);
            if !input.id.is_empty() && existing.is_none() {
                return Err(bad_request("模型已不存在").into());
            }
            let mut profile = existing
                .map(|i| prefs.models[i].clone())
                .unwrap_or_default();
            profile.id = if input.id.is_empty() {
                format!("m{}", uuid::Uuid::new_v4().simple())
            } else {
                input.id
            };
            profile.name = input.name.trim().into();
            profile.provider = provider;
            profile.model = input.model.trim().into();
            profile.base_url = input.base_url.trim().into();
            profile.key_env = input.key_env.trim().into();
            profile.executable = input.executable.trim().into();
            profile.reasoning = input.reasoning;
            profile.validate().map_err(|_| {
                bad_request("模型档案无效：检查名称、模型 ID、URL、密钥变量名和推理力度")
            })?;
            if let Some(i) = existing {
                prefs.models[i] = profile;
            } else {
                if prefs.models.len() >= 40 {
                    return Err(bad_request("模型数量超过上限").into());
                }
                if prefs.models.is_empty() {
                    prefs.translation_model = profile.id.clone();
                    prefs.ask_model = profile.id.clone();
                }
                prefs.models.push(profile);
            }
        }
        "options" => {
            prefs.pages_per_batch = input
                .pages_per_batch
                .parse()
                .map_err(|_| bad_request("无效批量页数"))?;
            prefs.parallel = input
                .parallel
                .parse()
                .map_err(|_| bad_request("无效并行数"))?;
            prefs.ask_above = input
                .ask_above
                .parse()
                .map_err(|_| bad_request("无效费用阈值"))?;
            let concurrent: usize = input
                .concurrent_papers
                .parse()
                .map_err(|_| bad_request("无效文献并发数"))?;
            if !(1..=10).contains(&prefs.pages_per_batch)
                || prefs.parallel > 8
                || ![0, 20, 30, 40, 60, 100, 200, 500].contains(&prefs.ask_above)
                || !(1..=4).contains(&concurrent)
            {
                return Err(bad_request("翻译参数超出范围").into());
            }
            prefs
                .extra
                .insert("concurrent_papers".into(), json!(concurrent));
            if let Some(model) = prefs
                .models
                .iter_mut()
                .find(|m| m.id == prefs.translation_model)
            {
                model.reasoning = input.reasoning;
                model
                    .validate()
                    .map_err(|_| bad_request("当前模型不支持该推理力度"))?;
            } else if !input.reasoning.is_empty() {
                return Err(bad_request("请先选择翻译模型").into());
            }
        }
        _ => return Err(bad_request("无效设置操作").into()),
    }
    prefs
        .validate()
        .map_err(|_| bad_request("桌面设置验证失败，请检查模型档案或快捷键冲突"))?;
    merge_models(&mut raw, &prefs.models)?;
    raw["translation_model"] = json!(prefs.translation_model);
    raw["ask_model"] = json!(prefs.ask_model);
    raw["pages_per_batch"] = json!(prefs.pages_per_batch);
    raw["parallel"] = json!(prefs.parallel);
    raw["ask_above"] = json!(prefs.ask_above);
    if let Some(n) = prefs.extra.get("concurrent_papers") {
        raw["concurrent_papers"] = n.clone();
    }
    easyread_gpui::store::write_json(&file, &raw).map_err(Error::from_anyhow)?;
    Ok(see_other("/settings?tab=model"))
}

#[route(POST "/settings/reading")]
async fn save_reading(Form(input): Form<ReadingInput>) -> Result<SeeOther> {
    let root = desktop_library_path();
    if !root.is_dir() {
        return Err(bad_request("桌面书库不存在").into());
    }
    let file = root.join(".settings.json");
    let _lock = SETTINGS_WRITE_LOCK
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    if revision(&file) != input.revision {
        return Err(bad_request("桌面设置已更改，请刷新后重试").into());
    }
    let mut raw = easyread_gpui::store::read_json(&file, json!({})).map_err(Error::from_anyhow)?;
    let mut prefs = model_preferences().map_err(Error::from_anyhow)?;
    prefs.appearance.set_font(match input.font.as_str() {
        "noto" => easyread_gpui::session::ReadingFont::NotoSerifSc,
        "system" => easyread_gpui::session::ReadingFont::SystemSerif,
        "sans" => easyread_gpui::session::ReadingFont::SansSerif,
        _ => return Err(bad_request("无效字体").into()),
    });
    if !input.size.is_finite()
        || !(14. ..=32.).contains(&input.size)
        || !input.width.is_finite()
        || !(25. ..=60.).contains(&input.width)
        || !input.leading.is_finite()
        || !(1.2..=2.6).contains(&input.leading)
    {
        return Err(bad_request("阅读排版数值超出范围").into());
    }
    prefs.appearance.font_size = input.size;
    prefs.appearance.line_width = input.width;
    prefs.appearance.line_spacing = input.leading;
    prefs.appearance.margin_notes = input.margin.is_some();
    prefs.bilingual = input.bilingual.is_some();
    prefs.features.ask_ai = input.ask_ai.is_some();
    prefs.features.edit = input.edit.is_some();
    prefs.features.original = input.original.is_some();
    prefs.features.page = input.page.is_some();
    prefs
        .validate()
        .map_err(|_| bad_request("阅读设置验证失败"))?;
    let appearance =
        serde_json::to_value(&prefs.appearance).map_err(|e| Error::from_anyhow(e.into()))?;
    for (key, value) in appearance.as_object().expect("appearance object") {
        raw["appearance"][key] = value.clone();
    }
    for (name, on) in [
        ("ask_ai", prefs.features.ask_ai),
        ("edit", prefs.features.edit),
        ("original", prefs.features.original),
        ("page", prefs.features.page),
    ] {
        raw["features"][name] = json!(on);
    }
    raw["bilingual"] = json!(prefs.bilingual);
    easyread_gpui::store::write_json(&file, &raw).map_err(Error::from_anyhow)?;
    Ok(see_other("/settings?tab=reading"))
}

#[page(POST "/settings/test")]
async fn test_model() -> Result<impl View> {
    let connection = model_preferences()
        .map_err(Error::from_anyhow)?
        .connection(true)
        .map_err(|_| bad_request("翻译模型未就绪，请检查配置和密钥环境变量"))?;
    let result = tokio::task::spawn_blocking(move || {
        connection.request(json!([{"role":"system","content":"Translate the next sentence into Chinese. Only output the translation."},
            {"role":"user","content":"Reading makes ideas clearer."}]),&AtomicBool::new(false),&mut |_| {})
    }).await?;
    let (heading, answer) = match result {
        Ok(answer) => ("Test succeeded", answer),
        Err(error) => ("Test failed", format!("{error:#}")),
    };
    Ok(
        view! { <!DOCTYPE html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>"Model test · Vellio"</title><link rel="stylesheet" href="/settings.css"></head>
        <body class="settings-page"><main class="test-result"><a href="/settings?tab=model">"← Back to Model settings"</a><h1>(heading)</h1><p>"Reading makes ideas clearer."</p><pre>(answer)</pre></main></body></html> },
    )
}
