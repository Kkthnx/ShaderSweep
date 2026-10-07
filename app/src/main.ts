import "./styles.css";
import {
  cancelClean,
  cancelRestart,
  clean,
  onProgress,
  onScanStep,
  openLink,
  restartPc,
  reveal,
  scan,
} from "./bridge";
import {
  escapeHtml,
  formatBytes,
  formatDate,
  formatDuration,
  plural,
  timeAgo,
} from "./format";
import { icons } from "./icons";
import type {
  CleanResult,
  GroupId,
  LinkName,
  ProviderScan,
  QueueCheck,
  ScanResult,
  ScanStep,
} from "./types";

type Phase = "scanning" | "ready" | "cleaning" | "done" | "error";

/** How long Windows counts down before it restarts. */
const RESTART_SECONDS = 60;

/** How many recent file names the activity feed keeps. */
const ACTIVITY_LINES = 5;

const GROUPS: { id: GroupId; title: string; hint: string; hideMissing: boolean }[] = [
  { id: "shaders", title: "Shader caches", hint: "Rebuilt by your GPU driver and Windows", hideMissing: false },
  { id: "launchers", title: "Game launchers and games", hint: "Saved web pages and images. Logins stay", hideMissing: true },
  { id: "housekeeping", title: "Windows housekeeping", hint: "General cleanup. Anything risky starts off", hideMissing: true },
];

type RestartState =
  | { kind: "counting"; endsAt: number }
  | { kind: "error"; message: string }
  | null;

const state = {
  phase: "scanning" as Phase,
  scan: null as ScanResult | null,
  scanStep: null as ScanStep | null,
  result: null as CleanResult | null,
  error: "",
  selected: new Set<string>(),
  expanded: new Set<string>(),
  openGroups: new Set<GroupId>(["shaders"]),
  progress: new Map<string, { done: boolean; freed: number; files: number }>(),
  activity: [] as string[],
  startedAt: 0,
  now: Date.now(),
  cancelling: false,
  preview: false,
  queue: true,
  copied: false,
  /** What the last restart did to the queued files, kept until dismissed. */
  settled: null as QueueCheck | null,
  restart: null as RestartState,
};

const $ = <T extends HTMLElement>(id: string): T => document.getElementById(id) as T;
const el = {
  brand: $("brand"),
  rescan: $<HTMLButtonElement>("rescan"),
  gpu: $("gpu"),
  notice: $("notice"),
  summary: $("summary"),
  list: $("list"),
  credit: $("credit"),
  scroll: document.querySelector(".scroll") as HTMLElement,
  go: $<HTMLButtonElement>("go"),
  preview: $<HTMLButtonElement>("opt-preview"),
  queue: $<HTMLButtonElement>("opt-queue"),
  links: $("links"),
  dialog: $<HTMLDialogElement>("restart-dialog"),
  confirm: $<HTMLButtonElement>("restart-confirm"),
};

// Remembered between runs. Storage can be unavailable, so it is never required.
const store = {
  get(key: string, fallback: boolean): boolean {
    try {
      const raw = localStorage.getItem(`shadersweep.${key}`);
      return raw === null ? fallback : raw === "1";
    } catch {
      return fallback;
    }
  },
  set(key: string, value: boolean): void {
    try {
      localStorage.setItem(`shadersweep.${key}`, value ? "1" : "0");
    } catch {
      /* the choice just will not be remembered */
    }
  },
};

const busy = () => state.phase === "scanning" || state.phase === "cleaning";

/** Shader rows always show, so you can see what was checked. Everywhere else
 *  a row only matters if there is something in it to remove. */
function hasData(p: ProviderScan): boolean {
  return p.found && (p.group === "shaders" || p.bytes > 0);
}

function chosen(): ProviderScan[] {
  return (state.scan?.providers ?? []).filter((p) => hasData(p) && state.selected.has(p.id));
}

function vendorName(vendor: string): string {
  return { nvidia: "NVIDIA", amd: "AMD", intel: "Intel" }[vendor] ?? vendor;
}

function shortPath(path: string): string {
  // The tail of a path is the part that tells files apart.
  const parts = path.split("\\");
  return parts.length > 4 ? `...\\${parts.slice(-4).join("\\")}` : path;
}

// ---- rendering -------------------------------------------------------------

