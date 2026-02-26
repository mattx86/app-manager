use crate::models::{EnabledStatus, RunState, Source, StartupEntry};
use crate::version_info;
use anyhow::{Context, Result};
use std::collections::HashMap;
use winreg::enums::*;
use winreg::RegKey;

pub fn collect_services() -> Result<Vec<StartupEntry>> {
    // Step 1: Enumerate all WIN32 services via native EnumServicesStatusExW
    let service_infos = enumerate_services_native()?;

    // Step 2: Build process start-time lookup from PIDs
    let process_start_times = build_process_start_times();

    // Step 3: Get config from registry for each service
    let services_key = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("SYSTEM\\CurrentControlSet\\Services")
        .context("Failed to open Services registry key")?;

    let mut entries = Vec::new();
    for info in &service_infos {
        if let Some(entry) = build_entry(&services_key, info, &process_start_times) {
            entries.push(entry);
        }
    }

    // Sort by name
    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok(entries)
}

/// Enumerate all WIN32 services using native EnumServicesStatusExW (no sc.exe spawn).
fn enumerate_services_native() -> Result<Vec<ScServiceInfo>> {
    use windows::Win32::System::LibraryLoader::{LoadLibraryA, GetProcAddress};
    use windows::core::PCSTR;

    let lib = unsafe { LoadLibraryA(PCSTR(b"advapi32.dll\0".as_ptr())) }
        .map_err(|e| anyhow::anyhow!("LoadLibrary advapi32: {}", e))?;

    type OpenSCManagerFn = unsafe extern "system" fn(
        machine: *const u16, database: *const u16, access: u32,
    ) -> isize;
    type EnumServicesFn = unsafe extern "system" fn(
        sc_manager: isize, info_level: u32, service_type: u32, service_state: u32,
        services: *mut u8, buf_size: u32, bytes_needed: *mut u32,
        services_returned: *mut u32, resume_handle: *mut u32, group_name: *const u16,
    ) -> i32;
    type CloseHandleFn = unsafe extern "system" fn(handle: isize) -> i32;

    let open_scm: OpenSCManagerFn = unsafe {
        std::mem::transmute(
            GetProcAddress(lib, PCSTR(b"OpenSCManagerW\0".as_ptr()))
                .ok_or_else(|| anyhow::anyhow!("GetProcAddress OpenSCManagerW failed"))?
        )
    };
    let enum_svc: EnumServicesFn = unsafe {
        std::mem::transmute(
            GetProcAddress(lib, PCSTR(b"EnumServicesStatusExW\0".as_ptr()))
                .ok_or_else(|| anyhow::anyhow!("GetProcAddress EnumServicesStatusExW failed"))?
        )
    };
    let close_svc: CloseHandleFn = unsafe {
        std::mem::transmute(
            GetProcAddress(lib, PCSTR(b"CloseServiceHandle\0".as_ptr()))
                .ok_or_else(|| anyhow::anyhow!("GetProcAddress CloseServiceHandle failed"))?
        )
    };

    const SC_MANAGER_ENUMERATE_SERVICE: u32 = 0x0004;
    const SC_ENUM_PROCESS_INFO: u32 = 0;
    const SERVICE_WIN32: u32 = 0x30;
    const SERVICE_STATE_ALL: u32 = 0x03;
    const SERVICE_RUNNING: u32 = 0x04;

    let sc_handle = unsafe { open_scm(std::ptr::null(), std::ptr::null(), SC_MANAGER_ENUMERATE_SERVICE) };
    if sc_handle == 0 {
        anyhow::bail!("OpenSCManagerW failed");
    }

    // First call to get required buffer size
    let mut bytes_needed: u32 = 0;
    let mut services_returned: u32 = 0;
    let mut resume_handle: u32 = 0;

    unsafe {
        enum_svc(
            sc_handle, SC_ENUM_PROCESS_INFO, SERVICE_WIN32, SERVICE_STATE_ALL,
            std::ptr::null_mut(), 0, &mut bytes_needed,
            &mut services_returned, &mut resume_handle, std::ptr::null(),
        );
    }

    if bytes_needed == 0 {
        unsafe { close_svc(sc_handle); }
        anyhow::bail!("EnumServicesStatusExW: no buffer needed");
    }

    let mut buffer = vec![0u8; bytes_needed as usize];
    resume_handle = 0;

    let ok = unsafe {
        enum_svc(
            sc_handle, SC_ENUM_PROCESS_INFO, SERVICE_WIN32, SERVICE_STATE_ALL,
            buffer.as_mut_ptr(), bytes_needed, &mut bytes_needed,
            &mut services_returned, &mut resume_handle, std::ptr::null(),
        )
    };

    if ok == 0 {
        unsafe { close_svc(sc_handle); }
        anyhow::bail!("EnumServicesStatusExW failed");
    }

    // ENUM_SERVICE_STATUS_PROCESSW layout (x64):
    //   lpServiceName: *const u16  (8 bytes)
    //   lpDisplayName: *const u16  (8 bytes)
    //   SERVICE_STATUS_PROCESS:
    //     dwServiceType: u32, dwCurrentState: u32, dwControlsAccepted: u32,
    //     dwWin32ExitCode: u32, dwServiceSpecificExitCode: u32,
    //     dwCheckPoint: u32, dwWaitHint: u32, dwProcessId: u32, dwServiceFlags: u32
    //   (36 bytes + 4 bytes padding = 40 bytes)
    //   Total: 56 bytes per entry
    #[repr(C)]
    struct ServiceStatusProcess {
        service_type: u32,
        current_state: u32,
        _controls_accepted: u32,
        _win32_exit_code: u32,
        _svc_specific_exit_code: u32,
        _check_point: u32,
        _wait_hint: u32,
        process_id: u32,
        _service_flags: u32,
    }
    #[repr(C)]
    struct EnumEntry {
        service_name: *const u16,
        display_name: *const u16,
        status: ServiceStatusProcess,
    }

    let entry_size = std::mem::size_of::<EnumEntry>();
    let mut services = Vec::with_capacity(services_returned as usize);

    for i in 0..services_returned as usize {
        let entry_ptr = unsafe { buffer.as_ptr().add(i * entry_size) as *const EnumEntry };
        let entry = unsafe { &*entry_ptr };

        let read_wide = |ptr: *const u16| -> String {
            if ptr.is_null() { return String::new(); }
            unsafe {
                let mut len = 0;
                while *ptr.add(len) != 0 { len += 1; }
                String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len))
            }
        };

        services.push(ScServiceInfo {
            service_name: read_wide(entry.service_name),
            display_name: read_wide(entry.display_name),
            is_running: entry.status.current_state == SERVICE_RUNNING,
            pid: entry.status.process_id,
        });
    }

    unsafe { close_svc(sc_handle); }
    Ok(services)
}

