use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;

use lsp_types::{
    ClientCapabilities, Diagnostic, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, DocumentFormattingParams,
    FormattingOptions, GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverContents,
    HoverParams, InitializeParams, InitializedParams, Location, MarkedString, PartialResultParams,
    Position, PublishDiagnosticsParams, TextDocumentContentChangeEvent, TextDocumentIdentifier,
    TextDocumentItem, TextDocumentPositionParams, TextEdit, VersionedTextDocumentIdentifier,
    WorkDoneProgressParams, WorkspaceFolder,
};
use serde::Serialize;
use serde_json::Value;
use url::Url;

#[derive(Clone, Debug)]
pub struct ServerSpec {
    pub language_id: String,
    pub command: String,
    pub args: Vec<String>,
    pub extensions: Vec<String>,
}

pub fn builtin_servers() -> Vec<ServerSpec> {
    vec![
        ServerSpec {
            language_id: "rust".into(),
            command: "rust-analyzer".into(),
            args: vec![],
            extensions: vec!["rs".into()],
        },
        ServerSpec {
            language_id: "python".into(),
            command: "pylsp".into(),
            args: vec![],
            extensions: vec!["py".into()],
        },
        ServerSpec {
            language_id: "typescript".into(),
            command: "typescript-language-server".into(),
            args: vec!["--stdio".into()],
            extensions: vec!["ts".into(), "tsx".into(), "js".into(), "jsx".into()],
        },
        ServerSpec {
            language_id: "go".into(),
            command: "gopls".into(),
            args: vec![],
            extensions: vec!["go".into()],
        },
        ServerSpec {
            language_id: "c".into(),
            command: "clangd".into(),
            args: vec![],
            extensions: vec![
                "c".into(),
                "h".into(),
                "cpp".into(),
                "hpp".into(),
                "cc".into(),
            ],
        },
    ]
}

#[derive(Debug)]
enum ServerMessage {
    Response {
        id: u64,
        result: Option<Value>,
        error: Option<Value>,
    },
    Notification {
        method: String,
        params: Value,
    },
    Disconnected,
}

#[derive(Clone, Debug)]
enum PendingKind {
    Initialize,
    Format { uri: Url, version: u32 },
    Definition,
    Hover,
}

#[derive(Clone, Debug)]
pub enum LspEvent {
    Diagnostics(Url, Vec<Diagnostic>),
    FormatEdits {
        uri: Url,
        version: u32,
        edits: Vec<TextEdit>,
    },
    Definition(Location),
    Hover {
        text: String,
    },
}

pub struct LspClient {
    spec: ServerSpec,
    process: Child,
    writer: Arc<Mutex<ChildStdin>>,
    rx: Receiver<ServerMessage>,
    next_id: u64,
    initialized: bool,
    pending: HashMap<u64, PendingKind>,
    open_docs: HashMap<Url, i32>,
    workspace_root: Option<Url>,
}

