#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod history;
mod profiles;
mod robocopy;
mod settings;

use chrono::{Datelike, Timelike};
use profiles::Profile;
use robocopy::{normalize_path, CheckPathResult, PreviewResult, RunHandle};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, RunEvent, State, WindowEvent};

pub struct AppState {
    pub root: PathBuf,
    pub logs_dir: PathBuf,
    pub running: Arc<Mutex<Option<RunHandle>>>,
}

impl AppState {
    fn init() -> Self {
        let root = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from(std::env::var("APPDATA").unwrap_or_default()))
            .join("BackupSync");
        let logs_dir = root.join("logs");
        let _ = fs::create_dir_all(&logs_dir);
        AppState {
            root,
            logs_dir,
            running: Arc::new(Mutex::new(None)),
        }
    }
}

// ------------------------------------------------------------------
// اعتبارسنجی پروفایل (P1-12) — در save و start
// ------------------------------------------------------------------

fn validate_profile(profile: &Profile) -> Result<(), String> {
    if profile.source.trim().is_empty() || profile.destination.trim().is_empty() {
        return Err("مسیر منبع و مقصد نمی‌تواند خالی باشد.".to_string());
    }
    if normalize_path(&profile.source).to_lowercase()
        == normalize_path(&profile.destination).to_lowercase()
    {
        return Err("مسیر منبع و مقصد نمی‌توانند یکسان باشند.".to_string());
    }
    for p in &profile.excludes {
        let t = p.trim();
        if t.is_empty() {
            return Err("الگوی حذف نمی‌تواند خالی باشد.".to_string());
        }
        if t.starts_with('/') {
            return Err(format!(
                "الگوی حذف با «/» شروع می‌شود و به‌عنوان سوییچ robocopy خوانده می‌شود: {t}"
            ));
        }
    }
    if profile.schedule_enabled {
        if profile.schedule_frequency != "daily" && profile.schedule_frequency != "weekly" {
            return Err("تکرار زمان‌بندی نامعتبر است.".to_string());
        }
        if profile.schedule_action != "backup" && profile.schedule_action != "check" {
            return Err("نوع کار زمان‌بندی نامعتبر است.".to_string());
        }
        if parse_hhmm(&profile.schedule_time).is_none() {
            return Err("ساعت زمان‌بندی نامعتبر است (HH:MM).".to_string());
        }
        if profile.schedule_frequency == "weekly" && profile.schedule_weekday > 6 {
            return Err("روز هفته زمان‌بندی نامعتبر است.".to_string());
        }
    }
    Ok(())
}

// ------------------------------------------------------------------
// زمان‌بندی رجیستری/ساعت و اجرای خودکار
// ------------------------------------------------------------------

fn parse_hhmm(s: &str) -> Option<(u8, u8)> {
    let mut parts = s.split(':');
    let h = parts.next()?.trim().parse::<u8>().ok()?;
    let m = parts.next()?.trim().parse::<u8>().ok()?;
    if parts.next().is_some() || h > 23 || m > 59 {
        return None;
    }
    Some((h, m))
}

fn weekday_index(now: chrono::DateTime<chrono::Local>) -> u8 {
    use chrono::Weekday::*;
    match now.weekday() {
        Sat => 0,
        Sun => 1,
        Mon => 2,
        Tue => 3,
        Wed => 4,
        Thu => 5,
        Fri => 6,
    }
}

/// بررسی هر ۲۰ ثانیه؛ پروفایل‌های فعال با ساعت/روز فعلی اجرا می‌شوند.
async fn run_scheduler(app: AppHandle) {
    let state = app.state::<AppState>();
    let mut last_fired: HashMap<String, ()> = HashMap::new();
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(20)).await;
        let profiles = match profiles::load(&state.root) {
            Ok(list) => list,
            Err(_) => continue, // اگر فایل خراب است، silent ادامه ندهیم (کاربر در UI می‌بیند)
        };
        let now = chrono::Local::now();
        for profile in profiles {
            if !profile.schedule_enabled {
                continue;
            }
            let Some((h, m)) = parse_hhmm(&profile.schedule_time) else {
                continue;
            };
            if now.hour() != h as u32 || now.minute() != m as u32 {
                continue;
            }
            if profile.schedule_frequency == "weekly"
                && weekday_index(now) != profile.schedule_weekday
            {
                continue;
            }
            let marker = format!("{}-{}", profile.id, now.format("%Y%m%d%H%M"));
            if last_fired.contains_key(&marker) {
                continue;
            }
            last_fired.insert(marker, ());
            run_scheduled_task(app.clone(), profile).await;
        }
    }
}

