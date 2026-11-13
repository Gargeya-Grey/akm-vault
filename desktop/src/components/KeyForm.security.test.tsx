import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { KeyForm } from "./KeyForm";
import type { KeyView } from "../types";

const mock = vi.hoisted(() => ({ update: vi.fn().mockResolvedValue({}) }));
vi.mock("../api", () => ({ api: mock, errorText: String }));
afterEach(cleanup);

// FW-03: an edit names only the fields that the user changed. Native code keeps every field that
// the request does not name exactly as it was stored, so the form never sends a stored value back.
const stored: KeyView = {
  id: "native-entry-uuid", title: " Padded title ", username: null, issuer: " Issuer ",
  project: " demo", environment: "dev ", scopes: ["literal,scope", "read"], envVar: "PADDED_KEY",
  resolvedEnvVar: "PADDED_KEY", url: " https://example.invalid ", notes: "  two leading spaces", tags: ["a", "c"],
  expiresAt: "2027-03-15T00:00:45Z", rotationDays: 36500, createdAt: null, updatedAt: null, rotationDueAt: null,
  secretChangedAt: null, secretChange: "unknown", historyCount: 0,
};

async function save(item: KeyView, edit: () => void) {
  mock.update.mockClear();
  render(<KeyForm item={item} onSaved={() => {}} onClose={() => {}} />);
  edit();
  fireEvent.click(screen.getByRole("button", { name: "Save changes" }));
  await waitFor(() => expect(mock.update).toHaveBeenCalled());
  return mock.update.mock.calls[0][1];
}
const type = (label: string | RegExp, value: string) => fireEvent.change(screen.getByLabelText(label), { target: { value } });

test("a notes-only edit names only the notes", async () => {
  expect(await save(stored, () => type("Notes", "A new note"))).toEqual({ notes: "A new note" });
});

test("an edit of one field names only that field, as the user typed it", async () => {
  expect(await save(stored, () => type("Issuer", " New issuer "))).toEqual({ issuer: " New issuer " });
});

test("a field changed and changed back to the text that the form showed is not named", async () => {
  const draft = await save(stored, () => {
    type("Notes", "A new note");
    type("Notes", "  two leading spaces");
    type("Expires", "2028-01-01");
    type("Expires", "2027-03-15");
    type("Issuer", "New issuer");
  });
  expect(draft).toEqual({ issuer: "New issuer" });
});

test("an edit that changes no field names none", async () => {
  expect(await save(stored, () => {})).toEqual({});
});

// FV-12: the form shows the stored username and can change it.
const withUsername: KeyView = { ...stored, username: " AKIAFAKEEXAMPLE " };

test("the edit form shows the stored username, and an edit of it names only the username", async () => {
  const draft = await save(withUsername, () => {
    expect((screen.getByLabelText("Username") as HTMLInputElement).value).toBe(" AKIAFAKEEXAMPLE ");
    type("Username", " AKIANEWFAKE ");
  });
  expect(draft).toEqual({ username: " AKIANEWFAKE " });
});

test("a cleared field is named with an empty text or null, which clears it", async () => {
  const draft = await save(withUsername, () => {
    type("Username", "");
    type("Tags", "");
    type("Expires", "");
    type(/Rotat/, "");
  });
  expect(draft).toEqual({ username: "", tags: [], expiresAt: null, rotationDays: null });
});

test("the add form names every field", async () => {
  const add = vi.fn().mockResolvedValue({});
  Object.assign(mock, { add });
  render(<KeyForm onSaved={() => {}} onClose={() => {}} />);
  type("Title", "Fake AWS");
  type("Secret", "fake-secret-value");
  type("Username", "AKIAFAKEEXAMPLE");
  fireEvent.click(screen.getByRole("button", { name: "Add key" }));
  await waitFor(() => expect(add).toHaveBeenCalled());
  expect(add.mock.calls[0][0]).toEqual({
    title: "Fake AWS", secret: "fake-secret-value", username: "AKIAFAKEEXAMPLE", issuer: "", project: "", environment: "",
    envVar: "", url: "", notes: "", scopes: [], tags: [], expiresAt: null, rotationDays: null,
  });
});

test("a changed list, expiry and rotation are read from the form's text", async () => {
  const draft = await save(stored, () => {
    type("Scopes", "read\nwrite");
    type("Expires", "2028-01-01");
    type(/Rotat/, "20000");
  });
  // 20000 days is inside the core's bound (36500) and was outside the form's old bound (3650).
  expect(draft).toEqual({ scopes: ["read", "write"], expiresAt: "2028-01-01T23:59:59Z", rotationDays: 20000 });
});

// FW-11: the scopes field holds one scope on each line, and a comma is part of a scope, as each
// `--scope` option of `akm edit` is one scope.
const commaScope: KeyView = { ...stored, scopes: ["resource,subresource", "read"] };

test("the scopes field shows one stored scope on each line, and a new line adds one scope", async () => {
  const draft = await save(commaScope, () => {
    const field = screen.getByLabelText("Scopes") as HTMLTextAreaElement;
    expect(field.value).toBe("resource,subresource\nread");
    type("Scopes", `${field.value}\nwrite`);
  });
  expect(draft).toEqual({ scopes: ["resource,subresource", "read", "write"] });
});

test("an empty line or a line of spaces adds no scope", async () => {
  const draft = await save(commaScope, () => type("Scopes", "resource,subresource\n\n  \nread\r\nwrite\n"));
  expect(draft).toEqual({ scopes: ["resource,subresource", "read", "write"] });
});

test("a tag is split where the vault splits it, so the form shows what a reopen shows", async () => {
  expect(await save(stored, () => type("Tags", "one; two,three\tfour"))).toEqual({ tags: ["one", "two", "three", "four"] });
});
