mod dialogs;
mod installed_table;
mod process_table;
mod table;

use crate::actions;
use crate::collector;
use crate::installed_apps;
use crate::models::*;
use crate::processes;
use crate::services;
use eframe::egui;
use std::collections::HashSet;
use std::io::Write;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::sync::mpsc;
use std::time::Instant;

/// Action requested from the table UI.
#[derive(Debug, Clone)]
pub enum PendingAction {
    Enable(usize),
    Disable(usize),
    Start(usize),
    Stop(usize),
    ConfirmDelete(usize),
    ConfirmUninstall(usize),
    Properties(usize),
}

/// Status message shown in the bottom bar.
struct StatusMessage {
    text: String,
    is_error: bool,
    when: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Installed,
    StartupApps,
    Processes,
    Services,
}

struct LoadResult {
    entries: Vec<StartupEntry>,
    all_services: Vec<StartupEntry>,
    all_processes: Vec<ProcessInfo>,
    installed_apps: Vec<InstalledApp>,
    is_admin: bool,
}

pub struct StartupApp {
    entries: Vec<StartupEntry>,
    all_services: Vec<StartupEntry>,
    all_processes: Vec<ProcessInfo>,
    installed_apps: Vec<InstalledApp>,
    is_admin: bool,
    active_tab: Tab,
    hide_microsoft_services: bool,
    hide_windows_processes: bool,
    auto_refresh_processes: bool,
    last_process_refresh: Instant,
    expanded_pids: HashSet<u32>,
    pending_action: Option<PendingAction>,
    rescan_receiver: Option<mpsc::Receiver<()>>,
    status: Option<StatusMessage>,
    selected_row: Option<usize>,
    hovered_row: Option<usize>,
    loading: bool,
    load_receiver: Option<mpsc::Receiver<LoadResult>>,
    process_refresh_receiver: Option<mpsc::Receiver<Vec<ProcessInfo>>>,
    service_properties: Option<dialogs::ServicePropertiesInfo>,
    process_properties: Option<dialogs::ProcessPropertiesInfo>,
    startup_entry_properties: Option<dialogs::StartupEntryPropertiesInfo>,
    show_about: bool,
}

impl StartupApp {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            // Run all four collectors in parallel
            let (result, all_services, all_processes, installed) = std::thread::scope(|s| {
                let h1 = s.spawn(|| collector::collect_all_entries());
                let h2 = s.spawn(|| services::collect_services().unwrap_or_default());
                let h3 = s.spawn(|| processes::collect_processes());
                let h4 = s.spawn(|| installed_apps::collect_installed_apps());
                (
                    h1.join().unwrap_or(collector::CollectionResult { entries: vec![], is_admin: false }),
                    h2.join().unwrap_or_default(),
                    h3.join().unwrap_or_default(),
                    h4.join().unwrap_or_default(),
                )
            });

