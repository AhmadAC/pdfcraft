//! Command palette: search and run every user-facing command.
//! Opened with Alt+P. Searches command labels; pressing Enter runs the selected command.

use egui::{
    vec2, Align, Align2, Color32, Key, Layout, Margin, RichText, ScrollArea, Sense, Stroke,
    TextEdit, Window,
};
use pdfcraft_engine::commands::COMMANDS;

use crate::PdfCraftApp;

#[derive(Clone, Default)]
struct PaletteState {
    pub is_open: bool,
    pub query: String,
    pub selected: usize,
}

pub fn show(app: &mut PdfCraftApp, ctx: &egui::Context) {
    // 1. Check for Alt + P shortcut to toggle the palette
    let mut toggle = false;
    ctx.input(|i| {
        for event in &i.events {
            if let egui::Event::Key {
                key: Key::P,
                pressed: true,
                modifiers,
                ..
            } = event
            {
                if modifiers.alt {
                    toggle = true;
                }
            }
        }
    });

    // 2. Load palette state securely from egui context
    let mut state = ctx.data_mut(|d| {
        d.get_temp::<PaletteState>(egui::Id::new("pdfcraft_palette_state"))
            .unwrap_or_default()
    });

    if toggle {
        state.is_open = !state.is_open;
        if state.is_open {
            state.query.clear();
            state.selected = 0;
        }
    }

    if !state.is_open {
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("pdfcraft_palette_state"), state));
        return;
    }

    // 3. Filter commands
    let q = state.query.to_lowercase().trim().to_string();
    let filtered: Vec<_> = COMMANDS
        .iter()
        .filter(|c| {
            q.is_empty()
                || c.id.to_lowercase().contains(&q)
                || c.label.to_lowercase().contains(&q)
                || c.menu.unwrap_or("").to_lowercase().contains(&q)
        })
        .collect();

    let count = filtered.len();
    if state.selected >= count && count > 0 {
        state.selected = count - 1;
    }

    let mut execute_cmd: Option<&'static str> = None;
    let mut should_close = false;

    // 4. Keyboard Navigation
    ctx.input(|i| {
        if i.key_pressed(Key::Escape) {
            should_close = true;
        }
        if i.key_pressed(Key::ArrowDown) && count > 0 {
            state.selected = (state.selected + 1).min(count - 1);
        }
        if i.key_pressed(Key::ArrowUp) && count > 0 {
            state.selected = state.selected.saturating_sub(1);
        }
        if i.key_pressed(Key::Enter) && count > 0 {
            execute_cmd = Some(filtered[state.selected].id);
            should_close = true;
        }
    });

    let screen_rect = ctx.input(|i| i.screen_rect());
    let width = 560.0_f32.min(screen_rect.width() - 32.0);

    // 5. Draw Modal Window
    Window::new("command_palette_modal")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .anchor(Align2::CENTER_TOP, vec2(0.0, 80.0))
        .fixed_size(vec2(width, 360.0))
        .frame(
            egui::Frame::new()
                .fill(Color32::from_rgb(26, 30, 41))
                .stroke(Stroke::new(1.0, Color32::from_rgb(54, 63, 88)))
                .inner_margin(Margin::same(12i8)),
        )
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                // Search Input
                ui.horizontal(|ui| {
                    ui.label(RichText::new("🔍").size(16.0));
                    let resp = ui.add_sized(
                        vec2(ui.available_width(), 28.0),
                        TextEdit::singleline(&mut state.query)
                            .hint_text("Type a command (Alt+P or Esc to close)..."),
                    );
                    resp.request_focus();
                });

                ui.add_space(6.0);

                // Command List
                ScrollArea::vertical()
                    .max_height(300.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if filtered.is_empty() {
                            ui.vertical_centered(|ui| {
                                ui.add_space(20.0);
                                ui.label(
                                    RichText::new("No matching commands found.")
                                        .color(Color32::from_rgb(140, 150, 175)),
                                );
                            });
                            return;
                        }

                        for (idx, c) in filtered.iter().enumerate() {
                            let is_selected = idx == state.selected;
                            let bg_color = if is_selected {
                                Color32::from_rgb(33, 115, 204)
                            } else {
                                Color32::TRANSPARENT
                            };

                            let frame = egui::Frame::new()
                                .fill(bg_color)
                                .inner_margin(Margin::symmetric(10i8, 6i8));

                            frame.show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(c.label).color(Color32::WHITE).strong());

                                    if let Some(menu) = c.menu {
                                        ui.label(
                                            RichText::new(format!("({menu})"))
                                                .size(11.0)
                                                .color(Color32::from_rgb(150, 160, 180)),
                                        );
                                    }

                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.label(
                                            RichText::new(c.id)
                                                .size(11.0)
                                                .color(Color32::from_rgb(120, 130, 150)),
                                        );
                                    });
                                });
                            });

                            let row_rect = ui.min_rect();
                            if ui.interact(row_rect, ui.id().with(idx), Sense::click()).clicked() {
                                execute_cmd = Some(c.id);
                                should_close = true;
                            }
                        }
                    });
            });
        });

    if should_close {
        state.is_open = false;
    }

    ctx.data_mut(|d| d.insert_temp(egui::Id::new("pdfcraft_palette_state"), state));

    // 6. Execute Command
    if let Some(cmd_id) = execute_cmd {
        app.run_command(cmd_id);
    }
}