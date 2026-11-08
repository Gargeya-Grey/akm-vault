import { expect, test } from "@playwright/test";
import { installNativeBoundaryMock } from "./native-boundary";

for (const theme of ["light", "dark"]) {
 for (const driver of ["dom", "native"] as const) {
  for (const operation of ["import", "export"] as const) {
    for (const reply of ["success", "error", ...(operation === "export" ? ["cancel" as const] : [])] as const) {
      test(`${theme} ${driver} ${operation} ${reply}: hidden view handoff retains a path-free outcome`, async ({ page }) => {
        await page.setViewportSize({ width: 390, height: 1000 });
        await installNativeBoundaryMock(page, theme);
        await page.addInitScript(({ operation }) => {
          const host = window as unknown as { __TAURI_INTERNALS__: { invoke: (command: string, args: unknown) => Promise<unknown> }; settleOutcome: (reply: string) => void };
          const original = host.__TAURI_INTERNALS__.invoke;
          host.__TAURI_INTERNALS__.invoke = async (command, args) => {
            if (command === (operation === "import" ? "env_import" : "env_save")) {
              return new Promise((resolve, reject) => {
                host.settleOutcome = (reply) => reply === "error"
                  ? reject({ kind: "io", message: "FAKE_PRIVATE_ERROR C:/private-origin-a/export.env" })
                  : resolve(reply === "cancel" ? null : operation === "import" ? { added: 0, updated: 1 } : "C:/private-origin-a/export.env");
              });
            }
            return original(command, args);
          };
        }, { operation });
        const errors: string[] = [];
        page.on("pageerror", (error) => errors.push(error.message));
        await page.goto("/");
        await page.getByRole("button", { name: "Use keys", exact: true }).click();
        await page.getByLabel("Project", { exact: true }).selectOption("demo");
        await page.getByLabel("Environment", { exact: true }).selectOption("dev");
        if (operation === "import") {
          await page.getByRole("button", { name: "Import .env…" }).click();
          await page.getByLabel("Paste the .env contents").fill("OPENAI_API_KEY=fake-handoff-secret");
          await page.getByRole("button", { name: "Review import" }).click();
        } else await page.getByRole("button", { name: "Save .env…" }).click();
        const outcomes = page.getByRole("region", { name: "Operation outcomes" });
        await expect(outcomes).toContainText("awaiting an outcome");
        await expect(page.getByRole("button", { name: "Dismiss operation outcomes" })).toBeDisabled();
        // DOM and owned-native-notice tests are distinct mocked boundaries.
        // Neither is a real Windows minimize; that remains an owner-ready proof.
        if (driver === "native") await page.evaluate(() => {
          if (document.hidden) throw new Error("Native-notice fixture must remain DOM-visible");
          window.__akmTest.expireView();
        });
        else await page.evaluate(() => {
          Object.defineProperty(document, "hidden", { configurable: true, get: () => true });
          document.dispatchEvent(new Event("visibilitychange"));
          Object.defineProperty(document, "hidden", { configurable: true, get: () => false });
          document.dispatchEvent(new Event("visibilitychange"));
        });
        await expect(page.getByRole("dialog")).toHaveCount(0);
        await expect(page.getByRole("button", { name: "Save .env…", exact: true })).toBeEnabled();
        await page.getByRole("button", { name: "Keys", exact: true }).click();
        await page.evaluate((reply) => (window as unknown as { settleOutcome: (reply: string) => void }).settleOutcome(reply), reply);
        await expect(outcomes).toContainText(reply === "cancel" ? "cancelled" : reply === "success" ? "completed" : "reported an error");
        if (reply === "cancel") { await expect(outcomes).toContainText("No file was saved by this request."); await expect(outcomes).not.toContainText("completed"); }
        await expect(outcomes).not.toContainText("fake-handoff-secret");
        await expect(outcomes).not.toContainText("FAKE_PRIVATE_ERROR");
        await expect(outcomes).not.toContainText("private-origin-a");
        await expect(page.getByRole("button", { name: "Add key", exact: true })).toBeVisible();
        expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
        await page.getByRole("button", { name: "Dismiss operation outcomes" }).click();
        await expect(outcomes).toHaveCount(0);
        expect(errors).toEqual([]);
      });
    }
  }
 }
}
