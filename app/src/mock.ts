// Sample data for previewing the interface in a browser. Development only.
// Add ?s=changed, empty, old, pending, settled, stuck, running or fast to the
// address to see other states.
import type {
  CleanResult,
  GroupId,
  Progress,
  ProviderScan,
  ScanResult,
  ScanStep,
} from "./types";

const GB = 1024 ** 3;
const MB = 1024 ** 2;
const mode = new URLSearchParams(location.search).get("s") ?? "";
const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

const SHADER =
  "Compiled shader binaries for your exact GPU and driver. The driver rebuilds them the next time a game needs them, so the first launch afterwards can be slower.";

interface Seed {
  id: string;
  label: string;
  blurb: string;
  group: GroupId;
  bytes: number;
  on?: boolean;
  vendor?: string;
  running?: string[];
  caution?: string;
  note?: string;
  about?: string;
}

const SEEDS: Seed[] = [
  { id: "nvidia", label: "NVIDIA shader cache", blurb: "DirectX, OpenGL and compute caches written by the NVIDIA driver.", group: "shaders", bytes: 6.34 * GB, vendor: "nvidia", note: "A few index files are held by the display driver. Those are queued for your next restart." },
  { id: "amd", label: "AMD shader cache", blurb: "DirectX 9, 11, 12, OpenGL and Vulkan caches written by the AMD driver.", group: "shaders", bytes: 0, vendor: "amd" },
  { id: "intel", label: "Intel shader cache", blurb: "Shader cache written by the Intel graphics driver.", group: "shaders", bytes: 0, vendor: "intel" },
  { id: "windows", label: "Windows DirectX cache", blurb: "The shader cache Windows keeps for every GPU. Disk Cleanup lists it too.", group: "shaders", bytes: 1.4 * MB },
  { id: "steam", label: "Steam shader cache", blurb: "Pre-compiled shaders Steam stores next to your games, in every library.", group: "shaders", bytes: 10.7 * MB },
  { id: "discord", label: "Discord cache", blurb: "Saved images, media, scripts and GPU data from Discord, including PTB and Canary.", group: "launchers", bytes: 364 * MB, running: mode === "running" ? ["Discord.exe"] : [], about: "Discord's Cache, Code Cache and GPU cache folders. Clearing them does not log you out or touch messages, servers or friends." },
  { id: "epic", label: "Epic Games Launcher cache", blurb: "The launcher's saved web pages and images.", group: "launchers", bytes: 241 * MB },
  { id: "battlenet", label: "Battle.net cache", blurb: "The Battle.net app's saved web content and downloaded data.", group: "launchers", bytes: 1.17 * GB },
  { id: "steamclient", label: "Steam client web cache", blurb: "The saved pages and images behind Steam's store, library and overlay.", group: "launchers", bytes: 830 * MB, running: mode === "running" ? ["steam.exe", "steamwebhelper.exe"] : [] },
  { id: "ubisoft", label: "Ubisoft Connect cache", blurb: "Saved web content from Ubisoft Connect.", group: "launchers", bytes: 682 * MB },
  { id: "wow", label: "World of Warcraft cache", blurb: "The Cache folder of each game version, such as retail, Classic and the PTR.", group: "launchers", bytes: 14 * MB, on: false, note: "The first login afterwards re-downloads item and quest data, so it loads slower once." },
  { id: "temp", label: "Temporary files", blurb: "Files apps left in your Temp folder and in Windows\\Temp.", group: "housekeeping", bytes: 135 * MB, note: "Files changed in the last 24 hours are skipped, so a running installer is not pulled out from under itself." },
  { id: "thumbnails", label: "Thumbnail cache", blurb: "The pictures File Explorer saved of your files and folders.", group: "housekeeping", bytes: 9 * MB },
  { id: "deliveryopt", label: "Delivery Optimization files", blurb: "Windows Update download pieces Windows keeps to share with other PCs.", group: "housekeeping", bytes: 2.1 * GB },
  { id: "prefetch", label: "Prefetch files", blurb: "Windows' record of which files apps load at start.", group: "housekeeping", bytes: 8 * MB, on: false, caution: "This does not make anything faster. Apps start slightly slower the first time afterwards." },
  { id: "gpudumps", label: "GPU crash dumps", blurb: "Windows kernel dumps written when the graphics driver stops responding.", group: "housekeeping", bytes: 7.25 * GB, on: false, caution: "Keep these if a driver vendor or Microsoft support may ask for one." },
  { id: "recyclebin", label: "Recycle Bin", blurb: "Everything in the Recycle Bin on every drive.", group: "housekeeping", bytes: 312 * MB, on: false, caution: "This permanently deletes your files. It cannot be undone." },
  { id: "eventlogs", label: "Event Viewer logs", blurb: "Application, System and the other Windows event logs. The Security log is never cleared.", group: "housekeeping", bytes: 285 * MB, on: false, caution: "These logs are the record of what went wrong. Clearing them cannot be undone." },
  { id: "installers", label: "Driver installer leftovers", blurb: "Extracted installers in C:\\AMD and C:\\NVIDIA\\DisplayDriver. Off by default so you can still roll back.", group: "housekeeping", bytes: 2.58 * GB, on: false, note: "Skip this while a driver install is running." },
];

