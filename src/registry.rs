use crate::models::{RegistryHive, Source, StartupEntry};
use winreg::enums::*;
use winreg::RegKey;

struct RunKeyInfo {
    path: &'static str,
    hive: RegistryHive,
    is_run_once: bool,
}

const RUN_KEYS: &[RunKeyInfo] = &[
    RunKeyInfo {
        path: r"Software\Microsoft\Windows\CurrentVersion\Run",
        hive: RegistryHive::HKCU,
        is_run_once: false,
    },
    RunKeyInfo {
        path: r"Software\Microsoft\Windows\CurrentVersion\Run",
        hive: RegistryHive::HKLM,
        is_run_once: false,
    },
    RunKeyInfo {
        path: r"Software\Microsoft\Windows\CurrentVersion\RunOnce",
        hive: RegistryHive::HKCU,
        is_run_once: true,
    },
    RunKeyInfo {
        path: r"Software\Microsoft\Windows\CurrentVersion\RunOnce",
        hive: RegistryHive::HKLM,
        is_run_once: true,
    },
    // 32-bit app entries on 64-bit Windows
    RunKeyInfo {
        path: r"Software\Wow6432Node\Microsoft\Windows\CurrentVersion\Run",
        hive: RegistryHive::HKLM,
        is_run_once: false,
    },
    RunKeyInfo {
        path: r"Software\Wow6432Node\Microsoft\Windows\CurrentVersion\RunOnce",
        hive: RegistryHive::HKLM,
        is_run_once: true,
    },
];

fn read_run_key(info: &RunKeyInfo) -> Vec<StartupEntry> {
    let predef = match info.hive {
        RegistryHive::HKCU => RegKey::predef(HKEY_CURRENT_USER),
        RegistryHive::HKLM => RegKey::predef(HKEY_LOCAL_MACHINE),
    };

    let key = match predef.open_subkey_with_flags(info.path, KEY_READ) {
        Ok(k) => k,
        Err(_) => return Vec::new(),
    };

    let mut entries = Vec::new();
    for value in key.enum_values().flatten() {
        let (name, reg_value) = value;
        if name.is_empty() {
            continue;
        }

        let command = match reg_value.vtype {
            REG_SZ | REG_EXPAND_SZ => String::from_utf16_lossy(
                &reg_value
                    .bytes
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect::<Vec<u16>>(),
            )
            .trim_end_matches('\0')
            .to_string(),
            _ => continue,
        };

        let source = if info.is_run_once {
            Source::RegistryRunOnce {
                hive: info.hive,
                key_path: info.path.to_string(),
            }
        } else {
            Source::RegistryRun {
                hive: info.hive,
                key_path: info.path.to_string(),
            }
        };

        entries.push(StartupEntry::new(name, command, source));
    }

    entries
}

pub fn collect_registry_entries() -> Vec<StartupEntry> {
    let mut entries = Vec::new();
    for info in RUN_KEYS {
        entries.extend(read_run_key(info));
    }
    entries
}
