//! Spotlight / Quick-Open Command Palette modal dialog in PdfCraft.
//! Opened via Alt + P (or ⌘K/Ctrl+K), allowing users to fuzzy-search and execute
//! any command in the engine registry including the OCR Zero-Score Highlighter.

use egui::{
    vec2, Align, Align2, Color32, Context, Frame, Key, Layout, Margin, RichText, Rounding,
    ScrollArea, Stroke, TextEdit, Ui, Window,
};
use pdfcraft_engine::commands::{self, CommandSpec, COMMANDS};
use pdfcraft_engine::{DocId, Session};

/// Visual state and search query tracking for the command palette.
pub struct CommandPalette {
    pub is_open: bool,
    pub search_query: String,
    pub selected_index: usize,
    pub is_mac: bool,
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self {
            is_open: false,
            search_query: String::new(),
            selected_index: 0,
            is_mac: cfg!(target_os = "macos"),
        }
    }
}

impl CommandPalette {
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens the command palette and resets input state.
    pub fn open(&mut self) {
        self.is_open = true;
        self.search_query.clear();
        self.selected_index = 0;
    }

    /// Closes the palette dialog.
    pub fn close(&mut self) {
        self.is_open = false;
        self.search_query.clear();
        self.selected_index = 0;
    }

    /// Toggles the palette dialog.
    pub fn toggle(&mut self) {
        if self.is_open {
            self.close();
        } else {
            self.open();
        }
    }

    /// Filters commands matching the search term, sorted by relevance and enabled state.
    pub fn matching_commands(
        &self,
        session: &Session,
        active: Option<DocId>,
    ) -> Vec<(&'static CommandSpec, bool, String)> {
        let query = self.search_query.to_lowercase().trim().to_string();

        let mut results = Vec::new();

        for spec in COMMANDS {
            let label = commands::current_label(spec, session, active);
            let enabled = commands::is_enabled(spec, session, active);
            let id = spec.id;
            let menu = spec.menu.unwrap_or("");

            // Matching criteria
            let matches = if query.is_empty() {
                true
            } else {
                label.to_lowercase().contains(&query)
                    || id.to_lowercase().contains(&query)
                    || menu.to_lowercase().contains(&query)
            };

            if matches {
                results.push((spec, enabled, label));
            }
        }

        // Sort enabled items to top, followed by exact match precedence
        results.sort_by(|(a_spec, a_enabled, a_label), (b_spec, b_enabled, b_label)| {
            if *a_enabled != *b_enabled {
                return b_enabled.cmp(a_enabled);
            }
            if !query.is_empty() {
                let a_starts = a_label.to_lowercase().starts_with(&query);
                let b_starts = b_label.to_lowercase().starts_with(&query);
                if a_starts != b_starts {
                    return b_starts.cmp(&a_starts);
                }
            }
            a_spec.label.cmp(b_spec.label)
        });

        results
    }

    /// Renders the Command Palette window if open, returning `Some(command_id)` if triggered.
    pub fn show(
        &mut self,
        ctx: &Context,
        session: &Session,
        active: Option<DocId>,
    ) -> Option<&'static str> {
        if !self.is_open {
            return None;
        }

        let mut executed_id: Option<&'static str> = None;
        let mut should_close = false;

        let filtered = self.matching_commands(session, active);
        let total_items = filtered.len();

        if self.selected_index >= total_items && total_items > 0 {
            self.selected_index = total_items - 1;
        }

        // Navigation input handling
        ctx.input(|i| {
            if i.key_pressed(Key::Escape) {
                should_close = true;
            }
            if i.key_pressed(Key::ArrowDown) && total_items > 0 {
                self.selected_index = (self.selected_index + 1).min(total_items - 1);
            }
            if i.key_pressed(Key::ArrowUp) && total_items > 0 {
                self.selected_index = self.selected_index.saturating_sub(1);
            }
            if i.key_pressed(Key::Enter) && total_items > 0 && self.selected_index < total_items {
                let (spec, enabled, _) = &filtered[self.selected_index];
                if *enabled {
                    executed_id = Some(spec.id);
                    should_close = true;
                }
            }
        });

        if should_close {
            self.close();
            return executed_id;
        }

        // Modal palette dialog
        Window::new("command_palette_window")
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .anchor(Align2::CENTER_TOP, vec2(0.0, 80.0))
            .fixed_size(vec2(620.0, 390.0))
            .frame(
                Frame::popup(&ctx.style())
                    .fill(Color32::from_rgb(24, 27, 36))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(52, 60, 82)))
                    .rounding(Rounding::same(8.0))
                    .inner_margin(Margin::same(12.0)),
            )
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    // Search bar
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("🔍").size(16.0));
                        let response = ui.add_sized(
                            vec2(ui.available_width(), 32.0),
                            TextEdit::singleline(&mut self.search_query)
                                .hint_text("Type a command... (Alt+P or Esc to dismiss)")
                                .text_color(Color32::WHITE)
                                .font(egui::TextStyle::Body),
                        );
                        response.request_focus();
                    });

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);

                    // Search results scroll area
                    ScrollArea::vertical()
                        .max_height(300.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if filtered.is_empty() {
                                ui.vertical_centered(|ui| {
                                    ui.add_space(24.0);
                                    ui.label(
                                        RichText::new("No matching commands found")
                                            .color(Color32::from_rgb(140, 150, 175)),
                                    );
                                });
                                return;
                            }

                            for (idx, (spec, enabled, current_label)) in filtered.iter().enumerate() {
                                let is_selected = idx == self.selected_index;

                                let bg_color = if is_selected {
                                    Color32::from_rgb(33, 115, 204)
                                } else {
                                    Color32::TRANSPARENT
                                };

                                let text_color = if *enabled {
                                    Color32::WHITE
                                } else {
                                    Color32::from_rgb(110, 120, 140)
                                };

                                Frame::none()
                                    .fill(bg_color)
                                    .rounding(Rounding::same(4.0))
                                    .inner_margin(Margin::symmetric(10.0, 6.0))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.vertical(|ui| {
                                                ui.horizontal(|ui| {
                                                    ui.label(
                                                        RichText::new(current_label)
                                                            .color(text_color)
                                                            .strong(),
                                                    );
                                                    if let Some(ref s) = spec.shortcut {
                                                        let label = s.label(self.is_mac);
                                                        ui.label(
                                                            RichText::new(format!("[{label}]"))
                                                                .size(11.0)
                                                                .color(Color32::from_rgb(243, 156, 18)),
                                                        );
                                                    }
                                                });
                                                if let Some(menu) = spec.menu {
                                                    ui.label(
                                                        RichText::new(format!("Menu ▸ {}", menu))
                                                            .size(11.0)
                                                            .color(Color32::from_rgb(140, 150, 170)),
                                                    );
                                                }
                                            });

                                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                                ui.label(
                                                    RichText::new(spec.id)
                                                        .size(11.0)
                                                        .color(Color32::from_rgb(115, 125, 145)),
                                                );
                                            });
                                        });
                                    });

                                let item_rect = ui.min_rect();
                                if ui.interact(item_rect, ui.id().with(idx), egui::Sense::click()).clicked() {
                                    if *enabled {
                                        executed_id = Some(spec.id);
                                        should_close = true;
                                    }
                                }
                            }
                        });
                });
            });

        if should_close {
            self.close();
        }

        executed_id
    }
}