/// Build a map of PID -> process start time using sysinfo.
fn build_process_start_times() -> HashMap<u32, chrono::DateTime<chrono::Local>> {
    use sysinfo::{ProcessesToUpdate, System};

    let mut map = HashMap::new();
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    for (pid, process) in sys.processes() {
        let start_secs = process.start_time();
        if start_secs > 0 {
            if let Some(dt) = chrono::DateTime::from_timestamp(start_secs as i64, 0) {
                map.insert(pid.as_u32(), dt.with_timezone(&chrono::Local));
            }
        }
    }

    map
}

struct ScServiceInfo {
    service_name: String,
    display_name: String,
    is_running: bool,
    pid: u32,
}

fn build_entry(
    services_key: &RegKey,
    info: &ScServiceInfo,
    process_start_times: &HashMap<u32, chrono::DateTime<chrono::Local>>,
) -> Option<StartupEntry> {
    let svc_key = services_key.open_subkey(&info.service_name).ok()?;

    let image_path: String = svc_key.get_value("ImagePath").ok()?;
    if image_path.trim().is_empty() {
        return None;
    }

    let start_type: u32 = svc_key.get_value("Start").unwrap_or(3);
    let object_name: String = svc_key.get_value("ObjectName").unwrap_or_default();

    let enabled = match start_type {
        2 => EnabledStatus::Enabled,   // SERVICE_AUTO_START
        3 => EnabledStatus::Manual,    // SERVICE_DEMAND_START
        4 => EnabledStatus::Disabled,  // SERVICE_DISABLED
        _ => EnabledStatus::Unknown,
    };

    let run_state = if info.is_running {
        RunState::Running
    } else {
        RunState::Stopped
    };

    let source = Source::Service {
        service_name: info.service_name.clone(),
        command_line: image_path.clone(),
    };

    let display_name = if info.display_name.is_empty() {
        info.service_name.clone()
    } else {
        info.display_name.clone()
    };

    let mut entry = StartupEntry::new(display_name, image_path.clone(), source);
    entry.enabled = enabled;
    entry.run_state = run_state;
    entry.runs_as = clean_account_name(&object_name);
    entry.product_name = version_info::get_product_name(&image_path).unwrap_or_default();

    // Use process start time from the service's PID
    if info.pid > 0 {
        if let Some(dt) = process_start_times.get(&info.pid) {
            entry.last_ran = Some(*dt);
        }
    }

    Some(entry)
}

