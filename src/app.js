// BackupSync — frontend logic (vanilla JS, no bundler)
// Talks to the Rust backend via window.__TAURI__ (withGlobalTauri).

import { I18N } from "./i18n.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { open: openDialog } = window.__TAURI__.dialog;

let LANG = "en";
const t = (key, vars = {}) => {
  let value = (I18N[LANG] && I18N[LANG][key]) || I18N.en[key] || key;
  for (const [name, replacement] of Object.entries(vars)) {
    value = value.replaceAll(`{${name}}`, String(replacement));
  }
  return value;
};

const state = {
  profileId: null,
  profiles: [],
  runId: null,
  pendingRun: false,
  preview: null,
  startedAt: null,
  elapsedTimer: null,
  totalFiles: null,
  autoscroll: true,
  mirrorAcked: localStorage.getItem("mirror_ack") === "1",
  allowMassDelete: false,
  advancedWasOpen: false,
};

let passwordMem = null;
const $ = (id) => document.getElementById(id);
const DEFAULT_EXCLUDES = "Thumbs.db\n~$*\n*.tmp\ndesktop.ini";
const RESULT_CLASS = { success: "success", warnings: "warnings", failed: "failed", cancelled: "cancelled" };

// ----------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------
function fmtBytes(n) {
  if (n == null) return "—";
  const num = Number(n);
  if (!Number.isFinite(num)) return "—";
  if (num < 1024) return `${num} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = num / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
  return `${v.toFixed(1)} ${units[i]}`;
}

function fmtElapsed(ms) {
  const s = Math.max(0, Math.floor((Number(ms) || 0) / 1000));
  const m = Math.floor(s / 60);
  return `${String(m).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
}

function esc(s) {
  return String(s)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function safeResultClass(r) { return RESULT_CLASS[r] || "failed"; }

function reveal(el) {
  if (el && typeof el.scrollIntoView === "function") {
    el.scrollIntoView({ behavior: "smooth", block: "start" });
  }
}

function showBanner(kind, html) {
  const cls = ["error", "info", "warn"].includes(kind) ? kind : "info";
  const b = $("banner");
  b.innerHTML = `<div class="banner ${cls}">${html}</div>`;
  reveal(b);
}

function clearBanner() { $("banner").innerHTML = ""; }

function setLoading(btn, on) {
  btn.disabled = on;
  btn.classList.toggle("loading", on);
}

function resetProgressBar() {
  const bar = $("progressbar");
  const fill = $("progress-fill");
  bar.classList.remove("active");
  if (fill) {
    fill.classList.remove("determinate");
    fill.classList.add("indeterminate");
    fill.style.width = "";
  }
}

function applyI18n() {
  document.documentElement.lang = LANG === "fa" ? "fa" : "en";
  document.documentElement.dir = LANG === "fa" ? "rtl" : "ltr";
  document.title = LANG === "fa" ? "بکاپ‌گیر شبکه" : "BackupSync — Windows Network Backup";
  document.querySelectorAll("[data-i18n]").forEach((el) => {
    el.textContent = t(el.dataset.i18n);
  });
  document.querySelectorAll("[data-i18n-placeholder]").forEach((el) => {
    el.placeholder = t(el.dataset.i18nPlaceholder);
  });
  document.querySelectorAll("[data-i18n-value]").forEach((el) => {
    if (el.dataset.i18nValue === "profile.defaultName" && !el.value) {
      el.value = t("profile.defaultName");
    }
  });
  document.querySelectorAll("[data-i18n-aria-label]").forEach((el) => {
    el.setAttribute("aria-label", t(el.dataset.i18nAriaLabel));
  });
  $("mode-hint").textContent = modeHint($("mode").value);
}

function applyLanguageToDynamic() {
  applyI18n();
  renderProfiles();
  if (state.preview) renderPreview(state.preview);
}

// ----------------------------------------------------------------
// Path helpers and UI state
// ----------------------------------------------------------------
function pathKey(p) {
  let s = String(p || "").trim().replace(/\//g, "\\");
  if (s.length === 3 && s[1] === ":" && s.endsWith("\\")) return s.toLowerCase();
  while (s.length > 1 && s.endsWith("\\")) s = s.slice(0, -1);
  return s.toLowerCase();
}

function newProfileId() {
  return (crypto && crypto.randomUUID) ? crypto.randomUUID() : `profile-${Date.now()}`;
}

function renderProfiles() {
  const list = $("profile-list");
  const empty = $("profiles-empty");
  const items = state.profiles.slice(0, 3);
  empty.hidden = items.length > 0;
  list.innerHTML = items.map((p) => `
    <div class="profile-item-row${p.id === state.profileId ? " active" : ""}">
      <button class="profile-item" data-profile-id="${esc(p.id)}">
        <span class="profile-item-name">${esc(p.name || t("profile.defaultName"))}</span>
        <span class="profile-item-path ltr">${esc(p.destination || "")}</span>
      </button>
      <button class="profile-delete" data-delete-id="${esc(p.id)}" aria-label="${t("profiles.deleteAria")}">×</button>
    </div>`).join("");
  list.querySelectorAll(".profile-item").forEach((btn) => {
    btn.onclick = () => selectProfile(btn.dataset.profileId);
  });
  list.querySelectorAll(".profile-delete").forEach((btn) => {
    btn.onclick = () => requestDeleteProfile(btn.dataset.deleteId);
  });
}

function requestDeleteProfile(id) {
  const p = state.profiles.find((x) => x.id === id);
  if (!p) return;
  const name = esc(p.name || t("profile.defaultName"));
  showBanner(
    "warn",
    `${t("banner.deleteProfile", { name })}` +
      `<div class="banner-actions">
         <button class="btn danger small" id="profile-delete-yes">${t("banner.deleteProfileYes")}</button>
         <button class="btn ghost small" id="profile-delete-no">${t("banner.deleteProfileNo")}</button>
       </div>`
  );
  $("profile-delete-yes").onclick = () => deleteConfirmedProfile(id);
  $("profile-delete-no").onclick = () => clearBanner();
}

async function deleteConfirmedProfile(id) {
  try {
    state.profiles = await invoke("delete_profile", { profileId: id });
    const deletedActive = state.profileId === id;
    state.profileId = null;
    if (deletedActive) {
      if (state.profiles.length) selectProfile(state.profiles[0].id);
      else newProfileForm();
    } else {
      renderProfiles();
    }
    showBanner("info", t("banner.saved"));
    setTimeout(clearBanner, 2500);
  } catch (e) {
    showBanner("error", esc(String(e)));
  }
}

async function reloadProfiles(selectId = state.profileId) {
  try {
    state.profiles = await invoke("load_profiles");
  } catch (e) {
    state.profiles = [];
    showBanner("error", `${t("error.saveProfile")}: ${esc(String(e))}`);
  }
  if (selectId && state.profiles.some((p) => p.id === selectId)) {
    state.profileId = selectId;
  }
  renderProfiles();
}

function selectProfile(id) {
  const p = state.profiles.find((x) => x.id === id);
  if (!p) return;
  fillForm(p);
  renderProfiles();
}

function newProfileForm() {
  state.profileId = null;
  fillForm({
    id: null,
    name: t("profile.defaultName"),
    source: "",
    destination: "",
    mode: "safe",
    excludes: DEFAULT_EXCLUDES.split("\n"),
    networkUser: "",
    threads: 8,
  });
  renderProfiles();
}

function modeHint(mode) {
  return mode === "mirror_recycle" ? t("mode.mirrorHint") : t("mode.safeHint");
}

function fillForm(p) {
  state.profileId = p.id || null;
  $("profile-name").value = p.name || t("profile.defaultName");
  $("source").value = p.source || "";
  $("destination").value = p.destination || "";
  $("mode").value = p.mode || "safe";
  $("threads").value = p.threads || 8;
  $("excludes").value = (p.excludes && p.excludes.length)
    ? p.excludes.join("\n")
    : DEFAULT_EXCLUDES;
  $("network-user").value = p.networkUser || "";
  $("schedule-enabled").checked = Boolean(p.scheduleEnabled);
  $("schedule-action").value = p.scheduleAction || "backup";
  $("schedule-frequency").value = p.scheduleFrequency || "daily";
  $("schedule-time").value = p.scheduleTime || "21:30";
  $("schedule-weekday").value = String(p.scheduleWeekday ?? 0);
  updateScheduleUI();
  applyI18n();
}

// applyI18n updates element placeholders/labels; renderProfiles and fillForm must
// run after it for dynamic translations.
function applyAll() {
  applyI18n();
  renderProfiles();
}

function updateScheduleUI() {
  const enabled = $("schedule-enabled").checked;
  $("schedule-body").hidden = !enabled;
  $("weekday-row").hidden = $("schedule-frequency").value !== "weekly";
}

function collectForm() {
  const excludes = $("excludes").value
    .split("\n")
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
  const threads = Math.max(1, Math.min(128, parseInt($("threads").value, 10) || 8));
  const user = $("network-user").value.trim();
  return {
    id: state.profileId || newProfileId(),
    name: $("profile-name").value.trim() || t("profile.defaultName"),
    source: $("source").value.trim(),
    destination: $("destination").value.trim(),
    mode: $("mode").value,
    excludes,
    networkUser: user || null,
    threads,
    scheduleEnabled: $("schedule-enabled").checked,
    scheduleAction: $("schedule-action").value,
    scheduleFrequency: $("schedule-frequency").value,
    scheduleTime: $("schedule-time").value,
    scheduleWeekday: parseInt($("schedule-weekday").value, 10) || 0,
  };
}

function validateProfile(profile) {
  if (!profile.source || !profile.destination) {
    return t("error.emptyPaths");
  }
  for (const p of profile.excludes) {
    if (!p) return t("error.emptyExclude");
    if (p.startsWith("/")) return t("error.excludeSlash", { pattern: p });
  }
  return null;
}

function validateSave(profile) {
  const base = validateProfile(profile);
  if (base) return base;
  if (profile.scheduleEnabled && !/^[0-2]\d:[0-5]\d$/.test(profile.scheduleTime || "")) {
    return t("error.scheduleTime");
  }
  if (pathKey(profile.source) === pathKey(profile.destination)) {
    return t("error.samePath");
  }
  const exists = state.profiles.some((p) => p.id === profile.id);
  if (!exists && state.profiles.length >= 3) {
    return t("error.maxProfiles");
  }
  return null;
}

function takePassword() {
  const field = $("network-password");
  if (field && field.value) {
    passwordMem = field.value;
    field.value = "";
  }
  return passwordMem || null;
}

async function checkPathInput(inputId, hintId) {
  const path = $(inputId).value.trim();
  const hint = $(hintId);
  if (!path) { hint.textContent = ""; hint.className = "hint"; return; }
  try {
    const res = await invoke("check_path", { path });
    if (res.exists && res.isDir) {
      hint.textContent = t("path.ok");
      hint.className = "hint ok";
    } else if (res.exists) {
      hint.textContent = t("path.notDir");
      hint.className = "hint err";
    } else if (res.error && (res.error.includes("The server is unreachable") || res.error.includes("سرور در دسترس نیست"))) {
      hint.textContent = t("path.unreachable");
      hint.className = "hint err";
    } else {
      hint.textContent = t("path.notFound");
      hint.className = "hint err";
    }
  } catch {
    hint.textContent = "";
    hint.className = "hint";
  }
}

function renderPreview(res) {
  state.preview = res;
  const panel = $("preview-panel");
  panel.hidden = false;
  $("summary-panel").hidden = true;

  const mirror = $("mode").value === "mirror_recycle";
  const group = (title, dotClass, files, show) => {
    if (!show) return "";
    const rows = files.slice(0, 300).map(
      (f) => `<div class="file-line ltr"><span class="size">${fmtBytes(f.size)}</span>${esc(f.path)}</div>`
    ).join("");
    const more = files.length > 300
      ? `<div class="file-line muted">… ${Number(files.length) - 300} ${t("preview.more")}</div>` : "";
    return `
      <div class="preview-group">
        <div class="preview-group-title"><span class="${dotClass}">●</span> ${title}
          <span class="count-badge">${Number(files.length) || 0}</span></div>
        <div class="preview-files">${rows || `<div class="file-line muted">${t("preview.none")}</div>`}${more}</div>
      </div>`;
  };

  const delCount = Number(res.deletedCount) || 0;
  panel.innerHTML = `
    <div class="card">
      <div class="card-head"><span class="card-title">${t("preview.title")}</span>
        <span class="muted">${t("preview.nothingChanged")}</span></div>
      ${group(t("preview.new"), "new-dot", res.newFiles || [], true)}
      ${group(t("preview.changed"), "changed-dot", res.changedFiles || [], true)}
      ${group(t("preview.deleted"), "deleted-dot", res.deletedFiles || [], mirror)}
      <div class="preview-total">${t("preview.total")} <b>${fmtBytes(res.totalBytes)}</b>
        ${mirror && delCount ? ` — ${delCount} ${t("summary.moved")}` : ""}</div>
    </div>`;

  reveal(panel);
}

async function onPreview() {
  clearBanner();
  $("preview-panel").hidden = true;
  $("summary-panel").hidden = true;
  const profile = collectForm();
  const err = validateProfile(profile);
  if (err) {
    showBanner("error", esc(err));
    return;
  }
  const btn = $("btn-preview");
  setLoading(btn, true);
  try {
    const res = await invoke("preview_changes", {
      profile,
      password: takePassword(),
    });
    renderPreview(res);
  } catch (e) {
    showBanner("error", esc(String(e)));
  } finally {
    setLoading(btn, false);
  }
}

function collapseAdvancedForRun() {
  const adv = $("advanced");
  if (adv) {
    state.advancedWasOpen = adv.open;
    adv.open = false;
  }
}

function restoreAdvancedAfterRun() {
  const adv = $("advanced");
  if (adv) adv.open = Boolean(state.advancedWasOpen);
}

async function onStart() {
  clearBanner();
  const profile = collectForm();
  const err = validateProfile(profile);
  if (err) {
    showBanner("error", esc(err));
    return;
  }

  if (profile.mode === "mirror_recycle" && !state.mirrorAcked) {
    showBanner("warn",
      t("banner.mirrorAck") +
      `<div class="banner-actions">
         <button class="btn primary small" id="ack-yes">${t("banner.mirrorAckYes")}</button>
         <button class="btn ghost small" id="ack-no">${t("banner.mirrorAckNo")}</button>
       </div>`);
    $("ack-yes").onclick = () => {
      state.mirrorAcked = true;
      localStorage.setItem("mirror_ack", "1");
      clearBanner();
      onStart();
    };
    $("ack-no").onclick = () => clearBanner();
    return;
  }

  const btn = $("btn-start");
  setLoading(btn, true);
  $("btn-preview").disabled = true;
  state.pendingRun = true;
  state.totalFiles = state.preview
    ? (Number(state.preview.newCount) || 0) + (Number(state.preview.changedCount) || 0)
    : null;

  collapseAdvancedForRun();
  showRunPanel();

  const bar = $("progressbar");
  const fill = $("progress-fill");
  bar.classList.add("active");
  if (state.totalFiles && state.totalFiles > 0) {
    fill.classList.remove("indeterminate");
    fill.classList.add("determinate");
    fill.style.width = "0%";
  } else {
    fill.classList.remove("determinate");
    fill.classList.add("indeterminate");
    fill.style.width = "";
  }

  const allowMassDelete = state.allowMassDelete;
  state.allowMassDelete = false;

  try {
    const runId = await invoke("start_backup", {
      profile,
      password: takePassword(),
      allowMassDelete,
    });
    state.runId = runId;
    state.pendingRun = false;
    state.startedAt = Date.now();
    state.elapsedTimer = setInterval(() => {
      const el = $("stat-elapsed");
      if (el) el.textContent = fmtElapsed(Date.now() - state.startedAt);
    }, 1000);
  } catch (e) {
    state.pendingRun = false;
    state.runId = null;
    hideRunPanel();
    resetProgressBar();
    setLoading(btn, false);
    $("btn-preview").disabled = false;
    restoreAdvancedAfterRun();

    const msg = String(e);
    if (msg.startsWith("MASS_DELETE_JSON:")) {
      try {
        const d = JSON.parse(msg.slice("MASS_DELETE_JSON:".length));
        const total = Number(d.total) || 0;
        const extras = Number(d.extras) || 0;
        showBanner("warn",
          t("banner.massDelete", { total, extras }) +
          `<div class="banner-actions">
             <button class="btn danger small" id="mass-yes">${t("banner.massDeleteYes")}</button>
             <button class="btn ghost small" id="mass-no">${t("banner.massDeleteNo")}</button>
           </div>`);
        $("mass-yes").onclick = () => {
          state.allowMassDelete = true;
          clearBanner();
          onStart();
        };
        $("mass-no").onclick = () => clearBanner();
        return;
      } catch {
        showBanner("error", esc(msg));
        return;
      }
    }
    showBanner("error", esc(msg));
  }
}

function showRunPanel() {
  const panel = $("run-panel");
  panel.hidden = false;
  $("preview-panel").hidden = true;
  $("summary-panel").hidden = true;
  panel.innerHTML = `
    <div class="card">
      <div class="card-head">
        <span class="card-title">${t("run.title")}</span>
        <button id="btn-cancel" class="btn danger small">${t("run.cancel")}</button>
      </div>
      <div class="run-stats">
        <span>${t("run.files")} <b id="stat-files">0</b>${state.totalFiles ? ` / ${Number(state.totalFiles)}` : ""}</span>
        <span>${t("run.bytes")} <b id="stat-bytes">0 B</b></span>
        <span>${t("run.time")} <b id="stat-elapsed">00:00</b></span>
      </div>
      <div class="current-file ltr" id="current-file">—</div>
      <div class="log-box" id="log-box"></div>
    </div>`;
  $("btn-cancel").onclick = onCancel;

  const box = $("log-box");
  box.addEventListener("scroll", () => {
    const nearBottom = box.scrollTop + box.clientHeight >= box.scrollHeight - 30;
    state.autoscroll = nearBottom;
  });

  reveal(panel);
}

function hideRunPanel() {
  $("run-panel").hidden = true;
  $("run-panel").innerHTML = "";
}

async function onCancel() {
  if (!state.runId && !state.pendingRun) return;
  const btn = $("btn-cancel");
  if (btn) { btn.disabled = true; btn.textContent = t("run.cancelling"); }
  try { await invoke("cancel_backup", { runId: state.runId || null }); } catch {}
}

const MAX_LOG_NODES = 1000;
const ERROR_LINE_RE = /\bERROR\s+\d+\s+\(0x/;

function appendLogBatch(lines) {
  const box = $("log-box");
  if (!box || !Array.isArray(lines) || !lines.length) return;
  const frag = document.createDocumentFragment();
  for (const line of lines) {
    if (!line || !line.trim()) continue;
    const div = document.createElement("div");
    div.className = "log-line";
    if (ERROR_LINE_RE.test(line)) div.classList.add("err");
    else if (line.startsWith("♻") || line.startsWith("⚠")) div.classList.add("move");
    else if (/^\s*(ROBOCOPY|----|Options|Started|Source|Dest|Files|Dirs|Ended|Total|Bytes|Times|Speed)/.test(line))
      div.classList.add("dim");
    div.textContent = line;
    frag.appendChild(div);
  }
  box.appendChild(frag);
  while (box.childElementCount > MAX_LOG_NODES) {
    box.removeChild(box.firstChild);
  }
  if (state.autoscroll) box.scrollTop = box.scrollHeight;
}

function renderSummary(s) {
  clearInterval(state.elapsedTimer);
  hideRunPanel();
  // C5: set the bar to 100% determinate from the summary
  const bar = $("progressbar");
  const fill = $("progress-fill");
  bar.classList.add("active");
  if (fill) {
    fill.classList.remove("indeterminate");
    fill.classList.add("determinate");
    fill.style.width = "100%";
  }
  $("btn-start").disabled = false;
  $("btn-start").classList.remove("loading");
  $("btn-preview").disabled = false;
  restoreAdvancedAfterRun();

  const cls = safeResultClass(s.result);
  const badgeText = {
    success: t("status.success"),
    warnings: t("status.warnings"),
    failed: t("status.failed"),
    cancelled: t("status.cancelled"),
  }[cls];

  const copied = Number(s.copied) || 0;
  const skipped = Number(s.skipped) || 0;
  const failed = Number(s.failed) || 0;
  const moved = Number(s.movedToDeleted) || 0;

  const panel = $("summary-panel");
  panel.hidden = false;
  panel.innerHTML = `
    <div class="card">
      <div class="summary-head">
        <span class="card-title">${t("summary.title")}</span>
        <span class="badge ${cls}">${badgeText}</span>
      </div>
      <div class="summary-grid">
        <div class="summary-item"><div class="label">${t("summary.copied")}</div><div class="value">${copied}</div></div>
        <div class="summary-item"><div class="label">${t("summary.skipped")}</div><div class="value">${skipped}</div></div>
        <div class="summary-item"><div class="label">${t("summary.failed")}</div><div class="value">${failed}</div></div>
        ${moved ? `<div class="summary-item"><div class="label">${t("summary.moved")}</div><div class="value">${moved}</div></div>` : ""}
        <div class="summary-item"><div class="label">${t("summary.bytes")}</div><div class="value">${fmtBytes(s.bytes)}</div></div>
        <div class="summary-item"><div class="label">${t("summary.duration")}</div><div class="value">${fmtElapsed((Number(s.durationSecs) || 0) * 1000)}</div></div>
      </div>
      ${cls === "cancelled" ? `<div class="summary-note">${t("summary.cancelledNote")}</div>` : ""}
      ${cls === "failed" && Array.isArray(s.lastLines) && s.lastLines.length
        ? `<div class="summary-lastlines ltr">${esc(s.lastLines.join("\n"))}</div>` : ""}
    </div>`;

  state.runId = null;
  reveal(panel);
}

function wireEvents() {
  listen("backup://log", (e) => {
    const { runId, lines, line } = e.payload || {};
    if (runId === state.runId || (state.pendingRun && !state.runId)) {
      if (Array.isArray(lines)) appendLogBatch(lines);
      else if (typeof line === "string") appendLogBatch([line]);
    }
  });

  listen("backup://progress", (e) => {
    const p = e.payload || {};
    if (p.runId !== state.runId) return;
    const filesDone = Number(p.filesDone) || 0;
    const bytesDone = Number(p.bytesDone) || 0;
    const f = $("stat-files"), b = $("stat-bytes"), c = $("current-file");
    if (f) f.textContent = filesDone;
    if (b) b.textContent = fmtBytes(bytesDone);
    if (c) c.textContent = p.currentFile || "—";

    if (state.totalFiles && state.totalFiles > 0) {
      const fill = $("progress-fill");
      if (fill) {
        fill.classList.remove("indeterminate");
        fill.classList.add("determinate");
        fill.style.width = `${Math.min(100, Math.round((filesDone / state.totalFiles) * 100))}%`;
      }
    }
  });

  listen("backup://finished", (e) => {
    const s = e.payload;
    if (s && (s.runId === state.runId || state.pendingRun)) {
      renderSummary(s);
    }
    state.pendingRun = false;
  });
}

const RESULT_FA = () => ({
  success: t("status.shortSuccess"),
  warnings: t("status.shortWarnings"),
  failed: t("status.shortFailed"),
  cancelled: t("status.shortCancelled"),
});

async function loadHistory() {
  const list = $("history-list");
  list.innerHTML = "";
  let runs = [];
  try {
    runs = await invoke("list_history");
  } catch (e) {
    $("history-empty").hidden = false;
    $("history-empty").textContent = String(e);
    return;
  }
  $("history-empty").textContent = t("history.empty");
  $("history-empty").hidden = runs.length > 0;
  const labels = RESULT_FA();
  for (const r of runs) {
    const cls = safeResultClass(r.result);
    const copied = Number(r.copied) || 0;
    const failed = Number(r.failed) || 0;
    const row = document.createElement("div");
    row.className = "history-row";
    row.innerHTML = `
      <span class="h-date">${esc(r.startedAt)}</span>
      <span class="badge ${cls}">${labels[cls] || "—"}</span>
      <span class="h-name">${esc(r.profileName)} <span class="muted">→ ${esc(r.destination)}</span></span>
      <span class="h-counts">${copied} ${t("history.copied")} · ${failed} ${t("history.failed")}</span>
      <span class="h-duration">${fmtElapsed((Number(r.durationSecs) || 0) * 1000)}</span>`;
    row.onclick = () => openRunLog(r.runId, r.startedAt);
    list.appendChild(row);
  }
}

async function openRunLog(runId, date) {
  try {
    const log = await invoke("read_log", { runId });
    let moved = null;
    try { moved = await invoke("read_moved", { runId }); } catch {}
    $("log-dialog-title").textContent = `${t("log.title")} — ${date}`;
    $("log-dialog-body").textContent = moved
      ? `${log}\n\n---- ${t("log.movedSection")} ----\n${moved}`
      : log;
    $("log-overlay").hidden = false;
  } catch (e) {
    switchView("backup");
    showBanner("error", esc(String(e)));
  }
}

function switchView(name) {
  document.querySelectorAll(".nav-item").forEach((b) => {
    b.classList.toggle("active", b.dataset.view === name);
  });
  document.querySelectorAll(".view").forEach((v) => {
    v.classList.toggle("active", v.id === `view-${name}`);
  });
}

function wireUI() {
  document.querySelectorAll(".nav-item").forEach((btn) => {
    btn.onclick = () => {
      switchView(btn.dataset.view);
      if (btn.dataset.view === "history") loadHistory();
    };
  });

  $("browse-source").onclick = browseFor("source");
  $("browse-dest").onclick = browseFor("destination");
  $("source").addEventListener("change", () => checkPathInput("source", "source-status"));
  $("destination").addEventListener("change", () => checkPathInput("destination", "dest-status"));
  $("source").addEventListener("blur", () => checkPathInput("source", "source-status"));
  $("destination").addEventListener("blur", () => checkPathInput("destination", "dest-status"));

  $("network-password").addEventListener("input", () => {
    passwordMem = $("network-password").value || null;
  });
  $("network-password").addEventListener("blur", () => {
    if ($("network-password").value) {
      passwordMem = $("network-password").value;
      $("network-password").value = "";
    }
  });
  $("network-user").addEventListener("input", () => {
    passwordMem = null;
    $("network-password").value = "";
  });
  window.addEventListener("pagehide", () => { passwordMem = null; });

  $("mode").onchange = () => {
    $("mode-hint").textContent = modeHint($("mode").value);
  };

  $("schedule-enabled").addEventListener("change", updateScheduleUI);
  $("schedule-frequency").addEventListener("change", updateScheduleUI);

  $("auto-start").addEventListener("change", async () => {
    const el = $("auto-start");
    const enabled = el.checked;
    try {
      const settings = await invoke("set_auto_start", { enabled });
      el.checked = Boolean(settings.autoStart);
      showBanner("info", settings.autoStart
        ? (LANG === "fa" ? "راه‌اندازی خودکار فعال شد ✓" : "Launch on startup enabled ✓")
        : (LANG === "fa" ? "راه‌اندازی خودکار غیرفعال شد ✓" : "Launch on startup disabled ✓"));
      setTimeout(clearBanner, 2500);
    } catch (e) {
      el.checked = !enabled;
      showBanner("error", esc(String(e)));
    }
  });

  $("btn-language").onclick = async () => {
    const next = LANG === "fa" ? "en" : "fa";
    try {
      const settings = await invoke("set_language", { language: next });
      LANG = settings.language === "fa" ? "fa" : "en";
      applyAll();
      showBanner("info", LANG === "fa" ? "زبان به فارسی تغییر کرد ✓" : "Language switched to English ✓");
      setTimeout(clearBanner, 2500);
    } catch (e) {
      showBanner("error", esc(String(e)));
    }
  };

  $("btn-preview").onclick = onPreview;
  $("btn-start").onclick = onStart;
  $("btn-new-profile").onclick = newProfileForm;

  $("btn-save-profile").onclick = async () => {
    const profile = collectForm();
    const err = validateSave(profile);
    if (err) {
      showBanner("error", esc(err));
      return;
    }
    try {
      await invoke("save_profile", { profile });
      state.profileId = profile.id;
      await reloadProfiles(profile.id);
      showBanner("info", t("banner.saved"));
      setTimeout(clearBanner, 2500);
    } catch (e) {
      showBanner("error", esc(String(e)));
    }
  };

  $("btn-close-log").onclick = () => { $("log-overlay").hidden = true; };
  $("log-overlay").onclick = (e) => {
    if (e.target === $("log-overlay")) $("log-overlay").hidden = true;
  };
}

function browseFor(inputId) {
  return async () => {
    try {
      const dir = await openDialog({ directory: true, multiple: false });
      if (dir) {
        $(inputId).value = dir;
        const hintId = inputId === "source" ? "source-status" : "dest-status";
        checkPathInput(inputId, hintId);
      }
    } catch {}
  };
}

async function init() {
  wireUI();
  wireEvents();

  try {
    const settings = await invoke("get_settings");
    LANG = settings.language === "fa" ? "fa" : "en";
  } catch {
    LANG = "en";
  }
  $("btn-language").textContent = t("language.switch");
  $("auto-start").checked = false;
  applyAll();

  try {
    const settingsAgain = await invoke("get_settings");
    $("auto-start").checked = Boolean(settingsAgain.autoStart);
  } catch {}

  try {
    const profiles = await invoke("load_profiles");
    state.profiles = profiles.slice(0, 3);
    if (state.profiles.length) {
      selectProfile(state.profiles[0].id);
    } else {
      newProfileForm();
    }
  } catch (e) {
    newProfileForm();
    showBanner("error", `${t("error.saveProfile")}: ${esc(String(e))}`);
  }
}

init();

