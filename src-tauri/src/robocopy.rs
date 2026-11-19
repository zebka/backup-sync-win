//! Ù¾ÙˆØ´Ù‡â€ŒØ¨Ù†Ø¯ÛŒ Ùˆ Ù…Ø¯ÛŒØ±ÛŒØª ÙØ±Ø¢ÛŒÙ†Ø¯ robocopy.
//!
//! Ù†Ú©ØªÙ‡ Ú©Ù„ÛŒØ¯ÛŒ (Ø·Ø¨Ù‚ PLAN Â§6): Ø®Ø±ÙˆØ¬ÛŒ stdout Ú©Ù†Ø³ÙˆÙ„ robocopy Ù¾Ø§Ø±Ø³ Ù†Ù…ÛŒâ€ŒØ´ÙˆØ¯
//! (Ú©Ø¯Ù¾ÛŒØ¬ OEM Ù†Ø§Ù…â€ŒÙ‡Ø§ÛŒ ÙØ§Ø±Ø³ÛŒ Ø±Ø§ Ø®Ø±Ø§Ø¨ Ù…ÛŒâ€ŒÚ©Ù†Ø¯). Ø¨Ù‡ Ø¬Ø§ÛŒ Ø¢Ù† Ø¨Ø§ /UNILOG Ù„Ø§Ú¯ UTF-16LE
//! Ù…ÛŒâ€ŒØ³Ø§Ø²ÛŒÙ… Ùˆ Ù‡Ù…Ø§Ù† ÙØ§ÛŒÙ„ Ø±Ø§ Ù‡Ø± Û³Û°Û° Ù…ÛŒÙ„ÛŒâ€ŒØ«Ø§Ù†ÛŒÙ‡ tail Ù…ÛŒâ€ŒÚ©Ù†ÛŒÙ….
//!
//! Ù†Ú©ØªÙ‡ Ù…Ø³ÛŒØ±Ù‡Ø§ (Ø§Ø«Ø¨Ø§Øªâ€ŒØ´Ø¯Ù‡ Ø¯Ø± T4): Ø¨Ø§ /MT robocopy Ù…Ø³ÛŒØ±Ù‡Ø§ Ø±Ø§ **Ù…Ø·Ù„Ù‚** Ú†Ø§Ù¾
//! Ù…ÛŒâ€ŒÚ©Ù†Ø¯Ø› /FP Ø§Ø¶Ø§ÙÙ‡ Ù†Ù…ÛŒâ€ŒÚ©Ù†ÛŒÙ…. Ù…Ø³ÛŒØ± Ù†Ø³Ø¨ÛŒ Ø¨Ø§ `relative_to` Ø§Ø² Ø±ÛŒØ´Ù‡Ù” Ù…Ù†Ø¨Ø¹
//! (ÛŒØ§ Ù…Ù‚ØµØ¯ Ø¨Ø±Ø§ÛŒ *EXTRA) Ø¬Ø¯Ø§ Ù…ÛŒâ€ŒØ´ÙˆØ¯ Ùˆ Ù‡Ø±Ú¯Ø² Ù…Ø³ÛŒØ± Ù…Ø·Ù„Ù‚ Ø±Ø§ join Ù†Ù…ÛŒâ€ŒÚ©Ù†ÛŒÙ… â€”
//! `PathBuf::join` Ø¨Ø§ Ù…Ø³ÛŒØ± Ù…Ø·Ù„Ù‚ Ù¾Ø§ÛŒÙ‡ Ø±Ø§ Ø¯ÙˆØ± Ù…ÛŒâ€ŒØ±ÛŒØ²Ø¯ Ùˆ Â«Ø§Ù†ØªÙ‚Ø§Ù„ Ø±ÙˆÛŒ Ø®ÙˆØ¯Ø´Â»
//! Ù…ÛŒâ€ŒØ³Ø§Ø²Ø¯ (Ø¨Ø§Ú¯ T4).

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

/// Ù†Ø§Ù… Ù¾ÙˆØ´Ù‡ Ø³Ø·Ù„ Ø¨Ø§Ø²ÛŒØ§ÙØª Ø¯Ø± Ø­Ø§Ù„Øª Ø¢ÛŒÙ†Ù‡ â€” Ø§Ø² Ù…Ù‚Ø§ÛŒØ³Ù‡ Ú©Ù¾ÛŒ Ù…Ø³ØªØ«Ù†Ø§ Ù…ÛŒâ€ŒØ´ÙˆØ¯.
pub const DELETED_DIR: &str = "_Deleted";

// ------------------------------------------------------------------
// Ø³Ø§Ø®ØªØ§Ø±Ù‡Ø§ÛŒ Ø¯Ø§Ø¯Ù‡
// ------------------------------------------------------------------

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

/// Ø¯Ø³ØªÚ¯ÛŒØ±Ù‡ ÛŒÚ© Ø§Ø¬Ø±Ø§ÛŒ ÙØ¹Ø§Ù„ (Ø¨Ú©Ø§Ù¾ ÛŒØ§ Ù¾ÛŒØ´â€ŒÙ†Ù…Ø§ÛŒØ´) â€” Ø¨Ø±Ø§ÛŒ cancel Ùˆ kill-on-exit.
pub struct RunHandle {
    pub id: String,
    pub child: Arc<Mutex<Option<tokio::process::Child>>>,
    pub cancelled: Arc<AtomicBool>,
}

// ------------------------------------------------------------------
// Ù†Ø±Ù…Ø§Ù„â€ŒØ³Ø§Ø²ÛŒ Ù…Ø³ÛŒØ± (P0-7)
// ------------------------------------------------------------------

/// Ù†Ø±Ù…Ø§Ù„â€ŒØ³Ø§Ø²ÛŒ Ù…Ø³ÛŒØ± Ø¨Ø±Ø§ÛŒ Ø®Ø· ÙØ±Ù…Ø§Ù†:
/// - `/` Ø¨Ù‡ `\` ØªØ¨Ø¯ÛŒÙ„ Ù…ÛŒâ€ŒØ´ÙˆØ¯Ø›
/// - Ø¨Ú©â€ŒØ§Ø³Ù„Ø´ Ø§Ù†ØªÙ‡Ø§ÛŒÛŒ Ø­Ø°Ù Ù…ÛŒâ€ŒØ´ÙˆØ¯ØŒ **Ø¬Ø² Ø±ÛŒØ´Ù‡Ù” Ø¯Ø±Ø§ÛŒÙˆ** (`E:\` Ø¨Ø§ÛŒØ¯ Ù‡Ù…ÛŒÙ† Ø¨Ù…Ø§Ù†Ø¯ â€”
///   `E:` ÛŒØ¹Ù†ÛŒ Â«Ù¾ÙˆØ´Ù‡Ù” ÙØ¹Ù„ÛŒ Ø¯Ø±Ø§ÛŒÙˆ EÂ» Ùˆ robocopy Ù…Ø³ÛŒØ± Ø§Ø´ØªØ¨Ø§Ù‡ÛŒ Ù…ÛŒâ€ŒÚ¯ÛŒØ±Ø¯).
pub fn normalize_path(p: &str) -> String {
    let mut s = p.trim().replace('/', "\\");
    if s.len() == 3 && s.as_bytes()[1] == b':' && s.ends_with('\\') {
        return s; // Ø±ÛŒØ´Ù‡Ù” Ø¯Ø±Ø§ÛŒÙˆ Â«X:\Â»
    }
    while s.len() > 1 && s.ends_with('\\') {
        s.pop();
    }
    s
}

/// Ø­Ø°Ù Ø±ÛŒØ´Ù‡ Ø§Ø² Ø§Ø¨ØªØ¯Ø§ÛŒ Ù…Ø³ÛŒØ± Ù…Ø·Ù„Ù‚ Ú†Ø§Ù¾â€ŒØ´Ø¯Ù‡ Ø¯Ø± Ù„Ø§Ú¯Ø› Ø®Ø±ÙˆØ¬ÛŒ: Ù…Ø³ÛŒØ± Ù†Ø³Ø¨ÛŒ.
/// Ù…Ù‚Ø§ÛŒØ³Ù‡ Ù†Ø§Ù‡Ù…â€ŒØ­Ø³Ø§Ø³ Ø¨Ù‡ Ø¨Ø²Ø±Ú¯ÛŒ/Ú©ÙˆÚ†Ú©ÛŒ Ø­Ø±ÙˆÙ ASCIIØ› Ø§Ú¯Ø± Ù…Ø³ÛŒØ± Ø¨Ø§ Ø±ÛŒØ´Ù‡ Ø´Ø±ÙˆØ¹ Ù†Ø´ÙˆØ¯ØŒ
/// Ù‡Ù…Ø§Ù† Ù…Ø³ÛŒØ± (Ù…Ø·Ù„Ù‚) Ø¨Ø±Ú¯Ø±Ø¯Ø§Ù†Ø¯Ù‡ Ù…ÛŒâ€ŒØ´ÙˆØ¯ â€” Ù‡ÛŒÚ†â€ŒÙˆÙ‚Øª crash Ù†Ù…ÛŒâ€ŒÚ©Ù†Ø¯.
pub fn relative_to(root: &str, path: &str) -> String {
    let root_n = normalize_path(root);
    let path_n = normalize_path(path);
    if path_n.is_empty() {
        return path_n;
    }
    if bytes_eq_ignore_ascii(path_n.as_bytes(), root_n.as_bytes()) {
        return String::new(); // Ø®ÙˆØ¯Ù Ø±ÛŒØ´Ù‡
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
        // Ù…Ø±Ø² Ø¨Ø±Ø´ Ø¨Ø¹Ø¯ Ø§Ø² Ø¨Ú©â€ŒØ§Ø³Ù„Ø´ ASCII Ø§Ø³Øª â†’ Ù…Ø±Ø² Ø§Ù…Ù† Ú©Ø§Ø±Ø§Ú©ØªØ±
        path_n[pf.len()..].to_string()
    } else {
        path_n
    }
}

fn bytes_eq_ignore_ascii(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.eq_ignore_ascii_case(y))
}

// ------------------------------------------------------------------
// Ù†Ú©ØªÙ‡ Ù…Ù‡Ù… Ø¯Ø±Ø¨Ø§Ø±Ù‡ /UNILOG: Ú¯ÛŒÙˆÙ…Ù‡ Ø±Ø§ Ø¯Ø³ØªÛŒ Ø¯Ø§Ø®Ù„ Ø¢Ø±Ú¯ÙˆÙ…Ø§Ù† Ù†Ú¯Ø°Ø§Ø±!
// Ù¾Ø§Ø±Ø³Ø± Ø¯Ø§Ø®Ù„ÛŒ robocopy Ø¯Ø§Ø±Ø§ÛŒ escapeÛŒ Ú©Ù‡ std::process Ø¨Ø±Ø§ÛŒ Ú¯ÛŒÙˆÙ…Ù‡ Ù…ÛŒâ€ŒÚ©Ù†Ø¯
// (`\"`) Ù†ÛŒØ³Øª Ùˆ Ú©Ù„ Ø®Ø· Ø±Ø§ Ø®Ø±Ø§Ø¨ Ù…ÛŒâ€ŒÚ©Ù†Ø¯ (Ú©Ø¯ Ø®Ø±ÙˆØ¬ 16 Ø¨Ø¯ÙˆÙ† Ù„Ø§Ú¯).
// Ø¨Ù‡ Ø¬Ø§ÛŒØ´ Ù…Ø³ÛŒØ± Ø±Ø§ Ø®Ø§Ù… Ø¨Ø¯Ù‡Ø› Ø§Ú¯Ø± ÙØ§ØµÙ„Ù‡ Ø¯Ø§Ø´ØªÙ‡ Ø¨Ø§Ø´Ø¯ØŒ std Ø®ÙˆØ¯Ø´ Ú©Ù„
// Ø¢Ø±Ú¯ÙˆÙ…Ø§Ù† Ø±Ø§ quote Ù…ÛŒâ€ŒÚ©Ù†Ø¯ Ùˆ robocopy Ø§ÛŒÙ† ÙØ±Ù… Ø±Ø§ Ø¯Ø±Ø³Øª Ù…ÛŒâ€ŒÙÙ‡Ù…Ø¯.
// ------------------------------------------------------------------

