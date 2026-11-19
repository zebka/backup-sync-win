//! Robocopy process management and UNILOG tailing.
//!
//! Key rule (PLAN §6): we NEVER parse robocopy's console stdout
//! (the OEM codepage mangles non-English file names). Instead we ask for a
//! UTF-16LE /UNILOG file and tail that file every 300 ms.
//!
//! Path handling (proven in T4): with /MT robocopy already prints
//! absolute paths; we do NOT add /FP. Relative paths are derived with
//! `relative_to` by stripping the source root (for copy entries) or the
//! destination root (for *EXTRA entries); we never `PathBuf::join` an
//! absolute path (that would discard the base and produce a move onto itself).

use serde::Serialize;
use serde_json::json;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

use crate::history::RunSummary;
use crate::profiles::Profile;
use crate::settings;

pub const DELETED_DIR: &str = "_Deleted";

fn tr(fa: &str, en: &str) -> String {
    settings::tr(fa, en)
}

fn trf(fa: String, en: String) -> String {
    settings::tr(&fa, &en)
}

// --------------------------------------------------------------------
// Data structures
// --------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewResult {
    pub new_files: Vec<FileEntry>,
    pub changed_files: Vec<FileEntry>,
    pub deleted_files: Vec<FileEntry>,
    pub new_count: usize,
    pub changed_count: usize,
    pub deleted_count: usize,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckPathResult {
    pub exists: bool,
    pub is_dir: bool,
    pub error: Option<String>,
}

pub struct RunHandle {
    pub id: String,
    pub child: Arc<Mutex<Option<tokio::process::Child>>>,
    pub cancelled: Arc<AtomicBool>,
}

// --------------------------------------------------------------------
// Path normalization (P0-7)
// --------------------------------------------------------------------

/// Normalize a path for the command line:
/// - `/` becomes `\`
/// - trailing backslashes stripped, EXCEPT drive roots like `E:\` (a bare
///   `E:` means "current directory on drive E").
pub fn normalize_path(p: &str) -> String {
    let mut s = p.trim().replace('/', "\\");
    if s.len() == 3 && s.as_bytes()[1] == b':' && s.ends_with('\\') {
        return s; // drive root "X:\"
    }
    while s.len() > 1 && s.ends_with('\\') {
        s.pop();
    }
    s
}

/// Strip `root` from the absolute path printed in the log; returns the
/// relative part. ASCII case-insensitive; if the path does not start with
/// the root, the (absolute) path is returned unchanged — never panics.
pub fn relative_to(root: &str, path: &str) -> String {
    let root_n = normalize_path(root);
    let path_n = normalize_path(path);
    if path_n.is_empty() {
        return path_n;
    }
    if bytes_eq_ignore_ascii(path_n.as_bytes(), root_n.as_bytes()) {
        return String::new(); // the root itself
    }
    let prefix = if root_n.ends_with('\\') {
        root_n.clone()
    } else {
        format!("{root_n}\\")
    };
    let pb = path_n.as_bytes();
    let pf = prefix.as_bytes();
    if pb.len() > pf.len()
        && pb[..pf.len()]
            .iter()
            .zip(pf.iter())
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
    {
        path_n[pf.len()..].to_string()
    } else {
        path_n
    }
}

fn bytes_eq_ignore_ascii(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.eq_ignore_ascii_case(y))
}

// --------------------------------------------------------------------
// Robocopy arguments
// NOTE: do not put quotes inside arguments manually — the built-in
// robocopy parser does not understand \-escapes std::process applies to
// quotes, and the whole line breaks (exit code 16, empty log).
// --------------------------------------------------------------------

pub fn build_args(
    src: &str,
    dst: &str,
    excludes: &[String],
    threads: u32,
    unilog: &Path,
    list_only: bool,
    mirror_list: bool,
) -> Vec<String> {
    let mut args = vec![
        src.to_string(),
        dst.to_string(),
        "/E".to_string(),
        "/COPY:DAT".to_string(),
        "/DCOPY:DAT".to_string(),
        "/FFT".to_string(),
        "/R:2".to_string(),
        "/W:3".to_string(),
        "/XJ".to_string(),
        "/NP".to_string(),
        "/BYTES".to_string(),
        // /L passes copy nothing; /MT interleaves UNILOG lines from
        // multiple threads (a real robocopy flush race) — keep them serial.
        if list_only {
            "/MT:1".to_string()
        } else {
            format!("/MT:{}", threads.clamp(1, 128))
        },
    ];

    if !excludes.is_empty() {
        args.push("/XF".to_string());
        args.extend(excludes.iter().cloned());
    }

    // The recycle bin must never be included in comparison or copy
    args.push("/XD".to_string());
    args.push(DELETED_DIR.to_string());

    if mirror_list {
        args.push("/MIR".to_string());
    }
    if list_only {
        args.push("/L".to_string());
    }

    args.push(format!("/UNILOG:{}", unilog.display()));
    args
}

fn spawn_robocopy(args: &[String]) -> Result<tokio::process::Child, String> {
    let mut cmd = tokio::process::Command::new("robocopy");
    cmd.args(args)
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .kill_on_drop(true); // P1-9: if the process is dropped, the child is killed
    cmd.spawn().map_err(|e| {
        trf(
            format!("خطا در اجرای robocopy: {e}"),
            format!("failed to start robocopy: {e}"),
        )
    })
}

fn new_run_id() -> String {
    format!(
        "{}_{}",
        chrono::Local::now().format("%Y%m%d_%H%M%S"),
        chrono::Local::now().timestamp_subsec_millis()
    )
}

// --------------------------------------------------------------------
// UTF-16LE decoding and log reading
// --------------------------------------------------------------------

pub fn decode_utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|p| u16::from_le_bytes([p[0], p[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

/// Read the log for **human display** — capped at the last 2 MB (P1-10).
pub fn read_log_file(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| {
        trf(
            format!("خواندن لاگ ممکن نشد: {e}"),
            format!("failed to read the log: {e}"),
        )
    })?;
    const CAP: usize = 2 * 1024 * 1024;
    if bytes.len() > CAP {
        let mut start = bytes.len() - CAP;
        start += start % 2;
        let mut text = decode_utf16(&bytes[start..]);
        if let Some(pos) = text.find('\n') {
            text = text[pos + 1..].to_string();
        }
        Ok(format!(
            "{}{text}",
            tr(
                "… لاگ طولانی است؛ فقط ۲ مگابایت آخر نمایش داده می‌شود.\n",
                "… log is too long; only the last 2 MB is shown.\n",
            )
        ))
    } else {
        Ok(decode_utf16(&bytes))
    }
}

/// Read the full log (no cap) — used for machine parsing only.
pub fn read_log_file_full(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| {
        trf(
            format!("خواندن لاگ ممکن نشد: {e}"),
            format!("failed to read the log: {e}"),
        )
    })?;
    Ok(decode_utf16(&bytes))
}

/// Read only the new bytes from `offset` with a seek (P1-8), never re-reading.
/// A trailing odd byte and a trailing unpaired high surrogate are kept for
/// the next tick so no UTF-16 unit is split.
///
/// With `flush=true` (at process exit) a trailing fragment without a
/// terminating newline is returned as the final line — robocopy flushes
/// line terminators lazily, so a killed run (T5-redo) leaves the last
/// line without `\r\n`.
pub fn tail_log(
    path: &Path,
    offset: u64,
    byte_pending: &mut Vec<u8>,
    text_pending: &mut String,
    flush: bool,
) -> (u64, Vec<String>) {
    let mut file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return (offset, Vec::new()),
    };
    if file.seek(SeekFrom::Start(offset)).is_err() {
        return (offset, Vec::new());
    }
    let mut new = Vec::new();
    if file.read_to_end(&mut new).is_err() {
        return (offset, Vec::new());
    }
    byte_pending.extend_from_slice(&new);

    // only consume an even number of bytes; hold a trailing odd byte
    let usable = byte_pending.len() - (byte_pending.len() % 2);
    let mut units: Vec<u16> = byte_pending[..usable]
        .chunks_exact(2)
        .map(|p| u16::from_le_bytes([p[0], p[1]]))
        .collect();
    // hold an unpaired trailing high surrogate for the next tick
    if let Some(&last) = units.last() {
        if (0xD800..=0xDBFF).contains(&last) {
            units.pop();
        }
    }
    let consumed = offset + new.len() as u64; // offset always advances by what we read
    let tail = byte_pending.split_off(units.len() * 2);
    *byte_pending = tail;

    let mut text = String::new();
    // strip the BOM only at file start
    let skip = if offset == 0 && units.first() == Some(&0xFEFF) {
        1
    } else {
        0
    };
    text.push_str(&String::from_utf16_lossy(&units[skip..]));
    text_pending.push_str(&text);

    let mut lines = Vec::new();
    if let Some(pos) = text_pending.rfind('\n') {
        let complete = text_pending[..=pos].to_string();
        *text_pending = text_pending[pos + 1..].to_string();
        for l in complete.split('\n') {
            lines.push(l.trim_end_matches('\r').to_string());
        }
    }
    if flush && !text_pending.is_empty() {
        lines.push(std::mem::take(text_pending));
    }
    (consumed, lines)
}

