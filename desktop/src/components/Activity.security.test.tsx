import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { Activity } from "./Activity";

const native = vi.hoisted(() => ({ audit: vi.fn() }));
vi.mock("../api", () => ({ api: native, errorText: (error: unknown) => String(error) }));
afterEach(cleanup);

const base = { path: "C:/fake/journal/j-0.jsonl", entries: [], state: "consistent", damage: null, identity: "confirmed", notices: [], brokenAt: null, problemLine: null, totalEntries: 0 };
const shown = (overrides: Record<string, unknown>) => ({ ...base, ...overrides });

for (const state of ["absent", "empty", "consistent", "corrupt"]) {
  test(`journal ${state} does not claim authenticated completeness or inactivity`, async () => {
    native.audit.mockResolvedValue(shown({ entries: [], state, problemLine: state === "corrupt" ? 1 : null }));
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
  native.audit.mockResolvedValue(shown({ totalEntries: 3, entries: [
    { seq: "1", ts: "2026-10-31T00:00:00Z", action: "copy-requested", keyTitle: "Fake", keyId: "k1", detail: null },
    { seq: "2", ts: "2026-10-31T00:00:00Z", action: "copy", keyTitle: "Fake", keyId: "k1", detail: null },
    { seq: "3", ts: "2026-10-31T00:00:00Z", action: "add-completed", keyTitle: "Fake", keyId: "k1", detail: null },
  ] }));
  render(<Activity />);
  await screen.findByText("Copy requested");
  expect(screen.getByText("Legacy copy (outcome unknown)")).toBeTruthy();
  expect(screen.getByText("Added")).toBeTruthy();
  expect(screen.queryByText("Copied")).toBeNull();
});

test("a failed refresh removes the prior snapshot instead of leaving a stale success badge", async () => {
  native.audit.mockResolvedValueOnce(shown({})).mockRejectedValueOnce("Could not read the journal");
  render(<Activity />);
  await screen.findByText("Recorded chain is consistent");
  fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
  await screen.findByText("Could not read the journal");
  expect(screen.queryByText("Recorded chain is consistent")).toBeNull();
});

test("new launch/export request labels do not certify legacy start events", async () => {
  native.audit.mockResolvedValue(shown({ totalEntries: 4, entries: ["run-requested", "export-requested", "run-start", "export-start"].map((action, index) => ({ seq: String(index + 1), ts: "2026-10-31T00:00:00Z", action, keyTitle: null, keyId: null, detail: null })) }));
  render(<Activity />);
  await screen.findByText("Launch requested");
  expect(screen.getByText("Export requested")).toBeTruthy();
  expect(screen.getByText("Legacy run-start (outcome unknown)")).toBeTruthy();
  expect(screen.getByText("Legacy export-start (outcome unknown)")).toBeTruthy();
});

for (const brokenAt of ["7", null]) {
  test(`damage displays the physical line and ${brokenAt === null ? "no invented" : "the known"} offending sequence`, async () => {
    native.audit.mockResolvedValue(shown({ state: "corrupt", problemLine: 3, brokenAt }));
    render(<Activity />);
    const notice = await screen.findByRole("alert");
    expect(notice.textContent).toContain("physical line 3");
    if (brokenAt === null) expect(notice.textContent).not.toContain("sequence");
    else expect(notice.textContent).toContain("declared sequence 7 (not verified)");
  });
}

test("valid unknown prototype action keys render as literal unknown outcomes", async () => {
  native.audit.mockResolvedValue(shown({ totalEntries: 3, entries: ["__proto__", "constructor", "toString"].map((action,index) => ({ seq: String(index + 1), ts: "2026-10-31T00:00:00Z", action, keyTitle: null, keyId: null, detail: null })) }));
  render(<Activity />);
  for (const action of ["__proto__", "constructor", "toString"]) expect(await screen.findByText(`Legacy ${action} (outcome unknown)`)).toBeTruthy();
});

for (const seq of ["9007199254740991", "9007199254740992", "9007199254740993", "18446744073709551615"]) {
  test(`known offending sequence ${seq} is rendered without rounding`, async () => {
    native.audit.mockResolvedValue(shown({ state: "corrupt", brokenAt: seq, problemLine: 3 }));
    render(<Activity />);
    expect((await screen.findByRole("alert")).textContent).toContain(`sequence ${seq}`);
  });
}

test("the lead text says the journal is private to this device and not beside the vault", async () => {
  native.audit.mockResolvedValue(shown({}));
  render(<Activity />);
  const lead = (await screen.findByText(/kept on this device/)).textContent!;
  expect(lead).toContain("not beside the vault");
  expect(lead).not.toContain("sync providers");
});

for (const [damage, text] of [["truncatedTail", "the last line has no line end"], ["malformedJson", "a line is not a valid record"], ["brokenLink", "a record does not follow the one before it"], ["hashMismatch", "a record does not match its hash"]]) {
  test(`damage kind ${damage} is named without naming a cause`, async () => {
    native.audit.mockResolvedValue(shown({ state: "corrupt", damage, problemLine: 2, brokenAt: "5" }));
    render(<Activity />);
    const alert = (await screen.findByRole("alert")).textContent!;
    expect(alert).toContain(`(${text})`);
    expect(alert).toContain("the cause of damage are unknown");
  });
}

test("every notice from the journal is shown with its own text", async () => {
  const notices = [
    { kind: "legacySidecar", message: "A plaintext log from an older version sits beside the vault." },
    { kind: "identityUnconfirmed", message: "This vault could not be matched to a journal." },
    { kind: "earlierJournal", message: "An earlier journal exists for this path." },
    { kind: "relatedJournal", message: "Another journal covers a file with the same content." },
  ];
  native.audit.mockResolvedValue(shown({ identity: "unconfirmed", notices }));
  render(<Activity />);
  for (const notice of notices) expect(await screen.findByText(notice.message)).toBeTruthy();
});

test("a key that is no longer in the vault shows its short id, and a vault event shows Vault", async () => {
  native.audit.mockResolvedValue(shown({ totalEntries: 2, entries: [
    { seq: "1", ts: "2026-10-31T00:00:00Z", action: "remove-completed", keyTitle: null, keyId: "0123456789abcdef-aaaa", detail: null },
    { seq: "2", ts: "2026-10-31T00:00:00Z", action: "upgrade-completed", keyTitle: null, keyId: null, detail: null },
  ] }));
  render(<Activity />);
  expect(await screen.findByText("Key 01234567")).toBeTruthy();
  expect(screen.queryByText(/89abcdef/)).toBeNull();
  expect(screen.getByText("Vault")).toBeTruthy();
});
