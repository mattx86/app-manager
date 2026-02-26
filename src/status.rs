use crate::models::{EnabledStatus, RegistryHive, Source};
use chrono::{DateTime, Local};
use std::collections::HashMap;
use winreg::enums::*;
use winreg::RegKey;

pub struct ApprovalInfo {
    pub enabled: EnabledStatus,
    pub disabled_timestamp: Option<DateTime<Local>>,
}

const STARTUP_APPROVED_PATHS: &[(&str, RegistryHive)] = &[
    (
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run",
        RegistryHive::HKCU,
    ),
    (
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run",
        RegistryHive::HKLM,
    ),
    (
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run32",
        RegistryHive::HKCU,
    ),
    (
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run32",
        RegistryHive::HKLM,
    ),
    (
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder",
        RegistryHive::HKCU,
    ),
    (
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder",
        RegistryHive::HKLM,
    ),
];

fn filetime_to_datetime(ft: u64) -> Option<DateTime<Local>> {
    const FILETIME_UNIX_DIFF: u64 = 116_444_736_000_000_000;
    if ft < FILETIME_UNIX_DIFF || ft == 0 {
        return None;
    }
    let unix_100ns = ft - FILETIME_UNIX_DIFF;
    let secs = (unix_100ns / 10_000_000) as i64;
    let nanos = ((unix_100ns % 10_000_000) * 100) as u32;
    chrono::DateTime::from_timestamp(secs, nanos).map(|utc| utc.with_timezone(&Local))
}

fn parse_startup_approved(bytes: &[u8]) -> ApprovalInfo {
    if bytes.len() < 12 {
        return ApprovalInfo {
            enabled: EnabledStatus::Unknown,
            disabled_timestamp: None,
        };
    }

    let status_byte = bytes[0];
    let enabled = match status_byte {
        0x02 | 0x06 => EnabledStatus::Enabled,
        _ => EnabledStatus::Disabled,
    };

    let disabled_timestamp = if matches!(enabled, EnabledStatus::Disabled) {
        let ft_bytes: [u8; 8] = bytes[4..12].try_into().unwrap();
        let ft = u64::from_le_bytes(ft_bytes);
        filetime_to_datetime(ft)
    } else {
        None
    };

    ApprovalInfo {
        enabled,
        disabled_timestamp,
    }
}

/// Load all StartupApproved entries. Keys are formatted as "HIVE\path\valuename".
pub fn load_all_approvals() -> HashMap<String, ApprovalInfo> {
    let mut map = HashMap::new();

    for (path, hive) in STARTUP_APPROVED_PATHS {
        let predef = match hive {
            RegistryHive::HKCU => RegKey::predef(HKEY_CURRENT_USER),
            RegistryHive::HKLM => RegKey::predef(HKEY_LOCAL_MACHINE),
        };

        let key = match predef.open_subkey_with_flags(path, KEY_READ) {
            Ok(k) => k,
            Err(_) => continue,
        };

        for value in key.enum_values().flatten() {
            let (name, reg_value) = value;
            if name.is_empty() {
                continue;
            }

            let info = parse_startup_approved(&reg_value.bytes);
            let lookup_key = format!("{}\\{}\\{}", hive, path, name);
            map.insert(lookup_key, info);
        }
    }

    map
}

/// Get the approval status for a startup entry given its name and source.
pub fn get_approval_status(
    name: &str,
    source: &Source,
    approvals: &HashMap<String, ApprovalInfo>,
) -> (EnabledStatus, Option<DateTime<Local>>) {
    let lookup_key = match source {
        Source::RegistryRun { hive, .. } => {
            format!(
                "{}\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run\\{}",
                hive, name
            )
        }
        Source::RegistryRunOnce { .. } => {
            // RunOnce entries don't have StartupApproved entries
            return (EnabledStatus::Enabled, None);
        }
        Source::StartupFolder { path, is_common } => {
            let hive = if *is_common {
                RegistryHive::HKLM
            } else {
                RegistryHive::HKCU
            };
            // For startup folder, the lookup key is the filename (e.g. "Discord.lnk")
            let file_name = std::path::Path::new(path)
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or(name);
            format!(
                "{}\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\StartupFolder\\{}",
                hive, file_name
            )
        }
        Source::TaskScheduler { .. } => {
            // Task Scheduler uses its own enabled flag
            return (EnabledStatus::Unknown, None);
        }
        Source::Service { .. } => {
            // Services use their own start type
            return (EnabledStatus::Unknown, None);
        }
    };

    if let Some(info) = approvals.get(&lookup_key) {
        return (info.enabled, info.disabled_timestamp);
    }

    // Also check Run32 for registry entries
    if let Source::RegistryRun { hive, .. } = source {
        let run32_key = format!(
            "{}\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run32\\{}",
            hive, name
        );
        if let Some(info) = approvals.get(&run32_key) {
            return (info.enabled, info.disabled_timestamp);
        }
    }

    // No entry found = assume enabled (never toggled via Task Manager)
    (EnabledStatus::Enabled, None)
}