// --------------------------------------------------------------------
// Log line parser
// --------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    New,
    Newer,
    Older,
    Changed,
    Same,
    Tweaked,
    Extra,
    ExtraDir,
}

impl Kind {
    /// Without /XO these labels are copied too (P0-2).
    pub fn is_copied(self) -> bool {
        matches!(self, Kind::New | Kind::Newer | Kind::Older | Kind::Changed)
    }
}

/// Parse one log line: (kind, size in bytes, absolute path as printed).
/// Unknown lines return None and are ignored for events (but the raw log
/// is kept for diagnostics).
pub fn parse_line(line: &str) -> Option<(Kind, u64, String)> {
    let s = line.trim();
    if s.is_empty() {
        return None;
    }

    // order matters: check multi-word tags first
    let (kind, rest) = if let Some(r) = strip_tag(s, "*EXTRA File") {
        (Kind::Extra, r)
    } else if let Some(r) = strip_tag(s, "*EXTRA Dir") {
        (Kind::ExtraDir, r)
    } else if let Some(r) = strip_tag(s, "New File") {
        (Kind::New, r)
    } else if let Some(r) = strip_tag(s, "Newer") {
        (Kind::Newer, r)
    } else if let Some(r) = strip_tag(s, "Older") {
        (Kind::Older, r)
    } else if let Some(r) = strip_tag(s, "Changed") {
        (Kind::Changed, r)
    } else if let Some(r) = strip_tag(s, "Same") {
        (Kind::Same, r)
    } else if let Some(r) = strip_tag(s, "Tweaked") {
        (Kind::Tweaked, r)
    } else {
        return None;
    };

    let (size, path) = size_and_path(rest);
    if path.is_empty() {
        return None;
    }
    Some((kind, size, path))
}

fn strip_tag<'a>(s: &'a str, tag: &str) -> Option<&'a str> {
    if let Some(rest) = s.strip_prefix(tag) {
        if rest.starts_with(' ') || rest.starts_with('\t') {
            return Some(rest.trim_start());
        }
    }
    None
}

/// After the tag: an optional signed integer (size; for *EXTRA Dir it can
/// be negative) and then the path. The path itself may contain spaces.
fn size_and_path(rest: &str) -> (u64, String) {
    let t = rest.trim_start();
    let end = t.find(char::is_whitespace).unwrap_or(t.len());
    let first = &t[..end];
    if let Ok(n) = first.parse::<i64>() {
        let path = t[end..].trim_start().trim_end().to_string();
        return (n.max(0) as u64, path);
    }
    (0, t.trim_end().to_string())
}

/// Strict robocopy error-line pattern (P0-3): `ERROR <num> (0x...`.
pub fn is_error_line(line: &str) -> bool {
    let b = line.as_bytes();
    for i in 0..b.len() {
        if b[i] != b'E' {
            continue;
        }
        let starts_word = i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_');
        if starts_word && line[i..].starts_with("ERROR") {
            let rest = line[i + 5..].trim_start();
            let digits = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
            if digits > 0 && rest[digits..].trim_start().starts_with("(0x") {
                return true;
            }
        }
    }
    false
}

// --------------------------------------------------------------------
// Final summary table (position-based; robust to UI language)
// --------------------------------------------------------------------

fn parse_summary_row(line: &str) -> Option<[i64; 6]> {
    let t = line.trim();
    let colon = t.find(':')?;
    let label = t[..colon].trim();
    if label.is_empty() || label.chars().any(char::is_whitespace) {
        return None;
    }
    let tokens: Vec<&str> = t[colon + 1..].split_whitespace().collect();
    if tokens.len() != 6 {
        return None;
    }
    let mut vals = [0i64; 6];
    for (i, tok) in tokens.iter().enumerate() {
        vals[i] = tok.parse::<i64>().ok()?;
    }
    Some(vals)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SummaryTable {
    pub dirs: [i64; 6],
    pub files: [i64; 6],
    pub bytes: [i64; 6],
}

impl SummaryTable {
    pub fn files_copied(&self) -> u64 {
        self.files[1].max(0) as u64
    }
    pub fn files_skipped(&self) -> u64 {
        self.files[2].max(0) as u64
    }
    pub fn files_failed(&self) -> u64 {
        self.files[4].max(0) as u64
    }
    pub fn files_total(&self) -> u64 {
        self.files[0].max(0) as u64
    }
    pub fn files_extras(&self) -> u64 {
        self.files[5].max(0) as u64
    }
    pub fn bytes_copied(&self) -> u64 {
        self.bytes[1].max(0) as u64
    }
}

pub fn parse_summary_table(content: &str) -> Option<SummaryTable> {
    let rows: Vec<(usize, [i64; 6])> = content
        .lines()
        .enumerate()
        .filter_map(|(i, l)| parse_summary_row(l).map(|r| (i, r)))
        .collect();
    if rows.len() < 3 {
        return None;
    }
    let n = rows.len();
    let (i0, dirs) = rows[n - 3];
    let (i1, files) = rows[n - 2];
    let (i2, bytes) = rows[n - 1];
    if i1 == i0 + 1 && i2 == i1 + 1 {
        Some(SummaryTable {
            dirs,
            files,
            bytes,
        })
    } else {
        None
    }
}

/// Mirror fail-closed (C1): without a summary table we abort before moving.
pub fn ensure_mirror_list_ok(content: &str) -> Result<SummaryTable, String> {
    parse_summary_table(content).ok_or_else(|| {
        tr(
            "لاگ لیست آینه جدول خلاصه ندارد (خروجی robocopy خراب یا ناقص است)؛ هیچ فایلی جابه‌جا نشد.",
            "The mirror list log has no summary table (robocopy output corrupt or incomplete); no file was moved.",
        )
    })
}

// --------------------------------------------------------------------
// Exit code -> result (P1-9)
// --------------------------------------------------------------------

pub fn map_result(cancelled: bool, exit_code: i32) -> &'static str {
    if cancelled {
        "cancelled"
    } else if exit_code < 0 {
        "failed"
    } else if exit_code >= 8 {
        "failed"
    } else if exit_code >= 4 {
        "warnings"
    } else {
        "success"
    }
}

// --------------------------------------------------------------------
// Path validation and network access (P0-5)
// --------------------------------------------------------------------

