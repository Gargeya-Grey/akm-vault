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

// Class I: every dialog in the app. The inventory is `grep -rn "role=\"dialog\"" desktop/src`:
// KeyDetail, KeyForm (add and edit), RotateDialog, ConfirmDialog (move to the Recycle Bin, delete
// for good, format upgrade) and the Use keys import drawer.
const openers: { name: string; open: (page: Page) => Promise<void>; options?: Parameters<typeof installNativeBoundaryMock>[4] }[] = [
  { name: "the Add key drawer", open: (page) => page.getByRole("button", { name: "Add key", exact: true }).click() },
  {
    name: "the edit drawer",
    open: async (page) => {
      await page.locator(".key-row").first().click();
      await page.getByRole("button", { name: "Edit", exact: true }).click();
      await expect(page.getByRole("dialog", { name: "Edit key" })).toBeVisible();
    },
  },
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
    name: "the permanent-delete confirmation",
    open: async (page) => {
      await page.getByRole("button", { name: "Recycle Bin", exact: true }).click();
      await page.getByRole("button", { name: "Delete Twin for good" }).first().click();
      await expect(page.getByRole("heading", { name: "Delete this key for good?" })).toBeVisible();
    },
  },
  {
    name: "the import drawer",
    open: async (page) => {
      await page.getByRole("button", { name: "Use keys", exact: true }).click();
      await page.getByRole("button", { name: "Import .env…" }).click();
    },
  },
  {
    name: "the rotation dialog",
    open: async (page) => {
      await page.locator(".key-row").first().click();
      await page.getByRole("button", { name: "Rotate secret" }).click();
      await expect(page.getByRole("dialog", { name: "Rotate secret" })).toBeVisible();
    },
  },
  {
    name: "the format upgrade confirmation",
    options: { needsUpgrade: true },
    open: async (page) => {
      await page.getByRole("button", { name: "Review format upgrade" }).click();
      await expect(page.getByRole("heading", { name: "Upgrade this vault to KDBX 4.1?" })).toBeVisible();
    },
  },
];

for (const theme of ["light", "dark"]) {
  for (const { name, open: openDialog, options } of openers) {
    test(`${theme}: ${name} covers the rail and has an opaque surface`, async ({ page }) => {
      await open(page, theme, { recycled: [{ key: { title: "Twin" }, deletedAt: null }], ...options });
      await openDialog(page);
      const surface = page.locator(".overlay").last().locator(".raised");
      await expect(surface).toBeVisible();
      expect(await backdropCoversTheRail(page)).toBe(true);
      expect(surfaceIsOpaque(await surface.evaluate((node) => getComputedStyle(node).backgroundColor))).toBe(true);
    });
  }
}

/**
 * What a user with enlarged text cannot reach in the open dialog: an action outside the window or
 * covered by another element once it is scrolled into view, a dialog that leaves the window, and
 * anything that scrolls sideways.
 */
function outOfReach(page: Page) {
  return page.evaluate(() => {
    const surface = [...document.querySelectorAll(".overlay")].at(-1)!.querySelector<HTMLElement>(".raised")!;
    const problems: string[] = [];
    const scroller = document.scrollingElement!;
    if (scroller.scrollWidth > scroller.clientWidth) problems.push("the page scrolls sideways");
    if (surface.scrollWidth > surface.clientWidth) problems.push("the dialog scrolls sideways");
    const edge = surface.getBoundingClientRect();
    if (edge.left < 0 || edge.right > innerWidth) problems.push("the dialog leaves the window");
    for (const button of surface.querySelectorAll("button")) {
      button.scrollIntoView({ block: "nearest", inline: "nearest" });
      const box = button.getBoundingClientRect();
      const name = button.getAttribute("aria-label") ?? button.textContent?.trim();
      if (box.left < 0 || box.top < 0 || box.right > innerWidth || box.bottom > innerHeight) problems.push(`${name} is outside the window`);
      else if (!button.contains(document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2))) problems.push(`${name} is covered`);
    }
    return problems;
  });
}

