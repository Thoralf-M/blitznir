//! A one-thread HTTP server for the client's tests: it answers each request
//! with the next scripted reply and records what it was asked.

use crate::config::Endpoint;
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

pub(super) enum Reply {
    /// 200 text/event-stream: these lines, then close.
    Sse(Vec<String>),
    /// 200 and these lines, then wait: reports whether the client hung up.
    Hold(Vec<String>),
    /// Any status with this body.
    Plain(u16, String),
}

pub(super) struct Mock {
    pub(super) ep: Endpoint,
    /// (request head, JSON body) per request, in order.
    pub(super) requests: Arc<Mutex<Vec<(String, Value)>>>,
    /// One answer per Hold reply: whether the client hung up.
    pub(super) hung_up: Receiver<bool>,
    pub(super) server: JoinHandle<()>,
}

fn read_request(stream: &mut TcpStream) -> (String, Value) {
    let mut r = BufReader::new(stream);
    let mut head = String::new();
    loop {
        let mut line = String::new();
        r.read_line(&mut line).unwrap();
        if line == "\r\n" || line.is_empty() {
            break;
        }
        head.push_str(&line);
    }
    let len = head.lines().find_map(|l| {
        l.to_lowercase()
            .strip_prefix("content-length:")
            .map(|n| n.trim().parse().unwrap())
    });
    let mut body = vec![0; len.unwrap_or(0)];
    r.read_exact(&mut body).unwrap();
    (head, serde_json::from_slice(&body).unwrap_or_default())
}

pub(super) fn mock(replies: Vec<Reply>, key: &str) -> Mock {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://127.0.0.1:{}/v1/",
        listener.local_addr().unwrap().port()
    );
    let requests = Arc::new(Mutex::new(vec![]));
    let (hung_tx, hung_up) = channel();
    let log = requests.clone();
    let server = std::thread::spawn(move || {
        for reply in replies {
            let (mut s, _) = listener.accept().unwrap();
            log.lock().unwrap().push(read_request(&mut s));
            let sse =
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n";
            match reply {
                Reply::Sse(lines) => write!(s, "{sse}{}", lines.join("\n")).unwrap(),
                Reply::Plain(code, body) => {
                    write!(s, "HTTP/1.1 {code} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                }
                Reply::Hold(lines) => {
                    write!(s, "{sse}{}", lines.join("\n")).unwrap();
                    s.flush().unwrap();
                    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                    hung_tx.send(matches!(s.read(&mut [0; 16]), Ok(0))).unwrap();
                }
            }
        }
    });
    let ep = Endpoint {
        name: "mock".into(),
        url,
        key: key.into(),
        models: vec![],
        vision: None,
    };
    Mock {
        ep,
        requests,
        hung_up,
        server,
    }
}

pub(super) fn dead_endpoint(name: &str) -> Endpoint {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    Endpoint {
        name: name.into(),
        url: format!("http://127.0.0.1:{port}/v1"),
        key: String::new(),
        models: vec![],
        vision: None,
    }
}