pub fn check_path(path: &str) -> CheckPathResult {
    match fs::metadata(path) {
        Ok(md) => CheckPathResult {
            exists: true,
            is_dir: md.is_dir(),
            error: None,
        },
        Err(e) => CheckPathResult {
            exists: false,
            is_dir: false,
            error: Some(format!("{e}")),
        },
    }
}

/// check_path with a ~8 s timeout (P0-5).
pub async fn check_path_timeout(path: String) -> CheckPathResult {
    let task = tokio::task::spawn_blocking(move || check_path(&path));
    match tokio::time::timeout(std::time::Duration::from_secs(8), task).await {
        Ok(Ok(res)) => res,
        Ok(Err(_)) => CheckPathResult {
            exists: false,
            is_dir: false,
            error: Some(tr("بررسی مسیر شکست خورد.", "Path check failed.")),
        },
        Err(_) => CheckPathResult {
            exists: false,
            is_dir: false,
            error: Some(tr("سرور در دسترس نیست.", "The server is unreachable.")),
        },
    }
}

fn validate_source(source: &str) -> Result<(), String> {
    match fs::metadata(source) {
        Ok(md) if md.is_dir() => Ok(()),
        Ok(_) => Err(tr(
            "مسیر منبع یک پوشه نیست.",
            "The source path is not a folder.",
        )),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Err(tr(
            "دسترسی به منبع رد شد. نام کاربری/رمز شبکه را چک کن یا دسترسی سهم را.",
            "Access to the source was denied. Check network credentials or the share permission.",
        )),
        Err(_) => Err(trf(
            format!("مسیر منبع پیدا نشد: {source}\nچک کن سیستم روشن باشد و مسیر درست باشد."),
            format!("Source not found: {source}\nMake sure the host is powered on and the path is correct."),
        )),
    }
}

/// Prepare a source in the correct order (P0-5): net use first, then
/// validate_source with a timeout. Both preview_changes and start_run
/// use this so the order cannot diverge.
pub async fn prepare_source(
    user: Option<&str>,
    password: Option<&str>,
    source: &str,
) -> Result<(), String> {
    let src = normalize_path(source);
    if let (Some(user), Some(pass)) = (user, password) {
        if !user.is_empty() && !pass.is_empty() {
            // The host load can block ~40 s; bound it with the same 30 s timeout.
            match tokio::time::timeout(std::time::Duration::from_secs(30), net_use(user, pass, &src)).await {
                Ok(res) => res?,
                Err(_) => return Err(tr("سرور در دسترس نیست.", "The server is unreachable.")),
            }
        }
    }
    let src2 = src;
    let check = tokio::task::spawn_blocking(move || validate_source(&src2));
    match tokio::time::timeout(std::time::Duration::from_secs(30), check).await {
        Ok(Ok(res)) => res,
        Ok(Err(_)) => Err(tr("بررسی منبع شکست خورد.", "Source check failed.")),
        Err(_) => Err(tr("سرور در دسترس نیست.", "The server is unreachable.")),
    }
}

/// Temporary SMB connection using `net use`; the password is used only in
/// this call and never written anywhere. `net use` output is never shown
/// (it may contain the password).
async fn net_use(user: &str, password: &str, source: &str) -> Result<(), String> {
    let trimmed = source.trim_start_matches("\\\\");
    let host = trimmed.split('\\').next().unwrap_or_default();
    if host.is_empty() {
        return Ok(()); // local source
    }
    let target = format!("\\\\{host}\\IPC$");

    let output = tokio::process::Command::new("net")
        .args([
            "use",
            &target,
            password,
            &format!("/user:{user}"),
            "/persistent:no",
        ])
        .creation_flags(0x0800_0000)
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| {
            trf(
                format!("اجرای net use شکست خورد: {e}"),
                format!("net use failed: {e}"),
            )
        })?;

    if !output.status.success() {
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if text.contains("1219") {
            // Existing connection with other credentials — never auto-delete (P0-5)
            return Err(trf(
                format!(
                    "یک اتصال فعال دیگر به سرور {host} با اعتبارنامهٔ متفاوت وجود دارد.\nاول آن را قطع کن (در cmd):\nnet use \\\\{host}\\IPC$ /delete"
                ),
                format!(
                    "An active connection to {host} with different credentials already exists.\nDisconnect it first (in cmd):\nnet use \\\\{host}\\IPC$ /delete"
                ),
            ));
        }
        return Err(trf(
            format!("اتصال به سرور {host} با کاربر {user} ناموفق بود. نام کاربری/رمز را چک کن."),
            format!("Connection to {host} as {user} failed. Check username/password."),
        ));
    }
    Ok(())
}

fn validate_no_nesting(source: &str, destination: &str) -> Result<(), String> {
    let s = normalize_path(source).to_lowercase();
    let d = normalize_path(destination).to_lowercase();
    if s == d {
        return Err(tr(
            "منبع و مقصد نباید یکی باشند.",
            "Source and destination cannot be the same.",
        ));
    }
    if d.starts_with(&format!("{s}\\")) || d.starts_with(&format!("{s}/")) {
        return Err(tr(
            "مقصد نباید داخل منبع باشد (ریسک کپی بی‌نهایت).",
            "Destination cannot be inside source (infinite-copy risk).",
        ));
    }
    if s.starts_with(&format!("{d}\\")) || s.starts_with(&format!("{d}/")) {
        return Err(tr(
            "منبع نباید داخل مقصد باشد.",
            "Source cannot be inside destination.",
        ));
    }
    Ok(())
}

// --------------------------------------------------------------------
// Run slot (P0-4)
// --------------------------------------------------------------------

/// RAII guard for the "in progress" slot — released on any exit path
/// (error, panic, dropped future). `disarm` hands responsibility to the monitor.
pub struct SlotGuard {
    cell: Arc<Mutex<Option<RunHandle>>>,
    armed: bool,
}

impl SlotGuard {
    pub fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for SlotGuard {
    fn drop(&mut self) {
        if self.armed {
            *self.cell.lock().unwrap() = None;
        }
    }
}

/// Register a run handle synchronously before any `.await`. Returns an
/// error if the slot is already taken.
pub fn register_run(
    cell: &Arc<Mutex<Option<RunHandle>>>,
    id: &str,
) -> Result<(RunHandle, SlotGuard), String> {
    let mut guard = cell.lock().unwrap();
    if guard.is_some() {
        return Err(tr(
            "یک بکاپ یا پیش‌نمایش در حال اجراست؛ اول آن را لغو کن یا صبر کن تمام شود.",
            "A backup or preview is already running; cancel it first or wait for it to finish.",
        ));
    }
    let handle = RunHandle {
        id: id.to_string(),
        child: Arc::new(Mutex::new(None)),
        cancelled: Arc::new(AtomicBool::new(false)),
    };
    *guard = Some(RunHandle {
        id: id.to_string(),
        child: handle.child.clone(),
        cancelled: handle.cancelled.clone(),
    });
    drop(guard);
    Ok((
        handle,
        SlotGuard {
            cell: cell.clone(),
            armed: true,
        },
    ))
}

/// Wait for a child registered in `child_cell`; the cancel flag is checked
/// between polls and the child is killed if set. The mutex guard is never
/// held across an await.
async fn wait_child_in_cell(
    child_cell: &Arc<Mutex<Option<tokio::process::Child>>>,
    cancelled: &Arc<AtomicBool>,
) -> Option<std::process::ExitStatus> {
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let st = {
            let mut g = child_cell.lock().unwrap();
            match g.as_mut() {
                Some(c) => c.try_wait().ok().flatten(),
                None => return None,
            }
        };
        if let Some(s) = st {
            return Some(s);
        }
        if cancelled.load(Ordering::SeqCst) {
            let mut g = child_cell.lock().unwrap();
            if let Some(c) = g.as_mut() {
                let _ = c.start_kill();
            }
        }
    }
}

