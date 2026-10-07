import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { CleanResult, LinkName, Progress, ScanResult, ScanStep } from "./types";

// A plain browser has no Tauri runtime. In development that means a preview
// with sample data, which is how the interface is designed and checked.
const inTauri = "__TAURI_INTERNALS__" in window;

async function mock() {
  if (!import.meta.env.DEV) throw new Error("ShaderSweep must run inside its own window.");
  return import("./mock");
}

export async function scan(): Promise<ScanResult> {
  return inTauri ? invoke<ScanResult>("scan") : (await mock()).mockScan();
}

export async function clean(
  ids: string[],
  preview: boolean,
  queueLocked: boolean,
): Promise<CleanResult> {
  return inTauri
    ? invoke<CleanResult>("clean", { ids, preview, queueLocked })
    : (await mock()).mockClean(ids, preview);
}

export async function cancelClean(): Promise<void> {
  if (inTauri) await invoke("cancel_clean");
  else (await mock()).mockCancel();
}

export async function onProgress(handler: (p: Progress) => void): Promise<() => void> {
  if (inTauri) return listen<Progress>("clean-progress", (event) => handler(event.payload));
  return (await mock()).mockOnProgress(handler);
}

export async function onScanStep(handler: (s: ScanStep) => void): Promise<() => void> {
  if (inTauri) return listen<ScanStep>("scan-step", (event) => handler(event.payload));
  return (await mock()).mockOnScanStep(handler);
}

export async function reveal(path: string): Promise<void> {
  if (inTauri) await invoke("reveal", { path });
}

/** Opens one of the fixed links. The page never supplies an address. */
export async function openLink(name: LinkName): Promise<void> {
  if (inTauri) await invoke("open_link", { name });
  else window.open("about:blank#" + name, "_blank");
}

/** Starts a restart countdown and returns its length in seconds. */
export async function restartPc(delaySeconds: number): Promise<number> {
  return inTauri
    ? invoke<number>("restart_pc", { delaySeconds })
    : (await mock()).mockRestart(delaySeconds);
}

export async function cancelRestart(): Promise<void> {
  if (inTauri) await invoke("cancel_restart");
}