function renderGpu(): void {
  const data = state.scan;
  if (!data) {
    el.gpu.innerHTML = `<div class="gpu-row"><div class="line skeleton w60"></div><div class="line skeleton w40"></div></div>`;
    return;
  }

  const changed = new Set(data.driverChanges.map((c) => `${c.vendor}:${c.name}`));
  const rows = data.adapters.length
    ? data.adapters
        .map((a) => {
          const flag = changed.has(`${a.vendor}:${a.name}`) ? `<span class="chip">Updated</span>` : "";
          return `<div class="gpu-row">
            <div class="gpu-name">${escapeHtml(a.name)}${flag}</div>
            <div class="gpu-meta">Driver ${escapeHtml(a.version)} from ${escapeHtml(formatDate(a.date))}</div>
          </div>`;
        })
        .join("")
    : `<div class="gpu-row">
        <div class="gpu-name">No NVIDIA, AMD or Intel driver found</div>
        <div class="gpu-meta">The launcher, Windows and Steam caches can still be cleared.</div>
      </div>`;

  const last =
    data.lastCleanAt === null ? "Never cleaned" : `Last cleaned ${timeAgo(data.lastCleanAt)}`;
  const disk = data.disk
    ? `<span>${escapeHtml(data.disk.drive)} ${formatBytes(data.disk.free)} free of ${formatBytes(data.disk.total)}</span>`
    : "";
  const total =
    data.totalFreed > 0 ? `<span>${formatBytes(data.totalFreed)} freed so far</span>` : "";
  el.gpu.innerHTML = `${rows}<div class="gpu-last"><span>${last}</span>${disk}${total}</div>`;
}

interface Notice {
  kind: "info" | "warn" | "error" | "hint" | "ok";
  title: string;
  body: string;
  action?: { id: string; label: string };
}

function queueNotice(check: QueueCheck): Notice {
  const total = check.removed + check.remaining;
  if (check.remaining === 0) {
    return {
      kind: "ok",
      title: "The restart finished the job",
      body: `All ${plural(total, "file")} (${formatBytes(check.removedBytes)}) the display driver was holding were removed by Windows.`,
      action: { id: "dismiss-settled", label: "Dismiss" },
    };
  }
  return {
    kind: "warn",
    title: `${check.remaining} of ${total} files were still there after the restart`,
    body: `Windows removed ${plural(check.removed, "file")} (${formatBytes(check.removedBytes)}). The other ${check.remaining} could not be removed, or the driver created them again. They are index files the driver rewrites, so this is harmless.`,
    action: { id: "dismiss-settled", label: "Dismiss" },
  };
}

function restartCountdown(): number {
  if (state.restart?.kind !== "counting") return 0;
  return Math.max(0, Math.ceil((state.restart.endsAt - state.now) / 1000));
}