/// Ø¢Ø±Ú¯ÙˆÙ…Ø§Ù†â€ŒÙ‡Ø§ÛŒ Ù¾Ø§ÛŒÙ‡ robocopy (PLAN Â§6). Ø¨Ø§ `list_only` Ù…ÛŒâ€ŒØ´ÙˆØ¯ Ù¾ÛŒØ´â€ŒÙ†Ù…Ø§ÛŒØ´ (/L)
/// Ùˆ Ø¨Ø§ `mirror_list` Ù¾Ø§Ø±Ø³ ØªØ´Ø®ÛŒØµ Ø­Ø°Ùâ€ŒØ´Ø¯Ù‡â€ŒÙ‡Ø§ (/MIR /L).
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
        // Ú¯Ø°Ø±Ù‡Ø§ÛŒ /L Ú†ÛŒØ²ÛŒ Ú©Ù¾ÛŒ Ù†Ù…ÛŒâ€ŒÚ©Ù†Ù†Ø¯Ø› /MT Ø®Ø·ÙˆØ· UNILOG Ø±Ø§ Ú†Ù†Ø¯Ù†Ø®ÛŒ ØªØ±Ú©ÛŒØ¨
        // Ù…ÛŒâ€ŒÚ©Ù†Ø¯ (robocopy Ø¨Ø§Ú¯ flush Ø¯Ø§Ø±Ø¯) â€” serial Ø¨Ø§Ø´Ø¯ (T4-redo).
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

    // Ø³Ø·Ù„ _Deleted Ù‡Ø±Ú¯Ø² Ù†Ø¨Ø§ÛŒØ¯ Ø¯Ø§Ø®Ù„ Ù…Ù‚Ø§ÛŒØ³Ù‡ ÛŒØ§ Ú©Ù¾ÛŒ Ø¨ÛŒØ§ÛŒØ¯
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
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW â€” Ù¾Ù†Ø¬Ø±Ù‡ Ú©Ù†Ø³ÙˆÙ„ Ø¨Ø§Ø² Ù†Ø´ÙˆØ¯
        .kill_on_drop(true); // P1-9: Ø§Ú¯Ø± ØªØ³Ú© Ù…Ø±Ø¯Ù‡ ÛŒØ§ Ù¾Ø±ØªØ§Ø¨ Ø´Ø¯ØŒ ÙØ±Ø¢ÛŒÙ†Ø¯ Ù‡Ù… Ú©Ø´ØªÙ‡ Ø´ÙˆØ¯
    cmd.spawn().map_err(|e| format!("Ø®Ø·Ø§ Ø¯Ø± Ø§Ø¬Ø±Ø§ÛŒ robocopy: {e}"))
}

fn new_run_id() -> String {
    format!(
        "{}_{}",
        chrono::Local::now().format("%Y%m%d_%H%M%S"),
        chrono::Local::now().timestamp_subsec_millis()
    )
}

// ------------------------------------------------------------------
// Ø¯ÛŒÚ©ÙˆØ¯ÛŒÙ†Ú¯ UTF-16LE Ùˆ Ø®ÙˆØ§Ù†Ø¯Ù† Ù„Ø§Ú¯
// ------------------------------------------------------------------

pub fn decode_utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|p| u16::from_le_bytes([p[0], p[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

/// Ø®ÙˆØ§Ù†Ø¯Ù† Ù„Ø§Ú¯ (UTF-16LE) Ø¨Ù‡ Ø±Ø´ØªÙ‡ â€” Ø¨Ø§ Ú©Ù¾ Û² Ù…Ú¯Ø§Ø¨Ø§ÛŒØª Ø¢Ø®Ø± (P1-10).
/// ÙÙ‚Ø· Ø¨Ø±Ø§ÛŒ **Ù†Ù…Ø§ÛŒØ´ Ø§Ù†Ø³Ø§Ù†ÛŒ** (`read_log`) Ø§Ø³ØªØ› Ø¨Ø±Ø§ÛŒ Ù¾Ø§Ø±Ø³ Ù…Ø§Ø´ÛŒÙ†ÛŒ Ø§Ø²
/// `read_log_file_full` Ø§Ø³ØªÙØ§Ø¯Ù‡ Ú©Ù†ÛŒØ¯.
pub fn read_log_file(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("Ø®ÙˆØ§Ù†Ø¯Ù† Ù„Ø§Ú¯ Ù…Ù…Ú©Ù† Ù†Ø´Ø¯: {e}"))?;
    const CAP: usize = 2 * 1024 * 1024;
    if bytes.len() > CAP {
        // Ù‡Ù…â€ŒØªØ±Ø§Ø²ÛŒ Ø²ÙˆØ¬ Ø¨Ø±Ø§ÛŒ Ø¨Ø±Ø´ ÙˆØ³Ø· ÛŒÚ© ÙˆØ§Ø­Ø¯ UTF-16
        let mut start = bytes.len() - CAP;
        start += start % 2;
        let mut text = decode_utf16(&bytes[start..]);
        // Ø§ÙˆÙ„ÛŒÙ† Ø®Ø· Ù†Ø§Ù‚ØµÙ Ø¨Ø±Ø´ Ø±Ø§ Ø¯ÙˆØ± Ø¨Ø±ÛŒØ²
        if let Some(pos) = text.find('\n') {
            text = text[pos + 1..].to_string();
        }
        Ok(format!(
            "â€¦ Ù„Ø§Ú¯ Ø·ÙˆÙ„Ø§Ù†ÛŒ Ø§Ø³ØªØ› ÙÙ‚Ø· Û² Ù…Ú¯Ø§Ø¨Ø§ÛŒØª Ø¢Ø®Ø± Ù†Ù…Ø§ÛŒØ´ Ø¯Ø§Ø¯Ù‡ Ù…ÛŒâ€ŒØ´ÙˆØ¯.\n{text}"
        ))
    } else {
        Ok(decode_utf16(&bytes))
    }
}

/// Ø®ÙˆØ§Ù†Ø¯Ù† Ú©Ø§Ù…Ù„ Ù„Ø§Ú¯ Ø¨Ø¯ÙˆÙ† cap â€” ÙÙ‚Ø· Ø¨Ø±Ø§ÛŒ Ù¾Ø§Ø±Ø³ Ù…Ø§Ø´ÛŒÙ†ÛŒ (Ø¬Ø¯ÙˆÙ„ Ø®Ù„Ø§ØµÙ‡ØŒ Ù„ÛŒØ³Øª Ø¢ÛŒÙ†Ù‡ØŒ
/// Ù¾ÛŒØ´â€ŒÙ†Ù…Ø§ÛŒØ´). Ù„Ø§Ú¯â€ŒÙ‡Ø§ÛŒ /MIR /L Ø¨Ø±Ø§ÛŒ Ø¯Ø±Ø®Øªâ€ŒÙ‡Ø§ÛŒ Ø¨Ø²Ø±Ú¯ Ø¨Ù‡ Ú†Ù†Ø¯ Ù…Ú¯Ø§Ø¨Ø§ÛŒØª Ù…ÛŒâ€ŒØ±Ø³Ù†Ø¯Ø›
/// cap Ø¯Ø± Ø§ÛŒÙ†Ø¬Ø§ Ø§Ø² Ø§ÛŒØ¬Ø§Ø¯ **Ù…Ù†ØªÙ‚Ù„â€ŒÙ†Ø´Ø¯Ù‡â€ŒÛŒ Ø¨ÛŒâ€ŒØµØ¯Ø§** Ø¬Ù„ÙˆÚ¯ÛŒØ±ÛŒ Ù…ÛŒâ€ŒÚ©Ù†Ø¯.
pub fn read_log_file_full(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("Ø®ÙˆØ§Ù†Ø¯Ù† Ù„Ø§Ú¯ Ù…Ù…Ú©Ù† Ù†Ø´Ø¯: {e}"))?;
    Ok(decode_utf16(&bytes))
}

/// Ø®ÙˆØ§Ù†Ø¯Ù† ÙÙ‚Ø· Ø¨Ø§ÛŒØªâ€ŒÙ‡Ø§ÛŒ Ø¬Ø¯ÛŒØ¯ Ø§Ø² Ø¢ÙØ³Øª Ø¨Ø§ `seek` (P1-8 â€” Ú©Ù„ ÙØ§ÛŒÙ„ Ø¯ÙˆØ¨Ø§Ø±Ù‡ Ø®ÙˆØ§Ù†Ø¯Ù‡
/// Ù†Ù…ÛŒâ€ŒØ´ÙˆØ¯). Ø¨Ø§ÛŒØª ÙØ±Ø¯ Ø§Ù†ØªÙ‡Ø§ÛŒÛŒ Ùˆ Ø³Ø§Ø²Ù†Ø¯Ù‡Ù” Ø¨Ø§Ù„Ø§ÛŒÙ Ø¬ÙØªâ€ŒÙ†Ø´Ø¯Ù‡ Ø¯Ø± `byte_pending`
/// Ø¨Ø±Ø§ÛŒ ØªÛŒÚ© Ø¨Ø¹Ø¯ Ù†Ú¯Ù‡ Ø¯Ø§Ø´ØªÙ‡ Ù…ÛŒâ€ŒØ´ÙˆØ¯ ØªØ§ Ù‡ÛŒÚ† ÙˆØ§Ø­Ø¯ UTF-16 ÙˆØ³Ø·Ù‡ Ù‚Ø·Ø¹ Ù†Ø´ÙˆØ¯.
///
/// Ø§Ú¯Ø± `flush=true` (Ù¾Ø§ÛŒØ§Ù† ÙØ±Ø¢ÛŒÙ†Ø¯)ØŒ ØªÚ©Ù‡Ù” Ù…ØªÙ†Ù Ø¨Ø¯ÙˆÙ† `\n` Ø§Ù†ØªÙ‡Ø§ÛŒÛŒ Ù‡Ù… Ø¨Ù‡â€ŒØ¹Ù†ÙˆØ§Ù†
/// Ø¢Ø®Ø±ÛŒÙ† Ø®Ø· Ø¨Ø±Ù…ÛŒâ€ŒÚ¯Ø±Ø¯Ø¯ â€” robocopy ØªØ±Ù…ÛŒÙ†ÛŒØªÙˆØ± Ø®Ø· Ø±Ø§ ØªÙ†Ø¨Ù„ flush Ù…ÛŒâ€ŒÚ©Ù†Ø¯ Ùˆ Ø¯Ø±
/// Ø§Ø¬Ø±Ø§ÛŒ Ú©Ø´ØªÙ‡â€ŒØ´Ø¯Ù‡ (T5-redo) Ø¢Ø®Ø±ÛŒÙ† Ø®Ø· Ø¨Ø¯ÙˆÙ† `\r\n` Ù…ÛŒâ€ŒÙ…Ø§Ù†Ø¯.
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

    // ÙÙ‚Ø· ØªØ¹Ø¯Ø§Ø¯ Ø²ÙˆØ¬ Ø¨Ø§ÛŒØª Ù…ØµØ±Ù Ù…ÛŒâ€ŒØ´ÙˆØ¯Ø› Ø¨Ø§ÛŒØª ÙØ±Ø¯ Ø¢Ø®Ø± Ù†Ú¯Ù‡ Ø¯Ø§Ø´ØªÙ‡ Ù…ÛŒâ€ŒØ´ÙˆØ¯
    let usable = byte_pending.len() - (byte_pending.len() % 2);
    let mut units: Vec<u16> = byte_pending[..usable]
        .chunks_exact(2)
        .map(|p| u16::from_le_bytes([p[0], p[1]]))
        .collect();
    // Ø³Ø§Ø²Ù†Ø¯Ù‡Ù” Ø¨Ø§Ù„Ø§ÛŒ Ø¬ÙØªâ€ŒÙ†Ø´Ø¯Ù‡ Ø¯Ø± Ø§Ù†ØªÙ‡Ø§ â†’ Ø¨Ø±Ø§ÛŒ ØªÛŒÚ© Ø¨Ø¹Ø¯ Ù†Ú¯Ù‡ Ø¯Ø§Ø±
    if let Some(&last) = units.last() {
        if (0xD800..=0xDBFF).contains(&last) {
            units.pop();
        }
    }
    // offset = Ø¬Ø§ÛŒÚ¯Ø§Ù‡ Ø®ÙˆØ§Ù†Ø¯Ù† Ø¨Ø¹Ø¯ÛŒØ› Ø¨Ø§ÛŒØªâ€ŒÙ‡Ø§ÛŒ Ù…ØµØ±Ùâ€ŒÙ†Ø´Ø¯Ù‡ Ø¯Ø± byte_pending Ù…ÛŒâ€ŒÙ…Ø§Ù†Ù†Ø¯
    // Ùˆ ØªÚ©Ø±Ø§Ø± Ù†Ù…ÛŒâ€ŒØ´ÙˆÙ†Ø¯.
    let consumed = offset + new.len() as u64;
    let tail = byte_pending.split_off(units.len() * 2);
    *byte_pending = tail;

    let mut text = String::new();
    // BOM ÙÙ‚Ø· Ø¯Ø± Ø§Ø¨ØªØ¯Ø§ÛŒ ÙØ§ÛŒÙ„
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

// ------------------------------------------------------------------
// Ù¾Ø§Ø±Ø³ Ø®Ø·ÙˆØ· Ù„Ø§Ú¯ (PLAN Â§6 + P0-1/P0-2)
// ------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `New File`
    New,
    /// `Newer` â€” Ù…Ù‚ØµØ¯ Ù‚Ø¯ÛŒÙ…ÛŒâ€ŒØªØ± Ø§Ø³ØªØ› Ø¨Ø¯ÙˆÙ† /XO Ú©Ù¾ÛŒ Ù…ÛŒâ€ŒØ´ÙˆØ¯
    Newer,
    /// `Older` â€” Ù…Ù‚ØµØ¯ Ø¬Ø¯ÛŒØ¯ØªØ± Ø§Ø³ØªØ› Ø¨Ø¯ÙˆÙ† /XO Ú©Ù¾ÛŒ Ù…ÛŒâ€ŒØ´ÙˆØ¯ (Ø§Ø«Ø¨Ø§Øª T5-redo/T9)
    Older,
    /// `Changed` â€” Ø§Ù†Ø¯Ø§Ø²Ù‡/Ø²Ù…Ø§Ù† Ù…ØªÙØ§ÙˆØªØ› Ú©Ù¾ÛŒ Ù…ÛŒâ€ŒØ´ÙˆØ¯ (Ø§Ø«Ø¨Ø§Øª T10)
    Changed,
    /// `Same` â€” ÛŒÚ©Ø³Ø§Ù†Ø› Ø±Ø¯
    Same,
    /// `Tweaked` â€” ÙÙ‚Ø· ÙˆÛŒÚ˜Ú¯ÛŒâ€ŒÙ‡Ø§ Ù…ØªÙØ§ÙˆØªØ› Ø±Ø¯
    Tweaked,
    /// `*EXTRA File` â€” ÙÙ‚Ø· Ø¯Ø± Ù…Ù‚ØµØ¯
    Extra,
    /// `*EXTRA Dir` â€” ÙÙ‚Ø· Ø¯Ø± Ù…Ù‚ØµØ¯ (Ù¾ÙˆØ´Ù‡)
    ExtraDir,
}

