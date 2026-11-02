import type { CleanResult, GroupId, ProviderScan, QueueCheck, ScanResult, ScanStep } from "./types";

export type Phase = "scanning" | "ready" | "cleaning" | "done" | "error";

/** How long Windows counts down before it restarts. */
export const RESTART_SECONDS = 60;

/** How many recent file names the activity feed keeps. */
export const ACTIVITY_LINES = 5;

/** Rows whose contents cannot be brought back. A real clean asks first. */
export const IRREVERSIBLE = new Set(["recyclebin", "eventlogs"]);

export const GROUPS: { id: GroupId; title: string; hint: string; hideMissing: boolean }[] = [
  { id: "shaders", title: "Shader caches", hint: "Rebuilt by your GPU driver and Windows", hideMissing: false },
  { id: "launchers", title: "Game launchers and games", hint: "Saved web pages and images. Logins stay", hideMissing: true },
  { id: "housekeeping", title: "Windows housekeeping", hint: "General cleanup. Anything risky starts off", hideMissing: true },
];

export type RestartState =
  | { kind: "counting"; endsAt: number }
  | { kind: "error"; message: string }
  | null;

export const state = {
  phase: "scanning" as Phase,
  scan: null as ScanResult | null,
  scanStep: null as ScanStep | null,
  result: null as CleanResult | null,
  error: "",
  selected: new Set<string>(),
  expanded: new Set<string>(),
  openGroups: new Set<GroupId>(["shaders"]),
  openNotices: new Set<string>(),
  progress: new Map<string, { done: boolean; freed: number; files: number }>(),
  activity: [] as string[],
  startedAt: 0,
  now: Date.now(),
  cancelling: false,
  preview: false,
  queue: true,
  copied: false,
  /** Free space on the system drive when the clean began, to show the change. */
  diskBefore: null as number | null,
  /** What the last restart did to the queued files, kept until dismissed. */
  settled: null as QueueCheck | null,
  restart: null as RestartState,
};

// Remembered between runs. Storage can be unavailable, so it is never required.
export const store = {
  get(key: string, fallback: boolean): boolean {
    try {
      const raw = localStorage.getItem(`shadersweep.${key}`);
      return raw === null ? fallback : raw === "1";
    } catch {
      return fallback;
    }
  },
  set(key: string, value: boolean): void {
    try {
      localStorage.setItem(`shadersweep.${key}`, value ? "1" : "0");
    } catch {
      /* the choice just will not be remembered */
    }
  },
};

export const busy = () => state.phase === "scanning" || state.phase === "cleaning";

/** Shader rows always show, so you can see what was checked. Everywhere else
 *  a row only matters if there is something in it to remove. */
export function hasData(p: ProviderScan): boolean {
  return p.found && (p.group === "shaders" || p.bytes > 0);
}

export function chosen(): ProviderScan[] {
  return (state.scan?.providers ?? []).filter((p) => hasData(p) && state.selected.has(p.id));
}

export function vendorName(vendor: string): string {
  return { nvidia: "NVIDIA", amd: "AMD", intel: "Intel" }[vendor] ?? vendor;
}

export function shortPath(path: string): string {
  // The tail of a path is the part that tells files apart.
  const parts = path.split("\\");
  return parts.length > 4 ? `...\\${parts.slice(-4).join("\\")}` : path;
}

export function expectedBytes(): number {
  return chosen().reduce((n, p) => n + p.bytes, 0);
}

export function defaultSelection(data: ScanResult): Set<string> {
  // A row for a running app starts unticked, since its files are locked.
  return new Set(
    data.providers
      .filter((p) => p.defaultOn && hasData(p) && p.running.length === 0)
      .map((p) => p.id),
  );
}
