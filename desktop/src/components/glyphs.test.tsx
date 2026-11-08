import { cleanup, render } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import { rotationUsed } from "../format";
import type { KeyView } from "../types";
import { cutsOf, LockCylinder } from "./glyphs";

afterEach(cleanup);

test("a key's cut depends only on its id, and stays the same", () => {
  expect(cutsOf("entry-a")).toEqual(cutsOf("entry-a"));
  expect(cutsOf("entry-a")).not.toEqual(cutsOf("entry-b"));
  expect(cutsOf("entry-a").every((depth) => depth >= 0 && depth <= 4)).toBe(true);
});

const openLock = (element: React.ReactElement) => render(element).container.querySelector(".lock-open");

test("twelve characters open the lock only where the 12-character rule applies", () => {
  expect(openLock(<LockCylinder filled={11} rule />)).toBeNull();
  expect(openLock(<LockCylinder filled={12} rule />)).not.toBeNull();
  // On the unlock screen, the length of a typed password says nothing about whether it is right.
  expect(openLock(<LockCylinder filled={40} />)).toBeNull();
});

const key = (rotationDays: number | null, rotationDueAt: string | null) => ({ rotationDays, rotationDueAt }) as KeyView;
const now = Date.parse("2026-06-01T00:00:00Z");

test("the life rule reports the used part of the rotation interval", () => {
  expect(rotationUsed(key(100, "2026-08-10T00:00:00Z"), now)).toBeCloseTo(0.3);
});

test("the life rule stays between empty and full", () => {
  expect(rotationUsed(key(10, "2026-05-01T00:00:00Z"), now)).toBe(1);
  expect(rotationUsed(key(10, "2027-05-01T00:00:00Z"), now)).toBe(0);
});

test("the life rule draws nothing when AKM has no interval or no due date", () => {
  expect(rotationUsed(key(null, null), now)).toBeNull();
  expect(rotationUsed(key(30, null), now)).toBeNull();
  expect(rotationUsed(key(30, "not a date"), now)).toBeNull();
});
