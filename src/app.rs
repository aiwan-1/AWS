use std::path::{Path, PathBuf};

use eframe::CreationContext;
use egui::{Context, Key, Modifiers};

use crate::editor::EditorView;
use crate::explorer::FileExplorer;
use crate::git::GitRepo;
use crate::scm::{ScmAction, ScmState};
use crate::tabs::{OpenFile, TabBar};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SidePanelView {
    Explorer,
    SourceControl,
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
        };
        if let Ok(cwd) = std::env::current_dir() {
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
        });
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
            self.open_git(&path);
            self.status_message = format!("Opened folder {}", path.display());
        }
    }

    pub fn action_save(&mut self) {
        match self.tabs.save_active() {
            Ok(Some(path)) => self.status_message = format!("Saved {}", path.display()),
            Ok(None) => self.action_save_as(),
            Err(e) => self.status_message = format!("Save failed: {e}"),
        }
        self.refresh_scm();
    }

    pub fn action_save_as(&mut self) {
        if let Some(path) = rfd::FileDialog::new().save_file() {
            match self.tabs.save_active_as(&path) {
                Ok(()) => self.status_message = format!("Saved {}", path.display()),
                Err(e) => self.status_message = format!("Save failed: {e}"),
            }
            self.refresh_scm();
        }
    }

    pub fn action_close_active(&mut self) {
        self.tabs.close_active();
    }

    pub fn open_path(&mut self, path: PathBuf) {
        match OpenFile::from_path(&path) {
            Ok(file) => {
                self.tabs.add_file(file);
                self.status_message = format!("Opened {}", path.display());
            }
            Err(e) => self.status_message = format!("Open failed: {e}"),
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
                    }
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            self.tabs.show_tab_bar(ui);
            ui.separator();
            EditorView::show(ui, &mut self.tabs);
        });

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
    }
}
