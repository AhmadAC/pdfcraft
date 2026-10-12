//! Command palette (architecture §4): search and run every user-facing command.
//!
//! Opened with ⌘K / Ctrl+K / Alt+P, or through View ▸ Find tools and commands. Searches command
//! labels, group names and keywords; pressing Enter runs the selected command or navigates to
//! its tool mode.

use egui::{
    vec2, Align, Align2, Color32, Event, FontId, Key, Layout, Margin, Response, RichText,
    ScrollArea, Sense, Stroke, TextEdit, Ui, Vec2, Window,
};
use pdfcraft_engine::commands::{self, CommandSpec, COMMANDS};

use crate::PdfCraftApp;

fn fold_case_str(s: &str) -> String {
    s.to_lowercase()
}

/// Whether `needle` matches `hay` case-insensitively.
fn matches_query(hay: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let h = fold_case_str(hay);
    let n = fold_case_str(needle);
    h.contains(&n)
}

/// A search hit in the command palette.
#[derive(Clone, Debug)]
struct Match {
    spec: &'static CommandSpec,
    label: String,
    shortcut: Option<String>,
    enabled: bool,
}

/// Collect and rank commands matching `query`.
fn matching_commands(app: &PdfCraftApp, query: &str) -> Vec<Match> {
    let active = app.active_doc_id();
    let q = fold_case_str(query.trim());
    let mut hits = Vec::new();

    for spec in COMMANDS {
        let label = commands::current_label(spec, &app.session, active);
        let enabled = commands::is_enabled(spec, &app.session, active);

        let matches = if q.is_empty() {
            true
        } else {
            matches_query(&label, &q)
                || matches_query(spec.id, &q)
                || spec.menu.is_some_and(|m| matches_query(m, &q))
        };

        if matches {
            let shortcut = spec.shortcut.map(|s| s.label(cfg!(target_os = "macos")));
            hits.push(Match {
                spec,
                label,
                shortcut,
                enabled,
            });
        }
    }

    hits.sort_by(|a, b| {
        if a.enabled != b.enabled {
            return b.enabled.cmp(&a.enabled);
        }
        if !q.is_empty() {
            let a_starts = fold_case_str(&a.label).starts_with(&q);
            let b_starts = fold_case_str(&b.label).starts_with(&q);
            if a_starts != b_starts {
                return b_starts.cmp(&a_starts);
            }
        }
        a.label.cmp(&b.label)
    });

    hits
}

/// Draw the command palette modal if it is open.
pub fn show(app: &mut PdfCraftApp, ctx: &egui::Context) {
    if !app.palette_open {
        return;
    }

    let hits = matching_commands(app, &app.palette_query);
    let count = hits.len();

    let mut execute_cmd: Option<&'static str> = None;
    let mut should_close = false;

    if app.palette_selected >= count && count > 0 {
        app.palette_selected = count - 1;
    }

    // Keyboard navigation
    ctx.input(|i| {
        if i.key_pressed(Key::Escape) {
            should_close = true;
        }
        if i.key_pressed(Key::ArrowDown) && count > 0 {
            app.palette_selected = (app.palette_selected + 1).min(count - 1);
        }
        if i.key_pressed(Key::ArrowUp) && count > 0 {
            app.palette_selected = app.palette_selected.saturating_sub(1);
        }
        if i.key_pressed(Key::Enter) && count > 0 && app.palette_selected < count {
            let hit = &hits[app.palette_selected];
            if hit.enabled {
                execute_cmd = Some(hit.spec.id);
                should_close = true;
            }
        }
    });

    if should_close {
        app.palette_open = false;
        app.palette_query.clear();
        return;
    }

    let screen_rect = ctx.screen_rect();
    let width = 560.0.min(screen_rect.width() - 32.0);

    Window::new("command_palette_popup")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .anchor(Align2::CENTER_TOP, vec2(0.0, 72.0))
        .fixed_size(vec2(width, 360.0))
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                // Input header
                ui.horizontal(|ui| {
                    ui.label(RichText::new("🔍").size(16.0));
                    let resp = ui.add_sized(
                        vec2(ui.available_width(), 28.0),
                        TextEdit::singleline(&mut app.palette_query)
                            .hint_text("Type a command or tool name...")
                            .desired_width(f32::INFINITY),
                    );
                    resp.request_focus();
                });

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(2.0);

                // Results list
                ScrollArea::vertical()
                    .max_height(280.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if hits.is_empty() {
                            ui.vertical_centered(|ui| {
                                ui.add_space(24.0);
                                ui.label(
                                    RichText::new("No matching commands")
                                        .color(Color32::from_gray(140)),
                                );
                            });
                            return;
                        }

                        for (idx, hit) in hits.iter().enumerate() {
                            let is_selected = idx == app.palette_selected;

                            let (text_color, bg_color) = if is_selected {
                                (
                                    Color32::WHITE,
                                    ui.visuals().selection.bg_fill,
                                )
                            } else if hit.enabled {
                                (
                                    ui.visuals().text_color(),
                                    Color32::TRANSPARENT,
                                )
                            } else {
                                (
                                    Color32::from_gray(120),
                                    Color32::TRANSPARENT,
                                )
                            };

                            let frame = egui::Frame::none()
                                .fill(bg_color)
                                .inner_margin(Margin::symmetric(8, 4));

                            frame.show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    // Command Label
                                    ui.label(
                                        RichText::new(&hit.label)
                                            .color(text_color)
                                            .strong(),
                                    );

                                    // Menu category breadcrumb
                                    if let Some(menu) = hit.spec.menu {
                                        ui.label(
                                            RichText::new(format!("({menu})"))
                                                .size(11.0)
                                                .color(Color32::from_gray(130)),
                                        );
                                    }

                                    // Shortcut badge aligned to right
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if let Some(ref s) = hit.shortcut {
                                            ui.label(
                                                RichText::new(s)
                                                    .size(11.0)
                                                    .color(Color32::from_rgb(240, 160, 40)),
                                            );
                                        }
                                    });
                                });
                            });

                            let row_rect = ui.min_rect();
                            let resp = ui.interact(row_rect, ui.id().with(idx), Sense::click());
                            if resp.clicked() && hit.enabled {
                                execute_cmd = Some(hit.spec.id);
                                should_close = true;
                            }
                        }
                    });
            });
        });

    if should_close {
        app.palette_open = false;
        app.palette_query.clear();
    }

    if let Some(cmd_id) = execute_cmd {
        app.run_command(cmd_id);
    }
}