// --------------------------------------------------------------------
// Preview
// --------------------------------------------------------------------

/// Preview run. The slot is registered by the caller; a unique temporary
/// log is created and removed when done.
pub async fn preview_run(
    profile: &Profile,
    password: Option<&str>,
    handle: &RunHandle,
    data_root: &Path,
) -> Result<PreviewResult, String> {
    let src = normalize_path(&profile.source);
    let dst = normalize_path(&profile.destination);
    prepare_source(profile.network_user.as_deref(), password, &src).await?;
    validate_no_nesting(&src, &dst)?;

    let ts = chrono::Local::now().format("%Y%m%d_%H%M%S_%3f");
    let tmp_log = data_root.join("logs").join(format!("preview_{ts}.log"));
    let _ = fs::remove_file(&tmp_log);

    let args = build_args(
        &src,
        &dst,
        &profile.excludes,
        profile.threads,
        &tmp_log,
        true,
        true,
    );
    let child = spawn_robocopy(&args)?;
    *handle.child.lock().unwrap() = Some(child);
    let status = wait_child_in_cell(&handle.child, &handle.cancelled)
        .await
        .ok_or_else(|| tr("پیش‌نمایش ناتمام ماند.", "The preview did not finish."))?;
    if handle.cancelled.load(Ordering::SeqCst) {
        let _ = fs::remove_file(&tmp_log);
        return Err(tr("پیش‌نمایش لغو شد.", "Preview cancelled."));
    }

    let result = finish_preview(&tmp_log, status, &src, &dst);
    let _ = fs::remove_file(&tmp_log);
    result
}

fn finish_preview(
    tmp_log: &Path,
    status: std::process::ExitStatus,
    src: &str,
    dst: &str,
) -> Result<PreviewResult, String> {
    let code = status.code().unwrap_or(-1);
    if code >= 8 {
        let log = read_log_file_full(tmp_log).unwrap_or_default();
        let tail: Vec<&str> = log.lines().rev().take(20).collect();
        let tail = tail.into_iter().rev().collect::<Vec<_>>().join("\n");
        return Err(trf(
            format!("پیش‌نمایش با خطا متوقف شد (کد {code}):\n{tail}"),
            format!("Preview stopped with an error (code {code}):\n{tail}"),
        ));
    }

    let content = read_log_file_full(tmp_log)?;
    let mut new_files = Vec::new();
    let mut changed = Vec::new();
    let mut deleted = Vec::new();
    let mut total_bytes = 0u64;

    for line in content.lines() {
        match parse_line(line) {
            Some((k, size, abs)) if k.is_copied() => {
                total_bytes += size;
                let path = relative_to(src, &abs);
                let entry = FileEntry { path, size };
                match k {
                    Kind::New => new_files.push(entry),
                    _ => changed.push(entry),
                }
            }
            Some((Kind::Extra, size, abs)) => {
                deleted.push(FileEntry {
                    path: relative_to(dst, &abs),
                    size,
                });
            }
            _ => {}
        }
    }

    Ok(PreviewResult {
        new_count: new_files.len(),
        changed_count: changed.len(),
        deleted_count: deleted.len(),
        new_files,
        changed_files: changed,
        deleted_files: deleted,
        total_bytes,
    })
}

// --------------------------------------------------------------------
// Real run
// --------------------------------------------------------------------

#[derive(Default)]
struct Counts {
    copied: u64,
    skipped: u64,
    errors: u64,
    bytes: u64,
}

/// Start a backup. In mirror_recycle mode the deleted-at-source files are
/// moved to _Deleted first (P0-6), then the real copy runs.
pub async fn start_run(
    app: AppHandle,
    data_root: PathBuf,
    running_cell: Arc<Mutex<Option<RunHandle>>>,
    profile: Profile,
    password: Option<String>,
    allow_mass_delete: bool,
) -> Result<String, String> {
    let src = normalize_path(&profile.source);
    let dst = normalize_path(&profile.destination);
    validate_no_nesting(&src, &dst)?;

    let run_id = new_run_id();
    let (handle, guard) = register_run(&running_cell, &run_id)?;
    let started_at = chrono::Local::now();
    let cancelled = handle.cancelled.clone();

    let logs_dir = data_root.join("logs");
    let log_path = logs_dir.join(format!("{run_id}.log"));
    let list_log = logs_dir.join(format!("{run_id}.list.log"));
    let moved_log = logs_dir.join(format!("{run_id}.moved.txt"));
    let _ = fs::remove_file(&log_path);
    let _ = fs::remove_file(&list_log);
    let _ = fs::remove_file(&moved_log);

    if cancelled.load(Ordering::SeqCst) {
        return abort_cancelled(&app, &data_root, &profile, &run_id, started_at, 0);
    }

    if let Err(e) = prepare_source(profile.network_user.as_deref(), password.as_deref(), &src).await {
        if cancelled.load(Ordering::SeqCst) {
            return abort_cancelled(&app, &data_root, &profile, &run_id, started_at, 0);
        }
        return Err(e);
    }
    if cancelled.load(Ordering::SeqCst) {
        return abort_cancelled(&app, &data_root, &profile, &run_id, started_at, 0);
    }

    let mut moved = 0u64;
    if profile.mode == "mirror_recycle" {
        moved = match mirror_recycle_pass(
            &app,
            &profile,
            &run_id,
            &src,
            &dst,
            &list_log,
            &moved_log,
            &handle,
            allow_mass_delete,
        )
        .await
        {
            Ok(n) => n,
            Err(e) => {
                let _ = fs::remove_file(&list_log);
                if cancelled.load(Ordering::SeqCst) {
                    return abort_cancelled(&app, &data_root, &profile, &run_id, started_at, 0);
                }
                return Err(e);
            }
        };
        if cancelled.load(Ordering::SeqCst) {
            let _ = fs::remove_file(&list_log);
            return abort_cancelled(&app, &data_root, &profile, &run_id, started_at, moved);
        }
    }

    let args = build_args(
        &src,
        &dst,
        &profile.excludes,
        profile.threads,
        &log_path,
        false,
        false,
    );
    let child = spawn_robocopy(&args)?;
    *handle.child.lock().unwrap() = Some(child);

    let monitor_guard = SlotGuard {
        cell: running_cell.clone(),
        armed: true,
    };
    guard.disarm();
    tokio::spawn(monitor_run(
        app,
        data_root,
        running_cell,
        profile,
        run_id.clone(),
        log_path,
        handle.child.clone(),
        cancelled,
        moved,
        started_at,
        monitor_guard,
    ));

    Ok(run_id)
}

fn abort_cancelled(
    app: &AppHandle,
    data_root: &Path,
    profile: &Profile,
    run_id: &str,
    started_at: chrono::DateTime<chrono::Local>,
    moved: u64,
) -> Result<String, String> {
    let finished = chrono::Local::now();
    let summary = RunSummary {
        run_id: run_id.to_string(),
        started_at: started_at.format("%Y-%m-%d %H:%M:%S").to_string(),
        finished_at: finished.format("%Y-%m-%d %H:%M:%S").to_string(),
        duration_secs: (finished - started_at).num_milliseconds() as f64 / 1000.0,
        profile_name: profile.name.clone(),
        mode: profile.mode.clone(),
        source: profile.source.clone(),
        destination: profile.destination.clone(),
        result: "cancelled".to_string(),
        exit_code: -1,
        copied: 0,
        skipped: 0,
        failed: 0,
        moved_to_deleted: moved,
        bytes: 0,
        last_lines: Vec::new(),
    };
    let _ = crate::history::append(data_root, summary.clone());
    let _ = app.emit("backup://finished", &summary);
    Err(tr(
        "بکاپ لغو شد (حین آماده‌سازی).",
        "Backup cancelled during preparation.",
    ))
}

