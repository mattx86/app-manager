use crate::models::ProcessInfo;
use crate::version_info;
use std::collections::{HashMap, HashSet};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{
    GetTokenInformation, LookupAccountSidW, TokenElevation, TokenUser, SID_NAME_USE,
    TOKEN_ELEVATION, TOKEN_QUERY, TOKEN_USER,
};
use windows::Win32::System::Threading::{OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION};

/// Collect all running processes.
/// Performs a double-refresh with a short delay to get accurate CPU usage values.
pub fn collect_processes() -> Vec<ProcessInfo> {
    let mut sys = System::new();

    // Request command line info alongside the defaults
    let refresh_kind = ProcessRefreshKind::everything()
        .with_cmd(UpdateKind::OnlyIfNotSet);

    // First refresh: establishes baseline for CPU measurement
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, refresh_kind);

    // Short delay so the second refresh can compute a meaningful CPU delta
    std::thread::sleep(std::time::Duration::from_millis(200));

    // Second refresh: CPU usage is now computed from the delta
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, refresh_kind);

    let mut processes: Vec<ProcessInfo> = sys
        .processes()
        .iter()
        .map(|(pid, process)| {
            let start_time = {
                let secs = process.start_time();
                if secs > 0 {
                    chrono::DateTime::from_timestamp(secs as i64, 0)
                        .map(|dt| dt.with_timezone(&chrono::Local))
                } else {
                    None
                }
            };
            let exe_path = process
                .exe()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            let command_line = {
                let args = process.cmd();
                if args.is_empty() {
                    String::new()
                } else {
                    args.iter()
                        .map(|a| a.to_string_lossy().to_string())
                        .collect::<Vec<_>>()
                        .join(" ")
                }
            };
            let product_name = version_info::get_product_name(&exe_path).unwrap_or_default();
            let disk = process.disk_usage();
            let pid_u32 = pid.as_u32();
            let (user_name, is_elevated) = get_process_user_and_elevation(pid_u32);
            ProcessInfo {
                pid: pid_u32,
                parent_pid: process.parent().map(|p| p.as_u32()),
                name: process.name().to_string_lossy().to_string(),
                exe_path,
                command_line,
                memory_bytes: process.memory(),
                cpu_usage: process.cpu_usage(),
                disk_read_bytes: disk.total_read_bytes,
                disk_write_bytes: disk.total_written_bytes,
                start_time,
                product_name,
                user_name,
                is_elevated,
            }
        })
        .collect();

    processes.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.pid.cmp(&b.pid))
    });

    processes
}

/// Get the user name and elevation status for a process by PID.
/// Returns (user_name, is_elevated). On failure, returns empty string / false.
fn get_process_user_and_elevation(pid: u32) -> (String, bool) {
    if pid <= 4 {
        // System/Idle — can't open tokens
        return (if pid == 0 { "SYSTEM".to_string() } else { "SYSTEM".to_string() }, false);
    }

    let proc_handle = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
        Ok(h) => h,
        Err(_) => return (String::new(), false),
    };

    let mut token_handle = HANDLE::default();
    let tok_ok = unsafe { OpenProcessToken(proc_handle, TOKEN_QUERY, &mut token_handle) };
    let _ = unsafe { CloseHandle(proc_handle) };
    if tok_ok.is_err() {
        return (String::new(), false);
    }

    // Get user name via TokenUser + LookupAccountSidW
    let user_name = get_token_user_name(token_handle);

    // Get elevation status via TokenElevation
    let is_elevated = get_token_elevation(token_handle);

    let _ = unsafe { CloseHandle(token_handle) };

    (user_name, is_elevated)
}

fn get_token_user_name(token: HANDLE) -> String {
    let mut buf = vec![0u8; 256];
    let mut needed: u32 = 0;
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            Some(buf.as_mut_ptr() as *mut std::ffi::c_void),
            buf.len() as u32,
            &mut needed,
        )
    };
    if ok.is_err() {
        return String::new();
    }

    let token_user = unsafe { &*(buf.as_ptr() as *const TOKEN_USER) };
    let sid = token_user.User.Sid;

    let mut name_buf = vec![0u16; 256];
    let mut domain_buf = vec![0u16; 256];
    let mut name_len = name_buf.len() as u32;
    let mut domain_len = domain_buf.len() as u32;
    let mut sid_type = SID_NAME_USE::default();

    let lookup_ok = unsafe {
        LookupAccountSidW(
            None,
            sid,
            Some(windows::core::PWSTR(name_buf.as_mut_ptr())),
            &mut name_len,
            Some(windows::core::PWSTR(domain_buf.as_mut_ptr())),
            &mut domain_len,
            &mut sid_type,
        )
    };
    if lookup_ok.is_err() {
        return String::new();
    }

    let domain = String::from_utf16_lossy(&domain_buf[..domain_len as usize]);
    let name = String::from_utf16_lossy(&name_buf[..name_len as usize]);

    if domain.is_empty() {
        name
    } else {
        format!("{}\\{}", domain, name)
    }
}

