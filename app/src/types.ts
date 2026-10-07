export interface Adapter {
  vendor: string;
  name: string;
  version: string;
  date: string;
}

export interface Folder {
  path: string;
  bytes: number;
}

export type GroupId = "shaders" | "launchers" | "housekeeping";

export interface ProviderScan {
  id: string;
  label: string;
  blurb: string;
  note: string | null;
  caution: string | null;
  about: string;
  group: GroupId;
  vendor: string | null;
  defaultOn: boolean;
  found: boolean;
  bytes: number;
  files: number;
  folders: Folder[];
  running: string[];
}

export interface DriverChange {
  vendor: string;
  name: string;
  version: string;
}

export interface DiskInfo {
  drive: string;
  free: number;
  total: number;
}

export interface QueueCheck {
  restarted: boolean;
  removed: number;
  removedBytes: number;
  remaining: number;
  remainingBytes: number;
}

export interface ScanResult {
  isAdmin: boolean;
  disk: DiskInfo | null;
  pendingRestart: number;
  queueCheck: QueueCheck | null;
  adapters: Adapter[];
  providers: ProviderScan[];
  lastCleanAt: number | null;
  totalFreed: number;
  driverChanges: DriverChange[];
}

export interface ProviderResult {
  id: string;
  freed: number;
  removedFiles: number;
  queuedFiles: number;
  queuedBytes: number;
  failedFiles: number;
  failedBytes: number;
  skippedRecent: number;
  holders: string[];
}

export interface CleanResult {
  preview: boolean;
  providers: ProviderResult[];
  freed: number;
  queuedFiles: number;
  queuedBytes: number;
  failedFiles: number;
  cancelled: boolean;
  report: string;
}

export interface Progress {
  id: string;
  done: boolean;
  freed: number;
  files: number;
  current: string | null;
}

export interface ScanStep {
  label: string;
  checked: number;
  total: number;
}

export type LinkName = "github" | "website" | "source" | "issues" | "releases";
