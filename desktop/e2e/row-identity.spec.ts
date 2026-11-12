import { expect, test, type Locator, type Page } from "@playwright/test";
import { installNativeBoundaryMock, type BoundaryOptions } from "./native-boundary";

// Fault classes M and H at the minimum window width (900 px). Look-alike rows show their IDs,
// every fact of a row stays on screen, and a value of 1,000 unbroken characters stays in its box.

async function open(page: Page, theme: string, options: BoundaryOptions, forced = false) {
  await page.setViewportSize({ width: 900, height: 800 });
  if (forced) await page.emulateMedia({ forcedColors: "active" });
  await installNativeBoundaryMock(page, theme, undefined, undefined, options);
  await page.goto("/");
  await expect(page.getByRole("button", { name: "Add key", exact: true })).toBeVisible();
}

/** The element shows its whole text: nothing is cut off, and no ellipsis stands in for it. */
async function readable(locator: Locator) {
  await expect(locator).toBeVisible();
  return locator.evaluate((node) => node.scrollWidth <= node.clientWidth && getComputedStyle(node).textOverflow !== "ellipsis");
}

/** `inner` lies inside `outer` from left to right. */
async function inside(inner: Locator, outer: Locator) {
  const a = (await inner.boundingBox())!;
  const b = (await outer.boundingBox())!;
  return a.x >= b.x - 0.5 && a.x + a.width <= b.x + b.width + 0.5;
}

const noSideScroll = (page: Page) => page.locator(".stage").evaluate((node) => node.scrollWidth - node.clientWidth);

const TWINS: BoundaryOptions = {
  keys: [
    { id: "twin-entry-aaaa-0001", title: "Twin", scopes: [] },
    { id: "twin-entry-bbbb-0002", title: "Twin", scopes: [] },
    { id: "other-entry-0003", title: "Twin", scopes: [], issuer: "Other issuer" },
  ],
};

for (const [theme, forced] of [["light", false], ["dark", false], ["dark", true]] as const) {
  test(`${theme}${forced ? ", forced colours" : ""}: two look-alike rows show their short IDs at 900 px, and a row that differs shows none`, async ({ page }) => {
    await open(page, theme, TWINS, forced);
    const rows = page.getByRole("button", { name: "Open Twin", exact: true });
    await expect(rows).toHaveCount(3);
    for (const id of ["twin-entry-a", "twin-entry-b"]) {
      const row = page.locator(".key-row", { hasText: `ID ${id}` });
      await expect(row).toHaveCount(1);
      expect(await readable(row.locator(".key-id")), id).toBe(true);
      expect(await inside(row.locator(".key-id"), row), id).toBe(true);
      await expect(row).toHaveAccessibleDescription(new RegExp(`, ID ${id}\\. `));
    }
    await expect(page.locator(".key-row", { hasText: "Other issuer" }).locator(".key-id")).toHaveCount(0);
    expect(await noSideScroll(page)).toBeLessThanOrEqual(0);
  });
}

const FULL_KEY = {
  id: "inventory-entry-0001", title: "Inventory key", issuer: "Acme Issuer", project: "inventory-project", environment: "staging-env",
  resolvedEnvVar: "INVENTORY_VAR", scopes: ["read", "write"], expiresAt: "2031-01-15T00:00:00Z", rotationDays: 90,
  secretChange: "known" as const, secretChangedAt: "2030-12-01T12:00:00Z", rotationDueAt: "2031-03-01T12:00:00Z", historyCount: 2,
};

