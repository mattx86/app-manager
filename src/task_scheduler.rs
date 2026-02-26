use crate::models::{EnabledStatus, RunState, Source, StartupEntry};
use anyhow::{Context, Result};
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, NaiveTime};
use windows::core::{Interface, BSTR};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::TaskScheduler::*;
use windows::Win32::System::Variant::VARIANT;

pub fn collect_task_scheduler_entries() -> Result<Vec<StartupEntry>> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }

    let result = unsafe { collect_inner() };

    unsafe {
        CoUninitialize();
    }

    result
}

unsafe fn collect_inner() -> Result<Vec<StartupEntry>> {
    let service: ITaskService =
        CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER)
            .context("Failed to create ITaskService")?;

    service
        .Connect(
            &VARIANT::default(),
            &VARIANT::default(),
            &VARIANT::default(),
            &VARIANT::default(),
        )
        .context("Failed to connect to Task Scheduler")?;

    let root_folder = service
        .GetFolder(&BSTR::from("\\"))
        .context("Failed to get root folder")?;

    let mut entries = Vec::new();
    enumerate_folder(&root_folder, &mut entries);
    Ok(entries)
}

unsafe fn enumerate_folder(folder: &ITaskFolder, entries: &mut Vec<StartupEntry>) {
    // Process tasks in this folder
    if let Ok(tasks) = folder.GetTasks(0) {
        if let Ok(count) = tasks.Count() {
            for i in 1..=count {
                let index = VARIANT::from(i);
                if let Ok(task) = tasks.get_Item(&index) {
                    if let Some(entry) = process_task(&task) {
                        entries.push(entry);
                    }
                }
            }
        }
    }

    // Recurse into subfolders
    if let Ok(folders) = folder.GetFolders(0) {
        if let Ok(count) = folders.Count() {
            for i in 1..=count {
                let index = VARIANT::from(i);
                if let Ok(subfolder) = folders.get_Item(&index) {
                    enumerate_folder(&subfolder, entries);
                }
            }
        }
    }
}

unsafe fn process_task(task: &IRegisteredTask) -> Option<StartupEntry> {
    let definition = task.Definition().ok()?;

    // Check if this task has a logon trigger
    let triggers = definition.Triggers().ok()?;
    let mut has_logon_trigger = false;
    let mut trigger_count = 0i32;
    triggers.Count(&mut trigger_count).ok()?;
    for i in 1..=trigger_count {
        if let Ok(trigger) = triggers.get_Item(i) {
            let mut trigger_type = TASK_TRIGGER_EVENT;
            if trigger.Type(&mut trigger_type).is_ok() && trigger_type == TASK_TRIGGER_LOGON {
                has_logon_trigger = true;
                break;
            }
        }
    }

    if !has_logon_trigger {
        return None;
    }

    // Filter out service tasks
    if is_service_task(&definition) {
        return None;
    }

    let name = task.Name().ok()?.to_string();
    let task_path = task.Path().ok()?.to_string();

    // Get the command from actions
    let command = get_task_command(&definition).unwrap_or_default();
    if command.is_empty() {
        return None;
    }

    // Get enabled status
    let enabled = match task.Enabled() {
        Ok(e) => {
            if e.as_bool() {
                EnabledStatus::Enabled
            } else {
                EnabledStatus::Disabled
            }
        }
        Err(_) => EnabledStatus::Unknown,
    };

    // Get last run time (OLE Automation date as f64)
    let last_ran = task
        .LastRunTime()
        .ok()
        .and_then(ole_date_to_datetime);

    let source = Source::TaskScheduler {
        task_path: task_path.clone(),
    };

    // Get the user account this task runs as
    let runs_as = get_task_user(&definition);

    let mut entry = StartupEntry::new(name, command, source);
    entry.enabled = enabled;
    entry.last_ran = last_ran;
    entry.run_state = RunState::Stopped;
    entry.runs_as = runs_as;

    Some(entry)
}

unsafe fn get_task_user(definition: &ITaskDefinition) -> String {
    if let Ok(principal) = definition.Principal() {
        let mut user_id = BSTR::default();
        if principal.UserId(&mut user_id).is_ok() && !user_id.is_empty() {
            let user = user_id.to_string();
            // Strip domain prefix (e.g., "DOMAIN\user" -> "user")
            if let Some(pos) = user.rfind('\\') {
                return user[pos + 1..].to_string();
            }
            return user;
        }
    }
    String::new()
}

unsafe fn is_service_task(definition: &ITaskDefinition) -> bool {
    if let Ok(principal) = definition.Principal() {
        let mut logon_type = TASK_LOGON_NONE;
        if principal.LogonType(&mut logon_type).is_ok()
            && (logon_type == TASK_LOGON_SERVICE_ACCOUNT || logon_type == TASK_LOGON_S4U)
        {
            return true;
        }
    }

    // Check if the action is svchost.exe
    if let Some(cmd) = get_task_command(definition) {
        let cmd_lower = cmd.to_lowercase();
        if cmd_lower.contains("svchost.exe") {
            return true;
        }
    }

    false
}

unsafe fn get_task_command(definition: &ITaskDefinition) -> Option<String> {
    let actions = definition.Actions().ok()?;
    let mut count = 0i32;
    actions.Count(&mut count).ok()?;

    for i in 1..=count {
        if let Ok(action) = actions.get_Item(i) {
            let mut action_type = TASK_ACTION_EXEC;
            if action.Type(&mut action_type).is_ok() && action_type == TASK_ACTION_EXEC {
                if let Ok(exec_action) = action.cast::<IExecAction>() {
                    let mut path = BSTR::default();
                    if exec_action.Path(&mut path).is_ok() {
                        let mut args = BSTR::default();
                        let _ = exec_action.Arguments(&mut args);
                        let cmd = if args.is_empty() {
                            path.to_string()
                        } else {
                            format!("{} {}", path, args)
                        };
                        return Some(cmd);
                    }
                }
            }
        }
    }

    None
}

/// Convert an OLE Automation date (f64) to DateTime<Local>.
fn ole_date_to_datetime(ole_date: f64) -> Option<DateTime<Local>> {
    // OLE date 0.0 = never ran; dates before 2000 are bogus "never ran" sentinel values
    if ole_date < 36526.0 {
        // 36526.0 is approx 2000-01-01 in OLE date
        return None;
    }

    let epoch = NaiveDate::from_ymd_opt(1899, 12, 30)?;
    let days = ole_date.floor() as i64;
    let day_fraction = ole_date.fract();

    let date = epoch.checked_add_signed(chrono::Duration::days(days))?;
    let total_secs = (day_fraction * 86400.0) as u32;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    let time = NaiveTime::from_hms_opt(hours, minutes, seconds)?;
    let naive = NaiveDateTime::new(date, time);

    naive.and_local_timezone(Local).single()
}
