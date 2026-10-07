import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { CleanResult, Progress, ScanResult } from "./types";

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

export async function onProgress(handler: (p: Progress) => void): Promise<() => void> {
  if (inTauri) return listen<Progress>("clean-progress", (event) => handler(event.payload));
  return (await mock()).mockOnProgress(handler);
}
