use crate::models::{EnabledStatus, RunState, Source};
use chrono::{DateTime, Local};
use eframe::egui;

#[derive(Debug, Clone, PartialEq)]
pub enum DialogResult {
    Open,
    Confirmed,
    Cancelled,
}

/// Data for the service properties dialog.
#[derive(Debug, Clone)]
pub struct ServicePropertiesInfo {
    pub service_name: String,
    pub display_name: String,
    pub description: String,
    pub status: RunState,
    pub startup_type: EnabledStatus,
    pub executable_path: String,
    pub log_on_as: String,
    pub product_name: String,
}

/// Show a service properties dialog. Returns true while the dialog is open.
pub fn show_service_properties(ctx: &egui::Context, info: &ServicePropertiesInfo) -> DialogResult {
    let mut result = DialogResult::Open;

    // Constrain dialog to fit within the window content area (below title bar, above status bar)
    let content = ctx.content_rect();
    let margin = 8.0;
    let max_w = (content.width() - margin * 2.0).max(200.0);
    let max_h = (content.height() - margin * 2.0).max(200.0);

    egui::Window::new(format!("{} Properties", info.display_name))
        .collapsible(false)
        .resizable(true)
        .default_width(420.0_f32.min(max_w))
        .max_width(max_w)
        .max_height(max_h)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(content.center())
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("service_props_grid")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        label_row(ui, "Service Name:", &info.service_name);
                        label_row(ui, "Display Name:", &info.display_name);
                        label_row(ui, "Status:", &info.status.to_string());
                        label_row(ui, "Startup Type:", &info.startup_type.to_string());
                        label_row(ui, "Log On As:", &info.log_on_as);
                        label_row_wrap(ui, "Executable:", &info.executable_path);
                        if !info.product_name.is_empty() {
                            label_row(ui, "Product Name:", &info.product_name);
                        }
                    });

                if !info.description.is_empty() {
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Description").strong());
                    ui.add_space(2.0);
                    ui.label(&info.description);
                }

                ui.add_space(12.0);
                ui.vertical_centered(|ui| {
                    if ui.button("   Close   ").clicked() {
                        result = DialogResult::Cancelled;
                    }
                });
                ui.add_space(4.0);
            });
        });

    result
}

fn label_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(egui::RichText::new(label).strong());
    ui.label(value);
    ui.end_row();
}

fn label_row_wrap(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(egui::RichText::new(label).strong());
    ui.add(egui::Label::new(value).wrap());
    ui.end_row();
}

/// Show the About dialog.
pub fn show_about(ctx: &egui::Context) -> DialogResult {
    let mut result = DialogResult::Open;

    egui::Window::new("about_dialog")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(8.0);
                ui.label(egui::RichText::new("App Manager").strong().size(18.0));
                ui.add_space(2.0);
                ui.label("v1.0.0");
                ui.add_space(8.0);
                ui.label("Copyright (C) 2026 Matt Smith");
                ui.label("MIT License");
                ui.add_space(8.0);
                ui.hyperlink_to(
                    "github.com/mattx86/app-manager",
                    "https://github.com/mattx86/app-manager",
                );
                ui.add_space(12.0);
                if ui.button("   Close   ").clicked() {
                    result = DialogResult::Cancelled;
                }
                ui.add_space(4.0);
            });
        });

    result
}

pub fn show_delete_confirmation(ctx: &egui::Context, entry_name: &str) -> DialogResult {
    let mut result = DialogResult::Open;

    egui::Window::new("Confirm Delete")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(8.0);
                ui.label(format!(
                    "Are you sure you want to delete '{}'?",
                    entry_name
                ));
                ui.label("This action cannot be undone.");
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("   Yes, Delete   ").clicked() {
                        result = DialogResult::Confirmed;
                    }
                    ui.add_space(16.0);
                    if ui.button("   Cancel   ").clicked() {
                        result = DialogResult::Cancelled;
                    }
                });
                ui.add_space(8.0);
            });
        });

    result
}

