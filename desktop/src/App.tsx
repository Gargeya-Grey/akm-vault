import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, errorText } from "./api";
import { Activity } from "./components/Activity";
import { KeysView } from "./components/KeysView";
import { NavBar } from "./components/NavBar";
import { RecycleBin } from "./components/RecycleBin";
import { ConfirmDialog, Notice } from "./components/Notice";
import { Settings } from "./components/Settings";
import { Unlock } from "./components/Unlock";
import { useTheme } from "./theme";
import type { OperationNotice, Status, View, WarningReport } from "./types";
import { acknowledgeCompletions, COMPLETION_EVENT, COMPLETION_MESSAGE, invalidateSession, retainedCompletions, sessionGeneration, SESSION_LOSS_EVENT, WARNING_EVENT } from "./sessionBoundary";
import type { OperationOutcome } from "./sessionBoundary";
import { retainedApplicationWarnings, retainedOperationNotices } from "./operationNotices";
import { NATIVE_CALL_DEADLINE_MS, REGISTRATION_DEADLINE_MS, STATUS_HEALTH_LIMIT_MS, withDeadline } from "./deadline";
import { nativeOpening, NATIVE_INVALIDATION_EVENT, NATIVE_OPEN_EVENT, NATIVE_PROTOCOL_ERROR, receiveNativeLock, receiveNativeViewExpiry } from "./nativeSession";

const HEALTH_MESSAGE = "AKM stopped receiving the session status, so it locked the vault.";
const WATCH_FAILED_MESSAGE = "AKM could not watch the session. Restart AKM.";
const LOCK_UNCONFIRMED_MESSAGE = "Could not confirm the native lock. Close AKM before leaving this computer.";

const OPERATION_MESSAGES: Record<OperationNotice["kind"], string> = {
  journal: "A vault change was saved, but its activity entry could not be confirmed. Do not repeat the saved operation. Check the relevant vault and its journal.",
  exportJournal: "An environment file was saved, but its activity entry could not be confirmed. Do not repeat the export. Check its saved destination and journal.",
  backupRefresh: "A vault change was saved, but its convenience backup could not be refreshed. Do not repeat the saved operation. Inspect its dated backups before relying on recovery.",
  backupRetention: "A vault change was saved, but retention of its pre-change encrypted backup could not be confirmed. Do not repeat the saved operation. Inspect its backups before relying on recovery.",
  backupPrune: "A vault change was saved, but old backups could not be pruned. Do not repeat the saved operation. Check its retained backups.",
  preference: "A vault was opened, but its path preference was not saved. Check the vault path the next time you open AKM.",
  importApplied: "An import was applied to the vault, but this window did not receive its result. Do not repeat it. Check the keys in the relevant vault.",
  importSessionEnded: "An import was saved, but its session ended before the result arrived. Do not repeat it. Open the vault and check the keys.",
  importFailed: "An import reported an error. This does not confirm that nothing was saved. Check the vault before you try again.",
  importDeclined: "An import was declined in the confirmation window. Nothing was saved.",
};

function outcomeMessage({ operation, outcome }: OperationOutcome) {
  const name = operation === "import" ? "Import" : operation === "export" ? "File export" : "Operation";
  switch (outcome) {
    case "pending": return `${name} is awaiting an outcome. Do not repeat it while it is pending.`;
    case "completed": return `${name} completed. Inspect the relevant vault or saved file before repeating it.`;
    case "cancelled": return operation === "import" ? "Import was declined in the confirmation window. No keys were changed by this request." : "File export was cancelled. No file was saved by this request.";
    case "reportedError": return `${name} reported an error. This does not confirm that nothing was saved. Inspect the relevant vault or export before retrying.`;
    case "unconfirmed": return `${name} outcome could not be confirmed after the session changed. Inspect the relevant vault or export before retrying.`;
    case "completedWhileLocked": return COMPLETION_MESSAGE;
  }
}

