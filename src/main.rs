// Rust Code Editor - a Visual Studio-esque code editor built with egui.
//
// Features:
//   * File explorer (tree view with expand/collapse)
//   * Tabbed editor with dirty-state indicators
//   * Syntax highlighting via egui_extras + syntect
//   * Menu bar (File: New, Open, Open Folder, Save, Save As, Close)
//   * Status bar with cursor info and messages
//   * Keyboard shortcuts (Ctrl+S, Ctrl+W, Ctrl+N, Ctrl+O)

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod editor;
mod explorer;
mod tabs;

use app::CodeEditorApp;

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([640.0, 400.0])
            .with_title("Rust Code Editor"),
        ..Default::default()
    };

    eframe::run_native(
        "Rust Code Editor",
        options,
        Box::new(|cc| Box::new(CodeEditorApp::new(cc))),
    )
}