pub fn show_uninstall_confirmation(ctx: &egui::Context, app_name: &str) -> DialogResult {
    let mut result = DialogResult::Open;

    egui::Window::new("Confirm Uninstall")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(8.0);
                ui.label(format!(
                    "Are you sure you want to uninstall '{}'?",
                    app_name
                ));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    let total = ui.available_width();
                    // Approximate button widths from text + padding
                    let btn1 = ui.spacing().button_padding.x * 2.0 + 130.0;
                    let btn2 = ui.spacing().button_padding.x * 2.0 + 55.0;
                    let gap = 16.0;
                    let pad = ((total - btn1 - btn2 - gap) / 2.0).max(0.0);
                    ui.add_space(pad);
                    if ui.button("   Yes, Uninstall   ").clicked() {
                        result = DialogResult::Confirmed;
                    }
                    ui.add_space(gap);
                    if ui.button("   Cancel   ").clicked() {
                        result = DialogResult::Cancelled;
                    }
                });
                ui.add_space(8.0);
            });
        });

    result
}

/// Data for the startup entry properties dialog.
#[derive(Debug, Clone)]
pub struct StartupEntryPropertiesInfo {
    pub name: String,
    pub product_name: String,
    pub command: String,
    pub source: Source,
    pub enabled: EnabledStatus,
    pub run_state: RunState,
    pub runs_as: String,
    pub requires_admin: bool,
    pub last_ran: Option<DateTime<Local>>,
}

/// Show a startup entry properties dialog.
pub fn show_startup_entry_properties(
    ctx: &egui::Context,
    info: &StartupEntryPropertiesInfo,
) -> DialogResult {
    let mut result = DialogResult::Open;

    let content = ctx.content_rect();
    let margin = 8.0;
    let max_w = (content.width() - margin * 2.0).max(200.0);
    let max_h = (content.height() - margin * 2.0).max(200.0);

    egui::Window::new(format!("{} Properties", info.name))
        .collapsible(false)
        .resizable(true)
        .default_width(460.0_f32.min(max_w))
        .max_width(max_w)
        .max_height(max_h)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(content.center())
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("startup_entry_props_grid")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        label_row(ui, "Name:", &info.name);

                        if !info.product_name.is_empty() {
                            label_row(ui, "Product Name:", &info.product_name);
                        }

                        label_row_wrap(ui, "Command:", &info.command);

                        let source_type = match &info.source {
                            Source::RegistryRun { .. } => "Registry (Run)",
                            Source::RegistryRunOnce { .. } => "Registry (RunOnce)",
                            Source::StartupFolder { is_common, .. } => {
                                if *is_common {
                                    "Common Startup Folder"
                                } else {
                                    "User Startup Folder"
                                }
                            }
                            Source::TaskScheduler { .. } => "Task Scheduler",
                            Source::Service { .. } => "Service",
                        };
                        label_row(ui, "Source:", source_type);
                        label_row_wrap(ui, "Location:", &info.source.display_location());

                        let (status_text, status_color) = match info.enabled {
                            EnabledStatus::Enabled => {
                                ("Enabled", egui::Color32::from_rgb(80, 200, 80))
                            }
                            EnabledStatus::Disabled => {
                                ("Disabled", egui::Color32::from_rgb(230, 160, 50))
                            }
                            EnabledStatus::Manual => {
                                ("Manual", egui::Color32::from_rgb(100, 160, 230))
                            }
                            EnabledStatus::Unknown => ("Unknown", egui::Color32::GRAY),
                        };
                        ui.label(egui::RichText::new("Status:").strong());
                        ui.label(egui::RichText::new(status_text).color(status_color));
                        ui.end_row();

                        let (state_text, state_color) = match info.run_state {
                            RunState::Running => {
                                ("Running", egui::Color32::from_rgb(80, 200, 80))
                            }
                            RunState::Stopped => ("Stopped", egui::Color32::GRAY),
                        };
                        ui.label(egui::RichText::new("State:").strong());
                        ui.label(egui::RichText::new(state_text).color(state_color));
                        ui.end_row();

                        if !info.runs_as.is_empty() {
                            label_row(ui, "Runs As:", &info.runs_as);
                        }

                        let visible_as = if info.requires_admin {
                            "Admin"
                        } else {
                            "User"
                        };
                        label_row(ui, "Visible As:", visible_as);

                        let time_text = match info.last_ran {
                            Some(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
                            None => "\u{2014}".to_string(),
                        };
                        label_row(ui, "Last Ran:", &time_text);
                    });

                ui.add_space(12.0);
                ui.vertical_centered(|ui| {
                    if ui.button("   Close   ").clicked() {
                        result = DialogResult::Cancelled;
                    }
                });
                ui.add_space(4.0);
            });
        });

    result
}

