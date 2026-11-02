// Handles to the page, and the one place markup is swapped in.

export const $ = <T extends HTMLElement>(id: string): T => document.getElementById(id) as T;
export const el = {
  brand: $("brand"),
  rescan: $<HTMLButtonElement>("rescan"),
  gpu: $("gpu"),
  notice: $("notice"),
  summary: $("summary"),
  list: $("list"),
  credit: $("credit"),
  scroll: document.querySelector(".scroll") as HTMLElement,
  go: $<HTMLButtonElement>("go"),
  preview: $<HTMLButtonElement>("opt-preview"),
  queue: $<HTMLButtonElement>("opt-queue"),
  links: $("links"),
  dialog: $<HTMLDialogElement>("restart-dialog"),
  confirm: $<HTMLButtonElement>("restart-confirm"),
  sure: $<HTMLDialogElement>("confirm-dialog"),
  sureList: $("confirm-list"),
  announce: $("announce"),
};

/** Says something to a screen reader once. Clearing first makes the same
 *  sentence read again if it is repeated. */
export function announce(text: string): void {
  el.announce.textContent = "";
  window.setTimeout(() => {
    el.announce.textContent = text;
  }, 60);
}


const lastHtml = new WeakMap<HTMLElement, string>();

/** Selector for the focused control inside `root`, built from its data
 *  attributes, so the same control can be found again after a redraw. */
function focusSelector(root: HTMLElement): string | null {
  const active = document.activeElement;
  if (!(active instanceof HTMLElement) || active === root || !root.contains(active)) return null;
  const attrs = [...active.attributes].filter((a) => a.name.startsWith("data-"));
  if (attrs.length === 0) return null;
  const parts = attrs.map((a) => `[${a.name}="${CSS.escape(a.value)}"]`).join("");
  return `${active.tagName.toLowerCase()}${parts}`;
}

/** Replaces a section's markup. Identical markup is skipped, so a spinner keeps
 *  turning, and keyboard focus is put back on the same control afterwards. */
export function setHtml(target: HTMLElement, html: string): void {
  if (lastHtml.get(target) === html) return;
  lastHtml.set(target, html);
  const selector = focusSelector(target);
  target.innerHTML = html;
  if (selector) target.querySelector<HTMLElement>(selector)?.focus({ preventScroll: true });
}

