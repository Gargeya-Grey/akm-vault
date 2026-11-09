import { expect, test } from "@playwright/test";
import { installNativeBoundaryMock } from "./native-boundary";

// Playwright hides scrollbars by default, and this check needs a real one to drag.
test.use({ launchOptions: { ignoreDefaultArgs: ["--hide-scrollbars"] } });

test("dragging the scrollbar scrolls an open list and leaves it open", async ({ page }) => {
  const keys = Array.from({ length: 45 }, (_, index) => ({ environment: `env-${String(index).padStart(3, "0")}` }));
  await installNativeBoundaryMock(page, "dark", undefined, undefined, { keys });
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/");
  await page.locator("#filter-environment").click();
  const menu = page.locator(".select-menu");
  await expect(menu).toBeVisible();
  const box = (await menu.boundingBox())!;
  const bar = await menu.evaluate((node) => (node as HTMLElement).offsetWidth - (node as HTMLElement).clientWidth);
  expect(bar).toBeGreaterThan(0);
  await page.mouse.move(box.x + box.width - bar / 2, box.y + 12);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width - bar / 2, box.y + 150, { steps: 6 });
  await page.mouse.up();
  await expect.poll(() => menu.evaluate((node) => node.scrollTop)).toBeGreaterThan(50);
  await expect(menu).toBeVisible();
});
