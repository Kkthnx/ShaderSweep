import { cancelClean, cancelRestart, clean, restartPc, scan } from "./bridge";
import { el } from "./dom";
import { escapeHtml, formatBytes } from "./format";
import { render } from "./render";
import { chosen, defaultSelection, IRREVERSIBLE, RESTART_SECONDS, state } from "./state";
import { renderDock } from "./views/dock";
import { renderSummary } from "./views/summary";

export function fail(error: unknown): void {
  state.phase = "error";
  state.error = error instanceof Error ? error.message : String(error);
  render();
}

export async function runScan(): Promise<void> {
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

/** A real clean that includes something irreversible asks first. */
export function requestClean(): void {
  const risky = state.preview ? [] : chosen().filter((p) => IRREVERSIBLE.has(p.id));
  if (risky.length === 0) {
    void runClean();
    return;
  }
  el.sureList.innerHTML = risky
    .map((p) => `<li>${escapeHtml(p.label)}, ${formatBytes(p.bytes)}</li>`)
    .join("");
  el.sure.showModal();
}

export async function runClean(): Promise<void> {
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

export async function stopClean(): Promise<void> {
  state.cancelling = true;
  renderDock();
  renderSummary();
  try {
    await cancelClean();
  } catch {
    /* the run ends on its own either way */
  }
}

export async function startRestart(): Promise<void> {
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

export async function stopRestart(): Promise<void> {
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

export async function copyText(text: string): Promise<boolean> {
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
