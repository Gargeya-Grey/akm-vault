import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  workers: 1,
  timeout: 30_000,
  reporter: "list",
  use: { channel: "msedge", baseURL: "http://127.0.0.1:1438" },
  webServer: {
    command: "npm run preview -- --host 127.0.0.1 --port 1438 --strictPort",
    url: "http://127.0.0.1:1438",
    reuseExistingServer: false,
  },
});