async fn run_scheduled_task(app: AppHandle, profile: Profile) {
    use tauri_plugin_notification::NotificationExt;
    let state = app.state::<AppState>();
    if profile.schedule_action == "check" {
        let run_id = format!("check_{}", chrono::Local::now().format("%Y%m%d_%H%M%S"));
        let (handle, guard) = match robocopy::register_run(&state.running, &run_id) {
            Ok(t) => t,
            Err(_) => return, // یک اجرای دستی یا دیگری فعال است؛ تلاش دوباره دقیقه بعد
        };
        let res = robocopy::preview_run(&profile, None, &handle, &state.root).await;
        drop(guard);
        match res {
            Ok(p) => {
                let _ = app
                    .notification()
                    .builder()
                    .title("بررسی زمان‌بندی کامل شد")
                    .body(format!(
                        "{} فایل جدید و {} فایل تغییرکرده یافت شد.",
                        p.new_count, p.changed_count
                    ))
                    .show();
            }
            Err(e) => {
                let _ = app
                    .notification()
                    .builder()
                    .title("بررسی زمان‌بندی ناموفق بود")
                    .body(e)
                    .show();
            }
        }
        return;
    }

    // بکاپ واقعی زمان‌بندی‌شده: هرگز allow_mass_delete=true ندهیم (امنیت حالت آینه)
    match robocopy::start_run(
        app.clone(),
        state.root.clone(),
        state.running.clone(),
        profile,
        None,
        false,
    )
    .await
    {
        Ok(_) => {}
        Err(e) => {
            let _ = app
                .notification()
                .builder()
                .title("اجرای زمان‌بندی‌شده انجام نشد")
                .body(e)
                .show();
        }
    }
}

// ------------------------------------------------------------------
// Commands (PLAN §8)
// ------------------------------------------------------------------

#[tauri::command]
fn get_auto_start() -> Result<bool, String> {
    Ok(settings::is_auto_start_enabled())
}

#[tauri::command]
fn set_auto_start(enabled: bool) -> Result<bool, String> {
    settings::set_auto_start(enabled)?;
    Ok(settings::is_auto_start_enabled())
}

#[tauri::command]
fn delete_profile(state: State<'_, AppState>, profile_id: String) -> Result<Vec<Profile>, String> {
    let id = profile_id.trim();
    if id.is_empty() {
        return Err("شناسه پروفایل نامعتبر است.".to_string());
    }
    profiles::remove(&state.root, id)?;
    profiles::load(&state.root)
}

#[tauri::command]
async fn load_profiles(state: State<'_, AppState>) -> Result<Vec<Profile>, String> {
    profiles::load(&state.root)
}

#[tauri::command]
async fn save_profile(state: State<'_, AppState>, profile: Profile) -> Result<(), String> {
    validate_profile(&profile)?;
    profiles::upsert(&state.root, profile)
}

#[tauri::command]
async fn preview_changes(
    state: State<'_, AppState>,
    profile: Profile,
    password: Option<String>,
) -> Result<PreviewResult, String> {
    // پیش‌نمایش هم جای «در حال اجرا» را می‌گیرد (P0-4) تا شروع بکاپ وسط
    // آن ممکن نشود؛ لاگ موقتی یکتا داخل preview_run ساخته می‌شود.
    let (handle, guard) = robocopy::register_run(&state.running, "preview")?;
    let res = robocopy::preview_run(&profile, password.as_deref(), &handle, &state.root).await;
    drop(guard); // آزادسازی جایگاه
    res
}

#[tauri::command]
async fn start_backup(
    app: AppHandle,
    state: State<'_, AppState>,
    profile: Profile,
    password: Option<String>,
    allow_mass_delete: Option<bool>,
) -> Result<String, String> {
    validate_profile(&profile)?;
    robocopy::start_run(
        app,
        state.root.clone(),
        state.running.clone(),
        profile,
        password,
        allow_mass_delete.unwrap_or(false),
    )
    .await
}

#[tauri::command]
async fn cancel_backup(state: State<'_, AppState>, run_id: Option<String>) -> Result<(), String> {
    let cell = state.running.lock().unwrap();
    match cell.as_ref() {
        Some(h) => {
            if let Some(rid) = run_id {
                if h.id != rid {
                    return Err("این شناسه با بکاپ فعال یکی نیست.".to_string());
                }
            }
            h.cancelled.store(true, Ordering::SeqCst);
            if let Some(c) = h.child.lock().unwrap().as_mut() {
                let _ = c.start_kill(); // kill تمیز و سریع فرآیند robocopy
            }
            Ok(())
        }
        None => Err("بکاپ فعالی وجود ندارد.".to_string()),
    }
}

#[tauri::command]
async fn list_history(state: State<'_, AppState>) -> Result<Vec<history::RunSummary>, String> {
    history::load(&state.root)
}