for (const theme of ["light", "dark"]) {
  test(`${theme}: at 900 px a key row shows every fact that the old row showed, in full`, async ({ page }) => {
    await open(page, theme, { keys: [FULL_KEY] });
    const row = page.getByRole("button", { name: "Open Inventory key", exact: true });
    await expect(page.locator(".group-head", { hasText: "inventory-project" })).toBeVisible();
    for (const [name, locator] of [
      ["title", row.locator(".key-title")],
      ["issuer and scopes", row.locator(".key-sub", { hasText: "Acme Issuer · 2 scopes" })],
      ["variable", row.locator(".key-var", { hasText: "INVENTORY_VAR" })],
      ["environment", row.locator(".key-env", { hasText: "staging-env" })],
      ["renewal", row.locator(".key-expiry > span").first()],
      ["expiry", row.locator(".key-when-line .key-sub")],
      ["state", row.locator(".state")],
    ] as const) {
      expect(await readable(locator), name).toBe(true);
      await expect(locator, name).toBeInViewport();
      expect(await inside(locator, row), name).toBe(true);
    }
    await expect(row.locator(".key-expiry > span").first()).toHaveText(/^rotate in \d+ days$/);
    await expect(row.locator(".key-when-line .key-sub")).toHaveText(/2031/);
    await row.click();
    const history = page.locator(".fact-row", { hasText: "History" }).locator(".fact-value");
    await expect(history).toHaveText("2 earlier versions in the vault history");
    await expect(history).toBeInViewport();
  });

  test(`${theme}: at 900 px a Recycle Bin row shows every fact, with its history count`, async ({ page }) => {
    await open(page, theme, { recycled: [{ key: FULL_KEY, deletedAt: "2026-06-01T12:00:00Z" }] });
    await page.getByRole("button", { name: "Recycle Bin", exact: true }).click();
    const row = page.locator(".bin-row");
    for (const text of ["Inventory key", "inventory-project", "staging-env", "INVENTORY_VAR", "Acme Issuer", "ID inventory-entry-0001", "2 earlier versions"]) {
      const fact = row.locator(".key-title, .key-sub", { hasText: text }).first();
      expect(await readable(fact), text).toBe(true);
      await expect(fact, text).toBeInViewport();
    }
  });
}

// UI-R2-03, UI-R2-05 and LOOK-R3-03: the project context, the project total and the no-expiry
// mark stay on screen in the zero-project cases, at the minimum width.
const shelfTotal = (page: Page) => page.getByRole("navigation", { name: "Projects" }).locator(".shelf-label");
const groupOf = (page: Page, project: string) => page.locator(".key-group").filter({ has: page.getByRole("heading", { name: project, exact: true }) });

for (const theme of ["light", "dark"]) {
  test(`${theme}: at 900 px an all-unfiled vault names "No project" on its shelf and its heading, and counts zero projects`, async ({ page }) => {
    await open(page, theme, { keys: [{ id: "loose-entry-0001", title: "Loose A", project: null }, { id: "loose-entry-0002", title: "Loose B", project: null }] });
    await expect(shelfTotal(page)).toHaveText("Projects 0");
    await expect(shelfTotal(page)).toBeInViewport();
    const unfiled = page.getByRole("navigation", { name: "Projects" }).getByRole("button", { name: "No project", exact: true });
    await expect(unfiled).toBeInViewport();
    await expect(unfiled.locator(".num")).toHaveText("2");
    await expect(page.getByRole("heading", { name: "No project", exact: true })).toBeInViewport();
    for (const title of ["Loose A", "Loose B"]) await expect(groupOf(page, "No project").getByRole("button", { name: `Open ${title}`, exact: true })).toBeVisible();
  });

  test(`${theme}: at 900 px an empty vault counts zero projects`, async ({ page }) => {
    await open(page, theme, { keys: [] });
    await expect(page.getByRole("heading", { name: "No keys yet." })).toBeVisible();
    await expect(shelfTotal(page)).toHaveText("Projects 0");
    await expect(shelfTotal(page)).toBeInViewport();
  });

  test(`${theme}: at 900 px a search that leaves only unfiled keys still names "No project"`, async ({ page }) => {
    await open(page, theme, { keys: [{ id: "filed-entry-0001", title: "Alpha", project: "alpha-project" }, { id: "loose-entry-0001", title: "Loose", project: null }] });
    await page.locator("#key-search").fill("Loose");
    await expect(page.getByRole("button", { name: "Open Alpha", exact: true })).toHaveCount(0);
    await expect(page.getByRole("heading", { name: "No project", exact: true })).toBeInViewport();
    await expect(groupOf(page, "No project").getByRole("button", { name: "Open Loose", exact: true })).toBeVisible();
  });

  test(`${theme}: at 900 px the shelf shows the project total and a key with no expiry date says so`, async ({ page }) => {
    await open(page, theme, { keys: [{ id: "first-entry-0001", title: "Alpha", project: "one", expiresAt: null }, { id: "second-entry-0002", title: "Beta", project: "two", expiresAt: "2031-01-15T00:00:00Z" }] });
    await expect(shelfTotal(page)).toHaveText("Projects 2");
    await expect(shelfTotal(page)).toBeInViewport();
    const noExpiry = page.getByRole("button", { name: "Open Alpha", exact: true }).locator(".key-when-line .key-sub");
    await expect(noExpiry).toHaveText("no expiry");
    expect(await readable(noExpiry)).toBe(true);
    await expect(noExpiry).toBeInViewport();
  });
}

