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
    expiresAt: null, rotationDays: null, createdAt: null, updatedAt: null, expired: false, rotationDueIn: null,
  };
  render(<KeyForm item={item} onSaved={() => {}} onClose={() => {}} />);
  fireEvent.change(screen.getByLabelText("Notes"), { target: { value: "A new note" } });
  fireEvent.click(screen.getByRole("button", { name: "Save changes" }));
  await waitFor(() => expect(mock.update).toHaveBeenCalled());
  expect(mock.update.mock.calls[0][1]).toMatchObject({ username: "AKIAEXAMPLE", secret: null, notes: "A new note" });
});
