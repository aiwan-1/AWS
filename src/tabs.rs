use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use egui::{Color32, RichText, Ui};

pub struct OpenFile {
    pub path: Option<PathBuf>,
    pub display_name: String,
    pub content: String,
    pub on_disk: String,
    pub language: String,
    pub cursor_line: usize,
    pub cursor_col: usize,
}

impl OpenFile {
    pub fn untitled() -> Self {
        Self {
            path: None,
            display_name: String::from("untitled"),
            content: String::new(),
            on_disk: String::new(),
            language: String::from("plain"),
            cursor_line: 0,
            cursor_col: 0,
        }
    }

    pub fn from_path(path: &Path) -> io::Result<Self> {
        let content = fs::read_to_string(path)?;
        let display_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let language = detect_language(path);
        Ok(Self {
            path: Some(path.to_path_buf()),
            display_name,
            on_disk: content.clone(),
            content,
            language,
            cursor_line: 0,
            cursor_col: 0,
        })
    }

    pub fn is_dirty(&self) -> bool {
        self.content != self.on_disk
    }

    pub fn save_to(&mut self, path: &Path) -> io::Result<()> {
        fs::write(path, &self.content)?;
        self.on_disk = self.content.clone();
        self.path = Some(path.to_path_buf());
        self.display_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        self.language = detect_language(path);
        Ok(())
    }
}

#[derive(Default)]
pub struct TabBar {
    pub files: Vec<OpenFile>,
    pub active: Option<usize>,
}

impl TabBar {
    pub fn add_file(&mut self, file: OpenFile) {
        if let Some(path) = &file.path {
            if let Some(idx) = self
                .files
                .iter()
                .position(|f| f.path.as_deref() == Some(path.as_path()))
            {
                self.active = Some(idx);
                return;
            }
        }
        self.files.push(file);
        self.active = Some(self.files.len() - 1);
    }

    pub fn active(&self) -> Option<&OpenFile> {
        self.active.and_then(|i| self.files.get(i))
    }

    pub fn active_mut(&mut self) -> Option<&mut OpenFile> {
        match self.active {
            Some(i) => self.files.get_mut(i),
            None => None,
        }
    }

    pub fn close_active(&mut self) {
        let Some(idx) = self.active else { return; };
        self.files.remove(idx);
        if self.files.is_empty() {
            self.active = None;
        } else if idx >= self.files.len() {
            self.active = Some(self.files.len() - 1);
        } else {
            self.active = Some(idx);
        }
    }

    pub fn save_active(&mut self) -> io::Result<Option<PathBuf>> {
        let Some(file) = self.active_mut() else {
            return Ok(None);
        };
        let Some(path) = file.path.clone() else {
            return Ok(None);
        };
        file.save_to(&path)?;
        Ok(Some(path))
    }

    pub fn save_active_as(&mut self, path: &Path) -> io::Result<()> {
        let Some(file) = self.active_mut() else {
            return Ok(());
        };
        file.save_to(path)
    }

    pub fn show_tab_bar(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            let mut to_close: Option<usize> = None;
            let mut to_activate: Option<usize> = None;

            for (i, file) in self.files.iter().enumerate() {
                let is_active = self.active == Some(i);
                let dirty = file.is_dirty();
                let label = if dirty {
                    format!("● {}", file.display_name)
                } else {
                    file.display_name.clone()
                };
                let mut text = RichText::new(label);
                if is_active {
                    text = text.strong().color(Color32::WHITE);
                }
                let resp = ui.selectable_label(is_active, text);
                if resp.clicked() {
                    to_activate = Some(i);
                }
                if ui.small_button("✕").clicked() {
                    to_close = Some(i);
                }
                ui.separator();
            }

            if let Some(i) = to_activate {
                self.active = Some(i);
            }
            if let Some(i) = to_close {
                self.files.remove(i);
                if self.files.is_empty() {
                    self.active = None;
                } else {
                    let new_active = self.active.map(|a| {
                        if a == i && a >= self.files.len() {
                            self.files.len() - 1
                        } else if a > i {
                            a - 1
                        } else {
                            a
                        }
                    });
                    self.active = new_active;
                }
            }
        });
    }
}

fn detect_language(path: &Path) -> String {
    match path.extension().and_then(|s| s.to_str()) {
        Some("rs") => "rs",
        Some("py") => "py",
        Some("js") => "js",
        Some("ts") => "ts",
        Some("jsx") => "js",
        Some("tsx") => "ts",
        Some("c") | Some("h") => "c",
        Some("cpp") | Some("cc") | Some("hpp") | Some("hh") => "cpp",
        Some("go") => "go",
        Some("java") => "java",
        Some("kt") => "kt",
        Some("rb") => "rb",
        Some("sh") | Some("bash") => "sh",
        Some("md") => "md",
        Some("toml") => "toml",
        Some("yaml") | Some("yml") => "yaml",
        Some("json") => "json",
        Some("html") => "html",
        Some("css") => "css",
        Some("xml") => "xml",
        Some("sql") => "sql",
        _ => "txt",
    }
    .to_string()
}
