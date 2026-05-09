use std::path::{Path, PathBuf};

use eframe::CreationContext;
use egui::{Context, Key, Modifiers};

use crate::editor::EditorView;
use crate::explorer::FileExplorer;
use crate::find::{self, FindAction, FindState};
use crate::git::GitRepo;
use crate::lsp::{self, LspEvent, LspManager};
use crate::problems::{self, ProblemAction};
use crate::scm::{ScmAction, ScmState};
use crate::search::{self, SearchAction, SearchState};
use crate::tabs::{OpenFile, TabBar};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SidePanelView {
    Explorer,
    SourceControl,
    Search,
}

pub struct CodeEditorApp {
    pub explorer: FileExplorer,
    pub tabs: TabBar,
    pub status_message: String,
    pub show_side_panel: bool,
    pub side_view: SidePanelView,
    pub git: Option<GitRepo>,
    pub scm: ScmState,
    pub blame_open: bool,
    pub blame_title: String,
    pub blame_text: String,
    pub lsp: LspManager,
    pub show_problems: bool,
    pub find: FindState,
    pub show_find_bar: bool,
    pub search: SearchState,
    pub format_on_save: bool,
    pub hover_open: bool,
    pub hover_text: String,
}

impl CodeEditorApp {
    pub fn new(_cc: &CreationContext<'_>) -> Self {
        let mut app = Self {
            explorer: FileExplorer::new(),
            tabs: TabBar::default(),
            status_message: String::from("Ready"),
            show_side_panel: true,
            side_view: SidePanelView::Explorer,
            git: None,
            scm: ScmState::default(),
            blame_open: false,
            blame_title: String::new(),
            blame_text: String::new(),
            lsp: LspManager::new(),
            show_problems: false,
            find: FindState::default(),
            show_find_bar: false,
            search: SearchState::default(),
            format_on_save: true,
            hover_open: false,
            hover_text: String::new(),
        };
        if let Ok(cwd) = std::env::current_dir() {
            app.lsp.set_workspace_root(cwd.clone());
            app.search.root = Some(cwd.clone());
            app.open_git(&cwd);
        }
        app
    }

    fn handle_shortcuts(&mut self, ctx: &Context) {
        let ctrl = Modifiers::COMMAND;
        ctx.input_mut(|i| {
            if i.consume_key(ctrl, Key::S) {
                self.action_save();
            }
            if i.consume_key(ctrl | Modifiers::SHIFT, Key::S) {
                self.action_save_as();
            }
            if i.consume_key(ctrl, Key::O) {
                self.action_open_file();
            }
            if i.consume_key(ctrl, Key::N) {
                self.action_new_file();
            }
            if i.consume_key(ctrl, Key::W) {
                self.action_close_active();
            }
            if i.consume_key(ctrl, Key::B) {
                self.show_side_panel = !self.show_side_panel;
            }
            if i.consume_key(ctrl | Modifiers::SHIFT, Key::E) {
                self.show_side_panel = true;
                self.side_view = SidePanelView::Explorer;
            }
            if i.consume_key(ctrl | Modifiers::SHIFT, Key::G) {
                self.show_side_panel = true;
                self.side_view = SidePanelView::SourceControl;
            }
            if i.consume_key(ctrl | Modifiers::SHIFT, Key::M) {
                self.show_problems = !self.show_problems;
            }
            if i.consume_key(ctrl, Key::F) {
                self.show_find_bar = true;
                self.find.show_replace = false;
                self.find.focus_query = true;
            }
            if i.consume_key(ctrl, Key::H) {
                self.show_find_bar = true;
                self.find.show_replace = true;
                self.find.focus_query = true;
            }
            if i.consume_key(ctrl | Modifiers::SHIFT, Key::F) {
                self.show_side_panel = true;
                self.side_view = SidePanelView::Search;
                self.search.focus_query = true;
            }
            if self.show_find_bar && i.consume_key(Modifiers::NONE, Key::Escape) {
                self.show_find_bar = false;
            }
            if i.consume_key(Modifiers::NONE, Key::F12) {
                self.action_goto_definition();
            }
            if i.consume_key(ctrl | Modifiers::SHIFT, Key::I) {
                self.action_show_hover();
            }
        });
    }

