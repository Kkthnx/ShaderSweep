import { el } from "../dom";
import { icons } from "../icons";
import type { LinkName } from "../types";

const LINK_ICONS: Record<LinkName, string> = {
  github: icons.github,
  website: icons.globe,
  source: icons.code,
  issues: icons.flag,
  releases: icons.download,
};

/** One link button. The backend decides where it goes, the page only names it. */
export function linkButton(name: LinkName, text: string): string {
  return `<button class="linkbtn" type="button" data-link="${name}">${LINK_ICONS[name]}<span>${text}</span></button>`;
}

export function renderCredit(): void {
  el.credit.innerHTML = [
    linkButton("github", "Made by Kkthnx"),
    linkButton("website", "kkthnx.com"),
    linkButton("issues", "Report a problem"),
  ].join("");
}