/// One /MIR /L pass to find files present in the destination but not in
/// the source, then move them to _Deleted. No real /PURGE is ever run.
///
/// P0-6: mass-delete guard, moved.txt, deepest-first empty-dir removal,
/// list log in logs/<run_id>.list.log and deleted afterwards.
#[allow(clippy::too_many_arguments)]
async fn mirror_recycle_pass(
    app: &AppHandle,
    profile: &Profile,
    run_id: &str,
    src: &str,
    dst: &str,
    list_log: &Path,
    moved_log: &Path,
    handle: &RunHandle,
    allow_mass_delete: bool,
) -> Result<u64, String> {
    let args = build_args(
        src,
        dst,
        &profile.excludes,
        profile.threads,
        list_log,
        true,
        true,
    );
    let child = spawn_robocopy(&args)?;
    *handle.child.lock().unwrap() = Some(child);
    let status = wait_child_in_cell(&handle.child, &handle.cancelled)
        .await
        .ok_or_else(|| tr("Ù„ÛŒØ³Øª Ø¢ÛŒÙ†Ù‡ Ù†Ø§ØªÙ…Ø§Ù… Ù…Ø§Ù†Ø¯.", "The mirror list did not finish."))?;
    if handle.cancelled.load(Ordering::SeqCst) {
        return Err(tr("Ù„ØºÙˆ Ø´Ø¯.", "Cancelled."));
    }
    if status.code().unwrap_or(-1) >= 8 {
        return Err(tr(
            "ØªØ´Ø®ÛŒØµ Ø­Ø°Ùâ€ŒØ´Ø¯Ù‡â€ŒÙ‡Ø§ Ø´Ú©Ø³Øª Ø®ÙˆØ±Ø¯Ø› Ø¨Ú©Ø§Ù¾ Ø§Ø¬Ø±Ø§ Ù†Ø´Ø¯.",
            "Detecting deleted files failed; the backup was not run.",
        ));
    }

    let content = read_log_file_full(list_log)?;

    // C1 fail-closed: no summary table -> abort before moving anything
    let table = ensure_mirror_list_ok(&content)?;

    // mass-delete guard (P0-6b)
    if !allow_mass_delete {
        let total = table.files_total();
        let extras = table.files_extras();
        if extras >= 10 && (total == 0 || extras * 2 > total) {
            return Err(format!(
                "MASS_DELETE_JSON:{{\"total\":{total},\"extras\":{extras}}}"
            ));
        }
    }

    let mut extra_files: Vec<String> = Vec::new();
    let mut extra_dirs: Vec<String> = Vec::new();
    for line in content.lines() {
        match parse_line(line) {
            Some((Kind::Extra, _, abs)) => {
                let rel = relative_to(dst, &abs);
                if !rel.is_empty() {
                    extra_files.push(rel);
                }
            }
            Some((Kind::ExtraDir, _, abs)) => {
                let rel = relative_to(dst, &abs);
                if !rel.is_empty() {
                    extra_dirs.push(rel);
                }
            }
            _ => {}
        }
    }
    if extra_files.is_empty() {
        let _ = fs::remove_file(list_log);
        return Ok(0);
    }

    let stamp = chrono::Local::now().format("%Y-%m-%d_%H%M%S").to_string();
    let recycle_root = PathBuf::from(dst).join(DELETED_DIR).join(&stamp);
    let dst_root = PathBuf::from(dst);

    let mut moved: u64 = 0;
    let mut moved_lines: Vec<String> = Vec::new();
    let mut failed_lines: Vec<String> = Vec::new();

    let msg_invalid_path = tr("مسیر نامعتبر", "invalid path");
    let msg_invalid_dest = tr("مسیر مقصد _Deleted نامعتبر", "invalid _Deleted destination");
    let msg_missing = tr("فایل در مقصد پیدا نشد", "file not found in destination");
    let msg_move_failed = tr("انتقال ناموفق", "move failed");

    for rel in &extra_files {
        if handle.cancelled.load(Ordering::SeqCst) {
            break;
        }
        // Mirror move safety (T4): never join an absolute path or ".."
        let rel_norm = rel.replace('/', "\\");
        if rel_norm.is_empty()
            || rel_norm.starts_with('\\')
            || rel_norm.contains("..")
            || Path::new(&rel_norm).is_absolute()
        {
            failed_lines.push(format!("{rel}\t{msg_invalid_path}"));
            continue;
        }
        let from = dst_root.join(&rel_norm);
        let to = recycle_root.join(&rel_norm);
        if from == to || !from.starts_with(&dst_root) || !to.starts_with(&recycle_root) {
            failed_lines.push(format!("{rel}\t{msg_invalid_dest}"));
            continue;
        }
        if !from.exists() {
            failed_lines.push(format!("{rel}\t{}", tr("ÙØ§ÛŒÙ„ Ø¯Ø± Ù…Ù‚ØµØ¯ Ù¾ÛŒØ¯Ø§ Ù†Ø´Ø¯", "file not found in destination")));
            failed_lines.push(format!("{rel}\t{msg_missing}"));
        }

        let _ = fs::create_dir_all(to.parent().unwrap_or(&recycle_root));
        let ok = match fs::rename(&from, &to) {
            Ok(_) => true,
            Err(_) => match fs::copy(&from, &to) {
                Ok(_) => fs::remove_file(&from).is_ok(),
                Err(_) => false,
            },
        };
        // count only after verifying destination exists and source is gone
        let verified = to.exists() && !from.exists();
        if ok && verified {
            moved += 1;
            moved_lines.push(rel.clone());
            let _ = app.emit(
                "backup://log",
                json!({ "runId": run_id, "lines": [trf(format!("â™» Ù…Ù†ØªÙ‚Ù„ Ø´Ø¯ Ø¨Ù‡ _Deleted: {rel}"), format!("â™» moved to _Deleted: {rel}"))] }),
            );
        } else {
            failed_lines.push(format!("{rel}\t{msg_move_failed}"));
            let _ = app.emit(
                "backup://log",
                json!({ "runId": run_id, "lines": [trf(format!("âš  Ø§Ù†ØªÙ‚Ø§Ù„ Ø¨Ù‡ _Deleted Ù†Ø§Ù…ÙˆÙÙ‚ Ø¨ÙˆØ¯: {rel}"), format!("âš  move to _Deleted failed: {rel}"))] }),
            );
        }
    }

    // record what was moved (P0-6c)
    let mut moved_txt = String::new();
    moved_txt.push_str(&format!("# {}\r\n", tr("Ù…Ù†ØªÙ‚Ù„â€ŒØ´Ø¯Ù‡ Ø¨Ù‡ _Deleted", "Moved to _Deleted")));
    for l in &moved_lines {
        moved_txt.push_str(l);
        moved_txt.push_str("\r\n");
    }
    if !failed_lines.is_empty() {
        moved_txt.push_str(&format!("# {}\r\n", tr("Ù†Ø§Ù…ÙˆÙÙ‚", "Failed")));
        for l in &failed_lines {
            moved_txt.push_str(l);
            moved_txt.push_str("\r\n");
        }
    }
    let _ = fs::write(moved_log, moved_txt);

    // cancel mid-move: return Ok(moved) so the caller records the real count (C5)
    if handle.cancelled.load(Ordering::SeqCst) {
        let _ = fs::remove_file(list_log);
        return Ok(moved);
    }

    // remove now-empty directories, deepest first (P0-6d), fs::remove_dir only
    let mut dir_rels: Vec<String> = extra_dirs.clone();
    for rel in &moved_lines {
        if let Some(parent) = Path::new(rel).parent() {
            let p = parent.to_string_lossy().to_string();
            if !p.is_empty() {
                dir_rels.push(p);
            }
        }
    }
    dir_rels.sort_by(|a, b| {
        let da = a.split('\\').count();
        let db = b.split('\\').count();
        db.cmp(&da).then(b.cmp(a))
    });
    dir_rels.dedup();
    for d in &dir_rels {
        if handle.cancelled.load(Ordering::SeqCst) {
            break;
        }
        if d.is_empty() || d.starts_with('\\') || d.contains("..") {
            continue;
        }
        let p = dst_root.join(d);
        if p.starts_with(&dst_root) && p != dst_root {
            let _ = fs::remove_dir(&p); // only succeeds if empty
        }
    }

    let _ = fs::remove_file(list_log); // P0-6e
    Ok(moved)
}

