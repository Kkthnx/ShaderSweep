// Sample data for previewing the interface in a browser. Development only.
// Add ?s=changed, ?s=empty or ?s=old to the address to see other states.
import type { CleanResult, Progress, ProviderScan, ScanResult } from "./types";

const GB = 1024 ** 3;
const MB = 1024 ** 2;
const mode = new URLSearchParams(location.search).get("s") ?? "";
const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

const provider = (
  id: string,
  label: string,
  blurb: string,
  bytes: number,
  extra: Partial<ProviderScan> = {},
): ProviderScan => ({
  id,
  label,
  blurb,
  note: null,
  about: "Compiled shader binaries for your exact GPU and driver. The driver rebuilds them the next time a game needs them, so the first launch afterwards can be slower.",
  vendor: null,
  defaultOn: true,
  found: bytes > 0,
  bytes,
  files: bytes > 0 ? Math.max(1, Math.round(bytes / (4 * MB))) : 0,
  folders:
    bytes > 0
      ? [
          { path: `C:\\Users\\Player\\AppData\\Local\\${label.split(" ")[0]}\\DXCache`, bytes: Math.round(bytes * 0.9) },
          { path: `C:\\Windows\\System32\\config\\systemprofile\\AppData\\Local\\${label.split(" ")[0]}\\DXCache`, bytes: Math.round(bytes * 0.1) },
        ]
      : [],
  ...extra,
});

function providers(): ProviderScan[] {
  if (mode === "empty") {
    return [
      provider("nvidia", "NVIDIA shader cache", "DirectX, OpenGL and compute caches written by the NVIDIA driver.", 0, { vendor: "nvidia" }),
      provider("amd", "AMD shader cache", "DirectX 9, 11, 12, OpenGL and Vulkan caches written by the AMD driver.", 0, { vendor: "amd" }),
      provider("intel", "Intel shader cache", "Shader cache written by the Intel graphics driver.", 0, { vendor: "intel" }),
      provider("windows", "Windows DirectX cache", "The shader cache Windows keeps for every GPU. Disk Cleanup lists it too.", 0),
      provider("steam", "Steam shader cache", "Pre-compiled shaders Steam stores next to your games, in every library.", 0),
      provider("installers", "Driver installer leftovers", "Extracted installers in C:\\AMD and C:\\NVIDIA\\DisplayDriver. Off by default so you can still roll back.", 0, { defaultOn: false }),
    ];
  }
  return [
    provider("nvidia", "NVIDIA shader cache", "DirectX, OpenGL and compute caches written by the NVIDIA driver.", 6.34 * GB, {
      vendor: "nvidia",
      note: "A few index files are held by the display driver. Those are queued for your next restart.",
    }),
    provider("amd", "AMD shader cache", "DirectX 9, 11, 12, OpenGL and Vulkan caches written by the AMD driver.", 0, { vendor: "amd" }),
    provider("intel", "Intel shader cache", "Shader cache written by the Intel graphics driver.", 0, { vendor: "intel" }),
    provider("windows", "Windows DirectX cache", "The shader cache Windows keeps for every GPU. Disk Cleanup lists it too.", 728 * 1024),
    provider("steam", "Steam shader cache", "Pre-compiled shaders Steam stores next to your games, in every library.", 10.7 * MB),
    provider("installers", "Driver installer leftovers", "Extracted installers in C:\\AMD and C:\\NVIDIA\\DisplayDriver. Off by default so you can still roll back.", 2.58 * GB, {
      defaultOn: false,
      note: "Skip this while a driver install is running.",
    }),
  ];
}

export async function mockScan(): Promise<ScanResult> {
  await delay(650);
  const changed = mode === "changed";
  return {
    isAdmin: true,
    disk: { drive: "C:", free: 214.3 * GB, total: 930.5 * GB },
    pendingRestart: mode === "pending" ? 6 : 0,
    adapters: [{ vendor: "nvidia", name: "NVIDIA GeForce RTX 5070", version: "616.92", date: "2026-09-04" }],
    providers: providers(),
    lastCleanAt: mode === "old" || changed ? Math.round(Date.now() / 1000) - 19 * 86400 : null,
    driverChanges: changed
      ? [{ vendor: "nvidia", name: "NVIDIA GeForce RTX 5070", version: "616.92" }]
      : [],
  };
}

let listener: ((p: Progress) => void) | null = null;

export async function mockOnProgress(handler: (p: Progress) => void): Promise<() => void> {
  listener = handler;
  return () => {
    listener = null;
  };
}

export async function mockClean(ids: string[], preview: boolean): Promise<CleanResult> {
  const all = providers();
  const results = [];
  let freed = 0;
  for (const id of ids) {
    listener?.({ id, done: false, freed: 0 });
    await delay(520);
    const found = all.find((p) => p.id === id);
    const bytes = found?.bytes ?? 0;
    const queued = id === "nvidia" && !preview ? 6 : 0;
    const got = id === "nvidia" && !preview ? bytes - 16 * MB : bytes;
    freed += got;
    listener?.({ id, done: true, freed: got });
    results.push({
      id,
      freed: got,
      removedFiles: Math.round(got / (4 * MB)),
      queuedFiles: queued,
      queuedBytes: queued ? 16 * MB : 0,
      failedFiles: 0,
      failedBytes: 0,
      holders: [],
    });
  }
  return {
    preview,
    providers: results,
    freed,
    queuedFiles: results.reduce((n, r) => n + r.queuedFiles, 0),
    queuedBytes: results.reduce((n, r) => n + r.queuedBytes, 0),
    failedFiles: 0,
  };
}
