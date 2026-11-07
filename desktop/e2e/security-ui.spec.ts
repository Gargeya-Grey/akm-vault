import { expect, test } from "@playwright/test";
import { installNativeBoundaryMock } from "./native-boundary";

declare global {
  interface Window {
    __akmTest: { calls: { command: string; args: Record<string, unknown> }[]; emitLock: () => void; emitNotice: (sessionId: string) => void; expireView: (sessionId?: string | null, hideId?: string) => string; owner: () => string | null };
  }
}

for (const theme of ["light", "dark"]) {
  test(`${theme}: native reservation precedes credentials and stale A notices cannot lock B`, async ({ page }) => {
    await installNativeBoundaryMock(page, theme);
    const errors: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await page.goto("/");
    await expect(page.getByRole("button", { name: "Add key", exact: true })).toBeVisible();
    const a = await page.evaluate(() => window.__akmTest.owner());
    expect(a).not.toBeNull();
    await page.evaluate(() => window.__akmTest.emitLock());
    await page.getByLabel("Master password", { exact: true }).fill("fake-browser-password");
    await page.getByRole("button", { name: "Unlock vault", exact: true }).click();
    await expect(page.getByRole("button", { name: "Add key", exact: true })).toBeVisible();
    const b = await page.evaluate(() => window.__akmTest.owner());
    expect(b).not.toBe(a);
    const opening = await page.evaluate(() => window.__akmTest.calls.filter(({ command }) => ["vault_prepare_open", "vault_unlock"].includes(command)));
    expect(opening.map(({ command }) => command)).toEqual(["vault_prepare_open", "vault_unlock"]);
    expect(opening[0].args).toEqual({}); // Tauri's core.invoke normalizes omitted arguments to {}.
    expect(opening[1].args.sessionId).toBe(b);
    await page.evaluate((sessionId) => { window.__akmTest.emitNotice(sessionId!); window.__akmTest.emitNotice(sessionId!); }, a);
    await expect(page.getByRole("button", { name: "Add key", exact: true })).toBeVisible();
    expect(await page.evaluate(() => window.__akmTest.owner())).toBe(b);
    await page.evaluate(() => window.__akmTest.emitLock());
    await expect(page.getByRole("button", { name: "Unlock vault", exact: true })).toBeVisible();
    expect(errors).toEqual([]);
  });
}

test("hiding the document with an import confirmation pending reaches native without a minimize event", async ({ page }) => {
  await installNativeBoundaryMock(page, "light");
  await page.goto("/");
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "Import .env…" }).click();
  await page.getByLabel("Paste the .env contents").fill("OPENAI_API_KEY=fake-browser-value");
  await page.getByLabel("Target project").fill("demo");
  await page.getByLabel("Target environment").fill("dev");
  await page.getByRole("button", { name: "Review import" }).click();
  await expect.poll(() => page.evaluate(() => window.__akmTest.calls.filter(({ command }) => command === "env_import").length)).toBe(1);
  expect(await page.evaluate(() => window.__akmTest.calls.filter(({ command }) => command === "view_discard").length)).toBe(0);
  await page.evaluate(() => {
    Object.defineProperty(document, "hidden", { configurable: true, get: () => true });
    document.dispatchEvent(new Event("visibilitychange"));
  });
  await expect.poll(() => page.evaluate(() => window.__akmTest.calls.filter(({ command }) => command === "view_discard").length)).toBe(1);
  expect(await page.evaluate(() => window.__akmTest.calls.filter(({ command }) => command === "vault_lock").length)).toBe(0);
});

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
      await expect(page.getByText("Imported: 0 added, 1 updated.")).toBeVisible();
      const imports = await page.evaluate(() => window.__akmTest.calls.filter((call) => call.command === "env_import"));
      expect(imports).toHaveLength(1);
      expect(await page.evaluate(() => window.__akmTest.calls.some((call) => call.command === "env_import_preview" || call.command === "env_import_confirm"))).toBe(false);
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
