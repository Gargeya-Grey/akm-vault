import type { Page } from "@playwright/test";
import type { ApplicationNotice, KeyView, OperationNotice } from "../src/types";

export interface BoundaryOptions {
  /** Replaces the single default key. */
  keys?: Partial<KeyView>[];
  recycled?: { key: Partial<KeyView>; deletedAt: string | null }[];
  auditEntries?: { seq: string; ts: string; action: string; keyTitle: string | null; project: string | null; environment: string | null; keyId: string | null; detail: string | null }[];
  /** The journal starts damaged, as after a bad line. `audit_recover` starts a new one. */
  damagedJournal?: boolean;
  /** What the user answers in the native window of `audit_recover`. Default: approve. */
  recovery?: "approve" | "decline";
  /** Keep a native record store, as the real Session does, even with no retained notice. */
  nativeStore?: boolean;
  /**
   * `hold`: a terminal launch never answers, as when the native review is still open.
   * `transport`: native records the launch, and the reply is lost in transport (U2-R1-07).
   */
  terminal?: "hold" | "transport";
  /**
   * `hold`: an import does not answer, as when its native confirmation is still open. The test ends
   * it with `__akmTest.applyImport`, which adds `importAdds` to the keys.
   */
  importReply?: "hold";
  importAdds?: Partial<KeyView>[];
  /** The vault has an older format, so the page offers the format upgrade. */
  needsUpgrade?: boolean;
}

