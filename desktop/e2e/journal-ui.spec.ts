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
