use chrono::{DateTime, Local};
use std::fmt;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[allow(clippy::upper_case_acronyms)]
pub enum RegistryHive {
    HKCU,
    HKLM,
}

impl fmt::Display for RegistryHive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegistryHive::HKCU => write!(f, "HKCU"),
            RegistryHive::HKLM => write!(f, "HKLM"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    RegistryRun {
        hive: RegistryHive,
        key_path: String,
    },
    RegistryRunOnce {
        hive: RegistryHive,
        key_path: String,
    },
    StartupFolder {
        path: String,
        is_common: bool,
    },
    TaskScheduler {
        task_path: String,
    },
    Service {
        service_name: String,
        command_line: String,
    },
}

impl Source {
    pub fn display_location(&self) -> String {
        match self {
            Source::RegistryRun { hive, key_path } => format!("{}\\{}", hive, key_path),
            Source::RegistryRunOnce { hive, key_path } => format!("{}\\{}", hive, key_path),
            Source::StartupFolder { is_common, .. } => {
                if *is_common {
                    "Common Startup Folder".to_string()
                } else {
                    "User Startup Folder".to_string()
                }
            }
            Source::TaskScheduler { task_path } => format!("Task: {}", task_path),
            Source::Service { command_line, .. } => command_line.clone(),
        }
    }

    pub fn sort_key(&self) -> u8 {
        match self {
            Source::RegistryRun { .. } => 0,
            Source::RegistryRunOnce { .. } => 1,
            Source::StartupFolder { .. } => 2,
            Source::TaskScheduler { .. } => 3,
            Source::Service { .. } => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnabledStatus {
    Enabled,
    Disabled,
    Manual,
    Unknown,
}

impl fmt::Display for EnabledStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EnabledStatus::Enabled => write!(f, "Enabled"),
            EnabledStatus::Disabled => write!(f, "Disabled"),
            EnabledStatus::Manual => write!(f, "Manual"),
            EnabledStatus::Unknown => write!(f, "Unknown"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    Running,
    Stopped,
}

impl fmt::Display for RunState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunState::Running => write!(f, "Running"),
            RunState::Stopped => write!(f, "Stopped"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StartupEntry {
    pub name: String,
    pub command: String,
    pub source: Source,
    pub enabled: EnabledStatus,
    pub run_state: RunState,
    pub last_ran: Option<DateTime<Local>>,
    pub requires_admin: bool,
    pub runs_as: String,
    pub product_name: String,
}

impl StartupEntry {
    pub fn new(name: String, command: String, source: Source) -> Self {
        Self {
            name,
            command,
            source,
            enabled: EnabledStatus::Unknown,
            run_state: RunState::Stopped,
            last_ran: None,
            requires_admin: false,
            runs_as: String::new(),
            product_name: String::new(),
        }
    }

    pub fn exe_name(&self) -> Option<String> {
        extract_exe_name(&self.command)
    }
}

pub fn extract_exe_name(command: &str) -> Option<String> {
    let command = command.trim();
    if command.is_empty() {
        return None;
    }

    let path_str = if let Some(stripped) = command.strip_prefix('"') {
        stripped.split('"').next()?
    } else {
        command.split_whitespace().next()?
    };

    // Expand common environment variables
    let expanded = expand_env_vars(path_str);

    Path::new(&expanded)
        .file_name()?
        .to_str()
        .map(|s| s.to_lowercase())
}

fn expand_env_vars(s: &str) -> String {
    let mut result = s.to_string();
    // Find all %VAR% patterns and expand them
    while let Some(start) = result.find('%') {
        if let Some(end) = result[start + 1..].find('%') {
            let var_name = &result[start + 1..start + 1 + end];
            if let Ok(value) = std::env::var(var_name) {
                result = format!("{}{}{}", &result[..start], value, &result[start + 2 + end..]);
            } else {
                // Can't expand, skip this one
                break;
            }
        } else {
            break;
        }
    }
    result
}

// ── Installed App Models ────────────────────────────────────────────

/// An installed application from the Windows Uninstall registry.
#[derive(Debug, Clone)]
pub struct InstalledApp {
    pub display_name: String,
    pub publisher: String,
    pub display_version: String,
    pub install_date: String,
    pub estimated_size_kb: u64,
    pub uninstall_string: String,
    pub modify_path: Option<String>,
    pub install_location: String,
}

// ── Process Models ──────────────────────────────────────────────────

/// A running process for the Processes tab.
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub name: String,
    pub exe_path: String,
    pub command_line: String,
    pub memory_bytes: u64,
    pub cpu_usage: f32,
    pub disk_read_bytes: u64,
    pub disk_write_bytes: u64,
    pub start_time: Option<DateTime<Local>>,
    pub product_name: String,
    pub user_name: String,
    pub is_elevated: bool,
}
