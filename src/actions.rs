use crate::models::*;
use anyhow::{Context, Result};
use std::os::windows::process::CommandExt;
use std::process::Command;
use winreg::enums::*;
use winreg::RegKey;

const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Enable a startup entry.
pub fn enable_entry(entry: &StartupEntry) -> Result<()> {
    match &entry.source {
        Source::RegistryRun { hive, .. } => {
            set_startup_approved(hive, "Run", &entry.name, true)
        }
        Source::RegistryRunOnce { .. } => {
            anyhow::bail!("RunOnce entries cannot be toggled")
        }
        Source::StartupFolder { path, is_common } => {
            let hive = if *is_common {
                RegistryHive::HKLM
            } else {
                RegistryHive::HKCU
            };
            let file_name = std::path::Path::new(path)
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or(&entry.name);
            set_startup_approved(&hive, "StartupFolder", file_name, true)
        }
        Source::TaskScheduler { task_path } => {
            let output = Command::new("schtasks")
                .args(["/Change", "/TN", task_path, "/ENABLE"])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
                .context("Failed to run schtasks")?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("schtasks failed: {}", stderr.trim());
            }
            Ok(())
        }
        Source::Service { service_name, .. } => {
            let output = Command::new("sc")
                .args(["config", service_name, "start=", "auto"])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
                .context("Failed to run sc config")?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("sc config failed: {}", stderr.trim());
            }
            Ok(())
        }
    }
}

/// Disable a startup entry.
pub fn disable_entry(entry: &StartupEntry) -> Result<()> {
    match &entry.source {
        Source::RegistryRun { hive, .. } => {
            set_startup_approved(hive, "Run", &entry.name, false)
        }
        Source::RegistryRunOnce { .. } => {
            anyhow::bail!("RunOnce entries cannot be toggled")
        }
        Source::StartupFolder { path, is_common } => {
            let hive = if *is_common {
                RegistryHive::HKLM
            } else {
                RegistryHive::HKCU
            };
            let file_name = std::path::Path::new(path)
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or(&entry.name);
            set_startup_approved(&hive, "StartupFolder", file_name, false)
        }
        Source::TaskScheduler { task_path } => {
            let output = Command::new("schtasks")
                .args(["/Change", "/TN", task_path, "/DISABLE"])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
                .context("Failed to run schtasks")?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("schtasks failed: {}", stderr.trim());
            }
            Ok(())
        }
        Source::Service { service_name, .. } => {
            let output = Command::new("sc")
                .args(["config", service_name, "start=", "disabled"])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
                .context("Failed to run sc config")?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("sc config failed: {}", stderr.trim());
            }
            Ok(())
        }
    }
}

/// Start (launch) the process for a startup entry.
pub fn start_entry(entry: &StartupEntry) -> Result<()> {
    if let Source::Service { service_name, .. } = &entry.source {
        let output = Command::new("sc")
            .args(["start", service_name])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .context("Failed to run sc start")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("sc start failed: {}", stderr.trim());
        }
        return Ok(());
    }

    let (exe, args) = parse_command(&entry.command);
    Command::new(&exe)
        .args(&args)
        .spawn()
        .with_context(|| format!("Failed to start {}", exe))?;
    Ok(())
}

/// Stop (kill) the process for a startup entry.
pub fn stop_entry(entry: &StartupEntry) -> Result<()> {
    if let Source::Service { service_name, .. } = &entry.source {
        let output = Command::new("sc")
            .args(["stop", service_name])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .context("Failed to run sc stop")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("sc stop failed: {}", stderr.trim());
        }
        return Ok(());
    }

    let exe_name = entry
        .exe_name()
        .context("Could not determine executable name")?;

    // Find PIDs for this exe
    let mut sys = sysinfo::System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let mut killed = false;
    for process in sys.processes().values() {
        let name = process.name().to_string_lossy().to_lowercase();
        if name == exe_name {
            let pid = process.pid().as_u32();
            let output = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/F"])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
                .with_context(|| format!("Failed to run taskkill for PID {}", pid))?;
            if output.status.success() {
                killed = true;
            }
        }
    }

    if killed {
        Ok(())
    } else {
        anyhow::bail!("No running process found for {}", exe_name)
    }
}

