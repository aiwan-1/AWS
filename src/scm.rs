use egui::{Color32, RichText, TextStyle, Ui};

use crate::git::{Change, CommitInfo, GitRepo, StatusEntry};

#[derive(Default)]
pub struct ScmState {
    pub commit_message: String,
    pub statuses: Vec<StatusEntry>,
    pub branches: Vec<String>,
    pub commits: Vec<CommitInfo>,
    pub current_branch: String,
    pub new_branch_name: String,
    pub last_error: Option<String>,
    pub last_info: Option<String>,
    pub selected: Option<(String, bool)>,
    pub diff: String,
}

pub enum ScmAction {
    Refresh,
    Stage(String),
    Unstage(String),
    Discard(String),
    Select(String, bool),
    Commit,
    Push,
    Pull,
    Fetch,
    CheckoutBranch(String),
    CreateBranch(String),
    ResolveOurs(String),
    ResolveTheirs(String),
    MarkResolved(String),
    BlameActive,
}

impl ScmState {
    pub fn show(&mut self, ui: &mut Ui, git: Option<&GitRepo>) -> Option<ScmAction> {
        let Some(_git) = git else {
            ui.label("Not in a git repository.");
            ui.label("Open a folder containing a .git directory.");
            return None;
        };

        let mut action = None;

        ui.horizontal(|ui| {
            ui.label(RichText::new("SOURCE CONTROL").small().strong());
            if ui.small_button("⟳").on_hover_text("Refresh").clicked() {
                action = Some(ScmAction::Refresh);
            }
        });
        ui.separator();

        ui.add(
            egui::TextEdit::multiline(&mut self.commit_message)
                .hint_text("Commit message")
                .desired_rows(2)
                .desired_width(f32::INFINITY),
        );
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("✓ Commit")
                .on_hover_text("Commit staged changes")
                .clicked()
            {
                action = Some(ScmAction::Commit);
            }
            if ui.button("⬇ Pull").clicked() {
                action = Some(ScmAction::Pull);
            }
            if ui.button("⬆ Push").clicked() {
                action = Some(ScmAction::Push);
            }
            if ui.button("↻ Fetch").clicked() {
                action = Some(ScmAction::Fetch);
            }
            if ui
                .button("👁 Blame")
                .on_hover_text("Blame the active file")
                .clicked()
            {
                action = Some(ScmAction::BlameActive);
            }
        });

        if let Some(err) = &self.last_error {
            ui.separator();
            ui.colored_label(Color32::LIGHT_RED, err);
        }
        if let Some(info) = &self.last_info {
            ui.separator();
            ui.colored_label(Color32::LIGHT_GREEN, info);
        }

        ui.separator();

        let staged: Vec<StatusEntry> = self.statuses.iter().filter(|s| s.staged).cloned().collect();
        let conflicted: Vec<StatusEntry> = self
            .statuses
            .iter()
            .filter(|s| s.change == Change::Conflicted)
            .cloned()
            .collect();
        let unstaged: Vec<StatusEntry> = self
            .statuses
            .iter()
            .filter(|s| {
                !s.staged && s.change != Change::Untracked && s.change != Change::Conflicted
            })
            .cloned()
            .collect();
        let untracked: Vec<StatusEntry> = self
            .statuses
            .iter()
            .filter(|s| !s.staged && s.change == Change::Untracked)
            .cloned()
            .collect();

        let selected = self.selected.clone();

        egui::ScrollArea::vertical()
            .id_source("scm_files")
            .max_height(ui.available_height() * 0.5)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if !conflicted.is_empty() {
                    ui.collapsing(
                        RichText::new(format!("Merge Conflicts ({})", conflicted.len()))
                            .color(Color32::from_rgb(255, 120, 120)),
                        |ui| {
                            for entry in &conflicted {
                                render_conflict_entry(ui, entry, &mut action);
                            }
                        },
                    );
                }
                if !staged.is_empty() {
                    ui.collapsing(format!("Staged ({})", staged.len()), |ui| {
                        for entry in &staged {
                            render_entry(ui, entry, &selected, &mut action);
                        }
                    });
                }
                if !unstaged.is_empty() {
                    ui.collapsing(format!("Changes ({})", unstaged.len()), |ui| {
                        for entry in &unstaged {
                            render_entry(ui, entry, &selected, &mut action);
                        }
                    });
                }
                if !untracked.is_empty() {
                    ui.collapsing(format!("Untracked ({})", untracked.len()), |ui| {
                        for entry in &untracked {
                            render_entry(ui, entry, &selected, &mut action);
                        }
                    });
                }
                if staged.is_empty()
                    && unstaged.is_empty()
                    && untracked.is_empty()
                    && conflicted.is_empty()
                {
                    ui.label(RichText::new("No changes.").italics());
                }

                ui.separator();
                ui.collapsing(format!("Branches ({})", self.branches.len()), |ui| {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.new_branch_name)
                                .hint_text("new-branch-name")
                                .desired_width(140.0),
                        );
                        if ui.button("+ Create").clicked() {
                            let name = self.new_branch_name.trim().to_string();
                            if !name.is_empty() {
                                action = Some(ScmAction::CreateBranch(name));
                            }
                        }
                    });
                    ui.separator();
                    for name in &self.branches {
                        let is_current = name == &self.current_branch;
                        ui.horizontal(|ui| {
                            let label = if is_current {
                                RichText::new(format!("● {name}"))
                                    .strong()
                                    .color(Color32::WHITE)
                            } else {
                                RichText::new(format!("  {name}"))
                            };
                            if ui
                                .selectable_label(is_current, label)
                                .on_hover_text(if is_current {
                                    "Current branch"
                                } else {
                                    "Click to checkout"
                                })
                                .clicked()
                                && !is_current
                            {
                                action = Some(ScmAction::CheckoutBranch(name.clone()));
                            }
                        });
                    }
                });

                ui.separator();
                ui.collapsing(format!("Recent Commits ({})", self.commits.len()), |ui| {
                    for c in &self.commits {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(&c.oid_short)
                                    .monospace()
                                    .color(Color32::LIGHT_BLUE),
                            );
                            ui.label(RichText::new(&c.summary));
                        });
                        ui.label(
                            RichText::new(format!("    {} · {}", c.author, c.time))
                                .small()
                                .color(Color32::GRAY),
                        );
                    }
                });
            });

        if let Some((path, staged_sel)) = &self.selected {
            ui.separator();
            ui.label(
                RichText::new(format!(
                    "DIFF: {}{}",
                    if *staged_sel { "[staged] " } else { "" },
                    path
                ))
                .small()
                .strong(),
            );
            egui::ScrollArea::both()
                .id_source("scm_diff")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if self.diff.is_empty() {
                        ui.label(RichText::new("(no diff)").italics());
                    } else {
                        render_diff(ui, &self.diff);
                    }
                });
        }

        action
    }
}

