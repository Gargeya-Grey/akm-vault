import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, test } from "vitest";
import { ConfirmDialog } from "./Notice";

afterEach(cleanup);

test("confirmation keeps keyboard focus inside its boundary and restores the opener", async () => {
  const user = userEvent.setup();
  const opener = document.createElement("button");
  opener.textContent = "Open confirmation";
  document.body.appendChild(opener);
  opener.focus();
  const view = render(<ConfirmDialog title="Confirm test" confirmLabel="Confirm action" onConfirm={() => {}} onCancel={() => {}}>Fake test data</ConfirmDialog>);
  expect(document.activeElement).toBe(screen.getByRole("button", { name: "Cancel" }));
  await user.tab();
  expect(document.activeElement).toBe(screen.getByRole("button", { name: "Confirm action" }));
  await user.tab();
  expect(document.activeElement).toBe(screen.getByRole("button", { name: "Cancel" }));
  view.unmount();
  expect(document.activeElement).toBe(opener);
  opener.remove();
});
