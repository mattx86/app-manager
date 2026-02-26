use crate::models::*;
use crate::prefetch;
use crate::process;
use crate::registry;
use crate::startup_folders;
use crate::status;
use crate::task_scheduler;
use crate::version_info;
use std::collections::HashSet;

const NONADMIN_PATHS_FILE: &str = "app-manager-nonadmin.txt";

pub struct CollectionResult {
    pub entries: Vec<StartupEntry>,
    pub is_admin: bool,
}

/// Save the task paths visible to the current (non-admin) user.
/// Called just before "Restart as Admin" so admin mode can compare.
pub fn save_nonadmin_task_paths(entries: &[StartupEntry]) {
    let temp_path = std::env::temp_dir().join(NONADMIN_PATHS_FILE);
    let paths: Vec<&str> = entries
        .iter()
        .filter_map(|e| match &e.source {
            Source::TaskScheduler { task_path } => Some(task_path.as_str()),
            _ => None,
        })
        .collect();
    let _ = std::fs::write(&temp_path, paths.join("\n"));
}

/// Load the saved non-admin task paths (if any) and delete the file.
fn load_nonadmin_task_paths() -> Option<HashSet<String>> {
    let temp_path = std::env::temp_dir().join(NONADMIN_PATHS_FILE);
    let content = std::fs::read_to_string(&temp_path).ok()?;
    let _ = std::fs::remove_file(&temp_path);
    Some(content.lines().map(|s| s.to_string()).collect())
}

pub fn collect_all_entries() -> CollectionResult {
    // Phase 1: Collect raw entries from all sources
    let mut entries: Vec<StartupEntry> = Vec::new();

    entries.extend(registry::collect_registry_entries());
    entries.extend(startup_folders::collect_startup_folder_entries());

    match task_scheduler::collect_task_scheduler_entries() {
        Ok(tasks) => entries.extend(tasks),
        Err(_) => {}
    }

    // Phase 2: Build enrichment caches
    let approvals = status::load_all_approvals();
    let process_snapshot = process::ProcessSnapshot::new();
    let prefetch_cache = prefetch::PrefetchCache::new();

    let is_admin = prefetch_cache.accessible;

    // Get current username for entries that run as the logged-in user
    let current_user = std::env::var("USERNAME").unwrap_or_default();

    // Phase 3: Enrich each entry
    for entry in &mut entries {
        // Set runs_as for non-task-scheduler entries (they run as current user)
        if !matches!(entry.source, Source::TaskScheduler { .. }) {
            entry.runs_as = current_user.clone();
        } else if entry.runs_as.is_empty() {
            entry.runs_as = current_user.clone();
        }
        // Enabled/disabled from StartupApproved (skip Task Scheduler, already set)
        if !matches!(entry.source, Source::TaskScheduler { .. }) {
            let (enabled, disabled_ts) =
                status::get_approval_status(&entry.name, &entry.source, &approvals);
            entry.enabled = enabled;

            // Use disabled timestamp as last_ran fallback if no better source
            if entry.last_ran.is_none() {
                entry.last_ran = disabled_ts;
            }
        }

        // Product name from PE version info
        entry.product_name = version_info::get_product_name(&entry.command).unwrap_or_default();

        // Running/stopped
        if let Some(exe) = entry.exe_name() {
            if process_snapshot.is_running(&exe) {
                entry.run_state = RunState::Running;

                // Use process start time as last_ran (most accurate when running)
                if let Some(start) = process_snapshot.start_time(&exe) {
                    entry.last_ran = Some(start);
                }
            } else {
                entry.run_state = RunState::Stopped;

                // Try prefetch for last_ran if we don't already have a time
                if entry.last_ran.is_none() {
                    let upper_exe = exe.to_uppercase();
                    entry.last_ran = prefetch_cache.last_ran(&upper_exe);
                }
            }
        }
    }

    // Determine admin-only entries by comparing with saved non-admin list.
    // Only Task Scheduler entries can differ between admin and non-admin modes.
    if is_admin {
        if let Some(nonadmin_paths) = load_nonadmin_task_paths() {
            // We have comparison data: mark entries NOT in the non-admin list
            for entry in &mut entries {
                if let Source::TaskScheduler { ref task_path } = entry.source {
                    entry.requires_admin = !nonadmin_paths.contains(task_path);
                } else {
                    entry.requires_admin = false;
                }
            }
        } else {
            // No comparison data (launched directly as admin), don't mark anything
            for entry in &mut entries {
                entry.requires_admin = false;
            }
        }
    } else {
        // Not running as admin: if we can see it, it's not admin-only
        for entry in &mut entries {
            entry.requires_admin = false;
        }
    }

    // Phase 4: Sort by source type, then by name
    entries.sort_by(|a, b| {
        a.source
            .sort_key()
            .cmp(&b.source.sort_key())
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    CollectionResult { entries, is_admin }
}
