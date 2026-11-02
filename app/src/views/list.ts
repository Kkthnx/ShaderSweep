import { el, setHtml } from "../dom";
import { escapeHtml, formatBytes } from "../format";
import { icons } from "../icons";
import { GROUPS, hasData, state } from "../state";
import type { ProviderScan } from "../types";

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

export function renderList(): void {
  const providers = state.scan?.providers;
  if (!providers) {
    setHtml(
      el.list,
      Array.from({ length: 5 })
        .map(() => `<div class="item"><div class="row"><div class="line skeleton w60"></div></div></div>`)
        .join(""),
    );
    return;
  }

  const picks =
    state.phase === "ready"
      ? `<div class="picks">Select
          <button class="textlink" type="button" data-pick="recommended">Recommended</button>
          <button class="textlink" type="button" data-pick="none">None</button>
        </div>`
      : "";

  const groups = GROUPS.map((group) => {
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
  setHtml(el.list, picks + groups);
}
