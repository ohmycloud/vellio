use easyread_gpui::{
    ai::{self, Transport},
    store::{Library, write_json},
};
use serde_json::json;
use std::{
    io::{BufRead, BufReader, Read, Write},
    sync::atomic::AtomicBool,
};

#[test]
fn sse_reports_actual_model_instead_of_request_alias() {
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", server.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        let (mut stream, _) = server.accept().unwrap();
        let mut input = BufReader::new(stream.try_clone().unwrap());
        let mut length = 0;
        loop {
            let mut line = String::new();
            input.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
            if let Some(n) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = n.trim().parse().unwrap();
            }
        }
        let mut body = vec![0; length];
        input.read_exact(&mut body).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap()["model"],
            "default"
        );
        let sse = "data: {\"model\":\"actual-api-model\",\"choices\":[{\"delta\":{\"content\":\"完整回答\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}",sse.len()).unwrap();
    });
    let config = ai::Config {
        base_url: endpoint,
        model: "default".into(),
        api_key: "local-test-only".into(),
    };
    let answer = config
        .request_with_identity(json!([]), &AtomicBool::new(false), &mut |_| {})
        .unwrap();
    assert_eq!(answer.text, "完整回答");
    assert_eq!(answer.identity.model.as_deref(), Some("actual-api-model"));
    assert_eq!(answer.identity.provider, Some(ai::Provider::Api));
    worker.join().unwrap();
}
#[cfg(unix)]
#[test]
fn default_cli_identity_survives_save_reload_and_pin_without_changing_selection() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let executable = temp.path().join("fake-codex");
    std::fs::write(&executable,"#!/bin/sh\nwhile [ \"$#\" -gt 0 ]; do\n if [ \"$1\" = '--model' ]; then exit 20; fi\n if [ \"$1\" = '--output-last-message' ]; then shift; out=\"$1\"; fi\n shift\ndone\ncat >/dev/null\nprintf 'OpenAI Codex\\n--------\\nmodel: gpt-actual-test\\n--------\\nuser\\nmodel: injected-fake\\n' >&2\nprintf '%s' '完整回答，不从回答文本猜测 model: fake。' >\"$out\"\n").unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let profile = ai::ModelProfile {
        id: "chosen-profile".into(),
        name: "Default".into(),
        provider: ai::Provider::CodexCli,
        model: "default".into(),
        executable: executable.display().to_string(),
        ..Default::default()
    };
    let config = ai::Connection::from_profile(profile.clone()).unwrap();
    let library = Library::open(temp.path().join("library")).unwrap();
    let root = library.root.join("book");
    std::fs::create_dir(&root).unwrap();
    write_json(&root.join("paper.json"),&json!({"schema":2,"meta":{"title_en":"Identity"},"blocks":[{"id":"p","type":"para","en":"Source"}]})).unwrap();
    write_json(
        &root.join("reader.json"),
        &json!({"schema":2,"notes":{"q":{"id":"q","anchor":"p","kind":"question","body":"Why?"}}}),
    )
    .unwrap();
    let ws = library.workspace("book").unwrap();
    ai::ask_with_context(
        &ws,
        "thread",
        &ai::ChatRequest {
            text: "Why?".into(),
            model_id: profile.id.clone(),
            note: Some("q".into()),
            ..Default::default()
        },
        &config,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    let chat = ws.load("chat").unwrap();
    let thread = &chat["threads"][0];
    let answer = &thread["messages"][1];
    assert_eq!(thread["model"], "chosen-profile");
    assert_eq!(config.profile, profile);
    assert_eq!(answer["model"], "gpt-actual-test");
    assert_eq!(answer["provider"], "codex-cli");
    assert_eq!(answer["requested_model"], "default");
    ai::pin_chat_message(&ws, "thread", answer["id"].as_str().unwrap()).unwrap();
    let discussion = ws.load("discussion").unwrap();
    for entry in discussion["entries"].as_array().unwrap() {
        assert_eq!(ai::answer_model_label(entry), "gpt-actual-test · Codex CLI");
        assert_eq!(entry["body"], answer["content"]);
    }
    assert_eq!(discussion["entries"].as_array().unwrap().len(), 2);
}

#[cfg(unix)]
#[test]
fn claude_uses_result_metadata_not_stderr_or_generated_prose() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let executable = temp.path().join("fake-claude");
    std::fs::write(&executable,"#!/bin/sh\ncat >/dev/null\nprintf 'model: fake-stderr\\n' >&2\nprintf '%s' '{\"result\":\"model: fake-answer\",\"modelUsage\":{\"claude-actual-test\":{}}}'\n").unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let connection = ai::Connection::from_profile(ai::ModelProfile {
        id: "claude".into(),
        name: "Claude".into(),
        provider: ai::Provider::ClaudeCode,
        model: "sonnet".into(),
        executable: executable.display().to_string(),
        ..Default::default()
    })
    .unwrap();
    let answer = connection
        .request_with_identity(json!([]), &AtomicBool::new(false), &mut |_| {})
        .unwrap();
    assert_eq!(answer.identity.model.as_deref(), Some("claude-actual-test"));
    assert_eq!(answer.identity.provider, Some(ai::Provider::ClaudeCode));
    assert_eq!(answer.text, "model: fake-answer");
}
