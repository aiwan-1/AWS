use egui::{Key, RichText, Ui};

#[derive(Default)]
pub struct FindState {
    pub query: String,
    pub replacement: String,
    pub case_sensitive: bool,
    pub show_replace: bool,
    pub focus_query: bool,
    pub last_status: String,
}

pub enum FindAction {
    Next,
    Prev,
    Replace,
    ReplaceAll,
    Close,
}

impl FindState {
    pub fn show_bar(&mut self, ui: &mut Ui) -> Option<FindAction> {
        let mut action = None;

        ui.horizontal(|ui| {
            ui.label(RichText::new("Find").small().strong());
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .hint_text("text to find")
                    .desired_width(260.0),
            );
            if self.focus_query {
                resp.request_focus();
                self.focus_query = false;
            }
            if resp.has_focus() {
                ui.input(|i| {
                    if i.key_pressed(Key::Enter) {
                        action = if i.modifiers.shift {
                            Some(FindAction::Prev)
                        } else {
                            Some(FindAction::Next)
                        };
                    }
                });
            }
            if ui
                .small_button("◀")
                .on_hover_text("Previous (Shift+Enter)")
                .clicked()
            {
                action = Some(FindAction::Prev);
            }
            if ui.small_button("▶").on_hover_text("Next (Enter)").clicked() {
                action = Some(FindAction::Next);
            }
            ui.checkbox(&mut self.case_sensitive, "Aa")
                .on_hover_text("Case sensitive");
            ui.checkbox(&mut self.show_replace, "Replace");
            if ui.small_button("✕").on_hover_text("Close (Esc)").clicked() {
                action = Some(FindAction::Close);
            }
            if !self.last_status.is_empty() {
                ui.separator();
                ui.label(RichText::new(&self.last_status).small());
            }
        });

        if self.show_replace {
            ui.horizontal(|ui| {
                ui.label(RichText::new("With").small().strong());
                ui.add(
                    egui::TextEdit::singleline(&mut self.replacement)
                        .hint_text("replacement")
                        .desired_width(260.0),
                );
                if ui
                    .button("Replace")
                    .on_hover_text("Replace current match")
                    .clicked()
                {
                    action = Some(FindAction::Replace);
                }
                if ui
                    .button("Replace All")
                    .on_hover_text("Replace every occurrence in this file")
                    .clicked()
                {
                    action = Some(FindAction::ReplaceAll);
                }
            });
        }

        action
    }
}

/// Find next/prev byte range of `query` in `haystack` starting from `from_byte`.
/// Forward search wraps to the start; backward wraps to the end.
pub fn find_byte_range(
    haystack: &str,
    query: &str,
    case_sensitive: bool,
    from_byte: usize,
    forward: bool,
) -> Option<(usize, usize)> {
    if query.is_empty() {
        return None;
    }
    let (h_owned, n_owned);
    let (h, n): (&str, &str) = if case_sensitive {
        (haystack, query)
    } else {
        h_owned = haystack.to_lowercase();
        n_owned = query.to_lowercase();
        (&h_owned, &n_owned)
    };
    if h.len() != haystack.len() {
        // Lowercasing changed byte length (non-ASCII case folding) — fall back
        // to case-sensitive search so byte indices remain valid.
        return find_byte_range_inner(haystack, query, from_byte, forward);
    }
    find_byte_range_inner(h, n, from_byte, forward)
}

fn find_byte_range_inner(
    h: &str,
    n: &str,
    from_byte: usize,
    forward: bool,
) -> Option<(usize, usize)> {
    let from = from_byte.min(h.len());
    let pos = if forward {
        h[from..].find(n).map(|p| from + p).or_else(|| h.find(n))
    } else {
        h[..from].rfind(n).or_else(|| h.rfind(n))
    }?;
    Some((pos, pos + n.len()))
}

pub fn byte_to_char(s: &str, byte_idx: usize) -> usize {
    s[..byte_idx.min(s.len())].chars().count()
}

pub fn char_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

pub fn replace_all(content: &str, query: &str, replacement: &str) -> (String, usize) {
    if query.is_empty() {
        return (content.to_string(), 0);
    }
    let count = content.matches(query).count();
    (content.replace(query, replacement), count)
}
