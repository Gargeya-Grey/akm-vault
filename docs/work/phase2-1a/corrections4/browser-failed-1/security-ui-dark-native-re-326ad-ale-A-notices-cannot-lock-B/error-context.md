# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: security-ui.spec.ts >> dark: native reservation precedes credentials and stale A notices cannot lock B
- Location: e2e\security-ui.spec.ts:11:3

# Error details

```
Error: expect(received).toBeUndefined()

Received: {}
```

# Page snapshot

```yaml
- generic [ref=e2]:
  - banner [ref=e3]:
    - paragraph [ref=e4]: AKM
    - navigation "Sections" [ref=e5]:
      - list [ref=e6]:
        - listitem [ref=e7]:
          - button "Keys" [ref=e8] [cursor=pointer]
        - listitem [ref=e9]:
          - button "Activity" [ref=e10] [cursor=pointer]
        - listitem [ref=e11]:
          - button "Settings" [ref=e12] [cursor=pointer]
    - button "Reload vault" [ref=e13] [cursor=pointer]
    - button "Lock now" [ref=e14] [cursor=pointer]
    - button "Switch to light mode" [ref=e15] [cursor=pointer]: Light mode
  - main [ref=e16]:
    - generic [ref=e17]:
      - generic [ref=e18]:
        - heading [level=1] [ref=e19]:
          - text: Keys,
          - emphasis [ref=e20]: accounted for.
        - paragraph [ref=e21]: Search what this vault holds, reveal only when you mean to, and see what needs rotating. The file stays a normal KeePass vault.
      - button "Add key" [ref=e22] [cursor=pointer]
    - generic "Vault summary" [ref=e23]:
      - generic [ref=e24]:
        - generic [ref=e25]: "1"
        - generic [ref=e26]: key in this vault
      - generic [ref=e27]:
        - generic [ref=e28]: "1"
        - generic [ref=e29]: project
      - generic [ref=e30]:
        - generic [ref=e31]: "0"
        - generic [ref=e32]: need attention
      - generic [ref=e33]:
        - generic [ref=e34]: "1"
        - generic [ref=e35]: shown now
    - region "API keys" [ref=e36]:
      - generic [ref=e37]:
        - generic [ref=e38]:
          - generic [ref=e39]: Search keys
          - searchbox "Search keys" [ref=e40]
        - generic [ref=e41]:
          - generic [ref=e42]: Project
          - combobox "Project" [ref=e43] [cursor=pointer]:
            - option "All projects" [selected]
            - option "demo"
        - generic [ref=e44]:
          - generic [ref=e45]: Environment
          - combobox "Environment" [ref=e46] [cursor=pointer]:
            - option "All environments" [selected]
            - option "dev"
      - generic [aria-hidden] [ref=e47]:
        - generic [ref=e48]: Key
        - generic [ref=e49]: Project
        - generic [ref=e50]: Env var
        - generic [ref=e51]: Expiry
        - generic [ref=e52]: Status
      - list [ref=e53]:
        - listitem [ref=e54]:
          - button "Open OpenAI" [ref=e55] [cursor=pointer]:
            - generic [ref=e56]:
              - generic [ref=e57]: OpenAI
              - generic [ref=e58]: OpenAI · 1 scope
            - generic [ref=e59]:
              - generic [ref=e60]: demo
              - generic [ref=e61]: dev
            - generic [ref=e62]: OPENAI_API_KEY
            - generic [ref=e63]:
              - generic [ref=e64]: —
              - generic [ref=e65]: no rotation
            - generic [ref=e66]: OK
            - generic [aria-hidden] [ref=e68]: →
```

# Test source