impl LspClient {
    pub fn spawn(spec: ServerSpec, root: Option<&Path>) -> std::io::Result<Self> {
        let mut child = Command::new(&spec.command)
            .args(&spec.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let writer = Arc::new(Mutex::new(stdin));
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                match read_message(&mut reader) {
                    Ok(Some(msg)) => {
                        if tx.send(msg).is_err() {
                            break;
                        }
                    }
                    Ok(None) | Err(_) => {
                        let _ = tx.send(ServerMessage::Disconnected);
                        break;
                    }
                }
            }
        });

        let workspace_root = root.and_then(|p| Url::from_directory_path(p).ok());

        let mut client = Self {
            spec,
            process: child,
            writer,
            rx,
            next_id: 1,
            initialized: false,
            pending: HashMap::new(),
            open_docs: HashMap::new(),
            workspace_root,
        };
        client.send_initialize();
        Ok(client)
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn send_initialize(&mut self) {
        let workspace_folders = self.workspace_root.as_ref().map(|u| {
            vec![WorkspaceFolder {
                uri: u.clone(),
                name: "workspace".into(),
            }]
        });
        #[allow(deprecated)]
        let params = InitializeParams {
            process_id: Some(std::process::id()),
            capabilities: ClientCapabilities::default(),
            workspace_folders,
            root_uri: self.workspace_root.clone(),
            ..Default::default()
        };
        let id = self.next_id();
        self.pending.insert(id, PendingKind::Initialize);
        let _ = send_request(&self.writer, id, "initialize", &params);
    }

    pub fn handle_messages(&mut self, events: &mut Vec<LspEvent>) -> bool {
        let mut alive = true;
        loop {
            match self.rx.try_recv() {
                Ok(ServerMessage::Response { id, result, error }) => {
                    let kind = self.pending.remove(&id);
                    match kind {
                        Some(PendingKind::Initialize) => {
                            if error.is_none() && result.is_some() {
                                self.initialized = true;
                                let _ = send_notification(
                                    &self.writer,
                                    "initialized",
                                    &InitializedParams {},
                                );
                            }
                        }
                        Some(PendingKind::Format { uri, version }) => {
                            if let Some(value) = result {
                                if let Ok(Some(edits)) =
                                    serde_json::from_value::<Option<Vec<TextEdit>>>(value)
                                {
                                    if !edits.is_empty() {
                                        events.push(LspEvent::FormatEdits {
                                            uri,
                                            version,
                                            edits,
                                        });
                                    }
                                }
                            }
                        }
                        Some(PendingKind::Definition) => {
                            if let Some(value) = result {
                                if let Ok(Some(resp)) =
                                    serde_json::from_value::<Option<GotoDefinitionResponse>>(value)
                                {
                                    let target = match resp {
                                        GotoDefinitionResponse::Scalar(loc) => Some(loc),
                                        GotoDefinitionResponse::Array(mut v) => v.drain(..).next(),
                                        GotoDefinitionResponse::Link(mut v) => {
                                            v.drain(..).next().map(|l| Location {
                                                uri: l.target_uri,
                                                range: l.target_selection_range,
                                            })
                                        }
                                    };
                                    if let Some(loc) = target {
                                        events.push(LspEvent::Definition(loc));
                                    }
                                }
                            }
                        }
                        Some(PendingKind::Hover) => {
                            if let Some(value) = result {
                                if let Ok(Some(hover)) =
                                    serde_json::from_value::<Option<Hover>>(value)
                                {
                                    let text = hover_text(&hover);
                                    if !text.is_empty() {
                                        events.push(LspEvent::Hover { text });
                                    }
                                }
                            }
                        }
                        None => {}
                    }
                }
                Ok(ServerMessage::Notification { method, params }) => {
                    if method == "textDocument/publishDiagnostics" {
                        if let Ok(p) = serde_json::from_value::<PublishDiagnosticsParams>(params) {
                            events.push(LspEvent::Diagnostics(p.uri, p.diagnostics));
                        }
                    }
                }
                Ok(ServerMessage::Disconnected) => {
                    alive = false;
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    alive = false;
                    break;
                }
            }
        }
        alive
    }

    pub fn request_format(&mut self, uri: Url, version: u32) {
        if !self.initialized || !self.open_docs.contains_key(&uri) {
            return;
        }
        let params = DocumentFormattingParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            options: FormattingOptions {
                tab_size: 4,
                insert_spaces: true,
                ..Default::default()
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        let id = self.next_id();
        self.pending
            .insert(id, PendingKind::Format { uri, version });
        let _ = send_request(&self.writer, id, "textDocument/formatting", &params);
    }

    pub fn request_definition(&mut self, uri: Url, line: u32, character: u32) {
        if !self.initialized || !self.open_docs.contains_key(&uri) {
            return;
        }
        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position { line, character },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };
        let id = self.next_id();
        self.pending.insert(id, PendingKind::Definition);
        let _ = send_request(&self.writer, id, "textDocument/definition", &params);
    }

    pub fn request_hover(&mut self, uri: Url, line: u32, character: u32) {
        if !self.initialized || !self.open_docs.contains_key(&uri) {
            return;
        }
        let params = HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri },
                position: Position { line, character },
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        };
        let id = self.next_id();
        self.pending.insert(id, PendingKind::Hover);
        let _ = send_request(&self.writer, id, "textDocument/hover", &params);
    }

    pub fn did_open(&mut self, uri: Url, content: &str) {
        if !self.initialized || self.open_docs.contains_key(&uri) {
            return;
        }
        let version = 1;
        self.open_docs.insert(uri.clone(), version);
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri,
                language_id: self.spec.language_id.clone(),
                version,
                text: content.to_string(),
            },
        };
        let _ = send_notification(&self.writer, "textDocument/didOpen", &params);
    }

    pub fn did_change(&mut self, uri: &Url, content: &str) {
        if !self.initialized {
            return;
        }
        let version = match self.open_docs.get_mut(uri) {
            Some(v) => {
                *v += 1;
                *v
            }
            None => return,
        };
        let params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: content.to_string(),
            }],
        };
        let _ = send_notification(&self.writer, "textDocument/didChange", &params);
    }

    pub fn did_save(&mut self, uri: &Url, content: &str) {
        if !self.initialized || !self.open_docs.contains_key(uri) {
            return;
        }
        let params = DidSaveTextDocumentParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            text: Some(content.to_string()),
        };
        let _ = send_notification(&self.writer, "textDocument/didSave", &params);
    }

    pub fn did_close(&mut self, uri: &Url) {
        if !self.initialized {
            return;
        }
        if self.open_docs.remove(uri).is_some() {
            let params = DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
            };
            let _ = send_notification(&self.writer, "textDocument/didClose", &params);
        }
    }

    pub fn shutdown(&mut self) {
        if self.initialized {
            let id = self.next_id();
            let _ = send_request::<Value>(&self.writer, id, "shutdown", &Value::Null);
            let _ = send_notification::<Value>(&self.writer, "exit", &Value::Null);
        }
        let _ = self.process.kill();
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn send_request<T: Serialize>(
    writer: &Arc<Mutex<ChildStdin>>,
    id: u64,
    method: &str,
    params: &T,
) -> std::io::Result<()> {
    let msg = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });
    write_message(writer, &msg)
}