            let _ = tx.send(LoadResult {
                entries: result.entries,
                all_services,
                all_processes,
                installed_apps: installed,
                is_admin: result.is_admin,
            });
        });

        Self {
            entries: Vec::new(),
            all_services: Vec::new(),
            all_processes: Vec::new(),
            installed_apps: Vec::new(),
            is_admin: false,
            active_tab: Tab::Installed,
            hide_microsoft_services: true,
            hide_windows_processes: true,
            auto_refresh_processes: false,
            last_process_refresh: Instant::now(),
            expanded_pids: HashSet::new(),
            pending_action: None,
            rescan_receiver: None,
            status: None,
            selected_row: None,
            hovered_row: None,
            loading: true,
            load_receiver: Some(rx),
            process_refresh_receiver: None,
            service_properties: None,
            process_properties: None,
            startup_entry_properties: None,
            show_about: false,
        }
    }

    /// Spawn a background thread to reload all data, showing the loading overlay.
    fn start_background_load(&mut self) {
        if self.loading {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.loading = true;
        self.load_receiver = Some(rx);

        std::thread::spawn(move || {
            let (result, all_services, all_processes, installed) = std::thread::scope(|s| {
                let h1 = s.spawn(|| collector::collect_all_entries());
                let h2 = s.spawn(|| services::collect_services().unwrap_or_default());
                let h3 = s.spawn(|| processes::collect_processes());
                let h4 = s.spawn(|| installed_apps::collect_installed_apps());
                (
                    h1.join().unwrap_or(collector::CollectionResult { entries: vec![], is_admin: false }),
                    h2.join().unwrap_or_default(),
                    h3.join().unwrap_or_default(),
                    h4.join().unwrap_or_default(),
                )
            });

            let _ = tx.send(LoadResult {
                entries: result.entries,
                all_services,
                all_processes,
                installed_apps: installed,
                is_admin: result.is_admin,
            });
        });
    }

    /// Lightweight process-only refresh (no loading overlay, no status message).
    fn start_process_refresh(&mut self) {
        if self.loading || self.process_refresh_receiver.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.process_refresh_receiver = Some(rx);
        std::thread::spawn(move || {
            let procs = processes::collect_processes();
            let _ = tx.send(procs);
        });
    }

    fn set_status(&mut self, text: &str, is_error: bool) {
        self.status = Some(StatusMessage {
            text: text.to_string(),
            is_error,
            when: Instant::now(),
        });
    }

    /// Get the currently visible entries for the active tab.
    fn active_entries(&self) -> Vec<&StartupEntry> {
        match self.active_tab {
            Tab::StartupApps => self.entries.iter().collect(),
            Tab::Services => {
                if self.hide_microsoft_services {
                    self.all_services
                        .iter()
                        .filter(|e| !services::is_microsoft_service(e))
                        .collect()
                } else {
                    self.all_services.iter().collect()
                }
            }
            Tab::Processes => Vec::new(), // Processes tab uses its own data model
            Tab::Installed => Vec::new(), // Installed tab uses its own data model
        }
    }

    /// Get mutable reference to the correct entry by tab + visible index.
    fn get_entry_by_visible_index(&self, index: usize) -> Option<&StartupEntry> {
        self.active_entries().get(index).copied()
    }

    fn execute_action(&mut self, action: PendingAction) {
        // Properties action
        if let PendingAction::Properties(i) = &action {
            if self.active_tab == Tab::Services {
                // Services tab: show service details dialog
                if let Some(entry) = self.get_entry_by_visible_index(*i) {
                    let entry = entry.clone();
                    if let Source::Service { service_name, .. } = &entry.source {
                        let description = services::get_service_description(service_name);
                        self.service_properties = Some(dialogs::ServicePropertiesInfo {
                            service_name: service_name.clone(),
                            display_name: entry.name.clone(),
                            description,
                            status: entry.run_state,
                            startup_type: entry.enabled,
                            executable_path: entry.command.clone(),
                            log_on_as: entry.runs_as.clone(),
                            product_name: entry.product_name.clone(),
                        });
                    }
                }
            } else {
                // StartupApps tab: show startup entry properties dialog
                if let Some(entry) = self.get_entry_by_visible_index(*i) {
                    self.startup_entry_properties =
                        Some(startup_entry_properties_from(entry));
                }
            }
            return;
        }

        let entry = match &action {
            PendingAction::Enable(i)
            | PendingAction::Disable(i)
            | PendingAction::Start(i)
            | PendingAction::Stop(i) => match self.get_entry_by_visible_index(*i) {
                Some(e) => e.clone(),
                None => return,
            },
            PendingAction::ConfirmDelete(_)
            | PendingAction::ConfirmUninstall(_)
            | PendingAction::Properties(_) => return,
        };

        let result = match &action {
            PendingAction::Enable(_) => {
                actions::enable_entry(&entry).map(|_| format!("Enabled '{}'", entry.name))
            }
            PendingAction::Disable(_) => {
                actions::disable_entry(&entry).map(|_| format!("Disabled '{}'", entry.name))
            }
            PendingAction::Start(_) => {
                actions::start_entry(&entry).map(|_| format!("Started '{}'", entry.name))
            }
            PendingAction::Stop(_) => {
                actions::stop_entry(&entry).map(|_| format!("Stopped '{}'", entry.name))
            }
            _ => return,
        };

        match result {
            Ok(msg) => {
                self.set_status(&msg, false);
                self.start_background_load();
            }
            Err(e) => {
                self.set_status(&format!("Error: {}", e), true);
            }
        }
    }

    fn delete_confirmed(&mut self, visible_index: usize) {
        let entry = match self.get_entry_by_visible_index(visible_index) {
            Some(e) => e.clone(),
            None => return,
        };
        let name = entry.name.clone();
        match actions::delete_entry(&entry) {
            Ok(_) => {
                self.set_status(&format!("Deleted '{}'", name), false);
                self.start_background_load();
            }
            Err(e) => {
                self.set_status(&format!("Error deleting '{}': {}", name, e), true);
            }
        }
    }

    fn uninstall_confirmed(&mut self, index: usize) {
        let app = match self.installed_apps.get(index) {
            Some(a) => a.clone(),
            None => return,
        };
        let name = app.display_name.clone();
        match run_shell_command(&app.uninstall_string) {
            Ok(()) => {
                self.set_status(&format!("Uninstalling '{}'...", name), false);
                // Poll the registry for the app to disappear (every 2s, up to 10 min)
                let (tx, rx) = mpsc::channel();
                self.rescan_receiver = Some(rx);
                let display_name = name.clone();
                std::thread::spawn(move || {
                    for _ in 0..300 {
                        std::thread::sleep(std::time::Duration::from_secs(2));
                        let apps = crate::installed_apps::collect_installed_apps();
                        let still_installed = apps.iter().any(|a| a.display_name == display_name);
                        if !still_installed {
                            break;
                        }
                    }
                    // Brief pause for any remaining registry cleanup
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    let _ = tx.send(());
                });
            }
            Err(e) => {
                self.set_status(&format!("Failed to uninstall '{}': {}", name, e), true);
            }
        }
    }

    fn filtered_process_count(&self) -> usize {
        if self.hide_windows_processes {
            self.all_processes
                .iter()
                .filter(|p| !processes::is_windows_process(p))
                .count()
        } else {
            self.all_processes.len()
        }
    }

    fn filtered_service_count(&self) -> usize {
        if self.hide_microsoft_services {
            self.all_services
                .iter()
                .filter(|e| !services::is_microsoft_service(e))
                .count()
        } else {
            self.all_services.len()
        }
    }

    fn export_csv(&mut self) {
        let tab_name = match self.active_tab {
            Tab::StartupApps => "startup-apps",
            Tab::Services => "services",
            Tab::Processes => "processes",
            Tab::Installed => "installed-apps",
        };
        let now = chrono::Local::now();
        let default_name = format!("{}-{}.csv", tab_name, now.format("%Y-%m-%d_%H%M%S"));

        let path = rfd::FileDialog::new()
            .set_file_name(&default_name)
            .add_filter("CSV Files", &["csv"])
            .save_file();

        let path = match path {
            Some(p) => p,
            None => return, // User cancelled
        };

        let result = match self.active_tab {
            Tab::StartupApps => self.write_startup_apps_csv(&path),
            Tab::Services => self.write_services_csv(&path),
            Tab::Processes => self.write_processes_csv(&path),
            Tab::Installed => self.write_installed_apps_csv(&path),
        };

        match result {
            Ok(count) => {
                self.set_status(
                    &format!("Exported {} rows to {}", count, path.display()),
                    false,
                );
            }
            Err(e) => {
                self.set_status(&format!("Export failed: {}", e), true);
            }
        }
    }

    fn write_startup_apps_csv(&self, path: &std::path::Path) -> Result<usize, String> {
        let entries = self.active_entries();
        let mut file = std::fs::File::create(path).map_err(|e| e.to_string())?;

        writeln!(file, "Name,Product Name,Command,Source,Status,State,Runs As,Visible As,Last Ran")
            .map_err(|e| e.to_string())?;

        for entry in &entries {
            let source = entry.source.display_location();
            let visible_as = if entry.requires_admin { "Admin" } else { "User" };
            let last_ran = match entry.last_ran {
                Some(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
                None => String::new(),
            };
            writeln!(
                file,
                "{},{},{},{},{},{},{},{},{}",
                csv_escape(&entry.name),
                csv_escape(&entry.product_name),
                csv_escape(&entry.command),
                csv_escape(&source),
                entry.enabled,
                entry.run_state,
                csv_escape(&entry.runs_as),
                visible_as,
                last_ran,
            )
            .map_err(|e| e.to_string())?;
        }

        Ok(entries.len())
    }

    fn write_services_csv(&self, path: &std::path::Path) -> Result<usize, String> {
        let entries = self.active_entries();
        let mut file = std::fs::File::create(path).map_err(|e| e.to_string())?;

        writeln!(file, "Name,Product Name,Command,Status,State,Runs As,Visible As,Last Started")
            .map_err(|e| e.to_string())?;

        for entry in &entries {
            let visible_as = if entry.requires_admin { "Admin" } else { "User" };
            let last_started = match entry.last_ran {
                Some(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
                None => String::new(),
            };
            writeln!(
                file,
                "{},{},{},{},{},{},{},{}",
                csv_escape(&entry.name),
                csv_escape(&entry.product_name),
                csv_escape(&entry.command),
                entry.enabled,
                entry.run_state,
                csv_escape(&entry.runs_as),
                visible_as,
                last_started,
            )
            .map_err(|e| e.to_string())?;
        }

        Ok(entries.len())
    }

    fn write_processes_csv(&self, path: &std::path::Path) -> Result<usize, String> {
        let rows = processes::build_visible_tree(
            &self.all_processes,
            &self.expanded_pids,
            self.hide_windows_processes,
        );
        let mut file = std::fs::File::create(path).map_err(|e| e.to_string())?;

        writeln!(file, "PID,Parent PID,Name,Product Name,Path,CPU %,Memory,Disk Read,Disk Write,Start Time")
            .map_err(|e| e.to_string())?;

        for row in &rows {
            let proc = row.process;
            let ppid = proc
                .parent_pid
                .map(|p| p.to_string())
                .unwrap_or_default();
            let cpu = format!("{:.1}", proc.cpu_usage);
            let memory = format_memory_csv(proc.memory_bytes);
            let disk_read = format_memory_csv(proc.disk_read_bytes);
            let disk_write = format_memory_csv(proc.disk_write_bytes);
            let start_time = match proc.start_time {
                Some(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
                None => String::new(),
            };
            writeln!(
                file,
                "{},{},{},{},{},{},{},{},{},{}",
                proc.pid,
                ppid,
                csv_escape(&proc.name),
                csv_escape(&proc.product_name),
                csv_escape(&proc.exe_path),
                cpu,
                memory,
                disk_read,
                disk_write,
                start_time,
            )
            .map_err(|e| e.to_string())?;
        }

        Ok(rows.len())
    }

    fn write_installed_apps_csv(&self, path: &std::path::Path) -> Result<usize, String> {
        let mut file = std::fs::File::create(path).map_err(|e| e.to_string())?;

        writeln!(
            file,
            "Name,Publisher,Version,Install Date,Size (KB),Uninstall Command,Modify Path,Install Location"
        )
        .map_err(|e| e.to_string())?;

        for app in &self.installed_apps {
            let modify = app.modify_path.as_deref().unwrap_or("");
            writeln!(
                file,
                "{},{},{},{},{},{},{},{}",
                csv_escape(&app.display_name),
                csv_escape(&app.publisher),
                csv_escape(&app.display_version),
                csv_escape(&app.install_date),
                app.estimated_size_kb,
                csv_escape(&app.uninstall_string),
                csv_escape(modify),
                csv_escape(&app.install_location),
            )
            .map_err(|e| e.to_string())?;
        }

        Ok(self.installed_apps.len())
    }
}

impl eframe::App for StartupApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Force dark mode every frame (overrides any persisted theme)
        ctx.set_visuals(egui::Visuals::dark());

        // Check for background load completion
        if let Some(rx) = &self.load_receiver {
            if let Ok(result) = rx.try_recv() {
                self.entries = result.entries;
                self.all_services = result.all_services;
                self.all_processes = result.all_processes;
                self.installed_apps = result.installed_apps;
                // Auto-expand all processes that have children
                self.expanded_pids = processes::parent_pids(&self.all_processes);
                self.is_admin = result.is_admin;
                self.loading = false;
                self.load_receiver = None;
                self.last_process_refresh = Instant::now();
                self.selected_row = None;
                self.hovered_row = None;
            }
        }

        // Fire rescan after uninstaller process exits
        if let Some(rx) = &self.rescan_receiver {
            if rx.try_recv().is_ok() {
                self.rescan_receiver = None;
                self.start_background_load();
            } else {
                // Keep polling while waiting for the uninstaller to finish
                ctx.request_repaint_after(std::time::Duration::from_millis(500));
            }
        }

        // Check for process-only refresh completion (auto-refresh, no overlay)
        if let Some(rx) = &self.process_refresh_receiver {
            if let Ok(new_procs) = rx.try_recv() {
                self.all_processes = new_procs;
                self.expanded_pids = processes::parent_pids(&self.all_processes);
                self.last_process_refresh = Instant::now();
                self.process_refresh_receiver = None;
            }
        }

        // Auto-refresh processes every 3 seconds when enabled and on the Processes tab
        if self.auto_refresh_processes && self.active_tab == Tab::Processes {
            if self.last_process_refresh.elapsed().as_secs() >= 3 {
                self.start_process_refresh();
            }
            // Keep requesting repaints so we check the timer regularly
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }

        // Draw a border around the entire window
        let window_rect = ctx.input(|i| i.viewport_rect());
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("window_border"),
        ));
        painter.rect_stroke(
            window_rect,
            0.0,
            egui::Stroke::new(1.0, egui::Color32::from_rgb(140, 140, 140)),
            egui::StrokeKind::Inside,
        );

        // Edge resize handles (since OS decorations are disabled)
        {
            let margin = 5.0;
            let pointer_pos = ctx.input(|i| i.pointer.interact_pos());
            if let Some(pos) = pointer_pos {
                let r = window_rect;
                let near_left = pos.x - r.left() < margin;
                let near_right = r.right() - pos.x < margin;
                let near_top = pos.y - r.top() < margin;
                let near_bottom = r.bottom() - pos.y < margin;

                let direction = match (near_left, near_right, near_top, near_bottom) {
                    (true, _, true, _) => Some(egui::ResizeDirection::NorthWest),
                    (true, _, _, true) => Some(egui::ResizeDirection::SouthWest),
                    (_, true, true, _) => Some(egui::ResizeDirection::NorthEast),
                    (_, true, _, true) => Some(egui::ResizeDirection::SouthEast),
                    (true, _, _, _) => Some(egui::ResizeDirection::West),
                    (_, true, _, _) => Some(egui::ResizeDirection::East),
                    (_, _, true, _) => Some(egui::ResizeDirection::North),
                    (_, _, _, true) => Some(egui::ResizeDirection::South),
                    _ => None,
                };

                if let Some(dir) = direction {
                    let cursor = match dir {
                        egui::ResizeDirection::North | egui::ResizeDirection::South => {
                            egui::CursorIcon::ResizeVertical
                        }
                        egui::ResizeDirection::East | egui::ResizeDirection::West => {
                            egui::CursorIcon::ResizeHorizontal
                        }
                        egui::ResizeDirection::NorthWest | egui::ResizeDirection::SouthEast => {
                            egui::CursorIcon::ResizeNwSe
                        }
                        egui::ResizeDirection::NorthEast | egui::ResizeDirection::SouthWest => {
                            egui::CursorIcon::ResizeNeSw
                        }
                    };
                    ctx.set_cursor_icon(cursor);

                    if ctx.input(|i| i.pointer.any_pressed()) {
                        ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
                    }
                }
            }
        }

        // Custom title bar (no OS decorations)
        egui::TopBottomPanel::top("title_bar")
            .frame(
                egui::Frame::side_top_panel(&ctx.style())
                    .inner_margin(egui::Margin {
                        left: 4,
                        right: 4,
                        top: 4,
                        bottom: 4,
                    }),
            )
            .show(ctx, |ui| {
            // Register drag interaction FIRST (lower priority than buttons added later)
            let title_bar_rect = ui.max_rect();
            let title_bar_response = ui.interact(
                title_bar_rect,
                egui::Id::new("title_bar_drag"),
                egui::Sense::click_and_drag(),
            );

            // Buttons and labels (higher priority, drawn on top of drag area)
            // Track whether any widget is hovered so drag doesn't fire from buttons
            let any_widget_hovered = ui.horizontal(|ui| {
                let mut hovered = false;

                // Disable tabs and action buttons while loading (window controls stay enabled)
                if self.loading {
                    ui.disable();
                }

                // Tab definitions
                let svc_count = self.filtered_service_count();
                let proc_count = self.filtered_process_count();
                let tabs: &[(Tab, String)] = &[
                    (Tab::Installed, format!("Installed Apps: {}", self.installed_apps.len())),
                    (Tab::StartupApps, format!("Startup Apps: {}", self.entries.len())),
                    (Tab::Processes, format!("Processes: {}", proc_count)),
                    (Tab::Services, format!("Services: {}", svc_count)),
                ];

                let selected_bg = egui::Color32::from_rgb(50, 50, 55);
                let hover_bg = egui::Color32::from_rgb(45, 45, 50);
                let accent = egui::Color32::from_rgb(100, 140, 200);

                for (tab, label) in tabs {
                    let is_selected = self.active_tab == *tab;
                    let text_color = if is_selected {
                        egui::Color32::WHITE
                    } else {
                        egui::Color32::from_rgb(170, 170, 170)
                    };

                    let r = ui.allocate_ui(egui::vec2(ui.available_height() * 4.0, ui.available_height()), |ui| {
                        let desired = ui.painter().layout_no_wrap(
                            label.to_string(),
                            egui::FontId::proportional(13.0),
                            text_color,
                        );
                        let padded_w = desired.rect.width() + 20.0;
                        let (rect, resp) = ui.allocate_exact_size(
                            egui::vec2(padded_w, ui.available_height()),
                            egui::Sense::click(),
                        );

                        let bg = if is_selected {
                            selected_bg
                        } else if resp.hovered() {
                            hover_bg
                        } else {
                            egui::Color32::TRANSPARENT
                        };

                        // Tab background with rounded top corners
                        let rounding = egui::CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 };
                        ui.painter().rect_filled(rect, rounding, bg);

                        // Accent underline for selected tab
                        if is_selected {
                            let line_rect = egui::Rect::from_min_size(
                                egui::pos2(rect.left(), rect.bottom() - 2.0),
                                egui::vec2(rect.width(), 2.0),
                            );
                            ui.painter().rect_filled(line_rect, 0.0, accent);
                        }

                        // Label centered in tab
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            label,
                            egui::FontId::proportional(13.0),
                            text_color,
                        );

                        resp
                    });

                    let resp = r.inner;
                    hovered |= resp.hovered();
                    if resp.clicked() && self.active_tab != *tab {
                        self.active_tab = *tab;
                        self.selected_row = None;
                        self.hovered_row = None;
                        self.pending_action = None;
                    }
                }

                ui.separator();

                // Checkbox for services tab
                if self.active_tab == Tab::Services {
                    let r = ui.checkbox(&mut self.hide_microsoft_services, "Hide Windows Services");
                    hovered |= r.hovered();
                    if r.changed() {
                        self.selected_row = None;
                        self.hovered_row = None;
                    }
                    ui.separator();
                }

                // Checkboxes for processes tab
                if self.active_tab == Tab::Processes {
                    let r = ui.checkbox(&mut self.hide_windows_processes, "Hide Windows Processes");
                    hovered |= r.hovered();
                    if r.changed() {
                        self.selected_row = None;
                        self.hovered_row = None;
                    }
                    let r = ui.checkbox(&mut self.auto_refresh_processes, "Auto-Refresh");
                    hovered |= r.hovered();
                    ui.separator();
                }

                // Global Refresh + Export buttons
                let r = ui.add_enabled(!self.loading, egui::Button::new("Refresh"));
                hovered |= r.hovered();
                if r.clicked() {
                    self.start_background_load();
                }
                let r = ui.add_enabled(!self.loading, egui::Button::new("Export"));
                hovered |= r.hovered();
                if r.clicked() {
                    self.export_csv();
                }

                ui.separator();

                // Admin indicator (draggable like title bar)
                if self.is_admin {
                    let r = ui.add(
                        egui::Label::new(
                            egui::RichText::new("Running as Administrator")
                                .color(egui::Color32::from_rgb(80, 200, 80)),
                        )
                        .sense(egui::Sense::click_and_drag()),
                    );
                    if r.drag_started() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    } else if r.double_clicked() {
                        let is_max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_max));
                    }
                } else {
                    let r = ui.add(
                        egui::Label::new(
                            egui::RichText::new("Standard User")
                                .color(egui::Color32::from_rgb(230, 160, 50)),
                        )
                        .sense(egui::Sense::click_and_drag()),
                    );
                    if r.drag_started() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    } else if r.double_clicked() {
                        let is_max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_max));
                    }
                    let r = ui.button("Restart as Admin");
                    hovered |= r.hovered();
                    if r.clicked() {
                        // Save current task paths so admin mode can detect truly new entries
                        collector::save_nonadmin_task_paths(&self.entries);
                        restart_as_admin();
                    }
                }

                // Push window control buttons to the right
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let btn_size = egui::vec2(30.0, 18.0);
                    // Close
                    let r = ui.add_sized(btn_size, egui::Button::new("X"));
                    hovered |= r.hovered();
                    if r.clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    // Maximize / Restore
                    let is_max = ctx.input(|i| {
                        i.viewport().maximized.unwrap_or(false)
                    });
                    let max_icon = if is_max { "\u{25A3}" } else { "\u{25A1}" };
                    let r = ui.add_sized(btn_size, egui::Button::new(max_icon));
                    hovered |= r.hovered();
                    if r.clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_max));
                    }
                    // Minimize: em dash
                    let r = ui.add_sized(btn_size, egui::Button::new("\u{2014}"));
                    hovered |= r.hovered();
                    if r.clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }
                });

                hovered
            }).inner;

            // Only handle drag/double-click on empty title bar space
            if !any_widget_hovered {
                if title_bar_response.double_clicked() {
                    let is_max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_max));
                } else if title_bar_response.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
            }
        });

        // Bottom panel: status bar
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(status) = &self.status {
                    // Auto-expire after 8 seconds
                    if status.when.elapsed().as_secs() < 8 {
                        let color = if status.is_error {
                            egui::Color32::from_rgb(230, 80, 80)
                        } else {
                            egui::Color32::from_rgb(80, 200, 80)
                        };
                        ui.colored_label(color, &status.text);
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let link = ui.add(
                        egui::Link::new(
                            egui::RichText::new("App Manager v1.0.0").small(),
                        ),
                    );
                    if link.clicked() {
                        self.show_about = true;
                    }
                });
            });
        });

        // Central panel: table with horizontal + vertical scrolling
        egui::CentralPanel::default().show(ctx, |ui| {
            // Disable content interaction while loading/scanning
            if self.loading {
                ui.disable();
            }

            // Use solid (non-floating) horizontal scrollbar so it has
            // dedicated space just above the status bar.
            ui.style_mut().spacing.scroll.floating = false;

            // Hide scrollbars until data is loaded
            let scroll_visibility = if self.loading {
                egui::scroll_area::ScrollBarVisibility::AlwaysHidden
            } else {
                egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded
            };

            match self.active_tab {
                Tab::StartupApps | Tab::Services => {
                    let visible_entries: Vec<StartupEntry> = self.active_entries().into_iter().cloned().collect();
                    let (col3_header, last_time_header) = match self.active_tab {
                        Tab::StartupApps => (Some("Source"), "Last Ran"),
                        Tab::Services => (None, "Last Started"),
                        _ => unreachable!(),
                    };

                    egui::ScrollArea::horizontal()
                        .scroll_bar_visibility(scroll_visibility)
                        .auto_shrink(false)
                        .show(ui, |ui| {
                        let show_delete = self.active_tab == Tab::StartupApps;
                        let show_properties = true;
                        let result = table::render_table(ui, &visible_entries, self.selected_row, self.hovered_row, col3_header, last_time_header, show_delete, show_properties);
                        self.hovered_row = result.hovered_row;
                        if let Some(clicked) = result.clicked_row {
                            self.selected_row = Some(clicked);
                        }
                        if let Some(action) = result.action {
                            match &action {
                                PendingAction::ConfirmDelete(_) => {
                                    self.pending_action = Some(action);
                                }
                                _ => {
                                    self.execute_action(action);
                                }
                            }
                        }
                        // Double-click opens properties dialog
                        if let Some(index) = result.double_clicked_row {
                            self.execute_action(PendingAction::Properties(index));
                        }
                    });
                }
                Tab::Installed => {
                    egui::ScrollArea::horizontal()
                        .scroll_bar_visibility(scroll_visibility)
                        .auto_shrink(false)
                        .show(ui, |ui| {
                        let result = installed_table::render_installed_table(
                            ui,
                            &self.installed_apps,
                            self.selected_row,
                            self.hovered_row,
                        );
                        self.hovered_row = result.hovered_row;
                        if let Some(clicked) = result.clicked_row {
                            self.selected_row = Some(clicked);
                        }
                        if let Some(action) = result.action {
                            match action {
                                installed_table::InstalledAppAction::Modify(i) => {
                                    if let Some(app) = self.installed_apps.get(i) {
                                        if let Some(ref path) = app.modify_path {
                                            let name = app.display_name.clone();
                                            match run_shell_command(path) {
                                                Ok(()) => self.set_status(
                                                    &format!("Launched modify for '{}'", name),
                                                    false,
                                                ),
                                                Err(e) => self.set_status(
                                                    &format!("Failed to modify '{}': {}", name, e),
                                                    true,
                                                ),
                                            }
                                        }
                                    }
                                }
                                installed_table::InstalledAppAction::Uninstall(i) => {
                                    self.pending_action = Some(PendingAction::ConfirmUninstall(i));
                                }
                            }
                        }
                    });
                }
                Tab::Processes => {
                    let procs = self.all_processes.clone();
                    let rows = processes::build_visible_tree(
                        &procs,
                        &self.expanded_pids,
                        self.hide_windows_processes,
                    );
                    egui::ScrollArea::horizontal()
                        .scroll_bar_visibility(scroll_visibility)
                        .auto_shrink(false)
                        .show(ui, |ui| {
                        let result = process_table::render_process_table(
                            ui,
                            &rows,
                            self.selected_row,
                            self.hovered_row,
                        );
                        self.hovered_row = result.hovered_row;
                        if let Some(clicked) = result.clicked_row {
                            self.selected_row = Some(clicked);
                        }
                        // Double-click on Processes tab opens process properties dialog
                        if let Some(index) = result.double_clicked_row {
                            if let Some(row) = rows.get(index) {
                                self.process_properties = Some(process_properties_from(row.process));
                            }
                        }
                        if let Some(action) = result.action {
                            match action {
                                process_table::ProcessAction::ToggleExpand(pid) => {
                                    if !self.expanded_pids.remove(&pid) {
                                        self.expanded_pids.insert(pid);
                                    }
                                }
                                process_table::ProcessAction::Kill(index) => {
                                    if let Some(row) = rows.get(index) {
                                        let pid = row.process.pid;
                                        let name = row.process.name.clone();
                                        match kill_process(pid) {
                                            Ok(_) => {
                                                self.set_status(
                                                    &format!("Killed '{}' (PID {})", name, pid),
                                                    false,
                                                );
                                                self.start_background_load();
                                            }
                                            Err(e) => {
                                                self.set_status(
                                                    &format!("Failed to kill PID {}: {}", pid, e),
                                                    true,
                                                );
                                            }
                                        }
                                    }
                                }
                                process_table::ProcessAction::Properties(index) => {
                                    if let Some(row) = rows.get(index) {
                                        self.process_properties =
                                            Some(process_properties_from(row.process));
                                    }
                                }
                            }
                        }
                    });
                }
            }
        });

        // Delete confirmation dialog
        if let Some(PendingAction::ConfirmDelete(index)) = self.pending_action.clone() {
            let visible = self.active_entries();
            let name = if index < visible.len() {
                visible[index].name.clone()
            } else {
                "Unknown".to_string()
            };

            match dialogs::show_delete_confirmation(ctx, &name) {
                dialogs::DialogResult::Confirmed => {
                    self.pending_action = None;
                    self.delete_confirmed(index);
                }
                dialogs::DialogResult::Cancelled => {
                    self.pending_action = None;
                }
                dialogs::DialogResult::Open => {
                    // Still showing
                }
            }
        }

        // Uninstall confirmation dialog
        if let Some(PendingAction::ConfirmUninstall(index)) = self.pending_action.clone() {
            let name = if let Some(app) = self.installed_apps.get(index) {
                app.display_name.clone()
            } else {
                "Unknown".to_string()
            };

            match dialogs::show_uninstall_confirmation(ctx, &name) {
                dialogs::DialogResult::Confirmed => {
                    self.pending_action = None;
                    self.uninstall_confirmed(index);
                }
                dialogs::DialogResult::Cancelled => {
                    self.pending_action = None;
                }
                dialogs::DialogResult::Open => {
                    // Still showing
                }
            }
        }

        // Service properties dialog
        if let Some(info) = &self.service_properties.clone() {
            match dialogs::show_service_properties(ctx, info) {
                dialogs::DialogResult::Cancelled => {
                    self.service_properties = None;
                }
                dialogs::DialogResult::Open => {}
                _ => {}
            }
        }

        // Process properties dialog
        if let Some(info) = &self.process_properties.clone() {
            match dialogs::show_process_properties(ctx, info) {
                dialogs::DialogResult::Cancelled => {
                    self.process_properties = None;
                }
                dialogs::DialogResult::Open => {}
                _ => {}
            }
        }

        // Startup entry properties dialog
        if let Some(info) = &self.startup_entry_properties.clone() {
            match dialogs::show_startup_entry_properties(ctx, info) {
                dialogs::DialogResult::Cancelled => {
                    self.startup_entry_properties = None;
                }
                dialogs::DialogResult::Open => {}
                _ => {}
            }
        }

        // About dialog
        if self.show_about {
            match dialogs::show_about(ctx) {
                dialogs::DialogResult::Cancelled => {
                    self.show_about = false;
                }
                dialogs::DialogResult::Open => {}
                _ => {}
            }
        }

        // Escape key closes open dialogs
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.show_about {
                self.show_about = false;
            } else if self.startup_entry_properties.is_some() {
                self.startup_entry_properties = None;
            } else if self.process_properties.is_some() {
                self.process_properties = None;
            } else if self.service_properties.is_some() {
                self.service_properties = None;
            }
        }

        // Loading overlay
        if self.loading {
            egui::Area::new(egui::Id::new("loading_overlay"))
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::popup(&ctx.style())
                        .inner_margin(egui::Margin::symmetric(24, 16))
                        .show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.spinner();
                                ui.add_space(8.0);
                                ui.label(egui::RichText::new("Loading...").color(egui::Color32::WHITE));
                            });
                        });
                });

            ctx.request_repaint();
        }
    }
}