```ts
  1  | import { expect, test } from "@playwright/test";
  2  | import { installNativeBoundaryMock } from "./native-boundary";
  3  | 
  4  | declare global {
  5  |   interface Window {
  6  |     __akmTest: { calls: { command: string; args: Record<string, unknown> }[]; emitLock: () => void; emitNotice: (sessionId: string) => void; owner: () => string | null };
  7  |   }
  8  | }
  9  | 
  10 | for (const theme of ["light", "dark"]) {
  11 |   test(`${theme}: native reservation precedes credentials and stale A notices cannot lock B`, async ({ page }) => {
  12 |     await installNativeBoundaryMock(page, theme);
  13 |     const errors: string[] = [];
  14 |     page.on("pageerror", (error) => errors.push(error.message));
  15 |     await page.goto("/");
  16 |     await expect(page.getByRole("button", { name: "Add key", exact: true })).toBeVisible();
  17 |     const a = await page.evaluate(() => window.__akmTest.owner());
  18 |     expect(a).not.toBeNull();
  19 |     await page.evaluate(() => window.__akmTest.emitLock());
  20 |     await page.getByLabel("Master password", { exact: true }).fill("fake-browser-password");
  21 |     await page.getByRole("button", { name: "Unlock vault", exact: true }).click();
  22 |     await expect(page.getByRole("button", { name: "Add key", exact: true })).toBeVisible();
  23 |     const b = await page.evaluate(() => window.__akmTest.owner());
  24 |     expect(b).not.toBe(a);
  25 |     const opening = await page.evaluate(() => window.__akmTest.calls.filter(({ command }) => ["vault_prepare_open", "vault_unlock"].includes(command)));
  26 |     expect(opening.map(({ command }) => command)).toEqual(["vault_prepare_open", "vault_unlock"]);
> 27 |     expect(opening[0].args).toBeUndefined();
     |                             ^ Error: expect(received).toBeUndefined()
  28 |     expect(opening[1].args.sessionId).toBe(b);
  29 |     await page.evaluate((sessionId) => { window.__akmTest.emitNotice(sessionId!); window.__akmTest.emitNotice(sessionId!); }, a);
  30 |     await expect(page.getByRole("button", { name: "Add key", exact: true })).toBeVisible();
  31 |     expect(await page.evaluate(() => window.__akmTest.owner())).toBe(b);
  32 |     await page.evaluate(() => window.__akmTest.emitLock());
  33 |     await expect(page.getByRole("button", { name: "Unlock vault", exact: true })).toBeVisible();
  34 |     expect(errors).toEqual([]);
  35 |   });
  36 | }
  37 | 
  38 | for (const width of [390, 1440]) {
  39 |   for (const theme of ["light", "dark"]) {
  40 |     test(`${width}px ${theme}: export gate, import review, theme switch and native lock`, async ({ page }) => {
  41 |       await page.setViewportSize({ width, height: 1000 });
  42 |       await installNativeBoundaryMock(page, theme);
  43 |       const errors: string[] = [];
  44 |       page.on("pageerror", (error) => errors.push(error.message));
  45 |       await page.goto("/");
  46 |       await expect(page.getByRole("button", { name: "Add key", exact: true })).toBeVisible();
  47 |       const nextTheme = theme === "light" ? "dark" : "light";
  48 |       await page.getByRole("button", { name: `Switch to ${nextTheme} mode` }).click();
  49 |       await expect(page.locator("html")).toHaveAttribute("data-theme", nextTheme);
  50 |       await page.getByRole("button", { name: `Switch to ${theme} mode` }).click();
  51 |       await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
  52 |       await page.getByRole("button", { name: "Settings", exact: true }).click();
  53 |       for (const name of ["Save .env…", "Preview", "Copy to clipboard"]) {
  54 |         await expect(page.getByRole("button", { name, exact: true })).toBeDisabled();
  55 |       }
  56 |       await page.getByLabel("Project", { exact: true }).selectOption("demo");
  57 |       await page.getByLabel("Environment", { exact: true }).selectOption("dev");
  58 |       await expect(page.getByRole("button", { name: "Preview", exact: true })).toBeEnabled();
  59 |       await page.getByRole("button", { name: "Import .env…" }).click();
  60 |       await page.getByLabel("Paste the .env contents").fill("OPENAI_API_KEY=fake-new-key");
  61 |       await page.getByRole("button", { name: "Review import" }).click();
  62 |       await expect(page.getByRole("region", { name: "Import review" })).toContainText("demo / dev");
  63 |       await expect(page.getByRole("region", { name: "Import review" })).toContainText("Replace secret");
  64 |       expect(await page.evaluate(() => window.__akmTest.calls.filter((call) => call.command === "env_import_confirm").length)).toBe(0);
  65 |       await page.getByRole("button", { name: "Confirm import" }).click();
  66 |       await expect(page.getByText("Imported: 0 added, 1 updated.")).toBeVisible();
  67 |       expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
  68 |       await page.getByRole("button", { name: "Preview", exact: true }).click();
  69 |       await expect(page.getByText("OPENAI_API_KEY=fake-browser-test-value", { exact: true })).toBeVisible();
  70 |       await page.evaluate(() => window.__akmTest.emitLock());
  71 |       await expect(page.getByRole("button", { name: "Unlock vault", exact: true })).toBeVisible();
  72 |       await expect(page.getByText("OPENAI_API_KEY=fake-browser-test-value", { exact: true })).toHaveCount(0);
  73 |       expect(errors).toEqual([]);
  74 |     });
  75 |   }
  76 | }
  77 | 
```