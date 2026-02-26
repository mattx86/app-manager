use crate::models::InstalledApp;
use std::collections::HashSet;
use winreg::enums::*;
use winreg::{RegKey, HKEY};

const UNINSTALL_PATHS: &[(HKEY, &str)] = &[
    (
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
    ),
    (
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
    ),
    (
        HKEY_CURRENT_USER,
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
    ),
];

fn read_string(key: &RegKey, name: &str) -> String {
    key.get_value::<String, _>(name).unwrap_or_default()
}

fn read_dword(key: &RegKey, name: &str) -> u64 {
    key.get_value::<u32, _>(name)
        .map(|v| v as u64)
        .unwrap_or(0)
}

pub fn collect_installed_apps() -> Vec<InstalledApp> {
    let mut apps = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();

    for &(hive, path) in UNINSTALL_PATHS {
        let predef = RegKey::predef(hive);
        let key = match predef.open_subkey_with_flags(path, KEY_READ) {
            Ok(k) => k,
            Err(_) => continue,
        };

        for subkey_name in key.enum_keys().flatten() {
            let subkey = match key.open_subkey_with_flags(&subkey_name, KEY_READ) {
                Ok(k) => k,
                Err(_) => continue,
            };

            let display_name = read_string(&subkey, "DisplayName");
            if display_name.is_empty() {
                continue;
            }

            let uninstall_string = read_string(&subkey, "UninstallString");
            if uninstall_string.is_empty() {
                continue;
            }

            // Skip if we already have this app (dedup by display_name)
            let name_lower = display_name.to_lowercase();
            if seen_names.contains(&name_lower) {
                continue;
            }
            seen_names.insert(name_lower);

            // Skip system components (entries with SystemComponent=1)
            if read_dword(&subkey, "SystemComponent") == 1 {
                continue;
            }

            let modify_path = {
                let val = read_string(&subkey, "ModifyPath");
                if val.is_empty() { None } else { Some(val) }
            };

            apps.push(InstalledApp {
                display_name,
                publisher: read_string(&subkey, "Publisher"),
                display_version: read_string(&subkey, "DisplayVersion"),
                install_date: read_string(&subkey, "InstallDate"),
                estimated_size_kb: read_dword(&subkey, "EstimatedSize"),
                uninstall_string,
                modify_path,
                install_location: read_string(&subkey, "InstallLocation"),
            });
        }
    }

    apps.sort_by(|a, b| {
        a.display_name
            .to_lowercase()
            .cmp(&b.display_name.to_lowercase())
    });

    apps
}