fn send_notification<T: Serialize>(
    writer: &Arc<Mutex<ChildStdin>>,
    method: &str,
    params: &T,
) -> std::io::Result<()> {
    let msg = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    });
    write_message(writer, &msg)
}

fn write_message(writer: &Arc<Mutex<ChildStdin>>, msg: &Value) -> std::io::Result<()> {
    let body = serde_json::to_string(msg)?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    let mut w = writer.lock().unwrap();
    w.write_all(header.as_bytes())?;
    w.write_all(body.as_bytes())?;
    w.flush()?;
    Ok(())
}

fn read_message<R: BufRead>(reader: &mut R) -> std::io::Result<Option<ServerMessage>> {
    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            return Ok(None);
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(len_str) = line.strip_prefix("Content-Length:") {
            content_length = len_str.trim().parse().ok();
        }
    }
    let len = content_length.ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "missing Content-Length")
    })?;
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;
    let value: Value = serde_json::from_slice(&buf)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let msg = if let Some(method) = value.get("method").and_then(|v| v.as_str()) {
        let method = method.to_string();
        let params = value.get("params").cloned().unwrap_or(Value::Null);
        ServerMessage::Notification { method, params }
    } else if let Some(id) = value.get("id").and_then(|v| v.as_u64()) {
        let result = value.get("result").cloned();
        let error = value.get("error").cloned();
        ServerMessage::Response { id, result, error }
    } else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "unrecognized message",
        ));
    };
    Ok(Some(msg))
}

pub struct LspManager {
    servers: HashMap<String, LspClient>,
    pub diagnostics: HashMap<Url, Vec<Diagnostic>>,
    pub specs: Vec<ServerSpec>,
    pub workspace_root: Option<PathBuf>,
    pub last_error: Option<String>,
    spawn_attempted: HashMap<String, bool>,
}

