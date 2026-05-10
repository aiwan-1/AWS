use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use egui::{CollapsingHeader, Ui};

pub struct FileExplorer {
    root: Option<PathBuf>,
    expanded: HashSet<PathBuf>,
}

impl FileExplorer {
    pub fn new() -> Self {
        Self {
            root: None,
            expanded: HashSet::new(),
        }
    }

    pub fn set_root(&mut self, root: PathBuf) {
        self.expanded.clear();
        self.expanded.insert(root.clone());
        self.root = Some(root);
    }

    /// Render the explorer. Returns a path to open if the user clicked a file.
    pub fn show(&mut self, ui: &mut Ui) -> Option<PathBuf> {
        let mut to_open: Option<PathBuf> = None;

        match self.root.clone() {
            None => {
                ui.label("No folder opened.");
                ui.label("Use File → Open Folder…");
            }
            Some(root) => {
                let label = root
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| root.display().to_string());
                ui.label(egui::RichText::new(label).strong());
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.render_dir(ui, &root, &mut to_open, 0);
                });
            }
        }

        to_open
    }

    fn render_dir(&mut self, ui: &mut Ui, dir: &Path, to_open: &mut Option<PathBuf>, depth: usize) {
        let entries = match read_sorted(dir) {
            Ok(e) => e,
            Err(e) => {
                ui.colored_label(egui::Color32::LIGHT_RED, format!("error: {e}"));
                return;
            }
        };

        for entry in entries {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);

            ui.horizontal(|ui| {
                ui.add_space((depth as f32) * 8.0);
                if is_dir {
                    let id = ui.make_persistent_id(&path);
                    let mut open = self.expanded.contains(&path);
                    let header = CollapsingHeader::new(format!("📁 {name}"))
                        .id_source(id)
                        .default_open(false)
                        .open(Some(open));
                    let resp = header.show(ui, |ui| {
                        self.render_dir(ui, &path, to_open, depth + 1);
                    });
                    if resp.header_response.clicked() {
                        open = !open;
                        if open {
                            self.expanded.insert(path.clone());
                        } else {
                            self.expanded.remove(&path);
                        }
                    }
                } else {
                    let icon = file_icon(&path);
                    if ui
                        .selectable_label(false, format!("{icon} {name}"))
                        .clicked()
                    {
                        *to_open = Some(path.clone());
                    }
                }
            });
        }
    }
}

fn read_sorted(dir: &Path) -> std::io::Result<Vec<fs::DirEntry>> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| {
        let is_dir = e.file_type().map(|t| !t.is_dir()).unwrap_or(true);
        let name = e.file_name().to_string_lossy().to_lowercase();
        (is_dir, name)
    });
    Ok(entries)
}

fn file_icon(path: &Path) -> &'static str {
    match path.extension().and_then(|s| s.to_str()) {
        Some("rs") => "🦀",
        Some("md") => "📝",
        Some("toml") | Some("yaml") | Some("yml") | Some("json") => "⚙",
        Some("png") | Some("jpg") | Some("jpeg") | Some("gif") | Some("svg") => "🖼",
        _ => "📄",
    }
}