const LONG = "Q".repeat(1000);
const LONG_FIELDS: [string, Record<string, unknown>][] = [
  ["title", { title: LONG }],
  ["issuer", { issuer: LONG }],
  ["project", { project: LONG }],
  ["environment", { environment: LONG }],
  ["variable", { resolvedEnvVar: LONG }],
];

for (const theme of ["light", "dark"]) {
  for (const [field, change] of LONG_FIELDS) {
    test(`${theme}: at 900 px a ${field} of 1,000 unbroken characters stays in its box and moves no control out of view`, async ({ page }) => {
      await open(page, theme, { keys: [{ id: "long-entry-0001", title: "Long", ...change }, { id: "neighbour-entry-0002", title: "Neighbour" }] });
      expect(await noSideScroll(page)).toBeLessThanOrEqual(0);
      await expect(page.getByRole("button", { name: "Add key", exact: true })).toBeInViewport({ ratio: 1 });
      await expect(page.locator("#key-search")).toBeInViewport({ ratio: 1 });
      const row = page.getByRole("button", { name: field === "title" ? `Open ${LONG}` : "Open Long", exact: true });
      await expect(row).toHaveCount(1);
      const parts = await row.locator(".key-ident, .state, .row-arrow").all();
      expect(parts.length).toBe(4);
      for (const each of parts) expect(await inside(each, row), `${field} row part`).toBe(true);
      if (field === "project") {
        const heading = page.locator(".group-head", { hasText: "QQQQ" });
        expect(await inside(heading, page.locator(".ledger-pane")), "group heading").toBe(true);
        expect(await heading.evaluate((node) => node.scrollWidth <= node.clientWidth)).toBe(true);
      }
      const neighbour = page.getByRole("button", { name: "Open Neighbour", exact: true });
      await neighbour.scrollIntoViewIfNeeded();
      await expect(neighbour).toBeInViewport();

      await row.scrollIntoViewIfNeeded();
      await row.click();
      const drawer = page.locator(".drawer.raised");
      await expect(drawer).toBeVisible();
      expect(await drawer.evaluate((node) => node.scrollWidth - node.clientWidth), `${field} drawer`).toBeLessThanOrEqual(0);
      for (const name of ["Move to Recycle Bin", "Edit", "Close"]) await expect(drawer.locator(".drawer-foot").getByRole("button", { name, exact: true }), `${field} ${name}`).toBeInViewport({ ratio: 1 });
      await expect(drawer.getByRole("button", { name: "Close", exact: true }).first(), `${field} close icon`).toBeInViewport({ ratio: 1 });
      await drawer.getByRole("button", { name: "Move to Recycle Bin" }).click();
      const confirm = page.locator(".confirm");
      await expect(confirm).toBeVisible();
      expect(await confirm.evaluate((node) => node.scrollWidth - node.clientWidth), `${field} confirmation`).toBeLessThanOrEqual(0);
      for (const name of ["Cancel", "Move to Recycle Bin"]) await expect(confirm.getByRole("button", { name, exact: true }), `${field} ${name}`).toBeInViewport({ ratio: 1 });
    });
  }

  test(`${theme}: at 900 px the permanent-delete confirmation of a 1,000-character entry keeps its buttons in view`, async ({ page }) => {
    await open(page, theme, { recycled: [{ key: { title: LONG, project: LONG, resolvedEnvVar: LONG }, deletedAt: null }] });
    await page.getByRole("button", { name: "Recycle Bin", exact: true }).click();
    expect(await noSideScroll(page)).toBeLessThanOrEqual(0);
    await page.locator(".bin-row .danger-text").click();
    const confirm = page.locator(".confirm");
    expect(await confirm.evaluate((node) => node.scrollWidth - node.clientWidth)).toBeLessThanOrEqual(0);
    for (const name of ["Cancel", "Delete for good"]) await expect(confirm.getByRole("button", { name, exact: true }), name).toBeInViewport({ ratio: 1 });
  });
}
