use std::io::{Read, Write};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use egui::{Key, RichText, TextStyle, Ui};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};

const MAX_BUFFER: usize = 200_000;

pub struct Terminal {
    _master: Box<dyn portable_pty::MasterPty + Send>,
    _child: Box<dyn portable_pty::Child + Send + Sync>,
    writer: Box<dyn Write + Send>,
    rx: Receiver<Vec<u8>>,
    pub buffer: String,
    pub input: String,
    pub focus_input: bool,
}

impl Terminal {
    pub fn spawn() -> Result<Self, String> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 30,
                cols: 100,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| e.to_string())?;
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        let cmd = CommandBuilder::new(shell);
        let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        Ok(Self {
            _master: pair.master,
            _child: child,
            writer,
            rx,
            buffer: String::new(),
            input: String::new(),
            focus_input: true,
        })
    }

    pub fn drain(&mut self) {
        while let Ok(bytes) = self.rx.try_recv() {
            let s = String::from_utf8_lossy(&bytes);
            append_ansi_stripped(&mut self.buffer, &s);
        }
        if self.buffer.len() > MAX_BUFFER {
            let cut = self.buffer.len() - (MAX_BUFFER / 2);
            // Cut at a char boundary.
            let safe = (cut..self.buffer.len())
                .find(|&i| self.buffer.is_char_boundary(i))
                .unwrap_or(self.buffer.len());
            self.buffer = self.buffer[safe..].to_string();
        }
    }

    pub fn send(&mut self, data: &[u8]) {
        let _ = self.writer.write_all(data);
        let _ = self.writer.flush();
    }

    pub fn show(&mut self, ui: &mut Ui) {
        self.drain();
        ui.horizontal(|ui| {
            ui.label(RichText::new("TERMINAL").small().strong());
            if ui.small_button("Clear").clicked() {
                self.buffer.clear();
            }
        });
        ui.separator();
        let avail = ui.available_height();
        egui::ScrollArea::vertical()
            .id_source("terminal_scroll")
            .stick_to_bottom(true)
            .max_height((avail - 32.0).max(60.0))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.label(RichText::new(&self.buffer).monospace());
            });
        ui.horizontal(|ui| {
            ui.label("$");
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.input)
                    .desired_width(f32::INFINITY)
                    .font(TextStyle::Monospace)
                    .hint_text("type a command and press Enter"),
            );
            if self.focus_input {
                resp.request_focus();
                self.focus_input = false;
            }
            if resp.has_focus() {
                let send_enter = ui.input(|i| i.key_pressed(Key::Enter));
                if send_enter {
                    let line = std::mem::take(&mut self.input);
                    let mut bytes = line.into_bytes();
                    bytes.push(b'\n');
                    self.send(&bytes);
                    resp.request_focus();
                }
            }
        });
    }
}

fn append_ansi_stripped(buf: &mut String, s: &str) {
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => match chars.next() {
                Some('[') => {
                    for c2 in chars.by_ref() {
                        if c2.is_ascii_alphabetic() || c2 == '~' {
                            break;
                        }
                    }
                }
                Some(']') => {
                    while let Some(c2) = chars.next() {
                        if c2 == '\x07' {
                            break;
                        }
                        if c2 == '\x1b' {
                            chars.next();
                            break;
                        }
                    }
                }
                Some(_) | None => {}
            },
            '\r' => {}
            '\x07' | '\x08' => {}
            _ => buf.push(c),
        }
    }
}
