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

export interface ProviderScan {
  id: string;
  label: string;
  blurb: string;
  note: string | null;
  about: string;
  vendor: string | null;
  defaultOn: boolean;
  found: boolean;
  bytes: number;
  files: number;
  folders: Folder[];
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

export interface ScanResult {
  isAdmin: boolean;
  disk: DiskInfo | null;
  pendingRestart: number;
  totalFreed: number;
  adapters: Adapter[];
  providers: ProviderScan[];
  lastCleanAt: number | null;
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
  holders: string[];
}

export interface CleanResult {
  preview: boolean;
  providers: ProviderResult[];
  freed: number;
  queuedFiles: number;
  queuedBytes: number;
  failedFiles: number;
  report: string;
}

export interface Progress {
  id: string;
  done: boolean;
  freed: number;
}
