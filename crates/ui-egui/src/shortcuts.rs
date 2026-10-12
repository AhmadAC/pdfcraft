//! Global keyboard shortcut dispatcher and keymapper for PdfCraft.
//! Maps egui keyboard inputs against engine commands in `COMMANDS` and
//! provides dedicated handling for `Alt + P` (Command Palette) and `Alt + Z` (Zero-Score Highlighter).

use egui::{Context, Event, Key, Modifiers};
use pdfcraft_engine::commands::{self, CommandSpec, Shortcut, COMMANDS};
use pdfcraft_engine::{DocId, Session};

/// Converts an egui `Key` enum to the canonical static string used by `Shortcut::key`.
pub fn key_to_str(key: Key) -> Option<&'static str> {
    match key {
        Key::A => Some("A"),
        Key::B => Some("B"),
        Key::C => Some("C"),
        Key::D => Some("D"),
        Key::E => Some("E"),
        Key::F => Some("F"),
        Key::G => Some("G"),
        Key::H => Some("H"),
        Key::I => Some("I"),
        Key::J => Some("J"),
        Key::K => Some("K"),
        Key::L => Some("L"),
        Key::M => Some("M"),
        Key::N => Some("N"),
        Key::O => Some("O"),
        Key::P => Some("P"),
        Key::Q => Some("Q"),
        Key::R => Some("R"),
        Key::S => Some("S"),
        Key::T => Some("T"),
        Key::U => Some("U"),
        Key::V => Some("V"),
        Key::W => Some("W"),
        Key::X => Some("X"),
        Key::Y => Some("Y"),
        Key::Z => Some("Z"),
        Key::Num0 => Some("0"),
        Key::Num1 => Some("1"),
        Key::Num2 => Some("2"),
        Key::Num3 => Some("3"),
        Key::Num4 => Some("4"),
        Key::Num5 => Some("5"),
        Key::Num6 => Some("6"),
        Key::Num7 => Some("7"),
        Key::Num8 => Some("8"),
        Key::Num9 => Some("9"),
        Key::Comma => Some(","),
        Key::Plus => Some("+"),
        Key::Minus => Some("-"),
        Key::Delete => Some("Delete"),
        _ => None,
    }
}

/// Matches an incoming egui key event and modifier state against a registered engine `Shortcut`.
pub fn matches_shortcut(shortcut: &Shortcut, key: Key, modifiers: &Modifiers, is_mac: bool) -> bool {
    let Some(key_str) = key_to_str(key) else {
        return false;
    };

    if shortcut.key != key_str {
        return false;
    }

    if shortcut.shift != modifiers.shift {
        return false;
    }

    if shortcut.alt != modifiers.alt {
        return false;
    }

    if is_mac {
        let cmd_pressed = modifiers.mac_cmd;
        let ctrl_pressed = modifiers.ctrl;
        if shortcut.command != cmd_pressed {
            return false;
        }
        if shortcut.mac_ctrl != ctrl_pressed {
            return false;
        }
    } else {
        let ctrl_pressed = modifiers.ctrl || modifiers.command;
        if shortcut.command != ctrl_pressed {
            return false;
        }
    }

    true
}

/// Dispatches shortcuts against the active session and document context.
pub struct ShortcutDispatcher {
    pub is_mac: bool,
}

impl Default for ShortcutDispatcher {
    fn default() -> Self {
        Self {
            is_mac: cfg!(target_os = "macos"),
        }
    }
}

impl ShortcutDispatcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Evaluates raw input events during the current egui frame.
    /// Returns the command id of any command triggered by a shortcut.
    pub fn process_input(
        &self,
        ctx: &Context,
        session: &Session,
        active: Option<DocId>,
        text_field_has_focus: bool,
    ) -> Option<&'static str> {
        let mut triggered: Option<&'static str> = None;

        ctx.input(|i| {
            // First check direct Alt+P shortcut for opening the command palette regardless of focus
            for event in &i.events {
                if let Event::Key {
                    key: Key::P,
                    pressed: true,
                    modifiers,
                    repeat: false,
                    ..
                } = event
                {
                    if modifiers.alt && !modifiers.command && !modifiers.ctrl {
                        triggered = Some("view.palette");
                        return;
                    }
                }
            }

            // Also check standard secondary Cmd+K / Ctrl+K trigger for command palette
            for event in &i.events {
                if let Event::Key {
                    key: Key::K,
                    pressed: true,
                    modifiers,
                    repeat: false,
                    ..
                } = event
                {
                    let cmd_pressed = if self.is_mac {
                        modifiers.mac_cmd
                    } else {
                        modifiers.ctrl || modifiers.command
                    };
                    if cmd_pressed && !modifiers.alt && !modifiers.shift {
                        triggered = Some("view.palette");
                        return;
                    }
                }
            }

            // Also check direct shortcut for Zero-Score Highlighter: Alt+Z
            for event in &i.events {
                if let Event::Key {
                    key: Key::Z,
                    pressed: true,
                    modifiers,
                    repeat: false,
                    ..
                } = event
                {
                    if modifiers.alt && !modifiers.command && !modifiers.ctrl {
                        if let Some(spec) = commands::command("ocr.highlight_zero_rows") {
                            if commands::is_enabled(spec, session, active) {
                                triggered = Some("ocr.highlight_zero_rows");
                                return;
                            }
                        }
                    }
                }
            }

            // General registry matching across all commands in COMMANDS
            for event in &i.events {
                if let Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    repeat: false,
                    ..
                } = event
                {
                    // If a text field currently has focus, skip commands where `in_text` is false (e.g. Undo / Select All)
                    for spec in COMMANDS {
                        if text_field_has_focus && !spec.in_text {
                            continue;
                        }

                        if let Some(ref shortcut) = spec.shortcut {
                            if matches_shortcut(shortcut, *key, modifiers, self.is_mac) {
                                if commands::is_enabled(spec, session, active) {
                                    triggered = Some(spec.id);
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        });

        triggered
    }

    /// Formats the shortcut label for a given command spec according to host platform.
    pub fn shortcut_label(&self, spec: &CommandSpec) -> Option<String> {
        spec.shortcut.map(|s| s.label(self.is_mac))
    }
}