import { expect, test, type Page } from "@playwright/test";
import { installNativeBoundaryMock } from "./native-boundary";

// Unit 4: accessible key status, the unlock summary, focus in modals and the reminder setting,
// in headless Edge with a mocked native boundary. They do not prove native notification delivery.

const DAY = 86_400_000;
const at = (days: number) => new Date(Date.now() + days * DAY).toISOString();

const keys = () => [
  { title: "Old key", issuer: "Acme", expiresAt: at(-3) },
  { title: "Late rotation", rotationDays: 30, secretChange: "known" as const, secretChangedAt: at(-40), rotationDueAt: at(-10) },
  { title: "Soon key", expiresAt: at(10) },
  { title: "Unknown key", rotationDays: 30 },
  { title: "Fine key", expiresAt: at(400) },
];

async function open(page: Page, theme: string, reminders?: { enabled: boolean; permission: "granted" | "denied" | "prompt"; delivery: "none" | "sent" | "failed"; blocked: boolean }) {
  await installNativeBoundaryMock(page, theme, undefined, undefined, { keys: keys(), reminders });
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("/");
  await expect(page.getByRole("button", { name: "Add key", exact: true })).toBeVisible();
  return errors;
}

for (const theme of ["light", "dark"]) {
  test(`${theme}: each key row announces its expiry and rotation status`, async ({ page }) => {
    const errors = await open(page, theme);
    const expectations: [string, string][] = [
      ["Old key", "Expired 3 days ago. No rotation interval."],
      ["Late rotation", "No expiry date. Rotation overdue by 10 days."],
      ["Soon key", "Expires in 10 days. No rotation interval."],
      ["Unknown key", "No expiry date. Rotation unknown, no known secret-change time."],
      ["Fine key", "Expires in 400 days. No rotation interval."],
    ];
    for (const [title, description] of expectations) {
      const row = page.getByRole("button", { name: `Open ${title}`, exact: true });
      await expect(row).toHaveAccessibleName(`Open ${title}`);
      await expect(row).toHaveAccessibleDescription(`Project demo, environment dev, variable OPENAI_API_KEY. ${description}`);
    }
    expect(errors).toEqual([]);
  });

  test(`${theme}: the unlock summary is dismissible by keyboard, stays away, and a lock removes it`, async ({ page }) => {
    const errors = await open(page, theme);
    const summary = page.getByRole("region", { name: "Keys needing attention" });
    await expect(summary.getByRole("heading")).toHaveText("4 keys need attention");
    await expect(summary.getByRole("listitem")).toHaveText([
      "Expired (1): Old key",
      "Rotation overdue (1): Late rotation",
      "Due soon (1): Soon key",
      "Rotation unknown (1): Unknown key",
    ]);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
    await page.getByRole("button", { name: "Dismiss summary" }).focus();
    await page.keyboard.press("Enter");
    await expect(summary).toHaveCount(0);
    await expect(page.getByRole("searchbox", { name: "Search keys" })).toBeFocused();
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await page.getByRole("button", { name: "Keys", exact: true }).click();
    await expect(summary).toHaveCount(0);
    await page.evaluate(() => window.__akmTest.emitLock());
    await expect(page.getByRole("button", { name: "Unlock vault", exact: true })).toBeVisible();
    await expect(page.getByText("Old key")).toHaveCount(0);
    await page.getByLabel("Master password", { exact: true }).fill("fake-browser-password");
    await page.getByRole("button", { name: "Unlock vault", exact: true }).click();
    await expect(summary.getByRole("heading")).toHaveText("4 keys need attention");
    expect(errors).toEqual([]);
  });

  test(`${theme}: a modal keeps focus inside, closes on Escape and returns focus to its row`, async ({ page }) => {
    const errors = await open(page, theme);
    const row = page.getByRole("button", { name: "Open Soon key", exact: true });
    await row.focus();
    await page.keyboard.press("Enter");
    const dialog = page.getByRole("dialog", { name: "Soon key" });
    await expect(dialog).toBeVisible();
    for (let step = 0; step < 14; step += 1) {
      await page.keyboard.press("Tab");
      expect(await dialog.evaluate((node) => node.contains(document.activeElement))).toBe(true);
    }
    for (let step = 0; step < 14; step += 1) {
      await page.keyboard.press("Shift+Tab");
      expect(await dialog.evaluate((node) => node.contains(document.activeElement))).toBe(true);
    }
    await dialog.getByRole("button", { name: "Rotate secret" }).focus();
    await page.keyboard.press("Enter");
    await expect(page.getByRole("dialog", { name: "Rotate secret" })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("dialog", { name: "Soon key" })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("dialog")).toHaveCount(0);
    await expect(row).toBeFocused();
    expect(errors).toEqual([]);
  });

  test(`${theme}: the reminder setting is off, turns on by keyboard, and shows a denied permission as a state`, async ({ page }) => {
    const errors = await open(page, theme);
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    const toggle = page.getByRole("checkbox", { name: "Show a Windows notification when keys need attention" });
    await expect(toggle).not.toBeChecked();
    await expect(toggle).toHaveAccessibleDescription(/only how many keys need attention/);
    await expect(page.getByRole("status").filter({ hasText: "Off." })).toBeVisible();
    await toggle.focus();
    await page.keyboard.press("Space");
    await expect(toggle).toBeChecked();
    await expect(page.getByRole("status").filter({ hasText: "On. AKM sends one count-only notification" })).toBeVisible();
    expect(await page.evaluate(() => window.__akmTest.calls.filter((call) => call.command === "reminders_set"))).toEqual([{ command: "reminders_set", args: { enabled: true } }]);
    expect(errors).toEqual([]);
  });

  test(`${theme}: a denied notification permission is visible in Settings and nothing is sent`, async ({ page }) => {
    const errors = await open(page, theme, { enabled: true, permission: "denied", delivery: "none", blocked: true });
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await expect(page.getByRole("status").filter({ hasText: "Windows has not allowed notifications for AKM" })).toBeVisible();
    expect(await page.evaluate(() => window.__akmTest.calls.filter((call) => call.command === "reminders_notify"))).toEqual([]);
    expect(errors).toEqual([]);
  });
}
