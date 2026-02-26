#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod collector;
mod gui;
mod installed_apps;
mod models;
mod prefetch;
mod process;
mod registry;
mod processes;
mod services;
mod startup_folders;
mod status;
mod task_scheduler;
mod version_info;

fn main() -> eframe::Result {
    let icon_rgba = include_bytes!(concat!(env!("OUT_DIR"), "/icon_rgba.bin")).to_vec();
    let icon = eframe::egui::IconData {
        rgba: icon_rgba,
        width: 48,
        height: 48,
    };

    let win_w: f32 = 1200.0;
    let win_h: f32 = 700.0;

    // Center the window on the primary monitor
    let position = {
        use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
        let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) } as f32;
        let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) } as f32;
        eframe::egui::pos2(
            (screen_w - win_w) / 2.0,
            (screen_h - win_h) / 2.0,
        )
    };

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([win_w, win_h])
            .with_min_inner_size([800.0, 400.0])
            .with_position(position)
            .with_title("App Manager")
            .with_decorations(false)
            .with_icon(icon)
            .with_active(true),
        ..Default::default()
    };

    eframe::run_native(
        "App Manager",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(eframe::egui::Visuals::dark());
            Ok(Box::new(gui::StartupApp::new()))
        }),
    )
}
