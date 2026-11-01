import type { Page } from "@playwright/test";

export async function installNativeBoundaryMock(page: Page, theme = "light") {
  await page.addInitScript(({ theme }) => {
    localStorage.setItem("akm-theme", theme);
    const callbacks = new Map<number, (event: unknown) => void>();
    const listeners = new Map<string, number>();
    const calls: { command: string; args: Record<string, unknown> }[] = [];
    let next = 0;
    let unlocked = true;
    const key = {
      id: "test-entry-uuid", title: "OpenAI", username: null, issuer: "OpenAI",
      project: "demo", environment: "dev", scopes: ["chat"], tags: [],
      envVar: "OPENAI_API_KEY", resolvedEnvVar: "OPENAI_API_KEY", url: null,
      notes: null, expiresAt: null, rotationDays: null, createdAt: null,
      updatedAt: null, expired: false, rotationDueIn: null,
    };
    const emitLock = () => {
      unlocked = false;
      const listener = listeners.get("vault-locked");
      if (listener !== undefined) callbacks.get(listener)?.({ event: "vault-locked", id: listener, payload: null });
    };
    Object.assign(window, {
      __akmTest: { calls, emitLock },
      __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} },
      __TAURI_INTERNALS__: {
        transformCallback: (callback: (event: unknown) => void) => { callbacks.set(++next, callback); return next; },
        unregisterCallback: (id: number) => callbacks.delete(id),
        invoke: async (command: string, args: Record<string, unknown>) => {
          calls.push({ command, args });
          switch (command) {
            case "plugin:event|listen": listeners.set(args.event as string, args.handler as number); return args.handler;
            case "plugin:event|unlisten": return;
            case "vault_status": return { path: "C:/fake-test/vault.kdbx", exists: true, unlocked, keyCount: unlocked ? 1 : 0, vaultName: "UI fixture", needsUpgrade: false };
            case "vault_warnings": return [];
            case "vault_activity": return;
            case "vault_lock": emitLock(); return;
            case "keys_list": return [key];
            case "backup_info": return { path: "C:/fake-test/vault.kdbx.bak", exists: false, bytes: 0, modified: null };
            case "env_import_preview": return { token: "preview-token", added: 0, updated: 1, unchanged: 0, changes: [{ name: "OPENAI_API_KEY", title: "OpenAI", action: "update" }] };
            case "env_import_confirm": return { added: 0, updated: 1 };
            case "env_export": return "OPENAI_API_KEY=fake-browser-test-value\n";
            default: throw new Error(`Unexpected test IPC: ${command}`);
          }
        },
      },
    });
  }, { theme });
}
