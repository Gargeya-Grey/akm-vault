import { act, renderHook } from "@testing-library/react";
import { expect, test } from "vitest";
import { readSource, useNativeRead } from "./nativeRead";
import { invalidateSession } from "./sessionBoundary";
import { heldReads } from "./testing/held";

// Fault classes K and L. The mechanism alone, with reads that the test finishes by hand.

function mount(deps: unknown[] = []) {
  const reads = heldReads<string>();
  let renders = 0;
  const hook = renderHook(({ deps: current }) => { renders += 1; return useNativeRead(reads.read, current); }, { initialProps: { deps } });
  return { ...reads, hook, current: () => hook.result.current, renders: () => renders };
}

const settle = () => act(async () => {});

test("the first read shows loading, then the data", async () => {
  const { calls, current } = mount();
  expect(current().state).toEqual({ status: "loading" });
  expect(current().busy).toBe(true);
  await act(async () => calls[0].resolve("first"));
  expect(current().state).toEqual({ status: "ready", data: "first" });
  expect(current().busy).toBe(false);
});

test("a first read that fails shows the failure and no data", async () => {
  const { calls, current } = mount();
  await act(async () => calls[0].reject({ message: "fake read failure" }));
  expect(current().state).toEqual({ status: "failed", error: "fake read failure" });
  expect(current().busy).toBe(false);
});

test("the newer of two reads wins: a later success of the older read changes nothing", async () => {
  const { calls, current } = mount();
  await act(async () => calls[0].resolve("first"));
  act(() => current().reload());
  act(() => current().reload());
  await act(async () => calls[2].resolve("newer"));
  expect(current().state).toEqual({ status: "ready", data: "newer" });
  await act(async () => calls[1].resolve("older"));
  expect(current().state).toEqual({ status: "ready", data: "newer" });
  expect(current().busy).toBe(false);
});

test("the newer of two reads wins: a later failure of the older read changes nothing", async () => {
  const { calls, current } = mount();
  act(() => current().reload());
  await act(async () => calls[1].resolve("newer"));
  await act(async () => calls[0].reject({ message: "older failure" }));
  expect(current().state).toEqual({ status: "ready", data: "newer" });
});

test("while the newer read is in flight, an older reply does not end the busy state", async () => {
  const { calls, current } = mount();
  act(() => current().reload());
  await act(async () => calls[0].resolve("older"));
  expect(current().state).toEqual({ status: "loading" });
  expect(current().busy).toBe(true);
  await act(async () => calls[1].resolve("newer"));
  expect(current().busy).toBe(false);
});

test("a retry keeps the failure on screen and busy, and a later success clears the error", async () => {
  const { calls, current } = mount();
  await act(async () => calls[0].reject({ message: "fake read failure" }));
  act(() => current().reload());
  expect(current().state).toEqual({ status: "failed", error: "fake read failure" });
  expect(current().busy).toBe(true);
  await act(async () => calls[1].resolve("back"));
  expect(current().state).toEqual({ status: "ready", data: "back" });
});

test("a refresh keeps the data on screen and busy, and a failed refresh drops the data", async () => {
  const { calls, current } = mount();
  await act(async () => calls[0].resolve("first"));
  act(() => current().reload());
  expect(current().state).toEqual({ status: "ready", data: "first" });
  expect(current().busy).toBe(true);
  await act(async () => calls[1].reject({ message: "refresh failed" }));
  expect(current().state).toEqual({ status: "failed", error: "refresh failed" });
});

test("a reply that arrives after unmount renders nothing", async () => {
  const { calls, hook, renders } = mount();
  hook.unmount();
  const before = renders();
  await act(async () => calls[0].resolve("late"));
  expect(renders()).toBe(before);
  expect(calls).toHaveLength(1);
});

test("a retry or a change reply after unmount starts no read", async () => {
  const { calls, current, hook } = mount();
  const { reload, apply } = current();
  hook.unmount();
  act(() => reload());
  act(() => apply((data) => data));
  expect(calls).toHaveLength(1);
});

test("a success that arrives after the session changed never shows", async () => {
  const { calls, current } = mount();
  invalidateSession();
  await act(async () => calls[0].resolve("old session"));
  expect(current().state).toEqual({ status: "loading" });
});

test("a failure that arrives after the session changed never shows", async () => {
  const { calls, current } = mount();
  await act(async () => calls[0].resolve("first"));
  act(() => current().reload());
  invalidateSession();
  await act(async () => calls[1].reject({ message: "old session failure" }));
  expect(current().state).toEqual({ status: "ready", data: "first" });
});

test("a change of the read's inputs drops the old data and an older reply changes nothing", async () => {
  const { calls, current, hook } = mount([1]);
  await act(async () => calls[0].resolve("for 1"));
  act(() => current().reload());
  hook.rerender({ deps: [2] });
  expect(current().state).toEqual({ status: "loading" });
  await act(async () => calls[1].resolve("late for 1"));
  expect(current().state).toEqual({ status: "loading" });
  await act(async () => calls[2].resolve("for 2"));
  expect(current().state).toEqual({ status: "ready", data: "for 2" });
});

test("a change reply with no read in flight patches the data and reads nothing", async () => {
  const { calls, current } = mount();
  await act(async () => calls[0].resolve("a"));
  act(() => current().apply((data) => `${data}+change`));
  expect(current().state).toEqual({ status: "ready", data: "a+change" });
  expect(current().busy).toBe(false);
  expect(calls).toHaveLength(1);
});

test("a change reply during a refresh patches the data, and the refresh that started earlier can no longer replace it", async () => {
  const { calls, current } = mount();
  await act(async () => calls[0].resolve("a"));
  act(() => current().reload());
  act(() => current().apply((data) => `${data}+change`));
  expect(current().state).toEqual({ status: "ready", data: "a+change" });
  expect(calls).toHaveLength(3);
  await act(async () => calls[1].resolve("read before the change"));
  expect(current().state).toEqual({ status: "ready", data: "a+change" });
  await act(async () => calls[2].resolve("read after the change"));
  expect(current().state).toEqual({ status: "ready", data: "read after the change" });
});

test("a change reply while the first read is in flight starts a new read that replaces it", async () => {
  const { calls, current } = mount();
  act(() => current().apply((data) => `${data}+change`));
  expect(current().state).toEqual({ status: "loading" });
  await act(async () => calls[0].resolve("read before the change"));
  expect(current().state).toEqual({ status: "loading" });
  await act(async () => calls[1].resolve("read after the change"));
  expect(current().state).toEqual({ status: "ready", data: "read after the change" });
});

test("a change reply after a failed read retries the read", async () => {
  const { calls, current } = mount();
  await act(async () => calls[0].reject({ message: "fake read failure" }));
  act(() => current().apply((data) => data));
  expect(current().busy).toBe(true);
  await settle();
  await act(async () => calls[1].resolve("back"));
  expect(current().state).toEqual({ status: "ready", data: "back" });
});

test("a read that throws before it returns a promise is a failure, not a crash", async () => {
  const hook = renderHook(() => useNativeRead<string>(readSource(() => { throw { message: "thrown at once" }; })));
  await settle();
  expect(hook.result.current.state).toEqual({ status: "failed", error: "thrown at once" });
});
