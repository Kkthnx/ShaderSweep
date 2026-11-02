import { syncClock } from "./clock";
import { renderDock } from "./views/dock";
import { renderGpu } from "./views/gpu";
import { renderList } from "./views/list";
import { renderNotice } from "./views/notices";
import { renderSummary } from "./views/summary";

export function render(): void {
  renderGpu();
  renderNotice();
  renderSummary();
  renderList();
  renderDock();
  syncClock();
}
