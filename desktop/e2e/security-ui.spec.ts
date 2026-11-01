import { expect, test } from "@playwright/test";
import { installNativeBoundaryMock } from "./native-boundary";

declare global {
  interface Window {
    __akmTest: { calls: { command: string; args: Record<string, unknown> }[]; emitLock: () => void };
  }
}

for (const width of [390, 1440]) {
  for (const theme of ["light", "dark"]) {
    test(`${width}px ${theme}: export gate, import review, theme switch and native lock`, async ({ page }) => {
      await page.setViewportSize({ width, height: 1000 });
      await installNativeBoundaryMock(page, theme);
      const errors: string[] = [];
      page.on("pageerror", (error) => errors.push(error.message));
      await page.goto("/");
      await expect(page.getByRole("button", { name: "Add key", exact: true })).toBeVisible();
      const nextTheme = theme === "light" ? "dark" : "light";
      await page.getByRole("button", { name: `Switch to ${nextTheme} mode` }).click();
      await expect(page.locator("html")).toHaveAttribute("data-theme", nextTheme);
      await page.getByRole("button", { name: `Switch to ${theme} mode` }).click();
      await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
      await page.getByRole("button", { name: "Settings", exact: true }).click();
      for (const name of ["Save .env…", "Preview", "Copy to clipboard"]) {
        await expect(page.getByRole("button", { name, exact: true })).toBeDisabled();
      }
      await page.getByLabel("Project", { exact: true }).selectOption("demo");
      await page.getByLabel("Environment", { exact: true }).selectOption("dev");
      await expect(page.getByRole("button", { name: "Preview", exact: true })).toBeEnabled();
      await page.getByRole("button", { name: "Import .env…" }).click();
      await page.getByLabel("Paste the .env contents").fill("OPENAI_API_KEY=fake-new-key");
      await page.getByRole("button", { name: "Review import" }).click();
      await expect(page.getByRole("region", { name: "Import review" })).toContainText("demo / dev");
      await expect(page.getByRole("region", { name: "Import review" })).toContainText("Replace secret");
      expect(await page.evaluate(() => window.__akmTest.calls.filter((call) => call.command === "env_import_confirm").length)).toBe(0);
      await page.getByRole("button", { name: "Confirm import" }).click();
      await expect(page.getByText("Imported: 0 added, 1 updated.")).toBeVisible();
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
      await page.getByRole("button", { name: "Preview", exact: true }).click();
      await expect(page.getByText("OPENAI_API_KEY=fake-browser-test-value", { exact: true })).toBeVisible();
      await page.evaluate(() => window.__akmTest.emitLock());
      await expect(page.getByRole("button", { name: "Unlock vault", exact: true })).toBeVisible();
      await expect(page.getByText("OPENAI_API_KEY=fake-browser-test-value", { exact: true })).toHaveCount(0);
      expect(errors).toEqual([]);
    });
  }
}