impl LspManager {
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
            diagnostics: HashMap::new(),
            specs: builtin_servers(),
            workspace_root: None,
            last_error: None,
            spawn_attempted: HashMap::new(),
        }
    }

    pub fn set_workspace_root(&mut self, path: PathBuf) {
        self.servers.clear();
        self.spawn_attempted.clear();
        self.diagnostics.clear();
        self.workspace_root = Some(path);
    }

    fn language_for_extension(&self, ext: &str) -> Option<String> {
        self.specs
            .iter()
            .find(|s| s.extensions.iter().any(|e| e == ext))
            .map(|s| s.language_id.clone())
    }

    fn ensure_server(&mut self, language_id: &str) -> Option<&mut LspClient> {
        if self.servers.contains_key(language_id) {
            return self.servers.get_mut(language_id);
        }
        if self
            .spawn_attempted
            .get(language_id)
            .copied()
            .unwrap_or(false)
        {
            return None;
        }
        self.spawn_attempted.insert(language_id.to_string(), true);
        let spec = self
            .specs
            .iter()
            .find(|s| s.language_id == language_id)?
            .clone();
        match LspClient::spawn(spec, self.workspace_root.as_deref()) {
            Ok(client) => {
                self.servers.insert(language_id.to_string(), client);
                self.servers.get_mut(language_id)
            }
            Err(e) => {
                self.last_error = Some(format!("{language_id}: {e}"));
                None
            }
        }
    }

    pub fn poll(&mut self) -> Vec<LspEvent> {
        let mut events: Vec<LspEvent> = Vec::new();
        let mut dead: Vec<String> = Vec::new();
        for (lang, client) in self.servers.iter_mut() {
            let alive = client.handle_messages(&mut events);
            if !alive {
                dead.push(lang.clone());
            }
        }
        // Diagnostics events also update the cache so total_count() stays current.
        for ev in &events {
            if let LspEvent::Diagnostics(uri, diags) = ev {
                self.diagnostics.insert(uri.clone(), diags.clone());
            }
        }
        for d in dead {
            self.servers.remove(&d);
        }
        events
    }

    pub fn request_format(&mut self, path: &Path, version: u32) {
        let Some((uri, lang)) = self.uri_and_lang_for(path) else {
            return;
        };
        if let Some(client) = self.servers.get_mut(&lang) {
            client.request_format(uri, version);
        }
    }

    pub fn request_definition(&mut self, path: &Path, line: u32, character: u32) {
        let Some((uri, lang)) = self.uri_and_lang_for(path) else {
            return;
        };
        if let Some(client) = self.servers.get_mut(&lang) {
            client.request_definition(uri, line, character);
        }
    }

    pub fn request_hover(&mut self, path: &Path, line: u32, character: u32) {
        let Some((uri, lang)) = self.uri_and_lang_for(path) else {
            return;
        };
        if let Some(client) = self.servers.get_mut(&lang) {
            client.request_hover(uri, line, character);
        }
    }

    fn uri_and_lang_for(&self, path: &Path) -> Option<(Url, String)> {
        let ext = path.extension().and_then(|s| s.to_str())?;
        let lang = self.language_for_extension(ext)?;
        let uri = Url::from_file_path(path).ok()?;
        Some((uri, lang))
    }

    pub fn open_doc(&mut self, path: &Path, content: &str) {
        let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
            return;
        };
        let Some(lang) = self.language_for_extension(ext) else {
            return;
        };
        let Ok(uri) = Url::from_file_path(path) else {
            return;
        };
        if let Some(client) = self.ensure_server(&lang) {
            client.did_open(uri, content);
        }
    }

    pub fn change_doc(&mut self, path: &Path, content: &str) {
        let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
            return;
        };
        let Some(lang) = self.language_for_extension(ext) else {
            return;
        };
        let Ok(uri) = Url::from_file_path(path) else {
            return;
        };
        if let Some(client) = self.servers.get_mut(&lang) {
            client.did_change(&uri, content);
        }
    }

    pub fn save_doc(&mut self, path: &Path, content: &str) {
        let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
            return;
        };
        let Some(lang) = self.language_for_extension(ext) else {
            return;
        };
        let Ok(uri) = Url::from_file_path(path) else {
            return;
        };
        if let Some(client) = self.servers.get_mut(&lang) {
            client.did_save(&uri, content);
        }
    }

    pub fn close_doc(&mut self, path: &Path) {
        let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
            return;
        };
        let Some(lang) = self.language_for_extension(ext) else {
            return;
        };
        let Ok(uri) = Url::from_file_path(path) else {
            return;
        };
        if let Some(client) = self.servers.get_mut(&lang) {
            client.did_close(&uri);
        }
        self.diagnostics.remove(&uri);
    }

    pub fn total_count(&self) -> (usize, usize) {
        let mut errors = 0usize;
        let mut warnings = 0usize;
        for diags in self.diagnostics.values() {
            for d in diags {
                match d.severity {
                    Some(lsp_types::DiagnosticSeverity::ERROR) => errors += 1,
                    Some(lsp_types::DiagnosticSeverity::WARNING) => warnings += 1,
                    _ => {}
                }
            }
        }
        (errors, warnings)
    }

    pub fn active_languages(&self) -> Vec<String> {
        self.servers.keys().cloned().collect()
    }
}

impl Default for LspManager {
    fn default() -> Self {
        Self::new()
    }
}

fn hover_text(hover: &Hover) -> String {
    fn marked_to_string(m: &MarkedString) -> String {
        match m {
            MarkedString::String(s) => s.clone(),
            MarkedString::LanguageString(ls) => ls.value.clone(),
        }
    }
    match &hover.contents {
        HoverContents::Scalar(m) => marked_to_string(m),
        HoverContents::Array(arr) => arr
            .iter()
            .map(marked_to_string)
            .collect::<Vec<_>>()
            .join("\n\n"),
        HoverContents::Markup(m) => m.value.clone(),
    }
}

pub fn apply_text_edits(content: &str, edits: &[TextEdit]) -> String {
    if edits.is_empty() {
        return content.to_string();
    }
    let mut sorted: Vec<&TextEdit> = edits.iter().collect();
    sorted.sort_by(|a, b| {
        b.range
            .start
            .line
            .cmp(&a.range.start.line)
            .then_with(|| b.range.start.character.cmp(&a.range.start.character))
    });
    let mut result = content.to_string();
    for edit in sorted {
        let start = position_to_byte(&result, edit.range.start);
        let end = position_to_byte(&result, edit.range.end);
        if start <= end && end <= result.len() {
            result.replace_range(start..end, &edit.new_text);
        }
    }
    result
}

fn position_to_byte(content: &str, pos: Position) -> usize {
    let mut line = 0u32;
    let mut col = 0u32;
    for (byte_idx, ch) in content.char_indices() {
        if line == pos.line && col == pos.character {
            return byte_idx;
        }
        if ch == '\n' {
            if line == pos.line {
                return byte_idx;
            }
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    content.len()
}