fn clean_account_name(name: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        return "SYSTEM".to_string();
    }
    if name.eq_ignore_ascii_case("LocalSystem") {
        return "SYSTEM".to_string();
    }
    // Strip domain/authority prefix (e.g. "NT AUTHORITY\LocalService" -> "LocalService")
    if let Some(pos) = name.rfind('\\') {
        return name[pos + 1..].to_string();
    }
    name.to_string()
}

/// Check if a service entry is a known built-in Windows service based on its binary path.
/// Each service is matched by its specific executable — broad path matching is avoided
/// because malware can place executables in Windows system folders.
pub fn is_microsoft_service(entry: &StartupEntry) -> bool {
    let cmd = match &entry.source {
        Source::Service { command_line, .. } => command_line,
        _ => return false,
    };

    let cmd_lower = cmd.to_lowercase();
    let cmd_trimmed = cmd_lower.trim_start_matches('"');

    // Check environment-variable prefixes (%systemroot%, %windir%)
    if WINDOWS_SERVICE_PREFIXES
        .iter()
        .any(|prefix| cmd_trimmed.starts_with(prefix))
    {
        return true;
    }

    // Also check expanded literal paths (e.g. C:\WINDOWS\system32\svchost.exe)
    if cmd_trimmed.contains("\\windows\\system32\\svchost.exe") {
        return true;
    }

    // System32 executables with Microsoft product name (expanded or env-var paths)
    if (cmd_trimmed.contains("\\windows\\system32\\")
        || cmd_trimmed.contains("%systemroot%\\system32\\"))
        && entry.product_name == "Microsoft\u{00ae} Windows\u{00ae} Operating System"
    {
        return true;
    }

    false
}

