import { el } from "../dom";
import { formatBytes } from "../format";
import { busy, chosen, state } from "../state";

export function renderDock(): void {
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
