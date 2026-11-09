import type { Page } from "@playwright/test";
import type { ApplicationNotice, KeyView, OperationNotice, ReminderStatus } from "../src/types";

export interface BoundaryOptions {
  /** Replaces the single default key. */
  keys?: Partial<KeyView>[];
  recycled?: { key: Partial<KeyView>; deletedAt: string | null }[];
  auditEntries?: { seq: string; ts: string; action: string; keyTitle: string | null; project: string | null; environment: string | null; keyId: string | null; detail: string | null }[];
  reminders?: ReminderStatus;
}

export async function installNativeBoundaryMock(page: Page, theme = "light", retainedNotices?: OperationNotice[], retainedWarnings?: ApplicationNotice[], options: BoundaryOptions = {}) {
  if (retainedNotices || retainedWarnings) {
    let operations = retainedNotices ?? [];
    let application = retainedWarnings ?? [];
    // This test-side native store outlives browser reloads, as the real Rust Session does.
    await page.exposeFunction("__akmWarningIPC", (command: string, ids: string[]) => {
      if (command === "vault_dismiss_warnings") {
        operations = operations.filter((notice) => !ids.includes(notice.id));
        application = application.filter((notice) => !ids.includes(notice.id));
        return;
      }
      return { application, operations };
    });
  }
  await page.addInitScript(({ theme, options }) => {
    localStorage.setItem("akm-theme", theme);
    const callbacks = new Map<number, (event: unknown) => void>();
    const listeners = new Map<string, number>();
    const calls: { command: string; args: Record<string, unknown> }[] = [];
    let next = 0;
    let unlocked = true;
    let owner: string | null = crypto.randomUUID();
    let claimed = false;
    let path = "C:/fake-test/vault.kdbx";
    const status = () => ({ sessionId: unlocked ? owner : null, path, exists: true, unlocked, keyCount: unlocked ? 1 : 0, vaultName: unlocked ? "UI fixture" : null, needsUpgrade: false });
    const key = {
      id: "test-entry-uuid", title: "OpenAI", username: null, issuer: "OpenAI",
      project: "demo", environment: "dev", scopes: ["chat"], tags: [],
      envVar: "OPENAI_API_KEY", resolvedEnvVar: "OPENAI_API_KEY", url: null,
      notes: null, expiresAt: null, rotationDays: null, createdAt: null,
      updatedAt: null, rotationDueAt: null, secretChangedAt: null, secretChange: "unknown", historyCount: 0,
    };
    const keys = (options.keys ?? [{}]).map((overrides, index) => ({ ...key, id: `${key.id}-${index}`, ...overrides }));
    let reminders = options.reminders ?? { enabled: false, permission: "granted", delivery: "none", blocked: false };
    const emitNotice = (sessionId: string) => {
      const listener = listeners.get("vault-locked");
      if (listener !== undefined) callbacks.get(listener)?.({ event: "vault-locked", id: listener, payload: { sessionId } });
    };
    const emitLock = () => {
      const invalidated = owner;
      owner = null;
      unlocked = false;
      if (invalidated !== null) emitNotice(invalidated);
    };
    const expireView = (sessionId = owner, hideId = crypto.randomUUID()) => {
      const listener = listeners.get("vault-view-expired");
      if (sessionId !== null && listener !== undefined) callbacks.get(listener)?.({ event: "vault-view-expired", id: listener, payload: { sessionId, hideId } });
      return hideId;
    };
    Object.assign(window, {
      __akmTest: { calls, emitLock, emitNotice, expireView, owner: () => owner },
      __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} },
      __TAURI_INTERNALS__: {
        transformCallback: (callback: (event: unknown) => void) => { callbacks.set(++next, callback); return next; },
        unregisterCallback: (id: number) => callbacks.delete(id),
        invoke: async (command: string, args: Record<string, unknown>) => {
          calls.push({ command, args });
          switch (command) {
            case "plugin:event|listen": listeners.set(args.event as string, args.handler as number); return args.handler;
            case "plugin:event|unlisten": return;
            case "vault_status": return status();
            case "vault_prepare_open": {
              emitLock();
              owner = crypto.randomUUID();
              claimed = false;
              return owner;
            }
            case "vault_unlock":
            case "vault_create": {
              if (owner === null || args.sessionId !== owner || claimed) throw { kind: "locked", message: "Invalid test reservation" };
              claimed = true;
              path = args.path as string;
              unlocked = true;
              return status();
            }
            case "vault_reload":
            case "vault_upgrade": return status();
            case "vault_warnings":
            case "vault_dismiss_warnings": {
              const bridge = (window as unknown as { __akmWarningIPC?: (command: string, ids: unknown) => Promise<unknown> }).__akmWarningIPC;
              return bridge ? bridge(command, args?.ids ?? []) : { application: [], operations: [] };
            }
            case "vault_activity": return;
            case "view_discard": return;
            case "vault_lock": {
              if (args.sessionId !== owner) throw { kind: "locked", message: "A different test native session is current" };
              emitLock();
              return;
            }
            case "keys_list": return keys;
            case "recycled_list": return (options.recycled ?? []).map((item, index) => ({ deletedAt: item.deletedAt, key: { ...key, id: `${key.id}-bin-${index}`, ...item.key } }));
            case "reminders_status": return reminders;
            case "reminders_set": reminders = { ...reminders, enabled: args.enabled as boolean, delivery: "none", blocked: false }; return reminders;
            case "reminders_notify": reminders = { ...reminders, delivery: "sent" }; return reminders;
            case "audit_read": return { path: "C:/fake-test/journal/j-00000000000000000000000000000000.jsonl", state: "consistent", damage: null, identity: "confirmed", notices: [], brokenAt: null, problemLine: null, totalEntries: options.auditEntries?.length ?? 3, entries: options.auditEntries ?? [
              { seq: "1", ts: "2026-10-31T00:00:00Z", action: "copy", keyTitle: "OpenAI", project: "demo", environment: "dev", keyId: "00000000-0000-4000-8000-000000000001", detail: null },
              { seq: "2", ts: "2026-10-31T00:00:00Z", action: "copy-requested", keyTitle: "OpenAI", project: "demo", environment: "dev", keyId: "00000000-0000-4000-8000-000000000001", detail: null },
              { seq: "3", ts: "2026-10-31T00:00:00Z", action: "add-completed", keyTitle: "OpenAI", project: "demo", environment: "dev", keyId: "00000000-0000-4000-8000-000000000001", detail: "fake detail: 2 keys added, 1 updated" },
            ] };
            case "backup_info": return { path: "C:/fake-test/vault.kdbx.bak", exists: false, bytes: 0, modified: null };
            case "env_import": return { added: 0, updated: 1 };
            case "key_history": return [{ index: 0, modifiedAt: "2026-10-30T12:00:00Z", changed: ["Password"], secretChanged: true }];
            case "env_launch_terminal": return keys.filter((candidate) => candidate.project === args.project && candidate.environment === args.environment).length;
            case "env_export": return "OPENAI_API_KEY=fake-browser-test-value\n";
            default: throw new Error(`Unexpected test IPC: ${command}`);
          }
        },
      },
    });
  }, { theme, options });
}
