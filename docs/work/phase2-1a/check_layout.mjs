import { chromium } from "file:///W:/API%20Keys%20Manager/desktop/node_modules/playwright-core/index.mjs";
import { preview } from "file:///W:/API%20Keys%20Manager/desktop/node_modules/vite/dist/node/index.js";
import ts from "file:///W:/API%20Keys%20Manager/desktop/node_modules/typescript/lib/typescript.js";
import { readFile, writeFile, copyFile, mkdir } from "node:fs/promises";

const base = "W:/API Keys Manager/docs/work/phase2-1a";
const cycle = process.argv.includes("--corrections4") ? `${base}/corrections4` : process.argv.includes("--corrections3") ? `${base}/corrections3` : base;
const capture = process.argv.find((arg) => arg.startsWith("--capture="))?.slice("--capture=".length);
if (capture && !/^[a-zA-Z0-9-]+$/.test(capture)) throw new Error("Capture name must be a simple directory name");
const work = capture ? `${cycle}/${capture}` : cycle;
if (capture) await mkdir(work); // Refuse to overwrite any earlier capture, including failed ones.
const scratch = "C:/Users/Gargeya/AppData/Local/Temp/opencode";
const mock = ts.transpileModule(await readFile("W:/API Keys Manager/desktop/e2e/native-boundary.ts", "utf8"), { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText;
const { installNativeBoundaryMock } = await import(`data:text/javascript;base64,${Buffer.from(mock).toString("base64")}`);
const server = await preview({ root: "W:/API Keys Manager/desktop", configFile: false, preview: { host: "127.0.0.1", port: 1439, strictPort: true, cors: true } });
const browser = await chromium.launch({ channel: "msedge", args: ["--allow-file-access-from-files"] });
const layoutFindings = [];

async function measure(file, name, theme) {
  const page = await browser.newPage({ viewport: { width: 1500, height: 1800 } });
  try {
    const query = new URLSearchParams({ page: `file:///${file}`, theme });
    await page.goto(`file:///C:/Users/Gargeya/.claude/design/checks/measure-layout.html?${query}`);
    await page.waitForFunction(() => document.querySelector("#out")?.textContent?.startsWith("RESULT:"));
    const text = await page.locator("#out").innerText();
    await writeFile(`${work}/${name}-layout-${theme}.txt`, text);
    console.log(name, theme, text.split("\n")[0]);
    if (!text.startsWith("RESULT: no FAIL line")) layoutFindings.push({ name, theme, verdict: text.split("\n")[0] });
  } finally { await page.close(); }
}

try {
  await copyFile(`${base}/graph.html`, `${scratch}/akm-unit1a-graph.html`);
  if (!(await readFile(`${base}/graph.html`)).equals(await readFile(`${scratch}/akm-unit1a-graph.html`))) throw new Error("Graph scratch copy differs");
  for (const theme of ["light", "dark"]) {
    await measure(`${scratch}/akm-unit1a-graph.html`, "graph", theme);
    if (process.argv.includes("--graph-only")) continue;
    const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
    await installNativeBoundaryMock(page, theme);
    await page.goto("http://127.0.0.1:1439/");
    await page.getByRole("button", { name: "Activity", exact: true }).click();
    await page.getByText("Recorded chain is consistent", { exact: true }).waitFor();
    await page.evaluate(() => document.fonts.ready);
    await page.screenshot({ path: `${work}/activity-${theme}-1440.png`, fullPage: true });
    let html = (await page.content()).replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, "").replaceAll('href="/assets/', 'href="http://127.0.0.1:1439/assets/');
    const fixture = `${scratch}/akm-unit1a-activity-${theme}.html`;
    await writeFile(fixture, html);
    await page.setViewportSize({ width: 390, height: 1000 });
    await page.screenshot({ path: `${work}/activity-${theme}-390.png`, fullPage: true });
    await page.close();
    await measure(fixture, "activity", theme);
    const notices = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
    await installNativeBoundaryMock(notices, theme, ["journal", "backupRefresh", "backupRetention", "backupPrune", "exportJournal"].map((kind) => ({ id: kind, originSession: "fake-a", kind })), [{ id: "fake-monitor-warning", message: "Fake app-global Windows monitor warning." }]);
    await notices.goto("http://127.0.0.1:1439/");
    await notices.getByRole("region", { name: "Saved-operation notices" }).waitFor();
    await notices.evaluate(() => document.fonts.ready);
    await notices.screenshot({ path: `${work}/notices-${theme}-1440.png`, fullPage: true });
    const noticeFixture = `${scratch}/akm-unit1a-notices-${theme}.html`;
    await writeFile(noticeFixture, (await notices.content()).replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, "").replaceAll('href="/assets/', 'href="http://127.0.0.1:1439/assets/'));
    await notices.setViewportSize({ width: 390, height: 1000 });
    await notices.screenshot({ path: `${work}/notices-${theme}-390.png`, fullPage: true });
    await notices.close();
    await measure(noticeFixture, "notices", theme);
    if (process.argv.includes("--corrections3") || process.argv.includes("--corrections4")) {
      const outcomes = await browser.newPage({ viewport: { width: 1440, height: 1100 } });
      await installNativeBoundaryMock(outcomes, theme);
      await outcomes.goto("http://127.0.0.1:1439/");
      await outcomes.getByRole("button", { name: "Settings", exact: true }).click();
      await outcomes.getByRole("button", { name: "Import .env…" }).click();
      await outcomes.getByLabel("Paste the .env contents").fill("TOKEN=fake-layout-token");
      await outcomes.getByLabel("Target project").fill("demo");
      await outcomes.getByLabel("Target environment").fill("dev");
      await outcomes.getByRole("button", { name: "Review import" }).click();
      await outcomes.getByRole("button", { name: "Confirm import" }).click();
      await outcomes.getByRole("region", { name: "Operation outcomes" }).waitFor();
      await outcomes.getByRole("button", { name: "Keys", exact: true }).click();
      await outcomes.evaluate(() => document.fonts.ready);
      await outcomes.screenshot({ path: `${work}/outcomes-${theme}-1440.png`, fullPage: true });
      const outcomeFixture = `${scratch}/akm-unit1a-outcomes-${theme}.html`;
      await writeFile(outcomeFixture, (await outcomes.content()).replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, "").replaceAll('href="/assets/', 'href="http://127.0.0.1:1439/assets/'));
      await outcomes.setViewportSize({ width: 390, height: 1000 });
      await outcomes.screenshot({ path: `${work}/outcomes-${theme}-390.png`, fullPage: true });
      await outcomes.close();
      await measure(outcomeFixture, "outcomes", theme);
    }
  }
  const graph = await browser.newPage({ viewport: { width: 1440, height: 1100 }, colorScheme: "light" });
  await graph.goto(`file:///${scratch}/akm-unit1a-graph.html`);
  await graph.evaluate(() => localStorage.setItem("theme", "light"));
  await graph.reload();
  await graph.locator("#theme-toggle").click();
  if (await graph.locator("html").getAttribute("data-theme") !== "dark") throw new Error("Graph theme switch did not select dark");
  await graph.reload();
  if (await graph.locator("html").getAttribute("data-theme") !== "dark") throw new Error("Graph theme preference was lost");
  await graph.locator("#theme-toggle").click();
  if (await graph.locator("html").getAttribute("data-theme") !== "light") throw new Error("Graph theme switch did not return to light");
  await graph.screenshot({ path: `${work}/graph-1440.png`, fullPage: true });
  await graph.close();
  console.log(process.argv.includes("--graph-only") ? "Graph theme switch/reload checks passed." : "Theme switch/reload checks passed. Screenshots are mocked-boundary renders; width measurements use inert copies of their rendered markup and CSS.");
  await writeFile(`${work}/${process.argv.includes("--graph-only") ? "graph-layout-findings" : "layout-findings"}.json`, JSON.stringify({ findings: layoutFindings, note: "All findings remain findings; do not interpret successful screenshots as a checker pass." }, null, 2));
  if (layoutFindings.length) process.exitCode = 1;
} finally {
  await browser.close();
  await new Promise((resolve) => server.httpServer.close(resolve));
}
