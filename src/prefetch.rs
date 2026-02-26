use chrono::{DateTime, Local};
use std::collections::HashMap;
use std::path::Path;

const PREFETCH_DIR: &str = r"C:\Windows\Prefetch";

pub struct PrefetchCache {
    last_ran: HashMap<String, DateTime<Local>>,
    pub accessible: bool,
}

impl PrefetchCache {
    pub fn new() -> Self {
        let mut last_ran = HashMap::new();
        let prefetch_path = Path::new(PREFETCH_DIR);

        let accessible = match std::fs::read_dir(prefetch_path) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("");
                    if !ext.eq_ignore_ascii_case("pf") {
                        continue;
                    }

                    if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                        if let Some(exe_name) = parse_prefetch_filename(filename) {
                            if let Ok(metadata) = entry.metadata() {
                                if let Ok(modified) = metadata.modified() {
                                    let dt: DateTime<Local> = modified.into();
                                    last_ran
                                        .entry(exe_name)
                                        .and_modify(|existing: &mut DateTime<Local>| {
                                            if dt > *existing {
                                                *existing = dt;
                                            }
                                        })
                                        .or_insert(dt);
                                }
                            }
                        }
                    }
                }
                true
            }
            Err(_) => false,
        };

        Self { last_ran, accessible }
    }

    pub fn last_ran(&self, exe_name: &str) -> Option<DateTime<Local>> {
        self.last_ran.get(&exe_name.to_uppercase()).copied()
    }
}

/// Extract exe name from prefetch filename: "CHROME.EXE-AB12CD34.pf" -> "CHROME.EXE"
fn parse_prefetch_filename(filename: &str) -> Option<String> {
    let without_ext = filename.strip_suffix(".pf").or(filename.strip_suffix(".PF"))?;
    let dash_pos = without_ext.rfind('-')?;
    let exe_name = &without_ext[..dash_pos];
    Some(exe_name.to_uppercase())
}
