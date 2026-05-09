use std::path::{Path, PathBuf};
use std::process::Command;

use git2::{DiffFormat, DiffOptions, ObjectType, Repository, StatusOptions};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub enum Change {
    Added,
    Modified,
    Deleted,
    Renamed,
    TypeChange,
    Untracked,
    Conflicted,
}

impl Change {
    pub fn glyph(self) -> &'static str {
        match self {
            Change::Added => "A",
            Change::Modified => "M",
            Change::Deleted => "D",
            Change::Renamed => "R",
            Change::TypeChange => "T",
            Change::Untracked => "U",
            Change::Conflicted => "!",
        }
    }
}

#[derive(Clone, Debug)]
pub struct StatusEntry {
    pub path: String,
    pub change: Change,
    pub staged: bool,
}

pub struct GitRepo {
    repo: Repository,
    workdir: PathBuf,
}

impl GitRepo {
    pub fn open(path: &Path) -> Option<Self> {
        let repo = Repository::discover(path).ok()?;
        let workdir = repo.workdir()?.to_path_buf();
        Some(Self { repo, workdir })
    }

    pub fn current_branch(&self) -> String {
        match self.repo.head() {
            Ok(head) => {
                if let Some(name) = head.shorthand() {
                    name.to_string()
                } else if let Some(oid) = head.target() {
                    let s = oid.to_string();
                    format!("({})", &s[..7.min(s.len())])
                } else {
                    "(no branch)".into()
                }
            }
            Err(_) => "(no commits)".into(),
        }
    }

    pub fn statuses(&self) -> Result<Vec<StatusEntry>, git2::Error> {
        let mut opts = StatusOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(true)
            .renames_head_to_index(true)
            .renames_index_to_workdir(true);
        let statuses = self.repo.statuses(Some(&mut opts))?;
        let mut out = Vec::new();
        for s in statuses.iter() {
            let path = match s.path() {
                Some(p) => p.to_string(),
                None => continue,
            };
            let st = s.status();

            if st.is_conflicted() {
                out.push(StatusEntry {
                    path: path.clone(),
                    change: Change::Conflicted,
                    staged: false,
                });
                continue;
            }

            let staged_change = if st.is_index_new() {
                Some(Change::Added)
            } else if st.is_index_modified() {
                Some(Change::Modified)
            } else if st.is_index_deleted() {
                Some(Change::Deleted)
            } else if st.is_index_renamed() {
                Some(Change::Renamed)
            } else if st.is_index_typechange() {
                Some(Change::TypeChange)
            } else {
                None
            };
            let unstaged_change = if st.is_wt_new() {
                Some(Change::Untracked)
            } else if st.is_wt_modified() {
                Some(Change::Modified)
            } else if st.is_wt_deleted() {
                Some(Change::Deleted)
            } else if st.is_wt_renamed() {
                Some(Change::Renamed)
            } else if st.is_wt_typechange() {
                Some(Change::TypeChange)
            } else {
                None
            };

            if let Some(c) = staged_change {
                out.push(StatusEntry {
                    path: path.clone(),
                    change: c,
                    staged: true,
                });
            }
            if let Some(c) = unstaged_change {
                out.push(StatusEntry {
                    path,
                    change: c,
                    staged: false,
                });
            }
        }
        out.sort_by(|a, b| b.staged.cmp(&a.staged).then_with(|| a.path.cmp(&b.path)));
        Ok(out)
    }

    pub fn stage(&self, path: &str) -> Result<(), git2::Error> {
        let mut index = self.repo.index()?;
        let abs = self.workdir.join(path);
        if abs.exists() {
            index.add_path(Path::new(path))?;
        } else {
            index.remove_path(Path::new(path))?;
        }
        index.write()?;
        Ok(())
    }

    pub fn unstage(&self, path: &str) -> Result<(), git2::Error> {
        let head = self
            .repo
            .head()
            .ok()
            .and_then(|h| h.peel(ObjectType::Commit).ok());
        match head {
            Some(obj) => self.repo.reset_default(Some(&obj), [path]),
            None => {
                let mut index = self.repo.index()?;
                index.remove_path(Path::new(path))?;
                index.write()?;
                Ok(())
            }
        }
    }

    pub fn discard(&self, entry: &StatusEntry) -> Result<(), String> {
        if entry.change == Change::Untracked {
            std::fs::remove_file(self.workdir.join(&entry.path)).map_err(|e| e.to_string())
        } else {
            self.shell_git(&["checkout", "HEAD", "--", &entry.path])
                .map(|_| ())
        }
    }

    pub fn commit(&self, message: &str) -> Result<git2::Oid, git2::Error> {
        let sig = self.repo.signature()?;
        let mut index = self.repo.index()?;
        let tree_oid = index.write_tree()?;
        let tree = self.repo.find_tree(tree_oid)?;
        let parent_commit = self.repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent_commit.iter().collect();
        self.repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
    }

    pub fn diff_for(&self, path: &str, staged: bool) -> Result<String, git2::Error> {
        let mut opts = DiffOptions::new();
        opts.pathspec(path)
            .include_untracked(true)
            .recurse_untracked_dirs(true);
        let diff = if staged {
            let head_tree = self.repo.head().ok().and_then(|h| h.peel_to_tree().ok());
            self.repo
                .diff_tree_to_index(head_tree.as_ref(), None, Some(&mut opts))?
        } else {
            self.repo.diff_index_to_workdir(None, Some(&mut opts))?
        };
        let mut out = String::new();
        diff.print(DiffFormat::Patch, |_d, _h, line| {
            let origin = line.origin();
            if matches!(origin, '+' | '-' | ' ') {
                out.push(origin);
            }
            out.push_str(std::str::from_utf8(line.content()).unwrap_or(""));
            true
        })?;
        Ok(out)
    }

    pub fn shell_git(&self, args: &[&str]) -> Result<String, String> {
        let out = Command::new("git")
            .args(args)
            .current_dir(&self.workdir)
            .output()
            .map_err(|e| e.to_string())?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        if out.status.success() {
            Ok(if stdout.is_empty() { stderr } else { stdout })
        } else {
            Err(if stderr.is_empty() { stdout } else { stderr })
        }
    }

    pub fn fetch(&self) -> Result<String, String> {
        self.shell_git(&["fetch", "--all", "--prune"])
    }

    pub fn pull(&self) -> Result<String, String> {
        self.shell_git(&["pull", "--ff-only"])
    }

    pub fn push(&self) -> Result<String, String> {
        self.shell_git(&["push"])
    }
}
