import { expect, test, type Page } from "@playwright/test";
import { installNativeBoundaryMock } from "./native-boundary";

// Regression checks from the review of the quiet-instrument look (8 Nov 2026).

async function open(page: Page, theme = "dark", keys?: Parameters<typeof installNativeBoundaryMock>[4]) {
  await installNativeBoundaryMock(page, theme, undefined, undefined, keys ?? {});
  await page.goto("/");
  await expect(page.getByRole("button", { name: "Add key", exact: true })).toBeVisible();
}

function backdropCoversTheRail(page: Page) {
  return page.evaluate(() => {
    const controls = [...document.querySelectorAll<HTMLElement>(".rail button")];
    return controls.length > 0 && controls.every((control) => {
      const box = control.getBoundingClientRect();
      return !!document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2)?.closest(".overlay");
    });
  });
}

function surfaceIsOpaque(colour: string) {
  return !/\/\s*(0?\.\d+|\d+%)|rgba\(|transparent/.test(colour);
}

function tokenColour(page: Page, name: string) {
  return page.evaluate((property) => {
    const probe = document.createElement("span");
    probe.style.color = `var(${property})`;
    document.body.append(probe);
    const value = getComputedStyle(probe).color;
    probe.remove();
    return value;
  }, name);
}

const openers: { name: string; open: (page: Page) => Promise<void> }[] = [
  { name: "the Add key drawer", open: (page) => page.getByRole("button", { name: "Add key", exact: true }).click() },
  { name: "the key drawer", open: (page) => page.locator(".key-row").first().click() },
  {
    name: "the delete confirmation",
    open: async (page) => {
      await page.locator(".key-row").first().click();
      await page.getByRole("button", { name: "Move to Recycle Bin" }).click();
      await expect(page.getByRole("alertdialog").or(page.getByRole("dialog", { name: /Recycle Bin/ }))).toBeVisible();
    },
  },
  {
    name: "the import drawer",
    open: async (page) => {
      await page.getByRole("button", { name: "Use keys", exact: true }).click();
      await page.getByRole("button", { name: "Import .env…" }).click();
    },
  },
];

for (const theme of ["light", "dark"]) {
  for (const { name, open: openDialog } of openers) {
    test(`${theme}: ${name} covers the rail and has an opaque surface`, async ({ page }) => {
      await open(page, theme);
      await openDialog(page);
      const surface = page.locator(".overlay").last().locator(".raised");
      await expect(surface).toBeVisible();
      expect(await backdropCoversTheRail(page)).toBe(true);
      expect(surfaceIsOpaque(await surface.evaluate((node) => getComputedStyle(node).backgroundColor))).toBe(true);
    });
  }
}

test.describe("forced colours", () => {
  const boxed = async (page: Page, selector: string) => page.locator(selector).first().evaluate((node) => {
    const style = getComputedStyle(node);
    return style.borderTopStyle !== "none" && parseFloat(style.borderTopWidth) >= 1;
  });

  test("controls and surfaces keep a boundary", async ({ page }) => {
    await open(page);
    await page.emulateMedia({ forcedColors: "active" });
    for (const selector of [".btn", ".key-rows", "input[type=search]"]) expect(await boxed(page, selector), selector).toBe(true);
    await page.locator(".key-row").first().click();
    expect(await boxed(page, ".drawer.raised")).toBe(true);
    await page.keyboard.press("Escape");
    await page.getByRole("button", { name: "Use keys", exact: true }).click();
    expect(await boxed(page, ".btn-secondary")).toBe(true);
  });

  test("the open list shows its edge and its active option", async ({ page }) => {
    await open(page);
    await page.emulateMedia({ forcedColors: "active" });
    await page.locator("#filter-environment").click();
    expect(await boxed(page, ".select-menu")).toBe(true);
    const active = page.locator(".select-option.active");
    expect(await active.evaluate((node) => getComputedStyle(node).forcedColorAdjust)).toBe("none");
  });

  test("the reminder switch is the system's checkbox", async ({ page }) => {
    await open(page);
    await page.emulateMedia({ forcedColors: "active" });
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    const box = page.locator(".reminder-toggle input");
    expect(await box.evaluate((node) => getComputedStyle(node).appearance)).toBe("auto");
    expect(await box.evaluate((node) => getComputedStyle(node, "::before").display)).toBe("none");
  });

  for (const selector of ["input[type=search]", "#filter-environment"]) {
    test(`keyboard focus on ${selector} shows an outline`, async ({ page }) => {
      await open(page);
      await page.emulateMedia({ forcedColors: "active" });
      await page.locator(selector).focus();
      expect(await page.locator(selector).evaluate((node) => getComputedStyle(node).outlineStyle)).toBe("solid");
    });
  }

  test("the current section and a status keep their marks", async ({ page }) => {
    await open(page);
    await page.emulateMedia({ forcedColors: "active" });
    for (const [selector, pseudo] of [[".nav-pill", "::before"], [".state-dot", ""]] as const) {
      expect(await page.locator(selector).first().evaluate((node, part) => getComputedStyle(node, part || undefined).forcedColorAdjust, pseudo), selector).toBe("none");
    }
  });
});

for (const width of [900, 1040, 1180, 1440]) {
  test(`at ${width} px, a long project name leaves Add key in view`, async ({ page }) => {
    await page.setViewportSize({ width, height: 800 });
    await open(page, "dark", { keys: [{ project: `Project${"W".repeat(80)}` }] });
    await page.locator(".shelf-item", { hasText: "ProjectW" }).click();
    await expect(page.locator(".list-head h1")).toContainText("ProjectW");
    const add = (await page.getByRole("button", { name: "Add key", exact: true }).boundingBox())!;
    const heading = (await page.locator(".list-head h1").boundingBox())!;
    expect(add.x + add.width).toBeLessThanOrEqual(width);
    expect(heading.x + heading.width).toBeLessThanOrEqual(add.x);
    expect(await page.locator(".stage").evaluate((node) => node.scrollWidth - node.clientWidth)).toBeLessThanOrEqual(0);
  });
}

test("a long tag or scope stays inside the key drawer", async ({ page }) => {
  await open(page, "dark", { keys: [{ tags: ["T".repeat(120)], scopes: ["S".repeat(120)] }] });
  await page.locator(".key-row").first().click();
  const drawer = page.locator(".drawer.raised");
  await expect(drawer).toBeVisible();
  expect(await drawer.evaluate((node) => node.scrollWidth - node.clientWidth)).toBeLessThanOrEqual(0);
});

test("the marker follows its item when text metrics change without a resize", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.addStyleTag({ content: ".nav-link { min-height: 90px; }" });
  await expect.poll(() => page.evaluate(() => {
    const pill = document.querySelector(".nav-pill")!.getBoundingClientRect();
    const current = document.querySelector('.nav-link[aria-current="page"]')!.getBoundingClientRect();
    return Math.abs(pill.top - current.top) < 1 && Math.abs(pill.height - current.height) < 1;
  })).toBe(true);
});

test("many projects leave the search and the first key in view", async ({ page }) => {
  await page.setViewportSize({ width: 1180, height: 700 });
  const keys = Array.from({ length: 120 }, (_, index) => ({ project: `project-${String(index).padStart(3, "0")}` }));
  await open(page, "dark", { keys });
  expect((await page.locator(".search input").boundingBox())!.y).toBeLessThan(330);
  expect((await page.locator(".key-row").first().boundingBox())!.y).toBeLessThan(560);
  const last = page.locator(".shelf-item", { hasText: "project-119" });
  await last.scrollIntoViewIfNeeded();
  await last.click();
  await expect(last).toHaveAttribute("aria-pressed", "true");
});

for (const theme of ["light", "dark"]) {
  test(`${theme}: a destructive text button keeps its danger colour`, async ({ page }) => {
    await open(page, theme);
    await page.locator(".key-row").first().click();
    const button = page.getByRole("button", { name: "Move to Recycle Bin" });
    await page.mouse.move(0, 0);
    expect(await button.evaluate((node) => getComputedStyle(node).color)).toBe(await tokenColour(page, "--danger"));
    await button.hover();
    expect(await button.evaluate((node) => getComputedStyle(node).color)).toBe(await tokenColour(page, "--danger"));
  });
}
