import { useCallback, useEffect, useReducer, useRef } from "react";
import type { DependencyList } from "react";
import { errorText } from "./errors";
import { reads } from "./native";
import { IMPORT_APPLIED_EVENT, sessionGeneration } from "./sessionBoundary";

/**
 * Every read of native state that a view shows goes through `useNativeRead` (fault classes K and
 * L, docs/design/frontend-reads-and-rows.md). The reads live in native.ts, which a view must not
 * import, and each entry of `nativeReads` is a token that only this hook runs.
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

/** A read that a view names. It is a token: only `useNativeRead` can run it, so a view cannot call it. */
export interface ReadSource<T> {
  readonly __reads?: T;
}

const runners = new WeakMap<object, () => Promise<unknown>>();

/** Makes a token for a read. The function runs only inside `useNativeRead`. */
export function readSource<T>(run: () => Promise<T>): ReadSource<T> {
  const token = Object.freeze({});
  runners.set(token, run);
  return token as ReadSource<T>;
}

function runnerOf<T>(source: ReadSource<T>): () => Promise<T> {
  const run = runners.get(source);
  if (run === undefined) throw new Error("useNativeRead takes a token from readSource or nativeReads.");
  return run as () => Promise<T>;
}

/** The reads of native state that views show. A view names one of these, and only the hook runs it. */
export const nativeReads = {
  keys: readSource(() => reads.keys()),
  recycled: readSource(() => reads.recycled()),
  journal: readSource(() => reads.audit(500)),
  backup: readSource(() => reads.backup()),
  history: (id: string) => readSource(() => reads.history(id)),
};

interface Model<T> {
  state: ReadState<T>;
  busy: boolean;
}

/**
 * Reads when the view mounts and again when one of `deps` changes. A change of `deps` drops the
 * data, because it belonged to the old value.
 */
export function useNativeRead<T>(read: ReadSource<T>, deps: DependencyList = []): NativeRead<T> {
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
      reply = Promise.resolve(runnerOf(source.current)());
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

  useEffect(() => {
    window.addEventListener(IMPORT_APPLIED_EVENT, reload);
    return () => window.removeEventListener(IMPORT_APPLIED_EVENT, reload);
  }, [reload]);

  const apply = useCallback((change: (data: T) => T) => {
    if (!live.current) return;
    const { state, busy } = model.current;
    const shown: ReadState<T> = state.status === "ready" ? { status: "ready", data: change(state.data) } : state;
    if (shown.status === "ready" && !busy) commit({ state: shown, busy: false });
    else start(shown);
  }, [commit, start]);

  return { state: model.current.state, busy: model.current.busy, reload, apply };
}