    pub fn action_goto_definition(&mut self) {
        let Some(file) = self.tabs.active() else {
            return;
        };
        let Some(path) = file.path.clone() else {
            return;
        };
        let line = file.cursor_line as u32;
        let character = file.cursor_col as u32;
        self.lsp.request_definition(&path, line, character);
        self.status_message = "Looking up definition…".into();
    }

    pub fn action_show_hover(&mut self) {
        let Some(file) = self.tabs.active() else {
            return;
        };
        let Some(path) = file.path.clone() else {
            return;
        };
        let line = file.cursor_line as u32;
        let character = file.cursor_col as u32;
        self.lsp.request_hover(&path, line, character);
        self.status_message = "Fetching hover…".into();
    }

    fn handle_lsp_events(&mut self, events: Vec<LspEvent>) {
        for event in events {
            match event {
                LspEvent::Diagnostics(_, _) => {
                    // Already cached by LspManager.poll
                }
                LspEvent::FormatEdits {
                    uri,
                    version,
                    edits,
                } => {
                    let Ok(path) = uri.to_file_path() else {
                        continue;
                    };
                    let Some(file) = self
                        .tabs
                        .files
                        .iter_mut()
                        .find(|f| f.path.as_deref() == Some(path.as_path()))
                    else {
                        continue;
                    };
                    if file.content_version != version {
                        // File edited since the request; skip stale edits.
                        continue;
                    }
                    let new_content = lsp::apply_text_edits(&file.content, &edits);
                    if new_content != file.content {
                        file.content = new_content;
                        file.content_version = file.content_version.wrapping_add(1);
                        // Persist the formatted output.
                        if let Some(p) = &file.path {
                            let p = p.clone();
                            let content = file.content.clone();
                            if std::fs::write(&p, &content).is_ok() {
                                file.on_disk = content.clone();
                                self.lsp.save_doc(&p, &content);
                                self.status_message = format!("Formatted {}", p.display());
                            }
                        }
                    }
                }
                LspEvent::Definition(loc) => {
                    if let Ok(path) = loc.uri.to_file_path() {
                        let line = loc.range.start.line + 1;
                        self.jump_to(path, line);
                    }
                }
                LspEvent::Hover { text } => {
                    self.hover_text = text;
                    self.hover_open = true;
                    self.status_message = "Hover ready".into();
                }
            }
        }
    }

    pub fn action_new_file(&mut self) {
        self.tabs.add_file(OpenFile::untitled());
        self.status_message = String::from("New file");
    }

