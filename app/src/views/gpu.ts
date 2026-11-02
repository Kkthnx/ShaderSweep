import { el, setHtml } from "../dom";
import { escapeHtml, formatBytes, formatDate, timeAgo } from "../format";
import { state } from "../state";

export function renderGpu(): void {
  const data = state.scan;
  if (!data) {
    setHtml(el.gpu, `<div class="gpu-row"><div class="line skeleton w60"></div><div class="line skeleton w40"></div></div>`);
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
  // One line per drive that holds a cache, so a game library on D: is not hidden.
  const disk = data.disks
    .map(
      (d) =>
        `<span>${escapeHtml(d.drive)} ${formatBytes(d.free)} free of ${formatBytes(d.total)}</span>`,
    )
    .join("");
  const total =
    data.totalFreed > 0 ? `<span>${formatBytes(data.totalFreed)} freed so far</span>` : "";
  setHtml(el.gpu, `${rows}<div class="gpu-last"><span>${last}</span>${disk}${total}</div>`);
}
