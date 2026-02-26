use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};

/// Extract the "Product Name" from a PE file's version resource.
/// Returns `None` if the file has no version info or the field is missing.
pub fn get_product_name(exe_path: &str) -> Option<String> {
    if exe_path.is_empty() {
        return None;
    }

    // Expand environment variables like %SystemRoot%
    let expanded = expand_env_vars(exe_path);

    // Strip quotes if present
    let clean = expanded.trim().trim_matches('"');

    // If the path contains arguments, extract just the executable path
    let path = extract_path(clean);

    let wide_path: Vec<u16> = OsStr::new(&path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        // Get required buffer size
        let mut handle: u32 = 0;
        let size = GetFileVersionInfoSizeW(PCWSTR(wide_path.as_ptr()), Some(&mut handle));
        if size == 0 {
            return None;
        }

        // Allocate and fill buffer
        let mut buffer = vec![0u8; size as usize];
        GetFileVersionInfoW(
            PCWSTR(wide_path.as_ptr()),
            Some(handle),
            size,
            buffer.as_mut_ptr() as *mut _,
        )
        .ok()?;

        // Query translation table to get language and codepage
        let translation_query: Vec<u16> = OsStr::new("\\VarFileInfo\\Translation")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut trans_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut trans_len: u32 = 0;

        let ok = VerQueryValueW(
            buffer.as_ptr() as *const _,
            PCWSTR(translation_query.as_ptr()),
            &mut trans_ptr,
            &mut trans_len,
        );

        if !ok.as_bool() || trans_ptr.is_null() || trans_len < 4 {
            // No translation table — try the common US English / Unicode codepage
            return query_product_name(&buffer, 0x0409, 0x04B0)
                .or_else(|| query_product_name(&buffer, 0x0409, 0x04E4))
                .or_else(|| query_product_name(&buffer, 0x0000, 0x04B0));
        }

        // Read the first translation entry (language, codepage)
        let lang = *(trans_ptr as *const u16);
        let codepage = *((trans_ptr as *const u16).add(1));

        query_product_name(&buffer, lang, codepage)
    }
}

unsafe fn query_product_name(buffer: &[u8], lang: u16, codepage: u16) -> Option<String> {
    let query = format!(
        "\\StringFileInfo\\{:04x}{:04x}\\ProductName",
        lang, codepage
    );
    let wide_query: Vec<u16> = OsStr::new(&query)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mut value_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
    let mut value_len: u32 = 0;

    let ok = VerQueryValueW(
        buffer.as_ptr() as *const _,
        PCWSTR(wide_query.as_ptr()),
        &mut value_ptr,
        &mut value_len,
    );

    if !ok.as_bool() || value_ptr.is_null() || value_len == 0 {
        return None;
    }

    // value_len includes the null terminator; the data is a wide string
    let slice = std::slice::from_raw_parts(value_ptr as *const u16, value_len as usize);
    // Trim trailing nulls
    let trimmed = match slice.iter().position(|&c| c == 0) {
        Some(pos) => &slice[..pos],
        None => slice,
    };
    let s = String::from_utf16_lossy(trimmed).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

/// Extract the executable path portion from a command string.
/// Handles quoted paths and paths with arguments.
fn extract_path(s: &str) -> String {
    let s = s.trim();
    if s.is_empty() {
        return String::new();
    }

    // If it starts with a quote, take up to the closing quote
    if let Some(stripped) = s.strip_prefix('"') {
        if let Some(end) = stripped.find('"') {
            return stripped[..end].to_string();
        }
        return stripped.to_string();
    }

    // If the path has a known executable extension, find where it ends
    let lower = s.to_lowercase();
    for ext in &[".exe", ".dll", ".sys", ".ocx"] {
        if let Some(pos) = lower.find(ext) {
            return s[..pos + ext.len()].to_string();
        }
    }

    // Fall back to first whitespace-delimited token
    s.split_whitespace()
        .next()
        .unwrap_or(s)
        .to_string()
}

fn expand_env_vars(s: &str) -> String {
    let mut result = s.to_string();
    while let Some(start) = result.find('%') {
        if let Some(end) = result[start + 1..].find('%') {
            let var_name = &result[start + 1..start + 1 + end];
            if let Ok(value) = std::env::var(var_name) {
                result = format!("{}{}{}", &result[..start], value, &result[start + 2 + end..]);
            } else {
                break;
            }
        } else {
            break;
        }
    }
    result
}
