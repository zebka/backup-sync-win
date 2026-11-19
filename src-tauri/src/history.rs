use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// خلاصه یک اجرای بکاپ — هم برای تاریخچه هم رویداد finished.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub run_id: String,
    pub started_at: String,
    pub finished_at: String,
    pub duration_secs: f64,
    pub profile_name: String,
    pub mode: String,
    pub source: String,
    pub destination: String,
    /// "success" | "warnings" | "failed" | "cancelled"
    pub result: String,
    pub exit_code: i32,
    pub copied: u64,
    pub skipped: u64,
    pub failed: u64,
    /// فقط حالت آینه: تعداد فایل‌هایی که به _Deleted منتقل شدند
    pub moved_to_deleted: u64,
    pub bytes: u64,
    /// برای اجراهای شکست‌خورده: ۲۰ خط آخر لاگ
    pub last_lines: Vec<String>,
}

const MAX_HISTORY: usize = 50;
const FILE_NAME: &str = "history.json";

// ------------------------------------------------------------------
// کمکی‌های ذخیره‌سازی (P1-10) — از profiles.rs هم استفاده می‌شوند
// ------------------------------------------------------------------

pub(crate) fn to_json<T: Serialize + ?Sized>(value: &T) -> Result<String, String> {
    serde_json::to_string_pretty(value).map_err(|e| format!("خطا در سریال‌سازی: {e}"))
}

/// نوشتن اتمی: `<name>.tmp` و بعد rename روی هدف.
pub(crate) fn atomic_write(dir: &Path, name: &str, data: &str) -> Result<(), String> {
    let tmp = dir.join(format!("{name}.tmp"));
    fs::write(&tmp, data).map_err(|e| format!("خطا در نوشتن {name}.tmp: {e}"))?;
    fs::rename(&tmp, dir.join(name)).map_err(|e| format!("خطا در جایگزینی {name}: {e}"))?;
    Ok(())
}

/// فایل خراب را نگه می‌داریم ولی از سر راه برمی‌داریم: `<name>.bad`
/// (اگر قبلی هست، `<name>.bad.1`, `.bad.2`, …).
pub(crate) fn move_aside_corrupt(dir: &Path, name: &str) -> String {
    let src = dir.join(name);
    let mut target = dir.join(format!("{name}.bad"));
    let mut n = 1;
    while target.exists() {
        target = dir.join(format!("{name}.bad.{n}"));
        n += 1;
    }
    if fs::rename(&src, &target).is_ok() {
        return target
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("{name}.bad"));
    }
    // اگر rename نشد (قفل/اجازه)، دست‌کم نام_candidates را برمی‌گردانیم
    format!("{name} (بدون تغییر، خراب ماند)")
}

/// بارگذاری تاریخچه؛ فایل خراب بی‌صدا نادیده گرفته نمی‌شود (P1-10).
pub fn load(dir: &Path) -> Result<Vec<RunSummary>, String> {
    let path = dir.join(FILE_NAME);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let s = fs::read_to_string(&path).map_err(|e| format!("خواندن {FILE_NAME} ممکن نشد: {e}"))?;
    match serde_json::from_str::<Vec<RunSummary>>(&s) {
        Ok(v) => Ok(v),
        Err(e) => {
            let bad = move_aside_corrupt(dir, FILE_NAME);
            Err(format!(
                "{FILE_NAME} خراب بود ({e}). فایل به «{bad}» منتقل شد و تاریخچه خالی شروع می‌شود."
            ))
        }
    }
}

pub fn save(dir: &Path, runs: &[RunSummary]) -> Result<(), String> {
    atomic_write(dir, FILE_NAME, &to_json(runs)?)
}

/// افزودن یک اجرا به تاریخچه و نگه‌داشتن فقط ۵۰ اجرای آخر.
/// لاگ‌های اجراهای حذف‌شده هم پاک می‌شوند (P1-10).
pub fn append(dir: &Path, summary: RunSummary) -> Result<(), String> {
    let mut runs = load(dir)?;
    runs.insert(0, summary); // جدیدترین اول
    if runs.len() > MAX_HISTORY {
        let dropped: Vec<RunSummary> = runs.split_off(MAX_HISTORY);
        save(dir, &runs)?;
        remove_run_logs(dir, &dropped);
        return Ok(());
    }
    save(dir, &runs)
}

fn remove_run_logs(dir: &Path, dropped: &[RunSummary]) {
    let logs = dir.join("logs");
    for r in dropped {
        if r.run_id.is_empty()
            || !r
                .run_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            continue;
        }
        let _ = fs::remove_file(logs.join(format!("{}.log", r.run_id)));
        let _ = fs::remove_file(logs.join(format!("{}.moved.txt", r.run_id)));
    }
}
