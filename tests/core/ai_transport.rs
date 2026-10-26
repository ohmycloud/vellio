use super::*;

#[test]
fn sse_unicode_multiline_done_error_and_cancel() {
    let input = "data: {\"choices\":[{\"delta\":{\"content\":\"中文🦀\"}}]}\r\n\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"继续\"},\n data-ignored\ndata: \"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
    let mut deltas = String::new();
    assert_eq!(
        ai::decode_sse(input.as_bytes(), &AtomicBool::new(false), |s| deltas
            .push_str(s))
        .unwrap(),
        "中文🦀继续"
    );
    assert_eq!(deltas, "中文🦀继续");
    assert!(
        ai::decode_sse(
            b"data: {\"error\":{}}\n\n".as_slice(),
            &AtomicBool::new(false),
            |_| {}
        )
        .is_err()
    );
    assert!(
        ai::decode_sse(
            b"data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n".as_slice(),
            &AtomicBool::new(false),
            |_| {}
        )
        .is_err()
    );
    assert!(
        ai::decode_sse(
            b"data: [DONE]\n\n".as_slice(),
            &AtomicBool::new(true),
            |_| {}
        )
        .is_err()
    );
}

#[test]
fn actual_http_ai_transport_and_thread_persistence() {
    let temp = tempfile::tempdir().unwrap();
    let pdf = temp.path().join("test.pdf");
    pdf_fixture(&pdf, 0, false);
    let ws = Library::open(temp.path().join("lib"))
        .unwrap()
        .import_pdf(&pdf, None)
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        let mut reader = BufReader::new(socket.try_clone().unwrap());
        let mut line = String::new();
        let mut length = 0;
        loop {
            line.clear();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
            if line.to_ascii_lowercase().starts_with("content-length:") {
                length = line
                    .split(':')
                    .nth(1)
                    .unwrap()
                    .trim()
                    .parse::<usize>()
                    .unwrap();
            }
        }
        let mut body = vec![0; length];
        std::io::Read::read_exact(&mut reader, &mut body).unwrap();
        let request: Value = serde_json::from_slice(&body).unwrap();
        assert!(request["stream"].as_bool().unwrap());
        assert!(
            request["messages"][0]["content"]
                .as_str()
                .unwrap()
                .contains("Rust PDF integration")
        );
        let data =
            "data: {\"choices\":[{\"delta\":{\"content\":\"验证回答🦀\"}}]}\n\ndata: [DONE]\n\n";
        write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",data.len(),data).unwrap();
    });
    let config = ai::Config {
        base_url: format!("http://127.0.0.1:{port}/v1"),
        model: "explicit-mock".into(),
        api_key: "test-only".into(),
    };
    let answer = ai::ask(
        &ws,
        "t-test",
        "解释论文",
        Some("pdf-p001"),
        &config,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    server.join().unwrap();
    assert_eq!(answer, "验证回答🦀");
    let chat = ws.load("chat").unwrap();
    assert_eq!(chat["threads"][0]["messages"].as_array().unwrap().len(), 2);
    assert_eq!(chat["threads"][0]["messages"][1]["status"], "done");
}

#[test]
fn translation_partial_failure_retry_preserves_ids_and_user_edits() {
    let temp = tempfile::tempdir().unwrap();
    let pdf = temp.path().join("test.pdf");
    pdf_fixture(&pdf, 0, false);
    let ws = Library::open(temp.path().join("lib"))
        .unwrap()
        .import_pdf(&pdf, None)
        .unwrap();
    ws.update("paper",|p|{p["blocks"]=json!([{"id":"a","type":"para","page":1,"en":"First","zh":""},{"id":"b","type":"para","page":1,"en":"Second","zh":""}]);Ok(())}).unwrap();
    ws.reader_ops(&[json!({"op":"edit","block":"a","zh":"用户版本","base":"base","at":"2030"})])
        .unwrap();
    let reader = ws.load("reader").unwrap();
    let (config, server) = mock_responses(vec![
        (
            200,
            "data: {\"choices\":[{\"delta\":{\"content\":\"第一段\"}}]}\n\ndata: [DONE]\n\n",
        ),
        (500, "deliberate error"),
    ]);
    assert!(ai::translate(&ws, &config, &AtomicBool::new(false), |_, _| {}).is_err());
    server.join().unwrap();
    assert_eq!(ws.load("paper").unwrap()["blocks"][0]["zh"], "第一段");
    assert_eq!(ws.load("job").unwrap()["state"], "partial");
    assert_eq!(ws.load("reader").unwrap(), reader);
    let (config, server) = mock_responses(vec![(
        200,
        "data: {\"choices\":[{\"delta\":{\"content\":\"第二段\"}}]}\n\ndata: [DONE]\n\n",
    )]);
    ai::translate(&ws, &config, &AtomicBool::new(false), |_, _| {}).unwrap();
    server.join().unwrap();
    assert_eq!(ws.load("paper").unwrap()["blocks"][1]["id"], "b");
    assert_eq!(ws.load("paper").unwrap()["blocks"][1]["zh"], "第二段");
    assert_eq!(ws.load("reader").unwrap(), reader);
    assert_eq!(ws.load("job").unwrap()["state"], "done");
}

#[test]
fn incomplete_ai_stream_preserves_partial_answer_with_error_status() {
    let temp = tempfile::tempdir().unwrap();
    let pdf = temp.path().join("test.pdf");
    pdf_fixture(&pdf, 0, false);
    let ws = Library::open(temp.path().join("lib"))
        .unwrap()
        .import_pdf(&pdf, None)
        .unwrap();
    let (config, server) = mock_responses(vec![(
        200,
        "data: {\"choices\":[{\"delta\":{\"content\":\"部分回答\"}}]}\n\n",
    )]);
    assert!(
        ai::ask(
            &ws,
            "t-partial",
            "问题",
            None,
            &config,
            &AtomicBool::new(false),
            |_| {}
        )
        .is_err()
    );
    server.join().unwrap();
    let answer = ws.load("chat").unwrap()["threads"][0]["messages"][1].clone();
    assert_eq!(answer["content"], "部分回答");
    assert_eq!(answer["status"], "error");
}
