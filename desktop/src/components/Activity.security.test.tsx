import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { Activity } from "./Activity";

const native = vi.hoisted(() => ({ audit: vi.fn() }));
vi.mock("../api", () => ({ api: native, errorText: (error: unknown) => String(error) }));
afterEach(cleanup);

for (const state of ["absent", "empty", "consistent", "corrupt"]) {
  test(`journal ${state} does not claim authenticated completeness or inactivity`, async () => {
    native.audit.mockResolvedValue({ path: "C:/fake/vault.kdbx.audit.jsonl", entries: [], state, brokenAt: null, problemLine: state === "corrupt" ? 1 : null, totalEntries: 0 });
    render(<Activity />);
    await screen.findByText("Completeness unknown");
    expect(screen.queryByText("Chain verified")).toBeNull();
    expect(screen.queryByText("Nothing has happened yet.")).toBeNull();
    expect(screen.getAllByText(/not authenticated/i).length).toBeGreaterThan(0);
    const label = { absent: "Journal unavailable", empty: "No records available", consistent: "Recorded chain is consistent", corrupt: "Journal data is damaged" }[state];
    expect(screen.getByText(label!)).toBeTruthy();
  });
}

test("requested and legacy events are not displayed as known completed actions", async () => {
  native.audit.mockResolvedValue({ path: "C:/fake/journal", state: "consistent", brokenAt: null, problemLine: null, totalEntries: 3, entries: [
    { seq: "1", ts: "2026-10-31T00:00:00Z", action: "copy-requested", keyTitle: "Fake", detail: null },
    { seq: "2", ts: "2026-10-31T00:00:00Z", action: "copy", keyTitle: "Fake", detail: null },
    { seq: "3", ts: "2026-10-31T00:00:00Z", action: "add-completed", keyTitle: "Fake", detail: null },
  ] });
  render(<Activity />);
  await screen.findByText("Copy requested");
  expect(screen.getByText("Legacy copy (outcome unknown)")).toBeTruthy();
  expect(screen.getByText("Added")).toBeTruthy();
  expect(screen.queryByText("Copied")).toBeNull();
});

test("a failed refresh removes the prior snapshot instead of leaving a stale success badge", async () => {
  native.audit.mockResolvedValueOnce({ path: "C:/fake/journal", entries: [], state: "consistent", brokenAt: null, problemLine: null, totalEntries: 0 }).mockRejectedValueOnce("Could not read the journal");
  render(<Activity />);
  await screen.findByText("Recorded chain is consistent");
  fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
  await screen.findByText("Could not read the journal");
  expect(screen.queryByText("Recorded chain is consistent")).toBeNull();
});

test("new launch/export request labels do not certify legacy start events", async () => {
  native.audit.mockResolvedValue({ path: "C:/fake/journal", state: "consistent", brokenAt: null, problemLine: null, totalEntries: 4, entries: ["run-requested", "export-requested", "run-start", "export-start"].map((action, index) => ({ seq: String(index + 1), ts: "2026-10-31T00:00:00Z", action, keyTitle: null, detail: null })) });
  render(<Activity />);
  await screen.findByText("Launch requested");
  expect(screen.getByText("Export requested")).toBeTruthy();
  expect(screen.getByText("Legacy run-start (outcome unknown)")).toBeTruthy();
  expect(screen.getByText("Legacy export-start (outcome unknown)")).toBeTruthy();
});

for (const brokenAt of ["7", null]) {
  test(`damage displays the physical line and ${brokenAt === null ? "no invented" : "the known"} offending sequence`, async () => {
    native.audit.mockResolvedValue({ path: "C:/fake/journal", entries: [], state: "corrupt", problemLine: 3, brokenAt, totalEntries: 0 });
    render(<Activity />);
    const notice = await screen.findByRole("alert");
    expect(notice.textContent).toContain("physical line 3");
    if (brokenAt === null) expect(notice.textContent).not.toContain("sequence");
    else expect(notice.textContent).toContain("sequence 7");
  });
}

test("valid unknown prototype action keys render as literal unknown outcomes", async () => {
  native.audit.mockResolvedValue({ path: "C:/fake/journal", state: "consistent", brokenAt: null, problemLine: null, totalEntries: 3, entries: ["__proto__", "constructor", "toString"].map((action,index) => ({ seq: String(index + 1), ts: "2026-10-31T00:00:00Z", action, keyTitle: null, detail: null })) });
  render(<Activity />);
  for (const action of ["__proto__", "constructor", "toString"]) expect(await screen.findByText(`Legacy ${action} (outcome unknown)`)).toBeTruthy();
});

for (const seq of ["9007199254740991", "9007199254740992", "9007199254740993", "18446744073709551615"]) {
  test(`known offending sequence ${seq} is rendered without rounding`, async () => {
    native.audit.mockResolvedValue({ path: "C:/fake/journal", state: "corrupt", brokenAt: seq, problemLine: 3, entries: [], totalEntries: 0 });
    render(<Activity />);
    expect((await screen.findByRole("alert")).textContent).toContain(`sequence ${seq}`);
  });
}