function notices(): Notice[] {
  const out: Notice[] = [];
  const data = state.scan;

  if (state.phase === "error") {
    out.push({ kind: "error", title: "Something went wrong", body: state.error });
    return out;
  }

  // A restart in progress outranks everything else on screen.
  if (state.restart?.kind === "counting") {
    const left = restartCountdown();
    out.push({
      kind: "warn",
      title: left > 0 ? `Restarting in ${left} seconds` : "Restarting now",
      body: "Windows will ask about any app with unsaved work and wait for you. Cancel if you are not ready.",
      action: left > 0 ? { id: "cancel-restart", label: "Cancel restart" } : undefined,
    });
  } else if (state.restart?.kind === "error") {
    out.push({ kind: "error", title: "The restart did not start", body: state.restart.message });
  }

  if (state.phase === "done" && state.result) {
    const r = state.result;
    if (r.cancelled) {
      out.push({
        kind: "warn",
        title: "Stopped early",
        body: "You cancelled the clean. Everything removed so far stays removed, and the rest was left alone.",
      });
    }
    if (r.preview) {
      out.push({
        kind: "hint",
        title: "Preview only",
        body: "Nothing was deleted. Turn off Preview only in Options to clear these for real.",
      });
    }
    if (r.queuedFiles > 0 && state.restart === null) {
      out.push({
        kind: "info",
        title: "Restart to finish",
        body: `${plural(r.queuedFiles, "file")} (${formatBytes(r.queuedBytes)}) are held open by the display driver. Windows removes them on your next restart, and ShaderSweep will check afterwards that it did.`,
        action: { id: "restart", label: "Restart now" },
      });
    }
    const skipped = r.providers.reduce((n, p) => n + p.skippedRecent, 0);
    if (skipped > 0) {
      out.push({
        kind: "hint",
        title: "Recent files were left alone",
        body: `${plural(skipped, "file")} changed in the last 24 hours, so a running app or installer may still need them.`,
      });
    }
    if (r.failedFiles > 0) {
      const holders = [...new Set(r.providers.flatMap((p) => p.holders))];
      const who = holders.length
        ? ` Windows says ${holders.slice(0, 4).join(", ")} ${holders.length === 1 ? "is" : "are"} using them.`
        : "";
      out.push({
        kind: "warn",
        title: "Some files were in use",
        body: `${plural(r.failedFiles, "file")} could not be removed.${who} Close the app and run it again.`,
      });
    }
    return out;
  }

  if (!data) return out;

  if (state.settled) out.push(queueNotice(state.settled));

  if (!data.isAdmin) {
    out.push({
      kind: "warn",
      title: "Not running as administrator",
      body: "System and service caches will be skipped. Start ShaderSweep as administrator to include them.",
    });
  }

  const runningRows = data.providers.filter((p) => hasData(p) && p.running.length > 0);
  if (runningRows.length > 0) {
    // "Discord cache" and "Steam client web cache" read better as app names.
    const names = runningRows
      .map((p) => p.label.replace(/ (web )?cache$/i, ""))
      .slice(0, 4)
      .join(", ");
    out.push({
      kind: "warn",
      title: "Some apps are running",
      body: `${names} ${runningRows.length === 1 ? "is" : "are"} open. Their rows start unticked, because a running app keeps files locked. Close them for a full clean, or tick the row anyway.`,
    });
  }

  if (data.pendingRestart > 0 && state.restart === null) {
    out.push({
      kind: "hint",
      title: "Restart pending",
      body: `${plural(data.pendingRestart, "file")} from an earlier clean ${data.pendingRestart === 1 ? "is" : "are"} waiting for a restart. Windows removes them on the next boot, so they still show below.`,
      action: { id: "restart", label: "Restart now" },
    });
  }

  if (data.driverChanges.length > 0) {
    const names = data.driverChanges.map((c) => `${vendorName(c.vendor)} ${c.version}`).join(" and ");
    const hasNvidia = data.driverChanges.some((c) => c.vendor === "nvidia");
    const asc = hasNvidia
      ? " If you turned on Auto Shader Compilation in the NVIDIA App, leave the NVIDIA row unchecked, since it already rebuilds shaders after an update."
      : "";
    out.push({
      kind: "info",
      title: "New driver detected",
      body: `${names} changed since your last clean. Drivers ignore shaders built for an older version, so cleaning mainly reclaims that space and helps if a game crashes, stutters or shows glitches.${asc}`,
    });
  } else if (data.lastCleanAt === null && data.adapters.length > 0 && out.length === 0) {
    out.push({
      kind: "hint",
      title: "No clean on record",
      body: "After your first clean, ShaderSweep tells you when a driver update makes another one worth running.",
    });
  }

  return out;
}

function renderNotice(): void {
  el.notice.innerHTML = notices()
    .map((n) => {
      const icon = n.kind === "warn" || n.kind === "error" ? icons.alert : n.kind === "ok" ? icons.check : icons.info;
      const action = n.action
        ? `<button class="notice-action" type="button" data-action="${n.action.id}">${escapeHtml(n.action.label)}</button>`
        : "";
      return `<div class="notice notice-${n.kind}" role="${n.kind === "error" ? "alert" : "status"}">
        <span class="notice-icon">${icon}</span>
        <div class="notice-body"><strong>${escapeHtml(n.title)}</strong><p>${escapeHtml(n.body)}</p>${action}</div>
      </div>`;
    })
    .join("");
}

function expectedBytes(): number {
  return chosen().reduce((n, p) => n + p.bytes, 0);
}