fn restart_as_admin() {
    let exe = std::env::current_exe().unwrap_or_default();
    let exe_wide: Vec<u16> = exe.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let verb: Vec<u16> = "runas\0".encode_utf16().collect();

    unsafe {
        windows::Win32::UI::Shell::ShellExecuteW(
            None,
            windows::core::PCWSTR(verb.as_ptr()),
            windows::core::PCWSTR(exe_wide.as_ptr()),
            None,
            None,
            windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        );
    }
    std::process::exit(0);
}

fn csv_escape(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

fn format_memory_csv(bytes: u64) -> String {
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

/// Parse a command string into (executable, arguments).
///
/// Handles three forms commonly found in Windows uninstall strings:
/// 1. Quoted: `"C:\Program Files\app.exe" /S` → split at closing quote
/// 2. Unquoted with `.exe`: `C:\Program Files\app.exe /S` → split after `.exe`
/// 3. Fallback: split on first whitespace
fn split_command(command: &str) -> (String, String) {
    let cmd = command.trim();

    // Case 1: Quoted executable
    if cmd.starts_with('"') {
        if let Some(end) = cmd[1..].find('"') {
            let exe = &cmd[1..1 + end];
            let args = cmd[1 + end + 1..].trim();
            return (exe.to_string(), args.to_string());
        }
    }

    // Case 2: Find .exe boundary (case-insensitive) — handles unquoted paths
    // with spaces like C:\Program Files (x86)\App\uninstall.exe /silent
    let lower = cmd.to_lowercase();
    if let Some(pos) = lower.find(".exe") {
        let end = pos + 4;
        let exe = &cmd[..end];
        let args = cmd[end..].trim();
        return (exe.to_string(), args.to_string());
    }

    // Case 3: No .exe found — split on first whitespace
    if let Some(pos) = cmd.find(char::is_whitespace) {
        let exe = &cmd[..pos];
        let args = cmd[pos..].trim();
        (exe.to_string(), args.to_string())
    } else {
        (cmd.to_string(), String::new())
    }
}

/// Run a shell command string (like an uninstall or modify path) via ShellExecuteW
/// with "runas" verb so UAC elevation is requested when needed.
fn run_shell_command(command: &str) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::core::PCWSTR;

    let (exe, args) = split_command(command);

    let exe_wide: Vec<u16> = std::ffi::OsStr::new(&exe)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let args_wide: Vec<u16> = std::ffi::OsStr::new(&args)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let verb_wide: Vec<u16> = std::ffi::OsStr::new("runas")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb_wide.as_ptr()),
            PCWSTR(exe_wide.as_ptr()),
            PCWSTR(args_wide.as_ptr()),
            PCWSTR::null(),
            windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        )
    };

    if result.0 as usize > 32 {
        Ok(())
    } else {
        Err(format!(
            "ShellExecute failed (code {}): {}",
            result.0 as usize, exe
        ))
    }
}

