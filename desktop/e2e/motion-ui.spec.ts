import { expect, test, type Page } from "@playwright/test";
import { installNativeBoundaryMock } from "./native-boundary";

// Regression checks for the motion and hover rules (UI review, 1 Nov 2026).

async function open(page: Page, theme: string) {
  await installNativeBoundaryMock(page, theme);
  await page.goto("/");
  await expect(page.getByRole("button", { name: "Add key", exact: true })).toBeVisible();
}

function token(page: Page, name: string) {
  return page.evaluate((property) => {
    const probe = document.createElement("span");
    probe.style.color = `var(${property})`;
    document.body.append(probe);
    const value = getComputedStyle(probe).color;
    probe.remove();
    return value;
  }, name);
}

for (const theme of ["light", "dark"]) {
  test(`${theme}: a dialog and its notice are fully opaque from the first frame`, async ({ page }) => {
    await open(page, theme);
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await page.getByRole("button", { name: "Import .env…" }).click();
    const dialog = page.getByRole("dialog");
    await expect(dialog).toBeVisible();
    const animated = await dialog.evaluate((element) => {
      const overlay = element.closest(".overlay")!;
      return [overlay, ...overlay.querySelectorAll("*")]
        .flatMap((node) => node.getAnimations())
        .map((animation) => (animation.effect as KeyframeEffect).getKeyframes())
        .some((frames) => frames.some((frame) => "opacity" in frame));
    });
    expect(animated).toBe(false);
    for (const element of [page.locator(".overlay"), dialog]) {
      expect(await element.evaluate((node) => getComputedStyle(node).opacity)).toBe("1");
    }
  });

  test(`${theme}: colour changes are instant, and only transform animates`, async ({ page }) => {
    await open(page, theme);
    const selectors = [".btn", ".btn-secondary", ".text-button", ".theme", ".nav-link", "select", ".key-row", ".row-arrow"];
    const animated = await page.evaluate((list) => {
      const found: string[] = [];
      for (const selector of list) {
        for (const node of document.querySelectorAll(selector)) {
          const style = getComputedStyle(node);
          const properties = style.transitionProperty.split(",").map((value) => value.trim());
          const durations = style.transitionDuration.split(",").map((value) => parseFloat(value));
          properties.forEach((property, index) => {
            const duration = durations[index % durations.length];
            if (duration > 0 && property !== "transform") found.push(`${selector}: ${property}`);
          });
        }
      }
      return [...new Set(found)];
    }, selectors);
    expect(animated).toEqual([]);
  });

  test(`${theme}: reduced motion removes press and arrow movement`, async ({ page }) => {
    await open(page, theme);
    const add = page.getByRole("button", { name: "Add key", exact: true });
    const pressed = async () => {
      await add.hover();
      await page.mouse.down();
      await page.waitForTimeout(250);
      const value = await add.evaluate((node) => getComputedStyle(node).transform);
      // Release away from the button, so the press never becomes a click.
      await page.mouse.move(0, 0);
      await page.mouse.up();
      return value;
    };
    expect(await pressed()).toMatch(/^matrix\(0\.97/);
    await page.emulateMedia({ reducedMotion: "reduce" });
    expect(await pressed()).toBe("none");
  });

  test(`${theme}: a selected filter shows the ink border on hover, and filled buttons keep their token fill`, async ({ page }) => {
    await open(page, theme);
    const filter = page.locator("#filter-project");
    await filter.selectOption({ index: 1 });
    await page.mouse.move(0, 0);
    const leaf = await token(page, "--leaf-ink");
    await expect.poll(() => filter.evaluate((node) => getComputedStyle(node).borderTopColor)).toBe(leaf);
    await filter.hover();
    const ink = await token(page, "--ink");
    await expect.poll(() => filter.evaluate((node) => getComputedStyle(node).borderTopColor)).toBe(ink);

    const add = page.getByRole("button", { name: "Add key", exact: true });
    const forest = await page.evaluate(() => {
      const probe = document.createElement("span");
      probe.style.backgroundColor = "var(--forest)";
      document.body.append(probe);
      const value = getComputedStyle(probe).backgroundColor;
      probe.remove();
      return value;
    });
    await add.hover();
    await page.waitForTimeout(250);
    expect(await add.evaluate((node) => getComputedStyle(node).backgroundColor)).toBe(forest);
    expect(await add.evaluate((node) => getComputedStyle(node).textDecorationLine)).toContain("underline");
  });
}