function renderSummary(): void {
  if (state.phase === "scanning") {
    const step = state.scanStep;
    const caption = step
      ? `Checking ${step.label}, ${step.checked} of ${step.total}`
      : "Looking through your caches";
    const pct = step ? Math.round((step.checked / step.total) * 100) : 4;
    el.summary.innerHTML = `<div class="eyebrow">Scanning</div>
      <div class="figure skeleton">0.00 GB</div>
      <div class="bar" role="progressbar" aria-label="Scan progress" aria-valuemin="0" aria-valuemax="100" aria-valuenow="${pct}"><span style="width:${pct}%"></span></div>
      <div class="caption"><span>${escapeHtml(caption)}</span></div>`;
    return;
  }

  if (state.phase === "cleaning") {
    const parts = [...state.progress.values()];
    const freed = parts.reduce((n, p) => n + p.freed, 0);
    const files = parts.reduce((n, p) => n + p.files, 0);
    const expected = Math.max(expectedBytes(), 1);
    const pct = Math.min(100, Math.round((freed / expected) * 100));
    const label = state.cancelling ? "Stopping" : state.preview ? "Measuring" : "Cleaning";
    const detail = state.preview
      ? `${plural(parts.filter((p) => p.done).length, "cache type")} measured`
      : `${plural(files, "file")} removed`;
    const lines = state.activity
      .map((line, i) => {
        const age = state.activity.length - 1 - i;
        return `<div class="activity-line" style="opacity:${Math.max(0.25, 1 - age * 0.2)}"><bdi>${escapeHtml(shortPath(line))}</bdi></div>`;
      })
      .join("");

    el.summary.innerHTML = `<div class="eyebrow">${label}</div>
      <div class="figure">${formatBytes(freed)}</div>
      <div class="bar" role="progressbar" aria-label="Clean progress" aria-valuemin="0" aria-valuemax="100" aria-valuenow="${pct}"><span style="width:${pct}%"></span></div>
      <div class="caption"><span>${detail}, ${formatDuration(state.now - state.startedAt)}</span><span class="pct">${pct}%</span></div>
      <div class="activity" aria-hidden="true">${lines || `<div class="activity-line">Starting</div>`}</div>`;
    return;
  }

  if (state.phase === "done" && state.result) {
    const r = state.result;
    const removed = r.providers.reduce((n, p) => n + p.removedFiles, 0);
    const copy = `<button class="link" id="copy" type="button">${state.copied ? "Copied" : "Copy report"}</button>`;
    const caption = r.preview
      ? `Across ${plural(r.providers.length, "cache type")}`
      : `${plural(removed, "file")} removed`;
    const eyebrow = r.cancelled ? "Freed before you stopped" : r.preview ? "Would free" : "Freed";
    el.summary.innerHTML = `<div class="eyebrow">${eyebrow}</div>
      <div class="figure is-done">${formatBytes(r.freed)}</div>
      <div class="caption"><span>${escapeHtml(caption)}</span>${copy}</div>`;
    return;
  }

  const picked = chosen();
  const bytes = picked.reduce((n, p) => n + p.bytes, 0);
  const files = picked.reduce((n, p) => n + p.files, 0);
  const anything = (state.scan?.providers ?? []).some((p) => p.found && p.bytes > 0);
  const caption = !anything
    ? "Your caches are already empty."
    : picked.length === 0
      ? "Nothing selected"
      : `${plural(files, "file")} in ${plural(picked.length, "cache type")}`;
  el.summary.innerHTML = `<div class="eyebrow">${state.preview ? "Would free" : "Ready to clear"}</div>
    <div class="figure">${formatBytes(bytes)}</div>
    <div class="caption"><span>${escapeHtml(caption)}</span></div>`;
}

function rowRight(p: ProviderScan): string {
  if (state.phase === "cleaning" && state.selected.has(p.id) && p.found) {
    const prog = state.progress.get(p.id);
    if (prog?.done) return `<span class="freed">${formatBytes(prog.freed)}${icons.check}</span>`;
    if (prog) return `<span class="live">${formatBytes(prog.freed)}<span class="spinner" role="img" aria-label="Working"></span></span>`;
    return `<span class="size muted">Waiting</span>`;
  }
  if (state.phase === "done" && state.result) {
    const hit = state.result.providers.find((r) => r.id === p.id);
    if (hit) return `<span class="freed">${formatBytes(hit.freed)}${icons.check}</span>`;
  }
  return p.found ? `<span class="size">${formatBytes(p.bytes)}</span>` : `<span class="size muted">Not found</span>`;
}

