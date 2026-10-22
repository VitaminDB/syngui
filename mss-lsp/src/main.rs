//! `mss-lsp` — LSP-сервер языка стилей MSS (syngui) поверх `mss-analysis`.
//!
//! Запуск без аргументов — сервер на stdin/stdout. `mss-lsp --check
//! файлы…` — диагностики в терминал (код выхода 1, если есть ошибки).

mod context;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidChangeWatchedFiles, DidCloseTextDocument, DidOpenTextDocument,
    DidSaveTextDocument, Notification as _, PublishDiagnostics, ShowMessage,
};
use lsp_types::request::{
    ColorPresentationRequest, Completion, DocumentColor, Formatting, GotoDefinition, HoverRequest,
    Request as _,
};
use lsp_types::{
    ColorInformation, ColorPresentation, ColorProviderCapability, CompletionItemKind,
    CompletionOptions, CompletionResponse, Diagnostic, DiagnosticSeverity, Documentation,
    GotoDefinitionResponse, Hover, HoverContents, HoverProviderCapability, InitializeParams,
    InsertTextFormat, Location, MarkupContent, MarkupKind, MessageType, NumberOrString, OneOf,
    PositionEncodingKind, PublishDiagnosticsParams, ServerCapabilities, ShowMessageParams,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions, TextEdit, Uri,
};

use context::Workspace;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--check") => std::process::exit(check(&args[1..])),
        Some("--version") => println!("mss-lsp {}", env!("CARGO_PKG_VERSION")),
        Some("--help" | "-h") => println!(
            "mss-lsp — LSP-сервер MSS (stdio).\n  mss-lsp            сервер\n  mss-lsp --check ф… диагностики файлов"
        ),
        _ => {
            if let Err(e) = serve() {
                eprintln!("mss-lsp: {e}");
                std::process::exit(1);
            }
        }
    }
}

/// `--check`: диагностики файлов с контекстом их каталога.
fn check(files: &[String]) -> i32 {
    let mut errors = false;
    for f in files {
        let path = PathBuf::from(f);
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{f}: {e}");
                errors = true;
                continue;
            }
        };
        let root = path
            .canonicalize()
            .ok()
            .and_then(|p| workspace_root_for(&p));
        let ws = Workspace::load(root);
        if let Some(e) = &ws.context_error {
            eprintln!("контекст: {e}");
        }
        let ctx = ws.context_for(path.canonicalize().ok().as_deref());
        for d in mss_analysis::diagnostics(&text, &ctx) {
            if d.severity == mss_analysis::Severity::Error {
                errors = true;
            }
            println!(
                "{f}:{}:{}: {}: {} [{}]",
                d.range.start.line + 1,
                d.range.start.col + 1,
                severity_word(d.severity),
                d.message,
                d.code
            );
        }
    }
    i32::from(errors)
}

