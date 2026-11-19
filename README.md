# BackupSync — Windows Incremental Backup for Network Shares

**Version 2.0 — bilingual UI (فارسی / English).** BackupSync is a lightweight, open-source **Windows backup application** for backing up **network shares**, **local folders**, and **offline drives** with an intuitive **RTL-capable UI** (Persian right-to-left and English left-to-right) and a fast native **Tauri + Rust** backend. It wraps **Microsoft Robocopy** with a safer, user-friendly experience: incremental copies, change preview, mirror mode with `_Deleted`, scheduled backups, and Windows startup support.

> اگر به‌دنبال یک **بکاپ ویندوز** یا **نرم‌افزار بکاپ شبکه** برای اشتراک‌های SMB هستید، BackupSync گزینه‌ای سبک، قابل‌ممیزی و متن‌باز است.

[فارسی](README.fa.md) · English · Keywords: Windows backup software, network share backup, incremental backup, Robocopy GUI, Tauri backup app, Persian backup tool, مروری‌بر Robocopy، بکاپ اختصاصی ویندوز، بکاپ اشتراک شبکه

## Languages / زبان‌ها

- UI languages: **فارسی (RTL)** and **English (LTR)**.
- Switch the interface from the sidebar with the **فارسی / English** button.
- User-facing errors and notifications are also emitted in the chosen language.

## Highlights

- **Incremental backup** — copies only new or changed files.
- **Safe mode** — no deletions ever; ideal for important folders.
- **Mirror with recycle protection** — removed-at-source files are moved to `_Deleted\<timestamp>\…`, never permanently deleted by the app.
- **Mass-delete guard** — refuses to move a suspicious majority of destination files until you explicitly confirm.
- **Change preview** — see new/changed/deleted candidates before any file is touched.
- **Live log + history** — UTF-16 `/UNILOG` tail, bounded DOM, final counts from the Robocopy summary table.
- **Profile management** — keep up to three backup profiles, switch and edit instantly.
- **Daily / weekly scheduling** — per-profile scheduled backup or check-only runs.
- **Windows startup** — optional launch at logon via the current-user Run key.
- **No password storage** — share passwords are kept in memory for the running app only and never written to profiles, logs, or settings.
- **Secure CSP** — restrictive Content Security Policy, no remote code, offline fonts bundled locally.

## Who is this for?

BackupSync is for anyone who needs a **simple, dependable windows network backup software** without paying for a heavyweight enterprise suite:

- IT teams backing up a department share to a local disk.
- Home users protecting a shared `Documents` / `Music` / `Admin` folder.
- Persian-speaking users who want a native RTL backup interface.

## Features

| Area | What BackupSync does |
| --- | --- |
| Copy engine | `robocopy.exe` with deterministic `/UNILOG` parsing |
| Modes | Safe incremental · Mirror with `_Deleted` recycle folder |
| Preview | `/L` dry-run list with per-file/new/changed/deleted groups |
| Profiles | Up to three saved profiles, select/update/delete |
| Scheduling | Daily or weekly, at a time you choose, backup or check-only |
| Startup | Optional Windows auto-start via `HKCU\...\Run` |
| Logs | UTF-16LE `/UNILOG`, live batching, 2MB display cap |
| Security | No passwords persisted; CSP `default-src 'self'` |

## Screenshots & usage

The app is a single desktop window with a Persian RTL layout:

1. Pick a **source** (local path or `\\server\share`).
2. Pick a **destination** folder.
3. Choose **Safe** or **Mirror with `_Deleted`**.
4. Optional: set advanced threads / excludes / network credentials.
5. Click **بررسی تغییرات** to preview, then **شروع بکاپ** to run.
6. Review progress, cancellation, and the final summary card.

## Data location

BackupSync stores its data under:

```text
%APPDATA%\BackupSync\
```

Files include:

```text
profiles.json           # up to 3 backup profiles (no passwords)
history.json            # last 50 run summaries
logs\<run_id>.log       # UTF-16LE Robocopy log
logs\<run_id>.moved.txt # mirror mode: files moved to _Deleted
```

## Requirements

- Windows 10 / 11
- **WebView2** Runtime (preinstalled on recent Windows releases)
- **Rust** stable `x86_64-pc-windows-msvc`
- **Visual Studio Build Tools** with the C++ workload
- **Tauri CLI v2** (`cargo install tauri-cli`) or a prebuilt `cargo-tauri`

## Build & run

```bash
git clone https://github.com/OWNER/backup-sync-win.git
cd backup-sync-win

# run in development
cargo tauri dev

# produce an NSIS installer
cargo tauri build
```

Installer output:

```text
src-tauri\target\release\bundle\nsis\BackupSync_*_x64-setup.exe
```

## Project layout

```text
src/            # HTML/CSS/JS frontend (Persian RTL, no bundler)
src-tauri/      # Rust backend (Tauri v2), Robocopy orchestration
  src/robocopy.rs
  src/profiles.rs
  src/history.rs
  src/settings.rs
```

## Design notes

- **No console stdout parsing** — Robocopy emits UTF-16 `/UNILOG`; the app tails the log safely across byte splits.
- **Fail-closed mirror** — if the mirror list pass has no summary table, nothing is moved.
- **Validation on save and start** — paths, excludes, schedule settings are checked before any run.
- **Network share credentials** — temporary `net use` only; the app never persists passwords.

## Roadmap

- Settings UI for retention of `_Deleted`.
- Per-profile bandwidth throttling.
- Tray mode and hidden-on-startup option.
- Portable signed releases.

## License

BackupSync is released under the **MIT License**. See [`LICENSE`](LICENSE).

## FAQ

### Does BackupSync work without a password?

Yes. If the share allows current-user access, leave the network username/password empty.

### Can scheduled backups use a stored password?

No. BackupSync deliberately avoids storing passwords; scheduled network runs require an already-open share session.

### Is this a replacement for enterprise backup?

It is a focused, auditable tool for everyday Windows folder and share backups — not a full disaster-recovery suite.