/// کاراکترهای مجاز شناسه اجرا — جلوگیری از path traversal.
fn valid_run_id(run_id: &str) -> bool {
    !run_id.is_empty()
        && run_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[tauri::command]
async fn read_log(state: State<'_, AppState>, run_id: String) -> Result<String, String> {
    if !valid_run_id(&run_id) {
        return Err("شناسه اجرا نامعتبر است.".to_string());
    }
    let path = state.logs_dir.join(format!("{run_id}.log"));
    if !path.exists() {
        return Err("فایل لاگ این اجرا پیدا نشد.".to_string());
    }
    robocopy::read_log_file(&path)
}

/// سابقهٔ فایل‌های منتقل‌شده به _Deleted برای یک اجرا (P0-6c).
#[tauri::command]
async fn read_moved(state: State<'_, AppState>, run_id: String) -> Result<Option<String>, String> {
    if !valid_run_id(&run_id) {
        return Err("شناسه اجرا نامعتبر است.".to_string());
    }
    let path = state.logs_dir.join(format!("{run_id}.moved.txt"));
    if !path.exists() {
        return Ok(None);
    }
    fs::read_to_string(&path)
        .map(Some)
        .map_err(|e| format!("خواندن فایل منتقل‌شده‌ها ممکن نشد: {e}"))
}

#[tauri::command]
async fn check_path(path: String) -> Result<CheckPathResult, String> {
    // UNC در دسترس‌ناپذیر تا ۲۰-۴۰ ثانیه metadata را قفل می‌کند → تایم‌اوت (P0-5)
    Ok(robocopy::check_path_timeout(path).await)
}

// ------------------------------------------------------------------
// بستن پنجره حین اجرا (P1-11)
// ------------------------------------------------------------------

/// فقط یک دیالوگ تأیید در یک زمان (C3) — کلیک‌های مکرر X دیالوگ‌های متعدد باز نمی‌کنند.
static CONFIRM_DIALOG_OPEN: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// پنجره بسته نمی‌شود؛ از کاربر سؤال می‌پرسیم. در صورت تأیید: لغو + کشتن
/// + صبر برای ثبت تاریخچه، بعد خروج. dialog باید روی thread خودش باشد
/// (blocking_show main thread را قفل می‌کند).
fn confirm_exit_during_run(app: AppHandle) {
    use std::sync::atomic::Ordering;
    // اگر قبلاً باز است، دوباره باز نکن
    if CONFIRM_DIALOG_OPEN.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        // در هر خروجی (تأیید، انصراف، خطا) پرچم پایین می‌آید تا تأیید بعدی ممکن شود
        struct FlagGuard;
        impl Drop for FlagGuard {
            fn drop(&mut self) {
                CONFIRM_DIALOG_OPEN.store(false, Ordering::SeqCst);
            }
        }
        let _flag_guard = FlagGuard;

        use tauri::Manager;
        use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
        let window = app.get_webview_window("main");
        let mut builder = app
            .dialog()
            .message(
                "یک بکاپ یا پیش‌نمایش در حال اجراست.\nبا خروج، اجرای فعلی لغو می‌شود \
                 (نتیجه در تاریخچه ثبت می‌شود). خارج می‌شوی؟",
            )
            .title("خروج در حین اجرا")
            .buttons(MessageDialogButtons::OkCancelCustom(
                "خروج و لغو".to_string(),
                "ماندن".to_string(),
            ));
        // دیالوگ باید جلوی پنجره اصلی باز شود نه پشت آن
        builder = match window.as_ref() {
            Some(w) => builder.parent(w),
            None => builder,
        };
        let confirmed = builder.blocking_show();
        if !confirmed {
            return;
        }

        let state = app.state::<AppState>();
        let (cancelled, child) = {
            let cell = state.running.lock().unwrap();
            match cell.as_ref() {
                Some(h) => (h.cancelled.clone(), h.child.clone()),
                None => {
                    let _ = app.exit(0);
                    return;
                }
            }
        };
        cancelled.store(true, Ordering::SeqCst);
        if let Some(c) = child.lock().unwrap().as_mut() {
            let _ = c.start_kill();
        }
        // صبر تا مونیتور تاریخچه را بنویسد و جایگاه آزاد شود (~۱۵ ثانیه سقف)
        for _ in 0..100 {
            std::thread::sleep(std::time::Duration::from_millis(150));
            if state.running.lock().unwrap().is_none() {
                break;
            }
        }
        let _ = app.exit(0);
    });
}

// ------------------------------------------------------------------
// ورودی برنامه
// ------------------------------------------------------------------

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(AppState::init())
        .invoke_handler(tauri::generate_handler![
            load_profiles,
            save_profile,
            preview_changes,
            start_backup,
            cancel_backup,
            list_history,
            read_log,
            read_moved,
            check_path,
            get_auto_start,
            set_auto_start,
            delete_profile,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                run_scheduler(handle).await;
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("خطا در ساخت برنامه tauri")
        .run(|app, event| match event {
            // P1-11: CloseRequested — اگر چیزی در حال اجراست، اول سؤال کن
            RunEvent::WindowEvent {
                event: WindowEvent::CloseRequested { api, .. },
                ..
            } => {
                let running = app.state::<AppState>().running.lock().unwrap().is_some();
                if running {
                    api.prevent_close();
                    confirm_exit_during_run(app.clone());
                }
            }
            RunEvent::Exit => {
                // آخرین لنگر: فرآیند robocopy را بکش (PLAN §10)
                let state = app.state::<AppState>();
                let guard = state.running.lock().unwrap();
                if let Some(h) = guard.as_ref() {
                    if let Some(c) = h.child.lock().unwrap().as_mut() {
                        let _ = c.start_kill();
                    }
                }
            }
            _ => {}
        });
}
