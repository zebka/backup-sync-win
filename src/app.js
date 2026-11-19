// Backup Sync — منطق فرانت‌اند (vanilla JS، بدون باندلر)
// با window.__TAURI__ (withGlobalTauri) به بک‌اند Rust وصل می‌شود.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { open: openDialog } = window.__TAURI__.dialog;

// ------------------------------------------------------------------
// وضعیت کلی
// ------------------------------------------------------------------
const state = {
  profileId: null,
  profiles: [],
  runId: null,            // اجرای فعال
  pendingRun: false,      // بین کلیک شروع و رسیدن runId (فاز آماده‌سازی)
  preview: null,          // آخرین PreviewResult
  startedAt: null,
  elapsedTimer: null,
  totalFiles: null,       // از پیش‌نمایش
  autoscroll: true,
  mirrorAcked: localStorage.getItem("mirror_ack") === "1",
  allowMassDelete: false, // تأیید نگهبان حذف دسته‌جمعی (P0-6b)
  advancedWasOpen: false, // بازگردانی وضعیت بخش پیشرفته بعد از اجرا (P0-8)
};

// رمز فقط در متغیر ماژول (حافظه) می‌ماند — هرگز در DOM بعد از خواندن و هرگز در localStorage (P0-5)
let passwordMem = null;

const $ = (id) => document.getElementById(id);

const DEFAULT_EXCLUDES = "Thumbs.db\n~$*\n*.tmp\ndesktop.ini";

const MODE_HINTS = {
  safe: "فقط فایل‌های جدید یا تغییرکرده کپی می‌شوند. هیچ‌چیز حذف نمی‌شود.",
  mirror_recycle:
    "مقصد مطابق منبع می‌شود، اما فایل‌هایی که از منبع حذف شده‌اند به پوشه _Deleted در مقصد منتقل می‌شوند — نه حذف قطعی.",
};

const RESULT_CLASS = {
  success: "success",
  warnings: "warnings",
  failed: "failed",
  cancelled: "cancelled",
};

const RESULT_FA = {
  success: "موفق",
  warnings: "با هشدار",
  failed: "شکست",
  cancelled: "لغو",
};

// ------------------------------------------------------------------
// کمکی‌ها
// ------------------------------------------------------------------
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

// P1-12: escape کامل کاراکترهای HTML به‌همراه " و '
function esc(s) {
  return String(s)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function safeResultClass(r) {
  return RESULT_CLASS[r] || "failed";
}

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

// ------------------------------------------------------------------
// فرم پروفایل و اعتبارسنجی
// ------------------------------------------------------------------
function pathKey(p) {
  let s = String(p || "").trim().replace(/\//g, "\\");
  // ریشهٔ درایو "X:\" دست‌نخورده؛ بقیهٔ بک‌اسلش‌ها حذف می‌شوند
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
        <span class="profile-item-name">${esc(p.name || "پیش‌فرض")}</span>
        <span class="profile-item-path ltr">${esc(p.destination || "")}</span>
      </button>
      <button class="profile-delete" data-delete-id="${esc(p.id)}" aria-label="حذف پروفایل">×</button>
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
  showBanner("warn",
    `آیا پروفایل «${esc(p.name || "پیش‌فرض")}» حذف شود؟ فایل‌های منبع و مقصد حذف نمی‌شوند.` +
    `<div class="banner-actions">
       <button class="btn danger small" id="profile-delete-yes">بله، حذف شود</button>
       <button class="btn ghost small" id="profile-delete-no">انصراف</button>
     </div>`);
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
    showBanner("info", "پروفایل حذف شد ✓");
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
    showBanner("error", "خواندن پروفایل‌ها شکست خورد: " + esc(String(e)));
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
    name: "پیش‌فرض",
    source: "",
    destination: "",
    mode: "safe",
    excludes: DEFAULT_EXCLUDES.split("\n"),
    networkUser: "",
    threads: 8,
  });
  renderProfiles();
}

function updateScheduleUI() {
  const enabled = $("schedule-enabled").checked;
  $("schedule-body").hidden = !enabled;
  $("weekday-row").hidden = $("schedule-frequency").value !== "weekly";
}

function fillForm(p) {
  state.profileId = p.id || null;
  $("profile-name").value = p.name || "پیش‌فرض";
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
  $("mode-hint").textContent = MODE_HINTS[$("mode").value] || MODE_HINTS.safe;
}