/// Data for the process properties dialog.
#[derive(Debug, Clone)]
pub struct ProcessPropertiesInfo {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub name: String,
    pub exe_path: String,
    pub command_line: String,
    pub cpu_usage: f32,
    pub memory_bytes: u64,
    pub disk_read_bytes: u64,
    pub disk_write_bytes: u64,
    pub start_time: Option<DateTime<Local>>,
    pub product_name: String,
    pub user_name: String,
    pub is_elevated: bool,
}

/// Show a process properties dialog. Returns the dialog state.
pub fn show_process_properties(
    ctx: &egui::Context,
    info: &ProcessPropertiesInfo,
) -> DialogResult {
    let mut result = DialogResult::Open;

    let content = ctx.content_rect();
    let margin = 8.0;
    let max_w = (content.width() - margin * 2.0).max(200.0);
    let max_h = (content.height() - margin * 2.0).max(200.0);

    egui::Window::new(format!("{} (PID {}) Properties", info.name, info.pid))
        .collapsible(false)
        .resizable(true)
        .default_width(460.0_f32.min(max_w))
        .max_width(max_w)
        .max_height(max_h)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(content.center())
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("process_props_grid")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        label_row(ui, "PID:", &info.pid.to_string());

                        let ppid_text = match info.parent_pid {
                            Some(ppid) => ppid.to_string(),
                            None => "\u{2014}".to_string(),
                        };
                        label_row(ui, "Parent PID:", &ppid_text);

                        label_row(ui, "Name:", &info.name);

                        if !info.product_name.is_empty() {
                            label_row(ui, "Product Name:", &info.product_name);
                        }

                        if !info.exe_path.is_empty() {
                            label_row_wrap(ui, "Path:", &info.exe_path);
                        }

                        if !info.command_line.is_empty() {
                            label_row_wrap(ui, "Command Line:", &info.command_line);
                        }

                        let cpu_text = if info.cpu_usage > 0.05 {
                            format!("{:.1}%", info.cpu_usage)
                        } else {
                            "0%".to_string()
                        };
                        label_row(ui, "CPU:", &cpu_text);

                        label_row(ui, "Memory:", &format_memory(info.memory_bytes));

                        let dr = format_bytes(info.disk_read_bytes);
                        label_row(ui, "Disk Read:", &dr);

                        let dw = format_bytes(info.disk_write_bytes);
                        label_row(ui, "Disk Write:", &dw);

                        let runs_as = if info.user_name.is_empty() { "--" } else { &info.user_name };
                        label_row(ui, "Runs As:", runs_as);

                        let visible_as = if info.is_elevated { "Admin" } else { "User" };
                        label_row(ui, "Visible As:", visible_as);

                        let time_text = match info.start_time {
                            Some(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
                            None => "\u{2014}".to_string(),
                        };
                        label_row(ui, "Start Time:", &time_text);
                    });

                ui.add_space(12.0);
                ui.vertical_centered(|ui| {
                    if ui.button("   Close   ").clicked() {
                        result = DialogResult::Cancelled;
                    }
                });
                ui.add_space(4.0);
            });
        });

    result
}

fn format_memory(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        "\u{2014}".to_string()
    } else if bytes >= 1_073_741_824 {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}
