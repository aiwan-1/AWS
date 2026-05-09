use egui::{Color32, Key, RichText};
use lsp_types::{CompletionItem, CompletionTextEdit, Position, SignatureHelp};

#[derive(Default)]
pub struct CompletionState {
    pub open: bool,
    pub items: Vec<CompletionItem>,
    pub filtered: Vec<usize>,
    pub selected: usize,
    pub anchor: Option<egui::Pos2>,
    pub invocation_id: u64,
    pub typed_prefix: String,
    pub invoked_at: Option<Position>,
}

#[derive(Default)]
pub struct SignatureHelpState {
    pub open: bool,
    pub help: Option<SignatureHelp>,
}

pub enum CompletionAction {
    Accept(Box<CompletionItem>),
    Cancel,
}

impl CompletionState {
    pub fn next_invocation(&mut self) -> u64 {
        self.invocation_id = self.invocation_id.wrapping_add(1);
        self.invocation_id
    }

    pub fn set_items(&mut self, items: Vec<CompletionItem>, prefix: &str) {
        self.items = items;
        self.typed_prefix = prefix.to_string();
        self.refilter();
        self.selected = 0;
        self.open = !self.filtered.is_empty();
    }

    pub fn refilter(&mut self) {
        let prefix = self.typed_prefix.to_lowercase();
        self.filtered = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, it)| {
                if prefix.is_empty() {
                    return true;
                }
                let label = &it.label;
                let filter = it.filter_text.as_deref().unwrap_or(label);
                filter.to_lowercase().contains(&prefix)
            })
            .map(|(i, _)| i)
            .collect();
        if self.selected >= self.filtered.len() {
            self.selected = 0;
        }
    }

    pub fn close(&mut self) {
        self.open = false;
        self.items.clear();
        self.filtered.clear();
        self.selected = 0;
        self.anchor = None;
        self.invoked_at = None;
        self.typed_prefix.clear();
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<CompletionAction> {
        if !self.open || self.filtered.is_empty() {
            return None;
        }
        let anchor = self.anchor.unwrap_or_else(|| egui::pos2(50.0, 100.0));
        let mut action: Option<CompletionAction> = None;

        // Handle keyboard navigation BEFORE the area renders so it
        // intercepts arrows/enter/esc even when focus is in the editor.
        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, Key::ArrowDown)
                && !self.filtered.is_empty()
            {
                self.selected = (self.selected + 1) % self.filtered.len();
            }
            if i.consume_key(egui::Modifiers::NONE, Key::ArrowUp)
                && !self.filtered.is_empty()
            {
                self.selected = if self.selected == 0 {
                    self.filtered.len() - 1
                } else {
                    self.selected - 1
                };
            }
            if i.consume_key(egui::Modifiers::NONE, Key::Escape) {
                action = Some(CompletionAction::Cancel);
            }
            if i.consume_key(egui::Modifiers::NONE, Key::Enter)
                || i.consume_key(egui::Modifiers::NONE, Key::Tab)
            {
                if let Some(idx) = self.filtered.get(self.selected).copied() {
                    if let Some(item) = self.items.get(idx) {
                        action = Some(CompletionAction::Accept(Box::new(item.clone())));
                    }
                }
            }
        });

        let area_id = egui::Id::new("completion_popup");
        let response = egui::Area::new(area_id)
            .fixed_pos(anchor)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(ui.visuals().window_fill)
                    .show(ui, |ui| {
                        ui.set_max_width(380.0);
                        ui.set_max_height(260.0);
                        egui::ScrollArea::vertical()
                            .max_height(260.0)
                            .show(ui, |ui| {
                                let len = self.filtered.len();
                                for (n, idx) in self.filtered.iter().enumerate().take(80) {
                                    if let Some(item) = self.items.get(*idx) {
                                        let kind = kind_glyph(item);
                                        let row = format!(
                                            "{}  {}{}",
                                            kind,
                                            item.label,
                                            item.detail
                                                .as_deref()
                                                .map(|d| format!("  ·  {d}"))
                                                .unwrap_or_default()
                                        );
                                        let resp = ui.selectable_label(n == self.selected, row);
                                        if resp.clicked() {
                                            action = Some(CompletionAction::Accept(Box::new(
                                                item.clone(),
                                            )));
                                        }
                                    }
                                }
                                if len == 0 {
                                    ui.label(RichText::new("No completions").italics());
                                }
                            });
                    });
            });
        let _ = response;

        action
    }
}

impl SignatureHelpState {
    pub fn set(&mut self, help: SignatureHelp) {
        self.help = Some(help);
        self.open = true;
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let Some(help) = self.help.clone() else {
            return;
        };
        let mut open = true;
        egui::Window::new("Signature Help")
            .open(&mut open)
            .default_size([520.0, 200.0])
            .resizable(true)
            .show(ctx, |ui| {
                let active_sig = help.active_signature.unwrap_or(0) as usize;
                for (i, sig) in help.signatures.iter().enumerate() {
                    let is_active = i == active_sig;
                    let mut text = RichText::new(&sig.label).monospace();
                    if is_active {
                        text = text.strong().color(Color32::WHITE);
                    }
                    ui.label(text);
                    if let Some(doc) = &sig.documentation {
                        let s = match doc {
                            lsp_types::Documentation::String(s) => s.clone(),
                            lsp_types::Documentation::MarkupContent(m) => m.value.clone(),
                        };
                        ui.label(RichText::new(s).small().color(Color32::GRAY));
                    }
                    ui.separator();
                }
            });
        self.open = open;
    }
}

fn kind_glyph(item: &CompletionItem) -> &'static str {
    use lsp_types::CompletionItemKind as K;
    match item.kind {
        Some(K::FUNCTION) | Some(K::METHOD) => "ƒ",
        Some(K::VARIABLE) => "α",
        Some(K::CLASS) | Some(K::STRUCT) => "C",
        Some(K::INTERFACE) | Some(K::ENUM) => "E",
        Some(K::FIELD) | Some(K::PROPERTY) => "·",
        Some(K::KEYWORD) => "K",
        Some(K::SNIPPET) => "❖",
        Some(K::CONSTANT) => "ᴋ",
        Some(K::MODULE) => "M",
        _ => "▢",
    }
}

/// Compute the text to insert and the (start_char, end_char) range to replace.
/// Returns (insert_text, replace_start_char, replace_end_char).
pub fn resolve_insertion(
    item: &CompletionItem,
    content: &str,
    cursor_char: usize,
    fallback_prefix_len: usize,
) -> (String, usize, usize) {
    if let Some(edit) = &item.text_edit {
        match edit {
            CompletionTextEdit::Edit(e) => {
                let s = crate::lsp::position_to_char_idx(content, e.range.start);
                let en = crate::lsp::position_to_char_idx(content, e.range.end);
                return (e.new_text.clone(), s, en);
            }
            CompletionTextEdit::InsertAndReplace(e) => {
                let s = crate::lsp::position_to_char_idx(content, e.replace.start);
                let en = crate::lsp::position_to_char_idx(content, e.replace.end);
                return (e.new_text.clone(), s, en);
            }
        }
    }
    let text = item
        .insert_text
        .clone()
        .unwrap_or_else(|| item.label.clone());
    let start = cursor_char.saturating_sub(fallback_prefix_len);
    (text, start, cursor_char)
}