fn get_token_elevation(token: HANDLE) -> bool {
    let mut elevation = TOKEN_ELEVATION::default();
    let mut needed: u32 = 0;
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut TOKEN_ELEVATION as *mut std::ffi::c_void),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut needed,
        )
    };
    if ok.is_err() {
        return false;
    }
    elevation.TokenIsElevated != 0
}

/// Return the set of PIDs that are parents of at least one other process.
/// Used to auto-expand the tree on load.
pub fn parent_pids(processes: &[ProcessInfo]) -> HashSet<u32> {
    let pid_set: HashSet<u32> = processes.iter().map(|p| p.pid).collect();
    let mut parents = HashSet::new();
    for proc in processes {
        if let Some(ppid) = proc.parent_pid {
            if pid_set.contains(&ppid) && ppid != proc.pid {
                parents.insert(ppid);
            }
        }
    }
    parents
}

/// A flattened tree row: depth level + reference to the process.
pub struct TreeRow<'a> {
    pub depth: usize,
    pub process: &'a ProcessInfo,
    pub has_children: bool,
    pub is_expanded: bool,
    /// Whether this node is the last sibling at its depth level.
    pub is_last_sibling: bool,
    /// For each ancestor depth 0..depth, true means a vertical connector line
    /// should be drawn (the ancestor at that depth has more siblings below).
    pub connector_lines: Vec<bool>,
}

/// Build a flattened visible tree from the process list.
///
/// - `expanded_pids`: PIDs whose children are visible.
/// - `hide_windows`: if true, skip known Windows processes (and their subtrees
///   unless they have non-Windows descendants).
pub fn build_visible_tree<'a>(
    processes: &'a [ProcessInfo],
    expanded_pids: &HashSet<u32>,
    hide_windows: bool,
) -> Vec<TreeRow<'a>> {
    let pid_set: HashSet<u32> = processes.iter().map(|p| p.pid).collect();
    let proc_map: HashMap<u32, &ProcessInfo> = processes.iter().map(|p| (p.pid, p)).collect();

    // Build children map
    let mut children_map: HashMap<u32, Vec<u32>> = HashMap::new();
    for proc in processes {
        if let Some(ppid) = proc.parent_pid {
            if pid_set.contains(&ppid) && ppid != proc.pid {
                children_map.entry(ppid).or_default().push(proc.pid);
            }
        }
    }

    // Sort children by name then PID for stable display
    for kids in children_map.values_mut() {
        kids.sort_by(|a, b| {
            let a_name = proc_map.get(a).map(|p| p.name.to_lowercase()).unwrap_or_default();
            let b_name = proc_map.get(b).map(|p| p.name.to_lowercase()).unwrap_or_default();
            a_name.cmp(&b_name).then(a.cmp(b))
        });
    }

    // If hiding Windows processes, precompute which PIDs have non-Windows descendants
    let non_windows_pids: HashSet<u32> = if hide_windows {
        let mut visible = HashSet::new();
        for proc in processes {
            if !is_windows_process(proc) {
                // Mark this process and all ancestors as visible
                visible.insert(proc.pid);
                let mut current = proc.parent_pid;
                while let Some(ppid) = current {
                    if !visible.insert(ppid) {
                        break; // Already marked, ancestors are too
                    }
                    current = proc_map.get(&ppid).and_then(|p| p.parent_pid);
                }
            }
        }
        visible
    } else {
        HashSet::new()
    };

    // Find root processes (parent not in our process list)
    let mut roots: Vec<u32> = processes
        .iter()
        .filter(|p| {
            match p.parent_pid {
                None => true,
                Some(ppid) => !pid_set.contains(&ppid) || ppid == p.pid,
            }
        })
        .map(|p| p.pid)
        .collect();

    roots.sort_by(|a, b| {
        let a_name = proc_map.get(a).map(|p| p.name.to_lowercase()).unwrap_or_default();
        let b_name = proc_map.get(b).map(|p| p.name.to_lowercase()).unwrap_or_default();
        a_name.cmp(&b_name).then(a.cmp(b))
    });

    // DFS traversal — track connector line state for tree drawing.
    // Stack items: (pid, depth, is_last_sibling)
    let root_count = roots.len();
    let mut result = Vec::new();
    let mut stack: Vec<(u32, usize, bool)> = roots
        .iter()
        .enumerate()
        .rev()
        .map(|(i, &pid)| (pid, 0usize, i == root_count - 1))
        .collect();

    // is_last_at[d] tracks whether the most recently processed node at depth d
    // was the last sibling. A vertical line at column c exists if the node at
    // depth c+1 is NOT the last sibling (i.e., more siblings at c+1 to come).
    let mut is_last_at: Vec<bool> = Vec::new();

    while let Some((pid, depth, is_last)) = stack.pop() {
        let proc = match proc_map.get(&pid) {
            Some(p) => p,
            None => continue,
        };

        // Filter: skip Windows processes (and their subtree) unless they have
        // non-Windows descendants
        if hide_windows && !non_windows_pids.contains(&pid) {
            continue;
        }

        let kids = children_map.get(&pid);
        let has_children = kids.map_or(false, |k| {
            if hide_windows {
                k.iter().any(|child_pid| non_windows_pids.contains(child_pid))
            } else {
                !k.is_empty()
            }
        });
        let is_expanded = expanded_pids.contains(&pid);

        // Record this node's last-sibling status
        while is_last_at.len() <= depth {
            is_last_at.push(true);
        }
        is_last_at[depth] = is_last;

        // Build connector_lines for ancestor columns 0..depth-1.
        // connector_lines[c] = true means draw a vertical line at column c,
        // which happens when the node at depth c+1 is NOT the last sibling.
        let connector_lines: Vec<bool> = (0..depth.saturating_sub(1))
            .map(|c| !is_last_at[c + 1])
            .collect();

        result.push(TreeRow {
            depth,
            process: proc,
            has_children,
            is_expanded,
            is_last_sibling: is_last,
            connector_lines,
        });

        // Push children in reverse order (so first child is popped first)
        if is_expanded && has_children {
            if let Some(kids) = kids {
                let visible_kids: Vec<u32> = kids
                    .iter()
                    .filter(|&&child_pid| !hide_windows || non_windows_pids.contains(&child_pid))
                    .copied()
                    .collect();
                let kid_count = visible_kids.len();
                for (i, child_pid) in visible_kids.into_iter().enumerate().rev() {
                    stack.push((child_pid, depth + 1, i == kid_count - 1));
                }
            }
        }
    }

    result
}