fn kill_process(pid: u32) -> Result<(), String> {
    let output = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(stderr.trim().to_string())
    }
}

fn startup_entry_properties_from(entry: &StartupEntry) -> dialogs::StartupEntryPropertiesInfo {
    dialogs::StartupEntryPropertiesInfo {
        name: entry.name.clone(),
        product_name: entry.product_name.clone(),
        command: entry.command.clone(),
        source: entry.source.clone(),
        enabled: entry.enabled,
        run_state: entry.run_state,
        runs_as: entry.runs_as.clone(),
        requires_admin: entry.requires_admin,
        last_ran: entry.last_ran,
    }
}

fn process_properties_from(proc: &ProcessInfo) -> dialogs::ProcessPropertiesInfo {
    dialogs::ProcessPropertiesInfo {
        pid: proc.pid,
        parent_pid: proc.parent_pid,
        name: proc.name.clone(),
        exe_path: proc.exe_path.clone(),
        command_line: proc.command_line.clone(),
        cpu_usage: proc.cpu_usage,
        memory_bytes: proc.memory_bytes,
        disk_read_bytes: proc.disk_read_bytes,
        disk_write_bytes: proc.disk_write_bytes,
        start_time: proc.start_time,
        product_name: proc.product_name.clone(),
        user_name: proc.user_name.clone(),
        is_elevated: proc.is_elevated,
    }
}