// FW-07 and its class: 150 and 200 percent text (a root font of 24 and 32 px), at the minimum
// window width and at a common width.
for (const theme of ["light", "dark"]) {
  for (const { name, open: openDialog, options } of openers) {
    test(`${theme}: ${name} keeps every action in reach with enlarged text`, async ({ page }) => {
      // Earlier versions put a button low in the key drawer, where the sticky footer could cover it.
      await open(page, theme, { keys: [{ historyCount: 2 }], recycled: [{ key: { title: "Twin" }, deletedAt: null }], ...options });
      await openDialog(page);
      const surface = page.locator(".overlay").last().locator(".raised");
      await expect(surface).toBeVisible();
      // The drawer slides in from the right; measure where it comes to rest.
      await surface.evaluate((node) => Promise.all(node.getAnimations().map((animation) => animation.finished)));
      const found: [string, string[]][] = [];
      for (const width of [900, 1180]) {
        for (const font of [24, 32]) {
          await page.setViewportSize({ width, height: 800 });
          await page.evaluate((size) => { document.documentElement.style.fontSize = `${size}px`; }, font);
          found.push([`${width} px wide, root font ${font} px`, await outOfReach(page)]);
        }
      }
      expect(found.filter(([, problems]) => problems.length > 0)).toEqual([]);
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
    expect(await boxed(page, ".drawer .fact-list + .fact-list"), "the line between the row facts and the details").toBe(true);
    await page.keyboard.press("Escape");
    await page.getByRole("button", { name: "Use keys", exact: true }).click();
    expect(await boxed(page, ".btn-secondary")).toBe(true);
  });

  const lock = (page: Page) => page.evaluate(() => (window as unknown as { __akmTest: { emitLock: () => void } }).__akmTest.emitLock());
  const firstKey = (page: Page) => page.locator(".key-row").first().click();
  // A line between two regions, or the edge of a panel, that the normal themes draw with a shadow or
  // a fill. A line drawn by a pseudo-element names it. The fixture gives each list two rows.
  const lines: { name: string; reach: (page: Page) => Promise<unknown>; selector: string; pseudo?: "::before"; side: "Top" | "Right" | "Bottom" }[] = [
    { name: "the edge of the rail", reach: async () => {}, selector: ".rail", side: "Right" },
    { name: "the line above the rail tools", reach: async () => {}, selector: ".rail-tools", side: "Top" },
    { name: "the line above the drawer's actions", reach: (page) => page.locator(".key-row").first().click(), selector: ".drawer-foot", side: "Top" },
    { name: "the line under the Activity headings", reach: (page) => page.getByRole("button", { name: "Activity", exact: true }).click(), selector: ".audit-head", side: "Bottom" },
    { name: "the line under an Activity row", reach: (page) => page.getByRole("button", { name: "Activity", exact: true }).click(), selector: ".audit-row", side: "Bottom" },
    {
      name: "the line under the preview heading",
      reach: async (page) => {
        await page.getByRole("button", { name: "Use keys", exact: true }).click();
        await page.getByLabel("Project", { exact: true }).selectOption("demo");
        await page.getByLabel("Environment", { exact: true }).selectOption("dev");
        await page.getByRole("button", { name: "Preview", exact: true }).click();
      },
      selector: ".preview-head",
      side: "Bottom",
    },
    { name: "the edge of the lock stage", reach: lock, selector: ".lock-stage", side: "Right" },
    { name: "the line between two key rows", reach: async () => {}, selector: ".key-line + .key-line", pseudo: "::before", side: "Top" },
    { name: "the line between two Recycle Bin rows", reach: (page) => page.getByRole("button", { name: "Recycle Bin", exact: true }).click(), selector: ".bin-row + .bin-row", pseudo: "::before", side: "Top" },
    { name: "the line between two facts in the key drawer", reach: firstKey, selector: ".drawer .fact-row + .fact-row", pseudo: "::before", side: "Top" },
    { name: "the line between two groups of the key form", reach: (page) => page.getByRole("button", { name: "Add key", exact: true }).click(), selector: ".form-group + .form-group", pseudo: "::before", side: "Top" },
    { name: "the line between two Settings sections", reach: (page) => page.getByRole("button", { name: "Settings", exact: true }).click(), selector: ".settings-section + .settings-section", pseudo: "::before", side: "Top" },
    { name: "the line between two promises of the lock screen", reach: lock, selector: ".lock-promises li + li", pseudo: "::before", side: "Top" },
    { name: "the edge of the lock art", reach: lock, selector: ".lock-art", side: "Top" },
    { name: "the edge of the key tile in the key drawer", reach: firstKey, selector: ".drawer .key-tile", side: "Top" },
    { name: "the edge of a scope chip", reach: firstKey, selector: ".drawer .chip", side: "Top" },
  ];
  for (const { name, reach, selector, pseudo, side } of lines) {
    test(`${name} keeps a border`, async ({ page }) => {
      await open(page, "dark", { keys: [{}, { title: "Second" }], recycled: [{ key: { title: "Twin" }, deletedAt: null }, { key: { title: "Twin" }, deletedAt: null }] });
      await page.emulateMedia({ forcedColors: "active" });
      await reach(page);
      const line = await page.locator(selector).first().evaluate((node, [edge, part]) => {
        const style = getComputedStyle(node, part);
        return [style.getPropertyValue(`border-${edge}-style`), parseFloat(style.getPropertyValue(`border-${edge}-width`))];
      }, [side.toLowerCase(), pseudo] as const);
      expect(line[0] !== "none" && Number(line[1]) >= 1, `${selector}${pseudo ?? ""}: ${line.join(" ")}`).toBe(true);
    });
  }

  test("the open list shows its edge and its active option", async ({ page }) => {
    await open(page);
    await page.emulateMedia({ forcedColors: "active" });
    await page.locator("#filter-environment").click();
    expect(await boxed(page, ".select-menu")).toBe(true);
    const active = page.locator(".select-option.active");
    expect(await active.evaluate((node) => getComputedStyle(node).forcedColorAdjust)).toBe("none");
  });

  for (const selector of ["input[type=search]", "#filter-environment"]) {
    test(`keyboard focus on ${selector} shows an outline`, async ({ page }) => {
      await open(page);
      await page.emulateMedia({ forcedColors: "active" });
      await page.locator(selector).focus();
      expect(await page.locator(selector).evaluate((node) => getComputedStyle(node).outlineStyle)).toBe("solid");
    });
  }

  test("the current section, a status and a heading keep their marks", async ({ page }) => {
    await open(page);
    await page.emulateMedia({ forcedColors: "active" });
    const adjust = (selector: string, pseudo: string) => page.locator(selector).first().evaluate((node, part) => getComputedStyle(node, part || undefined).forcedColorAdjust, pseudo);
    for (const [selector, pseudo] of [[".nav-pill", "::before"], [".state-dot", ""], [".group-head", "::before"]] as const) {
      expect(await adjust(selector, pseudo), selector).toBe("none");
    }
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    expect(await adjust(".settings-section > h2", "::before"), "the stem of a Settings heading").toBe("none");
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

function manyEnvironments(count = 45) {
  return Array.from({ length: count }, (_, index) => ({ environment: `env-${String(index).padStart(3, "0")}` }));
}

async function menuScroll(page: Page) {
  return page.locator(".select-menu").evaluate((node) => node.scrollTop);
}

test("the wheel scrolls an open list, later options stay clickable, and the keyboard still reveals its option", async ({ page }) => {
  await open(page, "dark", { keys: manyEnvironments() });
  await page.locator("#filter-environment").click();
  const menu = page.locator(".select-menu");
  await expect(menu).toBeVisible();
  const box = (await menu.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  for (let turn = 0; turn < 5; turn += 1) await page.mouse.wheel(0, 200);
  await expect.poll(() => menuScroll(page)).toBeGreaterThan(400);
  const settled = await menuScroll(page);
  await page.waitForTimeout(150);
  expect(await menuScroll(page)).toBe(settled);
  const later = page.locator(".select-option", { hasText: "env-020" });
  await later.click();
  await expect(page.locator("#filter-environment")).toHaveValue("env-020");
  await page.locator("#filter-environment").focus();
  await page.keyboard.press("Enter");
  await page.keyboard.press("End");
  const last = page.locator(".select-option.active");
  await expect(last).toHaveText(/env-044/);
  await expect(last).toBeInViewport({ ratio: 1 });
});

test("a project name of 1,000 characters leaves its heading and Add key in view", async ({ page }) => {
  await page.setViewportSize({ width: 900, height: 620 });
  await open(page, "dark", { keys: [{ project: `P${"name ".repeat(200)}`.trim() }] });
  await page.locator(".shelf-item", { hasText: "Pname" }).click();
  await expect(page.locator(".list-head h1")).toContainText("Pname");
  const add = page.getByRole("button", { name: "Add key", exact: true });
  await expect(add).toBeInViewport({ ratio: 1 });
  await expect(page.locator(".list-head h1")).toBeInViewport();
  const heading = page.locator(".list-head h1");
  expect(await heading.evaluate((node) => node.scrollHeight)).toBeGreaterThan(await heading.evaluate((node) => node.clientHeight));
  expect(await heading.evaluate((node) => getComputedStyle(node).overflowY)).toBe("auto");
});

test("in forced colours, set and unset lock pins have different fills", async ({ page }) => {
  await open(page);
  await page.emulateMedia({ forcedColors: "active" });
  await page.evaluate(() => (window as unknown as { __akmTest: { emitLock: () => void } }).__akmTest.emitLock());
  await page.getByRole("button", { name: /new vault/i }).first().click();
  await page.locator('input[type="password"]').first().fill("fake-pass");
  const fill = (selector: string) => page.locator(selector).first().evaluate((node) => getComputedStyle(node).fill);
  await expect(page.locator(".lock-pin-set").first()).toBeAttached();
  expect(await fill(".lock-pin-set")).not.toBe(await fill(".lock-pin:not(.lock-pin-set)"));
});

for (const width of [900, 1040]) {
  test(`at ${width} px, recycled keys with one title show each fact in full`, async ({ page }) => {
    await page.setViewportSize({ width, height: 800 });
    const prefix = "shared-project-name-".repeat(6);
    await open(page, "dark", { recycled: [
      { key: { title: "Twin", project: `${prefix}alpha`, environment: `${prefix}prod`, resolvedEnvVar: `${"V".repeat(90)}_A`, historyCount: 2 }, deletedAt: "2026-06-01T12:00:00Z" },
      { key: { title: "Twin", project: `${prefix}beta`, environment: `${prefix}test`, resolvedEnvVar: `${"V".repeat(90)}_B`, historyCount: 3 }, deletedAt: null },
    ] });
    await page.getByRole("button", { name: "Recycle Bin", exact: true }).click();
    await expect(page.locator(".bin-row")).toHaveCount(2);
    for (const text of [`${prefix}alpha`, `${prefix}beta`, `${prefix}prod`, `${prefix}test`, `${"V".repeat(90)}_A`, `${"V".repeat(90)}_B`]) {
      const fact = page.locator(".bin-row .key-sub", { hasText: text }).first();
      await expect(fact).toBeVisible();
      expect(await fact.evaluate((node) => node.scrollWidth <= node.clientWidth && getComputedStyle(node).textOverflow !== "ellipsis"), text).toBe(true);
    }
    expect(await page.locator(".stage").evaluate((node) => node.scrollWidth - node.clientWidth)).toBeLessThanOrEqual(0);
  });
}

test("at 900 px, long variable names in the key list can be read in full", async ({ page }) => {
  await page.setViewportSize({ width: 900, height: 800 });
  const stem = `LONG_VARIABLE_NAME_${"X".repeat(70)}`;
  await open(page, "dark", { keys: [{ title: "One", resolvedEnvVar: `${stem}_A` }, { title: "Two", resolvedEnvVar: `${stem}_B` }] });
  for (const name of [`${stem}_A`, `${stem}_B`]) {
    const cell = page.locator(".key-var", { hasText: name });
    await expect(cell).toBeVisible();
    expect(await cell.evaluate((node) => node.scrollWidth <= node.clientWidth && getComputedStyle(node).textOverflow !== "ellipsis"), name).toBe(true);
  }
  expect(await page.locator(".stage").evaluate((node) => node.scrollWidth - node.clientWidth)).toBeLessThanOrEqual(0);
});

test("at 900 px, two project names that differ at the end can be told apart on the shelf", async ({ page }) => {
  await page.setViewportSize({ width: 900, height: 800 });
  const stem = "project-with-a-long-common-prefix-".repeat(3);
  await open(page, "dark", { keys: [{ project: `${stem}one` }, { project: `${stem}two` }] });
  for (const name of [`${stem}one`, `${stem}two`]) {
    const label = page.locator(".shelf-item span", { hasText: name }).first();
    await expect(label).toBeVisible();
    expect(await label.evaluate((node) => node.scrollWidth <= node.clientWidth && getComputedStyle(node).textOverflow !== "ellipsis"), name).toBe(true);
  }
});

for (const theme of ["light", "dark"]) {
  test(`${theme}: a 1,000-character project name stays inside the shelf and leaves its neighbours in view`, async ({ page }) => {
    await page.setViewportSize({ width: 900, height: 800 });
    await open(page, theme, { keys: [{ project: "Z".repeat(1000) }, { project: "aaa-neighbour" }, { project: "bbb-neighbour" }] });
    const shelf = (await page.locator(".shelf").boundingBox())!;
    for (const name of ["aaa-neighbour", "bbb-neighbour"]) {
      const box = (await page.locator(".shelf-item", { hasText: name }).boundingBox())!;
      expect(box.y, name).toBeGreaterThanOrEqual(shelf.y);
      expect(box.y + box.height, name).toBeLessThanOrEqual(shelf.y + shelf.height);
    }
    const long = (await page.locator(".shelf-item", { hasText: "ZZZ" }).boundingBox())!;
    expect(long.height).toBeLessThanOrEqual(shelf.height);
  });

  test(`${theme}: a long unbroken title stays inside the permanent-delete confirmation`, async ({ page }) => {
    await page.setViewportSize({ width: 900, height: 800 });
    await open(page, theme, { recycled: [{ key: { title: "T".repeat(600), resolvedEnvVar: "V".repeat(300) }, deletedAt: null }] });
    await page.getByRole("button", { name: "Recycle Bin", exact: true }).click();
    await page.locator(".bin-row .danger-text").click();
    const confirm = page.locator(".confirm");
    await expect(confirm).toBeVisible();
    expect(await confirm.evaluate((node) => node.scrollWidth - node.clientWidth)).toBeLessThanOrEqual(0);
  });
}

test("the journal detail of an import and an export is visible at 900 px", async ({ page }) => {
  await page.setViewportSize({ width: 900, height: 800 });
  await installNativeBoundaryMock(page, "dark", undefined, undefined, { auditEntries: [
    { seq: "1", ts: "2026-10-31T00:00:00Z", action: "import-completed", keyTitle: null, project: "demo", environment: "dev", keyId: null, detail: "fake import detail: 2 added" },
    { seq: "2", ts: "2026-10-31T00:00:00Z", action: "export-completed", keyTitle: null, project: "demo", environment: "dev", keyId: null, detail: "fake export detail: 3 keys" },
  ] });
  await page.goto("/");
  await page.getByRole("button", { name: "Activity", exact: true }).click();
  await expect(page.getByText("fake import detail: 2 added", { exact: true })).toBeVisible();
  await expect(page.getByText("fake export detail: 3 keys", { exact: true })).toBeVisible();
});