/** A thin bar under a row while it is being cleaned. */
function rowBar(p: ProviderScan): string {
  if (state.phase !== "cleaning" || !state.selected.has(p.id) || !p.found) return "";
  const prog = state.progress.get(p.id);
  const pct = prog ? (prog.done ? 100 : Math.min(99, Math.round((prog.freed / Math.max(p.bytes, 1)) * 100))) : 0;
  return `<span class="rowbar" aria-hidden="true"><span style="width:${pct}%"></span></span>`;
}

function renderRow(p: ProviderScan): string {
  const on = state.selected.has(p.id) && p.found;
  const locked = !p.found || state.phase !== "ready";
  const open = state.expanded.has(p.id);

  const folders = p.folders.length
    ? `<ul class="folders">${p.folders
        .map(
          (f) =>
            `<li><span class="path" title="${escapeHtml(f.path)}"><bdi>${escapeHtml(f.path)}</bdi></span><span class="size">${formatBytes(f.bytes)}</span><button class="open" type="button" data-open="${escapeHtml(f.path)}" aria-label="Open ${escapeHtml(f.path)}">Open</button></li>`,
        )
        .join("")}</ul>`
    : p.found
      ? ""
      : `<p class="detail-text">Nothing found on this PC.</p>`;
  const about = `<p class="detail-text">${escapeHtml(p.about)}</p>`;
  const note = p.note ? `<p class="detail-text note">${escapeHtml(p.note)}</p>` : "";

  const running = p.running.length
    ? `<span class="runhint">${escapeHtml(p.label.replace(/ (web )?cache$/i, ""))} is running. Close it for a full clean.</span>`
    : "";
  const caution = p.caution ? `<span class="caution">${escapeHtml(p.caution)}</span>` : "";
  const chip = p.running.length ? `<span class="chip warn">Running</span>` : "";

  return `<div class="item${p.found ? "" : " is-missing"}${on ? " is-on" : ""}">
    <label class="row">
      <input type="checkbox" data-id="${p.id}" ${on ? "checked" : ""} ${locked ? "disabled" : ""} />
      <span class="box">${icons.check}</span>
      <span class="text">
        <span class="label">${escapeHtml(p.label)}${chip}</span>
        <span class="blurb">${escapeHtml(p.blurb)}</span>
        ${caution}${running}
      </span>
      <span class="right">${rowRight(p)}</span>
    </label>
    <button class="peek" type="button" data-peek="${p.id}" aria-expanded="${open}" aria-label="Show details for ${escapeHtml(p.label)}">${icons.chevron}</button>
    ${rowBar(p)}
    <div class="detail"${open ? "" : " hidden"}>${about}${note}${folders}</div>
  </div>`;
}

function renderList(): void {
  const providers = state.scan?.providers;
  if (!providers) {
    el.list.innerHTML = Array.from({ length: 5 })
      .map(() => `<div class="item"><div class="row"><div class="line skeleton w60"></div></div></div>`)
      .join("");
    return;
  }

  el.list.innerHTML = GROUPS.map((group) => {
    const all = providers.filter((p) => p.group === group.id);
    const shown = group.hideMissing ? all.filter(hasData) : all;
    if (shown.length === 0) return "";

    const found = all.filter(hasData);
    const bytes = found.reduce((n, p) => n + p.bytes, 0);
    const picked = found.filter((p) => state.selected.has(p.id)).length;
    const open = state.openGroups.has(group.id);
    const meta = found.length
      ? `${found.length} found, ${formatBytes(bytes)}${picked ? `, ${picked} ticked` : ""}`
      : "Nothing found";

    return `<section class="group">
      <button class="group-head" type="button" data-group="${group.id}" aria-expanded="${open}">
        <span class="group-text"><span class="group-title">${group.title}</span><span class="group-hint">${group.hint}</span></span>
        <span class="group-meta">${meta}</span>
        <span class="group-chevron">${icons.chevron}</span>
      </button>
      <div class="group-body"${open ? "" : " hidden"}>${shown.map(renderRow).join("")}</div>
    </section>`;
  }).join("");
}

function renderCredit(): void {
  el.credit.innerHTML = `<span>Made by <button class="textlink" type="button" data-link="github">Kkthnx</button></span>
    <button class="textlink" type="button" data-link="website">kkthnx.com</button>
    <button class="textlink" type="button" data-link="issues">Report a problem</button>`;
}

