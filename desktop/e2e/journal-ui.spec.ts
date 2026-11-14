import { expect, test } from "@playwright/test";
import { installNativeBoundaryMock } from "./native-boundary";

for (const width of [390, 1440]) {
  for (const theme of ["light", "dark"]) {
    test(`${width}px ${theme}: journal evidence distinguishes request, completion and legacy uncertainty`, async ({ page }) => {
      await page.setViewportSize({ width, height: 1000 });
      await installNativeBoundaryMock(page, theme);
      const errors: string[] = [];
      page.on("pageerror", (error) => errors.push(error.message));
      await page.goto("/");
      await page.getByRole("button", { name: "Activity", exact: true }).click();
      await expect(page.getByText("Recorded chain is consistent", { exact: true })).toBeVisible();
      await expect(page.getByText("Completeness unknown", { exact: true })).toBeVisible();
      await expect(page.getByText("Copy requested", { exact: true })).toBeVisible();
      await expect(page.getByText("Legacy copy (outcome unknown)", { exact: true })).toBeVisible();
      await expect(page.getByText("Added", { exact: true })).toBeVisible();
      await expect(page.getByText("Chain verified", { exact: true })).toHaveCount(0);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
      expect(errors).toEqual([]);
    });
  }
}

for (const width of [900, 1040, 1280]) {
  test(`${width}px: an activity record keeps its detail at every window width the app allows`, async ({ page }) => {
    await page.setViewportSize({ width, height: 800 });
    await installNativeBoundaryMock(page, "light");
    await page.goto("/");
    await page.getByRole("button", { name: "Activity", exact: true }).click();
    await expect(page.getByText("fake detail: 2 keys added, 1 updated", { exact: true })).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
  });
}

for (const theme of ["light", "dark"]) {
  test(`${theme}: native-retained notices survive a fresh document and only explicit dismissal clears them`, async ({ page }) => {
    await installNativeBoundaryMock(page, theme, [{ id: "reload-notice", originSession: "A", kind: "journal" }], [{ id: "reload-app-warning", message: "Fake Windows monitor warning" }]);
    await page.goto("/");
    const notices = page.getByRole("region", { name: "Saved-operation notices" });
    await expect(notices).toContainText("activity entry could not be confirmed");
    await expect(page.getByText("Fake Windows monitor warning", { exact: true })).toBeVisible();
    await page.reload();
    await expect(notices).toContainText("Do not repeat");
    await expect(page.getByText("Fake Windows monitor warning", { exact: true })).toBeVisible();
    await expect(page.getByRole("button", { name: "Add key", exact: true })).toBeVisible();
    await page.getByRole("button", { name: "Dismiss saved-operation notices" }).click();
    await expect(notices).toHaveCount(0);
    await expect(page.getByText("Fake Windows monitor warning", { exact: true })).toBeVisible();
    await page.getByRole("button", { name: "Dismiss warnings", exact: true }).click();
    await expect(page.getByText("Fake Windows monitor warning", { exact: true })).toHaveCount(0);
    await page.reload();
    await expect(page.getByRole("button", { name: "Add key", exact: true })).toBeVisible();
    await expect(notices).toHaveCount(0);
    await expect(page.getByText("Fake Windows monitor warning", { exact: true })).toHaveCount(0);
  });
}

// FR-14. A damaged journal can be replaced from the Activity view. Native asks the user in its own
// window; the page only asks. Class H: the control and its facts stay visible at the minimum width.
for (const width of [390, 900, 1440]) {
  for (const theme of ["light", "dark"]) {
    test(`${width}px ${theme}: a damaged journal offers a new one, and the answer is shown`, async ({ page }) => {
      await page.setViewportSize({ width, height: 1000 });
      await installNativeBoundaryMock(page, theme, undefined, undefined, { damagedJournal: true });
      const errors: string[] = [];
      page.on("pageerror", (error) => errors.push(error.message));
      await page.goto("/");
      await page.getByRole("button", { name: "Activity", exact: true }).click();
      await expect(page.getByText("Journal data is damaged", { exact: true })).toBeVisible();
      await expect(page.getByText(/stay blocked while the journal is damaged/)).toBeVisible();
      await expect(page.getByText(/cannot be proved complete/).first()).toBeVisible();
      const start = page.getByRole("button", { name: "Start a new journal…", exact: true });
      await expect(start).toBeVisible();
      await start.click();
      await expect(page.getByText("Recorded chain is consistent", { exact: true })).toBeVisible();
      await expect(page.getByText("j-00000000000000000000000000000000.jsonl.damaged-20261109T101500Z", { exact: true })).toBeVisible();
      await expect(page.getByText("ab".repeat(32), { exact: true })).toBeVisible();
      await expect(page.getByText("New journal started after damage", { exact: true })).toBeVisible();
      await expect(start).toHaveCount(0);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
      expect(errors).toEqual([]);
    });
  }
}

test("a declined recovery leaves the damaged journal on screen", async ({ page }) => {
  await installNativeBoundaryMock(page, "light", undefined, undefined, { damagedJournal: true, recovery: "decline" });
  await page.goto("/");
  await page.getByRole("button", { name: "Activity", exact: true }).click();
  await page.getByRole("button", { name: "Start a new journal…", exact: true }).click();
  await expect(page.getByText(/Cancelled\. The journal is unchanged/)).toBeVisible();
  await expect(page.getByText("Journal data is damaged", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Start a new journal…", exact: true })).toBeEnabled();
});

test("a journal that is not damaged shows no recovery control", async ({ page }) => {
  await installNativeBoundaryMock(page, "light");
  await page.goto("/");
  await page.getByRole("button", { name: "Activity", exact: true }).click();
  await expect(page.getByText("Recorded chain is consistent", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Start a new journal…" })).toHaveCount(0);
});
