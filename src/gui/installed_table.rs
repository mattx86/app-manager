use crate::models::InstalledApp;
use eframe::egui;
use egui_extras::{Column, TableBuilder};

pub enum InstalledAppAction {
    Modify(usize),
    Uninstall(usize),
}

pub struct InstalledTableResult {
    pub action: Option<InstalledAppAction>,
    pub clicked_row: Option<usize>,
    pub hovered_row: Option<usize>,
}

pub fn render_installed_table(
    ui: &mut egui::Ui,
    apps: &[InstalledApp],
    selected_row: Option<usize>,
    prev_hovered_row: Option<usize>,
) -> InstalledTableResult {
    let mut action = None;
    let mut clicked_row = None;
    let mut hovered_row = None;

    let available_height = ui.available_height();

    let table = TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .sense(egui::Sense::click())
        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
        .column(Column::initial(200.0).at_least(100.0)) // Name
        .column(Column::initial(180.0).at_least(80.0))  // Publisher
        .column(Column::initial(100.0).at_least(60.0))  // Version
        .column(Column::initial(100.0).at_least(70.0))  // Install Date
        .column(Column::initial(80.0).at_least(50.0))   // Size
        .column(Column::initial(200.0).at_least(80.0))  // Install Location
        .column(Column::remainder().at_least(150.0))     // Actions
        .min_scrolled_height(0.0)
        .max_scroll_height(available_height);

    table
        .header(20.0, |mut header| {
            header.col(|ui| { ui.strong("Name"); });
            header.col(|ui| { ui.strong("Publisher"); });
            header.col(|ui| { ui.strong("Version"); });
            header.col(|ui| { ui.strong("Install Date"); });
            header.col(|ui| { ui.strong("Size"); });
            header.col(|ui| { ui.strong("Install Location"); });
            header.col(|ui| { ui.strong("Actions"); });
        })
        .body(|body| {
            body.rows(24.0, apps.len(), |mut row| {
                let index = row.index();
                let app = &apps[index];
                let is_selected = selected_row == Some(index);
                let was_hovered = prev_hovered_row == Some(index);

                if is_selected || was_hovered {
                    row.set_selected(true);
                }

                let mut row_hovered = false;
                let mut row_clicked = false;

                // Name
                let (_, cell_resp) = row.col(|ui| {
                    let label = egui::Label::new(&app.display_name)
                        .truncate()
                        .sense(egui::Sense::click());
                    let resp = ui.add(label);
                    row_hovered |= resp.hovered();
                    row_clicked |= resp.clicked();
                });
                row_hovered |= cell_resp.hovered();
                row_clicked |= cell_resp.clicked();

                // Publisher
                let (_, cell_resp) = row.col(|ui| {
                    let text = if app.publisher.is_empty() { "\u{2014}" } else { &app.publisher };
                    let color = if app.publisher.is_empty() {
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
                });
                row_hovered |= cell_resp.hovered();
                row_clicked |= cell_resp.clicked();

                // Version
                let (_, cell_resp) = row.col(|ui| {
                    let text = if app.display_version.is_empty() { "--" } else { &app.display_version };
                    let label = egui::Label::new(text)
                        .truncate()
                        .sense(egui::Sense::click());
                    let resp = ui.add(label);
                    row_hovered |= resp.hovered();
                    row_clicked |= resp.clicked();
                });
                row_hovered |= cell_resp.hovered();
                row_clicked |= cell_resp.clicked();

                // Install Date
                let (_, cell_resp) = row.col(|ui| {
                    let text = format_install_date(&app.install_date);
                    let label = egui::Label::new(&text)
                        .sense(egui::Sense::click());
                    let resp = ui.add(label);
                    row_hovered |= resp.hovered();
                    row_clicked |= resp.clicked();
                });
                row_hovered |= cell_resp.hovered();
                row_clicked |= cell_resp.clicked();

                // Size
                let (_, cell_resp) = row.col(|ui| {
                    let text = format_size(app.estimated_size_kb);
                    let label = egui::Label::new(&text)
                        .sense(egui::Sense::click());
                    let resp = ui.add(label);
                    row_hovered |= resp.hovered();
                    row_clicked |= resp.clicked();
                });
                row_hovered |= cell_resp.hovered();
                row_clicked |= cell_resp.clicked();

                // Install Location
                let (_, cell_resp) = row.col(|ui| {
                    let text = if app.install_location.is_empty() { "--" } else { &app.install_location };
                    let label = egui::Label::new(text)
                        .truncate()
                        .sense(egui::Sense::click());
                    let resp = ui.add(label);
                    row_hovered |= resp.hovered();
                    row_clicked |= resp.clicked();
                });
                row_hovered |= cell_resp.hovered();
                row_clicked |= cell_resp.clicked();

                // Actions
                let (_, cell_resp) = row.col(|ui| {
                    ui.horizontal(|ui| {
                        let btn_size = egui::vec2(65.0, 18.0);

                        let has_modify = app.modify_path.is_some();
                        if ui
                            .add_enabled(has_modify, egui::Button::new("Modify").min_size(btn_size))
                            .clicked()
                        {
                            action = Some(InstalledAppAction::Modify(index));
                        }

                        if ui
                            .add_sized(btn_size, egui::Button::new("Uninstall"))
                            .clicked()
                        {
                            action = Some(InstalledAppAction::Uninstall(index));
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
            });
        });

    InstalledTableResult {
        action,
        clicked_row,
        hovered_row,
    }
}

fn format_install_date(raw: &str) -> String {
    if raw.len() == 8 {
        // YYYYMMDD -> YYYY-MM-DD
        format!("{}-{}-{}", &raw[..4], &raw[4..6], &raw[6..8])
    } else if raw.is_empty() {
        "--".to_string()
    } else {
        raw.to_string()
    }
}

fn format_size(kb: u64) -> String {
    if kb == 0 {
        "--".to_string()
    } else if kb >= 1_048_576 {
        format!("{:.1} GB", kb as f64 / 1_048_576.0)
    } else if kb >= 1024 {
        format!("{:.1} MB", kb as f64 / 1024.0)
    } else {
        format!("{} KB", kb)
    }
}