fn render_entry(
    ui: &mut Ui,
    entry: &StatusEntry,
    selected: &Option<(String, bool)>,
    action: &mut Option<ScmAction>,
) {
    ui.horizontal(|ui| {
        let color = match entry.change {
            Change::Added | Change::Untracked => Color32::LIGHT_GREEN,
            Change::Modified | Change::TypeChange => Color32::YELLOW,
            Change::Deleted => Color32::LIGHT_RED,
            Change::Renamed => Color32::LIGHT_BLUE,
            Change::Conflicted => Color32::from_rgb(255, 100, 100),
        };
        ui.colored_label(color, entry.change.glyph());

        let is_selected = selected
            .as_ref()
            .map(|(p, s)| p == &entry.path && *s == entry.staged)
            .unwrap_or(false);
        let resp = ui.selectable_label(is_selected, &entry.path);
        if resp.clicked() {
            *action = Some(ScmAction::Select(entry.path.clone(), entry.staged));
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if entry.staged {
                if ui.small_button("−").on_hover_text("Unstage").clicked() {
                    *action = Some(ScmAction::Unstage(entry.path.clone()));
                }
            } else {
                if ui.small_button("+").on_hover_text("Stage").clicked() {
                    *action = Some(ScmAction::Stage(entry.path.clone()));
                }
                if ui.small_button("↺").on_hover_text("Discard").clicked() {
                    *action = Some(ScmAction::Discard(entry.path.clone()));
                }
            }
        });
    });
}

fn render_conflict_entry(ui: &mut Ui, entry: &StatusEntry, action: &mut Option<ScmAction>) {
    ui.horizontal(|ui| {
        ui.colored_label(Color32::from_rgb(255, 100, 100), "!");
        ui.label(RichText::new(&entry.path).color(Color32::from_rgb(255, 200, 200)));
    });
    ui.horizontal(|ui| {
        ui.add_space(20.0);
        if ui
            .small_button("Use ours")
            .on_hover_text("Keep our version, stage")
            .clicked()
        {
            *action = Some(ScmAction::ResolveOurs(entry.path.clone()));
        }
        if ui
            .small_button("Use theirs")
            .on_hover_text("Take their version, stage")
            .clicked()
        {
            *action = Some(ScmAction::ResolveTheirs(entry.path.clone()));
        }
        if ui
            .small_button("Mark resolved")
            .on_hover_text("Stage as-is")
            .clicked()
        {
            *action = Some(ScmAction::MarkResolved(entry.path.clone()));
        }
    });
}

fn render_diff(ui: &mut Ui, diff: &str) {
    let font = TextStyle::Monospace.resolve(ui.style());
    for line in diff.lines() {
        let color = match line.chars().next() {
            Some('+') if !line.starts_with("+++") => Color32::from_rgb(150, 230, 150),
            Some('-') if !line.starts_with("---") => Color32::from_rgb(230, 150, 150),
            Some('@') => Color32::from_rgb(150, 200, 230),
            _ => ui.visuals().text_color(),
        };
        ui.label(RichText::new(line).font(font.clone()).color(color));
    }
}
