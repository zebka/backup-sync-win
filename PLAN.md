# Project Brief: "Backup Sync", a Windows incremental backup app (Tauri)

> This document is a self-contained brief for a coding agent. Build the app described here. Where this brief is silent, choose the simplest option that works. Do not add features that are not listed.

## 1. Goal

A small Windows desktop app that backs up a **source folder (usually a network share, e.g. `\\PC-NAME\Share\Docs`)** into a **local destination folder**, copying **only new or changed files** on each run, never re-copying everything.

The app is a polished GUI on top of the built-in `robocopy.exe`. Do NOT write a custom file-copy or diff engine. Robocopy does the incremental comparison, retries, and SMB handling.

The user is a network/IT engineer who values simple, practical solutions. Avoid over-engineering.

## 2. Terminology (use these exact words in code and UI)

- **Source**: the folder being backed up (network share or any path).
- **Destination**: the local folder where the backup is stored.
- **Profile**: a saved pair of source + destination + options.

## 3. Tech stack

- **Tauri v2**, Windows target only (`x86_64-pc-windows-msvc`).
- **Frontend: plain HTML + CSS + vanilla JS (ES modules), no bundler, no framework, no npm build step.** Set `frontendDist` to `../src`. Use `withGlobalTauri: true` so the JS can use `window.__TAURI__`.
- **Backend: Rust** (stable). Allowed crates: `serde`, `serde_json`, `tokio` (process, io, time), `chrono`, `dirs` (or Tauri's path API), `tauri` plugins `dialog` (folder picker) and `notification`.
- Tauri window: custom look, but keep native title bar for v1 (frameless is out of scope).
- Persian (UTF-8) file names must work everywhere (paths, log, UI).

## 3.1 Project layout

```
backup-app/
├─ PLAN.md
├─ README.md                 (how to run/build, in English)
├─ src/                      (frontend)
│  ├─ index.html
│  ├─ styles.css
│  └─ app.js
└─ src-tauri/
   ├─ Cargo.toml
   ├─ build.rs
   ├─ tauri.conf.json
   ├─ capabilities/default.json
   └─ src/
      ├─ main.rs             (entry, command registration)
      ├─ robocopy.rs         (build args, spawn, parse log)
      ├─ profiles.rs         (load/save profiles JSON)
      └─ history.rs          (run history JSON)
```

## 4. Features

### v1 (must have)

1. **Profile setup screen**
   - Source path: text input + "Browse" button (folder picker), also accepts typed UNC paths.
   - Destination path: text input + "Browse".
   - Mode selector (see section 5).
   - Exclude patterns: a simple multi-line text box, default: `Thumbs.db`, `~$*`, `*.tmp`, `desktop.ini`.
   - Optional "Network login": username + password, used only to connect to the share (see section 7).
   - Save profile (single profile is enough for v1, but store as a list so multiple can be added later).

2. **Check changes button ("بررسی تغییرات")**: runs robocopy in list-only mode (`/L`) and shows a categorized preview:
   - New files, Changed files, Deleted-at-source files (only relevant for Mirror mode), plus counts and total size to be copied.
   - No file is modified on disk during this step.

3. **Start backup button ("شروع بکاپ")**: runs the real copy.
   - Live progress: current file name, files done / total (from the preview counts if available), copied bytes, elapsed time.
   - Live log panel (scrolling, monospace, auto-scroll with pause on user scroll).
   - **Cancel** button that kills the robocopy process cleanly.
   - Final summary card: copied / skipped / failed counts, size, duration, and a result badge (Success / Completed with warnings / Failed).

4. **History**: list of the last 50 runs (date, profile, result, counts, duration). Click a run to see its saved log file. Stored locally as JSON + log files.

5. **Windows notification** when a run finishes (using the notification plugin).

### v2 (do NOT build now, but keep code structure friendly to it)

- Scheduling via Windows Task Scheduler (`schtasks`) launching the app with `--run-profile <id>` headless.
- Tray icon / minimize to tray.
- Multiple profiles UI.

## 5. Backup modes

Two modes only:

1. **Safe (default)**: copy new + changed files. Never delete anything in the destination.
2. **Mirror with recycle**: destination should match the source, but files that no longer exist in the source are **moved** to `<destination>\_Deleted\<yyyy-MM-dd_HHmmss>\<original relative path>` instead of being deleted. Never use `/MIR` or `/PURGE` for the real run.
   - Implementation: run a list-only pass with `/L /E /PURGE`-style detection (or `/MIR /L`) to find lines tagged `*EXTRA File`, move those files with `std::fs::rename` (create dirs as needed, fall back to copy+delete across volumes), then run the normal copy. The `_Deleted` folder must be excluded from the copy comparison (`/XD _Deleted`).

Show a short plain-language description under each mode in the UI. In Mirror mode, show a confirmation on first use explaining that removed files go to `_Deleted`.

## 6. Robocopy details

Base arguments for the real run:

```
robocopy "<SRC>" "<DST>" /E /COPY:DAT /DCOPY:DAT /FFT /R:2 /W:3 /XJ /NP /BYTES /MT:8 /UNILOG:"<logfile>"
```

- `/E` include subfolders (including empty ones).
- `/FFT` tolerate 2-second timestamp granularity (important for NAS/SMB).
- `/R:2 /W:3` limited retries so a locked file does not hang the run for the default ~30 days.
- `/XJ` skip junction points (avoid loops).
- `/XF <patterns>` and `/XD _Deleted` from the exclude list.
- Do NOT add `/XO`; robocopy's default compare (size + timestamp) is what we want.
- `/MT:<n>` multithreaded copy: make `n` a profile setting (`threads`, default 8) since network performance varies.
- Preview run: same arguments plus `/L`.
- Do NOT use `/MIR` or `/PURGE` for real runs.

**Output encoding (important, Persian file names):** do not parse robocopy's console stdout (OEM codepage corrupts Persian). Use `/UNILOG:<file>` (UTF-16LE log) and **tail that file** from Rust by polling every ~300 ms, decoding UTF-16LE, and emitting new lines to the frontend. Do not use `/TEE` and do not rely on stdout.

**Exit codes:** robocopy returns a bitmask. 0 = nothing to copy, 1 = files copied, 2 = extras present, 4 = mismatches, 8+ = failures. Treat `< 8` as success (with warnings if 4 is set) and `>= 8` as failed. Map this to the UI badge.

**Parsing log lines** (for preview and progress): lines like `New File`, `Newer`, `Older`, `Same`, `*EXTRA File`, `*EXTRA Dir`, with `/BYTES` giving sizes in plain numbers. Be tolerant: unknown lines go to the raw log only.

## 7. Network access and credentials

- Default: rely on the current Windows session's access to the share. Most home/office setups already work this way.
- If the user enters a username/password for the share: run `cmdkey /add:<host> /user:<user> /pass:<pass>` (stores in Windows Credential Manager) or `net use \\host\share /user:... /persistent:no` before the run. Never write the password to the profile JSON or to logs. Keep it in Windows Credential Manager only, referenced by host name.
- If the source is unreachable (path not found, access denied), show a clear human-readable error before starting robocopy (check with `std::fs::metadata` first).

## 8. Backend interface (Tauri commands and events)

Commands (all async, return `Result<T, String>`):

- `load_profiles() -> Vec<Profile>` / `save_profile(profile)`
- `preview_changes(profile) -> PreviewResult` (runs `/L`, returns lists + totals)
- `start_backup(profile) -> RunId`
- `cancel_backup(run_id)`
- `list_history() -> Vec<RunSummary>` / `read_log(run_id) -> String`
- `check_path(path) -> { exists: bool, is_dir: bool, error?: string }`

Events emitted to the frontend:

- `backup://log` payload `{ run_id, line }`
- `backup://progress` payload `{ run_id, current_file, files_done, bytes_done }`
- `backup://finished` payload `RunSummary`

Data location: `%APPDATA%\BackupSync\` containing `profiles.json`, `history.json`, `logs\<run_id>.log`.

`Profile` fields: `id`, `name`, `source`, `destination`, `mode` (`safe` | `mirror_recycle`), `excludes` (string list), `network_user` (optional string, no password), `threads` (number).

Only one backup may run at a time; reject a second start with a clear message.

## 9. UI / visual design

Target look: **dark, minimal, calm, in the spirit of the Claude Code / Claude desktop style**, not a generic dashboard.

- Dark theme only for v1. Background near `#1a1a1a`, surfaces `#222`/`#2a2a2a`, 1px subtle borders `#333`, text `#e8e6e1`, muted text `#9a978f`.
- One accent color: a warm orange/terracotta (about `#d97757`) for the primary button and active states. Success green and warning amber used only for status badges.
- Typography: system UI font stack for text; monospace (`Cascadia Mono`, `Consolas`) for paths and logs. Generous spacing, 8px grid, rounded corners (8-10px), no heavy shadows, no gradients.
- Layout: a single window (about 960x640, min 760x520) with a slim left sidebar (Backup, History) and a main content area.
  - **Backup view** top to bottom: profile card (source, destination, mode, excludes collapsed under "Advanced"), a row with two buttons ("بررسی تغییرات" secondary, "شروع بکاپ" primary), then a results area that switches between preview lists, live progress + log, and the final summary card.
- Buttons show disabled/loading states; a thin progress bar sits under the action row during a run.
- **Language and direction:** UI labels in **Persian**, layout `dir="rtl"`; paths, logs, and file names are forced `dir="ltr"` (use `unicode-bidi: plaintext` or isolate) so mixed Persian/English text renders correctly. Use a Persian-capable font (Vazirmatn if bundled locally as a font file, else system fallback `Tahoma`). Do not load fonts from a CDN; the app must work offline.
- Keep animations subtle (150-200 ms fades only).

## 10. Edge cases to handle

- Source or destination missing, or destination inside the source (reject: infinite-copy risk).
- Very long paths (>260 chars): note in the README that long paths may need to be enabled in Windows; do not add custom handling.
- Network drops mid-run: robocopy retries; if it ends with exit code >= 8, show Failed with the last 20 log lines.
- App closed during a run: kill the child robocopy process on exit.
- Locked/in-use files: they appear as failures in the log; count them and show "N files skipped" in the summary, not a crash.
- Paths with spaces, Persian characters, and trailing backslashes (quote and normalize properly; a trailing `\` before a closing quote breaks Windows arguments, so strip trailing backslashes).

## 11. Acceptance criteria

1. Choosing a network source and a local destination, then pressing "بررسی تغییرات" shows correct New/Changed counts without touching the destination.
2. The first backup copies everything; running it again immediately copies **0 files** and finishes in seconds.
3. Editing one file at the source and running again copies exactly that one file.
4. In Mirror mode, deleting a file at the source and running again moves it to `_Deleted\<timestamp>\...` in the destination; nothing is permanently deleted.
5. Persian file and folder names appear correctly in the preview, the live log, and the history logs.
6. Cancel stops robocopy within a couple of seconds and records the run as Cancelled.
7. No password appears in any file under `%APPDATA%\BackupSync\`.
8. `npm`/Node is not required to build; `cargo tauri build` (with Tauri CLI installed via `cargo install tauri-cli`) produces the installer.

## 12. Deliverables

- The full project in the layout from section 3.1.
- `README.md` with: prerequisites (Rust, MSVC build tools, WebView2), `cargo tauri dev`, `cargo tauri build`, and where data/logs are stored.
- Keep code commented where behavior is non-obvious (argument building, log tailing, exit-code mapping).

## 13. Out of scope for v1

Scheduling, tray icon, multiple-profile UI, encryption, compression, cloud targets, version history beyond `_Deleted`, auto-update, light theme, frameless window, macOS/Linux builds.