export async function installNativeBoundaryMock(page: Page, theme = "light", retainedNotices?: OperationNotice[], retainedWarnings?: ApplicationNotice[], options: BoundaryOptions = {}) {
  if (retainedNotices || retainedWarnings || options.nativeStore) {
    let operations = retainedNotices ?? [];
    let application = retainedWarnings ?? [];
    // This test-side native store outlives browser reloads, as the real Rust Session does.
    await page.exposeFunction("__akmWarningIPC", (command: string, payload: unknown) => {
      if (command === "vault_dismiss_warnings") {
        const ids = payload as string[];
        operations = operations.filter((notice) => !ids.includes(notice.id));
        application = application.filter((notice) => !ids.includes(notice.id));
        return;
      }
      // The native outcome record of a dispatched request.
      if (command === "record") { operations = [...operations, payload as OperationNotice]; return; }
      return { application, operations };
    });
  }
  await page.addInitScript(({ theme, options }) => {
    localStorage.setItem("akm-theme", theme);
    const callbacks = new Map<number, (event: unknown) => void>();
    const listeners = new Map<string, number>();
    const calls: { command: string; args: Record<string, unknown> }[] = [];
    let next = 0;
    // Not a native reply: native code rejects only with a kind on the closed list in types.ts.
    const transportFailure = () => ({ kind: "transport", message: "Fake channel closed." });
    let unlocked = true;
    let owner: string | null = crypto.randomUUID();
    let claimed = false;
    let path = "C:/fake-test/vault.kdbx";
    const status = () => ({ sessionId: unlocked ? owner : null, path, exists: true, unlocked, keyCount: unlocked ? 1 : 0, vaultName: unlocked ? "UI fixture" : null, needsUpgrade: options.needsUpgrade ?? false });
    const key = {
      id: "test-entry-uuid", title: "OpenAI", username: null, issuer: "OpenAI",
      project: "demo", environment: "dev", scopes: ["chat"], tags: [],
      envVar: "OPENAI_API_KEY", resolvedEnvVar: "OPENAI_API_KEY", url: null,
      notes: null, expiresAt: null, rotationDays: null, createdAt: null,
      updatedAt: null, rotationDueAt: null, secretChangedAt: null, secretChange: "unknown", historyCount: 0,
    };
    let keys = (options.keys ?? [{}]).map((overrides, index) => ({ ...key, id: `${key.id}-${index}`, ...overrides }));
    let answerImport: ((report: unknown) => void) | null = null;
    // The native window was approved: the keys change, and the reply arrives or is lost.
    const applyImport = (reply: boolean) => {
      const added = (options.importAdds ?? []).map((overrides, index) => ({ ...key, id: `${key.id}-imported-${index}`, ...overrides }));
      keys = [...keys, ...added];
      if (reply) answerImport?.({ added: added.length, updated: 0 });
    };
    let damaged = options.damagedJournal ?? false;
    let recovered = false;
    const journalPath = "C:/fake-test/journal/j-00000000000000000000000000000000.jsonl";
    const recoveredEntry = { seq: "1", ts: "2026-11-09T10:15:00Z", action: "journal-recovered", keyTitle: null, project: null, environment: null, keyId: null, detail: "A new journal started after damage. History before the damage cannot be proved complete." };
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
      __akmTest: { calls, emitLock, emitNotice, expireView, applyImport, owner: () => owner },
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
            case "env_launch_terminal": {
              // The contract with native: every launch carries a fresh UUID, and native keeps a record under it.
              if (typeof args.operationId !== "string" || !/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(args.operationId)) {
                throw new Error("env_launch_terminal needs a UUID operationId");
              }
              if (options.terminal === "hold") return new Promise(() => {});
              const bridge = (window as unknown as { __akmWarningIPC?: (command: string, payload: unknown) => Promise<unknown> }).__akmWarningIPC;
              if (bridge) await bridge("record", { id: args.operationId, originSession: owner ?? "", kind: "terminalStarted" });
              // The review's shape: an object with a kind that no native command sends.
              if (options.terminal === "transport") throw transportFailure();
              return keys.filter((candidate) => candidate.project === args.project && candidate.environment === args.environment).length;
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
            case "audit_recover": {
              if (!damaged) throw { kind: "journal", message: "The journal is not damaged, so nothing was changed." };
              if (options.recovery === "decline") return null;
              damaged = false;
              recovered = true;
              return { keptName: "j-00000000000000000000000000000000.jsonl.damaged-20261109T101500Z", sha256: "ab".repeat(32), bytes: 213 };
            }
            case "audit_read": if (damaged) return { path: journalPath, state: "corrupt", damage: "malformedJson", identity: "confirmed", notices: [], brokenAt: null, problemLine: 4, totalEntries: 0, entries: [] };
              else if (recovered) return { path: journalPath, state: "consistent", damage: null, identity: "confirmed", notices: [], brokenAt: null, problemLine: null, totalEntries: 1, entries: [recoveredEntry] };
              else return { path: journalPath, state: "consistent", damage: null, identity: "confirmed", notices: [], brokenAt: null, problemLine: null, totalEntries: options.auditEntries?.length ?? 3, entries: options.auditEntries ?? [
              { seq: "1", ts: "2026-10-31T00:00:00Z", action: "copy", keyTitle: "OpenAI", project: "demo", environment: "dev", keyId: "00000000-0000-4000-8000-000000000001", detail: null },
              { seq: "2", ts: "2026-10-31T00:00:00Z", action: "copy-requested", keyTitle: "OpenAI", project: "demo", environment: "dev", keyId: "00000000-0000-4000-8000-000000000001", detail: null },
              { seq: "3", ts: "2026-10-31T00:00:00Z", action: "add-completed", keyTitle: "OpenAI", project: "demo", environment: "dev", keyId: "00000000-0000-4000-8000-000000000001", detail: "fake detail: 2 keys added, 1 updated" },
            ] };
            case "backup_info": return { path: "C:/fake-test/vault.kdbx.bak", exists: false, bytes: 0, modified: null };
            case "env_import": return options.importReply === "hold" ? new Promise((resolve) => { answerImport = resolve; }) : { added: 0, updated: 1 };
            case "key_history": return [{ index: 0, modifiedAt: "2026-10-30T12:00:00Z", changed: ["Password"], secretChanged: true }];
            case "env_export": return "OPENAI_API_KEY=fake-browser-test-value\n";
            default: throw new Error(`Unexpected test IPC: ${command}`);
          }
        },
      },
    });
  }, { theme, options });
}
