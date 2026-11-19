//! راه‌اندازی خودکار برنامه با ویندوز (HKCU Run key، بدون نیاز به ادمین).

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
pub fn is_auto_start_enabled() -> bool {
    std::process::Command::new("reg")
        .args([
            "query",
            "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
            "/v",
            "BackupSync",
        ])
        .creation_flags(0x0800_0000)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(not(windows))]
pub fn is_auto_start_enabled() -> bool {
    false
}

#[cfg(windows)]
pub fn set_auto_start(enable: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("مسیر برنامه پیدا نشد: {e}"))?;
    let exe = exe.to_string_lossy();
    // مسیرهای دارای فاصله باید داخل گیومه قرار بگیرند
    let command_line = format!("\"{exe}\"");
    if enable {
        let out = std::process::Command::new("reg")
            .args([
                "add",
                "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                "/v",
                "BackupSync",
                "/t",
                "REG_SZ",
                "/d",
                &command_line,
                "/f",
            ])
            .creation_flags(0x0800_0000)
            .output()
            .map_err(|e| format!("اجرای reg add شکست خورد: {e}"))?;
        if !out.status.success() {
            return Err("ثبت راه‌اندازی خودکار در رجیستری شکست خورد.".to_string());
        }
    } else {
        let out = std::process::Command::new("reg")
            .args([
                "delete",
                "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run",
                "/v",
                "BackupSync",
                "/f",
            ])
            .creation_flags(0x0800_0000)
            .output()
            .map_err(|e| format!("اجرای reg delete شکست خورد: {e}"))?;
        // اگر مقدار از قبل نبود، پاک‌سازی عملاً موفق است
        if !out.status.success() && is_auto_start_enabled() {
            return Err("حذف راه‌اندازی خودکار از رجیستری شکست خورد.".to_string());
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn set_auto_start(_enable: bool) -> Result<(), String> {
    Err("راه‌اندازی خودکار فقط روی ویندوز پشتیبانی می‌شود.".to_string())
}

