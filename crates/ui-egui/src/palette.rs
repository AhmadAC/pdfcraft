//! Command palette (architecture §4): search and run every user-facing command.
//!
//! Opened with ⌘K / Ctrl+K, or through View ▸ Find tools and commands. Searches command labels,
//! group names and keywords; pressing Enter runs the selected command or navigates to its tool mode.

use egui::{Align, Align2, Color32, Margin, Stroke, TextEdit};

use crate::theme;

/// Whether `needle` matches `hay` case-insensitively.
fn matches(hay: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let h = pdfcraft_render::text::fold_case(hay);
    let n = pdfcraft_render::text::fold_case(needle);
    h.contains(&n)
}

fn group_icon(s: &str) -> &'static str {
    match s {
        "Edit a PDF" => "pen-line",
        "Scan & OCR" => "scan-text",
        "Sign" => "signature",
        "Prepare a form" => "text-cursor-input",
        "Action Wizard" => "list-checks",
        "Standards" => "file-check",
        _ => "circle",
    }
}

pub fn show(app: &mut crate::PdfCraftApp, ctx: &egui::Context) {
    if !app.palette_open {
        return;
    }
    // A command is ready to run, or its UI tool group to activate.
    let mut execute: Option<&str> = None;
    let mut select: Option<&'static str> = None;

    let active = app.active_doc_id();
    let q = pdfcraft_render::text::fold_case(app.palette_query.trim());
    let mut filtered = Vec::new();
    let mut i = 0;
    for c in pdfcraft_engine::commands::COMMANDS {
        let label = pdfcraft_engine::commands::current_label(c, &app.session, active);
        if q.is_empty()
            || matches(&label, &q)
            || matches(c.id, &q)
            || c.menu.is_some_and(|m| matches(m, &q))
        {
            filtered.push((i, c, label));
        }
        i += 1;
    }
    for g in pdfcraft_engine::catalog::TOOL_GROUPS {
        if q.is_empty() || matches(g.name, &q) || matches(g.description, &q) {
            filtered.push((i, &pdfcraft_engine::commands::CommandSpec {
                id: g.id,
                label: g.name,
                menu: Some("Tools"),
                shortcut: None,
                needs: pdfcraft_engine::commands::Needs::Nothing,
                in_text: false,
                icon: group_icon(g.name),
            }, g.name.into()));
        }
        i += 1;
    }
    // Sort hits: exact start > exact word > substring. Matches enabled now bubble to the top.
    filtered.sort_by(|(_, a, la), (_, b, lb)| {
        let ea = pdfcraft_engine::commands::is_enabled(a, &app.session, active);
        let eb = pdfcraft_engine::commands::is_enabled(b, &app.session, active);
        if ea != eb {
            return eb.cmp(&ea);
        }
        if !q.is_empty() {
            let (fa, fb) = (pdfcraft_render::text::fold_case(la), pdfcraft_render::text::fold_case(lb));
            let sa = fa.starts_with(&q);
            let sb = fb.starts_with(&q);
            if sa != sb {
                return sb.cmp(&sa);
            }
            let wa = fa.split_whitespace().any(|w| w.starts_with(&q));
            let wb = fb.split_whitespace().any(|w| w.starts_with(&q));
            if wa != wb {
                return wb.cmp(&wa);
            }
        }
        a.label.cmp(b.label)
    });

    let mut should_close = false;
    let count = filtered.len();
    if app.palette_selected >= count && count > 0 {
        app.palette_selected = count - 1;
    }

    ctx.input(|i| {
        if i.key_pressed(egui::Key::Escape) {
            should_close = true;
        }
        if i.key_pressed(egui::Key::ArrowDown) && count > 0 {
            app.palette_selected = (app.palette_selected + 1).min(count - 1);
        }
        if i.key_pressed(egui::Key::ArrowUp) && count > 0 {
            app.palette_selected = app.palette_selected.saturating_sub(1);
        }
        if i.key_pressed(egui::Key::Enter) && count > 0 && app.palette_selected < count {
            let (_, c, _) = &filtered[app.palette_selected];
            if pdfcraft_engine::commands::is_enabled(c, &app.session, active) {
                if c.id.starts_with("tools.") {
                    select = Some(c.id);
                } else {
                    execute = Some(c.id);
                }
                should_close = true;
            }
        }
    });

    if should_close {
        app.palette_open = false;
        app.palette_query.clear();
        return;
    }

    let rect = ctx.screen_rect();
    let width = 640.0_f32.min(rect.width() - 32.0);

    egui::Window::new("command_palette")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .anchor(Align2::CENTER_TOP, egui::vec2(0.0, 100.0))
        .fixed_size(egui::vec2(width, 420.0))
        .frame(
            egui::Frame::window(&ctx.style())
                .fill(theme::RGB_21_21_21)
                .stroke(Stroke::new(1.0, theme::RGB_43_43_43))
                .inner_margin(Margin::same(12.0)),
        )
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;

            ui.horizontal(|ui| {
                let text = egui::RichText::new(theme::icon("search")).size(20.0).color(Color32::from_gray(160));
                ui.label(text);
                ui.spacing_mut().text_edit_width = ui.available_width();
                let search = TextEdit::singleline(&mut app.palette_query)
                    .font(egui::TextStyle::Heading)
                    .hint_text(egui::RichText::new("Search tools and commands…").color(Color32::from_gray(120)))
                    .frame(false)
                    .margin(Margin::symmetric(4.0, 4.0));
                let resp = ui.add(search);
                if !resp.has_focus() && !resp.lost_focus() {
                    resp.request_focus();
                }
            });

            ui.painter().line_segment(
                [
                    ui.min_rect().left_bottom() + egui::vec2(0.0, 8.0),
                    ui.min_rect().right_bottom() + egui::vec2(0.0, 8.0),
                ],
                Stroke::new(1.0, theme::RGB_43_43_43),
            );
            ui.add_space(8.0);

            egui::ScrollArea::vertical().max_height(340.0).auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let mac = cfg!(target_os = "macos");

                for (idx, (id, c, label)) in filtered.iter().enumerate() {
                    let selected = idx == app.palette_selected;
                    let enabled = pdfcraft_engine::commands::is_enabled(c, &app.session, active);
                    let is_tool = c.id.starts_with("tools.");

                    let bg = if selected { theme::RGB_43_43_43 } else { Color32::TRANSPARENT };
                    let resp = egui::Frame::none()
                        .fill(bg)
                        .inner_margin(Margin::symmetric(12.0, 8.0))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                let press = theme::Press::track(ui, &ui.min_rect());
                                let alpha = if enabled { 255 } else { 120 };
                                let color = if selected { Color32::WHITE } else { Color32::from_gray(200) }.gamma_multiply(alpha as f32 / 255.0);

                                ui.label(egui::RichText::new(theme::icon(c.icon)).size(16.0).color(color));
                                ui.add_space(8.0);
                                
                                ui.vertical(|ui| {
                                    let mut rt = egui::RichText::new(label).size(15.0).color(color);
                                    if selected {
                                        rt = rt.strong();
                                    }
                                    ui.label(rt);
                                    if let Some(menu) = c.menu {
                                        ui.label(egui::RichText::new(format!("{} ▸ {}", menu, label)).size(12.0).color(Color32::from_gray(140).gamma_multiply(alpha as f32 / 255.0)));
                                    } else if is_tool {
                                        ui.label(egui::RichText::new("Tools").size(12.0).color(Color32::from_gray(140).gamma_multiply(alpha as f32 / 255.0)));
                                    }
                                });

                                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                    if let Some(s) = c.shortcut {
                                        ui.label(egui::RichText::new(s.label(mac)).size(13.0).color(Color32::from_gray(160).gamma_multiply(alpha as f32 / 255.0)));
                                    }
                                });

                                if press.clicked && enabled {
                                    if is_tool {
                                        select = Some(c.id);
                                    } else {
                                        execute = Some(c.id);
                                    }
                                    should_close = true;
                                }
                            });
                        })
                        .response;

                    if theme::hand(resp).clicked() && enabled {
                        if is_tool {
                            select = Some(c.id);
                        } else {
                            execute = Some(c.id);
                        }
                        should_close = true;
                    }
                }
                
                if count == 0 {
                    ui.vertical_centered(|ui| {
                        ui.add_space(40.0);
                        ui.label(egui::RichText::new("No matching commands").color(Color32::from_gray(140)));
                    });
                }
            });
        });

    if should_close {
        app.palette_open = false;
        app.palette_query.clear();
    }

    if let Some(cmd) = execute {
        app.run_command(cmd);
    }
    if let Some(t) = select {
        app.select_mode(t);
    }
}