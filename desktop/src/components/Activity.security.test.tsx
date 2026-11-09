import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { Activity } from "./Activity";

const native = vi.hoisted(() => ({ audit: vi.fn(), recoverJournal: vi.fn() }));
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

// FR-14. The control appears only for a damaged journal, asks native (which asks the user), and
// shows what was kept. The page cannot approve for the user: it sends one request with no argument.
const KEPT = { keptName: "j-0.jsonl.damaged-20261109T101500Z", sha256: "ab".repeat(32), bytes: 213 };
const startButton = () => screen.queryByRole("button", { name: "Start a new journal…" }) as HTMLButtonElement | null;

for (const state of ["absent", "empty", "consistent"]) {
  test(`a ${state} journal offers no recovery`, async () => {
    native.audit.mockResolvedValue(shown({ state }));
    render(<Activity />);
    await screen.findByText("Completeness unknown");
    expect(startButton()).toBeNull();
  });
}

test("a damaged journal says that disclosure is blocked, that the file is kept and that history cannot be proved complete", async () => {
  native.audit.mockResolvedValue(shown({ state: "corrupt", damage: "malformedJson", problemLine: 4 }));
  render(<Activity />);
  const button = await screen.findByRole("button", { name: "Start a new journal…" });
  expect(button).toBeTruthy();
  const text = document.body.textContent!;
  expect(text).toContain("stay blocked while the journal is damaged");
  expect(text).toContain("A window will ask you first");
  expect(text).toContain("kept unchanged");
  expect(text).toContain("cannot be proved complete");
  expect(native.recoverJournal).not.toHaveBeenCalled();
});

test("a recovery the user approved reloads the journal and names the kept file and its hash", async () => {
  const damaged = shown({ state: "corrupt", damage: "hashMismatch", problemLine: 4, totalEntries: 3 });
  const fresh = shown({ state: "consistent", totalEntries: 1, entries: [{ seq: "1", ts: "2026-11-09T10:15:00Z", action: "journal-recovered", keyTitle: null, keyId: null, detail: "A new journal started after damage." }] });
  native.audit.mockResolvedValueOnce(damaged).mockResolvedValueOnce(fresh);
  native.recoverJournal.mockResolvedValueOnce(KEPT);
  render(<Activity />);
  await screen.findByRole("button", { name: "Start a new journal…" });
  await act(async () => { fireEvent.click(startButton()!); });
  expect(native.recoverJournal).toHaveBeenCalledExactlyOnceWith();
  expect(native.audit).toHaveBeenCalledTimes(2);
  expect(await screen.findByText("Recorded chain is consistent")).toBeTruthy();
  expect(startButton()).toBeNull();
  expect(screen.getByText(KEPT.keptName)).toBeTruthy();
  expect(screen.getByText(KEPT.sha256)).toBeTruthy();
  expect(document.body.textContent).toContain("213 bytes");
  expect(screen.getByText("New journal started after damage")).toBeTruthy();
});

test("a recovery the user declined leaves the damaged journal on screen and still offers it", async () => {
  native.audit.mockResolvedValue(shown({ state: "corrupt", damage: "malformedJson", problemLine: 4 }));
  native.recoverJournal.mockResolvedValueOnce(null);
  render(<Activity />);
  await screen.findByRole("button", { name: "Start a new journal…" });
  await act(async () => { fireEvent.click(startButton()!); });
  expect(await screen.findByText(/Cancelled\. The journal is unchanged/)).toBeTruthy();
  expect(screen.getByText("Journal data is damaged")).toBeTruthy();
  expect(startButton()!.disabled).toBe(false);
  expect(screen.queryByText(KEPT.keptName)).toBeNull();
});

test("a recovery that failed shows the error, keeps the damaged journal and can be tried again", async () => {
  native.audit.mockResolvedValue(shown({ state: "corrupt", damage: "malformedJson", problemLine: 4 }));
  native.recoverJournal.mockRejectedValueOnce("The damaged journal could not be set aside. It is unchanged.").mockResolvedValueOnce(KEPT);
  render(<Activity />);
  await screen.findByRole("button", { name: "Start a new journal…" });
  await act(async () => { fireEvent.click(startButton()!); });
  expect(await screen.findByText(/could not be set aside/)).toBeTruthy();
  expect(screen.queryByText(KEPT.keptName)).toBeNull();
  expect(startButton()!.disabled).toBe(false);
  await act(async () => { fireEvent.click(startButton()!); });
  expect(native.recoverJournal).toHaveBeenCalledTimes(2);
});

test("the button is busy while the user answers the window, and one click sends one request", async () => {
  native.audit.mockResolvedValue(shown({ state: "corrupt", damage: "malformedJson", problemLine: 4 }));
  let answer!: (value: typeof KEPT | null) => void;
  native.recoverJournal.mockReturnValueOnce(new Promise((done) => { answer = done; }));
  render(<Activity />);
  await screen.findByRole("button", { name: "Start a new journal…" });
  await act(async () => { fireEvent.click(startButton()!); });
  const waiting = screen.getByRole("button", { name: "Waiting for your answer…" }) as HTMLButtonElement;
  expect(waiting.disabled).toBe(true);
  fireEvent.click(waiting);
  expect(native.recoverJournal).toHaveBeenCalledTimes(1);
  await act(async () => answer(null));
});
