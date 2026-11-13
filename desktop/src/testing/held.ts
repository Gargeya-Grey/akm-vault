import { readSource } from "../nativeRead";

/** A reply that a test finishes by hand, so that it can order two replies as it wants. */
export interface Held<T> {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (reason: unknown) => void;
}

export function held<T>(): Held<T> {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
}

/** A read whose every call returns a new held reply. `calls[n]` is the reply of call n. */
export function heldReads<T>() {
  const calls: Held<T>[] = [];
  const read = readSource(() => {
    const reply = held<T>();
    calls.push(reply);
    return reply.promise;
  });
  return { calls, read };
}