function renderDock(): void {
  const picked = chosen();
  const bytes = picked.reduce((n, p) => n + p.bytes, 0);
  const verb = state.preview ? "Preview" : "Clean";

  let label = verb;
  let disabled = false;
  let secondary = false;

  switch (state.phase) {
    case "scanning":
      label = "Scanning";
      disabled = true;
      break;
    case "cleaning":
      label = state.cancelling ? "Stopping" : "Cancel";
      disabled = state.cancelling;
      secondary = true;
      break;
    case "done":
    case "error":
      label = state.phase === "error" ? "Try again" : "Scan again";
      secondary = true;
      break;
    default:
      if (picked.length === 0) {
        label = "Nothing to clean";
        disabled = true;
      } else {
        label = bytes > 0 ? `${verb} ${formatBytes(bytes)}` : verb;
      }
  }

  el.go.textContent = label;
  el.go.disabled = disabled;
  el.go.classList.toggle("is-secondary", secondary);
  el.rescan.disabled = busy();

  el.preview.setAttribute("aria-checked", String(state.preview));
  el.queue.setAttribute("aria-checked", String(state.queue));
  el.preview.disabled = busy();
  el.queue.disabled = busy();
}

function render(): void {
  renderGpu();
  renderNotice();
  renderSummary();
  renderList();
  renderDock();
}

// ---- actions ---------------------------------------------------------------

function fail(error: unknown): void {
  state.phase = "error";
  state.error = error instanceof Error ? error.message : String(error);
  render();
}

function defaultSelection(data: ScanResult): Set<string> {
  // A row for a running app starts unticked, since its files are locked.
  return new Set(
    data.providers
      .filter((p) => p.defaultOn && hasData(p) && p.running.length === 0)
      .map((p) => p.id),
  );
}

async function runScan(): Promise<void> {
  state.phase = "scanning";
  state.result = null;
  state.copied = false;
  state.scanStep = null;
  state.progress.clear();
  render();

  try {
    const data = await scan();
    state.scan = data;
    if (data.queueCheck?.restarted) state.settled = data.queueCheck;
    state.selected = defaultSelection(data);
    state.phase = "ready";
    render();
  } catch (error) {
    fail(error);
  }
}

async function runClean(): Promise<void> {
  const ids = chosen().map((p) => p.id);
  if (ids.length === 0) return;

  state.phase = "cleaning";
  state.progress.clear();
  state.activity = [];
  state.result = null;
  state.cancelling = false;
  state.settled = null;
  state.startedAt = Date.now();
  state.now = state.startedAt;
  render();

  try {
    state.result = await clean(ids, state.preview, state.queue);
    state.phase = "done";
    render();
    // Bring the headline number back into view.
    el.scroll.scrollTo({ top: 0 });

    // Refresh the driver record and sizes without leaving the result screen.
    scan()
      .then((fresh) => {
        state.scan = fresh;
        render();
      })
      .catch(() => {});
  } catch (error) {
    fail(error);
  }
}

async function stopClean(): Promise<void> {
  state.cancelling = true;
  renderDock();
  renderSummary();
  try {
    await cancelClean();
  } catch {
    /* the run ends on its own either way */
  }
}

async function startRestart(): Promise<void> {
  try {
    const seconds = await restartPc(RESTART_SECONDS);
    state.restart = { kind: "counting", endsAt: Date.now() + seconds * 1000 };
  } catch (error) {
    state.restart = {
      kind: "error",
      message: error instanceof Error ? error.message : String(error),
    };
  }
  state.now = Date.now();
  render();
}

async function stopRestart(): Promise<void> {
  try {
    await cancelRestart();
    state.restart = null;
  } catch (error) {
    state.restart = {
      kind: "error",
      message: error instanceof Error ? error.message : String(error),
    };
  }
  render();
}

async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    // Older WebView builds only allow the legacy route.
    const box = document.createElement("textarea");
    box.value = text;
    box.style.position = "fixed";
    box.style.opacity = "0";
    document.body.appendChild(box);
    box.select();
    const ok = document.execCommand("copy");
    box.remove();
    return ok;
  }
}

// ---- wiring ----------------------------------------------------------------

el.brand.innerHTML = `${icons.logo}<span>ShaderSweep</span>`;
el.rescan.innerHTML = icons.refresh;
$("about").textContent = `ShaderSweep ${__APP_VERSION__} by Kkthnx. Only folders on a fixed list of regenerable caches are ever touched.`;
el.links.innerHTML = (
  [
    ["github", "GitHub"],
    ["website", "kkthnx.com"],
    ["source", "Source code"],
    ["issues", "Report a problem"],
    ["releases", "Latest release"],
  ] as [LinkName, string][]
)
  .map(([name, text]) => `<button class="textlink" type="button" data-link="${name}">${text}</button>`)
  .join("");
