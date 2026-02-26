use crate::gui::PendingAction;
use crate::models::*;
use eframe::egui;
use egui_extras::{Column, TableBuilder};

pub struct TableResult {
    pub action: Option<PendingAction>,
    pub clicked_row: Option<usize>,
    pub double_clicked_row: Option<usize>,
    pub hovered_row: Option<usize>,
}

pub fn render_table(
    ui: &mut egui::Ui,
    entries: &[StartupEntry],
    selected_row: Option<usize>,
    prev_hovered_row: Option<usize>,
    col3_header: Option<&str>,
    last_time_header: &str,
    show_delete: bool,
    show_properties: bool,
) -> TableResult {
    let mut action = None;
    let mut clicked_row = None;
    let mut double_clicked_row = None;
    let mut hovered_row = None;

    let available_height = ui.available_height();
    let show_col3 = col3_header.is_some();

    let mut builder = TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .sense(egui::Sense::click())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::initial(160.0).at_least(80.0)) // Name
        .column(Column::initial(180.0).at_least(80.0)) // Product Name
        .column(Column::initial(300.0).at_least(100.0)); // Command
    if show_col3 {
        builder = builder.column(Column::initial(220.0).at_least(80.0)); // Source
    }
    let table = builder
        .column(Column::initial(70.0).at_least(60.0)) // Status
        .column(Column::initial(65.0).at_least(55.0)) // State
        .column(Column::initial(90.0).at_least(60.0)) // Runs As
        .column(Column::initial(75.0).at_least(55.0)) // Visible As
        .column(Column::initial(140.0).at_least(100.0)) // Last Ran / Last Started
        .column(Column::remainder().at_least(200.0)) // Actions
        .min_scrolled_height(0.0)
        .max_scroll_height(available_height);

    table
        .header(20.0, |mut header| {
            header.col(|ui| { ui.strong("Name"); });
            header.col(|ui| { ui.strong("Product Name"); });
            header.col(|ui| { ui.strong("Command"); });
            if show_col3 {
                header.col(|ui| { ui.strong(col3_header.unwrap()); });
            }
            header.col(|ui| { ui.strong("Status"); });
            header.col(|ui| { ui.strong("State"); });
            header.col(|ui| { ui.strong("Runs As"); });
            header.col(|ui| { ui.strong("Visible As"); });
            header.col(|ui| { ui.strong(last_time_header); });
            header.col(|ui| { ui.strong("Actions"); });
        })
        .body(|body| {
            body.rows(24.0, entries.len(), |mut row| {
                let index = row.index();
                let entry = &entries[index];
                let is_selected = selected_row == Some(index);
                let was_hovered = prev_hovered_row == Some(index);

                if is_selected || was_hovered {
                    row.set_selected(true);
                }

                // Track hover/click from both cell backgrounds AND inner widgets
                let mut row_hovered = false;
                let mut row_clicked = false;
                let mut row_double_clicked = false;

                // Name
                let (_, cell_resp) = row.col(|ui| {
                    let label = egui::Label::new(&entry.name)
                        .truncate()
                        .sense(egui::Sense::click());
                    let resp = ui.add(label);
                    row_hovered |= resp.hovered();
                    row_clicked |= resp.clicked();
                    row_double_clicked |= resp.double_clicked();
                });
                row_hovered |= cell_resp.hovered();
                row_clicked |= cell_resp.clicked();
                row_double_clicked |= cell_resp.double_clicked();

                // Product Name
                let (_, cell_resp) = row.col(|ui| {
                    let text = if entry.product_name.is_empty() { "\u{2014}" } else { &entry.product_name };
                    let color = if entry.product_name.is_empty() {
                        egui::Color32::GRAY
                    } else {
                        egui::Color32::from_rgb(200, 200, 200)
                    };
                    let label = egui::Label::new(egui::RichText::new(text).color(color))
                        .truncate()
                        .sense(egui::Sense::click());
                    let resp = ui.add(label);
                    row_hovered |= resp.hovered();
                    row_clicked |= resp.clicked();
                    row_double_clicked |= resp.double_clicked();
                });
                row_hovered |= cell_resp.hovered();
                row_clicked |= cell_resp.clicked();
                row_double_clicked |= cell_resp.double_clicked();

                // Command
                let (_, cell_resp) = row.col(|ui| {
                    let label = egui::Label::new(&entry.command)
                        .truncate()
                        .sense(egui::Sense::click());
                    let resp = ui.add(label);
                    row_hovered |= resp.hovered();
                    row_clicked |= resp.clicked();
                    row_double_clicked |= resp.double_clicked();
                });
                row_hovered |= cell_resp.hovered();
                row_clicked |= cell_resp.clicked();
                row_double_clicked |= cell_resp.double_clicked();

                // Source (only when col3 is shown)
                if show_col3 {
                    let (_, cell_resp) = row.col(|ui| {
                        let loc = entry.source.display_location();
                        let label = egui::Label::new(&loc)
                            .truncate()
                            .sense(egui::Sense::click());
                        let resp = ui.add(label);
                        row_hovered |= resp.hovered();
                        row_clicked |= resp.clicked();
                    });
                    row_hovered |= cell_resp.hovered();
                    row_clicked |= cell_resp.clicked();
                }

                // Status (color-coded)
                let (_, cell_resp) = row.col(|ui| {
                    let (text, color) = match entry.enabled {
                        EnabledStatus::Enabled => (
                            "Enabled",
                            egui::Color32::from_rgb(80, 200, 80),
                        ),
                        EnabledStatus::Disabled => (
                            "Disabled",
                            egui::Color32::from_rgb(230, 160, 50),
                        ),
                        EnabledStatus::Manual => (
                            "Manual",
                            egui::Color32::from_rgb(100, 160, 230),
                        ),
                        EnabledStatus::Unknown => (
                            "Unknown",
                            egui::Color32::GRAY,
                        ),
                    };
                    let label = egui::Label::new(
                        egui::RichText::new(text).color(color),
                    ).sense(egui::Sense::click());
                    let resp = ui.add(label);
                    row_hovered |= resp.hovered();
                    row_clicked |= resp.clicked();
                    row_double_clicked |= resp.double_clicked();
                });
                row_hovered |= cell_resp.hovered();
                row_clicked |= cell_resp.clicked();
                row_double_clicked |= cell_resp.double_clicked();

                // State (color-coded)
                let (_, cell_resp) = row.col(|ui| {
                    let (text, color) = match entry.run_state {
                        RunState::Running => (
                            "Running",
                            egui::Color32::from_rgb(80, 200, 80),
                        ),
                        RunState::Stopped => (
                            "Stopped",
                            egui::Color32::GRAY,
                        ),
                    };
                    let label = egui::Label::new(
                        egui::RichText::new(text).color(color),
                    ).sense(egui::Sense::click());
                    let resp = ui.add(label);
                    row_hovered |= resp.hovered();
                    row_clicked |= resp.clicked();
                    row_double_clicked |= resp.double_clicked();
                });
                row_hovered |= cell_resp.hovered();
                row_clicked |= cell_resp.clicked();
                row_double_clicked |= cell_resp.double_clicked();

                // Runs As
                let (_, cell_resp) = row.col(|ui| {
                    let text = if entry.runs_as.is_empty() { "--" } else { &entry.runs_as };
                    let label = egui::Label::new(text)
                        .truncate()
                        .sense(egui::Sense::click());
                    let resp = ui.add(label);
                    row_hovered |= resp.hovered();
                    row_clicked |= resp.clicked();
                    row_double_clicked |= resp.double_clicked();
                });
                row_hovered |= cell_resp.hovered();
                row_clicked |= cell_resp.clicked();
                row_double_clicked |= cell_resp.double_clicked();

                // Visible As
                let (_, cell_resp) = row.col(|ui| {
                    let (text, color) = if entry.requires_admin {
                        ("Admin", egui::Color32::from_rgb(230, 160, 50))
                    } else {
                        ("User", ui.visuals().text_color())
                    };
                    let label = egui::Label::new(
                        egui::RichText::new(text).color(color),
                    ).sense(egui::Sense::click());
                    let resp = ui.add(label);
                    row_hovered |= resp.hovered();
                    row_clicked |= resp.clicked();
                    row_double_clicked |= resp.double_clicked();
                });
                row_hovered |= cell_resp.hovered();
                row_clicked |= cell_resp.clicked();
                row_double_clicked |= cell_resp.double_clicked();

                // Last Ran
                let (_, cell_resp) = row.col(|ui| {
                    let text = match entry.last_ran {
                        Some(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
                        None => "--".to_string(),
                    };
                    let label = egui::Label::new(&text)
                        .sense(egui::Sense::click());
                    let resp = ui.add(label);
                    row_hovered |= resp.hovered();
                    row_clicked |= resp.clicked();
                    row_double_clicked |= resp.double_clicked();
                });
                row_hovered |= cell_resp.hovered();
                row_clicked |= cell_resp.clicked();
                row_double_clicked |= cell_resp.double_clicked();

                // Actions (fixed-width buttons for alignment)
                let (_, cell_resp) = row.col(|ui| {
                    ui.horizontal(|ui| {
                        let btn_size = egui::vec2(55.0, 18.0);

                        let is_run_once = matches!(entry.source, Source::RegistryRunOnce { .. });
                        if !is_run_once {
                            let (label, act) = match entry.enabled {
                                EnabledStatus::Enabled => ("Disable", PendingAction::Disable(index)),
                                EnabledStatus::Disabled => ("Enable", PendingAction::Enable(index)),
                                EnabledStatus::Manual => ("Disable", PendingAction::Disable(index)),
                                EnabledStatus::Unknown => ("Disable", PendingAction::Disable(index)),
                            };
                            if ui.add_sized(btn_size, egui::Button::new(label)).clicked() {
                                action = Some(act);
                            }
                        } else {
                            ui.add_space(btn_size.x + ui.spacing().item_spacing.x);
                        }

                        let (label, act) = match entry.run_state {
                            RunState::Running => ("Stop", PendingAction::Stop(index)),
                            RunState::Stopped => ("Start", PendingAction::Start(index)),
                        };
                        if ui.add_sized(btn_size, egui::Button::new(label)).clicked() {
                            action = Some(act);
                        }

                        if show_delete {
                            if ui.add_sized(btn_size, egui::Button::new("Delete")).clicked() {
                                action = Some(PendingAction::ConfirmDelete(index));
                            }
                        }

                        if show_properties {
                            if ui.add_sized(btn_size, egui::Button::new("Properties")).clicked() {
                                action = Some(PendingAction::Properties(index));
                            }
                        }
                    });
                });
                row_hovered |= cell_resp.hovered();
                row_clicked |= cell_resp.clicked();

                if row_hovered {
                    hovered_row = Some(index);
                }
                if row_clicked {
                    clicked_row = Some(index);
                }
                if row_double_clicked {
                    double_clicked_row = Some(index);
                }
            });
        });

    TableResult { action, clicked_row, double_clicked_row, hovered_row }
}
