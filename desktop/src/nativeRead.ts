import { useCallback, useEffect, useReducer, useRef } from "react";
import type { DependencyList } from "react";
import { api, errorText } from "./api";
import { sessionGeneration } from "./sessionBoundary";

/**
 * Every read of native state that a view shows goes through `useNativeRead` (fault classes K and
 * L, docs/design/frontend-reads-and-rows.md). A view never calls a read of `api` itself:
 * `readGuard.test.ts` fails when one does.
 *
 * - Only the newest request can set the state. A late success or a late failure of an older
 *   request changes nothing.
 * - A reply that arrives after the view unmounted, or after the session changed, changes nothing.
 * - "ready" is the only state that holds data, so a view can say "empty" only from a read that
 *   succeeded. A failed read drops the data that an earlier read gave.
 */
export type ReadState<T> =
  | { status: "loading" }
  | { status: "ready"; data: T }
  | { status: "failed"; error: string };

export interface NativeRead<T> {
  readonly state: ReadState<T>;
  /** A read is in flight. In "ready" it is a refresh; in "failed" it is a retry. */
  readonly busy: boolean;
  /** Starts a new read. The current state stays on screen, marked busy, until the read ends. */
  reload: () => void;
  /**
   * Takes the reply of a change that the view made (a save, a removal). A read that was in
   * flight started before that reply, so it can no longer set the state: a new read replaces it.
   */
  apply: (change: (data: T) => T) => void;
}

/** The reads of native state that views show. A view names one of these and never `api` itself. */
export const nativeReads = {
  keys: () => api.keys(),
  recycled: () => api.recycled(),
  journal: () => api.audit(500),
  backup: () => api.backup(),
  reminders: () => api.remindersStatus(),
  history: (id: string) => () => api.history(id),
};

interface Model<T> {
  state: ReadState<T>;
  busy: boolean;
}

/**
 * Reads when the view mounts and again when one of `deps` changes. A change of `deps` drops the
 * data, because it belonged to the old value.
 */
export function useNativeRead<T>(read: () => Promise<T>, deps: DependencyList = []): NativeRead<T> {
  const model = useRef<Model<T>>({ state: { status: "loading" }, busy: true });
  const [, render] = useReducer((count: number) => count + 1, 0);
  // The newest function, so that a retry never calls one from an earlier render.
  const source = useRef(read);
  source.current = read;
  const newest = useRef(0);
  const live = useRef(false);

  const commit = useCallback((next: Model<T>) => {
    model.current = next;
    render();
  }, []);

  const start = useCallback((shown: ReadState<T>) => {
    const request = ++newest.current;
    const generation = sessionGeneration();
    commit({ state: shown, busy: true });
    const owns = () => live.current && request === newest.current && generation === sessionGeneration();
    let reply: Promise<T>;
    try {
      reply = Promise.resolve(source.current());
    } catch (cause) {
      reply = Promise.reject(cause);
    }
    reply.then(
      (data) => { if (owns()) commit({ state: { status: "ready", data }, busy: false }); },
      (cause: unknown) => { if (owns()) commit({ state: { status: "failed", error: errorText(cause) }, busy: false }); },
    );
  }, [commit]);

  useEffect(() => {
    live.current = true;
    start({ status: "loading" });
    return () => {
      live.current = false;
    };
  }, deps);

  // A change reply or a retry that ends after the view closed starts no read.
  const reload = useCallback(() => {
    if (live.current) start(model.current.state);
  }, [start]);

  const apply = useCallback((change: (data: T) => T) => {
    if (!live.current) return;
    const { state, busy } = model.current;
    const shown: ReadState<T> = state.status === "ready" ? { status: "ready", data: change(state.data) } : state;
    if (shown.status === "ready" && !busy) commit({ state: shown, busy: false });
    else start(shown);
  }, [commit, start]);

  return { state: model.current.state, busy: model.current.busy, reload, apply };
}
