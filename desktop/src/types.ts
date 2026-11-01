export interface Status {
  path: string;
  exists: boolean;
  unlocked: boolean;
  keyCount: number;
  vaultName: string | null;
  needsUpgrade: boolean;
}

export interface KeyView {
  id: string;
  title: string;
  username: string | null;
  issuer: string | null;
  project: string | null;
  environment: string | null;
  scopes: string[];
  envVar: string | null;
  resolvedEnvVar: string;
  url: string | null;
  notes: string | null;
  tags: string[];
  expiresAt: string | null;
  rotationDays: number | null;
  createdAt: string | null;
  updatedAt: string | null;
  expired: boolean;
  rotationDueIn: number | null;
}

export interface KeyDraft {
  title: string;
  secret?: string | null;
  username?: string | null;
  issuer?: string | null;
  project?: string | null;
  environment?: string | null;
  scopes: string[];
  envVar?: string | null;
  url?: string | null;
  notes?: string | null;
  tags: string[];
  expiresAt: string | null;
  rotationDays: number | null;
}

export interface AuditEntry {
  seq: number;
  ts: string;
  action: string;
  keyTitle: string | null;
  project: string | null;
  environment: string | null;
  detail: string | null;
}

export interface AuditReport {
  path: string;
  entries: AuditEntry[];
  brokenAt: number | null;
}

export interface ImportReport {
  added: number;
  updated: number;
}

export interface ImportPreview extends ImportReport {
  unchanged: number;
  token: string;
  changes: { name: string; title: string; action: "add" | "update" | "unchanged" }[];
}

export interface BackupInfo {
  path: string;
  exists: boolean;
  bytes: number;
  modified: string | null;
}

export interface CommandError {
  kind: string;
  message: string;
}

export type View = "keys" | "activity" | "settings";
export type Theme = "light" | "dark";
