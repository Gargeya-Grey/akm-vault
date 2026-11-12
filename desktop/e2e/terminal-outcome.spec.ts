import { expect, test, type Page } from "@playwright/test";
import { installNativeBoundaryMock, type BoundaryOptions } from "./native-boundary";
import type { OperationNotice } from "../src/types";

// The terminal outcome contract with native code, in headless Edge with a mocked native boundary.
// Every launch carries a fresh operation ID. Native keeps a record under it until the page
// acknowledges it, so a reply that the page never receives still has an outcome. These tests do
// not start a terminal: native launch behaviour needs the owner-present proof.

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

async function open(page: Page, theme: string, options: BoundaryOptions, notices?: OperationNotice[]) {
  await page.setViewportSize({ width: 900, height: 800 });
  await installNativeBoundaryMock(page, theme, notices, undefined, { nativeStore: true, ...options });
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/");
  await expect(page.getByRole("button", { name: "Add key", exact: true })).toBeVisible();
  return errors;
}

async function launch(page: Page) {
  await page.getByRole("button", { name: "Use keys", exact: true }).click();
  await page.getByLabel("Project", { exact: true }).selectOption("demo");
  await page.getByLabel("Environment", { exact: true }).selectOption("dev");
  await page.getByRole("button", { name: "Open terminal with keys…" }).click();
  const call = await page.waitForFunction(() => window.__akmTest.calls.find((each) => each.command === "env_launch_terminal")?.args);
  return (await call.jsonValue()) as { operationId: string };
}

const record = (page: Page, notice: OperationNotice) =>
  page.evaluate((notice) => (window as unknown as { __akmWarningIPC: (command: string, payload: unknown) => Promise<unknown> }).__akmWarningIPC("record", notice), notice);
const dismissals = (page: Page) => page.evaluate(() => window.__akmTest.calls.filter((call) => call.command === "vault_dismiss_warnings").map((call) => call.args.ids));

for (const theme of ["light", "dark"]) {
  test(`${theme}: a terminal launch sends a fresh operation ID, and its reply acknowledges the native record`, async ({ page }) => {
    const errors = await open(page, theme, {});
    const { operationId } = await launch(page);
    expect(operationId).toMatch(UUID);
    await expect(page.getByText("Opened a terminal with 1 key. That window keeps them until you close it, even after AKM locks.")).toBeVisible();
    await expect(page.getByRole("region", { name: "Operation outcomes" })).toContainText("A terminal opened with keys. It keeps them until you close it");
    await expect.poll(() => dismissals(page)).toContainEqual([operationId]);
    await page.reload();
    await expect(page.getByRole("button", { name: "Add key", exact: true })).toBeVisible();
    await page.evaluate(() => new Promise((done) => setTimeout(done, 3500)));
    await expect(page.getByRole("region", { name: "Saved-operation notices" })).toHaveCount(0);
    expect(errors).toEqual([]);
  });

  test(`${theme}: a lost terminal reply is settled from the native record while the page stays open`, async ({ page }) => {
    const errors = await open(page, theme, { terminal: "hold" });
    const { operationId } = await launch(page);
    const outcomes = page.getByRole("region", { name: "Operation outcomes" });
    await expect(outcomes).toContainText("A terminal launch is awaiting an outcome. A terminal may already hold keys.");
    await expect(page.getByRole("button", { name: "Dismiss operation outcomes" })).toBeDisabled();
    await record(page, { id: operationId, originSession: "A", kind: "terminalStarted" });
    await expect(outcomes).toContainText("A terminal opened with keys. It keeps them until you close it", { timeout: 8000 });
    await expect(page.getByRole("region", { name: "Saved-operation notices" })).toHaveCount(0);
    await expect.poll(() => dismissals(page)).toContainEqual([operationId]);
    await page.getByRole("button", { name: "Dismiss operation outcomes" }).click();
    await expect(outcomes).toHaveCount(0);
    expect(errors).toEqual([]);
  });

  test(`${theme}: after a reload during the native review, the record is the only truth and its notice names the terminal`, async ({ page }) => {
    const errors = await open(page, theme, { terminal: "hold" });
    const { operationId } = await launch(page);
    await page.reload();
    await expect(page.getByRole("button", { name: "Add key", exact: true })).toBeVisible();
    await record(page, { id: operationId, originSession: "A", kind: "terminalStarted" });
    const notices = page.getByRole("region", { name: "Saved-operation notices" });
    await expect(notices).toContainText("A terminal opened with keys, but this window did not receive its result. That terminal keeps the keys until you close it, even after AKM locks.", { timeout: 8000 });
    await expect(notices).toBeInViewport();
    await expect(page.getByRole("region", { name: "Operation outcomes" })).toHaveCount(0);
    await page.getByRole("button", { name: "Dismiss saved-operation notices" }).click();
    await expect(notices).toHaveCount(0);
    expect(await dismissals(page)).toContainEqual([operationId]);
    await page.reload();
    await expect(page.getByRole("button", { name: "Add key", exact: true })).toBeVisible();
    await page.evaluate(() => new Promise((done) => setTimeout(done, 3500)));
    await expect(notices).toHaveCount(0);
    expect(errors).toEqual([]);
  });

  test(`${theme}: at 900 px each new native notice shows its own text`, async ({ page }) => {
    const kinds = ["filePermissions", "journalLineageUnconfirmed", "terminalStarted", "terminalSessionEnded", "terminalFailed", "terminalDeclined"] as const;
    const errors = await open(page, theme, {}, kinds.map((kind) => ({ id: `retained-${kind}`, originSession: "A", kind })));
    const notices = page.getByRole("region", { name: "Saved-operation notices" }).locator(".notice");
    await expect(notices).toHaveCount(kinds.length);
    await expect(notices).toHaveText([
      /could not make its files private to your account\. Anyone who can read its folder can copy the encrypted vault and its backups\./,
      /changed outside AKM, so AKM cannot confirm that it is the same vault as before\./,
      /did not receive its result\. That terminal keeps the keys until you close it/,
      /its session ended before the result arrived\. That terminal keeps the keys until you close it/,
      /^A terminal launch failed\. No terminal from that request runs\.$/,
      /^A terminal launch was cancelled in a native window\. No terminal from that request runs\.$/,
    ]);
    for (const notice of await notices.all()) expect(await notice.evaluate((node) => node.scrollWidth <= node.clientWidth)).toBe(true);
    expect(await page.locator(".stage").evaluate((node) => node.scrollWidth - node.clientWidth)).toBeLessThanOrEqual(0);
    expect(errors).toEqual([]);
  });
}