/// Корень для `--check`: каталог с файлом контекста или с `.git`, иначе
/// каталог файла.
fn workspace_root_for(file: &Path) -> Option<PathBuf> {
    let start = file.parent()?;
    let mut dir = Some(start);
    while let Some(d) = dir {
        if context::CONTEXT_FILES.iter().any(|n| d.join(n).is_file()) || d.join(".git").exists() {
            return Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    Some(start.to_path_buf())
}

fn severity_word(s: mss_analysis::Severity) -> &'static str {
    match s {
        mss_analysis::Severity::Error => "ошибка",
        mss_analysis::Severity::Warning => "предупреждение",
        mss_analysis::Severity::Info => "сведение",
        mss_analysis::Severity::Hint => "подсказка",
    }
}

/// Кодировка колонок LSP: UTF-16 по умолчанию, UTF-32 (символы — как у
/// mss-analysis), если клиент умеет.
#[derive(Clone, Copy, PartialEq)]
enum Encoding {
    Utf16,
    Utf32,
}

struct Server {
    conn: Connection,
    encoding: Encoding,
    docs: HashMap<String, (Uri, String)>,
    ws: Workspace,
}

fn serve() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let (conn, io) = Connection::stdio();
    let (id, params) = conn.initialize_start()?;
    let init: InitializeParams = serde_json::from_value(params)?;

    let utf32 = init
        .capabilities
        .general
        .as_ref()
        .and_then(|g| g.position_encodings.as_ref())
        .is_some_and(|e| e.contains(&PositionEncodingKind::UTF32));
    let encoding = if utf32 {
        Encoding::Utf32
    } else {
        Encoding::Utf16
    };

    #[allow(deprecated)]
    let root = init
        .workspace_folders
        .as_ref()
        .and_then(|f| f.first())
        .map(|f| f.uri.clone())
        .or(init.root_uri.clone())
        .and_then(|u| uri_to_path(&u));

    let caps = ServerCapabilities {
        position_encoding: Some(if utf32 {
            PositionEncodingKind::UTF32
        } else {
            PositionEncodingKind::UTF16
        }),
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(TextDocumentSyncKind::FULL),
                save: Some(lsp_types::TextDocumentSyncSaveOptions::SaveOptions(
                    lsp_types::SaveOptions {
                        include_text: Some(true),
                    },
                )),
                ..Default::default()
            },
        )),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(
                [".", ":", "-", "(", "@", " "]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            ),
            ..Default::default()
        }),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        color_provider: Some(ColorProviderCapability::Simple(true)),
        document_formatting_provider: Some(OneOf::Left(true)),
        ..Default::default()
    };
    let result = serde_json::json!({
        "capabilities": caps,
        "serverInfo": { "name": "mss-lsp", "version": env!("CARGO_PKG_VERSION") },
    });
    conn.initialize_finish(id, result)?;

    let mut server = Server {
        conn,
        encoding,
        docs: HashMap::new(),
        ws: Workspace::load(root),
    };
    if let Some(e) = server.ws.context_error.clone() {
        server.show(
            MessageType::WARNING,
            format!("mss-lsp: файл контекста не разобран: {e}"),
        );
    }
    server.run()?;
    // Соединение закрывается раньше join: иначе поток записи ждёт вечно.
    drop(server);
    io.join()?;
    Ok(())
}

