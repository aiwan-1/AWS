use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use egui::{Color32, RichText, Ui};
use walkdir::WalkDir;

const MAX_RESULTS: usize = 1000;
const MAX_FILE_SIZE: u64 = 1_000_000;
const SKIP_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    "dist",
    "build",
    ".venv",
    "venv",
    "__pycache__",
    ".idea",
    ".vscode",
    ".next",
    ".cache",
];

#[derive(Default)]
pub struct SearchOptions {
    pub case_sensitive: bool,
}

#[derive(Clone, Debug)]
pub struct SearchResult {
    pub path: PathBuf,
    pub line: u32,
    pub byte_start: usize,
    pub byte_end: usize,
    pub snippet: String,
}

#[derive(Default)]
pub struct SearchState {
    pub query: String,
    pub options: SearchOptions,
    pub results: Vec<SearchResult>,
    pub last_status: String,
    pub focus_query: bool,
    pub root: Option<PathBuf>,
}

pub enum SearchAction {
    Run,
    Open(PathBuf, u32, usize, usize),
}

impl SearchState {
    pub fn show(&mut self, ui: &mut Ui) -> Option<SearchAction> {
        let mut action = None;

        ui.horizontal(|ui| {
            ui.label(RichText::new("SEARCH").small().strong());
        });
        ui.separator();
        let resp = ui.add(
            egui::TextEdit::singleline(&mut self.query)
                .hint_text("text to find in files")
                .desired_width(f32::INFINITY),
        );
        if self.focus_query {
            resp.request_focus();
            self.focus_query = false;
        }
        if resp.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            action = Some(SearchAction::Run);
        }
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.options.case_sensitive, "Aa")
                .on_hover_text("Case sensitive");
            if ui.button("Search").clicked() {
                action = Some(SearchAction::Run);
            }
            if !self.last_status.is_empty() {
                ui.separator();
                ui.label(RichText::new(&self.last_status).small());
            }
        });
        ui.separator();

        let by_file: Vec<(PathBuf, Vec<&SearchResult>)> = group_by_path(&self.results);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.results.is_empty() {
                    ui.label(RichText::new("No results.").italics());
                    return;
                }
                for (path, group) in &by_file {
                    let display = path
                        .file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.display().to_string());
                    ui.collapsing(format!("{display}  ({})", group.len()), |ui| {
                        for r in group {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!("{:>4}:", r.line))
                                        .monospace()
                                        .color(Color32::GRAY),
                                );
                                let snippet = if r.snippet.len() > 200 {
                                    &r.snippet[..200]
                                } else {
                                    &r.snippet
                                };
                                let resp =
                                    ui.selectable_label(false, RichText::new(snippet).monospace());
                                if resp.clicked() {
                                    action = Some(SearchAction::Open(
                                        r.path.clone(),
                                        r.line,
                                        r.byte_start,
                                        r.byte_end,
                                    ));
                                }
                            });
                        }
                    });
                }
            });

        action
    }
}

fn group_by_path(results: &[SearchResult]) -> Vec<(PathBuf, Vec<&SearchResult>)> {
    let mut out: Vec<(PathBuf, Vec<&SearchResult>)> = Vec::new();
    for r in results {
        match out.last_mut() {
            Some((p, v)) if *p == r.path => v.push(r),
            _ => out.push((r.path.clone(), vec![r])),
        }
    }
    out
}

pub fn run_search(root: &Path, query: &str, opts: &SearchOptions) -> Vec<SearchResult> {
    let mut results = Vec::new();
    if query.is_empty() {
        return results;
    }
    let needle_owned;
    let needle: &str = if opts.case_sensitive {
        query
    } else {
        needle_owned = query.to_lowercase();
        &needle_owned
    };

    let walker = WalkDir::new(root).into_iter().filter_entry(|e| {
        if e.file_type().is_dir() {
            !skipped_dir(e.file_name())
        } else {
            true
        }
    });
    for entry in walker.flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        if entry
            .metadata()
            .map(|m| m.len() > MAX_FILE_SIZE)
            .unwrap_or(true)
        {
            continue;
        }
        let path = entry.path();
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        if bytes[..bytes.len().min(8192)].contains(&0u8) {
            continue;
        }
        let content = match String::from_utf8(bytes) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let haystack_owned;
        let haystack: &str = if opts.case_sensitive {
            &content
        } else {
            haystack_owned = content.to_lowercase();
            &haystack_owned
        };
        if haystack.len() != content.len() {
            continue;
        }

        let mut byte = 0usize;
        while let Some(rel) = haystack[byte..].find(needle) {
            let start = byte + rel;
            let end = start + needle.len();
            let line_start = content[..start].rfind('\n').map(|p| p + 1).unwrap_or(0);
            let line_end = content[start..]
                .find('\n')
                .map(|p| start + p)
                .unwrap_or(content.len());
            let line_no = content[..start].matches('\n').count() as u32 + 1;
            let snippet = content[line_start..line_end].trim_end().to_string();
            results.push(SearchResult {
                path: path.to_path_buf(),
                line: line_no,
                byte_start: start,
                byte_end: end,
                snippet,
            });
            byte = end;
            if results.len() >= MAX_RESULTS {
                return results;
            }
        }
    }
    results
}

fn skipped_dir(name: &OsStr) -> bool {
    name.to_str()
        .map(|s| SKIP_DIRS.contains(&s))
        .unwrap_or(false)
}
