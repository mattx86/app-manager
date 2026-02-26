use chrono::{DateTime, Local};
use std::collections::{HashMap, HashSet};
use sysinfo::System;

pub struct ProcessSnapshot {
    running_exe_names: HashSet<String>,
    start_times: HashMap<String, DateTime<Local>>,
}

impl ProcessSnapshot {
    pub fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

        let mut running_exe_names = HashSet::new();
        let mut start_times: HashMap<String, DateTime<Local>> = HashMap::new();

        for process in sys.processes().values() {
            let name = process.name().to_string_lossy().to_lowercase();
            running_exe_names.insert(name.clone());

            let start_secs = process.start_time();
            if start_secs > 0 {
                if let Some(dt) = chrono::DateTime::from_timestamp(start_secs as i64, 0) {
                    let local_dt = dt.with_timezone(&Local);
                    // Keep the earliest start time for each exe name
                    start_times
                        .entry(name)
                        .and_modify(|existing| {
                            if local_dt < *existing {
                                *existing = local_dt;
                            }
                        })
                        .or_insert(local_dt);
                }
            }
        }

        Self {
            running_exe_names,
            start_times,
        }
    }

    pub fn is_running(&self, exe_name: &str) -> bool {
        self.running_exe_names.contains(&exe_name.to_lowercase())
    }

    pub fn start_time(&self, exe_name: &str) -> Option<DateTime<Local>> {
        self.start_times.get(&exe_name.to_lowercase()).copied()
    }
}
