import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { KeyForm } from "./KeyForm";
import type { KeyView } from "../types";

const mock = vi.hoisted(() => ({ update: vi.fn().mockResolvedValue({}) }));
vi.mock("../api", () => ({ api: mock, errorText: String }));
afterEach(cleanup);

test("a notes-only edit preserves an existing access-key username", async () => {
  const item: KeyView = {
    id: "native-entry-uuid", title: "AWS", username: "AKIAEXAMPLE", issuer: "AWS",
    project: "demo", environment: "dev", scopes: [], envVar: "AWS_SECRET_ACCESS_KEY",
    resolvedEnvVar: "AWS_SECRET_ACCESS_KEY", url: null, notes: null, tags: [],
    expiresAt: null, rotationDays: null, createdAt: null, updatedAt: null, rotationDueAt: null, secretChangedAt: null, secretChange: "unknown", historyCount: 0,
  };
  render(<KeyForm item={item} onSaved={() => {}} onClose={() => {}} />);
  fireEvent.change(screen.getByLabelText("Notes"), { target: { value: "A new note" } });
  fireEvent.click(screen.getByRole("button", { name: "Save changes" }));
  await waitFor(() => expect(mock.update).toHaveBeenCalled());
  expect(mock.update.mock.calls[0][1]).toMatchObject({ username: "AKIAEXAMPLE", secret: null, notes: "A new note" });
});

// The form shows some stored values in a lossy text form (a list joined with commas, an instant
// as a date). A field that the user did not touch goes back exactly as it was stored.
const stored: KeyView = {
  id: "native-entry-uuid", title: " Padded title ", username: null, issuer: " Issuer ",
  project: " demo", environment: "dev ", scopes: ["literal,scope", "read"], envVar: "PADDED_KEY",
  resolvedEnvVar: "PADDED_KEY", url: " https://example.invalid ", notes: "  two leading spaces", tags: ["a,b", "c"],
  expiresAt: "2027-03-15T00:00:45Z", rotationDays: 36500, createdAt: null, updatedAt: null, rotationDueAt: null,
  secretChangedAt: null, secretChange: "unknown", historyCount: 0,
};

test("an edit of one field sends every untouched field back exactly as it was stored", async () => {
  mock.update.mockClear();
  render(<KeyForm item={stored} onSaved={() => {}} onClose={() => {}} />);
  fireEvent.change(screen.getByLabelText("Issuer"), { target: { value: "New issuer" } });
  fireEvent.click(screen.getByRole("button", { name: "Save changes" }));
  await waitFor(() => expect(mock.update).toHaveBeenCalled());
  expect(mock.update.mock.calls[0][1]).toEqual({
    title: " Padded title ", secret: null, username: null, issuer: "New issuer", project: " demo", environment: "dev ",
    scopes: ["literal,scope", "read"], envVar: "PADDED_KEY", url: " https://example.invalid ", notes: "  two leading spaces",
    tags: ["a,b", "c"], expiresAt: "2027-03-15T00:00:45Z", rotationDays: 36500,
  });
});

test("a field that the user changed is read from the form's text", async () => {
  mock.update.mockClear();
  render(<KeyForm item={stored} onSaved={() => {}} onClose={() => {}} />);
  fireEvent.change(screen.getByLabelText("Scopes"), { target: { value: "read, write" } });
  fireEvent.change(screen.getByLabelText("Expires"), { target: { value: "2028-01-01" } });
  fireEvent.change(screen.getByLabelText(/Rotat/), { target: { value: "20000" } });
  fireEvent.click(screen.getByRole("button", { name: "Save changes" }));
  await waitFor(() => expect(mock.update).toHaveBeenCalled());
  // 20000 days is inside the core's bound (36500) and was outside the form's old bound (3650).
  expect(mock.update.mock.calls[0][1]).toMatchObject({
    title: " Padded title ", scopes: ["read", "write"], tags: ["a,b", "c"], expiresAt: "2028-01-01T23:59:59Z", rotationDays: 20000,
  });
});
