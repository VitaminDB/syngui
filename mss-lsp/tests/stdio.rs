//! Сквозная проверка: сервер по stdio отвечает на основные запросы.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};

use serde_json::{json, Value};

struct Client {
    child: std::process::Child,
    out: BufReader<std::process::ChildStdout>,
}

impl Client {
    fn send(&mut self, v: Value) {
        let body = v.to_string();
        let stdin = self.child.stdin.as_mut().unwrap();
        write!(stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        stdin.flush().unwrap();
    }

    fn recv(&mut self) -> Value {
        let mut len = 0;
        loop {
            let mut line = String::new();
            self.out.read_line(&mut line).unwrap();
            let line = line.trim();
            if line.is_empty() {
                break;
            }
            if let Some(n) = line.strip_prefix("Content-Length: ") {
                len = n.parse().unwrap();
            }
        }
        let mut buf = vec![0; len];
        self.out.read_exact(&mut buf).unwrap();
        serde_json::from_slice(&buf).unwrap()
    }

    /// Ответ на запрос `id` (уведомления по пути пропускаются, кроме
    /// диагностик — они складываются в `diags`).
    fn response(&mut self, id: i64, diags: &mut Vec<Value>) -> Value {
        loop {
            let m = self.recv();
            if m["id"] == json!(id) {
                return m["result"].clone();
            }
            if m["method"] == "textDocument/publishDiagnostics" {
                diags.push(m["params"].clone());
            }
        }
    }
}

#[test]
fn lsp_roundtrip() {
    let dir = std::env::temp_dir().join(format!("mss-lsp-stdio-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("theme.mss"), ":root { --bg: #102030; }").unwrap();
    std::fs::write(dir.join("mss-context.json"), r#"{"classes":["панель"]}"#).unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_mss-lsp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let out = BufReader::new(child.stdout.take().unwrap());
    let mut c = Client { child, out };
    let root = format!("file://{}", dir.display()).replace(' ', "%20");
    let uri = format!("{root}/app.mss");
    let mut diags = Vec::new();

    c.send(
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "capabilities":{"general":{"positionEncodings":["utf-16"]}},
        "rootUri": root, "workspaceFolders": [{"uri": root, "name": "t"}]}}),
    );
    let init = c.response(1, &mut diags);
    assert_eq!(init["capabilities"]["positionEncoding"], "utf-16");
    c.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));

    // «ё» — два байта и одна единица UTF-16; проверяет пересчёт колонок.
    let text = "/* ё */ .a { colr: var(--bg); color: var(--nope); }\n.";
    c.send(
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"mss","version":1,"text":text}}}),
    );

    c.send(
        json!({"jsonrpc":"2.0","id":2,"method":"textDocument/hover","params":{
        "textDocument":{"uri":uri},"position":{"line":0,"character":25}}}),
    );
    let hover = c.response(2, &mut diags);
    let md = hover["contents"]["value"].as_str().unwrap();
    assert!(md.contains("#102030"), "{md}");
    assert_eq!(hover["range"]["start"]["character"], 23);

    let d = diags.last().expect("диагностики после didOpen");
    let codes: Vec<_> = d["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["code"].as_str().unwrap().to_string())
        .collect();
    assert!(codes.contains(&"unknown-property".into()), "{codes:?}");
    assert!(codes.contains(&"unknown-variable".into()), "{codes:?}");
    assert!(!d["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x["message"].as_str().unwrap().contains("--bg")));

    c.send(
        json!({"jsonrpc":"2.0","id":3,"method":"textDocument/completion","params":{
        "textDocument":{"uri":uri},"position":{"line":1,"character":1}}}),
    );
    let comp = c.response(3, &mut diags);
    let labels: Vec<_> = comp
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["label"].as_str().unwrap().to_string())
        .collect();
    assert!(labels.contains(&"панель".into()), "{labels:?}");

    c.send(json!({"jsonrpc":"2.0","id":4,"method":"textDocument/documentColor","params":{"textDocument":{"uri":uri}}}));
    assert_eq!(c.response(4, &mut diags), json!([]));

    c.send(
        json!({"jsonrpc":"2.0","id":5,"method":"textDocument/formatting","params":{
        "textDocument":{"uri":uri},"options":{"tabSize":4,"insertSpaces":true}}}),
    );
    assert_eq!(
        c.response(5, &mut diags),
        json!([]),
        "документ с ошибкой не форматируется"
    );

    c.send(json!({"jsonrpc":"2.0","id":6,"method":"shutdown"}));
    c.response(6, &mut diags);
    c.send(json!({"jsonrpc":"2.0","method":"exit"}));
    assert!(c.child.wait().unwrap().success());
    std::fs::remove_dir_all(&dir).unwrap();
}
