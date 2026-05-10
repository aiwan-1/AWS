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

                if response.changed() {
                    file.content_version = file.content_version.wrapping_add(1);
                    let typed = ui.input(|i| {
                        i.events.iter().rev().find_map(|e| match e {
                            egui::Event::Text(t) => t.chars().last(),
                            _ => None,
                        })
                    });
                    file.last_typed_char = typed;
                } else {
                    file.last_typed_char = None;
                }

                if let Some(target_line) = file.goto_line.take() {
                    if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), response.id) {
                        let target = target_line.saturating_sub(1) as usize;
                        let mut idx = 0usize;
                        let mut line = 0usize;
                        for (i, ch) in file.content.char_indices() {
                            if line == target {
                                idx = i;
                                break;
                            }
                            if ch == '\n' {
                                line += 1;
                            }
                            idx = i + ch.len_utf8();
                        }
                        let cursor = egui::text::CCursor::new(file.content[..idx].chars().count());
                        state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::one(cursor)));
                        state.store(ui.ctx(), response.id);
                        response.request_focus();
                    }
                }

                if let Some((start_char, end_char)) = file.goto_range.take() {
                    if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), response.id) {
                        let start = egui::text::CCursor::new(start_char);
                        let end = egui::text::CCursor::new(end_char);
                        state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::two(start, end)));
                        state.store(ui.ctx(), response.id);
                        response.request_focus();
                    }
                }

                if response.has_focus() {
                    if let Some(state) = egui::TextEdit::load_state(ui.ctx(), response.id) {
                        if let Some(range) = state.cursor.char_range() {
                            let primary = range.primary.index;
                            let secondary = range.secondary.index;
                            file.cursor_char = primary;
                            file.cursor_anchor = secondary;
                            let (line, col) = line_col(&file.content, primary);
                            file.cursor_line = line;
                            file.cursor_col = col;
                        }
                    }
                }

                let row_height = ui.text_style_height(&TextStyle::Monospace);
                let glyph_width =
                    ui.fonts(|f| f.glyph_width(&TextStyle::Monospace.resolve(ui.style()), 'm'));
                let origin = response.rect.left_top();
                let pos = origin
                    + egui::vec2(
                        file.cursor_col as f32 * glyph_width,
                        (file.cursor_line as f32 + 1.2) * row_height,
                    );
                file.cursor_screen_pos = Some(pos);
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
