use std::path::{Path, PathBuf};

use egui::{Color32, RichText, Ui};
use lsp_types::{Diagnostic, DiagnosticSeverity};
use url::Url;

use crate::lsp::LspManager;

pub enum ProblemAction {
    Open(PathBuf, u32),
}

pub fn show(ui: &mut Ui, lsp: &LspManager) -> Option<ProblemAction> {
    let mut action: Option<ProblemAction> = None;

    let total: usize = lsp.diagnostics.values().map(|v| v.len()).sum();
    let (errors, warnings) = lsp.total_count();

    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("PROBLEMS ({total})"))
                .small()
                .strong(),
        );
        ui.separator();
        ui.colored_label(Color32::from_rgb(255, 120, 120), format!("✗ {errors}"));
        ui.colored_label(Color32::from_rgb(230, 200, 100), format!("⚠ {warnings}"));
        let langs = lsp.active_languages();
        if !langs.is_empty() {
            ui.separator();
            ui.label(
                RichText::new(format!("servers: {}", langs.join(", ")))
                    .small()
                    .color(Color32::GRAY),
            );
        }
        if let Some(err) = &lsp.last_error {
            ui.separator();
            ui.colored_label(Color32::LIGHT_RED, err);
        }
    });

    ui.separator();

    egui::ScrollArea::vertical()
        .id_source("problems_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if total == 0 {
                ui.label(RichText::new("No problems detected.").italics());
                return;
            }
            let mut entries: Vec<(&Url, &Vec<Diagnostic>)> = lsp.diagnostics.iter().collect();
            entries.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
            for (uri, diags) in entries {
                if diags.is_empty() {
                    continue;
                }
                let path: Option<PathBuf> = uri.to_file_path().ok();
                let display = path
                    .as_deref()
                    .and_then(|p: &std::path::Path| p.file_name())
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| uri.to_string());
                ui.collapsing(format!("{display}  ({})", diags.len()), |ui| {
                    for d in diags {
                        if let Some(p) = &path {
                            render_diag(ui, d, p, &mut action);
                        }
                    }
                });
            }
        });

    action
}

fn render_diag(ui: &mut Ui, d: &Diagnostic, path: &Path, action: &mut Option<ProblemAction>) {
    let (icon, color) = match d.severity {
        Some(DiagnosticSeverity::ERROR) => ("✗", Color32::from_rgb(255, 120, 120)),
        Some(DiagnosticSeverity::WARNING) => ("⚠", Color32::from_rgb(230, 200, 100)),
        Some(DiagnosticSeverity::INFORMATION) => ("ℹ", Color32::from_rgb(120, 180, 230)),
        Some(DiagnosticSeverity::HINT) => ("·", Color32::GRAY),
        _ => ("·", Color32::GRAY),
    };
    let line = d.range.start.line + 1;
    let col = d.range.start.character + 1;
    let summary = d.message.lines().next().unwrap_or("").to_string();

    let resp = ui.horizontal(|ui| {
        ui.colored_label(color, icon);
        ui.label(
            RichText::new(format!("{line}:{col}"))
                .monospace()
                .color(Color32::GRAY),
        );
        ui.label(summary);
    });
    if resp.response.interact(egui::Sense::click()).clicked() {
        *action = Some(ProblemAction::Open(path.to_path_buf(), line));
    }
}