impl Kind {
    /// Ø¨Ø¯ÙˆÙ† /XO Ø§ÛŒÙ† Ø¨Ø±Ú†Ø³Ø¨â€ŒÙ‡Ø§ ÙˆØ§Ù‚Ø¹Ø§Ù‹ Ú©Ù¾ÛŒ Ù…ÛŒâ€ŒØ´ÙˆÙ†Ø¯ (P0-2).
    pub fn is_copied(self) -> bool {
        matches!(self, Kind::New | Kind::Newer | Kind::Older | Kind::Changed)
    }
}

/// ØªÙØ³ÛŒØ± ÛŒÚ© Ø®Ø· Ù„Ø§Ú¯: (Ù†ÙˆØ¹ØŒ Ø§Ù†Ø¯Ø§Ø²Ù‡ Ø¨Ù‡ Ø¨Ø§ÛŒØªØŒ Ù…Ø³ÛŒØ± Ù…Ø·Ù„Ù‚ Ú†Ø§Ù¾â€ŒØ´Ø¯Ù‡).
/// Ø®Ø·ÙˆØ· Ù†Ø§Ø´Ù†Ø§Ø®ØªÙ‡ Ø¨Ø§ None Ø±Ø¯ Ù…ÛŒâ€ŒØ´ÙˆÙ†Ø¯ Ùˆ ÙÙ‚Ø· Ø¯Ø± Ù„Ø§Ú¯ Ø®Ø§Ù… Ø¯ÛŒØ¯Ù‡ Ù…ÛŒâ€ŒØ´ÙˆÙ†Ø¯.
/// Ù…Ø³ÛŒØ± Ø¨Ø±Ú¯Ø´ØªÛŒ Ù…Ø·Ù„Ù‚ Ø§Ø³ØªØ› caller Ø¨Ø§ relative_to Ø¢Ù† Ø±Ø§ Ù†Ø³Ø¨ÛŒ Ù…ÛŒâ€ŒÚ©Ù†Ø¯.
pub fn parse_line(line: &str) -> Option<(Kind, u64, String)> {
    let s = line.trim();
    if s.is_empty() {
        return None;
    }

    // ØªØ±ØªÛŒØ¨ Ù…Ù‡Ù… Ø§Ø³Øª: Ø¨Ø±Ú†Ø³Ø¨â€ŒÙ‡Ø§ÛŒ Ú†Ù†Ø¯Ú©Ù„Ù…Ù‡â€ŒØ§ÛŒ Ø§ÙˆÙ„ Ú†Ú© Ø´ÙˆÙ†Ø¯
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
        return None; // Ø®Ø·ÙˆØ· Ù¾ÙˆØ´Ù‡ Ùˆ Ø³Ø±Ø³ØªÙˆÙ†â€ŒÙ‡Ø§ Ùˆ Ø¬Ø¯ÙˆÙ„ Ø®Ù„Ø§ØµÙ‡ â€” Ù†Ø§Ø¯ÛŒØ¯Ù‡
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

/// Ø¨Ø¹Ø¯ Ø§Ø² Ø¨Ø±Ú†Ø³Ø¨: ÛŒÚ© Ø¹Ø¯Ø¯ (Ø§Ù†Ø¯Ø§Ø²Ù‡Ø› Ø¨Ø±Ø§ÛŒ *EXTRA Dir Ù…ÛŒâ€ŒØªÙˆØ§Ù†Ø¯ Ù…Ù†ÙÛŒ Ø¨Ø§Ø´Ø¯) Ùˆ Ø¨Ø¹Ø¯
/// Ù…Ø³ÛŒØ±. Ù…Ø³ÛŒØ± Ø®ÙˆØ¯Ø´ Ù…ÛŒâ€ŒØªÙˆØ§Ù†Ø¯ ÙØ§ØµÙ„Ù‡ Ø¯Ø§Ø´ØªÙ‡ Ø¨Ø§Ø´Ø¯.
fn size_and_path(rest: &str) -> (u64, String) {
    let t = rest.trim_start();
    let end = t.find(char::is_whitespace).unwrap_or(t.len());
    let first = &t[..end];
    if let Ok(n) = first.parse::<i64>() {
        let path = t[end..].trim_start().trim_end().to_string();
        return (n.max(0) as u64, path);
    }
    // Ø¨Ø¯ÙˆÙ† Ø¹Ø¯Ø¯: Ú©Ù„ Ø¨Ù‚ÛŒÙ‡ Ù…Ø³ÛŒØ± Ø§Ø³Øª
    (0, t.trim_end().to_string())
}

/// ÙÙ‚Ø· Ø§Ù„Ú¯ÙˆÛŒ Ø¯Ù‚ÛŒÙ‚ Ø®Ø·Ø§ÛŒ robocopy: `ERROR <Ø¹Ø¯Ø¯> (0xâ€¦` (P0-3 â€” Ù†Ù‡ Ù†Ø§Ù… ÙØ§ÛŒÙ„â€ŒÙ‡Ø§ØŒ
/// Ù†Ù‡ Ø®Ø·ÙˆØ· retry). Ù…Ø¹Ø§Ø¯Ù„ Ø¯Ø³ØªÛŒÙ `\bERROR\s+\d+\s+\(0x`.
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

// ------------------------------------------------------------------
// Ø¬Ø¯ÙˆÙ„ Ø®Ù„Ø§ØµÙ‡ Ù¾Ø§ÛŒØ§Ù† Ù„Ø§Ú¯ (P0-3)
// ------------------------------------------------------------------

/// ÛŒÚ© Ø±Ø¯ÛŒÙ Ø¬Ø¯ÙˆÙ„ Ø®Ù„Ø§ØµÙ‡: Â«Ø¨Ø±Ú†Ø³Ø¨Ù ØªÚ©â€ŒÚ©Ù„Ù…Ù‡â€ŒØ§ÛŒ : Û¶ Ø¹Ø¯Ø¯ ØµØ­ÛŒØ­Â».
/// Ø±Ø¯ÛŒÙâ€ŒÙ‡Ø§ÛŒ Times/Ended/Speed Ú†ÙˆÙ† Ø§Ø¹Ø´Ø§Ø±ÛŒ ÛŒØ§ ØºÛŒØ±Ø¹Ø¯Ø¯ÛŒâ€ŒØ§Ù†Ø¯ Ø±Ø¯ Ù…ÛŒâ€ŒØ´ÙˆÙ†Ø¯Ø›
/// Ø¨Ù†Ø§Ø¨Ø±Ø§ÛŒÙ† Ù¾Ø§Ø±Ø³ Ù…Ø³ØªÙ‚Ù„ Ø§Ø² Ø²Ø¨Ø§Ù† Ø±Ø§Ø¨Ø· ÙˆÛŒÙ†Ø¯ÙˆØ² Ø§Ø³Øª (Ø³ØªÙˆÙ†â€ŒÙ…Ø­ÙˆØ±ØŒ Ù†Ù‡ Ø¨Ø±Ú†Ø³Ø¨â€ŒÙ…Ø­ÙˆØ±).
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

/// Ø¬Ø¯ÙˆÙ„ Ø®Ù„Ø§ØµÙ‡: Ø¢Ø®Ø±ÛŒÙ† Û³ Ø±Ø¯ÛŒÙ Ù…ØªÙˆØ§Ù„ÛŒ (Ø¨Ù‡ ØªØ±ØªÛŒØ¨ Dirs, Files, Bytes) Ø¨Ø§ Ø³ØªÙˆÙ†â€ŒÙ‡Ø§ÛŒ
/// Total, Copied, Skipped, Mismatch, FAILED, Extras.
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

/// Mirror fail-closed (C1): Ù„Ø§Ú¯ Ù„ÛŒØ³Øª Ø¢ÛŒÙ†Ù‡ Ø¨Ø§ÛŒØ¯ Ø¬Ø¯ÙˆÙ„ Ø®Ù„Ø§ØµÙ‡ Ø¯Ø§Ø´ØªÙ‡ Ø¨Ø§Ø´Ø¯ØŒ
/// ÙˆÚ¯Ø±Ù†Ù‡ Ø§Ù†ØªÙ‚Ø§Ù„ Ø§Ù†Ø¬Ø§Ù… Ù†Ù…ÛŒâ€ŒØ´ÙˆØ¯ â€” B7 Ø¬Ø²Ø¦ÛŒØ§ØªØŒ Ù‡ÛŒÚ† ÙØ§ÛŒÙ„ÛŒ Ø¬Ø§Ø¨Ù‡â€ŒØ¬Ø§ Ù†Ù…ÛŒâ€ŒØ´ÙˆØ¯.
pub fn ensure_mirror_list_ok(content: &str) -> Result<SummaryTable, String> {
    parse_summary_table(content).ok_or_else(|| {
        "Ù„Ø§Ú¯ Ù„ÛŒØ³Øª Ø¢ÛŒÙ†Ù‡ Ø¬Ø¯ÙˆÙ„ Ø®Ù„Ø§ØµÙ‡ Ù†Ø¯Ø§Ø±Ø¯ (Ø®Ø±ÙˆØ¬ÛŒ robocopy Ø®Ø±Ø§Ø¨ ÛŒØ§ Ù†Ø§Ù‚Øµ Ø§Ø³Øª)Ø› Ù‡ÛŒÚ† ÙØ§ÛŒÙ„ÛŒ Ø¬Ø§Ø¨Ù‡â€ŒØ¬Ø§ Ù†Ø´Ø¯.".to_string()
    })
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

// ------------------------------------------------------------------
// Ù†Ù‚Ø´Ù‡ Ú©Ø¯ Ø®Ø±ÙˆØ¬ â†’ Ù†ØªÛŒØ¬Ù‡ (P1-9)
// ------------------------------------------------------------------

/// Ù†Ù‚Ø·Ù‡ Ø®Ø§Ù„Øµ: Ø§ÙˆÙ„ Ù¾Ø±Ú†Ù… Ù„ØºÙˆØŒ Ø¨Ø¹Ø¯ Ú©Ø¯ Ù…Ù†ÙÛŒ (Ø¨Ø¯ÙˆÙ† Ú©Ø¯) = failedØŒ
/// Ø¨Ø¹Ø¯ â€Ž>=8 = failedØŒ â€Ž4..=7 = warningsØŒ â€Ž0..=3 = success.
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

// ------------------------------------------------------------------
// Ø§Ø¹ØªØ¨Ø§Ø±Ø³Ù†Ø¬ÛŒ Ù…Ø³ÛŒØ±Ù‡Ø§ Ùˆ Ø¯Ø³ØªØ±Ø³ÛŒ Ø´Ø¨Ú©Ù‡ (P0-5)
// ------------------------------------------------------------------

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

/// `check_path` Ø¨Ø§ ØªØ§ÛŒÙ…â€ŒØ§ÙˆØª (~Û¸ Ø«Ø§Ù†ÛŒÙ‡) â€” fs::metadata Ø±ÙˆÛŒ UNC Ø¯Ø± Ø¯Ø³ØªØ±Ø³â€ŒÙ†Ø§Ù¾Ø°ÛŒØ±
/// ØªØ§ Û²Û°-Û´Û° Ø«Ø§Ù†ÛŒÙ‡ Ú¯ÛŒØ± Ù…ÛŒâ€ŒÚ©Ù†Ø¯ (P0-5).
pub async fn check_path_timeout(path: String) -> CheckPathResult {
    let task = tokio::task::spawn_blocking(move || check_path(&path));
    match tokio::time::timeout(std::time::Duration::from_secs(8), task).await {
        Ok(Ok(res)) => res,
        Ok(Err(_)) => CheckPathResult {
            exists: false,
            is_dir: false,
            error: Some("Ø¨Ø±Ø±Ø³ÛŒ Ù…Ø³ÛŒØ± Ø´Ú©Ø³Øª Ø®ÙˆØ±Ø¯.".to_string()),
        },
        Err(_) => CheckPathResult {
            exists: false,
            is_dir: false,
            error: Some("Ø³Ø±ÙˆØ± Ø¯Ø± Ø¯Ø³ØªØ±Ø³ Ù†ÛŒØ³Øª.".to_string()),
        },
    }
}

/// Ø®Ø·Ø§ÛŒ Ø§Ù†Ø³Ø§Ù†ÛŒ Ø¨Ø±Ø§ÛŒ Ù…Ø³ÛŒØ± Ù…Ù†Ø¨Ø¹ ØºÛŒØ±Ù‚Ø§Ø¨Ù„ Ø¯Ø³ØªØ±Ø³ (PLAN Â§7).
fn validate_source(source: &str) -> Result<(), String> {
    match fs::metadata(source) {
        Ok(md) if md.is_dir() => Ok(()),
        Ok(_) => Err("Ù…Ø³ÛŒØ± Ù…Ù†Ø¨Ø¹ ÛŒÚ© Ù¾ÙˆØ´Ù‡ Ù†ÛŒØ³Øª.".to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Err(
            "Ø¯Ø³ØªØ±Ø³ÛŒ Ø¨Ù‡ Ù…Ù†Ø¨Ø¹ Ø±Ø¯ Ø´Ø¯. Ù†Ø§Ù… Ú©Ø§Ø±Ø¨Ø±ÛŒ/Ø±Ù…Ø² Ø´Ø¨Ú©Ù‡ Ø±Ø§ Ú†Ú© Ú©Ù† ÛŒØ§ Ø¯Ø³ØªØ±Ø³ÛŒ Ø³Ù‡Ù… Ø±Ø§.".to_string(),
        ),
        Err(_) => Err(format!(
            "Ù…Ø³ÛŒØ± Ù…Ù†Ø¨Ø¹ Ù¾ÛŒØ¯Ø§ Ù†Ø´Ø¯: {source}\nÚ†Ú© Ú©Ù† Ø³ÛŒØ³ØªÙ… Ù…Ù‚ØµØ¯ Ø±ÙˆØ´Ù† Ø¨Ø§Ø´Ø¯ Ùˆ Ù…Ø³ÛŒØ± Ø¯Ø±Ø³Øª Ø¨Ø§Ø´Ø¯."
        )),
    }
}

/// Ø¢Ù…Ø§Ø¯Ù‡â€ŒØ³Ø§Ø²ÛŒ Ù…Ù†Ø¨Ø¹ Ø¨Ù‡ ØªØ±ØªÛŒØ¨ Ø¯Ø±Ø³Øª (P0-5): Ø§ÙˆÙ„ `net use`ØŒ Ø¨Ø¹Ø¯ validate_source
/// Ø¨Ø§ ØªØ§ÛŒÙ…â€ŒØ§ÙˆØª Û³Û° Ø«Ø§Ù†ÛŒÙ‡. Ù‡Ø± Ø¯Ùˆ `preview_changes` Ùˆ `start_run` Ø§Ø² Ù‡Ù…ÛŒÙ† Ø§Ø³ØªÙØ§Ø¯Ù‡
/// Ù…ÛŒâ€ŒÚ©Ù†Ù†Ø¯ ØªØ§ ØªØ±ØªÛŒØ¨ Ù‡Ø±Ú¯Ø² ÙˆØ§Ú˜Ú¯ÙˆÙ† Ù†Ø´ÙˆØ¯.
pub async fn prepare_source(
    user: Option<&str>,
    password: Option<&str>,
    source: &str,
) -> Result<(), String> {
    let src = normalize_path(source);
    if let (Some(user), Some(pass)) = (user, password) {
        if !user.is_empty() && !pass.is_empty() {
            // Ø¨Ø§Ø±Ú¯Ø°Ø§Ø±ÛŒ host Ù…Ù…Ú©Ù† Ø§Ø³Øª ~Û´Û° Ø«Ø§Ù†ÛŒÙ‡ Ú¯ÛŒØ± Ú©Ù†Ø¯Ø› Ø¨Ø§ Ù‡Ù…Ø§Ù† ØªØ§ÛŒÙ…â€ŒØ§ÙˆØª Û³Û° Ø«Ø§Ù†ÛŒÙ‡ Ù…Ø­Ø¯ÙˆØ¯ Ù…ÛŒâ€ŒØ´ÙˆØ¯
            match tokio::time::timeout(std::time::Duration::from_secs(30), net_use(user, pass, &src)).await {
                Ok(res) => res?,
                Err(_) => return Err("Ø³Ø±ÙˆØ± Ø¯Ø± Ø¯Ø³ØªØ±Ø³ Ù†ÛŒØ³Øª.".to_string()),
            }
        }
    }
    let src2 = src;
    let check = tokio::task::spawn_blocking(move || validate_source(&src2));
    match tokio::time::timeout(std::time::Duration::from_secs(30), check).await {
        Ok(Ok(res)) => res,
        Ok(Err(_)) => Err("Ø¨Ø±Ø±Ø³ÛŒ Ù…Ù†Ø¨Ø¹ Ø´Ú©Ø³Øª Ø®ÙˆØ±Ø¯.".to_string()),
        Err(_) => Err("Ø³Ø±ÙˆØ± Ø¯Ø± Ø¯Ø³ØªØ±Ø³ Ù†ÛŒØ³Øª.".to_string()),
    }
}

/// Ø§ØªØµØ§Ù„ Ø¨Ù‡ Ø§Ø´ØªØ±Ø§Ú© Ø´Ø¨Ú©Ù‡ Ø¨Ø§ `net use` Ù…ÙˆÙ‚Øª. Ø±Ù…Ø² ÙÙ‚Ø· Ø¯Ø± Ø®Ø· ÙØ±Ù…Ø§Ù† Ù‡Ù…ÛŒÙ† ÙØ±Ø§Ø®ÙˆØ§Ù†ÛŒ
/// Ø§Ø³ØªÙØ§Ø¯Ù‡ Ù…ÛŒâ€ŒØ´ÙˆØ¯ Ùˆ Ø¯Ø± Ù‡ÛŒÚ† ÙØ§ÛŒÙ„ ÛŒØ§ Ù„Ø§Ú¯ÛŒ Ù†Ù…ÛŒâ€ŒÙ†Ø´ÛŒÙ†Ø¯ (PLAN Â§7). Ø®Ø±ÙˆØ¬ÛŒ net use Ù‡Ù…
/// Ù‡Ø±Ú¯Ø² Ù†Ù…Ø§ÛŒØ´ Ø¯Ø§Ø¯Ù‡ Ù†Ù…ÛŒâ€ŒØ´ÙˆØ¯ Ú†ÙˆÙ† Ù…Ù…Ú©Ù† Ø§Ø³Øª Ø­Ø§ÙˆÛŒ Ø±Ù…Ø² Ø¨Ø§Ø´Ø¯.
async fn net_use(user: &str, password: &str, source: &str) -> Result<(), String> {
    // normalize_path Ù‚Ø¨Ù„Ø§Ù‹ `/` Ø±Ø§ Ø¨Ù‡ `\` ØªØ¨Ø¯ÛŒÙ„ Ú©Ø±Ø¯Ù‡ (P0-5)Ø›
    // \\host\share\... â†’ \\host\IPC$
    let trimmed = source.trim_start_matches("\\\\");
    let host = trimmed.split('\\').next().unwrap_or_default();
    if host.is_empty() {
        return Ok(()); // Ù…Ù†Ø¨Ø¹ Ù…Ø­Ù„ÛŒ
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
        .kill_on_drop(true) // Ø§Ú¯Ø± ØªØ§ÛŒÙ…â€ŒØ§ÙˆØª prepare_source ÙØ±Ø§Ø¨Ø±Ø³Ø¯ØŒ net use Ù‡Ù… Ú©Ø´ØªÙ‡ Ù…ÛŒâ€ŒØ´ÙˆØ¯
        .output()
        .await
        .map_err(|e| format!("Ø§Ø¬Ø±Ø§ÛŒ net use Ø´Ú©Ø³Øª Ø®ÙˆØ±Ø¯: {e}"))?;

    if !output.status.success() {
        // ÙÙ‚Ø· Ø¨Ø±Ø§ÛŒ ØªØ´Ø®ÛŒØµ 1219 Ù†Ú¯Ø§Ù‡ Ù…ÛŒâ€ŒÚ©Ù†ÛŒÙ…Ø› Ù…ØªÙ† Ù‡Ø±Ú¯Ø² Ø¨Ø±Ú¯Ø±Ø¯Ø§Ù†Ø¯Ù‡ Ù†Ù…ÛŒâ€ŒØ´ÙˆØ¯
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if text.contains("1219") {
            // Ø§ØªØµØ§Ù„ Ù…ÙˆØ¬ÙˆØ¯ Ø¨Ø§ Ø§Ø¹ØªØ¨Ø§Ø±Ù†Ø§Ù…Ù‡Ù” Ø¯ÛŒÚ¯Ø± â€” Ø­Ø°Ù Ø®ÙˆØ¯Ú©Ø§Ø± Ù…Ù…Ù†ÙˆØ¹ (P0-5)
            return Err(format!(
                "ÛŒÚ© Ø§ØªØµØ§Ù„ ÙØ¹Ø§Ù„ Ø¯ÛŒÚ¯Ø± Ø¨Ù‡ Ø³Ø±ÙˆØ± {host} Ø¨Ø§ Ø§Ø¹ØªØ¨Ø§Ø±Ù†Ø§Ù…Ù‡Ù” Ù…ØªÙØ§ÙˆØª ÙˆØ¬ÙˆØ¯ Ø¯Ø§Ø±Ø¯.\n\
                 Ø§ÙˆÙ„ Ø¢Ù† Ø±Ø§ Ù‚Ø·Ø¹ Ú©Ù† (Ø¯Ø± cmd):\nnet use \\\\{host}\\IPC$ /delete"
            ));
        }
        return Err(format!(
            "Ø§ØªØµØ§Ù„ Ø¨Ù‡ Ø³Ø±ÙˆØ± {host} Ø¨Ø§ Ú©Ø§Ø±Ø¨Ø± {user} Ù†Ø§Ù…ÙˆÙÙ‚ Ø¨ÙˆØ¯. Ù†Ø§Ù… Ú©Ø§Ø±Ø¨Ø±ÛŒ/Ø±Ù…Ø² Ø±Ø§ Ú†Ú© Ú©Ù†."
        ));
    }
    Ok(())
}

/// Ø§Ú¯Ø± Ù…Ù†Ø¨Ø¹ Ùˆ Ù…Ù‚ØµØ¯ Ø¯Ø§Ø®Ù„ Ù‡Ù… Ø¨ÙˆØ¯Ù†Ø¯ â†’ Ø±ÛŒØ³Ú© Ú©Ù¾ÛŒ Ø¨ÛŒâ€ŒÙ†Ù‡Ø§ÛŒØªØŒ Ø±Ø¯ Ø´Ùˆ (PLAN Â§10).
fn validate_no_nesting(source: &str, destination: &str) -> Result<(), String> {
    let s = normalize_path(source).to_lowercase();
    let d = normalize_path(destination).to_lowercase();
    if s == d {
        return Err("Ù…Ù†Ø¨Ø¹ Ùˆ Ù…Ù‚ØµØ¯ Ù†Ø¨Ø§ÛŒØ¯ ÛŒÚ©ÛŒ Ø¨Ø§Ø´Ù†Ø¯.".to_string());
    }
    if d.starts_with(&format!("{s}\\")) || d.starts_with(&format!("{s}/")) {
        return Err("Ù…Ù‚ØµØ¯ Ù†Ø¨Ø§ÛŒØ¯ Ø¯Ø§Ø®Ù„ Ù…Ù†Ø¨Ø¹ Ø¨Ø§Ø´Ø¯ (Ø±ÛŒØ³Ú© Ú©Ù¾ÛŒ Ø¨ÛŒâ€ŒÙ†Ù‡Ø§ÛŒØª).".to_string());
    }
    if s.starts_with(&format!("{d}\\")) || s.starts_with(&format!("{d}/")) {
        return Err("Ù…Ù†Ø¨Ø¹ Ù†Ø¨Ø§ÛŒØ¯ Ø¯Ø§Ø®Ù„ Ù…Ù‚ØµØ¯ Ø¨Ø§Ø´Ø¯.".to_string());
    }
    Ok(())
}

// ------------------------------------------------------------------
// Ø¬Ø§ÛŒÚ¯Ø§Ù‡ Â«Ø¯Ø± Ø­Ø§Ù„ Ø§Ø¬Ø±Ø§Â» (P0-4)
// ------------------------------------------------------------------

/// Ù†Ú¯Ù‡â€ŒØ¯Ø§Ø±Ù†Ø¯Ù‡Ù” RAII Ø¬Ø§ÛŒÚ¯Ø§Ù‡ Â«Ø¯Ø± Ø­Ø§Ù„ Ø§Ø¬Ø±Ø§Â» â€” Ø¯Ø± Ù‡Ø± Ø®Ø±ÙˆØ¬ÛŒ (Ø®Ø·Ø§ØŒ Ù¾Ø§Ù†ÛŒÚ©ØŒ Ø¯Ø±Ø§Ù¾Ù
/// ØªØ³Ú©) Ø¬Ø§ÛŒÚ¯Ø§Ù‡ Ø¢Ø²Ø§Ø¯ Ù…ÛŒâ€ŒØ´ÙˆØ¯. `disarm` Ù…Ø³Ø¦ÙˆÙ„ÛŒØª Ø±Ø§ Ø¨Ù‡ Ù…ÙˆÙ†ÛŒØªÙˆØ± Ù…ÛŒâ€ŒØ³Ù¾Ø§Ø±Ø¯.
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

/// Ø«Ø¨Øª Ø¯Ø³ØªÚ¯ÛŒØ±Ù‡ Ø¯Ø± Ø¬Ø§ÛŒÚ¯Ø§Ù‡ Â«Ø¯Ø± Ø­Ø§Ù„ Ø§Ø¬Ø±Ø§Â» â€” Ù‚Ø¨Ù„ Ø§Ø² Ù‡Ø± await ØµØ¯Ø§ Ø²Ø¯Ù‡ Ù…ÛŒâ€ŒØ´ÙˆØ¯.
/// Ø§Ú¯Ø± Ø¬Ø§ÛŒÚ¯Ø§Ù‡ Ø§Ø´ØºØ§Ù„ Ø¨Ø§Ø´Ø¯ØŒ Ø®Ø·Ø§ Ø¨Ø±Ù…ÛŒâ€ŒÚ¯Ø±Ø¯Ø§Ù†Ø¯ (Ø±Ø¯ Ø´Ø¯Ù† Ø´Ø±ÙˆØ¹ Ø¯ÙˆÙ… â€” P0-4).
pub fn register_run(
    cell: &Arc<Mutex<Option<RunHandle>>>,
    id: &str,
) -> Result<(RunHandle, SlotGuard), String> {
    let mut guard = cell.lock().unwrap();
    if guard.is_some() {
        return Err(
            "ÛŒÚ© Ø¨Ú©Ø§Ù¾ ÛŒØ§ Ù¾ÛŒØ´â€ŒÙ†Ù…Ø§ÛŒØ´ Ø¯Ø± Ø­Ø§Ù„ Ø§Ø¬Ø±Ø§Ø³ØªØ› Ø§ÙˆÙ„ Ø¢Ù† Ø±Ø§ Ù„ØºÙˆ Ú©Ù† ÛŒØ§ ØµØ¨Ø± Ú©Ù† ØªÙ…Ø§Ù… Ø´ÙˆØ¯.".to_string(),
        );
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

/// Ø§Ù†ØªØ¸Ø§Ø± Ø¨Ø±Ø§ÛŒ Ú©ÙˆØ¯Ú©ÛŒ Ú©Ù‡ Ø¯Ø± `child_cell` Ø«Ø¨Øª Ø§Ø³ØªØ› Ù¾Ø±Ú†Ù… Ù„ØºÙˆ Ø¨ÛŒÙ† Ø­Ù„Ù‚Ù‡â€ŒÙ‡Ø§ Ú†Ú©
/// Ù…ÛŒâ€ŒØ´ÙˆØ¯ Ùˆ Ø¯Ø± ØµÙˆØ±Øª Ù„ØºÙˆ Ú©ÙˆØ¯Ú© Ú©Ø´ØªÙ‡ Ù…ÛŒâ€ŒØ´ÙˆØ¯ (P0-4: Ù„ØºÙˆ Ø¯Ø± Ù‡Ù…Ù‡Ù” ÙØ§Ø²Ù‡Ø§).
/// MutexGuard Ù‡Ø±Ú¯Ø² Ø±ÙˆÛŒ await Ù†Ú¯Ù‡ Ø¯Ø§Ø´ØªÙ‡ Ù†Ù…ÛŒâ€ŒØ´ÙˆØ¯.
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

// ------------------------------------------------------------------
// Ù¾ÛŒØ´â€ŒÙ†Ù…Ø§ÛŒØ´ ØªØºÛŒÛŒØ±Ø§Øª (/L)
// ------------------------------------------------------------------

/// Ø§Ø¬Ø±Ø§ÛŒ Ù¾ÛŒØ´â€ŒÙ†Ù…Ø§ÛŒØ´. Ø¬Ø§ÛŒÚ¯Ø§Ù‡ Â«Ø¯Ø± Ø­Ø§Ù„ Ø§Ø¬Ø±Ø§Â» Ù‚Ø¨Ù„Ø§Ù‹ ØªÙˆØ³Ø· caller Ú¯Ø±ÙØªÙ‡ Ø´Ø¯Ù‡ Ùˆ Ù„Ø§Ú¯
/// Ù…ÙˆÙ‚ØªÛŒÙ ÛŒÚ©ØªØ§ Ø¯Ø§Ø±Ø¯ (P0-4). Ù¾Ø³ Ø§Ø² Ù¾Ø§ÛŒØ§Ù†ØŒ Ù„Ø§Ú¯ Ù…ÙˆÙ‚ØªÛŒ Ø­Ø°Ù Ù…ÛŒâ€ŒØ´ÙˆØ¯.
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
        true,  // /L â€” Ù‡ÛŒÚ† Ú†ÛŒØ² Ø±ÙˆÛŒ Ø¯ÛŒØ³Ú© ØªØºÛŒÛŒØ± Ù†Ù…ÛŒâ€ŒÚ©Ù†Ø¯
        true,  // /MIR â€” ÙÙ‚Ø· Ø¨Ø±Ø§ÛŒ Ø§ÛŒÙ† Ú©Ù‡ *EXTRAÙ‡Ø§ Ù‡Ù… Ù„ÛŒØ³Øª Ø´ÙˆÙ†Ø¯
    );
    let child = spawn_robocopy(&args)?;
    *handle.child.lock().unwrap() = Some(child);
    let status = wait_child_in_cell(&handle.child, &handle.cancelled)
        .await
        .ok_or_else(|| "Ù¾ÛŒØ´â€ŒÙ†Ù…Ø§ÛŒØ´ Ù†Ø§ØªÙ…Ø§Ù… Ù…Ø§Ù†Ø¯.".to_string())?;
    if handle.cancelled.load(Ordering::SeqCst) {
        let _ = fs::remove_file(&tmp_log);
        return Err("Ù¾ÛŒØ´â€ŒÙ†Ù…Ø§ÛŒØ´ Ù„ØºÙˆ Ø´Ø¯.".to_string());
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
        return Err(format!("Ù¾ÛŒØ´â€ŒÙ†Ù…Ø§ÛŒØ´ Ø¨Ø§ Ø®Ø·Ø§ Ù…ØªÙˆÙ‚Ù Ø´Ø¯ (Ú©Ø¯ {code}):\n{tail}"));
    }

    let content = read_log_file_full(tmp_log)?;
    let mut new_files = Vec::new();
    let mut changed = Vec::new();
    let mut deleted = Vec::new();
    let mut total_bytes = 0u64;

    for line in content.lines() {
        match parse_line(line) {
            Some((k, size, abs)) if k.is_copied() => {
                // P0-2: Newer/Older/Changed Ù‡Ù… Ú©Ù¾ÛŒ Ù…ÛŒâ€ŒØ´ÙˆÙ†Ø¯ (Ø¨Ø¯ÙˆÙ† /XO)
                total_bytes += size;
                let path = relative_to(src, &abs);
                let entry = FileEntry { path, size };
                match k {
                    Kind::New => new_files.push(entry),
                    _ => changed.push(entry),
                }
            }
            Some((Kind::Extra, size, abs)) => {
                // Ù…Ø³ÛŒØ± Ù†Ø³Ø¨ÛŒ Ù†Ø³Ø¨Øª Ø¨Ù‡ Ù…Ù‚ØµØ¯ (P0-1)
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

// ------------------------------------------------------------------
// Ø§Ø¬Ø±Ø§ÛŒ ÙˆØ§Ù‚Ø¹ÛŒ Ø¨Ú©Ø§Ù¾
// ------------------------------------------------------------------

#[derive(Default)]
struct Counts {
    copied: u64,
    skipped: u64,
    errors: u64,
    bytes: u64,
}

/// Ø´Ø±ÙˆØ¹ Ø¨Ú©Ø§Ù¾. Ø§Ú¯Ø± Ø­Ø§Ù„Øª mirror_recycle Ø¨Ø§Ø´Ø¯ Ø§ÙˆÙ„ ÙØ§ÛŒÙ„â€ŒÙ‡Ø§ÛŒ Ø§Ø¶Ø§ÙÙ‡ Ù…Ù‚ØµØ¯
/// Ø¨Ù‡ _Deleted Ù…Ù†ØªÙ‚Ù„ Ù…ÛŒâ€ŒØ´ÙˆÙ†Ø¯ØŒ Ø¨Ø¹Ø¯ Ú©Ù¾ÛŒ ÙˆØ§Ù‚Ø¹ÛŒ Ø§Ø¬Ø±Ø§ Ù…ÛŒâ€ŒØ´ÙˆØ¯.
///
/// ØªØ±ØªÛŒØ¨ ÙØ§Ø²Ù‡Ø§ (P0-4): Ø«Ø¨Øª Ø¬Ø§ÛŒÚ¯Ø§Ù‡ **Ù‚Ø¨Ù„ Ø§Ø² Ù‡Ø± await**ØŒ Ø¨Ø¹Ø¯ Ø¢Ù…Ø§Ø¯Ù‡â€ŒØ³Ø§Ø²ÛŒ
/// (net use â†’ validate â†’ Ù„ÛŒØ³Øª Ø¢ÛŒÙ†Ù‡ â†’ Ø§Ù†ØªÙ‚Ø§Ù„)ØŒ Ø¨Ø¹Ø¯ Ú©Ù¾ÛŒ.
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
    // Ú†Ú© Ù‡Ù…Ú¯Ø§Ù… Ùˆ Ø³Ø±ÛŒØ¹ â€” Ù‚Ø¨Ù„ Ø§Ø² Ø«Ø¨Øª Ø¬Ø§ÛŒÚ¯Ø§Ù‡
    validate_no_nesting(&src, &dst)?;

    let run_id = new_run_id();
    // Ø«Ø¨Øª Ù‚Ø¨Ù„ Ø§Ø² Ù‡Ø± await (P0-4) â€” Ø´Ø±ÙˆØ¹ Ø¯ÙˆÙ… Ù‡Ù…ÛŒÙ†â€ŒØ¬Ø§ Ø±Ø¯ Ù…ÛŒâ€ŒØ´ÙˆØ¯
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

    // ---- ÙØ§Ø² Ø¢Ù…Ø§Ø¯Ù‡â€ŒØ³Ø§Ø²ÛŒ: net use â†’ validate (P0-5ØŒ ØªØ±ØªÛŒØ¨ Ø¯Ø±Ø³Øª) ----
    if cancelled.load(Ordering::SeqCst) {
        return abort_cancelled(&app, &data_root, &profile, &run_id, started_at, 0);
    }
    if let Err(e) = prepare_source(profile.network_user.as_deref(), password.as_deref(), &src).await {
        // Ø§Ú¯Ø± Ú©Ø§Ø±Ø¨Ø± Ù‡Ù†Ú¯Ø§Ù… Ø¢Ù…Ø§Ø¯Ù‡â€ŒØ³Ø§Ø²ÛŒ Ù„ØºÙˆ Ú©Ø±Ø¯Ù‡ Ø¨Ø§Ø´Ø¯ØŒ Ù†ØªÛŒØ¬Ù‡ Â«Ù„ØºÙˆ Ø´Ø¯Ù‡Â» Ø¯Ø± ØªØ§Ø±ÛŒØ®Ú†Ù‡ Ø«Ø¨Øª Ù…ÛŒâ€ŒØ´ÙˆØ¯ (P0-4)
        if cancelled.load(Ordering::SeqCst) {
            return abort_cancelled(&app, &data_root, &profile, &run_id, started_at, 0);
        }
        return Err(e);
    }
    if cancelled.load(Ordering::SeqCst) {
        return abort_cancelled(&app, &data_root, &profile, &run_id, started_at, 0);
    }

    // ---- ÙØ§Ø² Ø¢ÛŒÙ†Ù‡: Ù„ÛŒØ³Øª Ø­Ø°Ùâ€ŒØ´Ø¯Ù‡â€ŒÙ‡Ø§ + Ø§Ù†ØªÙ‚Ø§Ù„ Ø¨Ù‡ _Deleted (PLAN Â§5, P0-6) ----
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

    // ---- Ø§Ø¬Ø±Ø§ÛŒ ÙˆØ§Ù‚Ø¹ÛŒ ----
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

    // Ù…Ø³Ø¦ÙˆÙ„ÛŒØª Ø¢Ø²Ø§Ø¯Ø³Ø§Ø²ÛŒ Ø¬Ø§ÛŒÚ¯Ø§Ù‡ Ø¨Ù‡ Ù…ÙˆÙ†ÛŒØªÙˆØ± Ø³Ù¾Ø±Ø¯Ù‡ Ù…ÛŒâ€ŒØ´ÙˆØ¯ (Ø­ØªÛŒ Ø¯Ø± Ù¾Ø§Ù†ÛŒÚ©)
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

/// Ø«Ø¨Øª Â«Ù„ØºÙˆ Ø´Ø¯Ù‡ Ø¯Ø± Ø­ÛŒÙ† Ø¢Ù…Ø§Ø¯Ù‡â€ŒØ³Ø§Ø²ÛŒÂ» Ø¯Ø± ØªØ§Ø±ÛŒØ®Ú†Ù‡ (P0-4) Ùˆ Ø¢Ø²Ø§Ø¯Ø³Ø§Ø²ÛŒ Ø¬Ø§ÛŒÚ¯Ø§Ù‡.
#[allow(clippy::too_many_arguments)]
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
    Err("Ø¨Ú©Ø§Ù¾ Ù„ØºÙˆ Ø´Ø¯ (Ø­ÛŒÙ† Ø¢Ù…Ø§Ø¯Ù‡â€ŒØ³Ø§Ø²ÛŒ).".to_string())
}

/// ÛŒÚ© Ú¯Ø°Ø± /MIR /L Ø¨Ø±Ø§ÛŒ Ù¾ÛŒØ¯Ø§ Ú©Ø±Ø¯Ù† ÙØ§ÛŒÙ„â€ŒÙ‡Ø§ÛŒ Ù…ÙˆØ¬ÙˆØ¯ Ø¯Ø± Ù…Ù‚ØµØ¯ ÙˆÙ„ÛŒ Ù†Ù‡ Ø¯Ø± Ù…Ù†Ø¨Ø¹ØŒ
/// Ø³Ù¾Ø³ Ø§Ù†ØªÙ‚Ø§Ù„ Ø¢Ù†Ù‡Ø§ Ø¨Ù‡ _Deleted. Ù‡ÛŒÚ† /PURGE ÙˆØ§Ù‚Ø¹ÛŒ Ø§Ø¬Ø±Ø§ Ù†Ù…ÛŒâ€ŒØ´ÙˆØ¯.
///
/// P0-6: Ù†Ú¯Ù‡Ø¨Ø§Ù† Ø­Ø°Ù Ø¯Ø³ØªÙ‡â€ŒØ¬Ù…Ø¹ÛŒØŒ moved.txtØŒ Ø­Ø°Ù Ù¾ÙˆØ´Ù‡â€ŒÙ‡Ø§ÛŒ Ø®Ø§Ù„ÛŒ Ø¹Ù…ÛŒÙ‚â€ŒØªØ±ÛŒÙ† Ø§ÙˆÙ„ØŒ
/// Ù„Ø§Ú¯ Ù„ÛŒØ³Øª Ø¯Ø± logs\<run_id>.list.log Ùˆ Ø­Ø°Ù Ø¢Ù† Ø¨Ø¹Ø¯ Ø§Ø² Ú©Ø§Ø±.
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
        .ok_or_else(|| "Ù„ÛŒØ³Øª Ø¢ÛŒÙ†Ù‡ Ù†Ø§ØªÙ…Ø§Ù… Ù…Ø§Ù†Ø¯.".to_string())?;
    if handle.cancelled.load(Ordering::SeqCst) {
        return Err("Ù„ØºÙˆ Ø´Ø¯.".to_string());
    }
    if status.code().unwrap_or(-1) >= 8 {
        return Err("ØªØ´Ø®ÛŒØµ Ø­Ø°Ùâ€ŒØ´Ø¯Ù‡â€ŒÙ‡Ø§ Ø´Ú©Ø³Øª Ø®ÙˆØ±Ø¯Ø› Ø¨Ú©Ø§Ù¾ Ø§Ø¬Ø±Ø§ Ù†Ø´Ø¯.".to_string());
    }

    let content = read_log_file_full(list_log)?;

    // C1: Ø¬Ø¯ÙˆÙ„ Ø®Ù„Ø§ØµÙ‡ Ù¾ÛŒØ¯Ø§ Ù†Ø´Ø¯ â†’ Ù„ØºÙˆ Ø­Ø±Ú©ØªØŒ Ù‡ÛŒÚ† ÙØ§ÛŒÙ„ÛŒ Ø¬Ø§Ø¨Ù‡â€ŒØ¬Ø§ Ù†Ù…ÛŒâ€ŒØ´ÙˆØ¯
    let table = ensure_mirror_list_ok(&content)?;

    // Ù†Ú¯Ù‡Ø¨Ø§Ù† Ø­Ø°Ù Ø¯Ø³ØªÙ‡â€ŒØ¬Ù…Ø¹ÛŒ (P0-6b): Ø§Ø² Ø±Ø¯ÛŒÙ Files Ø¬Ø¯ÙˆÙ„ Ø®Ù„Ø§ØµÙ‡
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
        return Ok(0);
    }

    let stamp = chrono::Local::now().format("%Y-%m-%d_%H%M%S");
    let recycle_root = PathBuf::from(dst).join(DELETED_DIR).join(stamp.to_string());
    let dst_root = PathBuf::from(dst);

    let mut moved: u64 = 0;
    let mut moved_lines: Vec<String> = Vec::new();
    let mut failed_lines: Vec<String> = Vec::new();

    for rel in &extra_files {
        // Ù„ØºÙˆ Ø¨ÛŒÙ† ÙØ§ÛŒÙ„â€ŒÙ‡Ø§ (P0-4)
        if handle.cancelled.load(Ordering::SeqCst) {
            break;
        }
        // Ø§ÛŒÙ…Ù†ÛŒ Ø§Ù†ØªÙ‚Ø§Ù„ (Ø¨Ø§Ú¯ T4): Ù‡Ø±Ú¯Ø² Ù…Ø³ÛŒØ± Ù…Ø·Ù„Ù‚ ÛŒØ§ Â«..Â» Ø±Ø§ join Ù†Ú©Ù†
        let rel_norm = rel.replace('/', "\\");
        if rel_norm.is_empty()
            || rel_norm.starts_with('\\')
            || rel_norm.contains("..")
            || Path::new(&rel_norm).is_absolute()
        {
            failed_lines.push(format!("{rel}\tÙ…Ø³ÛŒØ± Ù†Ø§Ù…Ø¹ØªØ¨Ø±"));
            continue;
        }
        let from = dst_root.join(&rel_norm);
        let to = recycle_root.join(&rel_norm);
        if from == to || !from.starts_with(&dst_root) || !to.starts_with(&recycle_root) {
            failed_lines.push(format!("{rel}\tÙ…Ø³ÛŒØ± Ù…Ù‚ØµØ¯ _Deleted Ù†Ø§Ù…Ø¹ØªØ¨Ø±"));
            continue;
        }
        if !from.exists() {
            failed_lines.push(format!("{rel}\tÙØ§ÛŒÙ„ Ø¯Ø± Ù…Ù‚ØµØ¯ Ù¾ÛŒØ¯Ø§ Ù†Ø´Ø¯"));
            continue;
        }

        let _ = fs::create_dir_all(to.parent().unwrap_or(&recycle_root));
        // Ø§Ø¨ØªØ¯Ø§ rename (Ø³Ø±ÛŒØ¹ØŒ Ù‡Ù…Ø§Ù† ÙˆÙ„ÙˆÙ…)Ø› Ø§Ú¯Ø± Ù†Ø´Ø¯ Ú©Ù¾ÛŒ + Ø­Ø°Ù
        let ok = match fs::rename(&from, &to) {
            Ok(_) => true,
            Err(_) => match fs::copy(&from, &to) {
                Ok(_) => fs::remove_file(&from).is_ok(),
                Err(_) => false,
            },
        };
        // Ø´Ù…Ø§Ø±Ø´ ÙÙ‚Ø· Ø¨Ø¹Ø¯ Ø§Ø² ØªØ£ÛŒÛŒØ¯ ÙˆØ§Ù‚Ø¹ÛŒ: Ù…Ù‚ØµØ¯ Ù‡Ø³Øª Ùˆ Ù…Ø¨Ø¯Ø£ Ù†ÛŒØ³Øª
        let verified = to.exists() && !from.exists();
        if ok && verified {
            moved += 1;
            moved_lines.push(rel.clone());
            let _ = app.emit(
                "backup://log",
                json!({ "runId": run_id, "lines": [format!("â™» Ù…Ù†ØªÙ‚Ù„ Ø´Ø¯ Ø¨Ù‡ _Deleted: {rel}")] }),
            );
        } else {
            failed_lines.push(format!("{rel}\tØ§Ù†ØªÙ‚Ø§Ù„ Ù†Ø§Ù…ÙˆÙÙ‚"));
            let _ = app.emit(
                "backup://log",
                json!({ "runId": run_id, "lines": [format!("âš  Ø§Ù†ØªÙ‚Ø§Ù„ Ø¨Ù‡ _Deleted Ù†Ø§Ù…ÙˆÙÙ‚ Ø¨ÙˆØ¯: {rel}")] }),
            );
        }
    }

    // Ø³Ø§Ø¨Ù‚Ù‡Ù” Ú†Ù‡ Ú†ÛŒØ²Ù‡Ø§ÛŒÛŒ Ù…Ù†ØªÙ‚Ù„ Ø´Ø¯ (P0-6c)
    let mut moved_txt = String::new();
    moved_txt.push_str("# Ù…Ù†ØªÙ‚Ù„â€ŒØ´Ø¯Ù‡ Ø¨Ù‡ _Deleted\r\n");
    for l in &moved_lines {
        moved_txt.push_str(l);
        moved_txt.push_str("\r\n");
    }
    if !failed_lines.is_empty() {
        moved_txt.push_str("# Ù†Ø§Ù…ÙˆÙÙ‚\r\n");
        for l in &failed_lines {
            moved_txt.push_str(l);
            moved_txt.push_str("\r\n");
        }
    }
    let _ = fs::write(moved_log, moved_txt);
    // cancel mid-move: return Ok(moved) so caller records real count in history

    // Ø­Ø°Ù Ù¾ÙˆØ´Ù‡â€ŒÙ‡Ø§ÛŒ Ø®Ø§Ù„ÛŒØŒ Ø¹Ù…ÛŒÙ‚â€ŒØªØ±ÛŒÙ† Ø§ÙˆÙ„ (P0-6d) â€” ÙÙ‚Ø· fs::remove_dir
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
            let _ = fs::remove_dir(&p); // ÙÙ‚Ø· Ø§Ú¯Ø± ÙˆØ§Ù‚Ø¹Ø§Ù‹ Ø®Ø§Ù„ÛŒ Ø¨Ø§Ø´Ø¯
        }
    }

    let _ = fs::remove_file(list_log); // P0-6e
    Ok(moved)
}

/// Ù¾Ø±Ø¯Ø§Ø²Ø´ ÛŒÚ© Ø¯Ø³ØªÙ‡ Ø®Ø· Ù„Ø§Ú¯: Ø´Ù…Ø§Ø±Ù†Ø¯Ù‡â€ŒÙ‡Ø§ + Ø¬Ù…Ø¹â€ŒØ¢ÙˆØ±ÛŒ Ø®Ø·ÙˆØ· Ø¨Ø±Ø§ÛŒ emit Ø¯Ø³ØªÙ‡â€ŒØ§ÛŒ (P1-8).
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
                // P0-2: Newer/Older/Changed Ù‡Ù… Ú©Ù¾ÛŒ Ù…ÛŒâ€ŒØ´ÙˆÙ†Ø¯
                counts.copied += 1;
                counts.bytes += size;
                *files_done += 1;
                // Ù…Ø³ÛŒØ± Ù†Ø³Ø¨ÛŒ Ø¯Ø± Ø±ÙˆÛŒØ¯Ø§Ø¯ Ù¾ÛŒØ´Ø±ÙØª (P0-1)
                *current_file = relative_to(src, &abs);
            }
            Some((Kind::Same, _, _)) | Some((Kind::Tweaked, _, _)) => counts.skipped += 1,
            _ => {}
        }
    }
}

