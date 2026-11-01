import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, errorText } from "./api";
import { Activity } from "./components/Activity";
import { KeysView } from "./components/KeysView";
import { NavBar } from "./components/NavBar";
import { ConfirmDialog, Notice } from "./components/Notice";
import { Settings } from "./components/Settings";
import { Unlock } from "./components/Unlock";
import { useTheme } from "./theme";
import type { Status, View } from "./types";
import { COMPLETION_EVENT, COMPLETION_MESSAGE, invalidateSession, sessionGeneration } from "./sessionBoundary";

export function App() {
  const { theme, toggle } = useTheme();
  const [status, setStatus] = useState<Status | null>(null);
  const [view, setView] = useState<View>("keys");
  const [error, setError] = useState<string | null>(null);
  const [warnings, setWarnings] = useState<string[]>([]);
  const [confirmUpgrade, setConfirmUpgrade] = useState(false);
  const [upgradeBusy, setUpgradeBusy] = useState(false);
  const [revision, setRevision] = useState(0);
  const [completed, setCompleted] = useState(false);
  const [bootInterrupted, setBootInterrupted] = useState(false);
  const [sessionEpoch, setSessionEpoch] = useState(0);
  const epoch = useRef(0);
  const transition = useRef(0);
  const request = useRef(0);
  const renderedEpoch = epoch.current;

  const showLocked = useCallback(() => {
    invalidateSession();
    request.current += 1;
    epoch.current += 1;
    transition.current += 1;
    setSessionEpoch(epoch.current);
    setBootInterrupted(true);
    setStatus((current) => current ? { ...current, unlocked: false, keyCount: 0, vaultName: null } : current);
    setView("keys");
    setConfirmUpgrade(false);
    setUpgradeBusy(false);
    setWarnings([]);
  }, []);

  useEffect(() => {
    const acknowledge = (event: Event) => {
      const origin = (event as CustomEvent<{ generation: number }>).detail?.generation;
      if (origin === sessionGeneration()) showLocked();
      setCompleted(true);
    };
    window.addEventListener(COMPLETION_EVENT, acknowledge);
    return () => window.removeEventListener(COMPLETION_EVENT, acknowledge);
  }, [showLocked]);

  const loadStatus = useCallback(async () => {
    setBootInterrupted(false);
    const currentRequest = ++request.current;
    const currentEpoch = epoch.current;
    const currentTransition = transition.current;
    const current = () => currentRequest === request.current && currentEpoch === epoch.current && currentTransition === transition.current;
    try {
      const next = await api.status();
      if (!current()) {
        if (currentEpoch !== epoch.current) setStatus((existing) => existing ?? { ...next, unlocked: false, keyCount: 0, vaultName: null, needsUpgrade: false });
        return;
      }
      if (!next.unlocked) showLocked();
      setStatus(next);
      setError(null);
    } catch (cause) {
      if (current()) setError(errorText(cause));
    }
  }, [showLocked]);

  useEffect(() => {
    void loadStatus();
  }, [loadStatus]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen("vault-locked", showLocked).then((cleanup) => {
      if (disposed) cleanup(); else unlisten = cleanup;
    }).catch((cause) => setError(errorText(cause)));
    return () => { disposed = true; unlisten?.(); request.current += 1; epoch.current += 1; };
  }, [showLocked]);

  useEffect(() => {
    if (!status?.unlocked) return;
    const currentEpoch = epoch.current;
    let lastActivity = 0;
    const activity = () => {
      if (Date.now() - lastActivity < 5000) return;
      lastActivity = Date.now();
      void api.activity().catch(() => { if (currentEpoch === epoch.current) void loadStatus(); });
    };
    const poll = window.setInterval(() => {
      void loadStatus();
      void api.warnings().then((items) => {
        if (currentEpoch === epoch.current && items.length) setWarnings((previous) => [...previous, ...items]);
      }).catch(() => {});
    }, 3000);
    window.addEventListener("pointerdown", activity);
    window.addEventListener("keydown", activity);
    window.addEventListener("wheel", activity, { passive: true });
    return () => {
      window.clearInterval(poll);
      window.removeEventListener("pointerdown", activity);
      window.removeEventListener("keydown", activity);
      window.removeEventListener("wheel", activity);
    };
  }, [status?.unlocked, loadStatus]);

  const lock = useCallback(async () => {
    showLocked();
    const currentEpoch = epoch.current;
    const currentTransition = transition.current;
    try {
      await api.lock();
    } catch (cause) {
      if (currentEpoch !== epoch.current || currentTransition !== transition.current) return;
      setError(`Could not confirm the native lock. Close AKM before leaving this computer. ${errorText(cause)}`);
    }
  }, [showLocked]);

  async function reload() {
    const currentEpoch = epoch.current;
    const currentTransition = ++transition.current;
    request.current += 1;
    const current = () => currentEpoch === epoch.current && currentTransition === transition.current;
    try {
      const next = await api.reload();
      if (!current()) return;
      request.current += 1;
      setStatus(next);
      setRevision((current) => current + 1);
      setError(null);
    } catch (cause) { if (current()) setError(errorText(cause)); }
  }

  async function upgrade() {
    const currentEpoch = epoch.current;
    const currentTransition = ++transition.current;
    request.current += 1;
    const current = () => currentEpoch === epoch.current && currentTransition === transition.current;
    setUpgradeBusy(true);
    try {
      const next = await api.upgrade();
      if (!current()) return;
      request.current += 1;
      setStatus(next);
      setConfirmUpgrade(false);
      setRevision((current) => current + 1);
      setError(null);
    } catch (cause) { if (current()) setError(errorText(cause)); }
    finally { if (current()) setUpgradeBusy(false); }
  }

  const bootError = error ?? (bootInterrupted ? "The session changed while AKM was starting. Try again." : null);

  return (
    <>
      <NavBar
        view={view}
        onView={setView}
        unlocked={status?.unlocked ?? false}
        theme={theme}
        onToggleTheme={toggle}
        onLock={() => void lock()}
        onReload={() => void reload()}
      />

      {completed && <div className="wrap session-notices">
        <Notice>{COMPLETION_MESSAGE}</Notice>
        <button type="button" className="text-button" onClick={() => setCompleted(false)}>Dismiss completed-operation receipt</button>
      </div>}

      {status && error && <div className="wrap session-notices"><Notice tone="error">{error}</Notice></div>}
      {status?.unlocked && warnings.length > 0 && (
        <div className="wrap session-notices">
          {warnings.map((warning, index) => <Notice tone="error" key={index}>{warning}</Notice>)}
          <button type="button" className="text-button" onClick={() => setWarnings([])}>Dismiss warnings</button>
        </div>
      )}
      {status?.unlocked && status.needsUpgrade && (
        <section className="wrap session-notices" aria-label="Vault format upgrade">
          <Notice>This older vault is read-only in AKM until you upgrade it to KDBX 4.1. The password and encryption settings stay unchanged.</Notice>
          <button type="button" className="btn-secondary" onClick={() => setConfirmUpgrade(true)}>Review format upgrade</button>
        </section>
      )}

      {status === null ? (
        <main className="wrap view boot">
          {bootError ? (
            <Notice tone="error">{bootError}</Notice>
          ) : (
            <p className="label">Opening the vault…</p>
          )}
          {bootError && (
            <button
              type="button"
              className="text-button"
              onClick={() => void loadStatus()}
            >
              Try again
            </button>
          )}
        </main>
      ) : !status.unlocked ? (
        <Unlock
          key={sessionEpoch}
          status={status}
          onUnlocked={(next) => {
            if (renderedEpoch !== epoch.current) return;
            request.current += 1;
            transition.current += 1;
            setStatus(next);
            setError(null);
            setWarnings([]);
            setView("keys");
          }}
        />
      ) : view === "keys" ? (
        <KeysView key={revision} readOnly={status.needsUpgrade} />
      ) : view === "activity" ? (
        <Activity />
      ) : (
        <Settings key={revision} status={status} onLocked={() => void lock()} />
      )}

      {confirmUpgrade && status?.unlocked && (
        <ConfirmDialog title="Upgrade this vault to KDBX 4.1?" confirmLabel="Upgrade format" busy={upgradeBusy} destructive={false} onConfirm={() => void upgrade()} onCancel={() => setConfirmUpgrade(false)}>
          <p>AKM will retain the original encrypted file as a backup, write KDBX 4.1, and decrypt it again to verify the contents before replacement. The password and KDF settings do not change. Older KeePass clients may no longer open the upgraded file.</p>
        </ConfirmDialog>
      )}
    </>
  );
}
