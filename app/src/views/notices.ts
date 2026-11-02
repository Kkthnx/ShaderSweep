import { el, setHtml } from "../dom";
import { escapeHtml, formatBytes, plural } from "../format";
import { icons } from "../icons";
import { hasData, state, vendorName } from "../state";
import type { QueueCheck } from "../types";

interface Notice {
  /** Stable across redraws, so a notice stays open while the page updates. */
  key: string;
  kind: "info" | "warn" | "error" | "hint" | "ok";
  title: string;
  body: string;
  action?: { id: string; label: string };
  /** Shown in full, never folded down to one line. */
  sticky?: boolean;
}

function queueNotice(check: QueueCheck): Notice {
  const total = check.removed + check.remaining;
  if (check.remaining === 0) {
    return {
      key: "settled",
      sticky: true,
      kind: "ok",
      title: "The restart finished the job",
      body: `All ${plural(total, "file")} (${formatBytes(check.removedBytes)}) the display driver was holding were removed by Windows.`,
      action: { id: "dismiss-settled", label: "Dismiss" },
    };
  }
  return {
    key: "settled",
    sticky: true,
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
    out.push({ key: "error", sticky: true, kind: "error", title: "Something went wrong", body: state.error });
    return out;
  }

  // A restart in progress outranks everything else on screen.
  if (state.restart?.kind === "counting") {
    const left = restartCountdown();
    out.push({
      key: "countdown",
      sticky: true,
      kind: "warn",
      title: left > 0 ? `Restarting in ${left} seconds` : "Restarting now",
      body: "Windows will ask about any app with unsaved work and wait for you. Cancel if you are not ready.",
      action: left > 0 ? { id: "cancel-restart", label: "Cancel restart" } : undefined,
    });
  } else if (state.restart?.kind === "error") {
    out.push({
      key: "restart-error",
      sticky: true,
      kind: "error",
      title: "The restart did not start",
      body: state.restart.message,
    });
  }

  if (state.phase === "done" && state.result) {
    const r = state.result;
    if (r.cancelled) {
      out.push({
        key: "cancelled",
        sticky: true,
        kind: "warn",
        title: "Stopped early",
        body: "You cancelled the clean. Everything removed so far stays removed, and the rest was left alone.",
      });
    }
    if (r.preview) {
      out.push({
        key: "preview",
        kind: "hint",
        title: "Preview only, nothing was deleted",
        body: "Nothing was deleted. This is what a real clean would remove.",
        action: { id: "go-real", label: "Switch to a real clean" },
      });
    }
    if (r.queuedFiles > 0 && state.restart === null) {
      out.push({
        key: "restart-to-finish",
        sticky: true,
        kind: "info",
        title: "Restart to finish",
        body: `${plural(r.queuedFiles, "file")} (${formatBytes(r.queuedBytes)}) are held open by the display driver. Windows removes them on your next restart, and ShaderSweep will check afterwards that it did.`,
        action: { id: "restart", label: "Restart now" },
      });
    }
    const skipped = r.providers.reduce((n, p) => n + p.skippedRecent, 0);
    if (skipped > 0) {
      out.push({
        key: "recent",
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
        key: "in-use",
        sticky: true,
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
      key: "not-admin",
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
      key: "running",
      kind: "warn",
      title: `${names} ${runningRows.length === 1 ? "is" : "are"} open`,
      body: `${names} ${runningRows.length === 1 ? "is" : "are"} open. Their rows start unticked, because a running app keeps files locked. Close them for a full clean, or tick the row anyway.`,
    });
  }

  if (data.pendingRestart > 0 && state.restart === null) {
    out.push({
      key: "restart-pending",
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
      key: "new-driver",
      kind: "info",
      title: "New driver detected",
      body: `${names} changed since your last clean. Drivers ignore shaders built for an older version, so cleaning mainly reclaims that space and helps if a game crashes, stutters or shows glitches.${asc}`,
    });
  } else if (data.lastCleanAt === null && data.adapters.length > 0 && out.length === 0) {
    out.push({
      key: "first-clean",
      kind: "hint",
      title: "No clean on record",
      body: "After your first clean, ShaderSweep tells you when a driver update makes another one worth running.",
    });
  }

  return out;
}

export function renderNotice(): void {
  const html = notices()
    .map((n) => {
      const icon = n.kind === "warn" || n.kind === "error" ? icons.alert : n.kind === "ok" ? icons.check : icons.info;
      const action = n.action
        ? `<button class="notice-action" type="button" data-action="${n.action.id}">${escapeHtml(n.action.label)}</button>`
        : "";
      const role = n.kind === "error" ? "alert" : "status";

      if (n.sticky) {
        return `<div class="notice notice-${n.kind}" role="${role}">
          <span class="notice-icon">${icon}</span>
          <div class="notice-body"><strong>${escapeHtml(n.title)}</strong><p>${escapeHtml(n.body)}</p>${action}</div>
        </div>`;
      }

      // Everything that is only good to know folds down to one line, so the
      // list of caches stays on screen. The title says enough to decide.
      const open = state.openNotices.has(n.key);
      return `<div class="notice notice-${n.kind} is-compact${open ? " is-open" : ""}" role="${role}">
        <div class="notice-bar">
          <button class="notice-toggle" type="button" data-notice="${n.key}" aria-expanded="${open}">
            <span class="notice-icon">${icon}</span>
            <strong>${escapeHtml(n.title)}</strong>
            <span class="notice-chevron">${icons.chevron}</span>
          </button>
          ${action}
        </div>
        <p class="notice-more"${open ? "" : " hidden"}>${escapeHtml(n.body)}</p>
      </div>`;
    })
    .join("");
  setHtml(el.notice, html);
}
