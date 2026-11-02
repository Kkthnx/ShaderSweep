import { el, setHtml } from "../dom";
import { escapeHtml, formatBytes, formatDuration, plural } from "../format";
import { chosen, expectedBytes, shortPath, state } from "../state";

export function renderSummary(): void {
  if (state.phase === "scanning") {
    const step = state.scanStep;
    const caption = step
      ? `Checking ${step.label}, ${step.checked} of ${step.total}`
      : "Looking through your caches";
    const pct = step ? Math.round((step.checked / step.total) * 100) : 4;
    setHtml(
      el.summary,
      `<div class="eyebrow">Scanning</div>
      <div class="figure skeleton">0.00 GB</div>
      <div class="bar" role="progressbar" aria-label="Scan progress" aria-valuemin="0" aria-valuemax="100" aria-valuenow="${pct}"><span style="width:${pct}%"></span></div>
      <div class="caption"><span>${escapeHtml(caption)}</span></div>`,
    );
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

    setHtml(
      el.summary,
      `<div class="eyebrow">${label}</div>
      <div class="figure">${formatBytes(freed)}</div>
      <div class="bar" role="progressbar" aria-label="Clean progress" aria-valuemin="0" aria-valuemax="100" aria-valuenow="${pct}"><span style="width:${pct}%"></span></div>
      <div class="caption"><span>${detail}, ${formatDuration(state.now - state.startedAt)}</span><span class="pct">${pct}%</span></div>
      <div class="activity" aria-hidden="true">${lines || `<div class="activity-line">Starting</div>`}</div>`,
    );
    return;
  }

  if (state.phase === "done" && state.result) {
    const r = state.result;
    const removed = r.providers.reduce((n, p) => n + p.removedFiles, 0);
    const copy = `<button class="link" id="copy" type="button" data-copy>${state.copied ? "Copied" : "Copy report"}</button>`;
    const caption = r.preview
      ? `Across ${plural(r.providers.length, "cache type")}`
      : `${plural(removed, "file")} removed`;
    const eyebrow = r.cancelled ? "Freed before you stopped" : r.preview ? "Would free" : "Freed";
    setHtml(
      el.summary,
      `<div class="eyebrow">${eyebrow}</div>
      <div class="figure is-done">${formatBytes(r.freed)}</div>
      <div class="caption"><span>${escapeHtml(caption)}</span>${copy}</div>`,
    );
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
  setHtml(
    el.summary,
    `<div class="eyebrow">${state.preview ? "Would free" : "Ready to clear"}</div>
    <div class="figure">${formatBytes(bytes)}</div>
    <div class="caption"><span>${escapeHtml(caption)}</span></div>`,
  );
}