impl Server {
    fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
        while let Ok(msg) = self.conn.receiver.recv() {
            match msg {
                Message::Request(req) => {
                    if self.conn.handle_shutdown(&req)? {
                        return Ok(());
                    }
                    self.request(req);
                }
                Message::Notification(n) => self.notification(n),
                Message::Response(_) => {}
            }
        }
        Ok(())
    }

    fn show(&self, typ: MessageType, message: String) {
        let _ = self
            .conn
            .sender
            .send(Message::Notification(Notification::new(
                ShowMessage::METHOD.to_string(),
                ShowMessageParams { typ, message },
            )));
    }

    fn reply(&self, id: RequestId, value: impl serde::Serialize) {
        let _ = self
            .conn
            .sender
            .send(Message::Response(Response::new_ok(id, value)));
    }

    fn notification(&mut self, n: Notification) {
        match n.method.as_str() {
            DidOpenTextDocument::METHOD => {
                if let Ok(p) =
                    serde_json::from_value::<lsp_types::DidOpenTextDocumentParams>(n.params)
                {
                    let uri = p.text_document.uri;
                    self.docs.insert(
                        uri.as_str().to_string(),
                        (uri.clone(), p.text_document.text),
                    );
                    self.publish(&uri);
                }
            }
            DidChangeTextDocument::METHOD => {
                if let Ok(p) =
                    serde_json::from_value::<lsp_types::DidChangeTextDocumentParams>(n.params)
                {
                    let uri = p.text_document.uri;
                    // Полная синхронизация: последнее изменение — весь текст.
                    if let Some(c) = p.content_changes.into_iter().last() {
                        self.docs
                            .insert(uri.as_str().to_string(), (uri.clone(), c.text));
                        self.publish(&uri);
                    }
                }
            }
            DidSaveTextDocument::METHOD => {
                if let Ok(p) =
                    serde_json::from_value::<lsp_types::DidSaveTextDocumentParams>(n.params)
                {
                    let uri = p.text_document.uri;
                    let path = uri_to_path(&uri);
                    let is_context = path.as_ref().is_some_and(|p| {
                        p.file_name()
                            .is_some_and(|n| context::CONTEXT_FILES.iter().any(|c| n == *c))
                    });
                    if is_context {
                        self.ws.rescan();
                        if let Some(e) = self.ws.context_error.clone() {
                            self.show(MessageType::WARNING, format!("mss-lsp: {e}"));
                        }
                    } else if let (Some(path), Some(text)) = (path, p.text) {
                        self.ws.update_file(&path, &text);
                    }
                    self.publish_all();
                }
            }
            DidChangeWatchedFiles::METHOD => {
                self.ws.rescan();
                self.publish_all();
            }
            DidCloseTextDocument::METHOD => {
                if let Ok(p) =
                    serde_json::from_value::<lsp_types::DidCloseTextDocumentParams>(n.params)
                {
                    let uri = p.text_document.uri;
                    self.docs.remove(uri.as_str());
                    self.send_diagnostics(uri, Vec::new());
                }
            }
            _ => {}
        }
    }

    fn context(&self, uri: &Uri) -> mss_analysis::Context {
        self.ws.context_for(uri_to_path(uri).as_deref())
    }

    fn publish_all(&self) {
        let uris: Vec<Uri> = self.docs.values().map(|d| d.0.clone()).collect();
        for u in uris {
            self.publish(&u);
        }
    }

    fn publish(&self, uri: &Uri) {
        let Some((_, text)) = self.docs.get(uri.as_str()) else {
            return;
        };
        let ctx = self.context(uri);
        let diags = mss_analysis::diagnostics(text, &ctx)
            .into_iter()
            .map(|d| Diagnostic {
                range: self.to_lsp_range(text, d.range),
                severity: Some(match d.severity {
                    mss_analysis::Severity::Error => DiagnosticSeverity::ERROR,
                    mss_analysis::Severity::Warning => DiagnosticSeverity::WARNING,
                    mss_analysis::Severity::Info => DiagnosticSeverity::INFORMATION,
                    mss_analysis::Severity::Hint => DiagnosticSeverity::HINT,
                }),
                code: Some(NumberOrString::String(d.code.to_string())),
                source: Some("mss".into()),
                message: d.message,
                ..Default::default()
            })
            .collect();
        self.send_diagnostics(uri.clone(), diags);
    }

    fn send_diagnostics(&self, uri: Uri, diagnostics: Vec<Diagnostic>) {
        let _ = self
            .conn
            .sender
            .send(Message::Notification(Notification::new(
                PublishDiagnostics::METHOD.to_string(),
                PublishDiagnosticsParams {
                    uri,
                    diagnostics,
                    version: None,
                },
            )));
    }

    fn request(&mut self, req: Request) {
        let id = req.id.clone();
        match req.method.as_str() {
            Completion::METHOD => {
                let Ok(p) = serde_json::from_value::<lsp_types::CompletionParams>(req.params)
                else {
                    return self.reply(id, serde_json::Value::Null);
                };
                let uri = p.text_document_position.text_document.uri;
                let Some((_, text)) = self.docs.get(uri.as_str()) else {
                    return self.reply(id, serde_json::Value::Null);
                };
                let pos = self.pos_from_lsp(text, p.text_document_position.position);
                let ctx = self.context(&uri);
                let resp = mss_analysis::completion(text, pos, &ctx).map(|c| {
                    let range = self.to_lsp_range(text, c.range);
                    CompletionResponse::Array(
                        c.items
                            .into_iter()
                            .map(|it| lsp_types::CompletionItem {
                                kind: Some(completion_kind(it.kind)),
                                detail: (!it.detail.is_empty()).then_some(it.detail),
                                documentation: (!it.documentation.is_empty()).then_some({
                                    Documentation::MarkupContent(MarkupContent {
                                        kind: MarkupKind::Markdown,
                                        value: it.documentation,
                                    })
                                }),
                                text_edit: Some(lsp_types::CompletionTextEdit::Edit(TextEdit {
                                    range,
                                    new_text: it.insert_text,
                                })),
                                insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                                filter_text: Some(it.label.clone()),
                                label: it.label,
                                ..Default::default()
                            })
                            .collect(),
                    )
                });
                self.reply(id, resp);
            }
            HoverRequest::METHOD => {
                let Ok(p) = serde_json::from_value::<lsp_types::HoverParams>(req.params) else {
                    return self.reply(id, serde_json::Value::Null);
                };
                let uri = p.text_document_position_params.text_document.uri;
                let Some((_, text)) = self.docs.get(uri.as_str()) else {
                    return self.reply(id, serde_json::Value::Null);
                };
                let pos = self.pos_from_lsp(text, p.text_document_position_params.position);
                let ctx = self.context(&uri);
                let resp = mss_analysis::hover(text, pos, &ctx).map(|h| Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: h.markdown,
                    }),
                    range: Some(self.to_lsp_range(text, h.range)),
                });
                self.reply(id, resp);
            }
            GotoDefinition::METHOD => {
                let Ok(p) = serde_json::from_value::<lsp_types::GotoDefinitionParams>(req.params)
                else {
                    return self.reply(id, serde_json::Value::Null);
                };
                let uri = p.text_document_position_params.text_document.uri;
                let Some((_, text)) = self.docs.get(uri.as_str()) else {
                    return self.reply(id, serde_json::Value::Null);
                };
                let pos = self.pos_from_lsp(text, p.text_document_position_params.position);
                let resp = mss_analysis::definition(text, pos).map(|r| {
                    GotoDefinitionResponse::Scalar(Location {
                        uri: uri.clone(),
                        range: self.to_lsp_range(text, r),
                    })
                });
                self.reply(id, resp);
            }
            DocumentColor::METHOD => {
                let Ok(p) = serde_json::from_value::<lsp_types::DocumentColorParams>(req.params)
                else {
                    return self.reply(id, serde_json::Value::Null);
                };
                let Some((_, text)) = self.docs.get(p.text_document.uri.as_str()) else {
                    return self.reply(id, Vec::<ColorInformation>::new());
                };
                let resp: Vec<ColorInformation> = mss_analysis::document_colors(text)
                    .into_iter()
                    .map(|c| ColorInformation {
                        range: self.to_lsp_range(text, c.range),
                        color: lsp_types::Color {
                            red: c.color.r as f32 / 255.0,
                            green: c.color.g as f32 / 255.0,
                            blue: c.color.b as f32 / 255.0,
                            alpha: c.color.a as f32 / 255.0,
                        },
                    })
                    .collect();
                self.reply(id, resp);
            }
            ColorPresentationRequest::METHOD => {
                let Ok(p) =
                    serde_json::from_value::<lsp_types::ColorPresentationParams>(req.params)
                else {
                    return self.reply(id, serde_json::Value::Null);
                };
                let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                let c = mss_analysis::Color::rgba(
                    byte(p.color.red),
                    byte(p.color.green),
                    byte(p.color.blue),
                    byte(p.color.alpha),
                );
                let resp: Vec<ColorPresentation> = mss_analysis::color_presentations(c)
                    .into_iter()
                    .map(|label| ColorPresentation {
                        text_edit: Some(TextEdit {
                            range: p.range,
                            new_text: label.clone(),
                        }),
                        label,
                        additional_text_edits: None,
                    })
                    .collect();
                self.reply(id, resp);
            }
            Formatting::METHOD => {
                let Ok(p) =
                    serde_json::from_value::<lsp_types::DocumentFormattingParams>(req.params)
                else {
                    return self.reply(id, serde_json::Value::Null);
                };
                let Some((_, text)) = self.docs.get(p.text_document.uri.as_str()) else {
                    return self.reply(id, serde_json::Value::Null);
                };
                let formatted = mss_analysis::format(text);
                let edits = if formatted == *text {
                    Vec::new()
                } else {
                    let end = self.end_pos(text);
                    vec![TextEdit {
                        range: lsp_types::Range {
                            start: lsp_types::Position::new(0, 0),
                            end,
                        },
                        new_text: formatted,
                    }]
                };
                self.reply(id, edits);
            }
            _ => {
                let _ = self.conn.sender.send(Message::Response(Response::new_err(
                    id,
                    lsp_server::ErrorCode::MethodNotFound as i32,
                    format!("не поддерживается: {}", req.method),
                )));
            }
        }
    }

    // --- Позиции -----------------------------------------------------------

    fn line_text(text: &str, line: usize) -> &str {
        let l = text.split('\n').nth(line).unwrap_or("");
        l.strip_suffix('\r').unwrap_or(l)
    }

    fn to_lsp_pos(&self, text: &str, p: mss_analysis::Position) -> lsp_types::Position {
        let col = match self.encoding {
            Encoding::Utf32 => p.col,
            Encoding::Utf16 => Self::line_text(text, p.line)
                .chars()
                .take(p.col)
                .map(char::len_utf16)
                .sum(),
        };
        lsp_types::Position::new(p.line as u32, col as u32)
    }

    fn to_lsp_range(&self, text: &str, r: mss_analysis::Range) -> lsp_types::Range {
        lsp_types::Range {
            start: self.to_lsp_pos(text, r.start),
            end: self.to_lsp_pos(text, r.end),
        }
    }

    fn pos_from_lsp(&self, text: &str, p: lsp_types::Position) -> mss_analysis::Position {
        let line = p.line as usize;
        let col = match self.encoding {
            Encoding::Utf32 => p.character as usize,
            Encoding::Utf16 => {
                let mut units = 0usize;
                let mut chars = 0usize;
                for c in Self::line_text(text, line).chars() {
                    if units >= p.character as usize {
                        break;
                    }
                    units += c.len_utf16();
                    chars += 1;
                }
                chars
            }
        };
        mss_analysis::Position::new(line, col)
    }

    fn end_pos(&self, text: &str) -> lsp_types::Position {
        let line = text.split('\n').count() - 1;
        let col = Self::line_text(text, line).chars().count();
        self.to_lsp_pos(text, mss_analysis::Position::new(line, col))
    }
}

