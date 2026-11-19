use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// یک پروفایل بکاپ: جفت منبع + مقصد + گزینه‌ها.
/// رمز عبور شبکه عمداً اینجا وجود ندارد — هرگز نباید روی دیسک ذخیره شود.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub source: String,
    pub destination: String,
    /// "safe" | "mirror_recycle"
    pub mode: String,
    #[serde(default)]
    pub excludes: Vec<String>,
    #[serde(default)]
    pub network_user: Option<String>,
    #[serde(default = "default_threads")]
    pub threads: u32,
    #[serde(default)]
    pub schedule_enabled: bool,
    /// "daily" | "weekly"
    #[serde(default = "default_frequency")]
    pub schedule_frequency: String,
    /// HH:MM، مثلاً 21:30
    #[serde(default = "default_time")]
    pub schedule_time: String,
    /// برای weekly: شنبه=0 ... جمعه=6
    #[serde(default)]
    pub schedule_weekday: u8,
    /// "backup" | "check"
    #[serde(default = "default_action")]
    pub schedule_action: String,
}

fn default_threads() -> u32 {
    8
}

fn default_frequency() -> String {
    "daily".to_string()
}

fn default_time() -> String {
    "21:30".to_string()
}

fn default_action() -> String {
    "backup".to_string()
}

const FILE_NAME: &str = "profiles.json";

/// بارگذاری پروفایل‌ها. فایل خراب دیگر بی‌صدا نادیده گرفته نمی‌شود (P1-10):
/// به `<name>.bad` منتقل می‌شود و خطا به UI گزارش می‌شود.
pub fn load(dir: &Path) -> Result<Vec<Profile>, String> {
    let path = dir.join(FILE_NAME);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let s = fs::read_to_string(&path).map_err(|e| format!("خواندن {FILE_NAME} ممکن نشد: {e}"))?;
    match serde_json::from_str::<Vec<Profile>>(&s) {
        Ok(v) => Ok(v),
        Err(e) => {
            let bad = crate::history::move_aside_corrupt(dir, FILE_NAME);
            Err(format!(
                "{FILE_NAME} خراب بود ({e}). فایل به «{bad}» منتقل شد و فهرست خالی شروع می‌شود."
            ))
        }
    }
}

/// ذخیره اتمی (P1-10): نوشتن در `.tmp` و بعد rename روی هدف.
pub fn save(dir: &Path, profiles: &[Profile]) -> Result<(), String> {
    crate::history::atomic_write(dir, FILE_NAME, &crate::history::to_json(profiles)?)
}

/// افزودن/به‌روزرسانی یک پروفایل در لیست. حداکثر سه پروفایل نگه می‌دارد.
pub fn upsert(dir: &Path, profile: Profile) -> Result<(), String> {
    let mut list = load(dir)?;
    if let Some(existing) = list.iter_mut().find(|p| p.id == profile.id) {
        *existing = profile;
    } else {
        if list.len() >= 3 {
            return Err("حداکثر سه پروفایل قابل ذخیره است.".to_string());
        }
        list.push(profile);
    }
    save(dir, &list)
}

/// حذف یک پروفایل ذخیره‌شده از فهرست — فقط metadata، هرگز فایل‌های منبع/مقصد.
pub fn remove(dir: &Path, id: &str) -> Result<(), String> {
    let mut list = load(dir)?;
    let before = list.len();
    list.retain(|p| p.id != id);
    if list.len() == before {
        return Err("پروفایل ذخیره‌شده پیدا نشد.".to_string());
    }
    save(dir, &list)
}
