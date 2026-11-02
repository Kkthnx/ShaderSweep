import "./styles.css";
import {
  copyText,
  requestClean,
  runClean,
  runScan,
  startRestart,
  stopClean,
  stopRestart,
} from "./actions";
import { onProgress, onScanStep, openLink, reveal } from "./bridge";
import { $, el } from "./dom";
import { icons } from "./icons";
import { render } from "./render";
import {
  ACTIVITY_LINES,
  busy,
  defaultSelection,
  RESTART_SECONDS,
  state,
  store,
} from "./state";
import type { GroupId, LinkName } from "./types";
import { linkButton, renderCredit } from "./views/credit";
import { renderDock } from "./views/dock";
import { renderList } from "./views/list";
import { renderNotice } from "./views/notices";
import { renderSummary } from "./views/summary";

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
  .map(([name, text]) => linkButton(name, text))
  .join("");
el.confirm.textContent = `Restart in ${RESTART_SECONDS} seconds`;

state.preview = store.get("preview", false);
state.queue = store.get("queue", true);

el.go.addEventListener("click", () => {
  if (state.phase === "ready") requestClean();
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
  const key = (event.target as HTMLElement).closest<HTMLElement>("[data-notice]")?.dataset.notice;
  if (key) {
    if (state.openNotices.has(key)) state.openNotices.delete(key);
    else state.openNotices.add(key);
    renderNotice();
    return;
  }
  const action = (event.target as HTMLElement).closest<HTMLElement>("[data-action]")?.dataset.action;
  if (action === "restart") el.dialog.showModal();
  else if (action === "cancel-restart") void stopRestart();
  else if (action === "go-real") {
    // Back to the list with the same rows ticked, now set to delete.
    state.preview = false;
    store.set("preview", false);
    state.result = null;
    state.phase = "ready";
    el.scroll.scrollTo({ top: 0 });
    render();
  } else if (action === "dismiss-settled") {
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

el.sure.addEventListener("close", () => {
  const confirmed = el.sure.returnValue === "confirm";
  el.sure.returnValue = "";
  if (confirmed) void runClean();
});

el.list.addEventListener("click", (event) => {
  const target = event.target as HTMLElement;

  const pick = target.closest<HTMLElement>("[data-pick]")?.dataset.pick;
  if (pick && state.scan) {
    state.selected = pick === "none" ? new Set() : defaultSelection(state.scan);
    render();
    return;
  }

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

let drawQueued = false;

void onProgress((p) => {
  state.progress.set(p.id, { done: p.done, freed: p.freed, files: p.files });
  if (p.current) {
    if (state.activity[state.activity.length - 1] !== p.current) state.activity.push(p.current);
    if (state.activity.length > ACTIVITY_LINES) state.activity.shift();
  }
  // Progress can arrive many times a frame. Draw once per frame instead.
  if (state.phase === "cleaning" && !drawQueued) {
    drawQueued = true;
    requestAnimationFrame(() => {
      drawQueued = false;
      if (state.phase !== "cleaning") return;
      state.now = Date.now();
      renderSummary();
      renderList();
    });
  }
});

void onScanStep((step) => {
  state.scanStep = step;
  if (state.phase === "scanning") renderSummary();
});

// Only the list scrolls. If anything ever scrolls the page itself, such as a
// focus jump, put it straight back so the window can never end up shifted.
window.addEventListener("scroll", () => {
  const page = document.scrollingElement;
  if (page && (page.scrollTop !== 0 || page.scrollLeft !== 0)) page.scrollTo(0, 0);
});

// F5 and Ctrl+R scan again instead of reloading the page, which would wipe the
// result on screen.
document.addEventListener("keydown", (event) => {
  const reload = event.key === "F5" || (event.ctrlKey && event.key.toLowerCase() === "r");
  if (!reload) return;
  event.preventDefault();
  // Not while a dialog is asking something, and not on a held key.
  const asking = el.dialog.open || el.sure.open;
  if (!event.repeat && !busy() && !asking) void runScan();
});

if (!import.meta.env.DEV) {
  // A cleaner should feel like an app, not a web page.
  document.addEventListener("contextmenu", (event) => event.preventDefault());
}

renderCredit();
render();
void runScan();