    pub fn action_open_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_file() {
            self.open_path(path);
        }
    }

    pub fn action_open_folder(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            self.explorer.set_root(path.clone());
            self.lsp.set_workspace_root(path.clone());
            self.search.root = Some(path.clone());
            self.search.results.clear();
            self.open_git(&path);
            self.status_message = format!("Opened folder {}", path.display());
        }
    }

    pub fn action_save(&mut self) {
        let saved = self.tabs.save_active();
        match saved {
            Ok(Some(path)) => {
                self.status_message = format!("Saved {}", path.display());
                if let Some(file) = self.tabs.active() {
                    let content = file.content.clone();
                    let version = file.content_version;
                    self.lsp.save_doc(&path, &content);
                    if self.format_on_save {
                        self.lsp.request_format(&path, version);
                    }
                }
            }
            Ok(None) => self.action_save_as(),
            Err(e) => self.status_message = format!("Save failed: {e}"),
        }
        self.refresh_scm();
    }

    pub fn action_save_as(&mut self) {
        if let Some(path) = rfd::FileDialog::new().save_file() {
            match self.tabs.save_active_as(&path) {
                Ok(()) => {
                    self.status_message = format!("Saved {}", path.display());
                    if let Some(file) = self.tabs.active() {
                        let content = file.content.clone();
                        self.lsp.save_doc(&path, &content);
                    }
                }
                Err(e) => self.status_message = format!("Save failed: {e}"),
            }
            self.refresh_scm();
        }
    }

    pub fn action_close_active(&mut self) {
        if let Some(file) = self.tabs.active() {
            if let Some(path) = &file.path {
                let p = path.clone();
                self.lsp.close_doc(&p);
            }
        }
        self.tabs.close_active();
    }

    pub fn open_path(&mut self, path: PathBuf) {
        match OpenFile::from_path(&path) {
            Ok(file) => {
                let content = file.content.clone();
                self.tabs.add_file(file);
                self.lsp.open_doc(&path, &content);
                if let Some(active) = self.tabs.active_mut() {
                    active.lsp_known_path = Some(path.clone());
                    active.lsp_synced_version = active.content_version;
                }
                self.status_message = format!("Opened {}", path.display());
            }
            Err(e) => self.status_message = format!("Open failed: {e}"),
        }
    }

    fn sync_lsp_documents(&mut self) {
        let mut changes: Vec<(PathBuf, String)> = Vec::new();
        for file in self.tabs.files.iter_mut() {
            if let Some(path) = &file.path {
                if file.content_version != file.lsp_synced_version {
                    file.lsp_synced_version = file.content_version;
                    changes.push((path.clone(), file.content.clone()));
                }
            }
        }
        for (path, content) in changes {
            self.lsp.change_doc(&path, &content);
        }
    }

    fn jump_to(&mut self, path: PathBuf, line: u32) {
        let already_open = self
            .tabs
            .files
            .iter()
            .position(|f| f.path.as_deref() == Some(path.as_path()));
        match already_open {
            Some(idx) => self.tabs.active = Some(idx),
            None => self.open_path(path.clone()),
        }
        if let Some(file) = self.tabs.active_mut() {
            file.goto_line = Some(line);
        }
        self.status_message = format!("{}:{}", path.display(), line);
    }

    fn jump_to_byte_range(&mut self, path: PathBuf, byte_start: usize, byte_end: usize) {
        let already_open = self
            .tabs
            .files
            .iter()
            .position(|f| f.path.as_deref() == Some(path.as_path()));
        match already_open {
            Some(idx) => self.tabs.active = Some(idx),
            None => self.open_path(path.clone()),
        }
        if let Some(file) = self.tabs.active_mut() {
            let start = find::byte_to_char(&file.content, byte_start);
            let end = find::byte_to_char(&file.content, byte_end);
            file.goto_range = Some((start, end));
        }
    }

    fn handle_find_action(&mut self, action: FindAction) {
        let Some(file) = self.tabs.active_mut() else {
            self.find.last_status = "No active file".into();
            return;
        };
        let case = self.find.case_sensitive;
        match action {
            FindAction::Close => {
                self.show_find_bar = false;
            }
            FindAction::Next => {
                if self.find.query.is_empty() {
                    return;
                }
                let from_byte = find::char_to_byte(&file.content, file.cursor_char + 1);
                if let Some((bs, be)) =
                    find::find_byte_range(&file.content, &self.find.query, case, from_byte, true)
                {
                    let s = find::byte_to_char(&file.content, bs);
                    let e = find::byte_to_char(&file.content, be);
                    file.goto_range = Some((s, e));
                    self.find.last_status = format!("Match @ {s}");
                } else {
                    self.find.last_status = "No match".into();
                }
            }
            FindAction::Prev => {
                if self.find.query.is_empty() {
                    return;
                }
                let cur = file.cursor_char.min(file.cursor_anchor);
                let from_byte = find::char_to_byte(&file.content, cur);
                if let Some((bs, be)) =
                    find::find_byte_range(&file.content, &self.find.query, case, from_byte, false)
                {
                    let s = find::byte_to_char(&file.content, bs);
                    let e = find::byte_to_char(&file.content, be);
                    file.goto_range = Some((s, e));
                    self.find.last_status = format!("Match @ {s}");
                } else {
                    self.find.last_status = "No match".into();
                }
            }
            FindAction::Replace => {
                let (sel_start, sel_end) = (
                    file.cursor_char.min(file.cursor_anchor),
                    file.cursor_char.max(file.cursor_anchor),
                );
                let bs = find::char_to_byte(&file.content, sel_start);
                let be = find::char_to_byte(&file.content, sel_end);
                let selected = &file.content[bs..be];
                let matches = if case {
                    selected == self.find.query
                } else {
                    selected.eq_ignore_ascii_case(&self.find.query)
                };
                if matches && !self.find.query.is_empty() {
                    let new_content = format!(
                        "{}{}{}",
                        &file.content[..bs],
                        &self.find.replacement,
                        &file.content[be..]
                    );
                    file.content = new_content;
                    file.content_version = file.content_version.wrapping_add(1);
                    let new_end_char = sel_start + self.find.replacement.chars().count();
                    file.goto_range = Some((sel_start, new_end_char));
                    file.cursor_char = new_end_char;
                    file.cursor_anchor = new_end_char;
                    self.find.last_status = "Replaced".into();
                } else {
                    let from_byte = find::char_to_byte(&file.content, file.cursor_char);
                    if let Some((nbs, nbe)) = find::find_byte_range(
                        &file.content,
                        &self.find.query,
                        case,
                        from_byte,
                        true,
                    ) {
                        let s = find::byte_to_char(&file.content, nbs);
                        let e = find::byte_to_char(&file.content, nbe);
                        file.goto_range = Some((s, e));
                        self.find.last_status = "Found next — click Replace again".into();
                    } else {
                        self.find.last_status = "No match".into();
                    }
                }
            }
            FindAction::ReplaceAll => {
                let (new_content, count) =
                    find::replace_all(&file.content, &self.find.query, &self.find.replacement);
                if count > 0 {
                    file.content = new_content;
                    file.content_version = file.content_version.wrapping_add(1);
                }
                self.find.last_status = format!("Replaced {count} occurrence(s)");
            }
        }
    }

    fn handle_search_action(&mut self, action: SearchAction) {
        match action {
            SearchAction::Run => {
                let Some(root) = self.search.root.clone() else {
                    self.search.last_status = "No workspace folder set".into();
                    return;
                };
                let started = std::time::Instant::now();
                let results = search::run_search(&root, &self.search.query, &self.search.options);
                let elapsed = started.elapsed();
                self.search.last_status =
                    format!("{} match(es) in {}ms", results.len(), elapsed.as_millis());
                self.search.results = results;
            }
            SearchAction::Open(path, line, byte_start, byte_end) => {
                self.jump_to_byte_range(path.clone(), byte_start, byte_end);
                self.status_message = format!("{}:{}", path.display(), line);
            }
        }
    }

    pub fn open_git(&mut self, path: &Path) {
        self.git = GitRepo::open(path);
        self.scm = ScmState::default();
        self.refresh_scm();
    }

    pub fn refresh_scm(&mut self) {
        let Some(git) = &self.git else {
            self.scm.statuses.clear();
            self.scm.branches.clear();
            self.scm.commits.clear();
            self.scm.current_branch.clear();
            return;
        };
        match git.statuses() {
            Ok(s) => {
                self.scm.statuses = s;
                self.scm.last_error = None;
            }
            Err(e) => self.scm.last_error = Some(e.to_string()),
        }
        self.scm.branches = git.local_branches();
        self.scm.commits = git.recent_commits(20);
        self.scm.current_branch = git.current_branch();
    }

    fn handle_scm_action(&mut self, action: ScmAction) {
        let Some(git) = &self.git else { return };
        self.scm.last_info = None;
        match action {
            ScmAction::Refresh => self.refresh_scm(),
            ScmAction::Stage(p) => {
                match git.stage(&p) {
                    Ok(()) => self.scm.last_error = None,
                    Err(e) => self.scm.last_error = Some(e.to_string()),
                }
                self.refresh_scm();
            }
            ScmAction::Unstage(p) => {
                match git.unstage(&p) {
                    Ok(()) => self.scm.last_error = None,
                    Err(e) => self.scm.last_error = Some(e.to_string()),
                }
                self.refresh_scm();
            }
            ScmAction::Discard(p) => {
                let entry = self
                    .scm
                    .statuses
                    .iter()
                    .find(|e| !e.staged && e.path == p)
                    .cloned();
                let res = match entry {
                    Some(e) => git.discard(&e),
                    None => Err("entry not found".into()),
                };
                match res {
                    Ok(()) => {
                        self.scm.last_error = None;
                        self.status_message = format!("Discarded {p}");
                    }
                    Err(e) => self.scm.last_error = Some(e),
                }
                self.refresh_scm();
            }
            ScmAction::Select(path, staged) => {
                self.scm.selected = Some((path.clone(), staged));
                match git.diff_for(&path, staged) {
                    Ok(d) => {
                        self.scm.diff = d;
                        self.scm.last_error = None;
                    }
                    Err(e) => {
                        self.scm.diff.clear();
                        self.scm.last_error = Some(e.to_string());
                    }
                }
            }
            ScmAction::Commit => {
                let msg = self.scm.commit_message.trim().to_string();
                if msg.is_empty() {
                    self.scm.last_error = Some("Commit message is empty.".into());
                    return;
                }
                match git.commit(&msg) {
                    Ok(oid) => {
                        let short = oid.to_string();
                        let short = &short[..short.len().min(8)];
                        self.status_message = format!("Committed {short}");
                        self.scm.commit_message.clear();
                        self.scm.last_error = None;
                        self.scm.last_info = Some(format!("Committed {short}"));
                    }
                    Err(e) => self.scm.last_error = Some(e.to_string()),
                }
                self.refresh_scm();
            }
            ScmAction::Push => self.run_remote_op("push"),
            ScmAction::Pull => self.run_remote_op("pull"),
            ScmAction::Fetch => self.run_remote_op("fetch"),
            ScmAction::CheckoutBranch(name) => match git.checkout_branch(&name) {
                Ok(_) => {
                    self.status_message = format!("Checked out {name}");
                    self.scm.last_info = Some(format!("Now on {name}"));
                    self.scm.last_error = None;
                    self.refresh_scm();
                }
                Err(e) => self.scm.last_error = Some(e),
            },
            ScmAction::CreateBranch(name) => match git.create_branch(&name) {
                Ok(_) => {
                    self.status_message = format!("Created and checked out {name}");
                    self.scm.last_info = Some(format!("Created {name}"));
                    self.scm.last_error = None;
                    self.scm.new_branch_name.clear();
                    self.refresh_scm();
                }
                Err(e) => self.scm.last_error = Some(e),
            },
            ScmAction::ResolveOurs(p) => {
                if let Err(e) = git.resolve_ours(&p) {
                    self.scm.last_error = Some(e);
                } else {
                    self.scm.last_info = Some(format!("Resolved {p} (ours)"));
                    self.scm.last_error = None;
                }
                self.refresh_scm();
            }
            ScmAction::ResolveTheirs(p) => {
                if let Err(e) = git.resolve_theirs(&p) {
                    self.scm.last_error = Some(e);
                } else {
                    self.scm.last_info = Some(format!("Resolved {p} (theirs)"));
                    self.scm.last_error = None;
                }
                self.refresh_scm();
            }
            ScmAction::MarkResolved(p) => {
                if let Err(e) = git.mark_resolved(&p) {
                    self.scm.last_error = Some(e);
                } else {
                    self.scm.last_info = Some(format!("Marked resolved: {p}"));
                    self.scm.last_error = None;
                }
                self.refresh_scm();
            }
            ScmAction::BlameActive => {
                let Some(file) = self.tabs.active() else {
                    self.scm.last_error = Some("No active file.".into());
                    return;
                };
                let Some(path) = file.path.clone() else {
                    self.scm.last_error = Some("Active file has no saved path.".into());
                    return;
                };
                let workdir = git.workdir().to_path_buf();
                let rel = match path.strip_prefix(&workdir) {
                    Ok(r) => r.to_string_lossy().to_string(),
                    Err(_) => path.display().to_string(),
                };
                match git.blame(&rel) {
                    Ok(text) => {
                        self.blame_text = text;
                        self.blame_title = format!("Blame: {rel}");
                        self.blame_open = true;
                        self.scm.last_error = None;
                    }
                    Err(e) => self.scm.last_error = Some(e),
                }
            }
        }
    }

    fn run_remote_op(&mut self, op: &str) {
        let Some(git) = &self.git else { return };
        let res = match op {
            "push" => git.push(),
            "pull" => git.pull(),
            "fetch" => git.fetch(),
            _ => return,
        };
        match res {
            Ok(out) => {
                self.scm.last_error = None;
                self.scm.last_info = Some(format!("git {op}:\n{}", out.trim()));
                self.status_message = format!("git {op} ok");
            }
            Err(e) => {
                self.scm.last_info = None;
                self.scm.last_error = Some(format!("git {op}: {e}"));
            }
        }
        self.refresh_scm();
    }
}