/// Process a batch of log lines: update counters and collect display lines (P1-8).
fn process_lines(
    src: &str,
    lines: Vec<String>,
    counts: &mut Counts,
    files_done: &mut u64,
    current_file: &mut String,
    last_lines: &mut Vec<String>,
    out: &mut Vec<String>,
) {
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        out.push(line.clone());

        last_lines.push(line.clone());
        if last_lines.len() > 20 {
            last_lines.remove(0);
        }

        if is_error_line(&line) {
            counts.errors += 1;
        }

        match parse_line(&line) {
            Some((k, size, abs)) if k.is_copied() => {
                counts.copied += 1;
                counts.bytes += size;
                *files_done += 1;
                *current_file = relative_to(src, &abs);
            }
            Some((Kind::Same, _, _)) | Some((Kind::Tweaked, _, _)) => counts.skipped += 1,
            _ => {}
        }
    }
}

/// Monitoring loop: tail /UNILOG every 300 ms, emit at most one log event
/// and one progress event per tick (P1-8), and take final counts from the
/// UNILOG summary table (P0-3).
#[allow(clippy::too_many_arguments)]
async fn monitor_run(
    app: AppHandle,
    data_root: PathBuf,
    running_cell: Arc<Mutex<Option<RunHandle>>>,
    profile: Profile,
    run_id: String,
    log_path: PathBuf,
    child_cell: Arc<Mutex<Option<tokio::process::Child>>>,
    cancelled: Arc<AtomicBool>,
    moved_count: u64,
    started_at: chrono::DateTime<chrono::Local>,
    guard: SlotGuard,
) {
    let src_norm = normalize_path(&profile.source);
    let mut offset = 0u64;
    let mut byte_pending: Vec<u8> = Vec::new();
    let mut text_pending = String::new();
    let mut counts = Counts::default();
    let mut current_file = String::new();
    let mut files_done = 0u64;
    let mut last_lines: Vec<String> = Vec::new();

    loop {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let exited = {
            let mut g = child_cell.lock().unwrap();
            match g.as_mut() {
                Some(c) => c.try_wait().ok().flatten().is_some(),
                None => true,
            }
        };

        let (new_offset, lines) = tail_log(
            &log_path,
            offset,
            &mut byte_pending,
            &mut text_pending,
            exited,
        );
        offset = new_offset;

        let (prev_files, prev_bytes) = (files_done, counts.bytes);
        let mut out = Vec::new();
        process_lines(
            &src_norm,
            lines,
            &mut counts,
            &mut files_done,
            &mut current_file,
            &mut last_lines,
            &mut out,
        );
        if !out.is_empty() {
            let _ = app.emit("backup://log", json!({ "runId": run_id, "lines": out }));
        }
        if files_done != prev_files || counts.bytes != prev_bytes {
            let _ = app.emit(
                "backup://progress",
                json!({
                    "runId": run_id,
                    "currentFile": current_file,
                    "filesDone": files_done,
                    "bytesDone": counts.bytes,
                }),
            );
        }

        if exited {
            break;
        }
    }

    let exit_code = {
        let mut g = child_cell.lock().unwrap();
        if let Some(c) = g.as_mut() {
            match c.try_wait() {
                Ok(Some(s)) => s.code().unwrap_or(-1),
                _ => -1,
            }
        } else {
            -1
        }
    };
    let was_cancelled = cancelled.load(Ordering::SeqCst);
    let result = map_result(was_cancelled, exit_code);

    // final counts from the UNILOG summary table (P0-3); fall back to live counters
    let full_log = read_log_file_full(&log_path).unwrap_or_default();
    let (copied, skipped, failed, bytes) = if let Some(t) = parse_summary_table(&full_log) {
        (
            t.files_copied(),
            t.files_skipped(),
            t.files_failed(),
            t.bytes_copied(),
        )
    } else {
        (counts.copied, counts.skipped, counts.errors, counts.bytes)
    };

    let finished = chrono::Local::now();
    let summary = RunSummary {
        run_id: run_id.clone(),
        started_at: started_at.format("%Y-%m-%d %H:%M:%S").to_string(),
        finished_at: finished.format("%Y-%m-%d %H:%M:%S").to_string(),
        duration_secs: (finished - started_at).num_milliseconds() as f64 / 1000.0,
        profile_name: profile.name.clone(),
        mode: profile.mode.clone(),
        source: profile.source.clone(),
        destination: profile.destination.clone(),
        result: result.to_string(),
        exit_code,
        copied,
        skipped,
        failed,
        moved_to_deleted: moved_count,
        bytes,
        last_lines: if result == "failed" {
            last_lines
        } else {
            Vec::new()
        },
    };

    let _ = crate::history::append(&data_root, summary.clone());

    // Windows notification (PLAN Â§4.5)
    use tauri_plugin_notification::NotificationExt;
    let (title, body) = match result {
        "success" => (
            settings::tr("Ø¨Ú©Ø§Ù¾ Ú©Ø§Ù…Ù„ Ø´Ø¯ âœ”", "Backup completed âœ”"),
            trf(format!("{copied} ÙØ§ÛŒÙ„ Ú©Ù¾ÛŒ Ø´Ø¯."), format!("{copied} files copied.")),
        ),
        "warnings" => (
            settings::tr("Ø¨Ú©Ø§Ù¾ Ø¨Ø§ Ù‡Ø´Ø¯Ø§Ø± Ú©Ø§Ù…Ù„ Ø´Ø¯", "Backup finished with warnings"),
            trf(format!("{copied} ÙØ§ÛŒÙ„ Ú©Ù¾ÛŒ Ø´Ø¯ØŒ Ú†Ù†Ø¯ Ù…ÙˆØ±Ø¯ Ù†Ø§Ù‡Ù…Ø§Ù‡Ù†Ú¯ Ø¨ÙˆØ¯."), format!("{copied} files copied; some mismatches detected.")),
        ),
        "failed" => (
            settings::tr("Ø¨Ú©Ø§Ù¾ Ø´Ú©Ø³Øª Ø®ÙˆØ±Ø¯ âœ–", "Backup failed âœ–"),
            trf(format!("Ú©Ø¯ Ø®Ø·Ø§: {exit_code}"), format!("exit code: {exit_code}")),
        ),
        _ => (
            settings::tr("Ø¨Ú©Ø§Ù¾ Ù„ØºÙˆ Ø´Ø¯", "Backup cancelled"),
            settings::tr("ØªÙˆØ³Ø· Ú©Ø§Ø±Ø¨Ø± Ù„ØºÙˆ Ø´Ø¯.", "Cancelled by the user."),
        ),
    };
    let _ = app.notification().builder().title(title).body(body).show();

    let _ = app.emit("backup://finished", &summary);

    // release the slot (the guard is the panic backstop)
    *running_cell.lock().unwrap() = None;
    drop(guard);
}


