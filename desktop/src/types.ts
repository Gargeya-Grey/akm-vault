export interface Status {
  sessionId: string | null;
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
  /** When the rotation interval elapses. Null when AKM has no secret-change time or no interval. */
  rotationDueAt: string | null;
  /** When the secret last changed. Null when AKM has no such time. */
  secretChangedAt: string | null;
  secretChange: SecretChange;
  /** The number of earlier versions in the vault's native history. */
  historyCount: number;
}

export type SecretChange = "known" | "unknown" | "changed_outside";

/** The receipt of a rotation. It holds times and metadata, never a secret. */
export interface RotationReceipt {
  key: KeyView;
  changedAt: string;
  previousChangedAt: string | null;
}

/** One earlier version of a key. It holds a time and field names, never a value. */
export interface HistoryVersion {
  /** 0 is the version that the newest change replaced. */
  index: number;
  modifiedAt: string | null;
  /** The KDBX field names that differ from the version that replaced this one. */
  changed: string[];
  secretChanged: boolean;
}

export interface RecycledKey {
  key: KeyView;
  deletedAt: string | null;
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
  seq: string;
  ts: string;
  action: string;
  keyTitle: string | null;
  project: string | null;
  environment: string | null;
  keyId: string | null;
  detail: string | null;
}

export interface JournalNotice {
  kind: "legacySidecar" | "identityUnconfirmed" | "earlierJournal" | "relatedJournal";
  message: string;
}

export interface AuditReport {
  path: string;
  entries: AuditEntry[];
  brokenAt: string | null;
  state: "absent" | "empty" | "consistent" | "corrupt";
  damage: "truncatedTail" | "malformedJson" | "brokenLink" | "hashMismatch" | null;
  identity: "confirmed" | "unconfirmed";
  notices: JournalNotice[];
  problemLine: number | null;
  totalEntries: number;
}

/** The result of starting a new journal after damage (FR-14). The damaged file is kept beside it. */
export interface JournalRecovery {
  /** File name only. The folder is the one shown as the journal path. */
  keptName: string;
  /** SHA-256 of the kept bytes, as 64 lowercase hex digits. */
  sha256: string;
  bytes: number;
}

export interface OperationNotice {
  id: string;
  originSession: string;
  kind:
    | "journal" | "backupRefresh" | "backupPrune" | "backupRetention" | "exportJournal" | "preference" | "filePermissions" | "journalLineageUnconfirmed"
    | "importApplied" | "importSessionEnded" | "importFailed" | "importDeclined"
    | "terminalStarted" | "terminalSessionEnded" | "terminalFailed" | "terminalDeclined";
}

export interface WarningReport {
  application: ApplicationNotice[];
  operations: OperationNotice[];
}

export interface ApplicationNotice {
  id: string;
  message: string;
}

export interface ImportReport {
  added: number;
  updated: number;
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

/**
 * The opt-in notification, as the native layer reports it. `blocked` means that AKM stopped
 * trying for this run, after a refused permission or a failed notification.
 */
export interface ReminderStatus {
  enabled: boolean;
  permission: "granted" | "denied" | "prompt";
  delivery: "none" | "sent" | "failed";
  blocked: boolean;
}

export type View = "keys" | "use" | "bin" | "activity" | "settings";
export type Theme = "light" | "dark";