impl eframe::App for CodeEditorApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.handle_shortcuts(ctx);
        let events = self.lsp.poll();
        self.handle_lsp_events(events);
        self.sync_lsp_documents();
        ctx.request_repaint_after(std::time::Duration::from_millis(200));

        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New        Ctrl+N").clicked() {
                        self.action_new_file();
                        ui.close_menu();
                    }
                    if ui.button("Open File…    Ctrl+O").clicked() {
                        self.action_open_file();
                        ui.close_menu();
                    }
                    if ui.button("Open Folder…").clicked() {
                        self.action_open_folder();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Save        Ctrl+S").clicked() {
                        self.action_save();
                        ui.close_menu();
                    }
                    if ui.button("Save As…  Ctrl+Shift+S").clicked() {
                        self.action_save_as();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Close Tab    Ctrl+W").clicked() {
                        self.action_close_active();
                        ui.close_menu();
                    }
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui
                        .checkbox(&mut self.show_side_panel, "Side Panel  Ctrl+B")
                        .clicked()
                    {
                        ui.close_menu();
                    }
                    if ui.button("Explorer  Ctrl+Shift+E").clicked() {
                        self.show_side_panel = true;
                        self.side_view = SidePanelView::Explorer;
                        ui.close_menu();
                    }
                    if ui.button("Source Control  Ctrl+Shift+G").clicked() {
                        self.show_side_panel = true;
                        self.side_view = SidePanelView::SourceControl;
                        ui.close_menu();
                    }
                    if ui.button("Search  Ctrl+Shift+F").clicked() {
                        self.show_side_panel = true;
                        self.side_view = SidePanelView::Search;
                        self.search.focus_query = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui
                        .checkbox(&mut self.show_problems, "Problems  Ctrl+Shift+M")
                        .clicked()
                    {
                        ui.close_menu();
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.button("Find  Ctrl+F").clicked() {
                        self.show_find_bar = true;
                        self.find.show_replace = false;
                        self.find.focus_query = true;
                        ui.close_menu();
                    }
                    if ui.button("Replace  Ctrl+H").clicked() {
                        self.show_find_bar = true;
                        self.find.show_replace = true;
                        self.find.focus_query = true;
                        ui.close_menu();
                    }
                    if ui.button("Find in Files  Ctrl+Shift+F").clicked() {
                        self.show_side_panel = true;
                        self.side_view = SidePanelView::Search;
                        self.search.focus_query = true;
                        ui.close_menu();
                    }
                });
                ui.menu_button("Code", |ui| {
                    if ui.button("Go to Definition  F12").clicked() {
                        self.action_goto_definition();
                        ui.close_menu();
                    }
                    if ui.button("Show Hover  Ctrl+Shift+I").clicked() {
                        self.action_show_hover();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui
                        .checkbox(&mut self.format_on_save, "Format on Save")
                        .clicked()
                    {
                        ui.close_menu();
                    }
                });
                ui.menu_button("Help", |ui| {
                    ui.label("Rust Code Editor");
                    ui.label("A small VS Code-style editor in Rust.");
                });
            });
        });

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(git) = &self.git {
                    ui.label(format!("⎇ {}", git.current_branch()));
                    ui.separator();
                }
                let (errs, warns) = self.lsp.total_count();
                if errs + warns > 0 {
                    ui.colored_label(egui::Color32::from_rgb(255, 120, 120), format!("✗ {errs}"));
                    ui.colored_label(egui::Color32::from_rgb(230, 200, 100), format!("⚠ {warns}"));
                    ui.separator();
                }
                ui.label(&self.status_message);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(file) = self.tabs.active() {
                        ui.label(&file.language);
                        ui.separator();
                        ui.label(format!(
                            "Ln {}, Col {}",
                            file.cursor_line + 1,
                            file.cursor_col + 1
                        ));
                    }
                });
            });
        });

        if self.show_problems {
            egui::TopBottomPanel::bottom("problems_panel")
                .resizable(true)
                .default_height(220.0)
                .show(ctx, |ui| {
                    if let Some(action) = problems::show(ui, &self.lsp) {
                        match action {
                            ProblemAction::Open(path, line) => self.jump_to(path, line),
                        }
                    }
                });
        }

        if self.show_side_panel {
            egui::SidePanel::left("side_panel")
                .resizable(true)
                .default_width(280.0)
                .min_width(180.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(
                                self.side_view == SidePanelView::Explorer,
                                "🗂 Explorer",
                            )
                            .clicked()
                        {
                            self.side_view = SidePanelView::Explorer;
                        }
                        if ui
                            .selectable_label(
                                self.side_view == SidePanelView::SourceControl,
                                "⎇ Source Control",
                            )
                            .clicked()
                        {
                            self.side_view = SidePanelView::SourceControl;
                            self.refresh_scm();
                        }
                        if ui
                            .selectable_label(self.side_view == SidePanelView::Search, "🔍 Search")
                            .clicked()
                        {
                            self.side_view = SidePanelView::Search;
                            self.search.focus_query = true;
                        }
                    });
                    ui.separator();
                    match self.side_view {
                        SidePanelView::Explorer => {
                            if let Some(path) = self.explorer.show(ui) {
                                self.open_path(path);
                            }
                        }
                        SidePanelView::SourceControl => {
                            if let Some(action) = self.scm.show(ui, self.git.as_ref()) {
                                self.handle_scm_action(action);
                            }
                        }
                        SidePanelView::Search => {
                            if let Some(action) = self.search.show(ui) {
                                self.handle_search_action(action);
                            }
                        }
                    }
                });
        }

        let mut find_action: Option<FindAction> = None;
        egui::CentralPanel::default().show(ctx, |ui| {
            self.tabs.show_tab_bar(ui);
            ui.separator();
            if self.show_find_bar {
                find_action = self.find.show_bar(ui);
                ui.separator();
            }
            EditorView::show(ui, &mut self.tabs);
        });
        if let Some(action) = find_action {
            self.handle_find_action(action);
        }

        if self.blame_open {
            let mut open = true;
            egui::Window::new(&self.blame_title)
                .open(&mut open)
                .default_size([900.0, 600.0])
                .resizable(true)
                .show(ctx, |ui| {
                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(&self.blame_text).monospace());
                        });
                });
            self.blame_open = open;
        }

        if self.hover_open {
            let mut open = true;
            egui::Window::new("Hover")
                .open(&mut open)
                .default_size([520.0, 320.0])
                .resizable(true)
                .show(ctx, |ui| {
                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new(&self.hover_text).monospace());
                        });
                });
            self.hover_open = open;
        }
    }
}
