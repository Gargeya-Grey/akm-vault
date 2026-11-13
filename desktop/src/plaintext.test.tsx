import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import { Plaintext, useSealed, type SealedState } from "./plaintext";
import { openFor, seal, stampFor } from "./sealed";
import { changeData, invalidateSession } from "./sessionBoundary";

// U2-R1-04: the one holder of plaintext. A value shows only under the scope of its request, while
// its session and the vault data hold. Once it is dropped it never shows again.

afterEach(cleanup);

const TEXT = "FAKE_TOKEN=fake-held-plaintext";
const shows = () => (document.body.textContent ?? "").includes(TEXT);

let state!: SealedState;
function Holder({ scope }: { scope: string | null }) {
  state = useSealed(scope);
  return <p data-testid="held"><Plaintext value={state.value} scope={scope} fallback="hidden" /></p>;
}

function held(scope = "scope-a") {
  const view = render(<Holder scope={scope} />);
  let lines: number | null = null;
  act(() => { lines = state.hold(seal(`${TEXT}\n\n`, stampFor(scope))); });
  expect(shows()).toBe(true);
  expect(lines).toBe(1);
  return view;
}

test("a value shows under the scope of its request, and a token from another scope does not show", () => {
  render(<Holder scope="scope-a" />);
  act(() => { state.hold(seal(TEXT, stampFor("scope-b"))); });
  expect(shows()).toBe(false);
  expect(state.shown).toBe(false);
  expect(screen.getByTestId("held").textContent).toBe("hidden");
});

test("a scope change drops the value, and it does not come back with the scope", () => {
  const view = held();
  view.rerender(<Holder scope="scope-b" />);
  expect(shows()).toBe(false);
  view.rerender(<Holder scope="scope-a" />);
  expect(shows()).toBe(false);
});

test("a view that names no scope drops the value, and it does not come back", () => {
  const view = held();
  view.rerender(<Holder scope={null} />);
  expect(shows()).toBe(false);
  view.rerender(<Holder scope="scope-a" />);
  expect(shows()).toBe(false);
});

test("a change of vault data drops the value, and the token stays dropped", () => {
  held();
  act(() => { changeData(); });
  expect(shows()).toBe(false);
  expect(state.value).toBeNull();
});

test("a new session drops the value", () => {
  held();
  act(() => { invalidateSession(); });
  expect(shows()).toBe(false);
});

test("a stale token answers no line count, and holding null answers none", () => {
  render(<Holder scope="scope-a" />);
  const token = seal(TEXT, stampFor("scope-a"));
  act(() => { changeData(); });
  let lines: number | null = 0;
  act(() => { lines = state.hold(token); });
  expect(lines).toBeNull();
  act(() => { lines = state.hold(null); });
  expect(lines).toBeNull();
  expect(shows()).toBe(false);
});

test("an unmount drops the value, so the same token never shows again", () => {
  const scope = "scope-a";
  const view = render(<Holder scope={scope} />);
  const token = seal(TEXT, stampFor(scope));
  act(() => { state.hold(token); });
  expect(shows()).toBe(true);
  view.unmount();
  render(<Holder scope={scope} />);
  act(() => { state.hold(token); });
  expect(shows()).toBe(false);
});

test("a token found stale is dropped from the store, so the same token never opens again", () => {
  const token = seal(TEXT, stampFor("scope-a"));
  expect(openFor(token, "scope-b")).toBeNull();
  expect(openFor(token, "scope-a")).toBeNull();
  const other = seal(TEXT, stampFor("scope-a"));
  expect(openFor(other, null)).toBeNull();
  expect(openFor(other, "scope-a")).toBeNull();
});

test("a token is frozen, has no prototype and no fields, and says nothing when printed", () => {
  const token = seal(TEXT, stampFor("scope-a"));
  expect(Object.isFrozen(token)).toBe(true);
  expect(Object.getPrototypeOf(token)).toBeNull();
  expect(Reflect.ownKeys(token)).toEqual([]);
  expect(JSON.stringify(token)).toBe("{}");
});
