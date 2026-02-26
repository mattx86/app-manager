use crate::models::{Source, StartupEntry};
use std::path::PathBuf;

fn user_startup_folder() -> Option<PathBuf> {
    std::env::var("APPDATA").ok().map(|appdata| {
        PathBuf::from(appdata)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("Startup")
    })
}

fn common_startup_folder() -> Option<PathBuf> {
    std::env::var("ProgramData").ok().map(|pd| {
        PathBuf::from(pd)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("Startup")
    })
}

fn resolve_lnk(path: &std::path::Path) -> Option<String> {
    let shortcut = lnk::ShellLink::open(path).ok()?;
    let target = shortcut
        .link_info()
        .as_ref()
        .and_then(|li| li.local_base_path().clone())?;
    let args = shortcut
        .arguments()
        .as_ref()
        .map(|a| format!(" {}", a))
        .unwrap_or_default();
    Some(format!("{}{}", target, args))
}

fn scan_startup_folder(folder: &std::path::Path, is_common: bool) -> Vec<StartupEntry> {
    let mut entries = Vec::new();

    let read_dir = match std::fs::read_dir(folder) {
        Ok(rd) => rd,
        Err(_) => return entries,
    };

    for dir_entry in read_dir.flatten() {
        let path = dir_entry.path();
        if !path.is_file() {
            continue;
        }

        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        // Skip desktop.ini
        if file_name.eq_ignore_ascii_case("desktop.ini") {
            continue;
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let (name, command) = match ext.as_str() {
            "lnk" => {
                let display_name = file_name.trim_end_matches(".lnk").to_string();
                let target = resolve_lnk(&path)
                    .unwrap_or_else(|| path.to_string_lossy().to_string());
                (display_name, target)
            }
            "exe" | "bat" | "cmd" => {
                let display_name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&file_name)
                    .to_string();
                (display_name, path.to_string_lossy().to_string())
            }
            _ => continue,
        };

        let source = Source::StartupFolder {
            path: path.to_string_lossy().to_string(),
            is_common,
        };

        // For StartupApproved lookup, we need the filename (e.g., "Discord.lnk")
        let mut entry = StartupEntry::new(file_name.clone(), command, source);
        // Use the friendly name for display, keep file_name in entry for approval lookup
        entry.name = name;
        entries.push(entry);
    }

    entries
}

pub fn collect_startup_folder_entries() -> Vec<StartupEntry> {
    let mut entries = Vec::new();

    if let Some(folder) = user_startup_folder() {
        entries.extend(scan_startup_folder(&folder, false));
    }

    if let Some(folder) = common_startup_folder() {
        entries.extend(scan_startup_folder(&folder, true));
    }

    entries
}