/// Specific command-line prefixes for known built-in Windows services.
/// Uses %systemroot% and %windir% forms (both resolve to C:\Windows).
static WINDOWS_SERVICE_PREFIXES: &[&str] = &[
    // svchost-hosted services
    "%systemroot%\\system32\\svchost.exe",
    "%windir%\\system32\\svchost.exe",
    // System32 services (alphabetical)
    "%systemroot%\\system32\\alg.exe",                  // Application Layer Gateway
    "%systemroot%\\system32\\appvclient.exe",            // Microsoft App-V Client (Enterprise/Education)
    "%systemroot%\\system32\\dllhost.exe",               // COM Surrogate / DCOM Server
    "%systemroot%\\system32\\fxssvc.exe",                // Windows Fax Service
    "%systemroot%\\system32\\gameinputsvc.exe",          // GameInput Service
    "%systemroot%\\system32\\inetsrv\\inetinfo.exe",     // IIS Admin Service
    "%systemroot%\\system32\\lsass.exe",                 // Local Security Authority
    "%systemroot%\\system32\\locator.exe",               // RPC Locator
    "%systemroot%\\system32\\midisrv.exe",               // MIDI Service
    "%systemroot%\\system32\\mqsvc.exe",                 // Message Queuing (MSMQ)
    "%systemroot%\\system32\\msdtc.exe",                 // Distributed Transaction Coordinator
    "%systemroot%\\system32\\msiexec.exe",               // Windows Installer
    "%systemroot%\\system32\\openssh\\ssh-agent.exe",    // OpenSSH Authentication Agent
    "%systemroot%\\system32\\perceptionsimulation\\perceptionsimulationservice.exe", // Mixed Reality Simulation
    "%systemroot%\\system32\\perfhost.exe",              // Performance Counter DLL Host (64-bit)
    "%systemroot%\\system32\\refsdedupsvc.exe",          // ReFS Data Deduplication
    "%systemroot%\\system32\\searchindexer.exe",         // Windows Search Indexer
    "%systemroot%\\system32\\securityhealthservice.exe", // Windows Security Health
    "%systemroot%\\system32\\sensordataservice.exe",     // Sensor Data Service
    "%systemroot%\\system32\\sgrmbroker.exe",            // System Guard Runtime Monitor Broker
    "%systemroot%\\system32\\snmp.exe",                  // SNMP Service
    "%systemroot%\\system32\\snmptrap.exe",              // SNMP Trap Service
    "%systemroot%\\system32\\spectrum.exe",              // Windows Perception Service
    "%systemroot%\\system32\\spoolsv.exe",               // Print Spooler
    "%systemroot%\\system32\\sppsvc.exe",                // Software Protection Platform
    "%systemroot%\\system32\\tcpsvcs.exe",               // Simple TCP/IP Services
    "%systemroot%\\system32\\tieringengineservice.exe",  // Storage Tiers Management
    "%systemroot%\\system32\\ui0detect.exe",             // Interactive Services Detection (Win10)
    "%systemroot%\\system32\\vds.exe",                   // Virtual Disk Service
    "%systemroot%\\system32\\vssvc.exe",                 // Volume Shadow Copy
    "%systemroot%\\system32\\wbem\\wmiapsrv.exe",       // WMI Performance Adapter
    "%systemroot%\\system32\\wbengine.exe",              // Block Level Backup Engine
    "%systemroot%\\system32\\wmcompute.exe",             // Hyper-V Host Compute
    "%systemroot%\\system32\\wssvc.exe",                 // Windows Store Service
    // SysWow64
    "%systemroot%\\syswow64\\perfhost.exe",              // Performance Counter DLL Host (32-bit)
    // Servicing
    "%systemroot%\\servicing\\trustedinstaller.exe",     // Windows Modules Installer
    // .NET Framework
    "%systemroot%\\microsoft.net\\framework64\\v3.0\\wpf\\presentationfontcache.exe", // WPF Font Cache
    "%systemroot%\\microsoft.net\\framework64\\v4.0.30319\\smsvchost.exe", // .NET TCP Port Sharing
    // Windows Media Player
    "%programfiles%\\windows media player\\wmpnetwk.exe", // Media Player Network Sharing
    // Windows Defender
    "%programdata%\\microsoft\\windows defender\\",      // Defender Antivirus (MsMpEng, NisSrv)
    "c:\\programdata\\microsoft\\windows defender\\",    // Defender (expanded path form)
];

/// Fetch a service's description from the registry.
pub fn get_service_description(service_name: &str) -> String {
    let services_key = match RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("SYSTEM\\CurrentControlSet\\Services")
    {
        Ok(k) => k,
        Err(_) => return String::new(),
    };
    let svc_key = match services_key.open_subkey(service_name) {
        Ok(k) => k,
        Err(_) => return String::new(),
    };
    svc_key.get_value("Description").unwrap_or_default()
}