/// Check if a process is a known built-in Windows process.
pub fn is_windows_process(proc: &ProcessInfo) -> bool {
    let name_lower = proc.name.to_lowercase();
    WINDOWS_PROCESS_NAMES
        .iter()
        .any(|&known| name_lower == known)
}

/// Known Windows system process names (lowercase).
static WINDOWS_PROCESS_NAMES: &[&str] = &[
    // Core kernel/session
    "system",
    "secure system",
    "registry",
    "smss.exe",
    "csrss.exe",
    "wininit.exe",
    "winlogon.exe",
    "services.exe",
    "lsass.exe",
    "lsaiso.exe",
    "svchost.exe",
    // Desktop/shell
    "dwm.exe",
    "sihost.exe",
    "taskhostw.exe",
    "ctfmon.exe",
    "fontdrvhost.exe",
    "dllhost.exe",
    "conhost.exe",
    // UWP / modern shell
    "runtimebroker.exe",
    "searchhost.exe",
    "startmenuexperiencehost.exe",
    "shellexperiencehost.exe",
    "textinputhost.exe",
    "widgetservice.exe",
    "widgets.exe",
    "phoneexperiencehost.exe",
    "lockapp.exe",
    "gameinputsvc.exe",
    // Windows Defender / Security
    "msmpeng.exe",
    "nissrv.exe",
    "securityhealthservice.exe",
    "securityhealthsystray.exe",
    "sgrmbroker.exe",
    // Networking / services
    "spoolsv.exe",
    "dashost.exe",
    "wmiprvse.exe",
    "searchindexer.exe",
    "searchprotocolhost.exe",
    "searchfilterhost.exe",
    "audiodg.exe",
    "wuauclt.exe",
    "trustedinstaller.exe",
    "wudfhost.exe",
    "comppkgsrv.exe",
    // Memory / idle
    "memory compression",
    "system idle process",
    "idle",
    // Other common Windows processes
    "msiexec.exe",
    "smartscreen.exe",
    "applicationframehost.exe",
    "systemsettings.exe",
    "useroobebroker.exe",
    "backgroundtaskhost.exe",
    "lsm.exe",
    "wlanext.exe",
    "unsecapp.exe",
    "taskmgr.exe",
    "mpcmdrun.exe",
    "werfault.exe",
    "backgroundtransferhost.exe",
    "settingsynchost.exe",
    "systemsettingsbroker.exe",
    "usocoreworker.exe",
    "musnotification.exe",
    "musnotifyicon.exe",
];