/// Delete a startup entry entirely.
pub fn delete_entry(entry: &StartupEntry) -> Result<()> {
    match &entry.source {
        Source::RegistryRun { hive, key_path } | Source::RegistryRunOnce { hive, key_path } => {
            let predef = match hive {
                RegistryHive::HKCU => RegKey::predef(HKEY_CURRENT_USER),
                RegistryHive::HKLM => RegKey::predef(HKEY_LOCAL_MACHINE),
            };
            let key = predef
                .open_subkey_with_flags(key_path, KEY_SET_VALUE)
                .context("Failed to open registry key for writing")?;
            key.delete_value(&entry.name)
                .with_context(|| format!("Failed to delete value '{}'", entry.name))?;

            // Also clean up StartupApproved entry if it exists
            let _ = cleanup_startup_approved(hive, &entry.name);
            Ok(())
        }
        Source::StartupFolder { path, .. } => {
            std::fs::remove_file(path)
                .with_context(|| format!("Failed to delete file: {}", path))?;
            Ok(())
        }
        Source::TaskScheduler { task_path } => {
            let output = Command::new("schtasks")
                .args(["/Delete", "/TN", task_path, "/F"])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
                .context("Failed to run schtasks")?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("schtasks failed: {}", stderr.trim());
            }
            Ok(())
        }
        Source::Service { service_name, .. } => {
            let output = Command::new("sc")
                .args(["delete", service_name])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
                .context("Failed to run sc delete")?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("sc delete failed: {}", stderr.trim());
            }
            Ok(())
        }
    }
}

// --- Helpers ---

fn set_startup_approved(
    hive: &RegistryHive,
    subkey: &str,
    value_name: &str,
    enable: bool,
) -> Result<()> {
    let predef = match hive {
        RegistryHive::HKCU => RegKey::predef(HKEY_CURRENT_USER),
        RegistryHive::HKLM => RegKey::predef(HKEY_LOCAL_MACHINE),
    };

    let path = format!(
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\{}",
        subkey
    );

    let key = predef
        .open_subkey_with_flags(&path, KEY_READ | KEY_SET_VALUE)
        .with_context(|| format!("Failed to open {}", path))?;

    // Read existing value or create a new 12-byte buffer
    let mut data: Vec<u8> = key
        .get_raw_value(value_name)
        .map(|v| v.bytes)
        .unwrap_or_else(|_| vec![0u8; 12]);

    if data.len() < 12 {
        data.resize(12, 0);
    }

    if enable {
        data[0] = 0x02;
        // Zero out the FILETIME bytes
        for b in &mut data[4..12] {
            *b = 0;
        }
    } else {
        data[0] = 0x03;
        // Set current time as FILETIME
        let now = std::time::SystemTime::now();
        let since_epoch = now
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let filetime =
            (since_epoch.as_nanos() / 100) as u64 + 116_444_736_000_000_000u64;
        data[4..12].copy_from_slice(&filetime.to_le_bytes());
    }

    let reg_value = winreg::RegValue {
        vtype: REG_BINARY,
        bytes: data,
    };
    key.set_raw_value(value_name, &reg_value)
        .with_context(|| format!("Failed to write StartupApproved for '{}'", value_name))?;

    Ok(())
}

fn cleanup_startup_approved(hive: &RegistryHive, value_name: &str) -> Result<()> {
    let predef = match hive {
        RegistryHive::HKCU => RegKey::predef(HKEY_CURRENT_USER),
        RegistryHive::HKLM => RegKey::predef(HKEY_LOCAL_MACHINE),
    };

    for subkey in &["Run", "Run32", "StartupFolder"] {
        let path = format!(
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\{}",
            subkey
        );
        if let Ok(key) = predef.open_subkey_with_flags(&path, KEY_SET_VALUE) {
            let _ = key.delete_value(value_name);
        }
    }

    Ok(())
}

/// Parse a command string into (exe, args).
fn parse_command(command: &str) -> (String, Vec<String>) {
    let command = command.trim();
    if command.is_empty() {
        return (String::new(), Vec::new());
    }

    if let Some(stripped) = command.strip_prefix('"') {
        // Quoted path: "C:\path\to\exe.exe" arg1 arg2
        if let Some(end_quote) = stripped.find('"') {
            let exe = &stripped[..end_quote];
            let rest = stripped[end_quote + 1..].trim();
            let args: Vec<String> = if rest.is_empty() {
                Vec::new()
            } else {
                shell_split(rest)
            };
            return (exe.to_string(), args);
        }
    }

    // Unquoted: split on whitespace
    let parts: Vec<&str> = command.splitn(2, char::is_whitespace).collect();
    let exe = parts[0].to_string();
    let args = if parts.len() > 1 {
        shell_split(parts[1].trim())
    } else {
        Vec::new()
    };
    (exe, args)
}

/// Simple shell-like argument splitting (handles quoted args).
fn shell_split(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = s.chars();

    while let Some(c) = chars.next() {
        match c {
            '"' => in_quotes = !in_quotes,
            ' ' | '\t' if !in_quotes => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        args.push(current);
    }
    args
}