function collectRawExcludes() {
  return $("excludes").value
    .split("\n")
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

function collectForm() {
  const excludes = collectRawExcludes();
  const threads = Math.max(1, Math.min(128, parseInt($("threads").value, 10) || 8));
  const user = $("network-user").value.trim();
  return {
    id: state.profileId || newProfileId(),
    name: $("profile-name").value.trim() || "پیش‌فرض",
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

// P1-12: اعتبارسنجی در save و start (و پیش‌نمایش)
function validateProfile(profile) {
  if (!profile.source || !profile.destination) {
    return "مسیر منبع و مقصد را وارد کن.";
  }
  const rawLines = $("excludes").value.split("\n");
  // رد کردن خطوطی که فقط فاصله دارند ممنوع نیست (اگر بین خطوط باشد)، اما اگر با / شروع شود خطا بده
  for (const p of profile.excludes) {
    if (!p) return "الگوی حذف نمی‌تواند خالی باشد.";
    if (p.startsWith("/")) {
      return `الگوی حذف با «/» شروع می‌شود و به‌عنوان سوییچ robocopy خوانده می‌شود: ${p}`;
    }
  }
  // اگر کاربر عمداً تنها یک خط خالی گذاشته نیست، اوکی است
  void rawLines;
  return null;
}

function validateSave(profile) {
  const base = validateProfile(profile);
  if (base) return base;
  if (profile.scheduleEnabled && !/^[0-2]\d:[0-5]\d$/.test(profile.scheduleTime || "")) {
    return "ساعت زمان‌بندی نامعتبر است (مثال 21:30).";
  }
  if (pathKey(profile.source) === pathKey(profile.destination)) {
    return "مسیر منبع و مقصد نمی‌توانند یکسان باشند.";
  }
  const exists = state.profiles.some((p) => p.id === profile.id);
  if (!exists && state.profiles.length >= 3) {
    return "حداکثر سه پروفایل قابل ذخیره است.";
  }
  return null;
}

// P0-5: رمز در حافظه می‌ماند؛ پس از خواندن از DOM پاک می‌شود
function takePassword() {
  const field = $("network-password");
  if (field && field.value) {
    passwordMem = field.value;
    field.value = "";
  }
  return passwordMem || null;
}

// ------------------------------------------------------------------
// چک مسیر با تایپ
// ------------------------------------------------------------------
async function checkPathInput(inputId, hintId) {
  const path = $(inputId).value.trim();
  const hint = $(hintId);
  if (!path) { hint.textContent = ""; hint.className = "hint"; return; }
  try {
    const res = await invoke("check_path", { path });
    if (res.exists && res.isDir) {
      hint.textContent = "✓ پوشه موجود است";
      hint.className = "hint ok";
    } else if (res.exists) {
      hint.textContent = "✖ این مسیر پوشه نیست";
      hint.className = "hint err";
    } else if (res.error && res.error.includes("سرور در دسترس نیست")) {
      hint.textContent = "✖ سرور در دسترس نیست";
      hint.className = "hint err";
    } else {
      hint.textContent = "✖ پیدا نشد — مسیر یا دسترسی را چک کن";
      hint.className = "hint err";
    }
  } catch {
    hint.textContent = "";
    hint.className = "hint";
  }
}

// ------------------------------------------------------------------
// پیش‌نمایش تغییرات
// ------------------------------------------------------------------
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
      ? `<div class="file-line muted">… و ${Number(files.length) - 300} مورد دیگر</div>` : "";
    return `
      <div class="preview-group">
        <div class="preview-group-title"><span class="${dotClass}">●</span> ${title}
          <span class="count-badge">${Number(files.length) || 0}</span></div>
        <div class="preview-files">${rows || '<div class="file-line muted">— هیچ</div>'}${more}</div>
      </div>`;
  };

  const delCount = Number(res.deletedCount) || 0;
  panel.innerHTML = `
    <div class="card">
      <div class="card-head"><span class="card-title">پیش‌نمایش تغییرات</span>
        <span class="muted">هیچ فایلی هنوز تغییر نکرده است</span></div>
      ${group("فایل‌های جدید", "new-dot", res.newFiles || [], true)}
      ${group("فایل‌های تغییرکرده", "changed-dot", res.changedFiles || [], true)}
      ${group("حذف‌شده در منبع", "deleted-dot", res.deletedFiles || [], mirror)}
      <div class="preview-total">مجموع حجم قابل کپی: <b>${fmtBytes(res.totalBytes)}</b>
        ${mirror && delCount ? ` — ${delCount} فایل به _Deleted منتقل خواهد شد` : ""}</div>
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

// ------------------------------------------------------------------
// شروع بکاپ و رویدادهای زنده
// ------------------------------------------------------------------
function collapseAdvancedForRun() {
  const adv = $("advanced");
  if (adv) {
    state.advancedWasOpen = adv.open;
    adv.open = false;
  }
}

function restoreAdvancedAfterRun() {
  const adv = $("advanced");
  if (adv) {
    adv.open = Boolean(state.advancedWasOpen);
  }
}

async function onStart() {
  clearBanner();
  const profile = collectForm();
  const err = validateProfile(profile);
  if (err) {
    showBanner("error", esc(err));
    return;
  }

  // تأیید بار اول حالت آینه (PLAN §5)
  if (profile.mode === "mirror_recycle" && !state.mirrorAcked) {
    showBanner("warn",
      `حالت آینه: فایل‌هایی که از منبع حذف شوند از مقصد <b>حذف نمی‌شوند</b>، بلکه به ` +
      `<span class="mono ltr">_Deleted\\&lt;تاریخ&gt;</span> منتقل می‌شوند. مطمئنی؟` +
      `<div class="banner-actions">
         <button class="btn primary small" id="ack-yes">بله، ادامه بده</button>
         <button class="btn ghost small" id="ack-no">نه، برگرد</button>
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
          `هشدار حذف دسته‌جمعی (حالت آینه): منبع <b>${total}</b> فایل دارد، ولی <b>${extras}</b> فایل در مقصد هست ` +
          `که در منبع پیدا نشدند. اگر منبع اشتباه یا خالی باشد، همه به <span class="mono ltr">_Deleted</span> منتقل می‌شوند. ادامه می‌دهی؟` +
          `<div class="banner-actions">
             <button class="btn danger small" id="mass-yes">بله، به _Deleted منتقل کن</button>
             <button class="btn ghost small" id="mass-no">نه، لغو کن</button>
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
        <span class="card-title">بکاپ در حال اجرا…</span>
        <button id="btn-cancel" class="btn danger small">لغو</button>
      </div>
      <div class="run-stats">
        <span>فایل: <b id="stat-files">0</b>${state.totalFiles ? ` / ${Number(state.totalFiles)}` : ""}</span>
        <span>حجم: <b id="stat-bytes">0 B</b></span>
        <span>زمان: <b id="stat-elapsed">00:00</b></span>
      </div>
      <div class="current-file ltr" id="current-file">—</div>
      <div class="log-box" id="log-box"></div>
    </div>`;
  $("btn-cancel").onclick = onCancel;

  const box = $("log-box");
  box.addEventListener("scroll", () => {
    const nearBottom =
      box.scrollTop + box.clientHeight >= box.scrollHeight - 30;
    state.autoscroll = nearBottom;
  });

  reveal(panel);
}

function hideRunPanel() {
  $("run-panel").hidden = true;
  $("run-panel").innerHTML = "";
}

async function onCancel() {
  // P0-4: حتی وقتی pendingRun است و runId هنوز برنگشته هم باید لغو کار کند
  if (!state.runId && !state.pendingRun) return;
  const btn = $("btn-cancel");
  if (btn) { btn.disabled = true; btn.textContent = "در حال لغو…"; }
  try { await invoke("cancel_backup", { runId: state.runId || null }); } catch {}
}

// P1-8 + P0-3: درج دسته‌ای با DocumentFragment و سقف ۱۰۰۰ گره؛
// هایلایت خطا فقط روی الگوی دقیق \bERROR\s+\d+\s+\(0x
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

// ------------------------------------------------------------------
// کارت خلاصه نهایی
// ------------------------------------------------------------------
function renderSummary(s) {
  clearInterval(state.elapsedTimer);
  hideRunPanel();
  // C5: پایان اجرا → نوار پیشرفت روی ۱۰۰٪ گام‌به‌گام (ولو این‌که خطوط لاگ
  // زیر /MT:8 گم شده باشند، تعداد نهایی از جدول خلاصه می‌آید)
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
    success: "موفق",
    warnings: "کامل شد (با هشدار)",
    failed: "شکست خورد",
    cancelled: "لغو شد",
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
        <span class="card-title">نتیجه اجرا</span>
        <span class="badge ${cls}">${badgeText}</span>
      </div>
      <div class="summary-grid">
        <div class="summary-item"><div class="label">کپی‌شده</div><div class="value">${copied}</div></div>
        <div class="summary-item"><div class="label">ردشده</div><div class="value">${skipped}</div></div>
        <div class="summary-item"><div class="label">ناموفق</div><div class="value">${failed}</div></div>
        ${moved ? `<div class="summary-item"><div class="label">به _Deleted منتقل شد</div><div class="value">${moved}</div></div>` : ""}
        <div class="summary-item"><div class="label">حجم کپی</div><div class="value">${fmtBytes(s.bytes)}</div></div>
        <div class="summary-item"><div class="label">مدت</div><div class="value">${fmtElapsed((Number(s.durationSecs) || 0) * 1000)}</div></div>
      </div>
      ${cls === "cancelled"
        ? `<div class="summary-note">لغو شد — ممکن است فایل‌های نیمه‌تمام در مقصد باقی مانده باشند؛ با اجرای مجدد بکاپ این فایل‌ها ترمیم می‌شوند.</div>`
        : ""}
      ${cls === "failed" && Array.isArray(s.lastLines) && s.lastLines.length
        ? `<div class="summary-lastlines ltr">${esc(s.lastLines.join("\n"))}</div>` : ""}
    </div>`;

  state.runId = null;
  reveal(panel);
}

// ------------------------------------------------------------------
// رویدادهای بک‌اند
// ------------------------------------------------------------------
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
        const pct = Math.min(100, Math.round((filesDone / state.totalFiles) * 100));
        fill.style.width = `${pct}%`;
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

// ------------------------------------------------------------------
// تاریخچه
// ------------------------------------------------------------------
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
  $("history-empty").textContent = "هنوز اجرایی ثبت نشده است.";
  $("history-empty").hidden = runs.length > 0;
  for (const r of runs) {
    const cls = safeResultClass(r.result);
    const copied = Number(r.copied) || 0;
    const failed = Number(r.failed) || 0;
    const row = document.createElement("div");
    row.className = "history-row";
    row.innerHTML = `
      <span class="h-date">${esc(r.startedAt)}</span>
      <span class="badge ${cls}">${RESULT_FA[cls] || "شکست"}</span>
      <span class="h-name">${esc(r.profileName)} <span class="muted">→ ${esc(r.destination)}</span></span>
      <span class="h-counts">${copied} کپی · ${failed} ناموفق</span>
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
    $("log-dialog-title").textContent = `لاگ اجرا — ${date}`;
    $("log-dialog-body").textContent = moved
      ? `${log}\n\n---- فایل‌های منتقل‌شده به _Deleted ----\n${moved}`
      : log;
    $("log-overlay").hidden = false;
  } catch (e) {
    // P2: به‌جای alert() خطا را در بنر نشان بده
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

// ------------------------------------------------------------------
// ناوبری و اتصال رویدادهای UI
// ------------------------------------------------------------------
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

  // P0-5: نگه داشتن رمز فقط در حافظه + پاک شدن با تغییر کاربر یا بستن برنامه
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
    $("mode-hint").textContent = MODE_HINTS[$("mode").value] || MODE_HINTS.safe;
  };

  $("schedule-enabled").addEventListener("change", updateScheduleUI);
  $("schedule-frequency").addEventListener("change", updateScheduleUI);
  $("auto-start").addEventListener("change", async () => {
    const el = $("auto-start");
    const enabled = el.checked;
    try {
      const actual = await invoke("set_auto_start", { enabled });
      el.checked = Boolean(actual);
      showBanner("info", actual ? "راه‌اندازی خودکار فعال شد ✓" : "راه‌اندازی خودکار غیرفعال شد ✓");
      setTimeout(clearBanner, 2500);
    } catch (e) {
      el.checked = !enabled;
      showBanner("error", esc(String(e)));
    }
  });

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
      showBanner("info", "پروفایل ذخیره شد ✓");
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

// ------------------------------------------------------------------
// شروع
// ------------------------------------------------------------------
async function init() {
  wireUI();
  wireEvents();
  try {
    $("auto-start").checked = Boolean(await invoke("get_auto_start"));
  } catch {
    $("auto-start").checked = false;
  }
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
    showBanner("error", "خواندن پروفایل‌ها شکست خورد: " + esc(String(e)));
  }
}

init();