/// Ø­Ù„Ù‚Ù‡ Ù…Ø§Ù†ÛŒØªÙˆØ±ÛŒÙ†Ú¯: Ù‡Ø± Û³Û°Û°ms ÙØ§ÛŒÙ„ /UNILOG Ø±Ø§ tail Ù…ÛŒâ€ŒÚ©Ù†Ø¯ØŒ Ø­Ø¯Ø§Ú©Ø«Ø± ÛŒÚ© Ø±ÙˆÛŒØ¯Ø§Ø¯
/// `backup://log` Ùˆ ÛŒÚ© `backup://progress` Ø¯Ø± Ù‡Ø± ØªÛŒÚ© Ù…ÛŒâ€ŒÙØ±Ø³ØªØ¯ (P1-8) Ùˆ Ø¯Ø±
/// Ù¾Ø§ÛŒØ§Ù†ØŒ Ø´Ù…Ø§Ø±Ù†Ø¯Ù‡â€ŒÙ‡Ø§ Ø±Ø§ Ø§Ø² Ø¬Ø¯ÙˆÙ„ Ø®Ù„Ø§ØµÙ‡Ù” UNILOG Ù…ÛŒâ€ŒÚ¯ÛŒØ±Ø¯ (P0-3).
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

        // Ø¢ÛŒØ§ ÙØ±Ø¢ÛŒÙ†Ø¯ ØªÙ…Ø§Ù… Ø´Ø¯Ù‡ØŸ
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

    // ---- Ú©Ø¯ Ø®Ø±ÙˆØ¬ Ùˆ Ù†ØªÛŒØ¬Ù‡ (P1-9) ----
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

    // ---- Ø´Ù…Ø§Ø±Ù†Ø¯Ù‡â€ŒÙ‡Ø§ÛŒ Ù†Ù‡Ø§ÛŒÛŒ Ø§Ø² Ø¬Ø¯ÙˆÙ„ Ø®Ù„Ø§ØµÙ‡Ù” UNILOG (P0-3) ----
    let full_log = read_log_file_full(&log_path).unwrap_or_default();
    let (copied, skipped, failed, bytes) = if let Some(t) = parse_summary_table(&full_log) {
        (
            t.files_copied(),
            t.files_skipped(),
            t.files_failed(),
            t.bytes_copied(),
        )
    } else {
        // Ø§Ø¬Ø±Ø§ÛŒ Ú©Ø´ØªÙ‡â€ŒØ´Ø¯Ù‡ ÛŒØ§ ÙØ±Ù…Øª ØºÛŒØ±Ù…Ù†ØªØ¸Ø±Ù‡ â†’ Ø´Ù…Ø§Ø±Ù†Ø¯Ù‡â€ŒÙ‡Ø§ÛŒ Ø²Ù†Ø¯Ù‡
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

    // Ø§Ø¹Ù„Ø§Ù† ÙˆÛŒÙ†Ø¯ÙˆØ² (PLAN Â§4.5)
    use tauri_plugin_notification::NotificationExt;
    let (title, body) = match result {
        "success" => ("Ø¨Ú©Ø§Ù¾ Ú©Ø§Ù…Ù„ Ø´Ø¯ âœ”", format!("{copied} ÙØ§ÛŒÙ„ Ú©Ù¾ÛŒ Ø´Ø¯.")),
        "warnings" => (
            "Ø¨Ú©Ø§Ù¾ Ø¨Ø§ Ù‡Ø´Ø¯Ø§Ø± Ú©Ø§Ù…Ù„ Ø´Ø¯",
            format!("{copied} ÙØ§ÛŒÙ„ Ú©Ù¾ÛŒ Ø´Ø¯ØŒ Ú†Ù†Ø¯ Ù…ÙˆØ±Ø¯ Ù†Ø§Ù‡Ù…Ø§Ù‡Ù†Ú¯ Ø¨ÙˆØ¯."),
        ),
        "failed" => ("Ø¨Ú©Ø§Ù¾ Ø´Ú©Ø³Øª Ø®ÙˆØ±Ø¯ âœ–", format!("Ú©Ø¯ Ø®Ø·Ø§: {exit_code}")),
        _ => ("Ø¨Ú©Ø§Ù¾ Ù„ØºÙˆ Ø´Ø¯", "ØªÙˆØ³Ø· Ú©Ø§Ø±Ø¨Ø± Ù„ØºÙˆ Ø´Ø¯.".to_string()),
    };
    let _ = app.notification().builder().title(title).body(body).show();

    let _ = app.emit("backup://finished", &summary);

    // ÙˆØ¶Ø¹ÛŒØª Â«Ø¯Ø± Ø­Ø§Ù„ Ø§Ø¬Ø±Ø§Â» Ø±Ø§ Ø¢Ø²Ø§Ø¯ Ú©Ù† (Ú¯Ø§Ø±Ø¯ Ù‡Ù… Ù…Ø­Ø§ÙØ¸ Ù¾Ø§Ù†ÛŒÚ© Ø§Ø³Øª)
    *running_cell.lock().unwrap() = None;
    drop(guard);
}

