use egui::{FontId, TextStyle, Ui};
use egui_extras::syntax_highlighting::{highlight, CodeTheme};

use crate::tabs::TabBar;

pub struct EditorView;

impl EditorView {
    pub fn show(ui: &mut Ui, tabs: &mut TabBar) {
        let Some(file) = tabs.active_mut() else {
            ui.centered_and_justified(|ui| {
                ui.label("No file open. Open a file from the explorer or File menu.");
            });
            return;
        };

        let language = file.language.clone();
        let theme = CodeTheme::from_memory(ui.ctx());

        let mut layouter = |ui: &Ui, source: &str, wrap_width: f32| {
            let mut layout_job = highlight(ui.ctx(), &theme, source, &language);
            layout_job.wrap.max_width = wrap_width;
            ui.fonts(|f| f.layout_job(layout_job))
        };

        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let response = ui.add_sized(
                    ui.available_size(),
                    egui::TextEdit::multiline(&mut file.content)
                        .font(TextStyle::Monospace)
                        .code_editor()
                        .desired_rows(20)
                        .lock_focus(true)
                        .desired_width(f32::INFINITY)
                        .layouter(&mut layouter),
                );

                if response.has_focus() {
                    if let Some(state) = egui::TextEdit::load_state(ui.ctx(), response.id) {
                        if let Some(range) = state.cursor.char_range() {
                            let cursor = range.primary.index;
                            let (line, col) = line_col(&file.content, cursor);
                            file.cursor_line = line;
                            file.cursor_col = col;
                        }
                    }
                }
            });

        // Encourage a readable monospace size for the code area.
        let _ = FontId::monospace(14.0);
    }
}

fn line_col(text: &str, char_idx: usize) -> (usize, usize) {
    let mut line = 0usize;
    let mut col = 0usize;
    for (i, ch) in text.chars().enumerate() {
        if i == char_idx {
            return (line, col);
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}