export function App() {
  const { theme, toggle } = useTheme();
  const [status, setStatus] = useState<Status | null>(null);
  const [view, setView] = useState<View>("keys");
  const [error, setError] = useState<string | null>(null);
  const [warnings, setWarnings] = useState(retainedApplicationWarnings);
  const [warningDismissError, setWarningDismissError] = useState(false);
  const [warningDismissBusy, setWarningDismissBusy] = useState(false);
  const [operationNotices, setOperationNotices] = useState(retainedOperationNotices);
  const [dismissBusy, setDismissBusy] = useState(false);
  const [dismissError, setDismissError] = useState(false);
  const [confirmUpgrade, setConfirmUpgrade] = useState(false);
  const [upgradeBusy, setUpgradeBusy] = useState(false);
  const [revision, setRevision] = useState(0);
  const [completions, setCompletions] = useState(retainedCompletions);
  const [bootInterrupted, setBootInterrupted] = useState(false);
  const [sessionEpoch, setSessionEpoch] = useState(0);
  const [watch, setWatch] = useState<"registering" | "ready" | "failed">("registering");
  const [lockUnconfirmed, setLockUnconfirmed] = useState(false);
  const watchReady = useRef(false);
  const lastAccepted = useRef(0);
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
    setStatus((current) => current ? { ...current, sessionId: null, unlocked: false, keyCount: 0, vaultName: null } : current);
    setView("keys");
    setConfirmUpgrade(false);
    setUpgradeBusy(false);
  }, []);

  const lock = useCallback(async (reason?: string) => {
    showLocked();
    if (reason) setError(reason);
    const currentEpoch = epoch.current;
    const currentTransition = transition.current;
    try {
      await withDeadline(api.lock(), NATIVE_CALL_DEADLINE_MS);
    } catch (cause) {
      if (currentEpoch !== epoch.current || currentTransition !== transition.current) return;
      setError(`Could not confirm the native lock. Close AKM before leaving this computer. ${errorText(cause)}`);
    }
  }, [showLocked]);

  const failClosed = useCallback(async () => {
    showLocked();
    watchReady.current = false;
    setWatch("failed");
    setError(null);
    setLockUnconfirmed(false);
    const currentEpoch = epoch.current;
    try {
      await withDeadline(api.lockCurrent(), NATIVE_CALL_DEADLINE_MS);
    } catch {
      if (currentEpoch === epoch.current) setLockUnconfirmed(true);
    }
  }, [showLocked]);

  useEffect(() => {
    const opening = () => {
      request.current += 1;
      transition.current += 1;
      setStatus((current) => current ? { ...current, sessionId: null, unlocked: false, keyCount: 0, vaultName: null } : current);
      setView("keys");
      setConfirmUpgrade(false);
      setUpgradeBusy(false);
      setError(null);
    };
    const invalidated = (event: Event) => {
      showLocked();
      const message = (event as CustomEvent<{ message?: string }>).detail?.message;
      if (message) setError(message);
    };
    window.addEventListener(NATIVE_OPEN_EVENT, opening);
    window.addEventListener(NATIVE_INVALIDATION_EVENT, invalidated);
    return () => {
      window.removeEventListener(NATIVE_OPEN_EVENT, opening);
      window.removeEventListener(NATIVE_INVALIDATION_EVENT, invalidated);
    };
  }, [showLocked]);

  useEffect(() => {
    const update = () => setCompletions(retainedCompletions());
    const sessionLost = (event: Event) => {
      const origin = (event as CustomEvent<{ generation: number }>).detail?.generation;
      if (origin === sessionGeneration()) showLocked();
    };
    window.addEventListener(COMPLETION_EVENT, update);
    window.addEventListener(SESSION_LOSS_EVENT, sessionLost);
    setCompletions(retainedCompletions()); // Replay is presentation-only, not a new lock signal.
    return () => {
      window.removeEventListener(COMPLETION_EVENT, update);
      window.removeEventListener(SESSION_LOSS_EVENT, sessionLost);
    };
  }, [showLocked]);

  useEffect(() => {
    const receive = (event: Event) => {
      const report = (event as CustomEvent<WarningReport>).detail;
      setWarnings(report.application);
      setOperationNotices(report.operations);
    };
    window.addEventListener(WARNING_EVENT, receive);
    setWarnings(retainedApplicationWarnings());
    setOperationNotices(retainedOperationNotices());
    void api.warnings().catch(() => {});
    const poll = window.setInterval(() => { void api.warnings().catch(() => {}); }, 3000);
    return () => { window.removeEventListener(WARNING_EVENT, receive); window.clearInterval(poll); };
  }, []);

  async function dismissNotices() {
    setDismissBusy(true);
    setDismissError(false);
    try { await api.dismissWarnings(operationNotices.map(({ id }) => id)); }
    catch { setDismissError(true); }
    finally { setDismissBusy(false); }
  }

  async function dismissApplicationWarnings() {
    setWarningDismissBusy(true);
    setWarningDismissError(false);
    try { await api.dismissWarnings(warnings.map(({ id }) => id)); }
    catch { setWarningDismissError(true); }
    finally { setWarningDismissBusy(false); }
  }

  const loadStatus = useCallback(async () => {
    if (!watchReady.current || nativeOpening()) return;
    setBootInterrupted(false);
    const currentRequest = ++request.current;
    const currentEpoch = epoch.current;
    const currentTransition = transition.current;
    const current = () => !nativeOpening() && currentRequest === request.current && currentEpoch === epoch.current && currentTransition === transition.current;
    try {
      const next = await api.status();
      if (currentEpoch === epoch.current && currentTransition === transition.current && next.unlocked) lastAccepted.current = Date.now();
      if (!current()) return;
      if (!next.unlocked) showLocked();
      setStatus(next);
      setError(null);
    } catch (cause) {
      if (current()) setError(errorText(cause));
    }
  }, [showLocked]);

  useEffect(() => {
    if (!status?.unlocked) return;
    lastAccepted.current = Date.now();
    const currentEpoch = epoch.current;
    let lastActivity = 0;
    const activity = () => {
      if (Date.now() - lastActivity < 5000) return;
      lastActivity = Date.now();
      void api.activity().catch(() => { if (currentEpoch === epoch.current) void loadStatus(); });
    };
    const poll = window.setInterval(() => {
      void loadStatus();
    }, 3000);
    const health = window.setInterval(() => {
      if (Date.now() - lastAccepted.current >= STATUS_HEALTH_LIMIT_MS) void lock(HEALTH_MESSAGE);
    }, 1000);
    window.addEventListener("pointerdown", activity);
    window.addEventListener("keydown", activity);
    window.addEventListener("wheel", activity, { passive: true });
    return () => {
      window.clearInterval(poll);
      window.clearInterval(health);
      window.removeEventListener("pointerdown", activity);
      window.removeEventListener("keydown", activity);
      window.removeEventListener("wheel", activity);
    };
  }, [status?.unlocked, loadStatus, lock]);

  useEffect(() => {
    const root = { disposed: false };
    const cleanups: (() => void)[] = [];
    watchReady.current = false;
    setWatch("registering");
    const register = (name: string, receive: (payload: unknown) => boolean) =>
      listen<unknown>(name, (event) => {
        if (!root.disposed && !receive(event?.payload)) void lock(NATIVE_PROTOCOL_ERROR);
      }).then((cleanup) => {
        if (root.disposed) cleanup(); else cleanups.push(cleanup);
      });
    void withDeadline(
      Promise.all([register("vault-locked", receiveNativeLock), register("vault-view-expired", receiveNativeViewExpiry)]),
      REGISTRATION_DEADLINE_MS,
    ).then(
      () => {
        if (root.disposed) return;
        watchReady.current = true;
        setWatch("ready");
        void loadStatus();
      },
      () => { if (!root.disposed) void failClosed(); },
    );
    return () => {
      root.disposed = true;
      watchReady.current = false;
      for (const cleanup of cleanups) cleanup();
      request.current += 1;
      epoch.current += 1;
      invalidateSession();
    };
  }, [lock, loadStatus, failClosed]);

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

      {completions.length > 0 && <section className="wrap session-notices" aria-label="Operation outcomes">
        {completions.map((record) => <Notice key={record.id} tone={record.outcome === "reportedError" || record.outcome === "unconfirmed" ? "error" : "info"}>{outcomeMessage(record)}</Notice>)}
        <button type="button" className="text-button" disabled={completions.every(({ outcome }) => outcome === "pending")} onClick={() => acknowledgeCompletions(completions.filter(({ outcome }) => outcome !== "pending").map(({ id }) => id))}>Dismiss operation outcomes</button>
      </section>}

      {status && error && <div className="wrap session-notices"><Notice tone="error">{error}</Notice></div>}
      {operationNotices.length > 0 && (
        <section className="wrap session-notices" aria-label="Saved-operation notices">
          {operationNotices.map((notice) => <Notice key={notice.id}>{OPERATION_MESSAGES[notice.kind]}</Notice>)}
          {dismissError && <Notice tone="error">Could not dismiss these notices. They remain available; try dismissing again.</Notice>}
          <button type="button" className="text-button" disabled={dismissBusy} onClick={() => void dismissNotices()}>Dismiss saved-operation notices</button>
        </section>
      )}
      {warnings.length > 0 && (
        <div className="wrap session-notices">
          {warnings.map((warning) => <Notice tone="error" key={warning.id}>{warning.message}</Notice>)}
          {warningDismissError && <Notice tone="error">Could not dismiss these warnings. They remain available; try dismissing again.</Notice>}
          <button type="button" className="text-button" disabled={warningDismissBusy} onClick={() => void dismissApplicationWarnings()}>Dismiss warnings</button>
        </div>
      )}
      {status?.unlocked && status.needsUpgrade && (
        <section className="wrap session-notices" aria-label="Vault format upgrade">
          <Notice>This older vault is read-only in AKM until you upgrade it to KDBX 4.1. The password and encryption settings stay unchanged.</Notice>
          <button type="button" className="btn-secondary" onClick={() => setConfirmUpgrade(true)}>Review format upgrade</button>
        </section>
      )}

      {watch === "failed" ? (
        <main className="wrap view boot">
          <Notice tone="error">{WATCH_FAILED_MESSAGE}</Notice>
          {lockUnconfirmed && <Notice tone="error">{LOCK_UNCONFIRMED_MESSAGE}</Notice>}
        </main>
      ) : status === null ? (
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
            setView("keys");
          }}
        />
      ) : view === "keys" ? (
        <KeysView key={revision} readOnly={status.needsUpgrade} />
      ) : view === "bin" ? (
        <RecycleBin key={revision} readOnly={status.needsUpgrade} />
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