// --------------------------------------------------------------------
// Unit tests — fixtures from real T1/T4/T5/T9/T10 logs
// --------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("backupsync_test_{name}_{}", std::process::id()));
        let _ = fs::create_dir_all(&d);
        d
    }

    #[test]
    fn parse_line_all_tags_from_real_logs() {
        let src7 = "C:\\Users\\Alireza\\AppData\\Local\\Temp\\opencode\\bs_test\\src7";
        let dst = "C:\\Users\\Alireza\\AppData\\Local\\Temp\\opencode\\bs_test\\dst";

        // T9: destination older — robocopy copies anyway
        let (k, sz, p) = parse_line(
            "\t    Older     \t\t      33\tC:\\Users\\Alireza\\AppData\\Local\\Temp\\opencode\\bs_test\\src7\\t1.txt",
        )
        .unwrap();
        assert_eq!(k, Kind::Older);
        assert_eq!(sz, 33);
        assert!(p.starts_with(src7), "absolute path expected: {p}");

        // T10: same time, size differs -> Changed
        let (k, sz, p) = parse_line(
            "\t    Changed   \t\t      17\tC:\\Users\\Alireza\\AppData\\Local\\Temp\\opencode\\bs_test\\src7\\t2.txt",
        )
        .unwrap();
        assert_eq!(k, Kind::Changed);
        assert_eq!(sz, 17);
        assert!(p.starts_with(src7));

        // T5: big file — New File
        let (k, sz, p) = parse_line(
            "\t    New File  \t\t2147483648\tC:\\Users\\Alireza\\AppData\\Local\\Temp\\opencode\\bs_test\\src\\big.bin",
        )
        .unwrap();
        assert_eq!(k, Kind::New);
        assert_eq!(sz, 2_147_483_648);
        assert!(p.ends_with("big.bin"));

        // T4: *EXTRA File
        let (k, sz, p) = parse_line(
            "\t  *EXTRA File \t\t      17\tC:\\Users\\Alireza\\AppData\\Local\\Temp\\opencode\\bs_test\\dst\\b.txt",
        )
        .unwrap();
        assert_eq!(k, Kind::Extra);
        assert_eq!(sz, 17);
        assert!(p.starts_with(dst));

        // T4: *EXTRA Dir with negative number
        let (k, sz, p) = parse_line(
            "\t*EXTRA Dir        -1\tC:\\Users\\Alireza\\AppData\\Local\\Temp\\opencode\\bs_test\\dst\\sub_two\\",
        )
        .unwrap();
        assert_eq!(k, Kind::ExtraDir);
        assert_eq!(sz, 0);
        assert!(p.ends_with("sub_two\\"));

        // Same and Tweaked
        let (k, _, _) = parse_line("\t    Same       \t\t      17\tC:\\x\\a.txt").unwrap();
        assert_eq!(k, Kind::Same);
        let (k, _, _) = parse_line("\t    Tweaked    \t\t         \tC:\\x\\a.txt").unwrap();
        assert_eq!(k, Kind::Tweaked);

        // Newer
        let (k, _, _) = parse_line("\t    Newer      \t\t      17\tC:\\x\\a.txt").unwrap();
        assert_eq!(k, Kind::Newer);

        // headers and table rows -> None
        assert!(parse_line("-------------------------------------------------------------------------------").is_none());
        assert!(parse_line("               Total    Copied   Skipped  Mismatch    FAILED    Extras").is_none());
        assert!(parse_line("").is_none());
        assert!(parse_line("\t    New File  \t\t      17\tC:\\ERROR dir\\a.txt").is_some());
    }

    #[test]
    fn parse_line_persian_name_and_unc() {
        let line = "\t    New File  \t\t      14\t\\\\localhost\\TestShare\\src6\\افایل تست.txt";
        let (k, sz, p) = parse_line(line).unwrap();
        assert_eq!(k, Kind::New);
        assert_eq!(sz, 14);
        assert!(p.ends_with(".txt"));
    }

    #[test]
    fn relative_to_strips_roots() {
        let src = "C:\\Users\\Alireza\\AppData\\Local\\Temp\\opencode\\bs_test\\src";
        assert_eq!(
            relative_to(src, &format!("{src}\\a\\b.txt")),
            "a\\b.txt"
        );
        assert_eq!(
            relative_to(src.to_lowercase().as_str(), &format!("{src}\\a\\b.txt")),
            "a\\b.txt"
        );
        // mismatch -> original path
        assert_eq!(relative_to(src, "D:\\x.txt"), "D:\\x.txt");
        // drive root
        assert_eq!(relative_to("E:\\", "E:\\a.txt"), "a.txt");
        // UNC
        assert_eq!(
            relative_to("\\\\localhost\\TestShare\\src6", "\\\\localhost\\TestShare\\src6\\f1.txt"),
            "f1.txt"
        );
        assert_eq!(
            relative_to("\\\\h\\s", "\\\\h\\s\\sub\\deep\\فایل.txt"),
            "sub\\deep\\فایل.txt"
        );
        assert_eq!(relative_to(src, src), "");
        assert_eq!(
            relative_to(
                "C:\\Users\\Alireza\\AppData\\Local\\Temp\\opencode\\bs_test\\dst",
                "C:\\Users\\Alireza\\AppData\\Local\\Temp\\opencode\\bs_test\\dst\\sub_two\\",
            ),
            "sub_two"
        );
        assert_eq!(
            relative_to("C:\\src", "C:\\src\\sub\\"),
            "sub"
        );
    }

    #[test]
    fn t4_regression_join_absolute_discards_base() {
        // legacy bug: PathBuf::join with an absolute path drops the base
        let base = PathBuf::from("C:\\dst\\_Deleted\\ts");
        let abs = "C:\\dst\\b.txt";
        let buggy = base.join(abs);
        assert_eq!(buggy, PathBuf::from(abs), "join() with absolute drops base");
        // fix: relative_to always yields a relative path
        let rel = relative_to("C:\\dst", abs);
        assert_eq!(rel, "b.txt");
    }

    #[test]
    fn t4_regression_subfolder_move_paths() {
        let dst = "C:\\Users\\Alireza\\AppData\\Local\\Temp\\opencode\\bs_test\\dst";
        let abs = "C:\\Users\\Alireza\\AppData\\Local\\Temp\\opencode\\bs_test\\dst\\sub_two\\two files same name.txt";
        let rel = relative_to(dst, abs);
        assert_eq!(rel, "sub_two\\two files same name.txt");
        let ts = "2026-11-18_140000";
        let recycle_root = PathBuf::from(dst).join("_Deleted").join(ts);
        let expected = recycle_root.join(&rel);
        let got = recycle_root.join(rel.replace('/', "\\"));
        assert_eq!(got, expected);
    }

    #[test]
    fn parse_summary_table_t4_fixture() {
        let content = "\
               Total    Copied   Skipped  Mismatch    FAILED    Extras\r
    Dirs :         3         3         3         0         0         1\r
   Files :        10         1         9         0         0         4\r
   Bytes :       188        17       171         0         0        61\r
   Times :   0:00:00   0:00:00                       0:00:00   0:00:00\r
   Ended : 15 November 2026 13:58:27\r
";
        let t = parse_summary_table(content).expect("table must parse");
        assert_eq!(t.dirs, [3, 3, 3, 0, 0, 1]);
        assert_eq!(t.files, [10, 1, 9, 0, 0, 4]);
        assert_eq!(t.bytes, [188, 17, 171, 0, 0, 61]);
        assert_eq!(t.files_copied(), 1);
        assert_eq!(t.files_skipped(), 9);
        assert_eq!(t.files_failed(), 0);
        assert_eq!(t.bytes_copied(), 17);
        assert_eq!(t.files_extras(), 4);
        assert_eq!(t.files_total(), 10);
    }

    #[test]
    fn parse_summary_table_failed_positive() {
        let content = "\
               Total    Copied   Skipped  Mismatch    FAILED    Extras\r
    Dirs :         2         1         1         0         1         0\r
   Files :         6         3         2         0         2         0\r
   Bytes :       200        99        99         0         2         0\r
   Times :   0:00:01   0:00:01                       0:00:00   0:00:00\r
";
        let t = parse_summary_table(content).expect("table must parse");
        assert_eq!(t.files_failed(), 2);
        assert_eq!(t.files_copied(), 3);
    }

    #[test]
    fn parse_summary_table_absent_for_killed_run() {
        let content = "\t    New File  \t\t10737418240\tC:\\x\\huge10.bin";
        assert!(parse_summary_table(content).is_none());
        assert!(parse_summary_table("").is_none());
        // non-consecutive rows -> None
        let content = "\
    Dirs :         1         1         1         0         0         0\r
   Times :   0:00:00   0:00:00                       0:00:00   0:00:00\r
   Bytes :       118        33        85         0         0         0\r
   Files :         6         1         5         0         0         0\r
";
        assert!(parse_summary_table(content).is_none());
    }

    #[test]
    fn normalize_path_cases() {
        assert_eq!(normalize_path("E:\\"), "E:\\");
        assert_eq!(normalize_path("E:\\x\\"), "E:\\x");
        assert_eq!(normalize_path("\\\\h\\s\\"), "\\\\h\\s");
        assert_eq!(normalize_path("//h/s/"), "\\\\h\\s");
        assert_eq!(normalize_path("C:\\"), "C:\\");
        assert_eq!(normalize_path("C:\\a\\b\\"), "C:\\a\\b");
        assert_eq!(normalize_path("C:\\a/b/\\"), "C:\\a\\b");
        assert_eq!(normalize_path("  C:\\x  "), "C:\\x");
    }

    #[test]
    fn map_result_cases() {
        assert_eq!(map_result(true, 0), "cancelled");
        assert_eq!(map_result(true, 16), "cancelled");
        assert_eq!(map_result(false, -1), "failed"); // no code
        assert_eq!(map_result(false, 8), "failed");
        assert_eq!(map_result(false, 16), "failed");
        assert_eq!(map_result(false, 4), "warnings");
        assert_eq!(map_result(false, 7), "warnings");
        assert_eq!(map_result(false, 3), "success");
        assert_eq!(map_result(false, 1), "success");
        assert_eq!(map_result(false, 0), "success");
    }

    #[test]
    fn is_error_line_strict_pattern() {
        assert!(is_error_line(
            "2026/11/15 14:00:00 ERROR 5 (0x00000005) Accessing Source Directory"
        ));
        assert!(is_error_line("ERROR 32 (0x00000020) Accessing Destination Directory"));
        assert!(!is_error_line("C:\\Users\\ERRORUser\\file.txt"));
        assert!(!is_error_line("\t    New File  \t\t      17\tC:\\ERROR dir\\a.txt"));
        assert!(!is_error_line("some ERROR without code"));
        assert!(!is_error_line(""));
    }

    #[test]
    fn read_log_display_cap_vs_full_read() {
        let dir = tmp_dir("logcap");
        let path = dir.join("big.log");
        let mut content = String::new();
        for i in 0..60_000 {
            content.push_str(&format!("payload_{i:06}_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\n"));
        }
        let bytes: Vec<u8> = content.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        assert!(bytes.len() > 2 * 1024 * 1024, "fixture must exceed 2MB");
        fs::write(&path, &bytes).unwrap();
        let full = read_log_file_full(&path).unwrap();
        let capped = read_log_file(&path).unwrap();
        assert!(!capped.contains("payload_000000"));
        assert!(capped.contains("payload_059999"));
        assert!(capped.len() < full.len());
        assert!(full.contains("payload_000000"));
        assert!(full.contains("payload_059999"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn tail_log_split_at_every_byte_offset() {
        let content = "line1\r\nسند فارسی.txt\r\n😀emoji ok\r\nlast line no newline";
        let buf: Vec<u8> = content.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        let dir = tmp_dir("tail");
        let path = dir.join("tail_utf16.log");
        let expected = vec![
            "line1".to_string(),
            "سند فارسی.txt".to_string(),
            "😀emoji ok".to_string(),
            "last line no newline".to_string(),
        ];
        for split in 0..=buf.len() {
            let _ = fs::remove_file(&path);
            fs::write(&path, &buf[..split]).unwrap();
            let mut byte_pending: Vec<u8> = Vec::new();
            let mut text_pending = String::new();
            let (off, mut lines) = tail_log(&path, 0, &mut byte_pending, &mut text_pending, false);
            {
                use std::io::Write;
                let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
                f.write_all(&buf[split..]).unwrap();
            }
            let (_, lines2) = tail_log(&path, off, &mut byte_pending, &mut text_pending, true);
            lines.extend(lines2);
            let lines: Vec<String> = lines.into_iter().filter(|l| !l.trim().is_empty()).collect();
            assert_eq!(lines, expected, "split at byte offset {split}");
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn tail_log_final_flush_without_newline_t5_regression() {
        let dir = tmp_dir("t5flush");
        let path = dir.join("killed.log");
        let text = "\t    New File  \t\t10737418240\tC:\\x\\huge10.bin";
        let bytes: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        fs::write(&path, &bytes).unwrap();
        let mut byte_pending: Vec<u8> = Vec::new();
        let mut text_pending = String::new();
        let (_, lines) = tail_log(&path, 0, &mut byte_pending, &mut text_pending, true);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].ends_with("huge10.bin"));
        let (k, sz, _) = parse_line(&lines[0]).unwrap();
        assert_eq!(k, Kind::New);
        assert_eq!(sz, 10_737_418_240);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn register_run_twice_rejected_then_slot_freed() {
        let cell: Arc<Mutex<Option<RunHandle>>> = Arc::new(Mutex::new(None));
        let (h1, g1) = register_run(&cell, "run_1").expect("first registration");
        let err = match register_run(&cell, "run_2") {
            Err(e) => e,
            Ok(_) => panic!("second registration must fail"),
        };
        assert!(!err.is_empty());
        drop(g1);
        drop(h1);
        let (_h2, _g2) = register_run(&cell, "run_2").expect("slot freed after guard drop");
    }

    #[test]
    fn slot_guard_releases_on_early_return() {
        let cell: Arc<Mutex<Option<RunHandle>>> = Arc::new(Mutex::new(None));
        fn early_fail(cell: &Arc<Mutex<Option<RunHandle>>>) -> Result<(), String> {
            let (_h, _guard) = register_run(cell, "run_x")?;
            return Err("early failure".to_string());
        }
        let err = early_fail(&cell).unwrap_err();
        assert_eq!(err, "early failure");
        assert!(cell.lock().unwrap().is_none());
    }

    #[test]
    fn slot_guard_releases_on_panic() {
        let cell: Arc<Mutex<Option<RunHandle>>> = Arc::new(Mutex::new(None));
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let (_h, _guard) = register_run(&cell, "run_panic").unwrap();
            panic!("simulated panic");
        }));
        assert!(res.is_err(), "panic must be caught");
        assert!(cell.lock().unwrap().is_none());
    }

    #[test]
    fn mirror_list_without_summary_table_aborts() {
        let killed = "\t    New File  \t\t10737418240\tC:\\x\\huge10.bin";
        let err = ensure_mirror_list_ok(killed).unwrap_err();
        assert!(!err.is_empty());
        let t4 = "\
    Dirs :         3         3         3         0         0         1\r
   Files :        10         1         9         0         0         4\r
   Bytes :       188        17       171         0         0        61\r
";
        let table = ensure_mirror_list_ok(t4).expect("table must parse");
        assert_eq!(table.files_extras(), 4);
        assert_eq!(table.files_total(), 10);
    }
}

