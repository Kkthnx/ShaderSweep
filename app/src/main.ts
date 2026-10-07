import "./styles.css";
import { clean, onProgress, scan } from "./bridge";
import { escapeHtml, formatBytes, formatDate, plural, timeAgo } from "./format";
import { icons } from "./icons";
import type { CleanResult, ProviderScan, ScanResult } from "./types";

type Phase = "scanning" | "ready" | "cleaning" | "done" | "error";

const state = {
  phase: "scanning" as Phase,
  scan: null as ScanResult | null,
  result: null as CleanResult | null,
  error: "",
  selected: new Set<string>(),
  expanded: new Set<string>(),
  progress: new Map<string, { done: boolean; freed: number }>(),
  preview: false,
  queue: true,
  copied: false,
};

const $ = <T extends HTMLElement>(id: string): T => document.getElementById(id) as T;
const el = {
  brand: $("brand"),
  rescan: $<HTMLButtonElement>("rescan"),
  gpu: $("gpu"),
  notice: $("notice"),
  summary: $("summary"),
  list: $("list"),
  go: $<HTMLButtonElement>("go"),
  preview: $<HTMLButtonElement>("opt-preview"),
  queue: $<HTMLButtonElement>("opt-queue"),
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

function chosen(): ProviderScan[] {
  return (state.scan?.providers ?? []).filter((p) => p.found && state.selected.has(p.id));
}

// ---- rendering -----------------------------------------------------------

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
        <div class="gpu-meta">The Windows and Steam caches can still be cleared.</div>
      </div>`;

  const last =
    data.lastCleanAt === null ? "Never cleaned" : `Last cleaned ${timeAgo(data.lastCleanAt)}`;
  const disk = data.disk
    ? `<span>${escapeHtml(data.disk.drive)} ${formatBytes(data.disk.free)} free of ${formatBytes(data.disk.total)}</span>`
    : "";
  el.gpu.innerHTML = `${rows}<div class="gpu-last"><span>${last}</span>${disk}</div>`;
}

interface Notice {
  kind: "info" | "warn" | "error" | "hint";
  title: string;
  body: string;
}

function notices(): Notice[] {
  const out: Notice[] = [];
  const data = state.scan;

  if (state.phase === "error") {
    out.push({ kind: "error", title: "Something went wrong", body: state.error });
    return out;
  }

  if (state.phase === "done" && state.result) {
    const r = state.result;
    if (r.preview) {
      out.push({
        kind: "hint",
        title: "Preview only",
        body: "Nothing was deleted. Turn off Preview only in Options to clear these for real.",
      });
    }
    if (r.queuedFiles > 0) {
      out.push({
        kind: "info",
        title: "Restart to finish",
        body: `${plural(r.queuedFiles, "file")} (${formatBytes(r.queuedBytes)}) are held open by the display driver. Windows removes them on your next restart.`,
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
        body: `${plural(r.failedFiles, "file")} could not be removed.${who} Close your games and run it again, or turn on restart removal in Options.`,
      });
    }
    return out;
  }

  if (!data) return out;

  if (!data.isAdmin) {
    out.push({
      kind: "warn",
      title: "Not running as administrator",
      body: "System and service caches will be skipped. Start ShaderSweep as administrator to include them.",
    });
  }

  if (data.pendingRestart > 0) {
    out.push({
      kind: "hint",
      title: "Restart pending",
      body: `${plural(data.pendingRestart, "file")} from an earlier clean ${data.pendingRestart === 1 ? "is" : "are"} waiting for a restart. Windows removes them on the next boot, so they still show below.`,
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

function vendorName(vendor: string): string {
  return { nvidia: "NVIDIA", amd: "AMD", intel: "Intel" }[vendor] ?? vendor;
}

function renderNotice(): void {
  el.notice.innerHTML = notices()
    .map((n) => {
      const icon = n.kind === "warn" || n.kind === "error" ? icons.alert : icons.info;
      return `<div class="notice notice-${n.kind}" role="${n.kind === "error" ? "alert" : "status"}">
        <span class="notice-icon">${icon}</span>
        <div><strong>${escapeHtml(n.title)}</strong><p>${escapeHtml(n.body)}</p></div>
      </div>`;
    })
    .join("");
}

function renderSummary(): void {
  let eyebrow = "";
  let figure = "";
  let caption = "";
  let tone = "";

  if (state.phase === "scanning") {
    el.summary.innerHTML = `<div class="eyebrow">Scanning</div><div class="figure skeleton">0.00 GB</div><div class="caption">Looking through your caches</div>`;
    return;
  }

  if (state.phase === "cleaning") {
    const total = [...state.progress.values()].reduce((n, p) => n + p.freed, 0);
    const done = [...state.progress.values()].filter((p) => p.done).length;
    eyebrow = state.preview ? "Measuring" : "Cleaning";
    figure = formatBytes(total);
    caption = `${done} of ${state.selected.size} finished`;
  } else if (state.phase === "done" && state.result) {
    const r = state.result;
    const removed = r.providers.reduce((n, p) => n + p.removedFiles, 0);
    eyebrow = r.preview ? "Would free" : "Freed";
    figure = formatBytes(r.freed);
    caption = r.preview
      ? `Across ${plural(r.providers.length, "cache type")}`
      : `${plural(removed, "file")} removed`;
    tone = " is-done";
  } else {
    const picked = chosen();
    const bytes = picked.reduce((n, p) => n + p.bytes, 0);
    const files = picked.reduce((n, p) => n + p.files, 0);
    const anything = (state.scan?.providers ?? []).some((p) => p.found && p.bytes > 0);
    eyebrow = state.preview ? "Would free" : "Ready to clear";
    figure = formatBytes(bytes);
    caption = !anything
      ? "Your caches are already empty."
      : picked.length === 0
        ? "Nothing selected"
        : `${plural(files, "file")} in ${plural(picked.length, "cache type")}`;
  }

  const copy =
    state.phase === "done"
      ? `<button class="link" id="copy" type="button">${state.copied ? "Copied" : "Copy report"}</button>`
      : "";
  el.summary.innerHTML = `<div class="eyebrow">${eyebrow}</div><div class="figure${tone}">${figure}</div><div class="caption"><span>${escapeHtml(caption)}</span>${copy}</div>`;
}

function rowRight(p: ProviderScan): string {
  if (state.phase === "cleaning" && state.selected.has(p.id) && p.found) {
    const prog = state.progress.get(p.id);
    return prog?.done
      ? `<span class="freed">${formatBytes(prog.freed)}${icons.check}</span>`
      : `<span class="spinner" role="img" aria-label="Working"></span>`;
  }
  if (state.phase === "done" && state.result) {
    const hit = state.result.providers.find((r) => r.id === p.id);
    if (hit) return `<span class="freed">${formatBytes(hit.freed)}${icons.check}</span>`;
  }
  return p.found ? `<span class="size">${formatBytes(p.bytes)}</span>` : `<span class="size muted">Not found</span>`;
}

function renderList(): void {
  const providers = state.scan?.providers;
  if (!providers) {
    el.list.innerHTML = Array.from({ length: 5 })
      .map(() => `<div class="item"><div class="row"><div class="line skeleton w60"></div></div></div>`)
      .join("");
    return;
  }

  el.list.innerHTML = providers
    .map((p) => {
      const on = state.selected.has(p.id) && p.found;
      const locked = !p.found || state.phase !== "ready";
      const open = state.expanded.has(p.id);

      const folders = p.folders.length
        ? `<ul class="folders">${p.folders
            .map(
              (f) =>
                `<li><span class="path" title="${escapeHtml(f.path)}"><bdi>${escapeHtml(f.path)}</bdi></span><span class="size">${formatBytes(f.bytes)}</span></li>`,
            )
            .join("")}</ul>`
        : `<p class="detail-text">Nothing found on this PC.</p>`;
      const about = `<p class="detail-text">${escapeHtml(p.about)}</p>`;
      const note = p.note ? `<p class="detail-text note">${escapeHtml(p.note)}</p>` : "";

      return `<div class="item${p.found ? "" : " is-missing"}${on ? " is-on" : ""}">
        <label class="row">
          <input type="checkbox" data-id="${p.id}" ${on ? "checked" : ""} ${locked ? "disabled" : ""} />
          <span class="box">${icons.check}</span>
          <span class="text">
            <span class="label">${escapeHtml(p.label)}</span>
            <span class="blurb">${escapeHtml(p.blurb)}</span>
          </span>
          <span class="right">${rowRight(p)}</span>
        </label>
        <button class="peek" type="button" data-peek="${p.id}" aria-expanded="${open}" aria-label="Show folders for ${escapeHtml(p.label)}">${icons.chevron}</button>
        <div class="detail"${open ? "" : " hidden"}>${about}${note}${folders}</div>
      </div>`;
    })
    .join("");
}

function renderDock(): void {
  const picked = chosen();
  const bytes = picked.reduce((n, p) => n + p.bytes, 0);
  const verb = state.preview ? "Preview" : "Clean";

  let label = verb;
  let disabled = false;
  switch (state.phase) {
    case "scanning":
      label = "Scanning";
      disabled = true;
      break;
    case "cleaning":
      label = state.preview ? "Measuring" : "Cleaning";
      disabled = true;
      break;
    case "done":
    case "error":
      label = state.phase === "error" ? "Try again" : "Scan again";
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
  el.go.classList.toggle("is-secondary", state.phase === "done" || state.phase === "error");
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

// ---- report --------------------------------------------------------------

function buildReport(): string {
  const r = state.result;
  const data = state.scan;
  if (!r) return "";

  const lines = [`ShaderSweep ${__APP_VERSION__}`, r.preview ? "Preview, nothing was deleted" : "Clean"];
  for (const a of data?.adapters ?? []) {
    lines.push(`GPU: ${a.name}, driver ${a.version} (${a.date})`);
  }
  lines.push(`${r.preview ? "Would free" : "Freed"}: ${formatBytes(r.freed)}`);

  for (const item of r.providers) {
    const label = data?.providers.find((p) => p.id === item.id)?.label ?? item.id;
    const parts = [`${r.preview ? "would free" : "freed"} ${formatBytes(item.freed)}`];
    if (!r.preview) parts.push(`${plural(item.removedFiles, "file")} removed`);
    if (item.queuedFiles) parts.push(`${item.queuedFiles} queued for restart (${formatBytes(item.queuedBytes)})`);
    if (item.failedFiles) parts.push(`${item.failedFiles} in use (${formatBytes(item.failedBytes)})`);
    lines.push(`- ${label}: ${parts.join(", ")}`);
    if (item.holders.length) lines.push(`  held by: ${item.holders.join(", ")}`);
  }
  return lines.join("\n");
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

// ---- actions -------------------------------------------------------------

function fail(error: unknown): void {
  state.phase = "error";
  state.error = error instanceof Error ? error.message : String(error);
  render();
}

async function runScan(): Promise<void> {
  state.phase = "scanning";
  state.result = null;
  state.copied = false;
  state.progress.clear();
  render();

  try {
    const data = await scan();
    state.scan = data;
    state.selected = new Set(data.providers.filter((p) => p.defaultOn && p.found).map((p) => p.id));
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
  state.result = null;
  render();

  try {
    state.result = await clean(ids, state.preview, state.queue);
    state.phase = "done";
    render();
    // Bring the headline number back into view.
    document.querySelector(".scroll")?.scrollTo({ top: 0 });

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

// ---- wiring --------------------------------------------------------------

el.brand.innerHTML = `${icons.logo}<span>ShaderSweep</span>`;
$("about").textContent = `ShaderSweep ${__APP_VERSION__}. Only folders on a fixed list of regenerable caches are ever touched.`;
el.rescan.innerHTML = icons.refresh;
state.preview = store.get("preview", false);
state.queue = store.get("queue", true);

el.go.addEventListener("click", () => {
  if (state.phase === "ready") void runClean();
  else if (state.phase === "done" || state.phase === "error") void runScan();
});
el.rescan.addEventListener("click", () => void runScan());

el.summary.addEventListener("click", async (event) => {
  if (!(event.target as HTMLElement).closest("#copy")) return;
  if (await copyText(buildReport())) {
    state.copied = true;
    renderSummary();
    setTimeout(() => {
      state.copied = false;
      renderSummary();
    }, 1800);
  }
});

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

el.list.addEventListener("change", (event) => {
  const box = event.target as HTMLInputElement;
  const id = box.dataset.id;
  if (!id) return;
  if (box.checked) state.selected.add(id);
  else state.selected.delete(id);
  renderSummary();
  renderDock();
  box.closest(".item")?.classList.toggle("is-on", box.checked);
});

el.list.addEventListener("click", (event) => {
  const peek = (event.target as HTMLElement).closest<HTMLElement>("[data-peek]");
  if (!peek?.dataset.peek) return;
  const id = peek.dataset.peek;
  if (state.expanded.has(id)) state.expanded.delete(id);
  else state.expanded.add(id);
  renderList();
});

void onProgress((p) => {
  state.progress.set(p.id, { done: p.done, freed: p.freed });
  render();
});

if (!import.meta.env.DEV) {
  // A cleaner should feel like an app, not a web page.
  document.addEventListener("contextmenu", (event) => event.preventDefault());
}

render();
void runScan();
