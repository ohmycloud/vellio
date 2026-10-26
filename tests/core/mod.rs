//! End-to-end engine regressions, grouped by domain.
use super::*;

mod ai_transport;
mod formulas;
mod pdf_captions;
mod pdf_images;
mod pdf_math;
mod pdf_tables;
mod pdf_text;
mod storage;

fn mock_responses(responses: Vec<(u16, &'static str)>) -> (ai::Config, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = thread::spawn(move || {
        for (status, body) in responses {
            let (mut socket, _) = listener.accept().unwrap();
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
            let mut request = vec![0; length];
            std::io::Read::read_exact(&mut reader, &mut request).unwrap();
            write!(socket,"HTTP/1.1 {status} Test\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
    });
    (
        ai::Config {
            base_url: format!("http://127.0.0.1:{port}/v1"),
            model: "explicit-mock".into(),
            api_key: "test-only".into(),
        },
        handle,
    )
}
