use std::path::PathBuf;

use eframe::CreationContext;
use egui::{Context, Key, Modifiers};

use crate::editor::EditorView;
use crate::explorer::FileExplorer;
use crate::tabs::{OpenFile, TabBar};

pub struct CodeEditorApp {
    pub explorer: FileExplorer,
    pub tabs: TabBar,
    pub status_message: String,
    pub show_explorer: bool,
}

impl CodeEditorApp {
    pub fn new(_cc: &CreationContext<'_>) -> Self {
        Self {
            explorer: FileExplorer::new(),
            tabs: TabBar::default(),
            status_message: String::from("Ready"),
            show_explorer: true,
        }
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
                self.show_explorer = !self.show_explorer;
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
            self.status_message = format!("Opened folder {}", path.display());
        }
    }

    pub fn action_save(&mut self) {
        match self.tabs.save_active() {
            Ok(Some(path)) => self.status_message = format!("Saved {}", path.display()),
            Ok(None) => self.action_save_as(),
            Err(e) => self.status_message = format!("Save failed: {e}"),
        }
    }

    pub fn action_save_as(&mut self) {
        if let Some(path) = rfd::FileDialog::new().save_file() {
            match self.tabs.save_active_as(&path) {
                Ok(()) => self.status_message = format!("Saved {}", path.display()),
                Err(e) => self.status_message = format!("Save failed: {e}"),
            }
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
                        .checkbox(&mut self.show_explorer, "Explorer  Ctrl+B")
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

        if self.show_explorer {
            egui::SidePanel::left("explorer")
                .resizable(true)
                .default_width(220.0)
                .min_width(140.0)
                .show(ctx, |ui| {
                    ui.heading("Explorer");
                    ui.separator();
                    let to_open = self.explorer.show(ui);
                    if let Some(path) = to_open {
                        self.open_path(path);
                    }
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            self.tabs.show_tab_bar(ui);
            ui.separator();
            EditorView::show(ui, &mut self.tabs);
        });
    }
}