fn completion_kind(k: mss_analysis::CompletionKind) -> CompletionItemKind {
    use mss_analysis::CompletionKind as K;
    match k {
        K::Property => CompletionItemKind::PROPERTY,
        K::Value | K::Keyword => CompletionItemKind::VALUE,
        K::Function => CompletionItemKind::FUNCTION,
        K::Variable => CompletionItemKind::VARIABLE,
        K::Class => CompletionItemKind::CLASS,
        K::Element => CompletionItemKind::STRUCT,
        K::PseudoClass => CompletionItemKind::KEYWORD,
        K::Keyframes => CompletionItemKind::EVENT,
        K::Color => CompletionItemKind::COLOR,
    }
}

/// `file:///путь` → путь (с раскодированием `%XX`, в том числе кириллицы).
fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    let s = uri.as_str();
    let rest = s.strip_prefix("file://")?;
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);
    // `file://host/путь` — хост пропускаем.
    let path = if rest.starts_with('/') {
        rest
    } else {
        &rest[rest.find('/')?..]
    };
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&path[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).ok().map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn uri_paths() {
        let u =
            Uri::from_str("file:///home/x/%D0%9F%D1%80%D0%BE%D0%B5%D0%BA%D1%82%D1%8B/a%20b.mss")
                .unwrap();
        assert_eq!(
            uri_to_path(&u).unwrap(),
            PathBuf::from("/home/x/Проекты/a b.mss")
        );
        let u = Uri::from_str("untitled:Untitled-1").unwrap();
        assert!(uri_to_path(&u).is_none());
    }
}
