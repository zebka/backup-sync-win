//! تنظیمات برنامه (زبان + راه‌اندازی با ویندوز).
//! ریجستری فقط برای وضعیت auto-start منبع حقیقت است؛ زبان در settings.json نگه‌داری می‌شود.

#[cfg(windows)]
use std::os::windows::process::CommandExt;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub auto_start: bool,
    /// "fa" | "en"
    #[serde(default = "default_language")]
    pub language: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self { auto_start: false, language: default_language() }
    }
}

fn default_language() -> String {
    "fa".to_string()
}

fn root_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from(std::env::var("APPDATA").unwrap_or_default()))
        .join("BackupSync")
}

const FILE_NAME: &str = "settings.json";

pub fn load() -> Settings {
    load_from(&root_dir())
}

pub fn load_from(dir: &Path) -> Settings {
    let path = dir.join(FILE_NAME);
    if !path.exists() {
        return Settings::default();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
        .unwrap_or_default()
}

pub fn save_from(dir: &Path, settings: &Settings) -> Result<(), String> {
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| format!("failed to serialize settings: {e}"))?;
    crate::history::atomic_write(dir, FILE_NAME, &json)
}

pub fn save(settings: &Settings) -> Result<(), String> {
    save_from(&root_dir(), settings)
}

/// زبان فعلی برنامه.
pub fn current_language() -> String {
    load().language
}

/// ترجمهٔ رشته‌های بک‌اند بر اساس زبان ذخیره‌شده.
pub fn tr(fa: &str, en: &str) -> String {
    if current_language() == "en" {
        en.to_string()
    } else {
        fa.to_string()
    }
}

/// نسخهٔ Format! ورودی‌های String.
pub fn trf(fa: String, en: String) -> String {
    if current_language() == "en" {
        en
    } else {
        fa
    }
}

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
pub fn set_auto_start(enabled: bool) -> Result<Settings, String> {
    let mut settings = load();
    let exe = std::env::current_exe().map_err(|e| format!("failed to locate app path: {e}"))?;
    let exe = exe.to_string_lossy();
    let command_line = format!("\"{exe}\"");
    if enabled {
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
            .map_err(|e| format!("reg add failed: {e}"))?;
        if !out.status.success() {
            return Err(tr(
                "ثبت راه‌اندازی خودکار در رجیستری شکست خورد.",
                "Failed to register Windows startup.",
            ));
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
            .map_err(|e| format!("reg delete failed: {e}"))?;
        if !out.status.success() && is_auto_start_enabled() {
            return Err(tr(
                "حذف راه‌اندازی خودکار از رجیستری شکست خورد.",
                "Failed to remove Windows startup.",
            ));
        }
    }
    settings.auto_start = enabled;
    save(&settings)?;
    Ok(settings)
}

#[cfg(not(windows))]
pub fn set_auto_start(_enabled: bool) -> Result<Settings, String> {
    Err(tr(
        "راه‌اندازی خودکار فقط روی ویندوز پشتیبانی می‌شود.",
        "Windows startup is supported on Windows only.",
    ))
}

/// زبان را ذخیره می‌کند و Settings واقعی را برمی‌گرداند.
pub fn set_language(language: &str) -> Result<Settings, String> {
    if language != "fa" && language != "en" {
        return Err(tr("زبان نامعتبر است.", "Invalid language."));
    }
    let mut settings = load();
    settings.language = language.to_string();
    settings.auto_start = is_auto_start_enabled();
    save(&settings)?;
    Ok(settings)
}