el.confirm.textContent = `Restart in ${RESTART_SECONDS} seconds`;

state.preview = store.get("preview", false);
state.queue = store.get("queue", true);

el.go.addEventListener("click", () => {
  if (state.phase === "ready") void runClean();
  else if (state.phase === "cleaning") void stopClean();
  else if (state.phase === "done" || state.phase === "error") void runScan();
});
el.rescan.addEventListener("click", () => void runScan());

el.preview.addEventListener("click", () => {
  state.preview = !state.preview;
  store.set("preview", state.preview);
  render();
});
el.queue.addEventListener("click", () => {
  state.queue = !state.queue;
  store.set("queue", state.queue);
  render();
});

// Links open from a fixed list in the backend. The page only names one.
document.addEventListener("click", (event) => {
  const link = (event.target as HTMLElement).closest<HTMLElement>("[data-link]");
  if (link?.dataset.link) void openLink(link.dataset.link as LinkName).catch(() => {});
});

el.summary.addEventListener("click", async (event) => {
  if (!(event.target as HTMLElement).closest("#copy")) return;
  if (await copyText(state.result?.report ?? "")) {
    state.copied = true;
    renderSummary();
    setTimeout(() => {
      state.copied = false;
      renderSummary();
    }, 1800);
  }
});

el.notice.addEventListener("click", (event) => {
  const action = (event.target as HTMLElement).closest<HTMLElement>("[data-action]")?.dataset.action;
  if (action === "restart") el.dialog.showModal();
  else if (action === "cancel-restart") void stopRestart();
  else if (action === "dismiss-settled") {
    state.settled = null;
    renderNotice();
  }
});

el.dialog.addEventListener("close", () => {
  const confirmed = el.dialog.returnValue === "confirm";
  el.dialog.returnValue = "";
  if (confirmed) void startRestart();
});

el.list.addEventListener("change", (event) => {
  const box = event.target as HTMLInputElement;
  const id = box.dataset.id;
  if (!id) return;
  if (box.checked) state.selected.add(id);
  else state.selected.delete(id);
  renderSummary();
  renderDock();
  box.closest(".item")?.classList.toggle("is-on", box.checked);
  // The group header counts what is ticked.
  const scrollTop = el.scroll.scrollTop;
  renderList();
  el.scroll.scrollTop = scrollTop;
});

el.list.addEventListener("click", (event) => {
  const target = event.target as HTMLElement;

  const open = target.closest<HTMLElement>("[data-open]");
  if (open?.dataset.open) {
    void reveal(open.dataset.open).catch(() => {});
    return;
  }

  const group = target.closest<HTMLElement>("[data-group]")?.dataset.group as GroupId | undefined;
  if (group) {
    if (state.openGroups.has(group)) state.openGroups.delete(group);
    else state.openGroups.add(group);
    renderList();
    return;
  }

  const peek = target.closest<HTMLElement>("[data-peek]")?.dataset.peek;
  if (peek) {
    if (state.expanded.has(peek)) state.expanded.delete(peek);
    else state.expanded.add(peek);
    renderList();
  }
});

void onProgress((p) => {
  state.progress.set(p.id, { done: p.done, freed: p.freed, files: p.files });
  if (p.current) {
    if (state.activity[state.activity.length - 1] !== p.current) state.activity.push(p.current);
    if (state.activity.length > ACTIVITY_LINES) state.activity.shift();
  }
  if (state.phase === "cleaning") {
    state.now = Date.now();
    renderSummary();
    renderList();
  }
});

void onScanStep((step) => {
  state.scanStep = step;
  if (state.phase === "scanning") renderSummary();
});

// One slow clock drives the elapsed time and the restart countdown.
setInterval(() => {
  state.now = Date.now();
  if (state.phase === "cleaning") renderSummary();
  if (state.restart?.kind === "counting") renderNotice();
}, 500);

if (!import.meta.env.DEV) {
  // A cleaner should feel like an app, not a web page.
  document.addEventListener("contextmenu", (event) => event.preventDefault());
}

renderCredit();
render();
void runScan();