function providers(): ProviderScan[] {
  return SEEDS.map((s) => {
    const bytes = mode === "empty" ? 0 : s.bytes;
    return {
      id: s.id,
      label: s.label,
      blurb: s.blurb,
      note: s.note ?? null,
      caution: s.caution ?? null,
      about: s.about ?? (s.group === "shaders" ? SHADER : "Regenerable data. The app rebuilds it when it needs it."),
      group: s.group,
      vendor: s.vendor ?? null,
      defaultOn: s.on ?? true,
      found: bytes > 0,
      bytes,
      files: bytes > 0 ? Math.max(1, Math.round(bytes / (4 * MB))) : 0,
      folders:
        bytes > 0 && s.group !== "housekeeping"
          ? [
              { path: `C:\\Users\\Player\\AppData\\Local\\${s.label.split(" ")[0]}\\Cache`, bytes: Math.round(bytes * 0.9) },
              { path: `C:\\Windows\\System32\\config\\systemprofile\\AppData\\Local\\${s.label.split(" ")[0]}\\Cache`, bytes: Math.round(bytes * 0.1) },
            ]
          : [],
      running: s.running ?? [],
    };
  });
}

let scanListener: ((s: ScanStep) => void) | null = null;
export async function mockOnScanStep(handler: (s: ScanStep) => void): Promise<() => void> {
  scanListener = handler;
  return () => {
    scanListener = null;
  };
}

export async function mockScan(): Promise<ScanResult> {
  const list = SEEDS.map((s) => s.label);
  for (let i = 0; i < list.length; i++) {
    scanListener?.({ label: list[i], checked: i + 1, total: list.length });
    await delay(mode === "fast" ? 5 : 70);
  }
  const changed = mode === "changed";
  const settled = mode === "settled";
  return {
    isAdmin: true,
    disk: { drive: "C:", free: 214.3 * GB, total: 930.5 * GB },
    pendingRestart: mode === "pending" ? 6 : 0,
    queueCheck: settled
      ? { restarted: true, removed: 10, removedBytes: 34.2 * MB, remaining: 0, remainingBytes: 0 }
      : mode === "stuck"
        ? { restarted: true, removed: 7, removedBytes: 20 * MB, remaining: 3, remainingBytes: 14 * MB }
        : null,
    adapters: [{ vendor: "nvidia", name: "NVIDIA GeForce RTX 5070", version: "616.92", date: "2026-09-04" }],
    providers: providers(),
    lastCleanAt: mode === "old" || changed ? Math.round(Date.now() / 1000) - 19 * 86400 : null,
    totalFreed: mode === "old" || changed ? 31.2 * GB : 0,
    driverChanges: changed ? [{ vendor: "nvidia", name: "NVIDIA GeForce RTX 5070", version: "616.92" }] : [],
  };
}

let listener: ((p: Progress) => void) | null = null;
let cancelled = false;

export async function mockOnProgress(handler: (p: Progress) => void): Promise<() => void> {
  listener = handler;
  return () => {
    listener = null;
  };
}

export function mockCancel(): void {
  cancelled = true;
}

export async function mockRestart(delaySeconds: number): Promise<number> {
  return Math.min(Math.max(delaySeconds, 15), 300);
}

const SAMPLE_FILES = [
  "DXCache\\33cba91a2ac6783f.nvph",
  "DXCache\\sub\\33cba91ae4e0c354.nvph",
  "GLCache\\d390cfad34d302a5\\2ed2f37c0bcb4667\\fd0b.bin",
  "Cache\\Cache_Data\\f_000b23",
  "Cache\\Cache_Data\\data_3",
  "Code Cache\\js\\a1b2c3d4e5f6_0",
  "webcache_4430\\Service Worker\\CacheStorage\\ebda3e26\\0eb04561_1",
  "BrowserCaches\\70464630\\Cache\\Cache_Data\\data_2",
];

export async function mockClean(ids: string[], preview: boolean): Promise<CleanResult> {
  cancelled = false;
  const all = providers();
  const results = [];
  let freed = 0;

  for (const id of ids) {
    if (cancelled) break;
    const found = all.find((p) => p.id === id);
    const bytes = found?.bytes ?? 0;
    listener?.({ id, done: false, freed: 0, files: 0, current: null });

    // Stream a believable run: many small steps, each naming a file.
    const steps = mode === "fast" ? 3 : 28;
    let done = 0;
    for (let i = 1; i <= steps; i++) {
      if (cancelled) break;
      await delay(mode === "fast" ? 4 : 55);
      done = Math.round((bytes * i) / steps);
      listener?.({
        id,
        done: false,
        freed: done,
        files: i * 3,
        current: `C:\\Users\\Player\\AppData\\Local\\${SAMPLE_FILES[i % SAMPLE_FILES.length]}`,
      });
    }

    const queued = id === "nvidia" && !preview && !cancelled ? 6 : 0;
    const got = queued ? done - 16 * MB : done;
    freed += got;
    listener?.({ id, done: true, freed: got, files: Math.round(got / (4 * MB)), current: null });
    results.push({
      id,
      freed: got,
      removedFiles: Math.round(got / (4 * MB)),
      queuedFiles: queued,
      queuedBytes: queued ? 16 * MB : 0,
      failedFiles: 0,
      failedBytes: 0,
      skippedRecent: id === "temp" ? 3 : 0,
      holders: [] as string[],
    });
  }

  return {
    preview,
    providers: results,
    freed,
    queuedFiles: results.reduce((n, r) => n + r.queuedFiles, 0),
    queuedBytes: results.reduce((n, r) => n + r.queuedBytes, 0),
    failedFiles: 0,
    cancelled,
    report: "ShaderSweep 2.1.0\nClean on 2026-10-07 20:06 UTC",
  };
}