// ------------------------------------------------------------------
// ØªØ³Øªâ€ŒÙ‡Ø§ â€” Ø¨Ø§ Ú¯Ø²ÛŒØ¯Ù‡â€ŒÙ‡Ø§ÛŒ ÙˆØ§Ù‚Ø¹ÛŒ Ù„Ø§Ú¯â€ŒÙ‡Ø§ÛŒ T1/T4/T5/T9/T10
// ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("backupsync_test_{name}_{}", std::process::id()));
        let _ = fs::create_dir_all(&d);
        d
    }

    // ---------- parse_line: Ù‡Ù…Ù‡Ù” Ø¨Ø±Ú†Ø³Ø¨â€ŒÙ‡Ø§ (Ú¯Ø²ÛŒØ¯Ù‡Ù” ÙˆØ§Ù‚Ø¹ÛŒ Ù„Ø§Ú¯â€ŒÙ‡Ø§) ----------

    #[test]
    fn parse_line_all_tags_from_real_logs() {
        let src7 = "C:\\Users\\Alireza\\AppData\\Local\\Temp\\opencode\\bs_test\\src7";
        let dst = "C:\\Users\\Alireza\\AppData\\Local\\Temp\\opencode\\bs_test\\dst";

        // T9: ÙØ§ÛŒÙ„ Ù…Ù‚ØµØ¯ Ù‚Ø¯ÛŒÙ…ÛŒâ€ŒØªØ± â€” robocopy Ø¢Ù† Ø±Ø§ Ú©Ù¾ÛŒ Ù…ÛŒâ€ŒÚ©Ù†Ø¯
        let (k, sz, p) = parse_line(
            "\t    Older     \t\t      33\tC:\\Users\\Alireza\\AppData\\Local\\Temp\\opencode\\bs_test\\src7\\t1.txt",
        )
        .unwrap();
        assert_eq!(k, Kind::Older);
        assert_eq!(sz, 33);
        assert!(p.starts_with(src7), "Ù…Ø³ÛŒØ± Ù…Ø·Ù„Ù‚: {p}");

        // T10: Ù‡Ù…Ø§Ù† Ø²Ù…Ø§Ù†ØŒ Ø§Ù†Ø¯Ø§Ø²Ù‡ Ù…ØªÙØ§ÙˆØª â†’ Changed
        let (k, sz, p) = parse_line(
            "\t    Changed   \t\t      17\tC:\\Users\\Alireza\\AppData\\Local\\Temp\\opencode\\bs_test\\src7\\t2.txt",
        )
        .unwrap();
        assert_eq!(k, Kind::Changed);
        assert_eq!(sz, 17);
        assert!(p.starts_with(src7));

        // T5: ÙØ§ÛŒÙ„ Ø¨Ø²Ø±Ú¯ â€” New File
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

        // T4: *EXTRA Dir Ø¨Ø§ Ø¹Ø¯Ø¯ Ù…Ù†ÙÛŒ -1 (Ù…Ø³ÛŒØ± Ø¨Ø§ Ø¨Ú©â€ŒØ§Ø³Ù„Ø´ Ø§Ù†ØªÙ‡Ø§ÛŒÛŒ)
        let (k, sz, p) = parse_line(
            "\t*EXTRA Dir        -1\tC:\\Users\\Alireza\\AppData\\Local\\Temp\\opencode\\bs_test\\dst\\sub_two\\",
        )
        .unwrap();
        assert_eq!(k, Kind::ExtraDir);
        assert_eq!(sz, 0); // Ø¹Ø¯Ø¯ Ù…Ù†ÙÛŒ Ø¨Ù‡â€ŒØ¹Ù†ÙˆØ§Ù† Ø§Ù†Ø¯Ø§Ø²Ù‡ Ù†Ø§Ø¯ÛŒØ¯Ù‡ Ú¯Ø±ÙØªÙ‡ Ù…ÛŒâ€ŒØ´ÙˆØ¯
        assert!(p.ends_with("sub_two\\"), "Ù…Ø³ÛŒØ± Ù¾ÙˆØ´Ù‡: {p}");

        // Same Ùˆ Tweaked
        let (k, _, _) = parse_line("\t    Same       \t\t      17\tC:\\x\\a.txt").unwrap();
        assert_eq!(k, Kind::Same);
        let (k, _, _) = parse_line("\t    Tweaked    \t\t         \tC:\\x\\a.txt").unwrap();
        assert_eq!(k, Kind::Tweaked);

        // Newer
        let (k, _, _) = parse_line("\t    Newer      \t\t      17\tC:\\x\\a.txt").unwrap();
        assert_eq!(k, Kind::Newer);

        // Ø®Ø·ÙˆØ· ØºÛŒØ± Ø¨Ø±Ú†Ø³Ø¨â€ŒØ¯Ø§Ø± â†’ None
        assert!(parse_line("-------------------------------------------------------------------------------").is_none());
        assert!(parse_line("               Total    Copied   Skipped  Mismatch    FAILED    Extras").is_none());
        assert!(parse_line("").is_none());
        // Ù†Ø§Ù… ÙØ§ÛŒÙ„ Ø­Ø§ÙˆÛŒ Â«ERRORÂ» Ø¨Ø±Ú†Ø³Ø¨ Ù†ÛŒØ³Øª
        assert!(parse_line("\t    New File  \t\t      17\tC:\\ERROR files\\a.txt").is_some());
    }

    #[test]
    fn parse_line_persian_name_and_unc() {
        // T6: Ù…Ù†Ø¨Ø¹ UNC â€” Ù…Ø³ÛŒØ± Ù…Ø·Ù„Ù‚ UNC Ú†Ø§Ù¾ Ù…ÛŒâ€ŒØ´ÙˆØ¯
        let line = "\t    New File  \t\t      14\t\\\\localhost\\TestShare\\src6\\Ø³Ù†Ø¯ ÙØ§Ø±Ø³ÛŒ.txt";
        let (k, sz, p) = parse_line(line).unwrap();
        assert_eq!(k, Kind::New);
        assert_eq!(sz, 14);
        assert_eq!(p, "\\\\localhost\\TestShare\\src6\\Ø³Ù†Ø¯ ÙØ§Ø±Ø³ÛŒ.txt");
    }

    // ---------- relative_to (P0-1) ----------

    #[test]
    fn relative_to_strips_roots() {
        assert_eq!(
            relative_to(
                "C:\\Users\\Alireza\\AppData\\Local\\Temp\\opencode\\bs_test\\src",
                "C:\\Users\\Alireza\\AppData\\Local\\Temp\\opencode\\bs_test\\src\\a\\b.txt",
            ),
            "a\\b.txt"
        );
        // Ù†Ø§Ù‡Ù…â€ŒØ­Ø³Ø§Ø³ Ø¨Ù‡ Ø¨Ø²Ø±Ú¯ÛŒ/Ú©ÙˆÚ†Ú©ÛŒ
        assert_eq!(relative_to("c:\\SRC", "C:\\src\\a.txt"), "a.txt");
        // Ø¹Ø¯Ù… ØªØ·Ø§Ø¨Ù‚ â†’ Ù‡Ù…Ø§Ù† Ù…Ø³ÛŒØ± (Ø¨Ø¯ÙˆÙ† crash)
        assert_eq!(relative_to("C:\\src", "D:\\x.txt"), "D:\\x.txt");
        // Ø±ÛŒØ´Ù‡Ù” Ø¯Ø±Ø§ÛŒÙˆ
        assert_eq!(relative_to("E:\\", "E:\\a.txt"), "a.txt");
        // UNC (T6)
        assert_eq!(
            relative_to("\\\\localhost\\TestShare\\src6", "\\\\localhost\\TestShare\\src6\\f1.txt"),
            "f1.txt"
        );
        assert_eq!(
            relative_to("\\\\h\\s", "\\\\h\\s\\sub\\deep\\ÙØ§ÛŒÙ„.txt"),
            "sub\\deep\\ÙØ§ÛŒÙ„.txt"
        );
        // Ø®ÙˆØ¯ Ø±ÛŒØ´Ù‡ â†’ Ø®Ø§Ù„ÛŒ
        assert_eq!(relative_to("C:\\src", "C:\\src"), "");
        // ØªØ±ÛŒÙ„ÛŒÙ†Ú¯ Ø¨Ú©â€ŒØ§Ø³Ù„Ø´ Ø¯Ø± ÙˆØ±ÙˆØ¯ÛŒ Ù…Ø³ÛŒØ±
        assert_eq!(relative_to("C:\\src", "C:\\src\\sub\\"), "sub");
        // Ø¨Ú©â€ŒØ§Ø³Ù„Ø´ Ø§Ù†ØªÙ‡Ø§ÛŒÛŒ Ø±ÙˆÛŒ Ù¾ÙˆØ´Ù‡Ù” *EXTRA Dir
        assert_eq!(
            relative_to(
                "C:\\Users\\Alireza\\AppData\\Local\\Temp\\opencode\\bs_test\\dst",
                "C:\\Users\\Alireza\\AppData\\Local\\Temp\\opencode\\bs_test\\dst\\sub_two\\",
            ),
            "sub_two"
        );
    }

    // ---------- Ø¨Ø§Ú¯ T4: join Ù…Ø³ÛŒØ± Ù…Ø·Ù„Ù‚ Ù¾Ø§ÛŒÙ‡ Ø±Ø§ Ø¯ÙˆØ± Ù…ÛŒâ€ŒØ±ÛŒØ²Ø¯ ----------

    #[test]
    fn t4_regression_join_absolute_discards_base() {
        // Ø§Ø«Ø¨Ø§Øª Ø¨Ø§Ú¯ Ù‚Ø¯ÛŒÙ…ÛŒ: join Ø¨Ø§ Ù…Ø³ÛŒØ± Ù…Ø·Ù„Ù‚ØŒ Ù¾Ø§ÛŒÙ‡Ù” recycle_root Ø±Ø§ Ø¯ÙˆØ± Ù…ÛŒâ€ŒØ±ÛŒØ²Ø¯
        let recycle_root = PathBuf::from("C:\\dst\\_Deleted\\ts");
        let abs = "C:\\dst\\b.txt"; // Ú†ÛŒØ²ÛŒ Ú©Ù‡ parse_line Ù‚Ø¯ÛŒÙ…ÛŒ Ø¨Ø±Ù…ÛŒâ€ŒÚ¯Ø±Ø¯Ø§Ù†Ø¯
        let buggy_to = recycle_root.join(abs);
        let buggy_from = PathBuf::from("C:\\dst").join(abs);
        assert_eq!(buggy_to, buggy_from, "join Ù…Ø·Ù„Ù‚ = Ø§Ù†ØªÙ‚Ø§Ù„ Ø±ÙˆÛŒ Ø®ÙˆØ¯Ø´");

        // Ø±Ø§Ù‡â€ŒØ­Ù„ Ø¬Ø¯ÛŒØ¯: Ù…Ø³ÛŒØ± Ù†Ø³Ø¨ÛŒ
        let dst = "C:\\dst";
        let rel = relative_to(dst, abs);
        assert_eq!(rel, "b.txt");
        let from = PathBuf::from(dst).join(&rel);
        let to = recycle_root.join(&rel);
        assert_ne!(from, to);
        assert!(from.starts_with(Path::new(dst)));
        assert!(to.starts_with(&recycle_root));
    }

    #[test]
    fn t4_regression_subfolder_move_paths() {
        // ÙØ§ÛŒÙ„ Ø¯Ø§Ø®Ù„ Ø²ÛŒØ±Ù¾ÙˆØ´Ù‡ (T4: sub_two\two files same name.txt)
        let dst = "C:\\Users\\Alireza\\AppData\\Local\\Temp\\opencode\\bs_test\\dst";
        let abs = "C:\\Users\\Alireza\\AppData\\Local\\Temp\\opencode\\bs_test\\dst\\sub_two\\two files same name.txt";
        let rel = relative_to(dst, abs);
        assert_eq!(rel, "sub_two\\two files same name.txt");
        let recycle_root = PathBuf::from(dst).join(DELETED_DIR).join("2026-11-15_140000");
        let from = PathBuf::from(dst).join(&rel);
        let to = recycle_root.join(&rel);
        assert_ne!(from, to);
        assert_eq!(
            to,
            PathBuf::from(dst)
                .join("_Deleted")
                .join("2026-11-15_140000")
                .join("sub_two")
                .join("two files same name.txt"),
            "Ø§Ù†ØªÙ‚Ø§Ù„ Ø²ÛŒØ±Ù¾ÙˆØ´Ù‡ Ø¨Ø§ÛŒØ¯ Ø³Ø§Ø®ØªØ§Ø± Ø²ÛŒØ±Ù¾ÙˆØ´Ù‡ Ø±Ø§ Ø¯Ø± _Deleted Ù†Ú¯Ù‡ Ø¯Ø§Ø±Ø¯"
        );
    }

    // ---------- Ø¬Ø¯ÙˆÙ„ Ø®Ù„Ø§ØµÙ‡ (P0-3) ----------

    #[test]
    fn parse_summary_table_t4_fixture() {
        // Ú¯Ø²ÛŒØ¯Ù‡Ù” ÙˆØ§Ù‚Ø¹ÛŒ T4_mirror_list.log
        let content = "\
               Total    Copied   Skipped  Mismatch    FAILED    Extras\r
    Dirs :         3         3         3         0         0         1\r
   Files :        10         1         9         0         0         4\r
   Bytes :       188        17       171         0         0        61\r
   Times :   0:00:00   0:00:00                       0:00:00   0:00:00\r
   Ended : 15 November 2026 13:58:27\r
";
        let t = parse_summary_table(content).expect("Ø¬Ø¯ÙˆÙ„ Ø¨Ø§ÛŒØ¯ Ù¾Ø§Ø±Ø³ Ø´ÙˆØ¯");
        assert_eq!(t.dirs, [3, 3, 3, 0, 0, 1]);
        assert_eq!(t.files, [10, 1, 9, 0, 0, 4]);
        assert_eq!(t.bytes, [188, 17, 171, 0, 0, 61]);
        assert_eq!(t.files_copied(), 1);
        assert_eq!(t.files_skipped(), 9);
        assert_eq!(t.files_failed(), 0);
        assert_eq!(t.bytes_copied(), 17);
    }

    #[test]
    fn parse_summary_table_failed_positive() {
        // Ø³Ø¨Ú© ÙˆØ§Ù‚Ø¹ÛŒ robocopy Ø¨Ø§ FAILED > 0
        let content = "\
               Total    Copied   Skipped  Mismatch    FAILED    Extras\r
    Dirs :         2         1         1         0         1         0\r
   Files :         6         3         2         0         2         0\r
   Bytes :       200        99        99         0         2         0\r
   Times :   0:00:01   0:00:01                       0:00:00   0:00:00\r
";
        let t = parse_summary_table(content).expect("Ø¬Ø¯ÙˆÙ„ Ø¨Ø§ÛŒØ¯ Ù¾Ø§Ø±Ø³ Ø´ÙˆØ¯");
        assert_eq!(t.files_failed(), 2);
        assert_eq!(t.files_copied(), 3);
    }

    #[test]
    fn parse_summary_table_absent_for_killed_run() {
        // Ù„Ø§Ú¯Ù Ú©Ø´ØªÙ‡â€ŒØ´Ø¯Ù‡ (T5-redo): Ø®Ø· Ø¢Ø®Ø± New File Ø¨Ø¯ÙˆÙ† Ø¬Ø¯ÙˆÙ„ â†’ None
        let content = "\t    New File  \t\t10737418240\tC:\\x\\huge10.bin";
        assert!(parse_summary_table(content).is_none());
        assert!(parse_summary_table("").is_none());
        // Ø±Ø¯ÛŒÙâ€ŒÙ‡Ø§ÛŒ ØºÛŒØ± Ù…ØªÙˆØ§Ù„ÛŒ (Ø¨Ø§ Ø®Ø· Ù…ÛŒØ§Ù†ÛŒ ØºÛŒØ±Ù…Ø·Ø§Ø¨Ù‚Øª) â†’ None
        let content = "\
    Dirs :         1         1         1         0         0         0\r
   Times :   0:00:00   0:00:00                       0:00:00   0:00:00\r
   Bytes :       118        33        85         0         0         0\r
   Files :         6         1         5         0         0         0\r
";
        assert!(parse_summary_table(content).is_none());
    }

    // ---------- normalize_path (P0-7) ----------

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

    // ---------- map_result (P1-9) ----------

    #[test]
    fn map_result_cases() {
        assert_eq!(map_result(true, 0), "cancelled");
        assert_eq!(map_result(true, 16), "cancelled");
        assert_eq!(map_result(false, -1), "failed"); // Ø¨Ø¯ÙˆÙ† Ú©Ø¯
        assert_eq!(map_result(false, 8), "failed");
        assert_eq!(map_result(false, 16), "failed");
        assert_eq!(map_result(false, 4), "warnings");
        assert_eq!(map_result(false, 7), "warnings");
        assert_eq!(map_result(false, 3), "success");
        assert_eq!(map_result(false, 1), "success");
        assert_eq!(map_result(false, 0), "success");
    }

    // ---------- is_error_line (P0-3) ----------

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

    // ---------- tail_log: Ø¨Ø±Ø´ Ø¯Ø± Ù‡Ù…Ù‡Ù” Ø¢ÙØ³Øªâ€ŒÙ‡Ø§ÛŒ Ø¨Ø§ÛŒØªÛŒ (P1-8) ----------

    #[test]
    fn tail_log_split_at_every_byte_offset() {
        let content = "line1\r\nØ³Ù†Ø¯ ÙØ§Ø±Ø³ÛŒ.txt\r\nðŸ˜€emoji ok\r\nlast line no newline";
        let buf: Vec<u8> = content.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        let dir = tmp_dir("tail");
        let path = dir.join("tail_utf16.log");
        let expected = vec![
            "line1".to_string(),
            "Ø³Ù†Ø¯ ÙØ§Ø±Ø³ÛŒ.txt".to_string(),
            "ðŸ˜€emoji ok".to_string(),
            "last line no newline".to_string(),
        ];
        for split in 0..=buf.len() {
            let _ = fs::remove_file(&path);
            fs::write(&path, &buf[..split]).unwrap();
            let mut byte_pending: Vec<u8> = Vec::new();
            let mut text_pending = String::new();
            let (off, mut lines) = tail_log(&path, 0, &mut byte_pending, &mut text_pending, false);
            // Ù†ÛŒÙ…Ù‡Ù” Ø¯ÙˆÙ… + flush Ù¾Ø§ÛŒØ§Ù†ÛŒ
            {
                use std::io::Write;
                let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
                f.write_all(&buf[split..]).unwrap();
            }
            let (_, lines2) = tail_log(&path, off, &mut byte_pending, &mut text_pending, true);
            lines.extend(lines2);
            let lines: Vec<String> = lines.into_iter().filter(|l| !l.trim().is_empty()).collect();
            assert_eq!(lines, expected, "Ø¨Ø±Ø´ Ø¯Ø± Ø¢ÙØ³Øª Ø¨Ø§ÛŒØªÛŒ {split}");
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn tail_log_final_flush_without_newline_t5_regression() {
        // T5-redo: Ø¢Ø®Ø±ÛŒÙ† Ø®Ø· Ø§Ø¬Ø±Ø§ÛŒ Ú©Ø´ØªÙ‡â€ŒØ´Ø¯Ù‡ Ø¨Ø¯ÙˆÙ† \r\n (2E 00 62 00 69 00 6E 00
        // = "bin." Ø§Ø² Ø§Ù†ØªÙ‡Ø§ÛŒ ...\huge10.bin) Ø¨Ø§ÛŒØ¯ Ø¨Ø§ flush Ø¨Ø±Ú¯Ø±Ø¯Ø¯
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
        assert!(!capped.contains("payload_000000"), "oldest lines must be trimmed");
        assert!(capped.contains("payload_059999"), "newest lines must be kept");
        assert!(capped.len() < full.len(), "capped read must keep fewer bytes");
        assert!(full.contains("payload_000000"));
        assert!(full.contains("payload_059999"));
        let _ = fs::remove_dir_all(&dir);
    }

    // ---------- register_run / SlotGuard (P0-4) ----------

    #[test]
    fn register_run_twice_rejected_then_slot_freed() {
        let cell: Arc<Mutex<Option<RunHandle>>> = Arc::new(Mutex::new(None));
        let (h1, g1) = register_run(&cell, "run_1").expect("first registration");
        let err = match register_run(&cell, "run_2") {
            Err(e) => e,
            Ok(_) => panic!("second registration must fail"),
        };
        assert!(err.contains("Ø¯Ø± Ø­Ø§Ù„ Ø§Ø¬Ø±Ø§"), "unexpected error: {err}");
        // Ø¨Ø§ Ø§ÙØªØ§Ø¯Ù† Ú¯Ø§Ø±Ø¯ Ø§ÙˆÙ„ (Ù¾Ø§ÛŒØ§Ù† Ø§Ø¬Ø±Ø§)ØŒ Ø«Ø¨Øª Ø¯ÙˆÙ… Ù…ÙˆÙÙ‚ Ù…ÛŒâ€ŒØ´ÙˆØ¯
        drop(g1);
        drop(h1);
        let (_h2, _g2) = register_run(&cell, "run_2").expect("slot freed after guard drop");
    }

    #[test]
    fn slot_guard_releases_on_early_return() {
        // Ø«Ø¨ØªØŒ Ø®Ø·Ø§ØŒ Ø¨Ø§Ø²Ú¯Ø´Øª Ø²ÙˆØ¯Ø±Ø³ â†’ Ø¬Ø§ÛŒÚ¯Ø§Ù‡ Ø¨Ø§ÛŒØ¯ Ø¢Ø²Ø§Ø¯ Ø´ÙˆØ¯
        let cell: Arc<Mutex<Option<RunHandle>>> = Arc::new(Mutex::new(None));
        fn early_fail(cell: &Arc<Mutex<Option<RunHandle>>>) -> Result<(), String> {
            let (_h, _guard) = register_run(cell, "run_x")?;
            return Err("early failure".to_string()); // _guard Ø§ÛŒÙ†â€ŒØ¬Ø§ drop Ù…ÛŒâ€ŒØ´ÙˆØ¯
        }
        let err = early_fail(&cell).unwrap_err();
        assert_eq!(err, "early failure");
        assert!(cell.lock().unwrap().is_none(), "slot must be free after early return");
    }

    #[test]
    fn slot_guard_releases_on_panic() {
        let cell: Arc<Mutex<Option<RunHandle>>> = Arc::new(Mutex::new(None));
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let (_h, _guard) = register_run(&cell, "run_panic").unwrap();
            panic!("simulated panic");
        }));
        assert!(res.is_err(), "panic must be caught");
        assert!(cell.lock().unwrap().is_none(), "slot must be free after panic");
    }

    // ---------- Mirror fail-closed (C1) ----------

    #[test]
    fn mirror_list_without_summary_table_aborts() {
        // Ù„Ø§Ú¯ Ú©Ø´ØªÙ‡â€ŒØ´Ø¯Ù‡ / ÙØ±Ù…Øª Ø®Ø±Ø§Ø¨: Ø¬Ø¯ÙˆÙ„ Ù†Ø¯Ø§Ø±Ø¯ â†’ Ø¨Ø§ÛŒØ¯ Ø®Ø·Ø§ Ø¨Ø¯Ù‡Ø¯ØŒ Ù‡ÛŒÚ† ÙØ§ÛŒÙ„ÛŒ Ø¬Ø§Ø¨Ù‡â€ŒØ¬Ø§ Ù†Ø´ÙˆØ¯
        let killed = "\t    New File  \t\t10737418240\tC:\\x\\huge10.bin";
        let err = ensure_mirror_list_ok(killed).unwrap_err();
        assert!(err.contains("Ø¬Ø¯ÙˆÙ„ Ø®Ù„Ø§ØµÙ‡"), "unexpected error: {err}");
        // Ù„Ø§Ú¯ Ø³Ø§Ù„Ù… T4 â†’ Ok
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
