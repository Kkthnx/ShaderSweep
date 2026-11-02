import { state } from "./state";
import { renderNotice } from "./views/notices";
import { renderSummary } from "./views/summary";

/** Drives the elapsed time and the restart countdown. It only runs while one
 *  of them is on screen, so an idle window wakes nothing. */
let timer: number | undefined;

function tick(): void {
  state.now = Date.now();
  if (state.phase === "cleaning") renderSummary();
  if (state.restart?.kind === "counting") renderNotice();
}

export function syncClock(): void {
  const needed = state.phase === "cleaning" || state.restart?.kind === "counting";
  if (needed && timer === undefined) {
    timer = window.setInterval(tick, 500);
  } else if (!needed && timer !== undefined) {
    window.clearInterval(timer);
    timer = undefined;
  }
}
