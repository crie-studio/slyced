//! Small Windows helpers that must work even when the UI cannot start.

/// Native message box (no dependency on the UI or on the GPU).
pub fn message_box(title: &str, text: &str, error: bool) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND};
        let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        let (t, m) = (wide(title), wide(text));
        let icon = if error { MB_ICONERROR } else { MB_ICONINFORMATION };
        unsafe {
            MessageBoxW(std::ptr::null_mut(), m.as_ptr(), t.as_ptr(), MB_OK | icon | MB_SETFOREGROUND);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = error;
        eprintln!("{title}: {text}");
    }
}

/// e.g. "Windows 11 Pro 24H2 (build 26200) · x64".
pub fn os_description() -> String {
    let arch = if cfg!(target_arch = "aarch64") {
        "ARM64"
    } else if cfg!(target_arch = "x86") {
        "x86 (32 bits)"
    } else {
        "x64"
    };
    #[cfg(windows)]
    {
        let product = reg_string("ProductName").unwrap_or_else(|| "Windows".into());
        let build: u32 = reg_string("CurrentBuildNumber").and_then(|b| b.parse().ok()).unwrap_or(0);
        // Windows 11 still reports "Windows 10" in ProductName.
        let product = if build >= 22000 { product.replace("Windows 10", "Windows 11") } else { product };
        let display = reg_string("DisplayVersion").or_else(|| reg_string("ReleaseId")).unwrap_or_default();
        format!("{product} {display} (build {build}) · executável {arch}")
    }
    #[cfg(not(windows))]
    {
        format!("{} · {arch}", std::env::consts::OS)
    }
}

#[cfg(windows)]
fn reg_string(name: &str) -> Option<String> {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let key = wide(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
    let value = wide(name);
    let mut buf = vec![0u16; 256];
    let mut len = (buf.len() * 2) as u32;
    let rc = unsafe {
        RegGetValueW(HKEY_LOCAL_MACHINE, key.as_ptr(), value.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut len)
    };
    if rc != 0 {
        return None;
    }
    let chars = (len as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buf[..chars.min(buf.len())]))
}

/// Today on this computer's clock and time zone: `(year, month, day)`.
pub fn local_date() -> (u16, u16, u16) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::SystemInformation::GetLocalTime;
        let mut t = unsafe { std::mem::zeroed() };
        unsafe { GetLocalTime(&mut t) };
        (t.wYear, t.wMonth, t.wDay)
    }
    #[cfg(not(windows))]
    {
        // Days since 1970 in UTC, to civil date (Howard Hinnant).
        let days = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|d| d.as_secs() as i64 / 86_400).unwrap_or(0);
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        ((yoe + era * 400 + i64::from(m <= 2)) as u16, m as u16, d as u16)
    }
}

/// Physical memory of this computer in MB.
pub fn physical_memory_mb() -> Option<usize> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        let mut m: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
        m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        let ok = unsafe { GlobalMemoryStatusEx(&mut m) } != 0;
        ok.then_some((m.ullTotalPhys / (1024 * 1024)) as usize)
    }
    #[cfg(not(windows))]
    None
}

/// Memory the open series may use (MB): the setting, but at most half of this
/// computer's memory (older PCs with 2–4 GB stay responsive) and what a 32-bit
/// program can address.
pub fn memory_budget_mb(setting: usize) -> usize {
    let mut mb = setting;
    if let Some(ram) = physical_memory_mb() {
        mb = mb.min((ram / 2).max(1024));
    }
    if cfg!(target_pointer_width = "32") {
        mb = mb.min(1400);
    }
    mb
}

/// True inside Remote Desktop / Citrix-style sessions, where GPUs are often limited.
pub fn is_remote_session() -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_REMOTESESSION};
        unsafe { GetSystemMetrics(SM_REMOTESESSION) != 0 }
    }
    #[cfg(not(windows))]
    false
}
